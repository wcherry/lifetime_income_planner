//! Spending Tracker (transaction-level CSV import + categorization):
//! distinct from the planned-budget "Spending" page
//! (`handlers/spending.rs`/`spending_items` table) — this module never
//! touches that table. Every handler scopes its queries by `auth.user_id`.
//!
//! Re-import behavior: importing a CSV appends rows, deduped by a
//! content-based `dedupe_key` (see `compute_dedupe_key`), so re-uploading
//! the exact same statement is a safe no-op and a second, different
//! statement for the same month simply appends alongside the first. If a
//! bank re-exports a statement with slightly different wording, rows will
//! not dedupe — a known limitation of content-hash dedupe without a stable
//! external transaction id.

use std::collections::{HashMap, HashSet};

use actix_web::{delete, get, patch, post, put, web, HttpResponse};
use chrono::Utc;
use diesel::prelude::*;
use serde::Deserialize;
use uuid::Uuid;
use validator::Validate;

use crate::auth::AuthUser;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::handlers::auth::format_validation;
use crate::models::{
    best_guess_category, category_editable_by_user, category_visible_to_user, compute_dedupe_key,
    expense_category_breakdown, manual_entry_raw_row_json, months_of_quarter, normalize_label,
    parse_spending_transactions_csv, summarize_quarter, summarize_year, BulkCategorizeRequest,
    CategoryMappingSuggestion, CategoryMonthRow, CreateManualTransactionRequest,
    ImportSpendingTransactionsRequest, MonthKindTotal, NewCategoryRequest,
    NewSpendingTrackerCategory, NewSpendingTrackerCategoryMapping, NewSpendingTrackerImport,
    NewSpendingTrackerTransaction, SetTransactionCategoryRequest, SkippedRowResponse,
    SpendingTrackerCategory, SpendingTrackerCategoryKind, SpendingTrackerCategoryResponse,
    SpendingTrackerImport, SpendingTrackerImportResult, SpendingTrackerMonthSummary,
    SpendingTrackerQuarterSummaryResponse, SpendingTrackerTransaction,
    SpendingTrackerTransactionResponse, SpendingTrackerYearSummaryResponse, UpdateCategoryRequest,
};
use crate::schema::{
    spending_tracker_categories, spending_tracker_category_mappings, spending_tracker_imports,
    spending_tracker_transactions,
};

/// List predefined (visible to everyone) plus the caller's own custom
/// categories.
#[utoipa::path(
    get,
    path = "/api/spending-tracker/categories",
    tag = "spending_tracker",
    responses(
        (status = 200, description = "Predefined and caller-owned categories", body = [SpendingTrackerCategoryResponse]),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[get("/spending-tracker/categories")]
pub async fn list_categories(pool: web::Data<DbPool>, auth: AuthUser) -> AppResult<HttpResponse> {
    let user_id = auth.user_id.clone();
    let pool = pool.clone();
    let rows = web::block(move || -> AppResult<Vec<SpendingTrackerCategory>> {
        let mut conn = pool.get()?;
        let rows = spending_tracker_categories::table
            .filter(
                spending_tracker_categories::user_id
                    .is_null()
                    .or(spending_tracker_categories::user_id.eq(&user_id)),
            )
            .order(spending_tracker_categories::name.asc())
            .select(SpendingTrackerCategory::as_select())
            .load(&mut conn)?;
        Ok(rows)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    let body: Vec<SpendingTrackerCategoryResponse> = rows
        .iter()
        .map(|r| SpendingTrackerCategoryResponse::from_row(r, &auth.user_id))
        .collect();
    Ok(HttpResponse::Ok().json(body))
}

/// Create a custom category, owned by the caller.
#[utoipa::path(
    post,
    path = "/api/spending-tracker/categories",
    tag = "spending_tracker",
    request_body = NewCategoryRequest,
    responses(
        (status = 201, description = "Category created", body = SpendingTrackerCategoryResponse),
        (status = 400, description = "Validation error or duplicate category name"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[post("/spending-tracker/categories")]
pub async fn create_category(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    body: web::Json<NewCategoryRequest>,
) -> AppResult<HttpResponse> {
    let payload = body.into_inner();
    payload
        .validate()
        .map_err(|e| AppError::BadRequest(format_validation(&e)))?;

    let user_id = auth.user_id.clone();
    let pool = pool.clone();
    let row = web::block(move || -> AppResult<SpendingTrackerCategory> {
        let mut conn = pool.get()?;
        conn.transaction::<SpendingTrackerCategory, AppError, _>(|conn| {
            let existing_names: Vec<String> = spending_tracker_categories::table
                .filter(
                    spending_tracker_categories::user_id
                        .is_null()
                        .or(spending_tracker_categories::user_id.eq(&user_id)),
                )
                .select(spending_tracker_categories::name)
                .load(conn)?;
            crate::models::validate_unique_category_name(&payload.name, &existing_names)?;

            let id = Uuid::new_v4().to_string();
            let now = Utc::now().naive_utc();
            let new_category = NewSpendingTrackerCategory {
                id: id.clone(),
                user_id: Some(user_id.clone()),
                name: payload.name.trim().to_string(),
                kind: payload.kind.as_str().to_string(),
                is_predefined: false,
                created_at: now,
                updated_at: now,
            };
            diesel::insert_into(spending_tracker_categories::table)
                .values(&new_category)
                .execute(conn)?;

            let row = spending_tracker_categories::table
                .filter(spending_tracker_categories::id.eq(&id))
                .select(SpendingTrackerCategory::as_select())
                .first(conn)?;
            Ok(row)
        })
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    Ok(
        HttpResponse::Created().json(SpendingTrackerCategoryResponse::from_row(
            &row,
            &auth.user_id,
        )),
    )
}

/// Rename/re-kind a custom category. Predefined categories, and categories
/// owned by another user, cannot be edited.
#[utoipa::path(
    put,
    path = "/api/spending-tracker/categories/{id}",
    tag = "spending_tracker",
    params(("id" = String, Path, description = "Category id")),
    request_body = UpdateCategoryRequest,
    responses(
        (status = 200, description = "Category updated", body = SpendingTrackerCategoryResponse),
        (status = 400, description = "Validation error or duplicate category name"),
        (status = 403, description = "Category is predefined or not owned by the caller"),
        (status = 404, description = "Category not found"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[put("/spending-tracker/categories/{id}")]
pub async fn update_category(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    path: web::Path<String>,
    body: web::Json<UpdateCategoryRequest>,
) -> AppResult<HttpResponse> {
    let payload = body.into_inner();
    payload
        .validate()
        .map_err(|e| AppError::BadRequest(format_validation(&e)))?;

    let id = path.into_inner();
    let user_id = auth.user_id.clone();
    let pool = pool.clone();
    let row = web::block(move || -> AppResult<SpendingTrackerCategory> {
        let mut conn = pool.get()?;
        conn.transaction::<SpendingTrackerCategory, AppError, _>(|conn| {
            let existing = spending_tracker_categories::table
                .filter(spending_tracker_categories::id.eq(&id))
                .select(SpendingTrackerCategory::as_select())
                .first::<SpendingTrackerCategory>(conn)
                .optional()?
                .ok_or_else(|| AppError::NotFound("Category not found".into()))?;

            if !category_editable_by_user(&existing, &user_id) {
                return Err(AppError::Forbidden("This category cannot be edited".into()));
            }

            let existing_names: Vec<String> = spending_tracker_categories::table
                .filter(
                    spending_tracker_categories::user_id
                        .is_null()
                        .or(spending_tracker_categories::user_id.eq(&user_id)),
                )
                .filter(spending_tracker_categories::id.ne(&id))
                .select(spending_tracker_categories::name)
                .load(conn)?;
            crate::models::validate_unique_category_name(&payload.name, &existing_names)?;

            diesel::update(
                spending_tracker_categories::table.filter(spending_tracker_categories::id.eq(&id)),
            )
            .set((
                spending_tracker_categories::name.eq(payload.name.trim()),
                spending_tracker_categories::kind.eq(payload.kind.as_str()),
                spending_tracker_categories::updated_at.eq(Utc::now().naive_utc()),
            ))
            .execute(conn)?;

            let row = spending_tracker_categories::table
                .filter(spending_tracker_categories::id.eq(&id))
                .select(SpendingTrackerCategory::as_select())
                .first(conn)?;
            Ok(row)
        })
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    Ok(
        HttpResponse::Ok().json(SpendingTrackerCategoryResponse::from_row(
            &row,
            &auth.user_id,
        )),
    )
}

/// Delete a custom category. Transactions referencing it fall back to
/// uncategorized via `ON DELETE SET NULL`.
#[utoipa::path(
    delete,
    path = "/api/spending-tracker/categories/{id}",
    tag = "spending_tracker",
    params(("id" = String, Path, description = "Category id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 403, description = "Category is predefined or not owned by the caller"),
        (status = 404, description = "Category not found"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[delete("/spending-tracker/categories/{id}")]
pub async fn delete_category(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let user_id = auth.user_id.clone();
    let pool = pool.clone();
    web::block(move || -> AppResult<()> {
        let mut conn = pool.get()?;
        let existing = spending_tracker_categories::table
            .filter(spending_tracker_categories::id.eq(&id))
            .select(SpendingTrackerCategory::as_select())
            .first::<SpendingTrackerCategory>(&mut conn)
            .optional()?
            .ok_or_else(|| AppError::NotFound("Category not found".into()))?;

        if !category_editable_by_user(&existing, &user_id) {
            return Err(AppError::Forbidden(
                "This category cannot be deleted".into(),
            ));
        }

        diesel::delete(
            spending_tracker_categories::table.filter(spending_tracker_categories::id.eq(&id)),
        )
        .execute(&mut conn)?;
        Ok(())
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    Ok(HttpResponse::NoContent().finish())
}

/// Import a month's worth of transactions from CSV (tolerant header
/// detection; unparseable rows are skipped and reported back, not a hard
/// failure of the whole import). Exact re-imports are a safe no-op via
/// content-based dedupe.
#[utoipa::path(
    post,
    path = "/api/spending-tracker/import",
    tag = "spending_tracker",
    request_body = ImportSpendingTransactionsRequest,
    responses(
        (status = 201, description = "Import completed", body = SpendingTrackerImportResult),
        (status = 400, description = "Validation error or unparseable CSV"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[post("/spending-tracker/import")]
pub async fn import_transactions(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    body: web::Json<ImportSpendingTransactionsRequest>,
) -> AppResult<HttpResponse> {
    let payload = body.into_inner();
    payload
        .validate()
        .map_err(|e| AppError::BadRequest(format_validation(&e)))?;

    let parsed = parse_spending_transactions_csv(&payload.csv_content)?;

    let user_id = auth.user_id.clone();
    let pool = pool.clone();
    let result = web::block(move || -> AppResult<SpendingTrackerImportResult> {
        let mut conn = pool.get()?;
        conn.transaction::<SpendingTrackerImportResult, AppError, _>(|conn| {
            let import_id = Uuid::new_v4().to_string();
            let now = Utc::now().naive_utc();

            // Insert the import row first (with placeholder counts) since
            // transactions carry a NOT NULL FK to it; final counts are
            // written back with an UPDATE once they're known below.
            let new_import = NewSpendingTrackerImport {
                id: import_id.clone(),
                user_id: user_id.clone(),
                year: payload.year,
                month: payload.month,
                source_filename: payload
                    .source_filename
                    .as_ref()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
                row_count: 0,
                duplicate_count: 0,
                skipped_count: parsed.skipped.len() as i32,
            };
            diesel::insert_into(spending_tracker_imports::table)
                .values(&new_import)
                .execute(conn)?;

            // Category mapping (not auto-creation): many bank/card exports
            // include their own category column (see
            // `parse_spending_transactions_csv`). A row's label is resolved,
            // in order: (1) a mapping the caller has previously taught us
            // (see the module doc on learned mappings), (2) an exact
            // (case-insensitive, trimmed) match against a category already
            // visible to the caller. Anything else is left uncategorized
            // here and grouped into `category_mappings` below, with a
            // best-guess suggestion the caller can accept or correct — new
            // categories are never created on the caller's behalf.
            let existing_categories: Vec<SpendingTrackerCategory> =
                spending_tracker_categories::table
                    .filter(
                        spending_tracker_categories::user_id
                            .is_null()
                            .or(spending_tracker_categories::user_id.eq(&user_id)),
                    )
                    .select(SpendingTrackerCategory::as_select())
                    .load(conn)?;
            let category_lookup: HashMap<String, String> = existing_categories
                .iter()
                .map(|c| (normalize_label(&c.name), c.id.clone()))
                .collect();
            let learned_mappings: HashMap<String, String> = spending_tracker_category_mappings::table
                .filter(spending_tracker_category_mappings::user_id.eq(&user_id))
                .select((
                    spending_tracker_category_mappings::normalized_label,
                    spending_tracker_category_mappings::category_id,
                ))
                .load::<(String, String)>(conn)?
                .into_iter()
                .collect();

            let mut imported_count = 0usize;
            let mut duplicate_count = 0usize;
            // Newly imported transactions whose label didn't resolve via a
            // learned mapping or an exact match, grouped by normalized
            // label: (original-cased label, txn ids).
            let mut unmatched: HashMap<String, (String, Vec<String>)> = HashMap::new();

            for row in &parsed.rows {
                let category_id = row.category.as_ref().and_then(|label| {
                    let normalized = normalize_label(label);
                    learned_mappings
                        .get(&normalized)
                        .or_else(|| category_lookup.get(&normalized))
                        .cloned()
                });

                let txn_id = Uuid::new_v4().to_string();
                let dedupe_key = compute_dedupe_key(
                    payload.year,
                    payload.month,
                    row.transaction_date,
                    &row.description,
                    row.amount,
                );
                let new_txn = NewSpendingTrackerTransaction {
                    id: txn_id.clone(),
                    user_id: user_id.clone(),
                    import_id: import_id.clone(),
                    year: payload.year,
                    month: payload.month,
                    transaction_date: row.transaction_date,
                    description: row.description.clone(),
                    amount: row.amount,
                    category_id: category_id.clone(),
                    dedupe_key,
                    created_at: now,
                    updated_at: now,
                    raw_row_json: row.raw_row_json.clone(),
                    source_category_label: row.category.clone(),
                };
                // Insert one row at a time (rather than a single batch
                // insert) so `execute`'s return value accurately reports
                // whether *this* row was newly inserted or skipped as a
                // duplicate — a batch insert's row count doesn't reliably
                // break down per-row conflict outcomes in SQLite.
                let inserted = diesel::insert_into(spending_tracker_transactions::table)
                    .values(&new_txn)
                    .on_conflict((
                        spending_tracker_transactions::user_id,
                        spending_tracker_transactions::dedupe_key,
                    ))
                    .do_nothing()
                    .execute(conn)?;
                if inserted > 0 {
                    imported_count += 1;
                    if category_id.is_none() {
                        if let Some(label) = &row.category {
                            let key = normalize_label(label);
                            unmatched
                                .entry(key)
                                .or_insert_with(|| (label.trim().to_string(), Vec::new()))
                                .1
                                .push(txn_id);
                        }
                    }
                } else {
                    duplicate_count += 1;
                }
            }

            diesel::update(
                spending_tracker_imports::table.filter(spending_tracker_imports::id.eq(&import_id)),
            )
            .set((
                spending_tracker_imports::row_count.eq(imported_count as i32),
                spending_tracker_imports::duplicate_count.eq(duplicate_count as i32),
            ))
            .execute(conn)?;

            let mut category_mappings: Vec<CategoryMappingSuggestion> = unmatched
                .into_values()
                .map(|(label, transaction_ids)| {
                    let suggestion = best_guess_category(&label, &existing_categories);
                    CategoryMappingSuggestion {
                        label,
                        suggested_category_id: suggestion.map(|c| c.id.clone()),
                        suggested_category_name: suggestion.map(|c| c.name.clone()),
                        transaction_ids,
                    }
                })
                .collect();
            category_mappings.sort_by(|a, b| a.label.cmp(&b.label));

            Ok(SpendingTrackerImportResult {
                import_id,
                imported_count,
                duplicate_count,
                skipped_rows: parsed
                    .skipped
                    .iter()
                    .map(SkippedRowResponse::from)
                    .collect(),
                category_mappings,
            })
        })
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    Ok(HttpResponse::Created().json(result))
}

/// Record a single transaction by hand (e.g. cash spending, or something a
/// bank export missed) rather than via CSV import. Reuses the same
/// dedupe/import-audit machinery as a CSV import: a one-row "import" record
/// is created for it (`source_filename = None` marks it as manual), and
/// it's deduped the same way, so a manual entry that exactly matches an
/// already-imported transaction is rejected rather than silently
/// duplicated.
#[utoipa::path(
    post,
    path = "/api/spending-tracker/transactions",
    tag = "spending_tracker",
    request_body = CreateManualTransactionRequest,
    responses(
        (status = 201, description = "Transaction created", body = SpendingTrackerTransactionResponse),
        (status = 400, description = "Validation error, unknown category, or a duplicate of an existing transaction"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[post("/spending-tracker/transactions")]
pub async fn create_manual_transaction(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    body: web::Json<CreateManualTransactionRequest>,
) -> AppResult<HttpResponse> {
    let payload = body.into_inner();
    payload
        .validate()
        .map_err(|e| AppError::BadRequest(format_validation(&e)))?;

    let user_id = auth.user_id.clone();
    let pool = pool.clone();
    let (txn, category) = web::block(
        move || -> AppResult<(SpendingTrackerTransaction, Option<SpendingTrackerCategory>)> {
            let mut conn = pool.get()?;
            conn.transaction::<(SpendingTrackerTransaction, Option<SpendingTrackerCategory>), AppError, _>(
                |conn| {
                    let category = match &payload.category_id {
                        Some(cid) => Some(load_visible_category(conn, cid, &user_id)?),
                        None => None,
                    };

                    let import_id = Uuid::new_v4().to_string();
                    let now = Utc::now().naive_utc();
                    let new_import = NewSpendingTrackerImport {
                        id: import_id.clone(),
                        user_id: user_id.clone(),
                        year: payload.year,
                        month: payload.month,
                        source_filename: None,
                        row_count: 1,
                        duplicate_count: 0,
                        skipped_count: 0,
                    };
                    diesel::insert_into(spending_tracker_imports::table)
                        .values(&new_import)
                        .execute(conn)?;

                    let description = payload.description.trim().to_string();
                    let dedupe_key = compute_dedupe_key(
                        payload.year,
                        payload.month,
                        payload.transaction_date,
                        &description,
                        payload.amount,
                    );
                    let txn_id = Uuid::new_v4().to_string();
                    let new_txn = NewSpendingTrackerTransaction {
                        id: txn_id.clone(),
                        user_id: user_id.clone(),
                        import_id,
                        year: payload.year,
                        month: payload.month,
                        transaction_date: payload.transaction_date,
                        description: description.clone(),
                        amount: payload.amount,
                        category_id: payload.category_id.clone(),
                        dedupe_key,
                        created_at: now,
                        updated_at: now,
                        raw_row_json: manual_entry_raw_row_json(
                            payload.transaction_date,
                            &description,
                            payload.amount,
                        ),
                        source_category_label: None,
                    };
                    let inserted = diesel::insert_into(spending_tracker_transactions::table)
                        .values(&new_txn)
                        .on_conflict((
                            spending_tracker_transactions::user_id,
                            spending_tracker_transactions::dedupe_key,
                        ))
                        .do_nothing()
                        .execute(conn)?;
                    if inserted == 0 {
                        return Err(AppError::BadRequest(
                            "A transaction with this date, description, and amount already exists"
                                .into(),
                        ));
                    }

                    let txn = spending_tracker_transactions::table
                        .filter(spending_tracker_transactions::id.eq(&txn_id))
                        .select(SpendingTrackerTransaction::as_select())
                        .first(conn)?;
                    Ok((txn, category))
                },
            )
        },
    )
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    Ok(
        HttpResponse::Created().json(SpendingTrackerTransactionResponse::from_row(
            &txn,
            category.as_ref(),
        )),
    )
}

/// List the (year, month) buckets the caller has imported data for, with a
/// transaction count and the most recent import time — drives the month
/// picker and quarter-coverage display.
#[utoipa::path(
    get,
    path = "/api/spending-tracker/months",
    tag = "spending_tracker",
    responses(
        (status = 200, description = "Months with imported data", body = [SpendingTrackerMonthSummary]),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[get("/spending-tracker/months")]
pub async fn list_months(pool: web::Data<DbPool>, auth: AuthUser) -> AppResult<HttpResponse> {
    let user_id = auth.user_id.clone();
    let pool = pool.clone();
    let (transactions, imports) = web::block(
        move || -> AppResult<(Vec<SpendingTrackerTransaction>, Vec<SpendingTrackerImport>)> {
            let mut conn = pool.get()?;
            let transactions = spending_tracker_transactions::table
                .filter(spending_tracker_transactions::user_id.eq(&user_id))
                .select(SpendingTrackerTransaction::as_select())
                .load(&mut conn)?;
            let imports = spending_tracker_imports::table
                .filter(spending_tracker_imports::user_id.eq(&user_id))
                .select(SpendingTrackerImport::as_select())
                .load(&mut conn)?;
            Ok((transactions, imports))
        },
    )
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    let mut counts: HashMap<(i32, i32), i64> = HashMap::new();
    for t in &transactions {
        *counts.entry((t.year, t.month)).or_insert(0) += 1;
    }

    let mut last_imported: HashMap<(i32, i32), chrono::NaiveDateTime> = HashMap::new();
    for i in &imports {
        let entry = last_imported
            .entry((i.year, i.month))
            .or_insert(i.imported_at);
        if i.imported_at > *entry {
            *entry = i.imported_at;
        }
    }

    let mut summaries: Vec<SpendingTrackerMonthSummary> = counts
        .into_iter()
        .filter_map(|((year, month), transaction_count)| {
            last_imported
                .get(&(year, month))
                .map(|last_imported_at| SpendingTrackerMonthSummary {
                    year,
                    month,
                    transaction_count,
                    last_imported_at: *last_imported_at,
                })
        })
        .collect();
    summaries.sort_by_key(|s| std::cmp::Reverse((s.year, s.month)));

    Ok(HttpResponse::Ok().json(summaries))
}

#[derive(Debug, Deserialize)]
pub struct MonthQuery {
    pub year: i32,
    pub month: i32,
}

/// List the caller's transactions for one month, with the owning category's
/// name/kind denormalized in, ordered by transaction date.
#[utoipa::path(
    get,
    path = "/api/spending-tracker/transactions",
    tag = "spending_tracker",
    params(
        ("year" = i32, Query, description = "Calendar year"),
        ("month" = i32, Query, description = "Month, 1-12"),
    ),
    responses(
        (status = 200, description = "Transactions for the month", body = [SpendingTrackerTransactionResponse]),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[get("/spending-tracker/transactions")]
pub async fn list_transactions(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    query: web::Query<MonthQuery>,
) -> AppResult<HttpResponse> {
    let year = query.year;
    let month = query.month;
    let user_id = auth.user_id.clone();
    let pool = pool.clone();
    let rows = web::block(
        move || -> AppResult<Vec<(SpendingTrackerTransaction, Option<SpendingTrackerCategory>)>> {
            let mut conn = pool.get()?;
            let rows = spending_tracker_transactions::table
                .left_join(spending_tracker_categories::table)
                .filter(spending_tracker_transactions::user_id.eq(&user_id))
                .filter(spending_tracker_transactions::year.eq(year))
                .filter(spending_tracker_transactions::month.eq(month))
                .order(spending_tracker_transactions::transaction_date.asc())
                .select((
                    SpendingTrackerTransaction::as_select(),
                    Option::<SpendingTrackerCategory>::as_select(),
                ))
                .load(&mut conn)?;
            Ok(rows)
        },
    )
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    let body: Vec<SpendingTrackerTransactionResponse> = rows
        .iter()
        .map(|(t, c)| SpendingTrackerTransactionResponse::from_row(t, c.as_ref()))
        .collect();
    Ok(HttpResponse::Ok().json(body))
}

/// Look up a category by id and confirm it's visible to `user_id`
/// (predefined or owned). Returns the row so callers can reuse it.
fn load_visible_category(
    conn: &mut SqliteConnection,
    category_id: &str,
    user_id: &str,
) -> AppResult<SpendingTrackerCategory> {
    let category = spending_tracker_categories::table
        .filter(spending_tracker_categories::id.eq(category_id))
        .select(SpendingTrackerCategory::as_select())
        .first::<SpendingTrackerCategory>(conn)
        .optional()?;
    if !category_visible_to_user(category.as_ref(), user_id) {
        return Err(AppError::BadRequest(format!(
            "Unknown or inaccessible category id: {category_id}"
        )));
    }
    Ok(category.expect("category_visible_to_user confirmed Some"))
}

/// Keeps the caller's learned CSV-label -> category mapping in sync with an
/// explicit categorization choice for a transaction that carried a CSV
/// category label (see the module doc). Called from both the single-
/// transaction PATCH and bulk-categorize handlers, since either can be the
/// moment a user corrects or confirms a label's category.
/// `category_id = Some(id)` upserts the mapping to `id`; `category_id =
/// None` deletes any existing mapping for the label — explicitly choosing
/// "uncategorized" is itself a signal to stop auto-applying a stale one.
fn remember_category_mapping(
    conn: &mut SqliteConnection,
    user_id: &str,
    label: &str,
    category_id: Option<&str>,
) -> AppResult<()> {
    let normalized_label = normalize_label(label);
    match category_id {
        Some(cid) => {
            let now = Utc::now().naive_utc();
            let new_mapping = NewSpendingTrackerCategoryMapping {
                id: Uuid::new_v4().to_string(),
                user_id: user_id.to_string(),
                label: label.trim().to_string(),
                normalized_label,
                category_id: cid.to_string(),
                created_at: now,
                updated_at: now,
            };
            diesel::insert_into(spending_tracker_category_mappings::table)
                .values(&new_mapping)
                .on_conflict((
                    spending_tracker_category_mappings::user_id,
                    spending_tracker_category_mappings::normalized_label,
                ))
                .do_update()
                .set((
                    spending_tracker_category_mappings::label.eq(label.trim()),
                    spending_tracker_category_mappings::category_id.eq(cid),
                    spending_tracker_category_mappings::updated_at.eq(now),
                ))
                .execute(conn)?;
        }
        None => {
            diesel::delete(
                spending_tracker_category_mappings::table
                    .filter(spending_tracker_category_mappings::user_id.eq(user_id))
                    .filter(
                        spending_tracker_category_mappings::normalized_label.eq(&normalized_label),
                    ),
            )
            .execute(conn)?;
        }
    }
    Ok(())
}

/// Assign (or clear) a single transaction's category.
#[utoipa::path(
    patch,
    path = "/api/spending-tracker/transactions/{id}",
    tag = "spending_tracker",
    params(("id" = String, Path, description = "Transaction id")),
    request_body = SetTransactionCategoryRequest,
    responses(
        (status = 200, description = "Transaction updated", body = SpendingTrackerTransactionResponse),
        (status = 400, description = "Unknown or inaccessible category id"),
        (status = 404, description = "Transaction not found"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[patch("/spending-tracker/transactions/{id}")]
pub async fn set_transaction_category(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    path: web::Path<String>,
    body: web::Json<SetTransactionCategoryRequest>,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let payload = body.into_inner();
    let user_id = auth.user_id.clone();
    let pool = pool.clone();
    let (txn, category) = web::block(
        move || -> AppResult<(SpendingTrackerTransaction, Option<SpendingTrackerCategory>)> {
            let mut conn = pool.get()?;
            conn.transaction::<(SpendingTrackerTransaction, Option<SpendingTrackerCategory>), AppError, _>(
                |conn| {
                    let category = match &payload.category_id {
                        Some(cid) => Some(load_visible_category(conn, cid, &user_id)?),
                        None => None,
                    };

                    let updated = diesel::update(
                        spending_tracker_transactions::table
                            .filter(spending_tracker_transactions::id.eq(&id))
                            .filter(spending_tracker_transactions::user_id.eq(&user_id)),
                    )
                    .set((
                        spending_tracker_transactions::category_id.eq(&payload.category_id),
                        spending_tracker_transactions::updated_at.eq(Utc::now().naive_utc()),
                    ))
                    .execute(conn)?;
                    if updated == 0 {
                        return Err(AppError::NotFound("Transaction not found".into()));
                    }

                    let txn = spending_tracker_transactions::table
                        .filter(spending_tracker_transactions::id.eq(&id))
                        .select(SpendingTrackerTransaction::as_select())
                        .first::<SpendingTrackerTransaction>(conn)?;
                    if let Some(label) = &txn.source_category_label {
                        remember_category_mapping(
                            conn,
                            &user_id,
                            label,
                            payload.category_id.as_deref(),
                        )?;
                    }
                    Ok((txn, category))
                },
            )
        },
    )
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    Ok(
        HttpResponse::Ok().json(SpendingTrackerTransactionResponse::from_row(
            &txn,
            category.as_ref(),
        )),
    )
}

/// Assign (or clear) a category across many transactions at once, all
/// scoped to the caller.
#[utoipa::path(
    post,
    path = "/api/spending-tracker/transactions/bulk-categorize",
    tag = "spending_tracker",
    request_body = BulkCategorizeRequest,
    responses(
        (status = 200, description = "Number of transactions updated"),
        (status = 400, description = "Unknown or inaccessible category id"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[post("/spending-tracker/transactions/bulk-categorize")]
pub async fn bulk_categorize_transactions(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    body: web::Json<BulkCategorizeRequest>,
) -> AppResult<HttpResponse> {
    let payload = body.into_inner();
    payload
        .validate()
        .map_err(|e| AppError::BadRequest(format_validation(&e)))?;

    let user_id = auth.user_id.clone();
    let pool = pool.clone();
    let updated_count = web::block(move || -> AppResult<usize> {
        let mut conn = pool.get()?;
        conn.transaction::<usize, AppError, _>(|conn| {
            if let Some(cid) = &payload.category_id {
                load_visible_category(conn, cid, &user_id)?;
            }

            // Fetch the distinct CSV labels among the affected transactions
            // *before* updating, so a bulk-categorize call (including the
            // "map imported categories" review flow, which is really a
            // bulk-categorize per label) teaches the same lesson as a single
            // correction would.
            let labels: HashSet<String> = spending_tracker_transactions::table
                .filter(spending_tracker_transactions::user_id.eq(&user_id))
                .filter(spending_tracker_transactions::id.eq_any(&payload.transaction_ids))
                .select(spending_tracker_transactions::source_category_label)
                .load::<Option<String>>(conn)?
                .into_iter()
                .flatten()
                .collect();

            let updated = diesel::update(
                spending_tracker_transactions::table
                    .filter(spending_tracker_transactions::user_id.eq(&user_id))
                    .filter(spending_tracker_transactions::id.eq_any(&payload.transaction_ids)),
            )
            .set((
                spending_tracker_transactions::category_id.eq(&payload.category_id),
                spending_tracker_transactions::updated_at.eq(Utc::now().naive_utc()),
            ))
            .execute(conn)?;

            for label in &labels {
                remember_category_mapping(conn, &user_id, label, payload.category_id.as_deref())?;
            }

            Ok(updated)
        })
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    Ok(HttpResponse::Ok().json(serde_json::json!({ "updated_count": updated_count })))
}

#[derive(Debug, Deserialize)]
pub struct QuarterQuery {
    pub year: i32,
    pub quarter: i32,
}

/// Categorized income/expense totals for a quarter's three months, with
/// per-month coverage — for the Quarterly Review integration. Ignore-kind
/// and uncategorized transactions are excluded from both totals; a month
/// with zero imported transactions has `has_data = false` and zero totals.
#[utoipa::path(
    get,
    path = "/api/spending-tracker/quarter-summary",
    tag = "spending_tracker",
    params(
        ("year" = i32, Query, description = "Calendar year"),
        ("quarter" = i32, Query, description = "Quarter, 1-4"),
    ),
    responses(
        (status = 200, description = "Quarter's categorized totals and per-month coverage", body = SpendingTrackerQuarterSummaryResponse),
        (status = 400, description = "Invalid quarter"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[get("/spending-tracker/quarter-summary")]
pub async fn quarter_summary(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    query: web::Query<QuarterQuery>,
) -> AppResult<HttpResponse> {
    let year = query.year;
    let quarter = query.quarter;
    let months = months_of_quarter(quarter)?;

    let user_id = auth.user_id.clone();
    let pool = pool.clone();
    let rows = web::block(
        move || -> AppResult<Vec<(SpendingTrackerTransaction, Option<SpendingTrackerCategory>)>> {
            let mut conn = pool.get()?;
            let month_values: Vec<i32> = months.iter().map(|m| *m as i32).collect();
            let rows = spending_tracker_transactions::table
                .left_join(spending_tracker_categories::table)
                .filter(spending_tracker_transactions::user_id.eq(&user_id))
                .filter(spending_tracker_transactions::year.eq(year))
                .filter(spending_tracker_transactions::month.eq_any(month_values))
                .select((
                    SpendingTrackerTransaction::as_select(),
                    Option::<SpendingTrackerCategory>::as_select(),
                ))
                .load(&mut conn)?;
            Ok(rows)
        },
    )
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    let kind_rows: Vec<MonthKindTotal> = rows
        .iter()
        .map(|(t, c)| MonthKindTotal {
            year: t.year,
            month: t.month,
            kind: c
                .as_ref()
                .map(|c| SpendingTrackerCategoryKind::from_str_lenient(&c.kind)),
            amount: t.amount,
        })
        .collect();

    let summary = summarize_quarter(year, quarter, &kind_rows)?;
    Ok(HttpResponse::Ok().json(SpendingTrackerQuarterSummaryResponse::from(&summary)))
}

#[derive(Debug, Deserialize)]
pub struct YearQuery {
    pub year: i32,
}

/// Categorized income/expense totals for all twelve months of a calendar
/// year, for the year-over-year expenses chart. Same exclusion rules as
/// `quarter_summary`: ignore-kind and uncategorized transactions are
/// excluded from both totals; a month with zero imported transactions has
/// `has_data = false` and zero totals.
#[utoipa::path(
    get,
    path = "/api/spending-tracker/year-summary",
    tag = "spending_tracker",
    params(
        ("year" = i32, Query, description = "Calendar year"),
    ),
    responses(
        (status = 200, description = "Year's categorized totals and per-month coverage", body = SpendingTrackerYearSummaryResponse),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[get("/spending-tracker/year-summary")]
pub async fn year_summary(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    query: web::Query<YearQuery>,
) -> AppResult<HttpResponse> {
    let year = query.year;

    let user_id = auth.user_id.clone();
    let pool = pool.clone();
    let rows = web::block(
        move || -> AppResult<Vec<(SpendingTrackerTransaction, Option<SpendingTrackerCategory>)>> {
            let mut conn = pool.get()?;
            let rows = spending_tracker_transactions::table
                .left_join(spending_tracker_categories::table)
                .filter(spending_tracker_transactions::user_id.eq(&user_id))
                .filter(spending_tracker_transactions::year.eq(year))
                .select((
                    SpendingTrackerTransaction::as_select(),
                    Option::<SpendingTrackerCategory>::as_select(),
                ))
                .load(&mut conn)?;
            Ok(rows)
        },
    )
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    let kind_rows: Vec<MonthKindTotal> = rows
        .iter()
        .map(|(t, c)| MonthKindTotal {
            year: t.year,
            month: t.month,
            kind: c
                .as_ref()
                .map(|c| SpendingTrackerCategoryKind::from_str_lenient(&c.kind)),
            amount: t.amount,
        })
        .collect();

    let category_rows: Vec<CategoryMonthRow> = rows
        .iter()
        .filter_map(|(t, c)| {
            let category = c.as_ref()?;
            Some(CategoryMonthRow {
                month: t.month,
                category_id: category.id.clone(),
                category_name: category.name.clone(),
                kind: SpendingTrackerCategoryKind::from_str_lenient(&category.kind),
                amount: t.amount,
            })
        })
        .collect();

    let summary = summarize_year(year, &kind_rows);
    let mut response = SpendingTrackerYearSummaryResponse::from(&summary);
    response.expense_categories = expense_category_breakdown(&category_rows)
        .iter()
        .map(Into::into)
        .collect();
    Ok(HttpResponse::Ok().json(response))
}
