-- Scenario cloning and branching (roadmap Phase 4, feature 4). Nullable
-- self-reference recording which saved plan a clone was branched from, so the
-- UI can show lineage ("Cloned from Baseline"). Not a hard foreign key —
-- plans.id values are freely deletable and a clone should survive its parent
-- being deleted later.
ALTER TABLE plans ADD COLUMN parent_plan_id TEXT;
