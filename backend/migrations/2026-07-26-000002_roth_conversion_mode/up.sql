-- Roth conversion mode (roadmap Phase 3, feature 6): "fixed_ceiling" keeps
-- the existing behavior (fill taxable income to a flat, user-chosen AGI
-- ceiling each year). "irmaa_aware" instead sizes each year's conversion to
-- stay just under the household's next Medicare IRMAA bracket two years out,
-- tapering off as RMDs fill that room.
ALTER TABLE assumptions
    ADD COLUMN roth_conversion_mode TEXT NOT NULL DEFAULT 'fixed_ceiling';
