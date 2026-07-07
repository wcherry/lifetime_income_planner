CREATE TABLE accounts (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,

    name TEXT NOT NULL,

    -- Tax treatment: taxable | tax_deferred | tax_free | other
    category TEXT NOT NULL,
    -- Specific account type: brokerage | savings | checking | money_market |
    -- cd | ira | 401k | 403b | 457 | sep_ira | roth_ira | roth_401k | hsa |
    -- pension | cash_value_life_insurance
    account_type TEXT NOT NULL,
    -- self | spouse | joint
    owner TEXT NOT NULL,

    current_balance DOUBLE NOT NULL,
    -- Expected annual rate of return, as a percentage (e.g. 6.5).
    expected_roi DOUBLE NOT NULL,
    -- Annual dividend yield as a percentage.
    dividend_yield DOUBLE NOT NULL DEFAULT 0,
    -- Cost basis for taxable accounts (nullable otherwise).
    cost_basis DOUBLE,

    -- Target allocation percentages (optional, should sum to 100 when provided).
    allocation_stock_pct INTEGER,
    allocation_bond_pct INTEGER,
    allocation_cash_pct INTEGER,

    -- Free-form notes on withdrawal restrictions (e.g. age 59.5, penalties).
    withdrawal_restrictions TEXT,

    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX idx_accounts_user_id ON accounts (user_id);
