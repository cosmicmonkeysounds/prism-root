/**
 * tree-sitter-loom — grammar.js
 *
 * A tree-sitter grammar for the Loom storytelling language.
 * Reference: /docs/dev/loom-grammar.md (§§1-22) and /docs/dev/loom-design.md.
 *
 * Scope: optimised for editor syntax highlighting (Zed, Neovim, Helix,
 * GitHub Linguist). Lexical accuracy is the priority — every keyword,
 * sigil, and inline marker has a dedicated node so highlights.scm can
 * target it. Some structural constraints (e.g. "no choices in :script
 * docs", "tunnel-return cycle") are deliberately the validator's job,
 * not the parser's.
 *
 * Indent-sensitivity (§2.3) is handled by src/scanner.c, which emits
 * synthetic _newline / _indent / _dedent tokens à la Python.
 */

const PREC = {
    // Expression operator precedence (§10).
    OR: 1,
    AND: 2,
    NOT: 3,
    EQ: 4,
    CMP: 5,
    MEMBER: 6,
    ADD: 7,
    MUL: 8,
    UNARY: 9,
    POSTFIX: 10,

    // Disambiguators for line classes.
    SPEAKER_LINE: 20,    // SPEAKER ... beats stage_direction
    SECTION: 21,         // -- beats minus-prefixed text
    PROPERTY: 22,        // .name beats free-form text
    DIVERT: 23,          // -> beats minus-arrow text
    SLUGLINE: 24,        // ## beats # heading
    ANNOTATION: 25,      // @vo beats text starting with @
};

// Comma-separated list with trailing-comma tolerance.
const sep1 = (rule, sep) => seq(rule, repeat(seq(sep, rule)));
const sep  = (rule, sep) => optional(sep1(rule, sep));

module.exports = grammar({
    name: 'loom',

    externals: $ => [
        $._newline,
        $._indent,
        $._dedent,
        $._error_sentinel,
    ],

    extras: $ => [
        /[ \t]+/,
        $.line_comment,
        $.block_comment,
    ],

    // The "word" rule is used by tree-sitter to detect keyword reservation.
    // Anonymous string nodes that match `identifier` are promoted to
    // dedicated tokens, which makes the lexer keyword-aware.
    word: $ => $.identifier,

    conflicts: $ => [],

    // Supertypes deliberately omitted: tree-sitter requires every supertype
    // child to be a single visible node, which would force us to wrap every
    // expression / content variant in a unique node type. The downstream
    // highlights don't depend on it.

    rules: {

        // ────────────────────────────────────────────────────────────────
        // §1. Source organization
        // ────────────────────────────────────────────────────────────────

        source_file: $ => seq(
            optional($.shebang),
            repeat($._document),
        ),

        shebang: $ => /#![^\n]*\n/,

        _document: $ => seq(
            $.header,
            optional($._property_block),
            optional($.docstring),
            repeat($._top_level_item),
        ),

        // ────────────────────────────────────────────────────────────────
        // §4. Document structure
        // ────────────────────────────────────────────────────────────────

        header: $ => seq(
            '#',
            field('name', $.identifier),
            optional(field('title', $.string)),
            repeat($.doc_tag),
            $._newline,
        ),

        doc_tag: $ => seq(':', $.identifier),

        _property_block: $ => seq(
            $._indent,
            repeat1($.property),
            $._dedent,
        ),

        property: $ => prec(PREC.PROPERTY, seq(
            '.',
            field('key', $.identifier),
            optional(field('value', $._property_value)),
            $._newline,
        )),

        // Per-property parse is registry-driven (grammar §4 note). For
        // highlighting we just capture the tail as a permissive payload.
        _property_value: $ => /[^\n]+/,

        docstring: $ => seq($._docstring_text, $._newline),

        _docstring_text: $ => token(seq(
            "'''",
            repeat(choice(
                /[^']/,
                /'[^']/,
                /''[^']/,
            )),
            "'''",
        )),

        // ────────────────────────────────────────────────────────────────
        // §4.2 Top-level items
        // ────────────────────────────────────────────────────────────────

        _top_level_item: $ => choice(
            $.section,
            $.slugline_scene,
            $.cast_decl,
            $.cue_decl,
            $.location_decl,
            $.cohort_decl,
            $.participant_lifecycle,
            $.location_event,
            $.broadcast_block,
            $.faction_event,
            $.participant_faction_lifecycle,
            $.discovery_event,
            $.inline_faction_decl,
            $.generator_decl,
            $.scene_decl,
            $.compose_decl,
            $.knowledge_block,
            $.goal_decl,
            $.disposition_block,
            $.hook_decl,
            $.axis_decl,
            $.pool_decl,
            $.stat_decl,
            $.attribute_decl,
            $.tree_node_decl,
            $.timecode_block,
            $.sexp,
            $.binding_line,
            $.action_line,
            $._newline,
        ),

        // ────────────────────────────────────────────────────────────────
        // §5. Performance-model declarations
        // ────────────────────────────────────────────────────────────────

        cast_decl: $ => seq(
            'cast',
            field('id', $._cast_id),
            optional(field('label', $.string)),
            $._newline,
            optional($._property_block),
        ),

        _cast_id: $ => choice(
            $.speaker,
            $.static_ref,
        ),

        cue_decl: $ => seq(
            'cue',
            field('name', $.identifier),
            $._newline,
            $._property_block,
        ),

        location_decl: $ => seq(
            'location',
            field('id', $._location_id),
            optional(field('label', $.string)),
            $._newline,
            $._property_block,
        ),

        _location_id: $ => choice($.speaker, $.static_ref),

        cohort_decl: $ => seq(
            'cohort',
            field('name', $.identifier),
            $._newline,
            $._property_block,
        ),

        participant_lifecycle: $ => seq(
            'when',
            'participant',
            field('verb', choice('joins', 'leaves')),
            optional(seq(':', field('cohort', $.identifier))),
            $._newline,
            optional($._body_block),
        ),

        location_event: $ => seq(
            'when',
            'participant',
            field('verb', choice('enters', 'exits')),
            field('location', $.static_ref),
            $._newline,
            optional($._body_block),
        ),

        broadcast_block: $ => seq(
            'broadcast',
            field('scope', $.broadcast_scope),
            $._newline,
            optional($._body_block),
        ),

        broadcast_scope: $ => sep1($._scope_atom, choice('and', 'but')),

        _scope_atom: $ => choice(
            seq(':', 'all'),
            seq(':', $.scope_filter),
        ),

        scope_filter: $ => choice(
            seq('cohort',      '(', $.identifier, ')'),
            seq('location',    '(', $.static_ref, ')'),
            seq('participant', '(', $.resolve_ref, ')'),
            seq('cast',        '(', $.speaker, ')'),
            seq('faction',     '(', $._faction_ref, ')'),
        ),

        slugline_scene: $ => prec.right(PREC.SLUGLINE, seq(
            '##',
            field('slug', $.scene_slug),
            optional(field('slugline', $.string)),
            repeat($.modifier),
            $._newline,
            optional($.docstring),
            repeat($._content),
        )),

        scene_slug: $ => prec.right(seq(
            $.identifier,
            optional(seq('.', $.identifier)),
        )),

        // ────────────────────────────────────────────────────────────────
        // §6. Sections
        // ────────────────────────────────────────────────────────────────

        section: $ => prec.right(PREC.SECTION, seq(
            '--',
            optional(field('id', $.identifier)),
            repeat($.modifier),
            optional($.participant_scope),
            optional($.guard),
            $._newline,
            optional($.docstring),
            repeat($._content),
        )),

        modifier: $ => prec(PREC.PROPERTY, seq(
            '.',
            field('name', $.identifier),
            optional(seq('(', optional($._mod_arg), ')')),
        )),

        _mod_arg: $ => choice($.string, $.number, $.identifier),

        guard: $ => seq('if', $._expression),

        participant_scope: $ => seq(
            'as',
            choice('participant', 'faction', $.resolve_ref),
        ),

        // ────────────────────────────────────────────────────────────────
        // §7. Content (inside sections)
        // ────────────────────────────────────────────────────────────────

        _content: $ => choice(
            $.dialogue,
            $.flavor_line,
            $.stage_direction,
            $.choice,
            $.divert,
            $.return_line,
            $.action_line,
            $.annotation,
            $.each_visit_block,
            $.after_block,
            $.otherwise_block,
            $.when_block,
            $.match_block,
            $.sexp,
            $.binding_line,
            $._newline,
        ),

        _body_block: $ => seq(
            $._indent,
            repeat1($._content),
            $._dedent,
        ),

        // §7.1 Dialogue
        dialogue: $ => prec(PREC.SPEAKER_LINE, seq(
            field('speaker', $._dialogue_speaker),
            optional($.char_block),
            optional(field('dual', '^')),
            $._newline,
            optional($.parenthetical),
            optional($._dialogue_lines),
        )),

        _dialogue_speaker: $ => choice(
            $.speaker,
            $.resolve_ref,
            $.static_ref,
        ),

        char_block: $ => seq(
            '{',
            sep1($.char_item, ','),
            '}',
        ),

        char_item: $ => choice(
            $.identifier,
            seq(
                field('key', $.identifier),
                ':',
                field('value', choice($.identifier, $.string, $.number)),
            ),
        ),

        parenthetical: $ => seq(
            $._indent,
            '(',
            $.text_content,
            ')',
            $._newline,
            $._dedent,
        ),

        _dialogue_lines: $ => seq(
            $._indent,
            repeat1(seq($.text_content, $._newline)),
            $._dedent,
        ),

        // §7.2 Stage direction & flavor
        flavor_line: $ => seq(
            '>',
            $.text_content,
            $._newline,
        ),

        // Plain prose at content position.
        stage_direction: $ => seq(
            $._stage_direction_text,
            $._newline,
        ),

        // A run of "plain prose" characters that doesn't start with any
        // structural sigil. Conservative: lower precedence than every
        // structured line class.
        _stage_direction_text: $ => /[A-Za-z0-9_'"][^\n]*/,

        // §7.3 Choice
        choice: $ => seq(
            field('kind', choice('*', '+')),
            repeat($.modifier),
            field('label', $.text_content),
            optional($.guard),
            $._newline,
            optional($._body_block),
        ),

        // §7.4 Divert & return
        divert: $ => prec(PREC.DIVERT, seq(
            '->',
            field('target', $._divert_target),
            repeat($.modifier),
            optional($.participant_scope),
            optional($.guard),
            $._newline,
        )),

        _divert_target: $ => choice(
            $._divert_target_simple,
            $._divert_target_call,
            $.static_ref,
        ),

        _divert_target_simple: $ => prec.right(seq(
            $.identifier,
            optional(seq('.', $.identifier)),
        )),

        _divert_target_call: $ => seq(
            $.identifier,
            '(',
            optional($._arg_list),
            ')',
            '->',
        ),

        return_line: $ => seq(
            '<-',
            optional(field('thread', $.identifier)),
            $._newline,
        ),

        // §7.5 Action line
        action_line: $ => seq(
            optional(field('prefix', '~')),
            sep1($._action, ';'),
            $._newline,
        ),

        _action: $ => choice(
            $.keyword_action,
            $.namespace_call,
            $.mutation_expr,
        ),

        keyword_action: $ => seq(
            field('kw', $.action_keyword),
            field('args', optional($._action_args)),
        ),

        // Subset of registry-driven action keywords. `yield`, `wait`,
        // `return`, `cancel`, and `spawn` have dedicated statement forms
        // (§18) and are intentionally not duplicated here.
        action_keyword: $ => choice(
            'var', 'let', 'fire', 'advance', 'trigger', 'modify',
            'enroll', 'cue', 'cast', 'reveal',
        ),

        // Greedy tail — registry-driven (grammar §7.5).
        _action_args: $ => /[^\n;]+/,

        namespace_call: $ => seq(
            field('namespace', $.identifier),
            repeat1(seq('.', $.identifier)),
            '(',
            optional($._arg_list),
            ')',
        ),

        mutation_expr: $ => seq(
            field('lvalue', $._lvalue),
            field('op', $._assign_op),
            field('rhs', $._expression),
        ),

        _lvalue: $ => seq(
            $.resolve_ref,
        ),

        _assign_op: $ => choice(':=', '+=', '-=', '++'),

        // §7.6 Annotation
        annotation: $ => prec(PREC.ANNOTATION, seq(
            '@',
            field('name', $.identifier),
            optional(seq(':', $.identifier)),
            optional(field('body', /[^\n]+/)),
            $._newline,
        )),

        // §7.7 Blocks
        each_visit_block: $ => seq(
            'each', 'visit',
            optional($.modifier),
            $._newline,
            $._indent,
            repeat1($._visit_branch),
            $._dedent,
        ),

        _visit_branch: $ => choice(
            seq(
                choice('first', 'then', 'finally'),
                $._newline,
                $._body_block,
            ),
            seq(
                '/',
                $._newline,
                $._body_block,
            ),
        ),

        after_block: $ => seq(
            'after',
            $._expression,
            $._newline,
            $._body_block,
        ),

        otherwise_block: $ => seq(
            'otherwise',
            $._newline,
            $._body_block,
        ),

        when_block: $ => prec(2, seq(
            'when',
            // grammar §7.7: bare IDENT is an event name; `$expr` is a condition.
            // Prefer the identifier branch when the discriminant is a single
            // word (more common in practice and matches the doc's example).
            choice(
                prec(2, $.identifier),
                $._expression,
            ),
            $._newline,
            $._body_block,
        )),

        match_block: $ => seq(
            'match',
            $._expression,
            $._newline,
            $._indent,
            repeat1($.match_arm),
            $._dedent,
        ),

        match_arm: $ => seq(
            choice($.string, $.number, '_'),
            $._newline,
            $._body_block,
        ),

        // ────────────────────────────────────────────────────────────────
        // §8. Declarations & definitions (s-expression form)
        // ────────────────────────────────────────────────────────────────

        // An s-expression always leads with an operator/keyword and has at
        // least one additional atom (grammar §9). Both rules are needed so
        // that `(x)` and `(true)` resolve unambiguously as grouped
        // expressions rather than one-atom s-exprs.
        sexp: $ => seq(
            '(',
            field('head', $._sexp_head),
            repeat1($._sexp_atom),
            ')',
        ),

        _sexp_head: $ => choice(
            $.sexp_keyword,
            $.identifier,
            $.resolve_ref,
            $.static_ref,
            'and', 'or', 'not', 'if',
            'is', 'has', 'in', 'of',
            $._sexp_op,
        ),

        _sexp_op: $ => choice(
            '+', '-', '*', '/', '%',
            '==', '!=', '<', '<=', '>', '>=',
        ),

        _sexp_atom: $ => choice(
            $.string,
            $.number,
            $.boolean,
            $.nil,
            $.identifier,
            $.resolve_ref,
            $.static_ref,
            $.sexp,
            $.sexp_keyword,
        ),

        // Reserved s-expression operator words. Distinct from `identifier`
        // so highlights.scm can target them specifically.
        sexp_keyword: $ => choice(
            'list', 'relation', 'entity',
            'define', 'defn', 'defmacro',
            'import', 'export',
            'one-to-one', 'one-to-many', 'many-to-many',
        ),

        binding_line: $ => seq(
            'let',
            field('name', $.identifier),
            '=',
            field('value', $._expression),
            $._newline,
        ),

        // ────────────────────────────────────────────────────────────────
        // §9-§10. Expression grammar
        // ────────────────────────────────────────────────────────────────

        _expression: $ => choice(
            $.binary_expression,
            $.unary_expression,
            $.postfix_expression,
            $._atom,
        ),

        binary_expression: $ => choice(
            prec.left(PREC.OR,     seq($._expression, field('op', 'or'),  $._expression)),
            prec.left(PREC.AND,    seq($._expression, field('op', 'and'), $._expression)),
            prec.left(PREC.EQ,     seq($._expression, field('op', $._eq_op),     $._expression)),
            prec.left(PREC.CMP,    seq($._expression, field('op', $._cmp_op),    $._expression)),
            prec.left(PREC.MEMBER, seq($._expression, field('op', $._member_op), $._expression)),
            prec.left(PREC.ADD,    seq($._expression, field('op', choice('+', '-')), $._expression)),
            prec.left(PREC.MUL,    seq($._expression, field('op', choice('*', '/', '%')), $._expression)),
        ),

        unary_expression: $ => prec.right(PREC.UNARY, choice(
            seq(field('op', 'not'), $._expression),
            seq(field('op', '-'),   $._expression),
            seq(field('op', '!'),   $._expression),
        )),

        // Multi-word operators take priority over their single-word prefix
        // (grammar §3 multi-word lexical tokens: `is not`, `has not`).
        _eq_op:     $ => choice('==', '!=', $._is_not, 'is'),
        _is_not:    $ => prec(1, seq('is', 'not')),
        _cmp_op:    $ => choice('<', '<=', '>', '>='),
        _member_op: $ => choice($._has_not, 'has', 'in'),
        _has_not:   $ => prec(1, seq('has', 'not')),

        postfix_expression: $ => prec.left(PREC.POSTFIX, seq(
            $._atom,
            repeat1(choice(
                $.field_access,
                $.safe_nav,
                $.index_access,
            )),
            optional($.call_args),
        )),

        field_access: $ => seq('.', $.identifier),
        safe_nav:     $ => seq('?.', $.identifier),
        index_access: $ => seq('[', $._expression, ']'),
        call_args:    $ => seq('(', optional($._arg_list), ')'),

        _arg_list: $ => sep1($._expression, ','),

        _atom: $ => choice(
            $.number,
            $.string,
            $.boolean,
            $.nil,
            $.identifier,
            $.resolve_ref,
            $.static_ref,
            $.ledger_pred,
            $.list_comprehension,
            seq('(', $._expression, ')'),
            $.sexp,
        ),

        boolean: $ => choice('true', 'false'),
        nil:     $ => 'nil',

        list_comprehension: $ => seq(
            '[',
            $._expression,
            'for',
            field('var', $.identifier),
            'in',
            $._expression,
            optional(seq('where', $._expression)),
            ']',
        ),

        ledger_pred: $ => seq(
            field('kind', choice('played', 'visits', 'chose', 'last', 'since', 'count')),
            '(',
            choice($.identifier, $._ledger_field),
            ')',
        ),

        _ledger_field: $ => choice('speaker', 'choice', 'event'),

        // ────────────────────────────────────────────────────────────────
        // §11. Names, references, sigils
        // ────────────────────────────────────────────────────────────────

        resolve_ref: $ => prec.right(choice(
            seq('$', $.identifier, optional($.field_chain), optional('?')),
            seq('$', $.speaker, optional($.field_chain), optional('?')),
            seq('${', $._expression, '}'),
            seq('$(', $._sexp_head, repeat($._sexp_atom), ')'),
            // Qualifier forms: $var:trust, $entity:elena, $role:SPEAKER
            seq('$', $.identifier, ':', choice($.identifier, $.speaker)),
        )),

        field_chain: $ => prec(PREC.POSTFIX, repeat1(choice(
            $.field_access,
            $.safe_nav,
            $.index_access,
        ))),

        static_ref: $ => prec.right(seq(
            '@',
            $.identifier,
            repeat(seq('.', $.identifier)),
        )),

        backlink: $ => seq(
            '[[',
            optional(seq(field('type', $.identifier), ':')),
            field('target', $._backlink_target),
            optional(seq('|', field('display', $._backlink_display))),
            ']]',
        ),

        _backlink_target: $ => /[^|\]]+/,
        _backlink_display: $ => /[^\]]+/,

        // Inline assign wraps an expression in `< … >` with no precedence
        // dance: at point of `>`, prefer closing the inline_assign over
        // extending the expression with a comparison operator. Authors
        // wrap comparisons in parens (`< $x := (a > b) >`).
        inline_assign: $ => prec(10, seq(
            '<',
            field('lvalue', $.resolve_ref),
            field('op', $._assign_op),
            field('rhs', $._expression),
            '>',
        )),

        // ────────────────────────────────────────────────────────────────
        // §12. Inline text grammar
        //
        // text_content is a permissive sequence of literal runs and special
        // markers. It's the textual payload of dialogue lines, flavor lines,
        // parentheticals, and choice labels.
        // ────────────────────────────────────────────────────────────────

        text_content: $ => repeat1($._text_element),

        // Inline-text grammar §12. Note: `${expr}` and `$(expr)` are
        // already covered by resolve_ref (grammar §11.1), so we don't
        // re-introduce an inline_eval alternative here.
        _text_element: $ => choice(
            $.literal_run,
            $.escaped_char,
            $.backlink,
            $.text_variation,
            $.resolve_ref,
            $.static_ref,
            $.inline_trigger,
            $.inline_assign,
            $.range_closer,
            $.chain_trigger,
            $.cond_trigger,
        ),

        // Anything that isn't a special sigil opener. Inline runs stop at
        // any of: $ @ [ < \ /
        literal_run: $ => token(prec(-1, /[^\n$@\[<\\/]+/)),

        escaped_char: $ => /\\[\\<\[\{\$@\/]/,

        // §12.1 Text variation: [a / b / c].mode
        text_variation: $ => prec(2, seq(
            '[',
            sep1($._variation_text, '/'),
            ']',
            optional($.variation_mode),
        )),

        _variation_text: $ => repeat1(choice(
            $.literal_run,
            $.escaped_char,
            $.resolve_ref,
            $.static_ref,
            $.inline_trigger,
        )),

        variation_mode: $ => seq(
            '.',
            field('name', $.identifier),
            optional(seq('(', optional($._arg_list), ')')),
        ),

        // §12.2 Inline trigger: <type:args>
        inline_trigger: $ => seq(
            '<',
            field('type', $.identifier),
            ':',
            field('args', $._trigger_args),
            repeat($.trigger_attr),
            optional($.range_spec),
            '>',
        ),

        _trigger_args: $ => /[^>\s]+/,

        trigger_attr: $ => seq(
            $.identifier,
            ':',
            choice($.identifier, $.string, $.number, $.resolve_ref, $.static_ref),
        ),

        range_spec: $ => prec(10, choice(
            seq('for:', choice($.number, $._expression)),
            seq('%', $.identifier),
        )),

        // §12.3 Chain trigger: <a+b+c>
        chain_trigger: $ => seq(
            '<',
            $._trigger_head,
            repeat1(seq('+', $._trigger_head)),
            '>',
        ),

        _trigger_head: $ => seq(
            $.identifier,
            ':',
            /[^+>\s]+/,
        ),

        // §12.4 Conditional trigger: <?expr> <trigger>
        cond_trigger: $ => seq(
            '<?',
            $._expression,
            '>',
            $.inline_trigger,
        ),

        range_closer: $ => choice(
            '</>',
            seq('</%', $.identifier, '>'),
        ),

        // ────────────────────────────────────────────────────────────────
        // §16. Character archetype bodies
        // ────────────────────────────────────────────────────────────────

        knowledge_block: $ => seq(
            'knowledge',
            $._newline,
            $._indent,
            repeat1($.knowledge_field),
            $._dedent,
        ),

        knowledge_field: $ => seq(
            field('name', $.identifier),
            ':',
            field('type', $._knowledge_type),
            optional(seq('=', $._expression)),
            $._newline,
        ),

        _knowledge_type: $ => choice(
            'bool', 'int', 'float', 'string',
            seq('{', sep1($.identifier, ','), '}'),
            seq('list', '<', $._knowledge_type, '>'),
            seq($._knowledge_type_inner, '?'),
        ),

        _knowledge_type_inner: $ => choice(
            'bool', 'int', 'float', 'string',
            seq('list', '<', $._knowledge_type, '>'),
        ),

        goal_decl: $ => seq(
            'goal',
            field('name', $.identifier),
            $._newline,
            $._indent,
            repeat1($.goal_knob),
            $._dedent,
        ),

        goal_knob: $ => seq(
            field('name', choice(
                'priority', 'active_when', 'completes_when', 'fails_when',
                'drives', 'on_complete', 'on_fail',
            )),
            optional(seq('=', $._expression)),
            optional($._action_chain),
            $._newline,
        ),

        _action_chain: $ => sep1(choice($._action, $.divert), ';'),

        disposition_block: $ => seq(
            'disposition',
            field('target', choice($.resolve_ref, $.static_ref)),
            $._newline,
            $._indent,
            repeat1($._disposition_item),
            $._dedent,
        ),

        _disposition_item: $ => choice(
            $.disposition_axis,
            $.disposition_react,
        ),

        disposition_axis: $ => seq(
            field('name', $.identifier),
            '=',
            $.num_range,
            optional(seq(',', 'init', $.number)),
            optional(seq(',', $.mirror_clause)),
            $._newline,
        ),

        disposition_react: $ => seq(
            'reacts',
            $._expression,
            '->',
            field('tag', $.identifier),
            $._newline,
        ),

        num_range: $ => seq($.number, '..', $.number),

        mirror_clause: $ => seq('mirror', $.resolve_ref),

        hook_decl: $ => seq(
            'on',
            field('pattern', $._hook_pattern),
            $._newline,
            $._body_block,
        ),

        _hook_pattern: $ => choice(
            seq('meeting', $.resolve_ref),
            prec(2, seq($._expression, 'passes', $._expression)),
            prec(2, seq($._expression, 'drops', 'below', $._expression)),
            prec(2, seq($._expression, '==', $._expression)),
            prec(2, seq($._expression, 'is', $.identifier)),
            seq('cue', $.identifier),
            seq('event', $.identifier),
            seq($.resolve_ref, 'enters', $.static_ref),
            seq($.resolve_ref, 'exits', $.static_ref),
        ),

        // ────────────────────────────────────────────────────────────────
        // §17. Stats archetype
        // ────────────────────────────────────────────────────────────────

        attribute_decl: $ => seq(
            'attribute',
            field('name', $.identifier),
            '=',
            $.number,
            repeat(seq(',', $._attribute_mod)),
            $._newline,
        ),

        _attribute_mod: $ => choice(
            seq('range', $.num_range),
            seq('min', $.number),
            seq('max', $.number),
        ),

        axis_decl: $ => seq(
            'axis',
            field('name', $.identifier),
            $._newline,
            $._indent,
            repeat1($.axis_property),
            $._dedent,
        ),

        axis_property: $ => choice(
            seq('mode', $._axis_mode, $._newline),
            seq('curve', $._expression, $._newline),
            seq('on', 'use', $.static_ref, $._newline),
            seq('on', 'advance', $._action_chain, $._newline),
            seq('buy', 'from', $.identifier, $._newline),
            seq('milestones', $._newline, $._indent, repeat1($.milestone_entry), $._dedent),
            seq('advance', 'on', 'event', $.identifier, $._newline),
            seq('handler', $.static_ref, $._newline),
        ),

        _axis_mode: $ => choice(
            'xp_curve', 'use_tracking', 'point_buy',
            'milestone', 'narrative_trigger', 'sdk_controlled',
            $.identifier,
        ),

        milestone_entry: $ => seq($.number, ':', $._expression, $._newline),

        pool_decl: $ => seq(
            'pool',
            field('name', $.identifier),
            $._newline,
            $._indent,
            repeat1($.pool_property),
            $._dedent,
        ),

        pool_property: $ => seq(
            field('key', choice('max', 'min', 'regen', 'init')),
            optional('='),
            $._expression,
            optional(seq('/', $.duration)),
            optional(seq('when', $._expression)),
            $._newline,
        ),

        stat_decl: $ => seq(
            'stat',
            field('name', $.identifier),
            choice(
                // expression form
                seq('=', $._expression, $._newline),
                // lookup form
                seq($._newline, $._indent, repeat1($.lookup_property), $._dedent),
                // derived form
                seq('derived', 'from', sep1($.resolve_ref, ','), $._newline,
                    $._indent, 'formula', $._expression, $._newline, $._dedent),
            ),
        ),

        lookup_property: $ => choice(
            seq('lookup', $._expression, $._newline),
            seq('table', $.dict_literal, $._newline),
            seq('interpolate', $.identifier, $._newline),
        ),

        tree_node_decl: $ => seq(
            'node',
            field('name', $.identifier),
            $._newline,
            $._indent,
            repeat1($.tree_node_property),
            $._dedent,
        ),

        tree_node_property: $ => choice(
            seq('cost',     $.dict_literal, $._newline),
            seq('requires', $._expression,  $._newline),
            seq('effect',   $._tree_effect, $._newline),
            seq('rank',     $.number,       $._newline),
        ),

        _tree_effect: $ => choice(
            seq('stat',      '(', $.identifier, ')', $._stat_effect_op, $.number),
            seq('attribute', '(', $.identifier, ')', $._stat_effect_op, $.number),
            seq('var',       '(', $.identifier, ')', ':=', $._expression),
            seq('pool',      '(', $.identifier, ')', 'grant'),
            seq('ability',   $.static_ref),
            seq('luau',      $.static_ref),
        ),

        _stat_effect_op: $ => choice('+', '-', '*', '/'),

        // ────────────────────────────────────────────────────────────────
        // §18. Reactivity declarations
        // ────────────────────────────────────────────────────────────────

        generator_decl: $ => seq(
            'generator',
            field('name', $.identifier),
            optional($.param_list),
            $._newline,
            $._indent,
            optional($._scheduler_properties),
            repeat1($._generator_item),
            $._dedent,
        ),

        scene_decl: $ => seq(
            'scene',
            field('name', $.identifier),
            optional($.param_list),
            $._newline,
            $._indent,
            optional($._scheduler_properties),
            repeat1($.scene_state),
            $._dedent,
        ),

        scene_state: $ => seq(
            field('name', $.identifier),
            $._newline,
            $._indent,
            repeat1(choice($._generator_item, $.return_stmt)),
            $._dedent,
        ),

        return_stmt: $ => seq('return', optional($._expression), $._newline),

        _scheduler_properties: $ => seq(
            repeat1($.scheduler_property),
        ),

        scheduler_property: $ => choice(
            seq('.tier',      field('tier', choice('focal', 'active', 'ambient', $.identifier)), $._newline),
            seq('.priority',  $.number, $._newline),
            seq('.budget_ms', $.number, $._newline),
        ),

        // Generator/scene bodies share the conversation vocabulary plus the
        // coroutine primitives (loop / wait / yield / time). `_content`
        // already covers action_line / when / after / match / binding, so
        // we don't duplicate them here.
        _generator_item: $ => choice(
            $.loop_block,
            $.time_stmt,
            $.yield_stmt,
            $.wait_stmt,
            $._content,
        ),

        loop_block: $ => seq(
            'loop',
            optional(field('bound', choice($.number, 'forever'))),
            $._newline,
            $._indent,
            repeat1($._generator_item),
            $._dedent,
        ),

        time_stmt: $ => choice(
            seq('at',    $._time_spec,  $._newline),
            seq('every', $.duration,    $._newline),
            seq('every', 'random', '(', $.duration, ',', $.duration, ')', $._newline),
        ),

        _time_spec: $ => choice(
            seq($.number, choice('am', 'pm')),
            seq($.number, ':', $.number),
        ),

        wait_stmt: $ => choice(
            seq('wait', $.duration, $._newline),
            seq('wait', 'until', $._expression, $._newline),
        ),

        yield_stmt: $ => seq(
            'yield',
            optional($._yield_body),
            $._newline,
            optional($._body_block),
        ),

        _yield_body: $ => choice(
            seq('bark', 'from', $.static_ref),
            seq('with_chance', '(', $.number, ')'),
            /[^\n]+/,
        ),

        compose_decl: $ => seq(
            'compose',
            field('name', $.identifier),
            $._newline,
            $._indent,
            repeat1($.pattern_block),
            $._dedent,
        ),

        pattern_block: $ => seq(
            'pattern',
            optional(field('selector', $.resolve_ref)),
            $._newline,
            $._indent,
            repeat1($.pattern_arm),
            $._dedent,
        ),

        pattern_arm: $ => seq(
            field('discriminant', $._arm_discriminant),
            ':',
            field('text', $.string),
            optional(seq(',', 'weight', ':', $.number)),
            $._newline,
        ),

        _arm_discriminant: $ => choice(
            $.identifier,
            $.number,
            $.num_range,
            '_',
        ),

        timecode_block: $ => seq(
            'at',
            $.timecode,
            $._newline,
            $._indent,
            repeat1(choice($.track_command, $.dialogue)),
            $._dedent,
        ),

        track_command: $ => seq(
            ':', $.identifier, ':', $.identifier,
            optional(seq('(', optional($._arg_list), ')')),
            $._newline,
            optional(seq($._indent, repeat1($.modifier), $._dedent)),
        ),

        // ────────────────────────────────────────────────────────────────
        // §19. Faction archetype
        // ────────────────────────────────────────────────────────────────

        inline_faction_decl: $ => seq(
            'faction',
            field('name', $.identifier),
            $._newline,
            $._indent,
            repeat1($._faction_body_item),
            $._dedent,
        ),

        _faction_body_item: $ => choice(
            $.members_block,
            $.state_block,
            $.stance_block,
            $.goal_decl,
            $.hook_decl,
            $.generator_decl,
            $.scene_decl,
            $.binding_line,
            $.property,
        ),

        members_block: $ => seq(
            'members',
            $._newline,
            $._indent,
            repeat1($.member_entry),
            $._dedent,
        ),

        member_entry: $ => seq(
            choice(
                sep1($.static_ref, ','),
                seq('cohort', $.identifier),
                seq('match',  $._expression),
            ),
            optional($.visibility_clause),
            $._newline,
        ),

        visibility_clause: $ => seq(
            'visibility',
            ':',
            $._visibility_level,
        ),

        _visibility_level: $ => choice(
            'public', 'private',
            seq('cohort',      '(', $.identifier, ')'),
            seq('participant', '(', $.resolve_ref, ')'),
            seq('faction',     '(', $._faction_ref, ')'),
        ),

        state_block: $ => seq(
            'state',
            $._newline,
            $._indent,
            repeat1($.state_axis),
            $._dedent,
        ),

        state_axis: $ => seq(
            field('name', $.identifier),
            '=',
            $.num_range,
            optional(seq(',', 'init', $.number)),
            optional(seq(',', $.mirror_clause)),
            $._newline,
        ),

        stance_block: $ => seq(
            'stance',
            $._newline,
            $._indent,
            repeat1($.stance_entry),
            $._dedent,
        ),

        stance_entry: $ => seq(
            field('target', choice($.static_ref, 'default')),
            '=',
            field('level', $._stance_level),
            optional('asymmetric'),
            $._newline,
        ),

        _stance_level: $ => choice(
            'hostile', 'wary', 'neutral', 'friendly', 'allied',
            $.identifier,
        ),

        _faction_ref: $ => choice($.resolve_ref, $.static_ref),

        faction_event: $ => seq(
            'when',
            choice(
                seq('faction', field('verb', choice('emerges', 'dissolves', 'grows', 'shrinks'))),
                seq('stance', 'changes'),
            ),
            optional($._faction_event_qualifier),
            $._newline,
            optional($._body_block),
        ),

        _faction_event_qualifier: $ => choice(
            seq('from', 'template', $.static_ref),
            seq('in', $.resolve_ref),
            seq('(', $._faction_ref, ',', $._faction_ref, ')'),
        ),

        participant_faction_lifecycle: $ => seq(
            'when',
            'participant',
            field('verb', choice('proposes', 'joins', 'leaves')),
            'faction',
            $._newline,
            optional($._body_block),
        ),

        discovery_event: $ => seq(
            'when',
            choice(
                seq('stance', '(', $._faction_ref, ',', $._faction_ref, ')', 'revealed', 'to', $._observer),
                seq('membership', 'of', choice($.resolve_ref, $.static_ref), 'in', $._faction_ref, 'revealed', 'to', $._observer),
                seq($._faction_ref, 'discovers_member', choice($.resolve_ref, $.static_ref)),
                seq('stance', 'revealed', 'to', $._observer),
                seq('membership', 'revealed', 'to', $._observer),
            ),
            $._newline,
            optional($._body_block),
        ),

        _observer: $ => choice(
            $.resolve_ref,
            $.static_ref,
            seq('(', 'cohort',  $.identifier, ')'),
            seq('(', 'faction', $._faction_ref, ')'),
            seq(':', 'all'),
        ),

        // ────────────────────────────────────────────────────────────────
        // Misc shared productions
        // ────────────────────────────────────────────────────────────────

        param_list: $ => seq(
            '|',
            sep($.identifier, ','),
            '|',
        ),

        dict_literal: $ => seq(
            '{',
            sep($.dict_entry, ','),
            '}',
        ),

        dict_entry: $ => seq(
            field('key', $.identifier),
            ':',
            field('value', choice($._expression, $.dict_literal, $.list_literal)),
        ),

        list_literal: $ => seq(
            '[',
            sep($._expression, ','),
            ']',
        ),

        // ────────────────────────────────────────────────────────────────
        // Lexical structure (§3)
        // ────────────────────────────────────────────────────────────────

        identifier: $ => /[a-zA-Z_][a-zA-Z0-9_]*/,

        // ALL-CAPS speaker identifier. Grammar §3.2: lexes as SPEAKER at the
        // start of a dialogue line; lexes as IDENT elsewhere. We model it as
        // a distinct token and let the parser place it.
        speaker: $ => /[A-Z][A-Z0-9_]+/,

        number: $ => token(seq(
            optional('-'),
            /[0-9]+/,
            optional(seq('.', /[0-9]+/)),
            optional(choice('s', 'ms', 'm', 'h')),
        )),

        duration: $ => token(seq(
            /[0-9]+(\.[0-9]+)?/,
            choice('s', 'ms', 'm', 'h'),
        )),

        timecode: $ => /[0-9]+\.[0-9]+/,

        string: $ => token(seq(
            '"',
            repeat(choice(
                /[^"\\\n]/,
                /\\./,
            )),
            '"',
        )),

        line_comment: $ => token(seq('//', /[^\n]*/)),

        block_comment: $ => token(seq(
            '/*',
            repeat(choice(
                /[^*]/,
                /\*[^\/]/,
            )),
            '*/',
        )),
    },
});
