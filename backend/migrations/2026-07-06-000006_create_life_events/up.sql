CREATE TABLE life_events (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,

    name TEXT NOT NULL,

    -- sell_house | buy_home | inheritance | downsize | start_medicare |
    -- claim_social_security | pay_off_mortgage | relocate | large_purchase |
    -- gift | death_of_spouse | other
    event_type TEXT NOT NULL,

    event_date DATE NOT NULL,

    -- inflow (money in) | outflow (money out)
    direction TEXT NOT NULL,
    -- Cash amount for the event, always stored as a non-negative number;
    -- `direction` carries the sign.
    amount DOUBLE NOT NULL DEFAULT 0,

    -- Whether the event's cash flow is taxable.
    taxable BOOL NOT NULL DEFAULT 0,
    -- Whether the amount grows with inflation before the event date.
    inflation_adjusted BOOL NOT NULL DEFAULT 0,

    -- one_time | monthly | annual
    recurrence TEXT NOT NULL DEFAULT 'one_time',
    -- For recurring events, the (optional) date the recurrence stops.
    end_date DATE,

    notes TEXT,

    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX idx_life_events_user_id ON life_events (user_id);
