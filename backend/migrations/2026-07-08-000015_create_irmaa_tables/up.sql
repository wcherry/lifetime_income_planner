-- Medicare IRMAA surcharge reference data (roadmap Phase 3, feature 4). These
-- rows back the crate::irmaa::IrmaaTables the surcharge engine reads. They are
-- seeded at startup from the application's built-in 2025 CMS values and are
-- intended to be maintained through an admin role in a later phase.
--
-- One row per (filing_group, threshold) bracket: at or above `magi_threshold`,
-- both the Part B and Part D surcharges apply, on top of the standard
-- premiums. `filing_group` is one of "single" (also covers head-of-household
-- and qualifying widow(er)), "married_filing_jointly", or
-- "married_filing_separately".
CREATE TABLE irmaa_brackets (
    id TEXT PRIMARY KEY NOT NULL,

    -- Year the brackets were published for; the most recent year is used as
    -- the base and indexed forward by inflation.
    base_year INTEGER NOT NULL,

    filing_group TEXT NOT NULL,

    -- Household MAGI (2-years-prior lookback) at or above which this tier's
    -- surcharges apply.
    magi_threshold DOUBLE NOT NULL,

    -- Monthly surcharge added to the standard Part B premium, per enrolled
    -- household member.
    part_b_surcharge_monthly DOUBLE NOT NULL,

    -- Monthly surcharge added to the standard Part D premium, per enrolled
    -- household member.
    part_d_surcharge_monthly DOUBLE NOT NULL
);

CREATE UNIQUE INDEX idx_irmaa_bracket_key
    ON irmaa_brackets (base_year, filing_group, magi_threshold);
