//! `language/markdown` — the Markdown `LanguageContribution`.
//!
//! Port of `packages/prism-core/src/language/markdown/contribution.ts`
//! from pre-Rust commit `8426588`. Mirrors the shape of the sibling
//! Luau contribution: a single `create_markdown_contribution()` entry
//! point that the unified `LanguageRegistry` consumes.
//!
//! The block parser reused here comes from
//! [`crate::language::forms::markdown`] so there is exactly one
//! markdown tokenizer in the codebase — matching the rule baked into
//! the TS tree's `parseMarkdown` re-export from `@prism/core/forms`.
//! Each block token becomes a child `SyntaxNode` on the emitted
//! [`RootNode`] with `kind` = the block kind (`h1` / `p` / `li` /
//! `code` / …) and its textual payload in the `value` slot, matching
//! the legacy projection byte-for-byte so renderers that walk
//! `root.children` keep working after the port.

pub mod contribution;

pub use contribution::{create_markdown_contribution, MARKDOWN_EXTENSIONS, MARKDOWN_ID};
