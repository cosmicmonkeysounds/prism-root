//! `domain::ledger` — double-entry bookkeeping engine.
//!
//! Port of `@core/ledger` at commit 8426588. Splits the original
//! TypeScript module into four sub-modules:
//!
//! - [`types`] — core data types (accounts, entries, transactions,
//!   balances, queries).
//! - [`engine`] — `LedgerBook` generic engine, `LedgerAdapter`
//!   trait, and `MemoryLedgerAdapter`.
//! - [`currency`] — currency metadata, formatting, parsing, and
//!   rounding.
//! - [`finance`] — TVM calculations, line items, amortization,
//!   depreciation, recurrence helpers.

pub mod currency;
pub mod engine;
pub mod finance;
pub mod types;

pub use currency::{
    currency_decimals, format_compact, format_currency, parse_currency, round_currency,
    CurrencyInfo, COMMON_CURRENCIES,
};
pub use engine::{
    compute_balance, widget_contributions, AccountFilter, BalanceOptions, LedgerAdapter,
    LedgerBook, MemoryLedgerAdapter, NewAccount, NewEntry, NewTransaction, PostResult,
    PostTransactionResult, TransactionFilter, TransferOptions,
};
pub use finance::{
    advance_next_due, amortize, calc_line_totals, declining_balance_depreciation, fv, irr,
    next_due_dates, npv, pmt, pv, straight_line_depreciation, xirr, xnpv, AmortizationRow,
    DatedCashFlow, DepreciationRow, LineItem, LineTotals,
};
pub use types::{
    AccountBalance, AccountClass, AccountDimension, DimensionKind, LedgerAccount, LedgerDirection,
    LedgerEntry, LedgerQuery, LedgerTransaction,
};
