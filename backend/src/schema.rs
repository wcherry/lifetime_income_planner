// @generated automatically by Diesel CLI.

diesel::table! {
    aca_applicable_percentages (id) {
        id -> Text,
        fpl_percent -> Double,
        applicable_percentage -> Double,
    }
}

diesel::table! {
    aca_fpl_guidelines (id) {
        id -> Text,
        base_year -> Integer,
        household_size -> Integer,
        annual_amount -> Double,
    }
}

diesel::table! {
    accounts (id) {
        id -> Text,
        user_id -> Text,
        name -> Text,
        category -> Text,
        account_type -> Text,
        owner -> Text,
        current_balance -> Double,
        expected_roi -> Double,
        dividend_yield -> Double,
        cost_basis -> Nullable<Double>,
        allocation_stock_pct -> Nullable<Integer>,
        allocation_bond_pct -> Nullable<Integer>,
        allocation_cash_pct -> Nullable<Integer>,
        withdrawal_restrictions -> Nullable<Text>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    assumptions (id) {
        id -> Text,
        user_id -> Text,
        inflation_rate -> Double,
        investment_return_rate -> Double,
        healthcare_inflation_rate -> Double,
        social_security_cola_rate -> Double,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        roth_conversion_ceiling -> Double,
        roth_conversion_start_year -> Nullable<Integer>,
        roth_conversion_end_year -> Nullable<Integer>,
        withdrawal_strategy -> Text,
        aca_benchmark_annual_premium -> Double,
        medicare_part_b_annual_premium -> Double,
    }
}

diesel::table! {
    collaborators (id) {
        id -> Text,
        owner_user_id -> Text,
        collaborator_user_id -> Text,
        invited_email -> Text,
        role -> Text,
        status -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    income_sources (id) {
        id -> Text,
        user_id -> Text,
        name -> Text,
        income_type -> Text,
        owner -> Text,
        amount -> Double,
        frequency -> Text,
        start_date -> Date,
        end_date -> Nullable<Date>,
        growth_rate -> Double,
        cola -> Bool,
        taxability -> Text,
        notes -> Nullable<Text>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    irmaa_brackets (id) {
        id -> Text,
        base_year -> Integer,
        filing_group -> Text,
        magi_threshold -> Double,
        part_b_surcharge_monthly -> Double,
        part_d_surcharge_monthly -> Double,
    }
}

diesel::table! {
    life_events (id) {
        id -> Text,
        user_id -> Text,
        name -> Text,
        event_type -> Text,
        event_date -> Date,
        direction -> Text,
        amount -> Double,
        taxable -> Bool,
        inflation_adjusted -> Bool,
        recurrence -> Text,
        end_date -> Nullable<Date>,
        notes -> Nullable<Text>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    plaid_items (id) {
        id -> Text,
        user_id -> Text,
        account_id -> Nullable<Text>,
        plaid_item_id -> Text,
        plaid_access_token -> Text,
        institution_name -> Text,
        status -> Text,
        last_synced_at -> Nullable<Timestamp>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    plaid_transactions (id) {
        id -> Text,
        user_id -> Text,
        plaid_item_id -> Text,
        account_id -> Nullable<Text>,
        plaid_transaction_id -> Text,
        posted_date -> Date,
        amount -> Double,
        description -> Text,
        category -> Nullable<Text>,
        created_at -> Timestamp,
    }
}

diesel::table! {
    plan_snapshots (id) {
        id -> Text,
        plan_id -> Text,
        user_id -> Text,
        snapshot -> Text,
        created_at -> Timestamp,
    }
}

diesel::table! {
    plans (id) {
        id -> Text,
        user_id -> Text,
        name -> Text,
        snapshot -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        parent_plan_id -> Nullable<Text>,
    }
}

diesel::table! {
    profiles (id) {
        id -> Text,
        user_id -> Text,
        first_name -> Text,
        last_name -> Text,
        date_of_birth -> Date,
        marital_status -> Text,
        filing_status -> Text,
        state -> Text,
        retirement_date -> Date,
        life_expectancy -> Integer,
        spouse_first_name -> Nullable<Text>,
        spouse_last_name -> Nullable<Text>,
        spouse_date_of_birth -> Nullable<Date>,
        spouse_life_expectancy -> Nullable<Integer>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    quarterly_reviews (id) {
        id -> Text,
        user_id -> Text,
        year -> Integer,
        quarter -> Integer,
        planned_income -> Double,
        planned_spending -> Double,
        planned_tax -> Double,
        planned_withdrawal -> Double,
        actual_income -> Double,
        actual_spending -> Double,
        actual_tax -> Double,
        actual_balances -> Text,
        notes -> Nullable<Text>,
        created_at -> Timestamp,
    }
}

diesel::table! {
    social_security_estimates (id) {
        id -> Text,
        user_id -> Text,
        owner -> Text,
        statement_date -> Date,
        estimate_at_62 -> Nullable<Double>,
        estimate_at_67 -> Nullable<Double>,
        estimate_at_70 -> Nullable<Double>,
        source -> Text,
        created_at -> Timestamp,
    }
}

diesel::table! {
    spending_items (id) {
        id -> Text,
        user_id -> Text,
        name -> Text,
        category -> Text,
        amount -> Double,
        frequency -> Text,
        inflation_adjusted -> Bool,
        start_year -> Nullable<Integer>,
        end_year -> Nullable<Integer>,
        notes -> Nullable<Text>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    state_tax_brackets (id) {
        id -> Text,
        state -> Text,
        filing_status -> Text,
        floor_amount -> Double,
        rate -> Double,
    }
}

diesel::table! {
    state_tax_params (id) {
        id -> Text,
        state -> Text,
        filing_status -> Text,
        standard_deduction -> Double,
        taxes_social_security -> Integer,
        taxes_capital_gains_as_ordinary -> Integer,
    }
}

diesel::table! {
    tax_brackets (id) {
        id -> Text,
        tax_year -> Integer,
        bracket_type -> Text,
        filing_status -> Text,
        floor_amount -> Double,
        rate -> Double,
    }
}

diesel::table! {
    tax_documents (id) {
        id -> Text,
        user_id -> Text,
        tax_year -> Integer,
        form_type -> Text,
        box_data -> Text,
        source_filename -> Nullable<Text>,
        imported_at -> Timestamp,
    }
}

diesel::table! {
    tax_filing_params (id) {
        id -> Text,
        tax_year -> Integer,
        filing_status -> Text,
        standard_deduction -> Double,
        additional_senior_deduction -> Double,
        ss_base_threshold -> Double,
        ss_second_threshold -> Double,
    }
}

diesel::table! {
    users (id) {
        id -> Text,
        email -> Text,
        password_hash -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::joinable!(accounts -> users (user_id));
diesel::joinable!(assumptions -> users (user_id));
diesel::joinable!(collaborators -> users (owner_user_id));
diesel::joinable!(income_sources -> users (user_id));
diesel::joinable!(life_events -> users (user_id));
diesel::joinable!(plaid_items -> accounts (account_id));
diesel::joinable!(plaid_items -> users (user_id));
diesel::joinable!(plaid_transactions -> plaid_items (plaid_item_id));
diesel::joinable!(plaid_transactions -> users (user_id));
diesel::joinable!(plan_snapshots -> plans (plan_id));
diesel::joinable!(plan_snapshots -> users (user_id));
diesel::joinable!(plans -> users (user_id));
diesel::joinable!(profiles -> users (user_id));
diesel::joinable!(quarterly_reviews -> users (user_id));
diesel::joinable!(social_security_estimates -> users (user_id));
diesel::joinable!(spending_items -> users (user_id));
diesel::joinable!(tax_documents -> users (user_id));

diesel::allow_tables_to_appear_in_same_query!(
    aca_applicable_percentages,
    aca_fpl_guidelines,
    accounts,
    assumptions,
    collaborators,
    income_sources,
    irmaa_brackets,
    life_events,
    plaid_items,
    plaid_transactions,
    plan_snapshots,
    plans,
    profiles,
    quarterly_reviews,
    social_security_estimates,
    spending_items,
    state_tax_brackets,
    state_tax_params,
    tax_brackets,
    tax_documents,
    tax_filing_params,
    users,
);
