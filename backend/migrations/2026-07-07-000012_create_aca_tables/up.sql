-- ACA premium tax credit reference data (roadmap Phase 3, feature 1). These
-- tables back the crate::aca::AcaTables the subsidy engine reads. They are
-- seeded at startup from the application's built-in 2025 values and are intended
-- to be maintained through an admin role in a later phase.

-- Federal Poverty Guidelines (48 contiguous states + DC), one row per household
-- size. The engine indexes these forward from `base_year` by general inflation.
CREATE TABLE aca_fpl_guidelines (
    id TEXT PRIMARY KEY NOT NULL,

    -- Year the guidelines were published for; the most recent year is used as
    -- the base and indexed forward by inflation.
    base_year INTEGER NOT NULL,

    -- Number of people in the tax household (1, 2, ...).
    household_size INTEGER NOT NULL,

    -- Annual poverty-line dollar amount for that household size.
    annual_amount DOUBLE NOT NULL
);

CREATE UNIQUE INDEX idx_aca_fpl_key
    ON aca_fpl_guidelines (base_year, household_size);

-- Applicable-percentage curve: at `fpl_percent` of the poverty line the
-- household is expected to contribute `applicable_percentage` (a fraction) of
-- its MAGI toward the benchmark premium. Values between rows are interpolated.
CREATE TABLE aca_applicable_percentages (
    id TEXT PRIMARY KEY NOT NULL,

    -- Percentage of the Federal Poverty Line (e.g. 250.0 for 250%).
    fpl_percent DOUBLE NOT NULL,

    -- Expected contribution as a fraction of MAGI (e.g. 0.04 for 4%).
    applicable_percentage DOUBLE NOT NULL
);

CREATE UNIQUE INDEX idx_aca_applicable_key
    ON aca_applicable_percentages (fpl_percent);

-- Per-user ACA input: the annual benchmark (second-lowest silver, "SLCSP")
-- premium the household would face on the marketplace. 0 disables ACA subsidy
-- modeling. It grows with the healthcare inflation assumption over the plan.
ALTER TABLE assumptions
    ADD COLUMN aca_benchmark_annual_premium DOUBLE NOT NULL DEFAULT 0;
