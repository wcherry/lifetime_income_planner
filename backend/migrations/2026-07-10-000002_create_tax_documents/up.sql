-- Tax form imports (roadmap Phase 6, feature 3): a parsed tax document
-- (1099-DIV, 1099-INT, 1099-R, W2, SSA-1099, ...) with its box amounts
-- normalized into a JSON map, so actuals can be compared against the
-- assumptions-driven tax projection.
CREATE TABLE tax_documents (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,

    tax_year INTEGER NOT NULL,
    form_type TEXT NOT NULL,
    box_data TEXT NOT NULL,
    source_filename TEXT,

    imported_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX idx_tax_documents_user_year ON tax_documents (user_id, tax_year);
