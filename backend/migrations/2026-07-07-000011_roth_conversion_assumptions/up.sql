-- Roth conversion strategy (roadmap Phase 2, feature 6). A user can direct the
-- planner to convert traditional (tax-deferred) dollars to Roth (tax-free) each
-- year up to a target taxable-income ceiling, optionally limited to a year
-- window. These knobs live alongside the other planning assumptions.

-- Convert traditional -> Roth each year until taxable income reaches this
-- ceiling (in dollars). 0 disables Roth conversions.
ALTER TABLE assumptions
    ADD COLUMN roth_conversion_ceiling DOUBLE NOT NULL DEFAULT 0;

-- First and last calendar years the conversion strategy applies. NULL means
-- "no bound" on that end (convert from the start of, or through the end of, the
-- projection).
ALTER TABLE assumptions
    ADD COLUMN roth_conversion_start_year INTEGER;

ALTER TABLE assumptions
    ADD COLUMN roth_conversion_end_year INTEGER;
