use serde::Serialize;
use utoipa::ToSchema;

/// The planning assumptions actually used to build a projection, echoed back so
/// the client can show what drove the numbers.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ProjectionAssumptions {
    pub inflation_rate: f64,
    pub investment_return_rate: f64,
    pub healthcare_inflation_rate: f64,
    pub social_security_cola_rate: f64,
    /// Roth conversion ceiling driving the strategy (feature 6); 0 when off.
    pub roth_conversion_ceiling: f64,
    /// Optional year window the conversion strategy runs over.
    pub roth_conversion_start_year: Option<i32>,
    pub roth_conversion_end_year: Option<i32>,
    /// Withdrawal sequencing strategy driving the drawdown order (Phase 2,
    /// feature 9): `"conventional"` or `"tax_optimized"`.
    pub withdrawal_strategy: String,
    /// ACA benchmark (SLCSP) annual premium driving subsidy modeling
    /// (Phase 3, feature 1); 0 when off.
    pub aca_benchmark_annual_premium: f64,
    /// Medicare Part B annual premium per enrolled household member driving
    /// automatic post-65 cost modeling (Phase 3, feature 3); 0 when off.
    pub medicare_part_b_annual_premium: f64,
    /// True when the user has not saved assumptions and defaults were used.
    pub is_default: bool,
}

/// Headline figures summarizing the whole projection.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ProjectionSummary {
    /// Sum of all account balances today, before any growth.
    pub current_net_worth: f64,
    /// Total balance across all accounts at the end of the plan (the estate).
    pub projected_ending_balance: f64,
    pub total_lifetime_income: f64,
    pub total_lifetime_spending: f64,
    pub total_lifetime_withdrawals: f64,
    /// Total tax paid across the whole plan (federal + state).
    pub total_lifetime_taxes: f64,
    /// Federal portion of lifetime taxes.
    pub total_lifetime_federal_taxes: f64,
    /// State portion of lifetime taxes.
    pub total_lifetime_state_taxes: f64,
    /// Total dollars converted from tax-deferred to Roth over the plan (feature 6).
    pub total_lifetime_roth_conversions: f64,
    /// Total ACA premium tax credit received over the plan (Phase 3, feature 1).
    pub total_lifetime_aca_subsidies: f64,
    /// Total Medicare Part B premiums paid over the plan (Phase 3, feature 3).
    pub total_lifetime_medicare_premiums: f64,
    /// Total Medicare IRMAA surcharges (Part B + Part D) paid over the plan
    /// (Phase 3, feature 4).
    pub total_lifetime_irmaa_surcharges: f64,
    /// First year in which spending could not be fully funded, if any.
    pub depletion_year: Option<i32>,
}

/// One year's tax detail (roadmap Phase 2, features 1–5). All amounts are
/// annual dollars.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct YearTax {
    /// Ordinary taxable income excluding Social Security (income sources,
    /// tax-deferred withdrawals, taxable one-off inflows).
    pub ordinary_income: f64,
    /// Qualified dividends from taxable accounts (preferential rates).
    pub qualified_dividends: f64,
    /// Realized long-term capital gains from taxable-account withdrawals.
    pub capital_gains: f64,
    /// Gross Social Security benefits received this year.
    pub social_security_benefits: f64,
    /// Portion of Social Security benefits that is taxable.
    pub taxable_social_security: f64,
    /// Adjusted gross income (includes taxable Social Security).
    pub adjusted_gross_income: f64,
    /// Modified Adjusted Gross Income (roadmap Phase 3, feature 2): AGI plus
    /// the untaxed portion of Social Security benefits. Tracked every year —
    /// regardless of ACA eligibility — so MAGI-driven thresholds (ACA
    /// subsidies, and Medicare IRMAA surcharges in a later phase) can be
    /// forecast across the whole plan, not just the current year.
    pub magi: f64,
    /// Standard deduction applied (inflation-indexed, includes age-65 add-ons).
    pub standard_deduction: f64,
    /// Taxable income after the standard deduction.
    pub taxable_income: f64,
    /// Federal tax on ordinary income.
    pub federal_ordinary_tax: f64,
    /// Federal tax on qualified dividends and long-term capital gains.
    pub federal_capital_gains_tax: f64,
    pub federal_tax: f64,
    /// State taxable income (the state's own base, after its own deduction).
    pub state_taxable_income: f64,
    /// State standard deduction applied.
    pub state_standard_deduction: f64,
    pub state_tax: f64,
    /// State marginal tax rate (fraction) at the top of state taxable income.
    pub state_marginal_rate: f64,
    /// Property tax for the year. Reserved for a later milestone; currently 0.
    pub property_tax: f64,
    pub total_tax: f64,
    /// Total (federal + state) tax as a fraction of gross income.
    pub effective_rate: f64,
    /// Federal ordinary marginal tax rate (fraction) at the top of taxable income.
    pub marginal_rate: f64,
}

/// One year's ACA premium tax credit detail (roadmap Phase 3, feature 1). All
/// amounts are annual dollars.
#[derive(Debug, Clone, Serialize, ToSchema, Default)]
pub struct YearAca {
    /// Whether the household qualifies for a premium tax credit this year (a
    /// benchmark premium is set, the household is pre-Medicare, and income is at
    /// or above the poverty line).
    pub eligible: bool,
    /// Modified Adjusted Gross Income used for the ACA determination.
    pub magi: f64,
    /// Federal Poverty Line for the household size this year (inflation-indexed).
    pub federal_poverty_line: f64,
    /// MAGI as a percentage of the poverty line (e.g. 250.0 for 250%).
    pub fpl_percent: f64,
    /// Share of MAGI the household is expected to contribute (fraction).
    pub applicable_percentage: f64,
    /// Annual dollars the household is expected to pay toward the benchmark.
    pub expected_contribution: f64,
    /// The benchmark (SLCSP) premium used this year (inflation-adjusted).
    pub benchmark_premium: f64,
    /// The premium tax credit (annual subsidy) received this year.
    pub subsidy: f64,
}

/// One year's Medicare IRMAA surcharge detail (roadmap Phase 3, feature 4).
/// All dollar amounts are annual, household totals unless noted otherwise.
#[derive(Debug, Clone, Serialize, ToSchema, Default)]
pub struct YearIrmaa {
    /// Whether any household member enrolled in Medicare this year pays a
    /// surcharge (MAGI from the lookback year was at/above the lowest tier).
    pub applies: bool,
    /// Whether two-years-prior MAGI was available (from earlier in this same
    /// projection) to determine the tier. `false` for the first two years of
    /// the plan, when no in-plan history exists yet — treated as no surcharge
    /// rather than guessed at, since actual pre-plan MAGI isn't modeled.
    pub has_lookback_data: bool,
    /// Calendar year the lookback MAGI is drawn from (`year - 2`).
    pub lookback_year: i32,
    /// The MAGI the tier was determined from (0 when lookback data wasn't available).
    pub lookback_magi: f64,
    /// This tier's Part B surcharge, per enrolled person, per month.
    pub part_b_surcharge_monthly: f64,
    /// This tier's Part D surcharge, per enrolled person, per month.
    pub part_d_surcharge_monthly: f64,
    /// Number of household members enrolled (age 65+) and paying the surcharge this year.
    pub enrolled_count: i32,
    /// Household total surcharge for the year (Part B + Part D, both enrolled members).
    pub total_surcharge: f64,
}

/// A single life event occurring within a projection year.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct LifeEventOccurrence {
    pub name: String,
    /// Signed cash flow for this year: positive for inflows, negative for outflows.
    pub amount: f64,
}

/// An age- or date-based planning milestone (not a cash flow), e.g. Medicare
/// eligibility or the start of required minimum distributions.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Milestone {
    /// Short name for the marker, e.g. "Medicare eligibility".
    pub label: String,
    /// One-line explanation for the tooltip.
    pub detail: String,
    /// Age of the relevant person when the milestone is reached.
    pub age: i32,
}

/// One calendar year of the projection.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct YearProjection {
    pub year: i32,
    pub primary_age: i32,
    pub spouse_age: Option<i32>,
    /// Total account balance at the start of the year.
    pub starting_balance: f64,
    pub income: f64,
    pub spending: f64,
    /// Net signed cash flow from life events (inflows positive).
    pub life_events_net: f64,
    /// The individual life events that occur this year (for markers/tooltips).
    pub life_events: Vec<LifeEventOccurrence>,
    /// Age/regulatory milestones reached this year (for markers/tooltips).
    pub milestones: Vec<Milestone>,
    /// Investment growth credited to accounts this year.
    pub growth: f64,
    /// Total drawn from accounts to cover spending and taxes.
    pub withdrawals: f64,
    /// Required minimum distribution due this year across the household
    /// (RMD module), based on IRS Uniform Lifetime Table divisors applied to
    /// each owner's prior year-end tax-deferred balance. 0 before RMDs begin.
    /// This amount is enforced as a floor on tax-deferred withdrawals: when
    /// spending doesn't need it all, the excess is reinvested (see
    /// `contributions`).
    pub rmd_amount: f64,
    /// Medicare Part B premiums due this year (roadmap Phase 3, feature 3):
    /// the standard premium per household member enrolled (age 65+),
    /// inflation-indexed by the healthcare inflation rate. 0 before either
    /// person turns 65 or when Medicare modeling is disabled. Treated as an
    /// automatic cash need alongside spending, ahead of any IRMAA income-based
    /// surcharge (`irmaa_surcharge`).
    pub medicare_premiums: f64,
    /// Medicare IRMAA surcharge due this year (roadmap Phase 3, feature 4):
    /// the income-based add-on to the standard Part B and Part D premiums,
    /// based on household MAGI from two years prior. 0 before Medicare
    /// modeling applies, while lookback history is unavailable, or when MAGI
    /// stays under the lowest bracket. Treated as an automatic cash need
    /// alongside `medicare_premiums`; see `irmaa` for the full breakdown.
    pub irmaa_surcharge: f64,
    /// Surplus cash reinvested into accounts.
    pub contributions: f64,
    /// Dollars converted from tax-deferred to Roth this year (feature 6). The
    /// conversion is included in `tax.ordinary_income`.
    pub roth_conversion: f64,
    /// Tax owed this year (federal + state); a subset of the cash requirement.
    pub taxes: f64,
    /// Full tax breakdown for the year (features 1–5).
    pub tax: YearTax,
    /// Which category was drawn from first to cover this year's cash need
    /// (Phase 2, feature 9): `"taxable_first"` (the conventional default) or
    /// `"tax_deferred_first"` (the tax-optimized strategy swapped the order
    /// because realizing this year's taxable gains would have cost more at
    /// the margin than an equivalent ordinary withdrawal).
    pub withdrawal_order: String,
    /// ACA premium tax credit detail for the year (Phase 3, feature 1). The
    /// subsidy is a tax-free cash inflow that reduces the year's withdrawal need.
    pub aca: YearAca,
    /// Medicare IRMAA surcharge detail for the year (Phase 3, feature 4).
    pub irmaa: YearIrmaa,
    /// Total account balance at the end of the year.
    pub ending_balance: f64,
    /// Spending (or taxes) that could not be funded because accounts were exhausted.
    pub shortfall: f64,
}

/// A single account's withdrawal within a quarter of the near-term schedule.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct QuarterWithdrawal {
    pub account_id: String,
    pub account_name: String,
    pub category: String,
    pub amount: f64,
}

/// One quarter of the actionable near-term withdrawal schedule.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct QuarterProjection {
    /// Human-friendly label, e.g. "2026 Q1".
    pub label: String,
    pub year: i32,
    pub quarter: i32,
    pub income: f64,
    pub spending: f64,
    /// Estimated tax for the quarter (this year's tax split evenly).
    pub estimated_tax: f64,
    pub total_withdrawal: f64,
    pub withdrawals: Vec<QuarterWithdrawal>,
}

/// One estimated tax payment voucher (roadmap Phase 2, feature 7): the amount
/// due and the IRS Form 1040-ES due date for a quarter of the tax year.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct EstimatedTaxPayment {
    /// Human-friendly label, e.g. "2026 Q1".
    pub label: String,
    /// The income period the payment covers, e.g. "Jan – Mar".
    pub period: String,
    /// IRS due date in ISO form, e.g. "2026-04-15".
    pub due_date: String,
    /// Amount due for the installment.
    pub amount: f64,
}

/// Estimated quarterly taxes for the current tax year (feature 7): the year's
/// projected liability split into the four IRS estimated-tax installments, so
/// the user knows what to pay and when.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct EstimatedTaxes {
    pub tax_year: i32,
    /// Total projected tax for the year (federal + state).
    pub total: f64,
    /// Plain-language description of how the installments were derived.
    pub note: String,
    pub payments: Vec<EstimatedTaxPayment>,
}

/// Full projection response: the annual engine output (feature 8) plus the
/// near-term quarterly withdrawal schedule (feature 9) and the current-year
/// estimated tax installments (feature 7).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ProjectionResponse {
    pub current_year: i32,
    pub start_year: i32,
    pub end_year: i32,
    pub assumptions: ProjectionAssumptions,
    pub summary: ProjectionSummary,
    pub annual: Vec<YearProjection>,
    pub quarterly: Vec<QuarterProjection>,
    pub estimated_taxes: EstimatedTaxes,
}
