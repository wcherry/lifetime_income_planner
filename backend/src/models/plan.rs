use chrono::{NaiveDate, NaiveDateTime};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::models::{
    Account, Assumptions, IncomeSource, LifeEvent, NewAccount, NewAssumptions, NewIncomeSource,
    NewLifeEvent, NewSpendingItem, Profile, ProfileChangeset, SpendingItem,
};
use crate::schema::plans;

/// Persisted saved-plan row. `snapshot` holds a JSON [`PlanSnapshot`].
#[derive(Queryable, Selectable, Identifiable, Debug, Clone)]
#[diesel(table_name = plans)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct Plan {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub snapshot: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Insertable)]
#[diesel(table_name = plans)]
pub struct NewPlan {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub snapshot: String,
    pub updated_at: NaiveDateTime,
}

// --- Snapshot document -----------------------------------------------------
//
// Each sub-struct mirrors the *storage* columns of its resource (enums kept as
// their stored strings), minus the id/user_id/timestamp bookkeeping. The data
// was already validated when it was first created, so loading it back does not
// need to re-validate — only to re-key it with fresh ids for the target user.

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProfileSnapshot {
    pub first_name: String,
    pub last_name: String,
    pub date_of_birth: NaiveDate,
    pub marital_status: String,
    pub filing_status: String,
    pub state: String,
    pub retirement_date: NaiveDate,
    pub life_expectancy: i32,
    pub spouse_first_name: Option<String>,
    pub spouse_last_name: Option<String>,
    pub spouse_date_of_birth: Option<NaiveDate>,
    pub spouse_life_expectancy: Option<i32>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AssumptionsSnapshot {
    pub inflation_rate: f64,
    pub investment_return_rate: f64,
    pub healthcare_inflation_rate: f64,
    pub social_security_cola_rate: f64,
    // Roth conversion strategy (feature 6). Defaulted so plans saved before the
    // feature existed still load.
    #[serde(default)]
    pub roth_conversion_ceiling: f64,
    #[serde(default)]
    pub roth_conversion_start_year: Option<i32>,
    #[serde(default)]
    pub roth_conversion_end_year: Option<i32>,
    // Withdrawal sequencing strategy (feature 9). Defaulted so plans saved
    // before the feature existed still load.
    #[serde(default)]
    pub withdrawal_strategy: String,
    // ACA benchmark premium (Phase 3, feature 1). Defaulted for plans saved
    // before the feature existed.
    #[serde(default)]
    pub aca_benchmark_annual_premium: f64,
    // Medicare Part B premium (Phase 3, feature 3). Defaulted for plans saved
    // before the feature existed.
    #[serde(default)]
    pub medicare_part_b_annual_premium: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AccountSnapshot {
    pub name: String,
    pub category: String,
    pub account_type: String,
    pub owner: String,
    pub current_balance: f64,
    pub expected_roi: f64,
    pub dividend_yield: f64,
    pub cost_basis: Option<f64>,
    pub allocation_stock_pct: Option<i32>,
    pub allocation_bond_pct: Option<i32>,
    pub allocation_cash_pct: Option<i32>,
    pub withdrawal_restrictions: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IncomeSnapshot {
    pub name: String,
    pub income_type: String,
    pub owner: String,
    pub amount: f64,
    pub frequency: String,
    pub start_date: NaiveDate,
    pub end_date: Option<NaiveDate>,
    pub growth_rate: f64,
    pub cola: bool,
    pub taxability: String,
    pub notes: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SpendingSnapshot {
    pub name: String,
    pub category: String,
    pub amount: f64,
    pub frequency: String,
    pub inflation_adjusted: bool,
    pub start_year: Option<i32>,
    pub end_year: Option<i32>,
    pub notes: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LifeEventSnapshot {
    pub name: String,
    pub event_type: String,
    pub event_date: NaiveDate,
    pub direction: String,
    pub amount: f64,
    pub taxable: bool,
    pub inflation_adjusted: bool,
    pub recurrence: String,
    pub end_date: Option<NaiveDate>,
    pub notes: Option<String>,
}

/// The full working set captured in a saved plan.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct PlanSnapshot {
    pub profile: Option<ProfileSnapshot>,
    pub assumptions: Option<AssumptionsSnapshot>,
    pub accounts: Vec<AccountSnapshot>,
    pub income: Vec<IncomeSnapshot>,
    pub spending: Vec<SpendingSnapshot>,
    pub life_events: Vec<LifeEventSnapshot>,
}

impl PlanSnapshot {
    /// Capture a snapshot from the user's current rows.
    pub fn capture(
        profile: Option<&Profile>,
        assumptions: Option<&Assumptions>,
        accounts: &[Account],
        income: &[IncomeSource],
        spending: &[SpendingItem],
        life_events: &[LifeEvent],
    ) -> Self {
        PlanSnapshot {
            profile: profile.map(ProfileSnapshot::from),
            assumptions: assumptions.map(AssumptionsSnapshot::from),
            accounts: accounts.iter().map(AccountSnapshot::from).collect(),
            income: income.iter().map(IncomeSnapshot::from).collect(),
            spending: spending.iter().map(SpendingSnapshot::from).collect(),
            life_events: life_events.iter().map(LifeEventSnapshot::from).collect(),
        }
    }

    /// Lightweight counts for list/preview responses.
    pub fn contents(&self) -> PlanContents {
        PlanContents {
            has_profile: self.profile.is_some(),
            has_assumptions: self.assumptions.is_some(),
            accounts: self.accounts.len(),
            income: self.income.len(),
            spending: self.spending.len(),
            life_events: self.life_events.len(),
        }
    }
}

// --- Row -> snapshot conversions ------------------------------------------

impl From<&Profile> for ProfileSnapshot {
    fn from(p: &Profile) -> Self {
        ProfileSnapshot {
            first_name: p.first_name.clone(),
            last_name: p.last_name.clone(),
            date_of_birth: p.date_of_birth,
            marital_status: p.marital_status.clone(),
            filing_status: p.filing_status.clone(),
            state: p.state.clone(),
            retirement_date: p.retirement_date,
            life_expectancy: p.life_expectancy,
            spouse_first_name: p.spouse_first_name.clone(),
            spouse_last_name: p.spouse_last_name.clone(),
            spouse_date_of_birth: p.spouse_date_of_birth,
            spouse_life_expectancy: p.spouse_life_expectancy,
        }
    }
}

impl From<&Assumptions> for AssumptionsSnapshot {
    fn from(a: &Assumptions) -> Self {
        AssumptionsSnapshot {
            inflation_rate: a.inflation_rate,
            investment_return_rate: a.investment_return_rate,
            healthcare_inflation_rate: a.healthcare_inflation_rate,
            social_security_cola_rate: a.social_security_cola_rate,
            roth_conversion_ceiling: a.roth_conversion_ceiling,
            roth_conversion_start_year: a.roth_conversion_start_year,
            roth_conversion_end_year: a.roth_conversion_end_year,
            withdrawal_strategy: a.withdrawal_strategy.clone(),
            aca_benchmark_annual_premium: a.aca_benchmark_annual_premium,
            medicare_part_b_annual_premium: a.medicare_part_b_annual_premium,
        }
    }
}

impl From<&Account> for AccountSnapshot {
    fn from(a: &Account) -> Self {
        AccountSnapshot {
            name: a.name.clone(),
            category: a.category.clone(),
            account_type: a.account_type.clone(),
            owner: a.owner.clone(),
            current_balance: a.current_balance,
            expected_roi: a.expected_roi,
            dividend_yield: a.dividend_yield,
            cost_basis: a.cost_basis,
            allocation_stock_pct: a.allocation_stock_pct,
            allocation_bond_pct: a.allocation_bond_pct,
            allocation_cash_pct: a.allocation_cash_pct,
            withdrawal_restrictions: a.withdrawal_restrictions.clone(),
        }
    }
}

impl From<&IncomeSource> for IncomeSnapshot {
    fn from(i: &IncomeSource) -> Self {
        IncomeSnapshot {
            name: i.name.clone(),
            income_type: i.income_type.clone(),
            owner: i.owner.clone(),
            amount: i.amount,
            frequency: i.frequency.clone(),
            start_date: i.start_date,
            end_date: i.end_date,
            growth_rate: i.growth_rate,
            cola: i.cola,
            taxability: i.taxability.clone(),
            notes: i.notes.clone(),
        }
    }
}

impl From<&SpendingItem> for SpendingSnapshot {
    fn from(s: &SpendingItem) -> Self {
        SpendingSnapshot {
            name: s.name.clone(),
            category: s.category.clone(),
            amount: s.amount,
            frequency: s.frequency.clone(),
            inflation_adjusted: s.inflation_adjusted,
            start_year: s.start_year,
            end_year: s.end_year,
            notes: s.notes.clone(),
        }
    }
}

impl From<&LifeEvent> for LifeEventSnapshot {
    fn from(e: &LifeEvent) -> Self {
        LifeEventSnapshot {
            name: e.name.clone(),
            event_type: e.event_type.clone(),
            event_date: e.event_date,
            direction: e.direction.clone(),
            amount: e.amount,
            taxable: e.taxable,
            inflation_adjusted: e.inflation_adjusted,
            recurrence: e.recurrence.clone(),
            end_date: e.end_date,
            notes: e.notes.clone(),
        }
    }
}

// --- Snapshot -> insertable row conversions --------------------------------

impl ProfileSnapshot {
    pub fn into_changeset(&self, id: String, user_id: String, now: NaiveDateTime) -> ProfileChangeset {
        ProfileChangeset {
            id,
            user_id,
            first_name: self.first_name.clone(),
            last_name: self.last_name.clone(),
            date_of_birth: self.date_of_birth,
            marital_status: self.marital_status.clone(),
            filing_status: self.filing_status.clone(),
            state: self.state.clone(),
            retirement_date: self.retirement_date,
            life_expectancy: self.life_expectancy,
            spouse_first_name: self.spouse_first_name.clone(),
            spouse_last_name: self.spouse_last_name.clone(),
            spouse_date_of_birth: self.spouse_date_of_birth,
            spouse_life_expectancy: self.spouse_life_expectancy,
            updated_at: now,
        }
    }
}

impl AssumptionsSnapshot {
    pub fn into_new(&self, id: String, user_id: String, now: NaiveDateTime) -> NewAssumptions {
        NewAssumptions {
            id,
            user_id,
            inflation_rate: self.inflation_rate,
            investment_return_rate: self.investment_return_rate,
            healthcare_inflation_rate: self.healthcare_inflation_rate,
            social_security_cola_rate: self.social_security_cola_rate,
            updated_at: now,
            roth_conversion_ceiling: self.roth_conversion_ceiling,
            roth_conversion_start_year: self.roth_conversion_start_year,
            roth_conversion_end_year: self.roth_conversion_end_year,
            withdrawal_strategy: self.withdrawal_strategy.clone(),
            aca_benchmark_annual_premium: self.aca_benchmark_annual_premium,
            medicare_part_b_annual_premium: self.medicare_part_b_annual_premium,
        }
    }
}

impl AccountSnapshot {
    pub fn into_new(&self, id: String, user_id: String, now: NaiveDateTime) -> NewAccount {
        NewAccount {
            id,
            user_id,
            name: self.name.clone(),
            category: self.category.clone(),
            account_type: self.account_type.clone(),
            owner: self.owner.clone(),
            current_balance: self.current_balance,
            expected_roi: self.expected_roi,
            dividend_yield: self.dividend_yield,
            cost_basis: self.cost_basis,
            allocation_stock_pct: self.allocation_stock_pct,
            allocation_bond_pct: self.allocation_bond_pct,
            allocation_cash_pct: self.allocation_cash_pct,
            withdrawal_restrictions: self.withdrawal_restrictions.clone(),
            updated_at: now,
        }
    }
}

impl IncomeSnapshot {
    pub fn into_new(&self, id: String, user_id: String, now: NaiveDateTime) -> NewIncomeSource {
        NewIncomeSource {
            id,
            user_id,
            name: self.name.clone(),
            income_type: self.income_type.clone(),
            owner: self.owner.clone(),
            amount: self.amount,
            frequency: self.frequency.clone(),
            start_date: self.start_date,
            end_date: self.end_date,
            growth_rate: self.growth_rate,
            cola: self.cola,
            taxability: self.taxability.clone(),
            notes: self.notes.clone(),
            updated_at: now,
        }
    }
}

impl SpendingSnapshot {
    pub fn into_new(&self, id: String, user_id: String, now: NaiveDateTime) -> NewSpendingItem {
        NewSpendingItem {
            id,
            user_id,
            name: self.name.clone(),
            category: self.category.clone(),
            amount: self.amount,
            frequency: self.frequency.clone(),
            inflation_adjusted: self.inflation_adjusted,
            start_year: self.start_year,
            end_year: self.end_year,
            notes: self.notes.clone(),
            updated_at: now,
        }
    }
}

impl LifeEventSnapshot {
    pub fn into_new(&self, id: String, user_id: String, now: NaiveDateTime) -> NewLifeEvent {
        NewLifeEvent {
            id,
            user_id,
            name: self.name.clone(),
            event_type: self.event_type.clone(),
            event_date: self.event_date,
            direction: self.direction.clone(),
            amount: self.amount,
            taxable: self.taxable,
            inflation_adjusted: self.inflation_adjusted,
            recurrence: self.recurrence.clone(),
            end_date: self.end_date,
            notes: self.notes.clone(),
            updated_at: now,
        }
    }
}

// --- API DTOs --------------------------------------------------------------

/// Request body for saving the current working set as a plan, or renaming one.
#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct SavePlanRequest {
    #[validate(length(min = 1, max = 120, message = "is required"))]
    #[schema(example = "Baseline")]
    pub name: String,
}

/// Count summary of what a saved plan contains.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PlanContents {
    pub has_profile: bool,
    pub has_assumptions: bool,
    pub accounts: usize,
    pub income: usize,
    pub spending: usize,
    pub life_events: usize,
}

/// API view of a saved plan (metadata plus a lightweight content summary).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PlanResponse {
    pub id: String,
    pub name: String,
    pub contents: PlanContents,
    #[schema(value_type = String, format = DateTime)]
    pub created_at: NaiveDateTime,
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: NaiveDateTime,
}

impl PlanResponse {
    /// Build a response, parsing the stored snapshot for its content counts.
    /// A snapshot that fails to parse yields empty counts rather than an error.
    pub fn from_row(p: &Plan) -> Self {
        let contents = serde_json::from_str::<PlanSnapshot>(&p.snapshot)
            .map(|s| s.contents())
            .unwrap_or(PlanContents {
                has_profile: false,
                has_assumptions: false,
                accounts: 0,
                income: 0,
                spending: 0,
                life_events: 0,
            });
        PlanResponse {
            id: p.id.clone(),
            name: p.name.clone(),
            contents,
            created_at: p.created_at,
            updated_at: p.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, NaiveDateTime};

    fn ts() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
    }

    fn sample_account() -> Account {
        Account {
            id: "acct-1".into(),
            user_id: "u1".into(),
            name: "Brokerage".into(),
            category: "taxable".into(),
            account_type: "brokerage".into(),
            owner: "self".into(),
            current_balance: 250_000.0,
            expected_roi: 6.5,
            dividend_yield: 1.8,
            cost_basis: Some(200_000.0),
            allocation_stock_pct: Some(60),
            allocation_bond_pct: Some(30),
            allocation_cash_pct: Some(10),
            withdrawal_restrictions: Some("none".into()),
            created_at: ts(),
            updated_at: ts(),
        }
    }

    #[test]
    fn snapshot_round_trips_through_json_and_back_to_a_new_row() {
        let accounts = vec![sample_account()];
        let snapshot = PlanSnapshot::capture(None, None, &accounts, &[], &[], &[]);

        // Serialize and parse back, as the load path does.
        let json = serde_json::to_string(&snapshot).unwrap();
        let restored: PlanSnapshot = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.accounts.len(), 1);
        let new_row = restored.accounts[0].into_new("new-id".into(), "u2".into(), ts());

        // Fresh identity, preserved data.
        assert_eq!(new_row.id, "new-id");
        assert_eq!(new_row.user_id, "u2");
        assert_eq!(new_row.name, "Brokerage");
        assert_eq!(new_row.category, "taxable");
        assert_eq!(new_row.current_balance, 250_000.0);
        assert_eq!(new_row.expected_roi, 6.5);
        assert_eq!(new_row.cost_basis, Some(200_000.0));
        assert_eq!(new_row.allocation_stock_pct, Some(60));
        assert_eq!(new_row.withdrawal_restrictions.as_deref(), Some("none"));
    }

    #[test]
    fn contents_counts_reflect_the_snapshot() {
        let accounts = vec![sample_account(), sample_account()];
        let snapshot = PlanSnapshot::capture(None, None, &accounts, &[], &[], &[]);
        let contents = snapshot.contents();
        assert_eq!(contents.accounts, 2);
        assert_eq!(contents.income, 0);
        assert!(!contents.has_profile);
        assert!(!contents.has_assumptions);
    }
}
