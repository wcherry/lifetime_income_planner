//! Spending Tracker (transaction-level CSV import + categorization):
//! distinct from the planned-budget "Spending" page
//! (`models/spending.rs`/`handlers/spending.rs`/`spending_items` table) —
//! this module never touches that table. Pure, DB-free logic only: CSV
//! parsing, dedupe-key hashing, category-name validation, and quarter
//! summarization. Diesel table structs and handlers are added separately.
//!
//! Learned category mappings: when a user explicitly categorizes a
//! transaction that carries a CSV-provided category label
//! (`source_category_label`) — via a single edit or a bulk-categorize call —
//! that (label -> category) choice is persisted to
//! `spending_tracker_category_mappings` (see `handlers::spending_tracker`).
//! The next import checks these learned mappings before falling back to an
//! exact category-name match or the best-guess review flow
//! (`best_guess_category`), so a label only ever needs correcting once.
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use chrono::{NaiveDate, NaiveDateTime, Utc};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

use crate::error::{AppError, AppResult};
use crate::schema::{
    spending_tracker_categories, spending_tracker_category_mappings, spending_tracker_imports,
    spending_tracker_transactions,
};

/// Kind of spending-tracker category: which totals it contributes to.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SpendingTrackerCategoryKind {
    Income,
    Expense,
    Ignore,
}

impl SpendingTrackerCategoryKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            SpendingTrackerCategoryKind::Income => "income",
            SpendingTrackerCategoryKind::Expense => "expense",
            SpendingTrackerCategoryKind::Ignore => "ignore",
        }
    }

    /// Parse the `kind` column's stored TEXT value back into an enum.
    /// Unrecognized values fall back to `Expense` rather than panicking,
    /// since this reads persisted data that should always be one of the
    /// three values written by `as_str`.
    pub fn from_str_lenient(s: &str) -> Self {
        match s {
            "income" => SpendingTrackerCategoryKind::Income,
            "ignore" => SpendingTrackerCategoryKind::Ignore,
            _ => SpendingTrackerCategoryKind::Expense,
        }
    }
}

/// One successfully parsed row from an imported CSV.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedTransactionRow {
    pub row_number: usize,
    pub transaction_date: NaiveDate,
    pub description: String,
    pub amount: f64,
    /// The CSV's own category label for this row, if a category column was
    /// detected (e.g. many bank/card exports include one) — used to
    /// auto-categorize the imported transaction (see `resolve_category_id`).
    pub category: Option<String>,
    /// This row exactly as it appeared in the file — a JSON object string of
    /// `{header: value}` pairs, column order preserved, before any
    /// parsing/normalization. Stored verbatim (see `raw_row_to_json`) purely
    /// so a user can troubleshoot an individual imported transaction.
    pub raw_row_json: String,
}

/// One row that couldn't be parsed, and why, so the import can report it
/// back to the user instead of failing the whole file.
#[derive(Debug, Clone, PartialEq)]
pub struct SkippedRow {
    pub row_number: usize,
    pub reason: String,
}

/// Result of parsing a whole CSV: the rows that parsed cleanly, plus any
/// rows that were skipped and why.
#[derive(Debug, Clone, Default)]
pub struct ParsedCsvResult {
    pub rows: Vec<ParsedTransactionRow>,
    pub skipped: Vec<SkippedRow>,
}

const DATE_HEADERS: &[&str] = &["date", "transaction date", "posting date", "post date"];
const DESCRIPTION_HEADERS: &[&str] = &["description", "memo", "payee", "name"];
const AMOUNT_HEADERS: &[&str] = &["amount", "transaction amount"];
const DEBIT_HEADERS: &[&str] = &["debit", "debit amount"];
const CREDIT_HEADERS: &[&str] = &["credit", "credit amount"];
const CATEGORY_HEADERS: &[&str] = &["category", "categories"];
const DATE_FORMATS: &[&str] = &["%Y-%m-%d", "%m/%d/%Y", "%m/%d/%y"];

/// Try each of `DATE_FORMATS` in order; the first one that parses wins.
fn parse_date(raw: &str) -> Option<NaiveDate> {
    DATE_FORMATS
        .iter()
        .find_map(|fmt| NaiveDate::parse_from_str(raw, fmt).ok())
}

/// Tolerates a leading "$", thousands commas, and parenthesized negatives
/// (e.g. "(123.45)" -> -123.45). Returns `None` for anything else that
/// doesn't parse as a number.
fn parse_amount(raw: &str) -> Option<f64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (negative, unwrapped) =
        if trimmed.starts_with('(') && trimmed.ends_with(')') && trimmed.len() >= 2 {
            (true, &trimmed[1..trimmed.len() - 1])
        } else {
            (false, trimmed)
        };
    let unwrapped = unwrapped
        .trim()
        .strip_prefix('$')
        .unwrap_or(unwrapped.trim())
        .trim();
    let cleaned: String = unwrapped.chars().filter(|c| *c != ',').collect();
    let value: f64 = cleaned.parse().ok()?;
    Some(if negative { -value.abs() } else { value })
}

/// Case-insensitive lookup of the first header matching any of `candidates`.
fn find_header_index(headers: &csv::StringRecord, candidates: &[&str]) -> Option<usize> {
    headers
        .iter()
        .position(|h| candidates.contains(&h.trim().to_lowercase().as_str()))
}

/// Serializes one CSV row's original `{header: value}` pairs to a JSON
/// object string, column order preserved (relies on the `preserve_order`
/// serde_json feature) — the raw, unparsed values as they appeared in the
/// file, for troubleshooting. Falls back to `"{}"` on the (unreachable in
/// practice) case that serialization fails.
fn raw_row_to_json(headers: &csv::StringRecord, record: &csv::StringRecord) -> String {
    let map: serde_json::Map<String, serde_json::Value> = headers
        .iter()
        .zip(record.iter())
        .map(|(header, value)| (header.to_string(), serde_json::Value::String(value.to_string())))
        .collect();
    serde_json::to_string(&map).unwrap_or_else(|_| "{}".to_string())
}

/// Builds a `raw_row_json`-shaped JSON object for a manually-entered
/// transaction (no CSV row exists) — so the same "imported values"
/// troubleshooting view works whether a transaction came from a CSV import
/// or was entered by hand.
pub fn manual_entry_raw_row_json(transaction_date: NaiveDate, description: &str, amount: f64) -> String {
    let map: serde_json::Map<String, serde_json::Value> = [
        ("Date".to_string(), serde_json::Value::String(transaction_date.to_string())),
        ("Description".to_string(), serde_json::Value::String(description.to_string())),
        ("Amount".to_string(), serde_json::Value::String(amount.to_string())),
        ("Source".to_string(), serde_json::Value::String("Manual entry".to_string())),
    ]
    .into_iter()
    .collect();
    serde_json::to_string(&map).unwrap_or_else(|_| "{}".to_string())
}

/// Trims and lowercases a category label for matching/lookup — used both for
/// the in-memory exact-match lookup at import time and for the persisted
/// "learned" mapping rules (`spending_tracker_category_mappings.normalized_label`,
/// see the module doc on CSV-label auto-categorization).
pub fn normalize_label(label: &str) -> String {
    label.trim().to_lowercase()
}

/// Tolerant CSV parser. Case-insensitive header detection:
/// date column: "date", "transaction date", "posting date", "post date"
/// description column: "description", "memo", "payee", "name"
/// amount column: "amount", "transaction amount" — OR, if absent, combine separate
///   "debit"/"credit" ("debit amount"/"credit amount") columns: debit -> negative, credit -> positive.
/// category column (optional): "category", "categories" — many bank/card exports include their
///   own categorization; if present, it's carried through on each row for auto-categorization
///   (see `resolve_category_id`) rather than left for the user to assign by hand.
/// Date formats tried in order: "%Y-%m-%d", "%m/%d/%Y", "%m/%d/%y".
/// Amount parsing tolerates a leading "$", thousands commas, and parenthesized negatives e.g. "(123.45)" -> -123.45.
/// A row that can't be parsed (bad date, no usable amount) is recorded in `skipped`, not a hard error.
/// Returns Err(AppError::BadRequest) only if no date or no amount/debit-credit column could be found at all,
/// or if the CSV has zero data rows.
pub fn parse_spending_transactions_csv(csv_content: &str) -> AppResult<ParsedCsvResult> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_reader(csv_content.as_bytes());

    let headers = reader
        .headers()
        .map_err(|e| AppError::BadRequest(format!("Invalid CSV header row: {e}")))?
        .clone();

    let date_idx = find_header_index(&headers, DATE_HEADERS)
        .ok_or_else(|| AppError::BadRequest("CSV is missing a recognizable date column".into()))?;
    let description_idx = find_header_index(&headers, DESCRIPTION_HEADERS);
    let amount_idx = find_header_index(&headers, AMOUNT_HEADERS);
    let debit_idx = find_header_index(&headers, DEBIT_HEADERS);
    let credit_idx = find_header_index(&headers, CREDIT_HEADERS);
    let category_idx = find_header_index(&headers, CATEGORY_HEADERS);

    if amount_idx.is_none() && debit_idx.is_none() && credit_idx.is_none() {
        return Err(AppError::BadRequest(
            "CSV is missing a recognizable amount (or debit/credit) column".into(),
        ));
    }

    let mut rows = Vec::new();
    let mut skipped = Vec::new();
    let mut saw_any_row = false;

    for (i, record_result) in reader.records().enumerate() {
        saw_any_row = true;
        let row_number = i + 1;

        let record = match record_result {
            Ok(r) => r,
            Err(e) => {
                skipped.push(SkippedRow {
                    row_number,
                    reason: format!("Invalid CSV row: {e}"),
                });
                continue;
            }
        };

        let date_raw = record.get(date_idx).unwrap_or("").trim();
        let Some(transaction_date) = parse_date(date_raw) else {
            skipped.push(SkippedRow {
                row_number,
                reason: format!("Unparseable date: '{date_raw}'"),
            });
            continue;
        };

        let description = description_idx
            .and_then(|idx| record.get(idx))
            .unwrap_or("")
            .trim()
            .to_string();

        let amount = if let Some(idx) = amount_idx {
            let raw = record.get(idx).unwrap_or("").trim();
            match parse_amount(raw) {
                Some(a) => a,
                None => {
                    skipped.push(SkippedRow {
                        row_number,
                        reason: format!("Unparseable amount: '{raw}'"),
                    });
                    continue;
                }
            }
        } else {
            let debit_raw = debit_idx
                .and_then(|idx| record.get(idx))
                .unwrap_or("")
                .trim();
            let credit_raw = credit_idx
                .and_then(|idx| record.get(idx))
                .unwrap_or("")
                .trim();
            let combined = if !debit_raw.is_empty() {
                parse_amount(debit_raw).map(|d| -d.abs())
            } else {
                None
            }
            .or_else(|| {
                if !credit_raw.is_empty() {
                    parse_amount(credit_raw).map(|c| c.abs())
                } else {
                    None
                }
            });
            match combined {
                Some(a) => a,
                None => {
                    skipped.push(SkippedRow {
                        row_number,
                        reason: "No usable amount in debit/credit columns".to_string(),
                    });
                    continue;
                }
            }
        };

        let category = category_idx
            .and_then(|idx| record.get(idx))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        rows.push(ParsedTransactionRow {
            row_number,
            transaction_date,
            description,
            amount,
            category,
            raw_row_json: raw_row_to_json(&headers, &record),
        });
    }

    if !saw_any_row {
        return Err(AppError::BadRequest(
            "CSV has no data rows to import".into(),
        ));
    }

    Ok(ParsedCsvResult { rows, skipped })
}

/// Lowercase, trim, collapse internal whitespace to single spaces — used in
/// the dedupe key so cosmetic differences in a bank's export don't defeat
/// deduplication.
pub fn normalize_description(description: &str) -> String {
    description
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Deterministic, stable hash string of (year, month, date,
/// normalize_description(description), amount). Same inputs must always
/// produce the same output; different amount/date/description must (in
/// practice) differ.
pub fn compute_dedupe_key(
    year: i32,
    month: i32,
    date: NaiveDate,
    description: &str,
    amount: f64,
) -> String {
    // `DefaultHasher::new()` uses fixed (all-zero) keys, unlike
    // `RandomState`'s per-process-random keys — so hashing the same inputs
    // always produces the same output across process runs, which is exactly
    // what a stable, content-based dedupe key needs.
    let mut hasher = DefaultHasher::new();
    year.hash(&mut hasher);
    month.hash(&mut hasher);
    date.hash(&mut hasher);
    normalize_description(description).hash(&mut hasher);
    // f64 has no Hash impl; hash its bit pattern instead.
    amount.to_bits().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Create-a-custom-category request payload.
#[derive(Debug, Clone, Deserialize, ToSchema, Validate)]
pub struct NewCategoryRequest {
    #[validate(length(min = 1, max = 100, message = "name must be 1-100 characters"))]
    pub name: String,
    pub kind: SpendingTrackerCategoryKind,
}

/// Case-insensitive, trimmed duplicate-name check against a caller-supplied
/// list of existing category names (predefined + this user's own custom
/// ones). Err(AppError::BadRequest) on collision.
pub fn validate_unique_category_name(name: &str, existing_names: &[String]) -> AppResult<()> {
    let normalized = name.trim().to_lowercase();
    let collides = existing_names
        .iter()
        .any(|existing| existing.trim().to_lowercase() == normalized);
    if collides {
        return Err(AppError::BadRequest(format!(
            "A category named \"{}\" already exists",
            name.trim()
        )));
    }
    Ok(())
}

/// Splits `label` on non-alphanumeric characters into lowercase word tokens,
/// naively singularizing each by stripping a single trailing 's' (so e.g.
/// "Groceries"/"Grocery" and "Pets"/"Pet" contribute the same token). Used
/// by `best_guess_category` for loose, dependency-free label matching.
fn tokenize_for_matching(label: &str) -> std::collections::HashSet<String> {
    label
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| {
            let lower = w.to_lowercase();
            lower
                .strip_suffix('s')
                .map(|s| s.to_string())
                .unwrap_or(lower)
        })
        .collect()
}

/// Best-effort match of a CSV category label (that didn't exactly match) to
/// one of `categories`, by comparing normalized word tokens (see
/// `tokenize_for_matching`) between the label and each category's name.
/// Never guesses blindly: returns `None` unless at least one token is
/// shared. The category with the most shared tokens wins; ties are broken by
/// shorter name, then alphabetically, so the result is deterministic.
pub fn best_guess_category<'a>(
    label: &str,
    categories: &'a [SpendingTrackerCategory],
) -> Option<&'a SpendingTrackerCategory> {
    let label_tokens = tokenize_for_matching(label);
    if label_tokens.is_empty() {
        return None;
    }

    let mut best: Option<(usize, &SpendingTrackerCategory)> = None;
    for category in categories {
        let overlap = tokenize_for_matching(&category.name)
            .intersection(&label_tokens)
            .count();
        if overlap == 0 {
            continue;
        }
        let is_better = match best {
            None => true,
            Some((best_overlap, best_category)) => {
                overlap > best_overlap
                    || (overlap == best_overlap
                        && (category.name.len(), category.name.as_str())
                            < (best_category.name.len(), best_category.name.as_str()))
            }
        };
        if is_better {
            best = Some((overlap, category));
        }
    }
    best.map(|(_, category)| category)
}

/// [Jan,Feb,Mar] for quarter 1 ... [Oct,Nov,Dec] for quarter 4.
/// Err(AppError::BadRequest) if quarter not in 1..=4.
pub fn months_of_quarter(quarter: i32) -> AppResult<[u32; 3]> {
    match quarter {
        1 => Ok([1, 2, 3]),
        2 => Ok([4, 5, 6]),
        3 => Ok([7, 8, 9]),
        4 => Ok([10, 11, 12]),
        _ => Err(AppError::BadRequest(
            "Quarter must be between 1 and 4".into(),
        )),
    }
}

/// One month's worth of transactions, pre-aggregated by category kind, as
/// input to `summarize_quarter`.
#[derive(Debug, Clone)]
pub struct MonthKindTotal {
    pub year: i32,
    pub month: i32,
    /// None = uncategorized transaction (excluded from all totals)
    pub kind: Option<SpendingTrackerCategoryKind>,
    pub amount: f64,
}

/// Whether a given (year, month) has any imported transactions, and its
/// categorized totals.
#[derive(Debug, Clone)]
pub struct MonthCoverage {
    pub year: i32,
    pub month: i32,
    pub has_data: bool,
    pub income_total: f64,
    pub expense_total: f64,
}

/// A quarter's categorized totals plus per-month coverage, for the
/// Quarterly Review integration.
#[derive(Debug, Clone)]
pub struct QuarterSummary {
    pub year: i32,
    pub quarter: i32,
    pub income_total: f64,
    pub expense_total: f64,
    pub months: Vec<MonthCoverage>,
}

/// Sums abs(amount) per kind (Income -> income_total, Expense -> expense_total; Ignore and
/// None/uncategorized excluded entirely from both totals). `has_data` for a month is true if
/// ANY row (any kind, including uncategorized) exists for that (year, month) in `rows` — i.e. it
/// reflects "this month has imported transactions", not "this month is fully categorized".
/// Err(AppError::BadRequest) if quarter not in 1..=4.
pub fn summarize_quarter(
    year: i32,
    quarter: i32,
    rows: &[MonthKindTotal],
) -> AppResult<QuarterSummary> {
    let months = months_of_quarter(quarter)?;

    let mut income_total = 0.0;
    let mut expense_total = 0.0;
    let mut month_coverages = Vec::with_capacity(months.len());

    for month in months {
        let month = month as i32;
        let month_rows: Vec<&MonthKindTotal> = rows
            .iter()
            .filter(|r| r.year == year && r.month == month)
            .collect();
        let has_data = !month_rows.is_empty();

        let mut month_income = 0.0;
        let mut month_expense = 0.0;
        for row in &month_rows {
            match row.kind {
                Some(SpendingTrackerCategoryKind::Income) => month_income += row.amount.abs(),
                Some(SpendingTrackerCategoryKind::Expense) => month_expense += row.amount.abs(),
                Some(SpendingTrackerCategoryKind::Ignore) | None => {}
            }
        }

        income_total += month_income;
        expense_total += month_expense;
        month_coverages.push(MonthCoverage {
            year,
            month,
            has_data,
            income_total: month_income,
            expense_total: month_expense,
        });
    }

    Ok(QuarterSummary {
        year,
        quarter,
        income_total,
        expense_total,
        months: month_coverages,
    })
}

/// A calendar year's categorized totals plus per-month coverage — for the
/// year-over-year expenses chart on the Spending Tracker page.
#[derive(Debug, Clone)]
pub struct YearSummary {
    pub year: i32,
    pub income_total: f64,
    pub expense_total: f64,
    pub months: Vec<MonthCoverage>,
}

/// Same aggregation as `summarize_quarter`, but over all twelve months of
/// `year` rather than a quarter's three.
pub fn summarize_year(year: i32, rows: &[MonthKindTotal]) -> YearSummary {
    let mut income_total = 0.0;
    let mut expense_total = 0.0;
    let mut month_coverages = Vec::with_capacity(12);

    for month in 1..=12 {
        let month_rows: Vec<&MonthKindTotal> = rows
            .iter()
            .filter(|r| r.year == year && r.month == month)
            .collect();
        let has_data = !month_rows.is_empty();

        let mut month_income = 0.0;
        let mut month_expense = 0.0;
        for row in &month_rows {
            match row.kind {
                Some(SpendingTrackerCategoryKind::Income) => month_income += row.amount.abs(),
                Some(SpendingTrackerCategoryKind::Expense) => month_expense += row.amount.abs(),
                Some(SpendingTrackerCategoryKind::Ignore) | None => {}
            }
        }

        income_total += month_income;
        expense_total += month_expense;
        month_coverages.push(MonthCoverage {
            year,
            month,
            has_data,
            income_total: month_income,
            expense_total: month_expense,
        });
    }

    YearSummary {
        year,
        income_total,
        expense_total,
        months: month_coverages,
    }
}

/// One categorized transaction's month + amount, as input to
/// `expense_category_breakdown`. Only categorized rows are meaningful here —
/// callers filter out uncategorized transactions before building these
/// (matching `summarize_year`'s exclusion of uncategorized amounts).
#[derive(Debug, Clone)]
pub struct CategoryMonthRow {
    pub month: i32,
    pub category_id: String,
    pub category_name: String,
    pub kind: SpendingTrackerCategoryKind,
    pub amount: f64,
}

/// One category's expense total for each of the year's twelve months (index
/// 0 = January), for the Spending Tracker's stacked year chart.
#[derive(Debug, Clone)]
pub struct CategoryMonthTotal {
    pub category_id: String,
    pub category_name: String,
    pub monthly_totals: Vec<f64>,
}

/// Sums abs(amount) per category per month, expense-kind categories only
/// (income and ignore-kind rows are excluded, same as `summarize_year`'s
/// totals). Sorted by the category's full-year total, descending — the
/// caller decides how many series to render directly vs. fold into "Other".
pub fn expense_category_breakdown(rows: &[CategoryMonthRow]) -> Vec<CategoryMonthTotal> {
    let mut totals: HashMap<String, (String, [f64; 12])> = HashMap::new();
    for row in rows {
        if row.kind != SpendingTrackerCategoryKind::Expense {
            continue;
        }
        let entry = totals
            .entry(row.category_id.clone())
            .or_insert_with(|| (row.category_name.clone(), [0.0; 12]));
        if let Some(slot) = (row.month - 1).try_into().ok().and_then(|i: usize| entry.1.get_mut(i))
        {
            *slot += row.amount.abs();
        }
    }

    let mut result: Vec<CategoryMonthTotal> = totals
        .into_iter()
        .map(|(category_id, (category_name, monthly_totals))| CategoryMonthTotal {
            category_id,
            category_name,
            monthly_totals: monthly_totals.to_vec(),
        })
        .collect();
    result.sort_by(|a, b| {
        let total_a: f64 = a.monthly_totals.iter().sum();
        let total_b: f64 = b.monthly_totals.iter().sum();
        total_b
            .partial_cmp(&total_a)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    result
}

// ---------------------------------------------------------------------------
// Diesel table structs
// ---------------------------------------------------------------------------

/// A spending-tracker category, as persisted. `user_id = None` means
/// predefined/global (visible to every user, read-only).
#[derive(Queryable, Selectable, Identifiable, Debug, Clone)]
#[diesel(table_name = spending_tracker_categories)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct SpendingTrackerCategory {
    pub id: String,
    pub user_id: Option<String>,
    pub name: String,
    pub kind: String,
    pub is_predefined: bool,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Insertable)]
#[diesel(table_name = spending_tracker_categories)]
pub struct NewSpendingTrackerCategory {
    pub id: String,
    pub user_id: Option<String>,
    pub name: String,
    pub kind: String,
    pub is_predefined: bool,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// One CSV upload, kept as an audit trail (not one row per month).
#[derive(Queryable, Selectable, Identifiable, Debug, Clone)]
#[diesel(table_name = spending_tracker_imports)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct SpendingTrackerImport {
    pub id: String,
    pub user_id: String,
    pub year: i32,
    pub month: i32,
    pub source_filename: Option<String>,
    pub row_count: i32,
    pub duplicate_count: i32,
    pub skipped_count: i32,
    pub imported_at: NaiveDateTime,
}

#[derive(Insertable)]
#[diesel(table_name = spending_tracker_imports)]
pub struct NewSpendingTrackerImport {
    pub id: String,
    pub user_id: String,
    pub year: i32,
    pub month: i32,
    pub source_filename: Option<String>,
    pub row_count: i32,
    pub duplicate_count: i32,
    pub skipped_count: i32,
}

/// A single imported transaction, as persisted.
#[derive(Queryable, Selectable, Identifiable, Debug, Clone)]
#[diesel(table_name = spending_tracker_transactions)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct SpendingTrackerTransaction {
    pub id: String,
    pub user_id: String,
    pub import_id: String,
    pub year: i32,
    pub month: i32,
    pub transaction_date: NaiveDate,
    pub description: String,
    pub amount: f64,
    pub category_id: Option<String>,
    pub dedupe_key: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    /// This row exactly as it appeared in the imported file (JSON object
    /// string, see `raw_row_to_json`) — troubleshooting only.
    pub raw_row_json: String,
    /// The CSV's own category label for this row (see
    /// `ParsedTransactionRow::category`), `None` for manual entries or CSVs
    /// with no category column. Used to look up/learn a category mapping.
    pub source_category_label: Option<String>,
}

#[derive(Insertable)]
#[diesel(table_name = spending_tracker_transactions)]
pub struct NewSpendingTrackerTransaction {
    pub id: String,
    pub user_id: String,
    pub import_id: String,
    pub year: i32,
    pub month: i32,
    pub transaction_date: NaiveDate,
    pub description: String,
    pub amount: f64,
    pub category_id: Option<String>,
    pub dedupe_key: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub raw_row_json: String,
    pub source_category_label: Option<String>,
}

/// A persisted "learned" mapping from a CSV category label to one of the
/// user's categories (see the module doc). `normalized_label` (trimmed,
/// lowercased — see `normalize_label`) is the lookup key; `label` keeps the
/// original casing for display.
#[derive(Queryable, Selectable, Identifiable, Debug, Clone)]
#[diesel(table_name = spending_tracker_category_mappings)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct SpendingTrackerCategoryMapping {
    pub id: String,
    pub user_id: String,
    pub label: String,
    pub normalized_label: String,
    pub category_id: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Insertable)]
#[diesel(table_name = spending_tracker_category_mappings)]
pub struct NewSpendingTrackerCategoryMapping {
    pub id: String,
    pub user_id: String,
    pub label: String,
    pub normalized_label: String,
    pub category_id: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// Predefined categories seeded once at startup, `user_id = NULL`,
/// `is_predefined = true`, all `kind = expense`.
const PREDEFINED_CATEGORY_NAMES: &[&str] = &[
    "Housing",
    "Transportation",
    "Food",
    "Entertainment",
    "Medical",
    "General Merchandise",
    "Dependent Care",
    "Utilities",
    "Pets",
    "Gifts",
    "Other",
];

/// Seed the predefined spending-tracker categories where none exist yet.
/// Idempotent (checked against the count of `user_id IS NULL` rows), so
/// admin edits to the seeded rows are preserved on subsequent startups —
/// same pattern as `seed_tax_tables_if_empty`/`seed_aca_tables_if_empty`.
pub fn seed_spending_tracker_categories_if_empty(conn: &mut SqliteConnection) -> QueryResult<()> {
    let predefined_count: i64 = spending_tracker_categories::table
        .filter(spending_tracker_categories::user_id.is_null())
        .count()
        .get_result(conn)?;

    if predefined_count == 0 {
        let now = Utc::now().naive_utc();
        let rows: Vec<NewSpendingTrackerCategory> = PREDEFINED_CATEGORY_NAMES
            .iter()
            .map(|name| NewSpendingTrackerCategory {
                id: Uuid::new_v4().to_string(),
                user_id: None,
                name: name.to_string(),
                kind: SpendingTrackerCategoryKind::Expense.as_str().to_string(),
                is_predefined: true,
                created_at: now,
                updated_at: now,
            })
            .collect();
        diesel::insert_into(spending_tracker_categories::table)
            .values(&rows)
            .execute(conn)?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Response DTOs
// ---------------------------------------------------------------------------

/// API view of a category.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SpendingTrackerCategoryResponse {
    pub id: String,
    /// `true` for a user's own custom category, `false` for a predefined one.
    pub is_own: bool,
    pub name: String,
    pub kind: SpendingTrackerCategoryKind,
    pub is_predefined: bool,
}

impl SpendingTrackerCategoryResponse {
    pub fn from_row(row: &SpendingTrackerCategory, caller_user_id: &str) -> Self {
        SpendingTrackerCategoryResponse {
            id: row.id.clone(),
            is_own: row.user_id.as_deref() == Some(caller_user_id),
            name: row.name.clone(),
            kind: SpendingTrackerCategoryKind::from_str_lenient(&row.kind),
            is_predefined: row.is_predefined,
        }
    }
}

/// API view of a transaction, with the owning category's name/kind
/// denormalized in so the frontend doesn't need a second lookup. `raw_row`
/// carries the imported CSV row exactly as it appeared in the file
/// (`{header: value}`, column order preserved) so a user can troubleshoot an
/// individual imported transaction (e.g. via a details popup).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SpendingTrackerTransactionResponse {
    pub id: String,
    pub year: i32,
    pub month: i32,
    #[schema(value_type = String, format = Date)]
    pub transaction_date: NaiveDate,
    pub description: String,
    pub amount: f64,
    pub category_id: Option<String>,
    pub category_name: Option<String>,
    pub category_kind: Option<SpendingTrackerCategoryKind>,
    /// The imported CSV row's own `{header: value}` pairs, verbatim.
    #[schema(value_type = Object)]
    pub raw_row: serde_json::Value,
}

impl SpendingTrackerTransactionResponse {
    pub fn from_row(row: &SpendingTrackerTransaction, category: Option<&SpendingTrackerCategory>) -> Self {
        SpendingTrackerTransactionResponse {
            id: row.id.clone(),
            year: row.year,
            month: row.month,
            transaction_date: row.transaction_date,
            description: row.description.clone(),
            amount: row.amount,
            category_id: row.category_id.clone(),
            category_name: category.map(|c| c.name.clone()),
            category_kind: category.map(|c| SpendingTrackerCategoryKind::from_str_lenient(&c.kind)),
            raw_row: serde_json::from_str(&row.raw_row_json).unwrap_or(serde_json::Value::Null),
        }
    }
}

/// One row that couldn't be imported, returned to the caller so they know
/// what to fix (mirrors `SkippedRow`, but `Serialize`+`ToSchema` for the API).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SkippedRowResponse {
    pub row_number: usize,
    pub reason: String,
}

impl From<&SkippedRow> for SkippedRowResponse {
    fn from(row: &SkippedRow) -> Self {
        SkippedRowResponse {
            row_number: row.row_number,
            reason: row.reason.clone(),
        }
    }
}

/// One CSV category label (see `ParsedTransactionRow::category`) from this
/// import that didn't exactly match an existing category, along with a
/// best-guess suggestion (see `best_guess_category`) and the ids of the
/// newly imported transactions that carried the label. Lets the caller
/// review and apply (or correct) the mapping — e.g. via
/// `POST /spending-tracker/transactions/bulk-categorize` — rather than
/// having categories silently auto-created.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CategoryMappingSuggestion {
    pub label: String,
    pub suggested_category_id: Option<String>,
    pub suggested_category_name: Option<String>,
    pub transaction_ids: Vec<String>,
}

/// Result of a `POST /spending-tracker/import` call.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SpendingTrackerImportResult {
    pub import_id: String,
    /// Rows newly inserted by this import (excludes duplicates and skipped rows).
    pub imported_count: usize,
    /// Rows that parsed cleanly but were already present (same dedupe key).
    pub duplicate_count: usize,
    pub skipped_rows: Vec<SkippedRowResponse>,
    /// Distinct CSV category labels that need the caller's review (no exact
    /// match was found), one entry per label, each with a best-guess
    /// suggestion and the transactions it applies to.
    pub category_mappings: Vec<CategoryMappingSuggestion>,
}

/// One (year, month) bucket with imported data, for the month picker.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SpendingTrackerMonthSummary {
    pub year: i32,
    pub month: i32,
    pub transaction_count: i64,
    #[schema(value_type = String, format = DateTime)]
    pub last_imported_at: NaiveDateTime,
}

/// API view of `MonthCoverage`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SpendingTrackerMonthCoverageResponse {
    pub year: i32,
    pub month: i32,
    pub has_data: bool,
    pub income_total: f64,
    pub expense_total: f64,
}

impl From<&MonthCoverage> for SpendingTrackerMonthCoverageResponse {
    fn from(m: &MonthCoverage) -> Self {
        SpendingTrackerMonthCoverageResponse {
            year: m.year,
            month: m.month,
            has_data: m.has_data,
            income_total: m.income_total,
            expense_total: m.expense_total,
        }
    }
}

/// API view of `QuarterSummary`, for the Quarterly Review integration.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SpendingTrackerQuarterSummaryResponse {
    pub year: i32,
    pub quarter: i32,
    pub income_total: f64,
    pub expense_total: f64,
    pub months: Vec<SpendingTrackerMonthCoverageResponse>,
}

impl From<&QuarterSummary> for SpendingTrackerQuarterSummaryResponse {
    fn from(s: &QuarterSummary) -> Self {
        SpendingTrackerQuarterSummaryResponse {
            year: s.year,
            quarter: s.quarter,
            income_total: s.income_total,
            expense_total: s.expense_total,
            months: s.months.iter().map(Into::into).collect(),
        }
    }
}

/// API view of one `CategoryMonthTotal` — a category's expense total for
/// each of the year's twelve months, for the stacked year chart.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SpendingTrackerCategoryMonthSeriesResponse {
    pub category_id: String,
    pub category_name: String,
    /// Twelve entries, index 0 = January.
    pub monthly_totals: Vec<f64>,
}

impl From<&CategoryMonthTotal> for SpendingTrackerCategoryMonthSeriesResponse {
    fn from(c: &CategoryMonthTotal) -> Self {
        SpendingTrackerCategoryMonthSeriesResponse {
            category_id: c.category_id.clone(),
            category_name: c.category_name.clone(),
            monthly_totals: c.monthly_totals.clone(),
        }
    }
}

/// API view of `YearSummary`, for the year-over-year expenses chart.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SpendingTrackerYearSummaryResponse {
    pub year: i32,
    pub income_total: f64,
    pub expense_total: f64,
    pub months: Vec<SpendingTrackerMonthCoverageResponse>,
    /// Per-category monthly expense breakdown, sorted by full-year total
    /// descending — for the stacked area chart. Only categorized,
    /// expense-kind transactions contribute.
    pub expense_categories: Vec<SpendingTrackerCategoryMonthSeriesResponse>,
}

impl From<&YearSummary> for SpendingTrackerYearSummaryResponse {
    fn from(s: &YearSummary) -> Self {
        SpendingTrackerYearSummaryResponse {
            year: s.year,
            income_total: s.income_total,
            expense_total: s.expense_total,
            months: s.months.iter().map(Into::into).collect(),
            expense_categories: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Request DTOs
// ---------------------------------------------------------------------------

/// Import a month's worth of transactions from CSV.
#[derive(Debug, Clone, Deserialize, ToSchema, Validate)]
pub struct ImportSpendingTransactionsRequest {
    #[validate(range(min = 2000, max = 2100, message = "must be a plausible year"))]
    #[schema(example = 2026)]
    pub year: i32,
    #[validate(range(min = 1, max = 12, message = "month must be between 1 and 12"))]
    pub month: i32,
    #[validate(length(min = 1, message = "csv_content is required"))]
    #[schema(example = "Date,Description,Amount\n2026-01-05,Coffee Shop,-4.50")]
    pub csv_content: String,
    #[validate(length(max = 255))]
    pub source_filename: Option<String>,
}

/// Record a single transaction by hand — e.g. cash spending, or something a
/// bank export missed — rather than via CSV import.
#[derive(Debug, Clone, Deserialize, ToSchema, Validate)]
pub struct CreateManualTransactionRequest {
    #[validate(range(min = 2000, max = 2100, message = "must be a plausible year"))]
    #[schema(example = 2026)]
    pub year: i32,
    #[validate(range(min = 1, max = 12, message = "month must be between 1 and 12"))]
    pub month: i32,
    #[schema(value_type = String, format = Date, example = "2026-01-05")]
    pub transaction_date: NaiveDate,
    #[validate(length(min = 1, max = 500, message = "description is required"))]
    pub description: String,
    /// Signed: negative = expense, positive = income (matches the CSV import convention).
    pub amount: f64,
    pub category_id: Option<String>,
}

/// Rename/re-kind a custom category. Same shape/validation as
/// `NewCategoryRequest` (see its rationale) — a distinct type keeps the
/// OpenAPI schema and intent clear for update-only callers.
#[derive(Debug, Clone, Deserialize, ToSchema, Validate)]
pub struct UpdateCategoryRequest {
    #[validate(length(min = 1, max = 100, message = "name must be 1-100 characters"))]
    pub name: String,
    pub kind: SpendingTrackerCategoryKind,
}

/// Assign (or clear, with `None`) a single transaction's category.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct SetTransactionCategoryRequest {
    pub category_id: Option<String>,
}

/// Assign (or clear) a category across many transactions at once.
#[derive(Debug, Clone, Deserialize, ToSchema, Validate)]
pub struct BulkCategorizeRequest {
    #[validate(length(min = 1, message = "transaction_ids must not be empty"))]
    pub transaction_ids: Vec<String>,
    pub category_id: Option<String>,
}

/// Whether `category` (looked up by id, `None` if the id didn't resolve) is
/// visible to `caller_user_id` — i.e. it's predefined or owned by the
/// caller. Extracted so it's unit-testable without a database.
pub fn category_visible_to_user(
    category: Option<&SpendingTrackerCategory>,
    caller_user_id: &str,
) -> bool {
    match category {
        None => false,
        Some(c) => c.user_id.is_none() || c.user_id.as_deref() == Some(caller_user_id),
    }
}

/// Whether `category` may be edited/deleted by `caller_user_id` — must be
/// owned by the caller and not predefined. Extracted so it's unit-testable
/// without a database.
pub fn category_editable_by_user(category: &SpendingTrackerCategory, caller_user_id: &str) -> bool {
    !category.is_predefined && category.user_id.as_deref() == Some(caller_user_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_spending_transactions_csv ---

    #[test]
    fn parses_standard_date_description_amount_header() {
        let csv =
            "Date,Description,Amount\n2026-01-05,Coffee Shop,-4.50\n2026-01-06,Paycheck,2000.00";
        let result = parse_spending_transactions_csv(csv).unwrap();
        assert_eq!(result.rows.len(), 2);
        assert_eq!(result.skipped.len(), 0);
        assert_eq!(
            result.rows[0].transaction_date,
            NaiveDate::from_ymd_opt(2026, 1, 5).unwrap()
        );
        assert_eq!(result.rows[0].description, "Coffee Shop");
        assert_eq!(result.rows[0].amount, -4.50);
        assert_eq!(result.rows[1].amount, 2000.00);
    }

    #[test]
    fn parses_alternate_headers_transaction_date_memo() {
        let csv = "Transaction Date,Memo,Amount\n01/05/2026,Grocery Store,-88.12";
        let result = parse_spending_transactions_csv(csv).unwrap();
        assert_eq!(result.rows.len(), 1);
        assert_eq!(
            result.rows[0].transaction_date,
            NaiveDate::from_ymd_opt(2026, 1, 5).unwrap()
        );
        assert_eq!(result.rows[0].description, "Grocery Store");
    }

    #[test]
    fn parses_alternate_headers_posting_date_payee() {
        let csv = "Posting Date,Payee,Amount\n2026-02-01,Landlord,-1500.00";
        let result = parse_spending_transactions_csv(csv).unwrap();
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].description, "Landlord");
    }

    #[test]
    fn parses_alternate_headers_name_column_for_description() {
        let csv = "Date,Name,Amount\n2026-02-02,ACH Deposit,500.00";
        let result = parse_spending_transactions_csv(csv).unwrap();
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].description, "ACH Deposit");
    }

    #[test]
    fn parses_a_category_column_when_present() {
        let csv = "Date,Description,Amount,Category\n2026-01-05,Grocery Store,-88.12,Groceries";
        let result = parse_spending_transactions_csv(csv).unwrap();
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].category, Some("Groceries".to_string()));
    }

    #[test]
    fn blank_category_cell_is_none_not_an_empty_string() {
        let csv = "Date,Description,Amount,Category\n2026-01-05,Grocery Store,-88.12,";
        let result = parse_spending_transactions_csv(csv).unwrap();
        assert_eq!(result.rows[0].category, None);
    }

    #[test]
    fn category_is_none_when_no_category_column_present() {
        let csv = "Date,Description,Amount\n2026-01-05,Grocery Store,-88.12";
        let result = parse_spending_transactions_csv(csv).unwrap();
        assert_eq!(result.rows[0].category, None);
    }

    // --- raw_row_json ---

    #[test]
    fn raw_row_json_captures_original_headers_and_values_in_column_order() {
        let csv = "Date,Description,Amount\n2026-01-05,Grocery Store,-88.12";
        let result = parse_spending_transactions_csv(csv).unwrap();
        assert_eq!(
            result.rows[0].raw_row_json,
            r#"{"Date":"2026-01-05","Description":"Grocery Store","Amount":"-88.12"}"#
        );
    }

    #[test]
    fn raw_row_json_preserves_raw_unparsed_values_not_normalized_ones() {
        // The dollar sign and thousands comma are stripped by `parse_amount` for the
        // typed `amount` field, but the raw JSON should keep the CSV's own formatting.
        let csv = "Date,Description,Amount\n2026-01-05,Big Purchase,\"$1,234.56\"";
        let result = parse_spending_transactions_csv(csv).unwrap();
        assert_eq!(result.rows[0].amount, 1234.56);
        assert_eq!(
            result.rows[0].raw_row_json,
            r#"{"Date":"2026-01-05","Description":"Big Purchase","Amount":"$1,234.56"}"#
        );
    }

    // --- normalize_label ---

    #[test]
    fn normalize_label_trims_and_lowercases() {
        assert_eq!(normalize_label("  Groceries  "), "groceries");
        assert_eq!(normalize_label("GROCERIES"), "groceries");
    }

    #[test]
    fn normalize_label_of_differently_cased_labels_matches() {
        assert_eq!(normalize_label("Groceries"), normalize_label(" groceries "));
    }

    // --- manual_entry_raw_row_json ---

    #[test]
    fn manual_entry_raw_row_json_is_labeled_as_a_manual_entry() {
        let json = manual_entry_raw_row_json(
            NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(),
            "Cash tip",
            -20.0,
        );
        assert_eq!(
            json,
            r#"{"Date":"2026-01-05","Description":"Cash tip","Amount":"-20","Source":"Manual entry"}"#
        );
    }

    #[test]
    fn falls_back_to_debit_credit_columns_when_no_amount_column() {
        let csv = "Date,Description,Debit,Credit\n2026-01-10,Electric Bill,120.00,\n2026-01-11,Refund,,50.00";
        let result = parse_spending_transactions_csv(csv).unwrap();
        assert_eq!(result.rows.len(), 2);
        assert_eq!(result.rows[0].amount, -120.00);
        assert_eq!(result.rows[1].amount, 50.00);
    }

    #[test]
    fn malformed_row_is_skipped_not_fatal_bad_date() {
        let csv = "Date,Description,Amount\nnot-a-date,Mystery,10.00\n2026-01-05,Coffee,-4.50";
        let result = parse_spending_transactions_csv(csv).unwrap();
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.skipped.len(), 1);
        assert_eq!(result.skipped[0].row_number, 1);
    }

    #[test]
    fn malformed_row_is_skipped_not_fatal_non_numeric_amount() {
        let csv =
            "Date,Description,Amount\n2026-01-05,Coffee,not-a-number\n2026-01-06,Paycheck,2000.00";
        let result = parse_spending_transactions_csv(csv).unwrap();
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.skipped.len(), 1);
    }

    #[test]
    fn dollar_and_comma_amounts_parse() {
        let csv = "Date,Description,Amount\n2026-01-05,Big Purchase,\"$1,234.56\"";
        let result = parse_spending_transactions_csv(csv).unwrap();
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].amount, 1234.56);
    }

    #[test]
    fn parenthesized_amount_parses_as_negative() {
        let csv = "Date,Description,Amount\n2026-01-05,Card Purchase,(123.45)";
        let result = parse_spending_transactions_csv(csv).unwrap();
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].amount, -123.45);
    }

    #[test]
    fn no_date_column_at_all_is_a_hard_error() {
        let csv = "Description,Amount\nCoffee,-4.50";
        let result = parse_spending_transactions_csv(csv);
        assert!(matches!(result, Err(AppError::BadRequest(_))));
    }

    #[test]
    fn empty_header_only_csv_is_a_hard_error() {
        let csv = "Date,Description,Amount\n";
        let result = parse_spending_transactions_csv(csv);
        assert!(matches!(result, Err(AppError::BadRequest(_))));
    }

    // --- normalize_description ---

    #[test]
    fn normalize_description_lowercases_trims_and_collapses_whitespace() {
        assert_eq!(
            normalize_description("  Coffee   SHOP  Downtown  "),
            "coffee shop downtown"
        );
    }

    #[test]
    fn normalize_description_of_equivalent_strings_matches() {
        assert_eq!(
            normalize_description("Whole Foods #123"),
            normalize_description("whole   foods #123")
        );
    }

    // --- compute_dedupe_key ---

    #[test]
    fn compute_dedupe_key_is_deterministic() {
        let date = NaiveDate::from_ymd_opt(2026, 1, 5).unwrap();
        let a = compute_dedupe_key(2026, 1, date, "Coffee Shop", -4.50);
        let b = compute_dedupe_key(2026, 1, date, "Coffee Shop", -4.50);
        assert_eq!(a, b);
    }

    #[test]
    fn compute_dedupe_key_differs_on_amount() {
        let date = NaiveDate::from_ymd_opt(2026, 1, 5).unwrap();
        let a = compute_dedupe_key(2026, 1, date, "Coffee Shop", -4.50);
        let b = compute_dedupe_key(2026, 1, date, "Coffee Shop", -5.00);
        assert_ne!(a, b);
    }

    #[test]
    fn compute_dedupe_key_differs_on_description() {
        let date = NaiveDate::from_ymd_opt(2026, 1, 5).unwrap();
        let a = compute_dedupe_key(2026, 1, date, "Coffee Shop", -4.50);
        let b = compute_dedupe_key(2026, 1, date, "Tea Shop", -4.50);
        assert_ne!(a, b);
    }

    // --- validate_unique_category_name ---

    #[test]
    fn rejects_case_insensitive_collision() {
        let existing = vec!["Housing".to_string(), "Food".to_string()];
        let result = validate_unique_category_name("housing", &existing);
        assert!(matches!(result, Err(AppError::BadRequest(_))));
    }

    #[test]
    fn rejects_collision_with_surrounding_whitespace() {
        let existing = vec!["Housing".to_string()];
        let result = validate_unique_category_name("  Housing  ", &existing);
        assert!(matches!(result, Err(AppError::BadRequest(_))));
    }

    #[test]
    fn allows_distinct_names() {
        let existing = vec!["Housing".to_string(), "Food".to_string()];
        let result = validate_unique_category_name("Travel", &existing);
        assert!(result.is_ok());
    }

    // --- months_of_quarter ---

    #[test]
    fn months_of_quarter_covers_all_four_quarters() {
        assert_eq!(months_of_quarter(1).unwrap(), [1, 2, 3]);
        assert_eq!(months_of_quarter(2).unwrap(), [4, 5, 6]);
        assert_eq!(months_of_quarter(3).unwrap(), [7, 8, 9]);
        assert_eq!(months_of_quarter(4).unwrap(), [10, 11, 12]);
    }

    #[test]
    fn months_of_quarter_zero_is_an_error() {
        assert!(matches!(months_of_quarter(0), Err(AppError::BadRequest(_))));
    }

    #[test]
    fn months_of_quarter_five_is_an_error() {
        assert!(matches!(months_of_quarter(5), Err(AppError::BadRequest(_))));
    }

    // --- summarize_quarter ---

    #[test]
    fn summarize_quarter_sums_income_and_expense_by_kind() {
        let rows = vec![
            MonthKindTotal {
                year: 2026,
                month: 1,
                kind: Some(SpendingTrackerCategoryKind::Income),
                amount: 2000.0,
            },
            MonthKindTotal {
                year: 2026,
                month: 1,
                kind: Some(SpendingTrackerCategoryKind::Expense),
                amount: -500.0,
            },
            MonthKindTotal {
                year: 2026,
                month: 2,
                kind: Some(SpendingTrackerCategoryKind::Expense),
                amount: -300.0,
            },
            MonthKindTotal {
                year: 2026,
                month: 3,
                kind: Some(SpendingTrackerCategoryKind::Income),
                amount: 1000.0,
            },
        ];
        let summary = summarize_quarter(2026, 1, &rows).unwrap();
        assert_eq!(summary.income_total, 3000.0);
        assert_eq!(summary.expense_total, 800.0);
    }

    #[test]
    fn summarize_quarter_excludes_ignore_kind_from_totals() {
        let rows = vec![
            MonthKindTotal {
                year: 2026,
                month: 1,
                kind: Some(SpendingTrackerCategoryKind::Ignore),
                amount: -9999.0,
            },
            MonthKindTotal {
                year: 2026,
                month: 1,
                kind: Some(SpendingTrackerCategoryKind::Income),
                amount: 100.0,
            },
        ];
        let summary = summarize_quarter(2026, 1, &rows).unwrap();
        assert_eq!(summary.income_total, 100.0);
        assert_eq!(summary.expense_total, 0.0);
    }

    #[test]
    fn summarize_quarter_excludes_uncategorized_from_totals() {
        let rows = vec![
            MonthKindTotal {
                year: 2026,
                month: 1,
                kind: None,
                amount: -250.0,
            },
            MonthKindTotal {
                year: 2026,
                month: 1,
                kind: Some(SpendingTrackerCategoryKind::Income),
                amount: 100.0,
            },
        ];
        let summary = summarize_quarter(2026, 1, &rows).unwrap();
        assert_eq!(summary.income_total, 100.0);
        assert_eq!(summary.expense_total, 0.0);
    }

    #[test]
    fn summarize_quarter_has_data_true_for_month_with_only_uncategorized_rows() {
        let rows = vec![MonthKindTotal {
            year: 2026,
            month: 1,
            kind: None,
            amount: -10.0,
        }];
        let summary = summarize_quarter(2026, 1, &rows).unwrap();
        let jan = summary.months.iter().find(|m| m.month == 1).unwrap();
        assert!(jan.has_data);
    }

    #[test]
    fn summarize_quarter_partial_coverage_month_with_zero_rows_has_no_data_and_zero_totals() {
        // Only January (of Q1: Jan/Feb/Mar) has any rows.
        let rows = vec![MonthKindTotal {
            year: 2026,
            month: 1,
            kind: Some(SpendingTrackerCategoryKind::Income),
            amount: 500.0,
        }];
        let summary = summarize_quarter(2026, 1, &rows).unwrap();
        assert_eq!(summary.months.len(), 3);
        let jan = summary.months.iter().find(|m| m.month == 1).unwrap();
        let feb = summary.months.iter().find(|m| m.month == 2).unwrap();
        let mar = summary.months.iter().find(|m| m.month == 3).unwrap();
        assert!(jan.has_data);
        assert!(!feb.has_data);
        assert!(!mar.has_data);
        assert_eq!(feb.income_total, 0.0);
        assert_eq!(feb.expense_total, 0.0);
        assert_eq!(mar.income_total, 0.0);
        assert_eq!(mar.expense_total, 0.0);
    }

    #[test]
    fn summarize_quarter_invalid_quarter_errors() {
        let result = summarize_quarter(2026, 5, &[]);
        assert!(matches!(result, Err(AppError::BadRequest(_))));
    }

    // --- summarize_year ---

    #[test]
    fn summarize_year_sums_expenses_across_all_twelve_months() {
        let rows = vec![
            MonthKindTotal {
                year: 2026,
                month: 1,
                kind: Some(SpendingTrackerCategoryKind::Expense),
                amount: -500.0,
            },
            MonthKindTotal {
                year: 2026,
                month: 6,
                kind: Some(SpendingTrackerCategoryKind::Expense),
                amount: -300.0,
            },
            MonthKindTotal {
                year: 2026,
                month: 12,
                kind: Some(SpendingTrackerCategoryKind::Income),
                amount: 1000.0,
            },
        ];
        let summary = summarize_year(2026, &rows);
        assert_eq!(summary.months.len(), 12);
        assert_eq!(summary.expense_total, 800.0);
        assert_eq!(summary.income_total, 1000.0);
        assert_eq!(summary.months[0].expense_total, 500.0);
        assert_eq!(summary.months[5].expense_total, 300.0);
        assert_eq!(summary.months[11].income_total, 1000.0);
    }

    #[test]
    fn summarize_year_excludes_rows_from_other_years() {
        let rows = vec![MonthKindTotal {
            year: 2025,
            month: 1,
            kind: Some(SpendingTrackerCategoryKind::Expense),
            amount: -500.0,
        }];
        let summary = summarize_year(2026, &rows);
        assert_eq!(summary.expense_total, 0.0);
        assert!(!summary.months[0].has_data);
    }

    // --- expense_category_breakdown ---

    #[test]
    fn expense_category_breakdown_groups_by_category_and_month() {
        let rows = vec![
            CategoryMonthRow {
                month: 1,
                category_id: "food".into(),
                category_name: "Food".into(),
                kind: SpendingTrackerCategoryKind::Expense,
                amount: -100.0,
            },
            CategoryMonthRow {
                month: 2,
                category_id: "food".into(),
                category_name: "Food".into(),
                kind: SpendingTrackerCategoryKind::Expense,
                amount: -50.0,
            },
            CategoryMonthRow {
                month: 1,
                category_id: "housing".into(),
                category_name: "Housing".into(),
                kind: SpendingTrackerCategoryKind::Expense,
                amount: -1000.0,
            },
        ];
        let result = expense_category_breakdown(&rows);
        assert_eq!(result.len(), 2);
        // Sorted by full-year total, descending.
        assert_eq!(result[0].category_name, "Housing");
        assert_eq!(result[0].monthly_totals[0], 1000.0);
        assert_eq!(result[1].category_name, "Food");
        assert_eq!(result[1].monthly_totals[0], 100.0);
        assert_eq!(result[1].monthly_totals[1], 50.0);
        assert_eq!(result[1].monthly_totals[2], 0.0);
    }

    #[test]
    fn expense_category_breakdown_excludes_non_expense_kinds() {
        let rows = vec![
            CategoryMonthRow {
                month: 1,
                category_id: "salary".into(),
                category_name: "Salary".into(),
                kind: SpendingTrackerCategoryKind::Income,
                amount: 3000.0,
            },
            CategoryMonthRow {
                month: 1,
                category_id: "misc".into(),
                category_name: "Misc".into(),
                kind: SpendingTrackerCategoryKind::Ignore,
                amount: -20.0,
            },
        ];
        assert!(expense_category_breakdown(&rows).is_empty());
    }

    // --- category_visible_to_user / category_editable_by_user ---

    fn sample_category(
        id: &str,
        user_id: Option<&str>,
        is_predefined: bool,
    ) -> SpendingTrackerCategory {
        let now: NaiveDateTime = "2026-01-01T00:00:00"
            .parse()
            .expect("valid fixed timestamp");
        SpendingTrackerCategory {
            id: id.to_string(),
            user_id: user_id.map(|s| s.to_string()),
            name: "Sample".to_string(),
            kind: SpendingTrackerCategoryKind::Expense.as_str().to_string(),
            is_predefined,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn predefined_category_is_visible_but_not_editable() {
        let cat = sample_category("c1", None, true);
        assert!(category_visible_to_user(Some(&cat), "user-1"));
        assert!(!category_editable_by_user(&cat, "user-1"));
    }

    #[test]
    fn own_custom_category_is_visible_and_editable() {
        let cat = sample_category("c1", Some("user-1"), false);
        assert!(category_visible_to_user(Some(&cat), "user-1"));
        assert!(category_editable_by_user(&cat, "user-1"));
    }

    #[test]
    fn other_users_custom_category_is_neither_visible_nor_editable() {
        let cat = sample_category("c1", Some("user-2"), false);
        assert!(!category_visible_to_user(Some(&cat), "user-1"));
        assert!(!category_editable_by_user(&cat, "user-1"));
    }

    #[test]
    fn missing_category_is_not_visible() {
        assert!(!category_visible_to_user(None, "user-1"));
    }

    // --- best_guess_category ---

    fn named_category(id: &str, name: &str) -> SpendingTrackerCategory {
        let mut cat = sample_category(id, None, true);
        cat.name = name.to_string();
        cat
    }

    #[test]
    fn guesses_a_category_sharing_a_singular_plural_token() {
        let categories = vec![named_category("c1", "Pets"), named_category("c2", "Food")];
        let guess = best_guess_category("Pet Supplies", &categories);
        assert_eq!(guess.map(|c| c.id.as_str()), Some("c1"));
    }

    #[test]
    fn guesses_a_category_sharing_a_whole_word_token() {
        let categories = vec![
            named_category("c1", "Entertainment"),
            named_category("c2", "Food"),
        ];
        let guess = best_guess_category("Entertainment & Streaming", &categories);
        assert_eq!(guess.map(|c| c.id.as_str()), Some("c1"));
    }

    #[test]
    fn returns_none_when_no_category_shares_any_token() {
        let categories = vec![named_category("c1", "Food"), named_category("c2", "Housing")];
        let guess = best_guess_category("Gas & Fuel", &categories);
        assert!(guess.is_none());
    }

    #[test]
    fn ties_are_broken_by_shorter_name_then_alphabetically() {
        let categories = vec![
            named_category("c1", "Pet Care"),
            named_category("c2", "Pets"),
        ];
        // Both "Pet Care" and "Pets" share the "pet" token with the label;
        // the shorter name ("Pets") should win the tie.
        let guess = best_guess_category("Pet Store", &categories);
        assert_eq!(guess.map(|c| c.id.as_str()), Some("c2"));
    }
}
