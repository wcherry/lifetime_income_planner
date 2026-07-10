-- Social Security statement import (roadmap Phase 6, feature 4): the
-- estimated monthly benefit at ages 62/67/70 as shown on a real SSA
-- statement, so an income source can be generated from a chosen claiming age
-- instead of guessed by hand.
CREATE TABLE social_security_estimates (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,

    owner TEXT NOT NULL,
    statement_date DATE NOT NULL,
    estimate_at_62 DOUBLE,
    estimate_at_67 DOUBLE,
    estimate_at_70 DOUBLE,
    source TEXT NOT NULL,

    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX idx_ss_estimates_user ON social_security_estimates (user_id);
