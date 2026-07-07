CREATE TABLE income_sources (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,

    name TEXT NOT NULL,

    -- social_security | pension | rental | royalties | annuity | employment |
    -- consulting | part_time
    income_type TEXT NOT NULL,
    -- self | spouse | joint
    owner TEXT NOT NULL,

    -- Amount per `frequency` period.
    amount DOUBLE NOT NULL,
    -- monthly | annual
    frequency TEXT NOT NULL,

    start_date DATE NOT NULL,
    -- Null end date means the income continues for life.
    end_date DATE,

    -- Fixed annual growth as a percentage (e.g. raises); 0 for none.
    growth_rate DOUBLE NOT NULL DEFAULT 0,
    -- Whether the income receives a cost-of-living adjustment tied to inflation.
    cola BOOL NOT NULL DEFAULT 0,

    -- taxable | partially_taxable | tax_free
    taxability TEXT NOT NULL,

    notes TEXT,

    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX idx_income_sources_user_id ON income_sources (user_id);
