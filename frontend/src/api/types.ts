// Shapes mirror the backend DTOs (see backend/src/models).

export type MaritalStatus = "single" | "married" | "widowed";

export type FilingStatus =
  | "single"
  | "married_filing_jointly"
  | "married_filing_separately"
  | "head_of_household"
  | "qualifying_widow";

export interface User {
  id: string;
  email: string;
  created_at: string;
}

export interface AuthResponse {
  token: string;
  user: User;
}

export interface Profile {
  id: string;
  first_name: string;
  last_name: string;
  date_of_birth: string;
  marital_status: MaritalStatus;
  filing_status: FilingStatus;
  state: string;
  retirement_date: string;
  life_expectancy: number;
  spouse_first_name: string | null;
  spouse_last_name: string | null;
  spouse_date_of_birth: string | null;
  spouse_life_expectancy: number | null;
  updated_at: string;
}

export type AccountCategory = "taxable" | "tax_deferred" | "tax_free" | "other";

export type AccountType =
  | "brokerage"
  | "savings"
  | "checking"
  | "money_market"
  | "cd"
  | "ira"
  | "401k"
  | "403b"
  | "457"
  | "sep_ira"
  | "roth_ira"
  | "roth_401k"
  | "hsa"
  | "pension"
  | "cash_value_life_insurance";

export type AccountOwner = "self" | "spouse" | "joint";

export interface Account {
  id: string;
  name: string;
  category: AccountCategory;
  account_type: AccountType;
  owner: AccountOwner;
  current_balance: number;
  expected_roi: number;
  dividend_yield: number;
  cost_basis: number | null;
  allocation_stock_pct: number | null;
  allocation_bond_pct: number | null;
  allocation_cash_pct: number | null;
  withdrawal_restrictions: string | null;
  created_at: string;
  updated_at: string;
}

export interface AccountRequest {
  name: string;
  category: AccountCategory;
  account_type: AccountType;
  owner: AccountOwner;
  current_balance: number;
  expected_roi: number;
  dividend_yield: number;
  cost_basis?: number | null;
  allocation_stock_pct?: number | null;
  allocation_bond_pct?: number | null;
  allocation_cash_pct?: number | null;
  withdrawal_restrictions?: string | null;
}

export type SpendingCategory =
  | "essential"
  | "discretionary"
  | "healthcare"
  | "travel"
  | "one_time"
  | "charity"
  | "taxes"
  | "home_maintenance"
  | "vehicle_replacement"
  | "large_purchase";

export type SpendingFrequency = "monthly" | "annual" | "one_time";

export interface SpendingItem {
  id: string;
  name: string;
  category: SpendingCategory;
  amount: number;
  frequency: SpendingFrequency;
  annual_amount: number;
  inflation_adjusted: boolean;
  start_year: number | null;
  end_year: number | null;
  notes: string | null;
  updated_at: string;
}

export interface SpendingRequest {
  name: string;
  category: SpendingCategory;
  amount: number;
  frequency: SpendingFrequency;
  inflation_adjusted: boolean;
  start_year?: number | null;
  end_year?: number | null;
  notes?: string | null;
}

export type IncomeType =
  | "social_security"
  | "pension"
  | "rental"
  | "royalties"
  | "annuity"
  | "employment"
  | "consulting"
  | "part_time";

export type IncomeFrequency = "monthly" | "annual";

export type Taxability = "taxable" | "partially_taxable" | "tax_free";

export type IncomeOwner = "self" | "spouse" | "joint";

export interface IncomeSource {
  id: string;
  name: string;
  income_type: IncomeType;
  owner: IncomeOwner;
  amount: number;
  frequency: IncomeFrequency;
  annual_amount: number;
  start_date: string;
  end_date: string | null;
  growth_rate: number;
  cola: boolean;
  taxability: Taxability;
  notes: string | null;
  updated_at: string;
}

export interface IncomeRequest {
  name: string;
  income_type: IncomeType;
  owner: IncomeOwner;
  amount: number;
  frequency: IncomeFrequency;
  start_date: string;
  end_date?: string | null;
  growth_rate: number;
  cola: boolean;
  taxability: Taxability;
  notes?: string | null;
}

export type LifeEventType =
  | "sell_house"
  | "buy_home"
  | "inheritance"
  | "downsize"
  | "start_medicare"
  | "claim_social_security"
  | "pay_off_mortgage"
  | "relocate"
  | "large_purchase"
  | "gift"
  | "death_of_spouse"
  | "other";

export type CashFlowDirection = "inflow" | "outflow";

export type EventRecurrence = "one_time" | "monthly" | "annual";

export interface LifeEvent {
  id: string;
  name: string;
  event_type: LifeEventType;
  event_date: string;
  direction: CashFlowDirection;
  amount: number;
  signed_amount: number;
  taxable: boolean;
  inflation_adjusted: boolean;
  recurrence: EventRecurrence;
  end_date: string | null;
  notes: string | null;
  updated_at: string;
}

export interface LifeEventRequest {
  name: string;
  event_type: LifeEventType;
  event_date: string;
  direction: CashFlowDirection;
  amount: number;
  taxable: boolean;
  inflation_adjusted: boolean;
  recurrence: EventRecurrence;
  end_date?: string | null;
  notes?: string | null;
}

export type WithdrawalStrategy = "conventional" | "tax_optimized";

export interface Assumptions {
  inflation_rate: number;
  investment_return_rate: number;
  healthcare_inflation_rate: number;
  social_security_cola_rate: number;
  roth_conversion_ceiling: number;
  roth_conversion_start_year: number | null;
  roth_conversion_end_year: number | null;
  aca_benchmark_annual_premium: number;
  withdrawal_strategy: WithdrawalStrategy;
  medicare_part_b_annual_premium: number;
  is_default: boolean;
  updated_at: string | null;
}

export interface AssumptionsRequest {
  inflation_rate: number;
  investment_return_rate: number;
  healthcare_inflation_rate: number;
  social_security_cola_rate: number;
  roth_conversion_ceiling: number;
  roth_conversion_start_year?: number | null;
  roth_conversion_end_year?: number | null;
  aca_benchmark_annual_premium: number;
  withdrawal_strategy: WithdrawalStrategy;
  medicare_part_b_annual_premium: number;
}

export interface ProjectionAssumptions {
  inflation_rate: number;
  investment_return_rate: number;
  healthcare_inflation_rate: number;
  social_security_cola_rate: number;
  roth_conversion_ceiling: number;
  roth_conversion_start_year: number | null;
  roth_conversion_end_year: number | null;
  /** Withdrawal sequencing strategy driving the drawdown order (feature 9). */
  withdrawal_strategy: string;
  aca_benchmark_annual_premium: number;
  medicare_part_b_annual_premium: number;
  is_default: boolean;
}

export interface ProjectionSummary {
  current_net_worth: number;
  projected_ending_balance: number;
  total_lifetime_income: number;
  total_lifetime_spending: number;
  total_lifetime_withdrawals: number;
  total_lifetime_taxes: number;
  total_lifetime_federal_taxes: number;
  total_lifetime_state_taxes: number;
  total_lifetime_roth_conversions: number;
  total_lifetime_aca_subsidies: number;
  total_lifetime_medicare_premiums: number;
  total_lifetime_irmaa_surcharges: number;
  depletion_year: number | null;
}

export interface YearAca {
  eligible: boolean;
  magi: number;
  federal_poverty_line: number;
  /** MAGI as a percentage of the poverty line (e.g. 250.0 for 250%). */
  fpl_percent: number;
  /** Expected contribution as a fraction of MAGI (e.g. 0.04 for 4%). */
  applicable_percentage: number;
  expected_contribution: number;
  benchmark_premium: number;
  subsidy: number;
}

export interface YearIrmaa {
  applies: boolean;
  /** Whether two-years-prior MAGI was available; false for the plan's first two years. */
  has_lookback_data: boolean;
  lookback_year: number;
  lookback_magi: number;
  /** This tier's Part B surcharge, per enrolled person, per month. */
  part_b_surcharge_monthly: number;
  /** This tier's Part D surcharge, per enrolled person, per month. */
  part_d_surcharge_monthly: number;
  /** Number of household members enrolled (65+) and paying the surcharge this year. */
  enrolled_count: number;
  /** Household total surcharge for the year (Part B + Part D, both enrolled members). */
  total_surcharge: number;
}

export interface YearTax {
  ordinary_income: number;
  qualified_dividends: number;
  capital_gains: number;
  social_security_benefits: number;
  taxable_social_security: number;
  adjusted_gross_income: number;
  /** Modified Adjusted Gross Income (MAGI): AGI plus untaxed Social Security, tracked every year. */
  magi: number;
  standard_deduction: number;
  taxable_income: number;
  federal_ordinary_tax: number;
  federal_capital_gains_tax: number;
  federal_tax: number;
  state_taxable_income: number;
  state_standard_deduction: number;
  state_tax: number;
  /** State marginal rate as a fraction (e.g. 0.093). */
  state_marginal_rate: number;
  /** Property tax for the year. Reserved for a later milestone; currently 0. */
  property_tax: number;
  total_tax: number;
  /** Total (federal + state) tax as a fraction of gross income (0–1). */
  effective_rate: number;
  /** Federal ordinary marginal rate as a fraction (e.g. 0.22). */
  marginal_rate: number;
}

export interface LifeEventOccurrence {
  name: string;
  amount: number;
}

export interface Milestone {
  label: string;
  detail: string;
  age: number;
}

export interface YearProjection {
  year: number;
  primary_age: number;
  spouse_age: number | null;
  starting_balance: number;
  income: number;
  spending: number;
  life_events_net: number;
  life_events: LifeEventOccurrence[];
  milestones: Milestone[];
  growth: number;
  withdrawals: number;
  /** Required minimum distribution due this year across the household (RMD module); 0 before RMDs begin. */
  rmd_amount: number;
  /** Medicare Part B premiums due this year, per enrolled household member (65+); 0 if disabled. */
  medicare_premiums: number;
  /** Medicare IRMAA surcharge due this year, based on household MAGI from two years prior; 0 before it applies. */
  irmaa_surcharge: number;
  contributions: number;
  roth_conversion: number;
  taxes: number;
  tax: YearTax;
  /** Which category was drawn from first this year (feature 9): "taxable_first" or "tax_deferred_first". */
  withdrawal_order: string;
  aca: YearAca;
  irmaa: YearIrmaa;
  ending_balance: number;
  shortfall: number;
}

export interface QuarterWithdrawal {
  account_id: string;
  account_name: string;
  category: AccountCategory;
  amount: number;
}

export interface QuarterProjection {
  label: string;
  year: number;
  quarter: number;
  income: number;
  spending: number;
  estimated_tax: number;
  total_withdrawal: number;
  withdrawals: QuarterWithdrawal[];
}

export interface EstimatedTaxPayment {
  label: string;
  period: string;
  /** ISO due date, e.g. "2026-04-15". */
  due_date: string;
  amount: number;
}

export interface EstimatedTaxes {
  tax_year: number;
  total: number;
  note: string;
  payments: EstimatedTaxPayment[];
}

export interface Projection {
  current_year: number;
  start_year: number;
  end_year: number;
  assumptions: ProjectionAssumptions;
  summary: ProjectionSummary;
  annual: YearProjection[];
  quarterly: QuarterProjection[];
  estimated_taxes: EstimatedTaxes;
}

export interface PlanContents {
  has_profile: boolean;
  has_assumptions: boolean;
  accounts: number;
  income: number;
  spending: number;
  life_events: number;
}

export interface Plan {
  id: string;
  name: string;
  contents: PlanContents;
  created_at: string;
  updated_at: string;
}

export interface SavePlanRequest {
  name: string;
}

export interface UpsertProfileRequest {
  first_name: string;
  last_name: string;
  date_of_birth: string;
  marital_status: MaritalStatus;
  filing_status: FilingStatus;
  state: string;
  retirement_date: string;
  life_expectancy: number;
  spouse_first_name?: string | null;
  spouse_last_name?: string | null;
  spouse_date_of_birth?: string | null;
  spouse_life_expectancy?: number | null;
}
