-- Reference tax parameters (roadmap Phase 2, features 1–5). These tables hold
-- the federal brackets, standard deductions, Social Security thresholds, and
-- state rates the tax engine reads. They are seeded programmatically at startup
-- from the application's built-in 2025 values and are intended to be maintained
-- through an admin role in a later phase.

-- Federal brackets, both ordinary-income and preferential (long-term capital
-- gain / qualified dividend) schedules. One row per bracket floor.
CREATE TABLE tax_brackets (
    id TEXT PRIMARY KEY NOT NULL,

    -- Tax year the schedule was published for; the engine uses the most recent
    -- year at or before a projection year as its base and indexes forward by
    -- inflation.
    tax_year INTEGER NOT NULL,

    -- 'ordinary' or 'capital_gains'.
    bracket_type TEXT NOT NULL,

    -- Filing status: single, married_filing_jointly, married_filing_separately,
    -- head_of_household, qualifying_widow.
    filing_status TEXT NOT NULL,

    -- Lower bound of the bracket (income at or above this, up to the next
    -- bracket's floor, is taxed at `rate`).
    floor_amount DOUBLE NOT NULL,

    -- Marginal rate as a fraction (e.g. 0.22 for 22%).
    rate DOUBLE NOT NULL
);

CREATE UNIQUE INDEX idx_tax_brackets_key
    ON tax_brackets (tax_year, bracket_type, filing_status, floor_amount);

-- Per-filing-status scalar parameters: the standard deduction (and the age-65
-- add-on) plus the Social Security taxation thresholds.
CREATE TABLE tax_filing_params (
    id TEXT PRIMARY KEY NOT NULL,
    tax_year INTEGER NOT NULL,
    filing_status TEXT NOT NULL,

    -- Base standard deduction.
    standard_deduction DOUBLE NOT NULL,

    -- Additional standard deduction per taxpayer age 65+.
    additional_senior_deduction DOUBLE NOT NULL,

    -- Provisional-income thresholds for Social Security taxation. These are
    -- fixed in statute and are NOT inflation-indexed by the engine.
    ss_base_threshold DOUBLE NOT NULL,
    ss_second_threshold DOUBLE NOT NULL
);

CREATE UNIQUE INDEX idx_tax_filing_params_key
    ON tax_filing_params (tax_year, filing_status);

-- Flat state income-tax rate approximation, one row per state (and DC).
-- (Superseded by per-state brackets in the following migration.)
CREATE TABLE state_tax_rates (
    id TEXT PRIMARY KEY NOT NULL,

    -- Two-letter state code (uppercase).
    state TEXT NOT NULL,

    -- Rate as a percentage (e.g. 6.0 for 6%). No-income-tax states are 0.
    rate DOUBLE NOT NULL
);

CREATE UNIQUE INDEX idx_state_tax_rates_state ON state_tax_rates (state);
