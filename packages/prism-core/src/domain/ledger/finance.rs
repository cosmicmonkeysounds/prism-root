//! Financial calculations: TVM, line items, amortization, depreciation.
//!
//! Port of `@core/ledger/finance` at commit 8426588. All functions
//! are pure — no side effects or stored state. TVM functions are
//! Excel-compatible.

use chrono::NaiveDate;
use serde_json::Value;

use super::currency::round_currency;

// ── Line Items ────────────────────────────────────────────────────

/// A single billable/invoiceable line.
#[derive(Debug, Clone)]
pub struct LineItem {
    pub quantity: f64,
    pub amount: f64,
    pub tax_rate: Option<f64>,
    pub discount: Option<f64>,
    pub meta: Option<Value>,
}

/// Aggregated totals for a set of line items.
#[derive(Debug, Clone)]
pub struct LineTotals {
    pub subtotal: f64,
    pub tax: f64,
    pub total: f64,
}

/// Compute subtotal, tax, and total from a slice of line items.
pub fn calc_line_totals(items: &[LineItem], currency: Option<&str>) -> LineTotals {
    let mut subtotal = 0.0;
    let mut tax = 0.0;

    for item in items {
        let line_amount = item.quantity * item.amount;
        let after_discount = match item.discount {
            Some(d) => line_amount * (1.0 - d),
            None => line_amount,
        };
        subtotal += after_discount;
        if let Some(rate) = item.tax_rate {
            tax += after_discount * rate;
        }
    }

    let cur = currency.unwrap_or("USD");
    let subtotal = round_currency(subtotal, cur);
    let tax = round_currency(tax, cur);
    let total = round_currency(subtotal + tax, cur);

    LineTotals {
        subtotal,
        tax,
        total,
    }
}

// ── TVM — Time Value of Money ─────────────────────────────────────

/// Future value (Excel FV). `annuity_due` shifts payments to start
/// of period.
pub fn fv(rate: f64, nper: f64, pmt: f64, pv: f64, annuity_due: bool) -> f64 {
    if rate == 0.0 {
        return -(pv + pmt * nper);
    }
    let pow = (1.0 + rate).powf(nper);
    let factor = if annuity_due { 1.0 + rate } else { 1.0 };
    -(pv * pow + pmt * factor * (pow - 1.0) / rate)
}

/// Present value (Excel PV).
pub fn pv(rate: f64, nper: f64, pmt: f64, fv_val: f64, annuity_due: bool) -> f64 {
    if rate == 0.0 {
        return -(fv_val + pmt * nper);
    }
    let pow = (1.0 + rate).powf(nper);
    let factor = if annuity_due { 1.0 + rate } else { 1.0 };
    -(fv_val + pmt * factor * (pow - 1.0) / rate) / pow
}

/// Payment (Excel PMT).
pub fn pmt(rate: f64, nper: f64, pv_val: f64, fv_val: f64, annuity_due: bool) -> f64 {
    if rate == 0.0 {
        return -(pv_val + fv_val) / nper;
    }
    let pow = (1.0 + rate).powf(nper);
    let factor = if annuity_due { 1.0 + rate } else { 1.0 };
    -(pv_val * pow + fv_val) * rate / (factor * (pow - 1.0))
}

/// Net present value (Excel NPV). `cash_flows[0]` is period 1.
pub fn npv(rate: f64, cash_flows: &[f64]) -> f64 {
    cash_flows
        .iter()
        .enumerate()
        .map(|(i, cf)| cf / (1.0 + rate).powi(i as i32 + 1))
        .sum()
}

/// Internal rate of return (Newton-Raphson, max 100 iterations).
pub fn irr(cash_flows: &[f64], guess: f64) -> Option<f64> {
    let mut rate = guess;
    for _ in 0..100 {
        let mut npv_val = 0.0;
        let mut d_npv = 0.0;
        for (i, cf) in cash_flows.iter().enumerate() {
            let t = i as f64;
            let denom = (1.0 + rate).powf(t);
            if denom == 0.0 {
                return None;
            }
            npv_val += cf / denom;
            d_npv -= t * cf / (1.0 + rate).powf(t + 1.0);
        }
        if d_npv.abs() < 1e-15 {
            return None;
        }
        let new_rate = rate - npv_val / d_npv;
        if (new_rate - rate).abs() < 1e-10 {
            return Some(new_rate);
        }
        rate = new_rate;
    }
    None
}

/// A cash flow with an associated date (for XNPV/XIRR).
#[derive(Debug, Clone)]
pub struct DatedCashFlow {
    pub amount: f64,
    pub date: NaiveDate,
}

/// Net present value with irregular dates (Excel XNPV).
pub fn xnpv(rate: f64, cash_flows: &[DatedCashFlow]) -> f64 {
    if cash_flows.is_empty() {
        return 0.0;
    }
    let d0 = cash_flows[0].date;
    cash_flows
        .iter()
        .map(|cf| {
            let days = (cf.date - d0).num_days() as f64;
            cf.amount / (1.0 + rate).powf(days / 365.0)
        })
        .sum()
}

/// Internal rate of return with irregular dates (Excel XIRR).
pub fn xirr(cash_flows: &[DatedCashFlow], guess: f64) -> Option<f64> {
    if cash_flows.is_empty() {
        return None;
    }
    let d0 = cash_flows[0].date;
    let mut rate = guess;

    for _ in 0..100 {
        let mut npv_val = 0.0;
        let mut d_npv = 0.0;
        for cf in cash_flows {
            let t = (cf.date - d0).num_days() as f64 / 365.0;
            let denom = (1.0 + rate).powf(t);
            if denom == 0.0 {
                return None;
            }
            npv_val += cf.amount / denom;
            d_npv -= t * cf.amount / (1.0 + rate).powf(t + 1.0);
        }
        if d_npv.abs() < 1e-15 {
            return None;
        }
        let new_rate = rate - npv_val / d_npv;
        if (new_rate - rate).abs() < 1e-10 {
            return Some(new_rate);
        }
        rate = new_rate;
    }
    None
}

// ── Amortization ──────────────────────────────────────────────────

/// A single row of an amortization schedule.
#[derive(Debug, Clone)]
pub struct AmortizationRow {
    pub period: u32,
    pub payment: f64,
    pub principal: f64,
    pub interest: f64,
    pub balance: f64,
}

/// Generate an amortization schedule for a fixed-rate loan.
pub fn amortize(
    principal: f64,
    annual_rate: f64,
    periods: u32,
    currency: Option<&str>,
) -> Vec<AmortizationRow> {
    if periods == 0 {
        return Vec::new();
    }
    let cur = currency.unwrap_or("USD");
    let monthly_rate = annual_rate / 12.0;
    let payment = if monthly_rate == 0.0 {
        principal / periods as f64
    } else {
        -pmt(monthly_rate, periods as f64, principal, 0.0, false)
    };
    let payment = round_currency(payment, cur);

    let mut balance = principal;
    let mut rows = Vec::with_capacity(periods as usize);

    for p in 1..=periods {
        let interest = round_currency(balance * monthly_rate, cur);
        let mut principal_part = round_currency(payment - interest, cur);

        // Last period: clean up rounding remainder.
        if p == periods {
            principal_part = round_currency(balance, cur);
        }

        balance = round_currency(balance - principal_part, cur);

        rows.push(AmortizationRow {
            period: p,
            payment: if p == periods {
                round_currency(principal_part + interest, cur)
            } else {
                payment
            },
            principal: principal_part,
            interest,
            balance: if p == periods { 0.0 } else { balance },
        });
    }

    rows
}

// ── Depreciation ──────────────────────────────────────────────────

/// A single year in a depreciation schedule.
#[derive(Debug, Clone)]
pub struct DepreciationRow {
    pub year: u32,
    pub depreciation_amount: f64,
    pub accumulated_depreciation: f64,
    pub book_value: f64,
}

/// Straight-line depreciation schedule.
pub fn straight_line_depreciation(
    cost: f64,
    salvage: f64,
    useful_life_years: u32,
) -> Vec<DepreciationRow> {
    if useful_life_years == 0 {
        return Vec::new();
    }
    let depreciable = cost - salvage;
    let annual = depreciable / useful_life_years as f64;
    let mut accumulated = 0.0;
    let mut rows = Vec::with_capacity(useful_life_years as usize);

    for y in 1..=useful_life_years {
        accumulated += annual;
        rows.push(DepreciationRow {
            year: y,
            depreciation_amount: annual,
            accumulated_depreciation: accumulated,
            book_value: cost - accumulated,
        });
    }

    rows
}

/// Double-declining-balance depreciation schedule.
pub fn declining_balance_depreciation(
    cost: f64,
    salvage: f64,
    useful_life_years: u32,
) -> Vec<DepreciationRow> {
    if useful_life_years == 0 {
        return Vec::new();
    }
    let rate = 2.0 / useful_life_years as f64;
    let mut book_value = cost;
    let mut accumulated = 0.0;
    let mut rows = Vec::with_capacity(useful_life_years as usize);

    for y in 1..=useful_life_years {
        let mut dep = book_value * rate;
        // Don't depreciate below salvage.
        if book_value - dep < salvage {
            dep = book_value - salvage;
        }
        if dep < 0.0 {
            dep = 0.0;
        }
        accumulated += dep;
        book_value -= dep;
        rows.push(DepreciationRow {
            year: y,
            depreciation_amount: dep,
            accumulated_depreciation: accumulated,
            book_value,
        });
    }

    rows
}

// ── Recurrence Helpers ────────────────────────────────────────────

/// Advance a date string by a named interval. Supports `daily`,
/// `weekly`, `biweekly`, `monthly`, `quarterly`, `yearly`.
pub fn advance_next_due(current_due: &str, interval: &str) -> Option<String> {
    let date = NaiveDate::parse_from_str(current_due, "%Y-%m-%d").ok()?;
    let next = match interval {
        "daily" => date.succ_opt()?,
        "weekly" => date.checked_add_days(chrono::Days::new(7))?,
        "biweekly" => date.checked_add_days(chrono::Days::new(14))?,
        "monthly" => add_months(date, 1)?,
        "quarterly" => add_months(date, 3)?,
        "yearly" => add_months(date, 12)?,
        _ => return None,
    };
    Some(next.format("%Y-%m-%d").to_string())
}

/// Generate the next `count` due dates starting from `from`.
pub fn next_due_dates(from: &str, interval: &str, count: usize) -> Vec<String> {
    let mut dates = Vec::with_capacity(count);
    let mut current = from.to_string();
    for _ in 0..count {
        match advance_next_due(&current, interval) {
            Some(next) => {
                dates.push(next.clone());
                current = next;
            }
            None => break,
        }
    }
    dates
}

/// Add calendar months to a date, clamping to end-of-month.
fn add_months(date: NaiveDate, months: u32) -> Option<NaiveDate> {
    let total_months = date.month0() + months;
    let new_year = date.year() + (total_months / 12) as i32;
    let new_month = (total_months % 12) + 1;
    // Try the same day; if it doesn't exist, clamp to last day of month.
    let day = date.day();
    NaiveDate::from_ymd_opt(new_year, new_month, day).or_else(|| {
        // Walk backwards to find the last valid day.
        for d in (28..day).rev() {
            if let Some(nd) = NaiveDate::from_ymd_opt(new_year, new_month, d) {
                return Some(nd);
            }
        }
        NaiveDate::from_ymd_opt(new_year, new_month, 28)
    })
}

use chrono::Datelike;

#[cfg(test)]
mod tests {
    use super::*;

    // ── Line Items ────────────────────────────────────────────────

    #[test]
    fn line_totals_simple() {
        let items = vec![
            LineItem {
                quantity: 2.0,
                amount: 50.0,
                tax_rate: None,
                discount: None,
                meta: None,
            },
            LineItem {
                quantity: 1.0,
                amount: 30.0,
                tax_rate: None,
                discount: None,
                meta: None,
            },
        ];
        let totals = calc_line_totals(&items, Some("USD"));
        assert_eq!(totals.subtotal, 130.0);
        assert_eq!(totals.tax, 0.0);
        assert_eq!(totals.total, 130.0);
    }

    #[test]
    fn line_totals_with_tax() {
        let items = vec![LineItem {
            quantity: 1.0,
            amount: 100.0,
            tax_rate: Some(0.1),
            discount: None,
            meta: None,
        }];
        let totals = calc_line_totals(&items, Some("USD"));
        assert_eq!(totals.subtotal, 100.0);
        assert_eq!(totals.tax, 10.0);
        assert_eq!(totals.total, 110.0);
    }

    #[test]
    fn line_totals_with_discount() {
        let items = vec![LineItem {
            quantity: 1.0,
            amount: 100.0,
            tax_rate: Some(0.1),
            discount: Some(0.2),
            meta: None,
        }];
        let totals = calc_line_totals(&items, Some("USD"));
        assert_eq!(totals.subtotal, 80.0);
        assert_eq!(totals.tax, 8.0);
        assert_eq!(totals.total, 88.0);
    }

    #[test]
    fn line_totals_empty() {
        let totals = calc_line_totals(&[], Some("USD"));
        assert_eq!(totals.total, 0.0);
    }

    // ── TVM ───────────────────────────────────────────────────────

    #[test]
    fn fv_basic() {
        // $1000 at 5% for 10 years, no payments.
        let result = fv(0.05, 10.0, 0.0, -1000.0, false);
        assert!((result - 1628.89).abs() < 0.01);
    }

    #[test]
    fn fv_zero_rate() {
        let result = fv(0.0, 10.0, -100.0, -1000.0, false);
        assert!((result - 2000.0).abs() < 0.01);
    }

    #[test]
    fn pv_basic() {
        // What PV yields $1628.89 at 5% over 10 years?
        let result = pv(0.05, 10.0, 0.0, -1628.89, false);
        assert!((result - 1000.0).abs() < 0.01);
    }

    #[test]
    fn pmt_basic() {
        // $200,000 mortgage at 6% over 30 years (360 monthly payments).
        let result = pmt(0.06 / 12.0, 360.0, 200000.0, 0.0, false);
        assert!((result - (-1199.10)).abs() < 0.1);
    }

    #[test]
    fn npv_basic() {
        // Investment: -1000 upfront, then 400, 400, 400 at 10%.
        let flows = [-1000.0, 400.0, 400.0, 400.0];
        let result = npv(0.10, &flows);
        assert!((result - (-5.26)).abs() < 0.5);
    }

    #[test]
    fn irr_converges() {
        let flows = vec![-1000.0, 400.0, 400.0, 400.0];
        let result = irr(&flows, 0.1).unwrap();
        // Verify by checking NPV ≈ 0 at the computed rate.
        let check_npv: f64 = flows
            .iter()
            .enumerate()
            .map(|(i, cf)| cf / (1.0 + result).powi(i as i32))
            .sum();
        assert!(check_npv.abs() < 0.01);
    }

    #[test]
    fn xnpv_basic() {
        let flows = vec![
            DatedCashFlow {
                amount: -1000.0,
                date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            },
            DatedCashFlow {
                amount: 500.0,
                date: NaiveDate::from_ymd_opt(2026, 7, 1).unwrap(),
            },
            DatedCashFlow {
                amount: 600.0,
                date: NaiveDate::from_ymd_opt(2027, 1, 1).unwrap(),
            },
        ];
        let result = xnpv(0.10, &flows);
        assert!(result > 0.0); // profitable at 10%
    }

    #[test]
    fn xirr_converges() {
        let flows = vec![
            DatedCashFlow {
                amount: -1000.0,
                date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            },
            DatedCashFlow {
                amount: 1100.0,
                date: NaiveDate::from_ymd_opt(2027, 1, 1).unwrap(),
            },
        ];
        let result = xirr(&flows, 0.1).unwrap();
        // ~10% annual return.
        assert!((result - 0.10).abs() < 0.01);
    }

    // ── Amortization ──────────────────────────────────────────────

    #[test]
    fn amortize_sum_of_principal() {
        let rows = amortize(10000.0, 0.06, 12, Some("USD"));
        let total_principal: f64 = rows.iter().map(|r| r.principal).sum();
        assert!((total_principal - 10000.0).abs() < 0.02);
    }

    #[test]
    fn amortize_all_payments_equal_except_last() {
        let rows = amortize(10000.0, 0.06, 12, Some("USD"));
        let first_payment = rows[0].payment;
        // All but the last should be the same.
        for row in &rows[..rows.len() - 1] {
            assert!((row.payment - first_payment).abs() < 0.01);
        }
    }

    #[test]
    fn amortize_final_balance_zero() {
        let rows = amortize(10000.0, 0.06, 12, Some("USD"));
        assert!((rows.last().unwrap().balance).abs() < 0.01);
    }

    #[test]
    fn amortize_zero_rate() {
        let rows = amortize(1200.0, 0.0, 12, Some("USD"));
        for row in &rows {
            assert!((row.payment - 100.0).abs() < 0.01);
            assert_eq!(row.interest, 0.0);
        }
    }

    // ── Depreciation ──────────────────────────────────────────────

    #[test]
    fn straight_line_uniform() {
        let rows = straight_line_depreciation(10000.0, 1000.0, 5);
        let expected = 1800.0;
        for row in &rows {
            assert!((row.depreciation_amount - expected).abs() < 0.01);
        }
    }

    #[test]
    fn straight_line_final_book_value() {
        let rows = straight_line_depreciation(10000.0, 1000.0, 5);
        let last = rows.last().unwrap();
        assert!((last.book_value - 1000.0).abs() < 0.01);
    }

    #[test]
    fn declining_balance_decreasing() {
        let rows = declining_balance_depreciation(10000.0, 1000.0, 5);
        for i in 1..rows.len() {
            assert!(rows[i].depreciation_amount <= rows[i - 1].depreciation_amount + 0.01);
        }
    }

    #[test]
    fn declining_balance_final_above_salvage() {
        let rows = declining_balance_depreciation(10000.0, 1000.0, 5);
        let last = rows.last().unwrap();
        assert!(last.book_value >= 1000.0 - 0.01);
    }

    // ── Recurrence ────────────────────────────────────────────────

    #[test]
    fn advance_daily() {
        assert_eq!(
            advance_next_due("2026-01-15", "daily"),
            Some("2026-01-16".into())
        );
    }

    #[test]
    fn advance_weekly() {
        assert_eq!(
            advance_next_due("2026-01-15", "weekly"),
            Some("2026-01-22".into())
        );
    }

    #[test]
    fn advance_monthly() {
        assert_eq!(
            advance_next_due("2026-01-31", "monthly"),
            Some("2026-02-28".into())
        );
    }

    #[test]
    fn advance_quarterly() {
        assert_eq!(
            advance_next_due("2026-01-15", "quarterly"),
            Some("2026-04-15".into())
        );
    }

    #[test]
    fn advance_yearly() {
        assert_eq!(
            advance_next_due("2026-02-28", "yearly"),
            Some("2027-02-28".into())
        );
    }

    #[test]
    fn advance_unknown_interval() {
        assert_eq!(advance_next_due("2026-01-15", "unknown"), None);
    }

    #[test]
    fn next_due_dates_monthly() {
        let dates = next_due_dates("2026-01-15", "monthly", 3);
        assert_eq!(dates, vec!["2026-02-15", "2026-03-15", "2026-04-15"]);
    }
}
