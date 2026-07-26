-- "Don't incur penalties" (withdrawal sequencing): when set, the engine
-- excludes tax-deferred accounts from the drawdown order entirely for an
-- owner who hasn't reached age 59½, rather than drawing from them and paying
-- the 10% early-withdrawal penalty (IRC §72(t)).
ALTER TABLE assumptions
    ADD COLUMN avoid_early_withdrawal_penalty BOOL NOT NULL DEFAULT 0;
