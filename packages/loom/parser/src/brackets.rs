//! The five-bracket discipline (spec §5).
//!
//! Each bracket shape maps to one audience and one job — there is no
//! overlap. The parser dispatches on the open bracket and hands the
//! contents to the matching sub-grammar.
//!
//! * `( … )`      → [`crate::beats`] parenthetical (performer-facing)
//! * `{ … }`      → [`crate::beats`] interpolation (reader-facing)
//! * `[ … ]`      → [`crate::beats`] choice-only suppression
//! * `< … >`      → [`crate::directives`] Luau call or syntactic form
//! * ` ``` … ``` ` → [`crate::beats`] production-metadata sidecar
//!
//! Phase-1 stub.
