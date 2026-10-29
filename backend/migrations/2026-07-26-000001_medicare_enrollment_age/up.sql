-- Medicare enrollment age (roadmap Phase 3, feature 3): the age at which
-- Medicare Part B premiums (and IRMAA enrollment counting) begin for each
-- household member. Defaults to 65, matching the previous hardcoded
-- behavior; a later age models someone who delays enrollment because of
-- other creditable coverage (e.g. an employer plan).
ALTER TABLE assumptions
    ADD COLUMN medicare_enrollment_age INTEGER NOT NULL DEFAULT 65;
