CREATE TABLE profiles (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL UNIQUE,

    first_name TEXT NOT NULL,
    last_name TEXT NOT NULL,
    date_of_birth DATE NOT NULL,

    -- single | married | widowed
    marital_status TEXT NOT NULL,
    -- single | married_filing_jointly | married_filing_separately |
    -- head_of_household | qualifying_widow
    filing_status TEXT NOT NULL,

    state TEXT NOT NULL,
    retirement_date DATE NOT NULL,
    life_expectancy INTEGER NOT NULL,

    -- Optional spouse details (present when marital_status = 'married')
    spouse_first_name TEXT,
    spouse_last_name TEXT,
    spouse_date_of_birth DATE,
    spouse_life_expectancy INTEGER,

    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX idx_profiles_user_id ON profiles (user_id);
