//! Currency formatting and rounding utilities.
//!
//! Port of `@core/ledger/currency` at commit 8426588. Provides
//! currency metadata, formatting (standard and compact), parsing,
//! and precision-aware rounding.

// ── Currency Info ─────────────────────────────────────────────────

/// Static metadata for a currency.
pub struct CurrencyInfo {
    pub code: &'static str,
    pub label: &'static str,
    pub symbol: &'static str,
    pub decimals: u8,
}

/// Common world currencies.
pub const COMMON_CURRENCIES: &[CurrencyInfo] = &[
    CurrencyInfo {
        code: "USD",
        label: "US Dollar",
        symbol: "$",
        decimals: 2,
    },
    CurrencyInfo {
        code: "EUR",
        label: "Euro",
        symbol: "€",
        decimals: 2,
    },
    CurrencyInfo {
        code: "GBP",
        label: "British Pound",
        symbol: "£",
        decimals: 2,
    },
    CurrencyInfo {
        code: "JPY",
        label: "Japanese Yen",
        symbol: "¥",
        decimals: 0,
    },
    CurrencyInfo {
        code: "CHF",
        label: "Swiss Franc",
        symbol: "CHF",
        decimals: 2,
    },
    CurrencyInfo {
        code: "CAD",
        label: "Canadian Dollar",
        symbol: "CA$",
        decimals: 2,
    },
    CurrencyInfo {
        code: "AUD",
        label: "Australian Dollar",
        symbol: "A$",
        decimals: 2,
    },
    CurrencyInfo {
        code: "NZD",
        label: "New Zealand Dollar",
        symbol: "NZ$",
        decimals: 2,
    },
    CurrencyInfo {
        code: "CNY",
        label: "Chinese Yuan",
        symbol: "¥",
        decimals: 2,
    },
    CurrencyInfo {
        code: "INR",
        label: "Indian Rupee",
        symbol: "₹",
        decimals: 2,
    },
    CurrencyInfo {
        code: "BRL",
        label: "Brazilian Real",
        symbol: "R$",
        decimals: 2,
    },
    CurrencyInfo {
        code: "KRW",
        label: "South Korean Won",
        symbol: "₩",
        decimals: 0,
    },
    CurrencyInfo {
        code: "MXN",
        label: "Mexican Peso",
        symbol: "MX$",
        decimals: 2,
    },
    CurrencyInfo {
        code: "SEK",
        label: "Swedish Krona",
        symbol: "kr",
        decimals: 2,
    },
    CurrencyInfo {
        code: "NOK",
        label: "Norwegian Krone",
        symbol: "kr",
        decimals: 2,
    },
    CurrencyInfo {
        code: "DKK",
        label: "Danish Krone",
        symbol: "kr",
        decimals: 2,
    },
    CurrencyInfo {
        code: "SGD",
        label: "Singapore Dollar",
        symbol: "S$",
        decimals: 2,
    },
    CurrencyInfo {
        code: "HKD",
        label: "Hong Kong Dollar",
        symbol: "HK$",
        decimals: 2,
    },
    CurrencyInfo {
        code: "ZAR",
        label: "South African Rand",
        symbol: "R",
        decimals: 2,
    },
];

// ── Lookup ────────────────────────────────────────────────────────

fn find_currency(code: &str) -> Option<&'static CurrencyInfo> {
    let upper = code.to_uppercase();
    COMMON_CURRENCIES.iter().find(|c| c.code == upper)
}

/// Returns the number of decimal places for a currency code.
/// Unknown currencies default to 2.
pub fn currency_decimals(currency: &str) -> u8 {
    find_currency(currency).map_or(2, |c| c.decimals)
}

// ── Rounding ──────────────────────────────────────────────────────

/// Round a value to the precision of the given currency.
pub fn round_currency(amount: f64, currency: &str) -> f64 {
    let decimals = currency_decimals(currency);
    let factor = 10_f64.powi(decimals as i32);
    (amount * factor).round() / factor
}

// ── Formatting ────────────────────────────────────────────────────

/// Format an amount with the currency symbol and appropriate decimals.
///
/// Examples: `$1,234.56`, `¥1,235`, `€100.00`.
pub fn format_currency(amount: f64, currency: &str) -> String {
    let decimals = currency_decimals(currency);
    let symbol = find_currency(currency).map_or(currency, |c| c.symbol);
    let rounded = round_currency(amount, currency);
    let abs = rounded.abs();

    let formatted = format_with_commas(abs, decimals);
    if rounded < 0.0 {
        format!("-{symbol}{formatted}")
    } else {
        format!("{symbol}{formatted}")
    }
}

/// Format a number with thousands separators and fixed decimal places.
fn format_with_commas(value: f64, decimals: u8) -> String {
    let fixed = format!("{:.prec$}", value, prec = decimals as usize);
    let parts: Vec<&str> = fixed.split('.').collect();
    let int_part = parts[0];

    // Insert commas into the integer part.
    let mut with_commas = String::new();
    for (i, ch) in int_part.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            with_commas.push(',');
        }
        with_commas.push(ch);
    }
    let int_with_commas: String = with_commas.chars().rev().collect();

    if decimals == 0 {
        int_with_commas
    } else {
        format!("{int_with_commas}.{}", parts[1])
    }
}

/// Format an amount in compact notation (`$1.5M`, `$250K`, etc.).
pub fn format_compact(amount: f64, currency: &str) -> String {
    let symbol = find_currency(currency).map_or(currency, |c| c.symbol);
    let abs = amount.abs();
    let sign = if amount < 0.0 { "-" } else { "" };

    let (scaled, suffix) = if abs >= 1_000_000_000.0 {
        (abs / 1_000_000_000.0, "B")
    } else if abs >= 1_000_000.0 {
        (abs / 1_000_000.0, "M")
    } else if abs >= 1_000.0 {
        (abs / 1_000.0, "K")
    } else {
        return format_currency(amount, currency);
    };

    // Remove trailing zeros after the decimal.
    let num = format!("{scaled:.1}");
    let num = num.trim_end_matches('0').trim_end_matches('.');
    format!("{sign}{symbol}{num}{suffix}")
}

// ── Parsing ───────────────────────────────────────────────────────

/// Parse a currency string into a numeric value, stripping symbols
/// and commas. Returns `None` if the remaining string is not a
/// valid number.
pub fn parse_currency(value: &str) -> Option<f64> {
    let stripped: String = value
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    stripped.parse::<f64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── currency_decimals ─────────────────────────────────────────

    #[test]
    fn decimals_usd() {
        assert_eq!(currency_decimals("USD"), 2);
    }

    #[test]
    fn decimals_jpy() {
        assert_eq!(currency_decimals("JPY"), 0);
    }

    #[test]
    fn decimals_krw() {
        assert_eq!(currency_decimals("KRW"), 0);
    }

    #[test]
    fn decimals_unknown_defaults_to_2() {
        assert_eq!(currency_decimals("XYZ"), 2);
    }

    #[test]
    fn decimals_case_insensitive() {
        assert_eq!(currency_decimals("usd"), 2);
        assert_eq!(currency_decimals("jpy"), 0);
    }

    // ── format_currency ───────────────────────────────────────────

    #[test]
    fn format_usd() {
        assert_eq!(format_currency(1234.56, "USD"), "$1,234.56");
    }

    #[test]
    fn format_usd_negative() {
        assert_eq!(format_currency(-1234.56, "USD"), "-$1,234.56");
    }

    #[test]
    fn format_eur() {
        assert_eq!(format_currency(100.0, "EUR"), "€100.00");
    }

    #[test]
    fn format_gbp() {
        assert_eq!(format_currency(42.5, "GBP"), "£42.50");
    }

    #[test]
    fn format_jpy_no_decimals() {
        assert_eq!(format_currency(1234.7, "JPY"), "¥1,235");
    }

    #[test]
    fn format_zero() {
        assert_eq!(format_currency(0.0, "USD"), "$0.00");
    }

    #[test]
    fn format_large_number() {
        assert_eq!(format_currency(1_000_000.0, "USD"), "$1,000,000.00");
    }

    // ── format_compact ────────────────────────────────────────────

    #[test]
    fn compact_millions() {
        assert_eq!(format_compact(1_500_000.0, "USD"), "$1.5M");
    }

    #[test]
    fn compact_billions() {
        assert_eq!(format_compact(2_000_000_000.0, "USD"), "$2B");
    }

    #[test]
    fn compact_thousands() {
        assert_eq!(format_compact(250_000.0, "USD"), "$250K");
    }

    #[test]
    fn compact_small_falls_through() {
        assert_eq!(format_compact(42.50, "USD"), "$42.50");
    }

    #[test]
    fn compact_negative() {
        assert_eq!(format_compact(-1_500_000.0, "USD"), "-$1.5M");
    }

    // ── parse_currency ────────────────────────────────────────────

    #[test]
    fn parse_plain_number() {
        assert_eq!(parse_currency("1234.56"), Some(1234.56));
    }

    #[test]
    fn parse_with_dollar_sign() {
        assert_eq!(parse_currency("$1,234.56"), Some(1234.56));
    }

    #[test]
    fn parse_with_euro_sign() {
        assert_eq!(parse_currency("€100.00"), Some(100.0));
    }

    #[test]
    fn parse_negative() {
        assert_eq!(parse_currency("-$500.00"), Some(-500.0));
    }

    #[test]
    fn parse_invalid() {
        assert_eq!(parse_currency("not a number"), None);
    }

    #[test]
    fn parse_empty() {
        assert_eq!(parse_currency(""), None);
    }

    // ── round_currency ────────────────────────────────────────────

    #[test]
    fn round_usd() {
        // 1.005 is a classic f64 edge case (1.005 * 100 = 100.4999...).
        // Test with a value that round-trips cleanly in IEEE 754.
        assert_eq!(round_currency(1.456, "USD"), 1.46);
        assert_eq!(round_currency(1.234, "USD"), 1.23);
    }

    #[test]
    fn round_jpy() {
        assert_eq!(round_currency(99.7, "JPY"), 100.0);
    }

    #[test]
    fn round_precision_preserved() {
        assert_eq!(round_currency(1.0, "USD"), 1.0);
    }
}
