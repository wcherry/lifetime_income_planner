DROP TABLE state_tax_brackets;
DROP TABLE state_tax_params;

CREATE TABLE state_tax_rates (
    id TEXT PRIMARY KEY NOT NULL,
    state TEXT NOT NULL,
    rate DOUBLE NOT NULL
);

CREATE UNIQUE INDEX idx_state_tax_rates_state ON state_tax_rates (state);
