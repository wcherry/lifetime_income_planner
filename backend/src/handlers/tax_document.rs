//! Tax form imports (roadmap Phase 6, feature 3).

use std::collections::HashMap;

use actix_web::{delete, get, post, web, HttpResponse};
use diesel::prelude::*;
use serde::Deserialize;
use uuid::Uuid;
use validator::Validate;

use crate::auth::AuthUser;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::handlers::auth::format_validation;
use crate::models::{
    parse_box_data_csv, ImportTaxDocumentRequest, NewTaxDocument, TaxDocument,
    TaxDocumentResponse, TaxDocumentYearSummary,
};
use crate::schema::tax_documents;

/// Import a tax document's box amounts from CSV (roadmap Phase 6, feature 3).
#[utoipa::path(
    post,
    path = "/api/tax-documents/import",
    tag = "tax_documents",
    request_body = ImportTaxDocumentRequest,
    responses(
        (status = 201, description = "Document imported", body = TaxDocumentResponse),
        (status = 400, description = "Validation error or unparseable CSV"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[post("/tax-documents/import")]
pub async fn import_tax_document(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    body: web::Json<ImportTaxDocumentRequest>,
) -> AppResult<HttpResponse> {
    let payload = body.into_inner();
    payload
        .validate()
        .map_err(|e| AppError::BadRequest(format_validation(&e)))?;

    let box_data = parse_box_data_csv(&payload.csv_content)?;
    let box_data_json = serde_json::to_string(&box_data)
        .map_err(|e| AppError::Internal(format!("failed to serialize box data: {e}")))?;

    let id = Uuid::new_v4().to_string();
    let new_doc = NewTaxDocument {
        id: id.clone(),
        user_id: auth.user_id.clone(),
        tax_year: payload.tax_year,
        form_type: payload.form_type.as_str().to_string(),
        box_data: box_data_json,
        source_filename: payload
            .source_filename
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
    };

    let pool = pool.clone();
    let doc = web::block(move || -> AppResult<TaxDocument> {
        let mut conn = pool.get()?;
        diesel::insert_into(tax_documents::table)
            .values(&new_doc)
            .execute(&mut conn)?;
        let doc = tax_documents::table
            .filter(tax_documents::id.eq(&id))
            .select(TaxDocument::as_select())
            .first(&mut conn)?;
        Ok(doc)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    Ok(HttpResponse::Created().json(TaxDocumentResponse::from_row(&doc)?))
}

#[derive(Debug, Deserialize)]
pub struct TaxDocumentYearQuery {
    pub year: Option<i32>,
}

/// List imported tax documents, optionally filtered to one tax year.
#[utoipa::path(
    get,
    path = "/api/tax-documents",
    tag = "tax_documents",
    params(("year" = Option<i32>, Query, description = "Filter to a single tax year")),
    responses(
        (status = 200, description = "Imported tax documents", body = [TaxDocumentResponse]),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[get("/tax-documents")]
pub async fn list_tax_documents(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    query: web::Query<TaxDocumentYearQuery>,
) -> AppResult<HttpResponse> {
    let year = query.year;
    let user_id = auth.user_id.clone();
    let pool = pool.clone();
    let rows = web::block(move || -> AppResult<Vec<TaxDocument>> {
        let mut conn = pool.get()?;
        let mut q = tax_documents::table
            .filter(tax_documents::user_id.eq(&user_id))
            .into_boxed();
        if let Some(y) = year {
            q = q.filter(tax_documents::tax_year.eq(y));
        }
        let rows = q
            .order(tax_documents::imported_at.desc())
            .select(TaxDocument::as_select())
            .load(&mut conn)?;
        Ok(rows)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    let body: Vec<TaxDocumentResponse> = rows
        .iter()
        .map(TaxDocumentResponse::from_row)
        .collect::<AppResult<Vec<_>>>()?;
    Ok(HttpResponse::Ok().json(body))
}

/// Aggregate totals across every imported document for a tax year.
#[utoipa::path(
    get,
    path = "/api/tax-documents/summary/{year}",
    tag = "tax_documents",
    params(("year" = i32, Path, description = "Tax year")),
    responses(
        (status = 200, description = "Aggregated totals for the year", body = TaxDocumentYearSummary),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[get("/tax-documents/summary/{year}")]
pub async fn tax_document_year_summary(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    path: web::Path<i32>,
) -> AppResult<HttpResponse> {
    let year = path.into_inner();
    let user_id = auth.user_id.clone();
    let pool = pool.clone();
    let rows = web::block(move || -> AppResult<Vec<TaxDocument>> {
        let mut conn = pool.get()?;
        let rows = tax_documents::table
            .filter(tax_documents::user_id.eq(&user_id))
            .filter(tax_documents::tax_year.eq(year))
            .select(TaxDocument::as_select())
            .load(&mut conn)?;
        Ok(rows)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    let mut totals_by_field: HashMap<String, f64> = HashMap::new();
    let mut grand_total = 0.0;
    for row in &rows {
        let box_data: HashMap<String, f64> = serde_json::from_str(&row.box_data)
            .map_err(|e| AppError::Internal(format!("corrupt tax document box data: {e}")))?;
        for (field, amount) in box_data {
            *totals_by_field.entry(field).or_insert(0.0) += amount;
            grand_total += amount;
        }
    }

    Ok(HttpResponse::Ok().json(TaxDocumentYearSummary {
        tax_year: year,
        document_count: rows.len(),
        totals_by_field,
        grand_total,
    }))
}

/// Delete an imported tax document.
#[utoipa::path(
    delete,
    path = "/api/tax-documents/{id}",
    tag = "tax_documents",
    params(("id" = String, Path, description = "Tax document id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 404, description = "Not found"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[delete("/tax-documents/{id}")]
pub async fn delete_tax_document(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let user_id = auth.user_id.clone();
    let pool = pool.clone();
    let deleted = web::block(move || -> AppResult<usize> {
        let mut conn = pool.get()?;
        let n = diesel::delete(
            tax_documents::table
                .filter(tax_documents::id.eq(&id))
                .filter(tax_documents::user_id.eq(&user_id)),
        )
        .execute(&mut conn)?;
        Ok(n)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    if deleted == 0 {
        return Err(AppError::NotFound("Tax document not found".into()));
    }
    Ok(HttpResponse::NoContent().finish())
}
