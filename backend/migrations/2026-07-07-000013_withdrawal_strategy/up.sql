-- Withdrawal sequencing strategy (roadmap Phase 2, feature 9). "conventional"
-- keeps the existing behavior: draw taxable accounts fully before tax-deferred,
-- then tax-free. "tax_optimized" lets the engine reorder taxable accounts by
-- ascending embedded gain (realizing the cheapest gains first) and, in years
-- where realizing a gain would cost more at the margin than an equivalent
-- ordinary withdrawal, draw tax-deferred funds before taxable ones.
ALTER TABLE assumptions
    ADD COLUMN withdrawal_strategy TEXT NOT NULL DEFAULT 'conventional';
