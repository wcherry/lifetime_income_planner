CREATE TABLE assumptions (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL UNIQUE,

    -- General price inflation, as an annual percentage (e.g. 2.5 = 2.5%).
    inflation_rate DOUBLE NOT NULL DEFAULT 2.5,
    -- Default expected investment return, annual percentage.
    investment_return_rate DOUBLE NOT NULL DEFAULT 6.0,
    -- Healthcare-specific inflation, annual percentage.
    healthcare_inflation_rate DOUBLE NOT NULL DEFAULT 4.5,
    -- Assumed future Social Security cost-of-living adjustment, annual percentage.
    social_security_cola_rate DOUBLE NOT NULL DEFAULT 2.0,

    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX idx_assumptions_user_id ON assumptions (user_id);
