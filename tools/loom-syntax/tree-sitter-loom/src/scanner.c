// tree-sitter-loom external scanner.
//
// Emits three synthetic tokens used by grammar.js:
//
//   _newline       — end of a logical line (suppressed via valid_symbols when
//                    the grammar is inside ( ) [ ] { } and doesn't want one).
//   _indent        — leading indent at start of a logical line increased.
//   _dedent        — leading indent at start of a logical line decreased.
//
// Loom is indent-sensitive (docs/dev/loom-grammar.md §2.3): the lexer emits
// synthetic INDENT/DEDENT tokens when leading whitespace changes, Python-style.
// Inside bracketed forms (s-expressions, dict literals, etc.) the grammar
// simply doesn't reference _newline in valid_symbols, so the scanner sees
// nothing to do and the lexer's built-in whitespace handling takes over.
//
// Also implements a final `_error_sentinel` symbol so the scanner can detect
// error-recovery mode and bow out gracefully (idiomatic tree-sitter pattern).

#include "tree_sitter/parser.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <wctype.h>

enum TokenType {
    NEWLINE,
    INDENT,
    DEDENT,
    ERROR_SENTINEL,
};

typedef struct {
    // Stack of indent column widths. Index 0 is always 0 (column-0 baseline).
    // Push when indent grows, pop when it shrinks.
    uint16_t *indents;
    uint32_t indents_len;
    uint32_t indents_cap;
} Scanner;

static inline void skip(TSLexer *lexer) { lexer->advance(lexer, true); }

static void push_indent(Scanner *s, uint16_t width) {
    if (s->indents_len == s->indents_cap) {
        uint32_t new_cap = s->indents_cap ? s->indents_cap * 2 : 8;
        s->indents = realloc(s->indents, new_cap * sizeof(uint16_t));
        s->indents_cap = new_cap;
    }
    s->indents[s->indents_len++] = width;
}

static uint16_t top_indent(const Scanner *s) {
    return s->indents_len ? s->indents[s->indents_len - 1] : 0;
}

void *tree_sitter_loom_external_scanner_create(void) {
    Scanner *s = calloc(1, sizeof(Scanner));
    push_indent(s, 0);
    return s;
}

void tree_sitter_loom_external_scanner_destroy(void *payload) {
    Scanner *s = (Scanner *)payload;
    free(s->indents);
    free(s);
}

unsigned tree_sitter_loom_external_scanner_serialize(void *payload, char *buffer) {
    Scanner *s = (Scanner *)payload;
    unsigned written = 0;

    uint16_t len = (uint16_t)s->indents_len;
    if (written + sizeof(uint16_t) > TREE_SITTER_SERIALIZATION_BUFFER_SIZE) return 0;
    memcpy(buffer + written, &len, sizeof(uint16_t));
    written += sizeof(uint16_t);

    for (uint32_t i = 0; i < s->indents_len; i++) {
        if (written + sizeof(uint16_t) > TREE_SITTER_SERIALIZATION_BUFFER_SIZE) return 0;
        memcpy(buffer + written, &s->indents[i], sizeof(uint16_t));
        written += sizeof(uint16_t);
    }

    return written;
}

void tree_sitter_loom_external_scanner_deserialize(void *payload, const char *buffer, unsigned length) {
    Scanner *s = (Scanner *)payload;
    s->indents_len = 0;

    if (length == 0) {
        push_indent(s, 0);
        return;
    }

    unsigned read = 0;
    uint16_t len = 0;
    memcpy(&len, buffer + read, sizeof(uint16_t));
    read += sizeof(uint16_t);

    for (uint16_t i = 0; i < len; i++) {
        uint16_t v = 0;
        memcpy(&v, buffer + read, sizeof(uint16_t));
        read += sizeof(uint16_t);
        push_indent(s, v);
    }
}

// Skip a `// ...` line comment up to but not including the EOL.
static void skip_line_comment(TSLexer *lexer) {
    while (lexer->lookahead != 0 && lexer->lookahead != '\n' && lexer->lookahead != '\r') {
        skip(lexer);
    }
}

// Skip a `/* ... */` block comment. Caller has already consumed the leading
// `/*`. Returns true on a clean close, false on EOF.
static bool skip_block_comment(TSLexer *lexer) {
    while (lexer->lookahead != 0) {
        if (lexer->lookahead == '*') {
            skip(lexer);
            if (lexer->lookahead == '/') {
                skip(lexer);
                return true;
            }
        } else {
            skip(lexer);
        }
    }
    return false;
}

bool tree_sitter_loom_external_scanner_scan(void *payload, TSLexer *lexer,
                                            const bool *valid_symbols) {
    Scanner *s = (Scanner *)payload;

    // Tree-sitter sets every symbol valid (including ERROR_SENTINEL) during
    // panic recovery. Surrender — let the parser resync.
    if (valid_symbols[ERROR_SENTINEL]) {
        return false;
    }

    bool want_newline = valid_symbols[NEWLINE];
    bool want_indent  = valid_symbols[INDENT];
    bool want_dedent  = valid_symbols[DEDENT];

    if (!want_newline && !want_indent && !want_dedent) {
        return false;
    }

    // Walk whitespace + comments + continuations, tracking whether we crossed
    // a logical-line boundary and what column we landed at.
    bool saw_newline = false;
    uint16_t indent_width = 0;

    for (;;) {
        int32_t ch = lexer->lookahead;

        if (ch == ' ' || ch == '\t') {
            if (saw_newline) indent_width++;
            skip(lexer);
        } else if (ch == '\r') {
            skip(lexer);
        } else if (ch == '\n') {
            saw_newline = true;
            indent_width = 0;
            skip(lexer);
        } else if (ch == '\\') {
            // Explicit line continuation: `\\\n` eats both.
            skip(lexer);
            if (lexer->lookahead == '\r') skip(lexer);
            if (lexer->lookahead == '\n') {
                skip(lexer);
                // Continuation does NOT reset saw_newline / indent_width —
                // we're still on the same logical line.
            } else {
                return false;
            }
        } else if (ch == '/') {
            // Possible `//` or `/*`.
            lexer->mark_end(lexer);
            skip(lexer);
            if (lexer->lookahead == '/') {
                skip(lexer);
                skip_line_comment(lexer);
            } else if (lexer->lookahead == '*') {
                skip(lexer);
                if (!skip_block_comment(lexer)) return false;
            } else {
                // Lone `/`: not whitespace. Bail to the lexer.
                return false;
            }
        } else if (ch == 0) {
            break;
        } else {
            break;
        }
    }

    if (!saw_newline && lexer->lookahead != 0) {
        return false;
    }

    lexer->mark_end(lexer);

    uint16_t current = top_indent(s);

    // EOF: drain dedents back to the column-0 baseline, then emit a final
    // synthetic newline.
    if (lexer->lookahead == 0) {
        if (s->indents_len > 1 && want_dedent) {
            s->indents_len--;
            lexer->result_symbol = DEDENT;
            return true;
        }
        if (want_newline) {
            lexer->result_symbol = NEWLINE;
            return true;
        }
        return false;
    }

    if (indent_width > current && want_indent) {
        push_indent(s, indent_width);
        lexer->result_symbol = INDENT;
        return true;
    }

    if (indent_width < current && want_dedent) {
        s->indents_len--;
        lexer->result_symbol = DEDENT;
        return true;
    }

    if (want_newline) {
        lexer->result_symbol = NEWLINE;
        return true;
    }

    return false;
}
