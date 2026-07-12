-- Troubleshooting aid: the imported CSV row's own header->value pairs,
-- verbatim (before any parsing/normalization), stored as a JSON object
-- string so a user can see exactly what was in the file for a given
-- transaction (e.g. via a details popup).
ALTER TABLE spending_tracker_transactions
    ADD COLUMN raw_row_json TEXT NOT NULL DEFAULT '{}';
