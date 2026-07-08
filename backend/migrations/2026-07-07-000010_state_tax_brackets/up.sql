-- Replace the flat per-state rate with each state's own tax structure: its own
-- progressive brackets and standard deduction, whether it taxes Social Security,
-- and whether it taxes long-term gains / qualified dividends as ordinary income
-- (as most states, including California, do — there is no federal-style
-- 0/15/20% preferential rate at the state level).

DROP TABLE state_tax_rates;

-- Per-state, per-filing-status scalar parameters.
CREATE TABLE state_tax_params (
    id TEXT PRIMARY KEY NOT NULL,

    -- Two-letter state code (uppercase).
    state TEXT NOT NULL,

    -- Filing status: single, married_filing_jointly, married_filing_separately,
    -- head_of_household, qualifying_widow.
    filing_status TEXT NOT NULL,

    -- The state's own standard deduction.
    standard_deduction DOUBLE NOT NULL,

    -- 1 if the state taxes Social Security benefits, 0 otherwise (most do not).
    taxes_social_security INTEGER NOT NULL,

    -- 1 if long-term gains / qualified dividends are taxed as ordinary income at
    -- the state level (true for most states, including California), 0 otherwise.
    taxes_capital_gains_as_ordinary INTEGER NOT NULL
);

CREATE UNIQUE INDEX idx_state_tax_params_key
    ON state_tax_params (state, filing_status);

-- Per-state, per-filing-status progressive brackets. A flat-tax state has a
-- single row; a no-income-tax state has a single 0% row.
CREATE TABLE state_tax_brackets (
    id TEXT PRIMARY KEY NOT NULL,
    state TEXT NOT NULL,
    filing_status TEXT NOT NULL,

    -- Lower bound of the bracket.
    floor_amount DOUBLE NOT NULL,

    -- Marginal rate as a fraction (e.g. 0.093 for 9.3%).
    rate DOUBLE NOT NULL
);

CREATE UNIQUE INDEX idx_state_tax_brackets_key
    ON state_tax_brackets (state, filing_status, floor_amount);
