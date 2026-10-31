-- ACA 2026 rules. The enhanced premium tax credits (the curve in
-- aca_applicable_percentages) applied through 2025. From 2026 the original
-- schedule returns -- different contribution percentages and no credit above a
-- cutoff -- and the Marketplace measures income against the *prior* year's
-- poverty guidelines. These tables hold those rules; they are seeded at startup
-- from the application's built-in values when empty.

-- One row: how the credit changes over time.
CREATE TABLE aca_rules (
    id TEXT PRIMARY KEY NOT NULL,

    -- Last year the enhanced curve (aca_applicable_percentages) applies. NULL
    -- means the enhanced curve applies to every year.
    enhanced_through_year INTEGER,

    -- In years after the enhanced schedule, no credit above this percent of the
    -- poverty line. NULL means no limit.
    cliff_fpl_percent DOUBLE,

    -- Years the poverty guidelines lag the tax year (the Marketplace uses the
    -- prior year's guidelines: 1). 0 indexes from the tax year itself.
    guideline_lag_years INTEGER NOT NULL DEFAULT 0
);

-- The original applicable-percentage curve, used after the enhanced schedule.
-- Rows are interpolated by fpl_percent; two rows at the same percent make a
-- step, ordered by sort_order.
CREATE TABLE aca_standard_applicable_percentages (
    id TEXT PRIMARY KEY NOT NULL,
    sort_order INTEGER NOT NULL,
    fpl_percent DOUBLE NOT NULL,
    applicable_percentage DOUBLE NOT NULL
);
