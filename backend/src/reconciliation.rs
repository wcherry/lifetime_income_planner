//! Quarterly review reconciliation (roadmap Phase 5, features 1-4).
//!
//! A pure calculation engine, no DB/serde: given a quarter's actual cash-flow
//! figures (income, spending, tax) and each account's starting/ending balance
//! for the quarter, derives how much of the balance change is explained by
//! cash flow versus investment performance. This is informational only —
//! it's computed at read time from a completed review's stored figures, never
//! stored itself, and never blocks completing a review.

/// One account's balance movement across a reviewed quarter.
pub struct AccountBalanceChange {
    pub starting_balance: f64,
    pub ending_balance: f64,
}

/// The reconciliation of a quarter's actual cash flow against the observed
/// change in account balances.
pub struct Reconciliation {
    pub total_starting_balance: f64,
    pub total_ending_balance: f64,
    pub net_balance_change: f64,
    /// `actual_income - actual_spending - actual_tax`.
    pub net_cash_flow: f64,
    /// `net_balance_change - net_cash_flow` — the portion of the balance
    /// change not explained by cash flow, i.e. investment gain/loss.
    pub implied_investment_gain: f64,
}

/// Reconcile a quarter's actual cash flow against its actual account balance
/// changes.
pub fn reconcile(
    actual_income: f64,
    actual_spending: f64,
    actual_tax: f64,
    balances: &[AccountBalanceChange],
) -> Reconciliation {
    let total_starting_balance: f64 = balances.iter().map(|b| b.starting_balance).sum();
    let total_ending_balance: f64 = balances.iter().map(|b| b.ending_balance).sum();
    let net_balance_change = total_ending_balance - total_starting_balance;
    let net_cash_flow = actual_income - actual_spending - actual_tax;
    let implied_investment_gain = net_balance_change - net_cash_flow;

    Reconciliation {
        total_starting_balance,
        total_ending_balance,
        net_balance_change,
        net_cash_flow,
        implied_investment_gain,
    }
}

/// Which quarter (1-4) a calendar month (1-12) falls in.
pub fn quarter_of_month(month: u32) -> i32 {
    ((month as i32 - 1) / 3) + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_growth_when_balance_change_equals_net_cash_flow() {
        // Net cash flow: 10,000 - 6,000 - 1,000 = 3,000. Balances grew by
        // exactly that much, so nothing is left unexplained.
        let balances = vec![AccountBalanceChange {
            starting_balance: 100_000.0,
            ending_balance: 103_000.0,
        }];
        let result = reconcile(10_000.0, 6_000.0, 1_000.0, &balances);
        assert_eq!(result.total_starting_balance, 100_000.0);
        assert_eq!(result.total_ending_balance, 103_000.0);
        assert_eq!(result.net_balance_change, 3_000.0);
        assert_eq!(result.net_cash_flow, 3_000.0);
        assert_eq!(result.implied_investment_gain, 0.0);
    }

    #[test]
    fn positive_growth_shows_up_as_implied_investment_gain() {
        // Net cash flow: 10,000 - 6,000 - 1,000 = 3,000. Balances grew by
        // 3,500, so 500 is unexplained investment gain.
        let balances = vec![AccountBalanceChange {
            starting_balance: 100_000.0,
            ending_balance: 103_500.0,
        }];
        let result = reconcile(10_000.0, 6_000.0, 1_000.0, &balances);
        assert_eq!(result.net_balance_change, 3_500.0);
        assert_eq!(result.net_cash_flow, 3_000.0);
        assert_eq!(result.implied_investment_gain, 500.0);
    }

    #[test]
    fn shortfall_shows_up_as_implied_investment_loss() {
        // Net cash flow: 10,000 - 6,000 - 1,000 = 3,000, but balances only
        // grew by 1,000 — 2,000 less than cash flow would suggest, implying a
        // 2,000 investment loss.
        let balances = vec![AccountBalanceChange {
            starting_balance: 100_000.0,
            ending_balance: 101_000.0,
        }];
        let result = reconcile(10_000.0, 6_000.0, 1_000.0, &balances);
        assert_eq!(result.net_balance_change, 1_000.0);
        assert_eq!(result.net_cash_flow, 3_000.0);
        assert_eq!(result.implied_investment_gain, -2_000.0);
    }

    #[test]
    fn quarter_of_month_covers_all_twelve_months() {
        let expected = [
            (1, 1),
            (2, 1),
            (3, 1),
            (4, 2),
            (5, 2),
            (6, 2),
            (7, 3),
            (8, 3),
            (9, 3),
            (10, 4),
            (11, 4),
            (12, 4),
        ];
        for (month, quarter) in expected {
            assert_eq!(quarter_of_month(month), quarter, "month {month}");
        }
    }
}
