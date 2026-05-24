//! Angle-bracket directives `<kind: args>` (spec §14).
//!
//! Every directive is a call into the runtime's Luau registry —
//! `<sfx: bell>` invokes the `sfx` function. The parser does **not**
//! validate that `kind` exists; that's the resolver's job. A small
//! set of forms (`<if:>`, `<else if:>`, `<else>`, `<match:>`,
//! `<for:>`, `<each visit>`, `<after:>`, `<otherwise>`, `<anchor:>`,
//! `<let:>`) are syntactic — they affect parser scope and playhead
//! structure and so are baked into the grammar (spec §14.2).
//!
//! Phase-1 stub.
