//! Tax form imports (roadmap Phase 6, feature 3): a parsed tax document
//! (1099-DIV, 1099-INT, 1099-R, W2, SSA-1099, ...) with its box amounts
//! normalized into a field->amount map, so actuals can be compared against
//! the assumptions-driven tax projection.

use std::collections::HashMap;

use chrono::NaiveDateTime;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::error::{AppError, AppResult};
use crate::schema::tax_documents;

/// Kind of tax form imported.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaxFormType {
    #[serde(rename = "1099-div")]
    Div1099,
    #[serde(rename = "1099-int")]
    Int1099,
    #[serde(rename = "1099-r")]
    R1099,
    W2,
    #[serde(rename = "ssa-1099")]
    Ssa1099,
}

impl TaxFormType {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaxFormType::Div1099 => "1099-div",
            TaxFormType::Int1099 => "1099-int",
            TaxFormType::R1099 => "1099-r",
            TaxFormType::W2 => "w2",
            TaxFormType::Ssa1099 => "ssa-1099",
        }
    }
}

#[derive(Queryable, Selectable, Identifiable, Debug, Clone)]
#[diesel(table_name = tax_documents)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct TaxDocument {
    pub id: String,
    pub user_id: String,
    pub tax_year: i32,
    pub form_type: String,
    pub box_data: String,
    pub source_filename: Option<String>,
    pub imported_at: NaiveDateTime,
}

#[derive(Insertable)]
#[diesel(table_name = tax_documents)]
pub struct NewTaxDocument {
    pub id: String,
    pub user_id: String,
    pub tax_year: i32,
    pub form_type: String,
    pub box_data: String,
    pub source_filename: Option<String>,
}

/// Import request: raw two-column CSV (`field,amount` per row) plus which
/// form it came from and which tax year it applies to.
#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct ImportTaxDocumentRequest {
    #[validate(range(min = 2000, max = 2100, message = "must be a plausible tax year"))]
    #[schema(example = 2026)]
    pub tax_year: i32,

    pub form_type: TaxFormType,

    #[validate(length(min = 1, message = "csv_content is required"))]
    #[schema(example = "ordinary_dividends,1200.00\nqualified_dividends,900.00")]
    pub csv_content: String,

    #[validate(length(max = 255))]
    pub source_filename: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TaxDocumentResponse {
    pub id: String,
    pub tax_year: i32,
    pub form_type: String,
    pub box_data: HashMap<String, f64>,
    /// Sum of every parsed box amount, for a quick at-a-glance total.
    pub total: f64,
    pub source_filename: Option<String>,
    #[schema(value_type = String, format = DateTime)]
    pub imported_at: NaiveDateTime,
}

impl TaxDocumentResponse {
    pub fn from_row(row: &TaxDocument) -> AppResult<Self> {
        let box_data: HashMap<String, f64> = serde_json::from_str(&row.box_data)
            .map_err(|e| AppError::Internal(format!("corrupt tax document box data: {e}")))?;
        let total = box_data.values().sum();
        Ok(TaxDocumentResponse {
            id: row.id.clone(),
            tax_year: row.tax_year,
            form_type: row.form_type.clone(),
            box_data,
            total,
            source_filename: row.source_filename.clone(),
            imported_at: row.imported_at,
        })
    }
}

/// Aggregated totals across every imported document for a tax year.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TaxDocumentYearSummary {
    pub tax_year: i32,
    pub document_count: usize,
    pub totals_by_field: HashMap<String, f64>,
    pub grand_total: f64,
}

/// Parse a two-column `field,amount` CSV into a normalized map. Field names
/// are lowercased/trimmed/space-to-underscore so "Ordinary Dividends" and
/// "ordinary_dividends" collapse to the same key. Rows whose second column
/// isn't a number (e.g. a header row) are skipped rather than rejected, so
/// users don't have to strip a header line by hand.
pub fn parse_box_data_csv(csv_content: &str) -> AppResult<HashMap<String, f64>> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(csv_content.as_bytes());

    let mut fields = HashMap::new();
    for result in reader.records() {
        let record =
            result.map_err(|e| AppError::BadRequest(format!("Invalid CSV row: {e}")))?;
        if record.len() < 2 {
            continue;
        }
        let key = record[0].trim().to_lowercase().replace(' ', "_");
        let Ok(value) = record[1].trim().parse::<f64>() else {
            continue;
        };
        if key.is_empty() {
            continue;
        }
        fields.insert(key, value);
    }

    if fields.is_empty() {
        return Err(AppError::BadRequest(
            "No parseable field,amount rows found in the CSV".into(),
        ));
    }

    Ok(fields)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_field_amount_rows_and_normalizes_keys() {
        let csv = "Ordinary Dividends,1200.00\nqualified_dividends,900.50";
        let parsed = parse_box_data_csv(csv).unwrap();
        assert_eq!(parsed.get("ordinary_dividends"), Some(&1200.0));
        assert_eq!(parsed.get("qualified_dividends"), Some(&900.5));
    }

    #[test]
    fn skips_a_leading_header_row() {
        let csv = "field,amount\ninterest_income,42.10";
        let parsed = parse_box_data_csv(csv).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed.get("interest_income"), Some(&42.10));
    }

    #[test]
    fn empty_csv_is_rejected() {
        let result = parse_box_data_csv("field,amount\n");
        assert!(matches!(result, Err(AppError::BadRequest(_))));
    }
}
