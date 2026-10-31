-- Medicare enrollment age: the age at which Part B premiums (and any IRMAA
-- surcharge) begin for each household member. Defaults to 65; later when an
-- employer plan delays enrollment.
ALTER TABLE assumptions
    ADD COLUMN medicare_enrollment_age INTEGER NOT NULL DEFAULT 65;

-- Roth conversion sizing mode: 'fixed_ceiling' fills taxable income to
-- roth_conversion_ceiling; 'irmaa_aware' stays under the next Medicare IRMAA
-- tier; 'aca_aware' stays under the ACA subsidy cutoff.
ALTER TABLE assumptions
    ADD COLUMN roth_conversion_mode TEXT NOT NULL DEFAULT 'fixed_ceiling';
