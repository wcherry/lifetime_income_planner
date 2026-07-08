-- Medicare Part B premium modeling (roadmap Phase 3, feature 3). The annual
-- standard Part B premium the household pays per Medicare-enrolled member
-- (age 65+), before any income-based IRMAA surcharge (a later phase). It
-- defaults to the 2025 standard rate ($185.00/mo = $2,220/yr) so it models a
-- real, near-universal cost out of the box; 0 disables it.
ALTER TABLE assumptions
    ADD COLUMN medicare_part_b_annual_premium DOUBLE NOT NULL DEFAULT 2220;
