# Loom — Formal Grammar

> The grammar of `.loom` source. Precise enough to drive a parser; loose
> enough to read.

**Status:** initial draft (2026-05-21). Companion to
[`loom-design.md`](loom-design.md). This doc supersedes
`LEGACY-CODEBASE/loom-lang/grammar.md`, which was a quick reference, not
a true formal grammar.

**Audience:** anyone writing the parser, an LSP, a Lezer/tree-sitter
grammar, a syntax highlighter, or a third-party tool that reads `.loom`.
Writers don't need this doc — see `loom-design.md` §2 for that.

**Notation:** EBNF-ish.
- `'...'` — literal
- `A | B` — alternation
- `A?` — optional
- `A*` — zero or more
- `A+` — one or more
- `A , B , C` — sequence
- `A - B` — `A` excluding any `B`
- `[abc]` — any of the characters
- `<NAME>` — terminal token defined in §3
- `«note»` — non-grammatical commentary

The grammar is presented top-down: file structure first, line classes,
expressions, inline text. A bottom-up reader can start at §3 (lexical
structure) and walk back up.

---

## 1. Source organization

A Loom **document** is one `.loom` file. A **project** is a set of
documents sharing a `LoomRegistry` (the set of registered keywords,
sigils, triggers — see [design §11](loom-design.md#11-extension-model)).

```
SourceFile  ::= Shebang? , BOM? , Document+
Shebang     ::= '#!' , (-NL)* , NL                    // optional, first line only
Document    ::= Header , Properties? , Docstring? , Body
```

A single `.loom` file may contain multiple documents. They are
separated by the presence of a new `Header` line at column 0.

---

## 2. Whitespace, indentation, comments

Loom is **indentation-sensitive** (Python-style). The lexer emits
synthetic `INDENT` and `DEDENT` tokens when leading whitespace changes.

### 2.1 Physical lines

```
PhysicalLine  ::= Indent , Content , NL
Indent        ::= [\t ]*                              // tabs OR spaces, never mixed (lint error)
NL            ::= '\n' | '\r\n' | '\r'
```

End-of-file is a synthetic NL followed by enough DEDENTs to reach
column 0.

### 2.2 Logical lines & continuation

A logical line is one physical line unless the previous physical line
ended in a continuation. There are two continuation forms:

```
LineContinuation ::= '\\' , NL                        // explicit
ImplicitContinuation ::= NL inside a ( ) bracket pair  // s-expr line break
```

No other continuation. Long string literals do **not** wrap across
lines.

### 2.3 INDENT / DEDENT

The first non-blank, non-comment line of the file establishes the
**indent unit** — the count of leading whitespace characters (tabs OR
spaces; mixing is a hard error). Every subsequent line's indent must be
an integer multiple of that unit.

```
IndentLevel(line) = leading_whitespace_count / indent_unit
```

Between two adjacent logical lines `L1`, `L2`:
- If `IndentLevel(L2) > IndentLevel(L1)`, emit `INDENT` × (delta).
- If `IndentLevel(L2) < IndentLevel(L1)`, emit `DEDENT` × (delta).
- If equal, emit nothing.

Indent jumps > 1 unit at once are a hard error (`indent-jump`).

### 2.4 Comments & boneyard

```
LineComment   ::= '//' , (-NL)*                       // run-of-line
Boneyard      ::= '/*' , (any)*? , '*/'               // multi-line, eats whole region
```

Comments are discarded by the lexer. They do **not** affect indent
tokens (a comment line is treated as if it weren't there).

### 2.5 Escape sequences

Inside dialogue / flavor text the following escapes are recognized:

```
Escape   ::= '\\' , [\\<\[{$@/]
```

Anywhere else (s-expressions, expressions), strings are quoted and use
standard JSON-style escapes (`\"`, `\\`, `\n`, `\t`, `\r`, `\u{NNNN}`).

---

## 3. Lexical structure (terminal tokens)

These are the terminals the parser sees. The lexer is a hand-rolled
`Scanner`-based tokenizer (no regex on the hot path —
[design §2.5 / 7](loom-design.md#7-crate-layout)).

```
IDENT        ::= [a-zA-Z_] , [a-zA-Z0-9_]*           // user identifiers
KW           ::= IDENT - any reserved word           // see §3.1
SPEAKER      ::= [A-Z] , [A-Z0-9_]*                  // all-caps speaker name
NUMBER       ::= '-'? , [0-9]+ , ('.' , [0-9]+)? , ('s' | 'ms')?
STRING       ::= '"' , (StringChar | Escape)* , '"'
TIMECODE     ::= [0-9]+ , '.' , [0-9]+               // for `at 2.5`
DOCSTRING    ::= "'''" , (any)*? , "'''"
SIGIL_HASH   ::= '#'
SIGIL_DASH2  ::= '--'
SIGIL_STAR   ::= '*'
SIGIL_PLUS   ::= '+'
SIGIL_ARROW  ::= '->'
SIGIL_BACK   ::= '<-'
SIGIL_CHEV   ::= '>'                                  // only at line start, after indent
SIGIL_AT     ::= '@'
SIGIL_DOT    ::= '.'                                  // only at line start (property) or after IDENT (field)
SIGIL_TILDE  ::= '~'                                  // action prefix
SIGIL_QMARK  ::= '?'                                  // condition prefix (legacy form; prefer `if`)
SIGIL_PCT    ::= '%'                                  // named anchor
SIGIL_DOLLAR ::= '$'
LBRACE       ::= '{'
RBRACE       ::= '}'
LBRACK       ::= '['
RBRACK       ::= ']'
LBRACK2      ::= '[['
RBRACK2      ::= ']]'
LANGLE       ::= '<'
RANGLE       ::= '>'
RANGLE_CLOSE ::= '</>' | '</%' , IDENT , '>'
LPAREN       ::= '('
RPAREN       ::= ')'
PIPE         ::= '|'
COMMA        ::= ','
COLON        ::= ':'
SEMI         ::= ';'
EQ           ::= '='
WALRUS       ::= ':='
EQEQ         ::= '=='
NEQ          ::= '!='
GT, GE, LT, LE ::= '>' | '>=' | '<' | '<='
PLUSEQ, MINUSEQ, PLUSPLUS ::= '+=' | '-=' | '++'
MEMBER       ::= '?='                                 // 'has'
NOTMEMBER    ::= '!?='                                // 'has not'
SAFENAV      ::= '?.'
DOLLAR_BRACE ::= '${'
DOLLAR_PAREN ::= '$('

INDENT       ::= synthetic, see §2.3
DEDENT       ::= synthetic, see §2.3
NL           ::= newline (after comments stripped)
```

### 3.1 Reserved keywords

These identifiers cannot be used as user identifiers anywhere:

```
if  and  or  not  is  has
var  let  define  defn  defmacro
fire  advance  trigger
each  visit  first  then  finally
after  otherwise  when  match  weave
import  export
true  false  nil
```

`is not` and `has not` are two-word operators (lexer joins them with
look-ahead). `each visit` is a two-word block keyword (same).

### 3.2 Speaker vs identifier

A `SPEAKER` is an all-caps identifier *at the start of a line, after
indent, with no preceding sigil*, optionally followed by a character
block (`{ ... }`) or a `^` (for dual dialogue). Anywhere else,
`[A-Z][A-Z0-9_]*` lexes as a regular `IDENT`.

A `$UPPER` form (`$NARRATOR`, `$SPEAKER`) is a **resolve reference**,
not a literal speaker — its value is resolved at runtime and used as
the speaker label. See §10.1.

---

## 4. Document structure

```
Header     ::= '#' , IDENT , STRING? , Tag*
Tag        ::= ':' , IDENT                           // :conversation, :barks, :quest, :cutscene

Properties ::= INDENT , Property+ , DEDENT
Property   ::= '.' , IDENT , PropertyValue? , NL
PropertyValue ::= (-NL)*                             // free-form, parsed per-property by registry

Docstring  ::= DOCSTRING , NL
Body       ::= TopLevelItem*
```

The first non-blank line of a document must be a `Header`. Properties
must immediately follow the header (one indent level deeper, no blank
lines between). The optional docstring comes after properties.

### 4.1 Document type tags

The tag set on the header determines the document **archetype**, which
determines which top-level items are legal in the body.

| Tag set | Archetype | Body items allowed |
|---|---|---|
| (none) or `:conversation` | conversation | `Section`, declarations, definitions, imports |
| `:barks` | bark set | `BarkSection`, declarations, definitions |
| `:quest` | quest | `QuestStage`, declarations |
| `:cutscene` | cutscene | `TimecodeBlock`, declarations |
| `:typewriter` | typewriter profile | property-only (no body) |
| `:module` | shared module | declarations only (no sections) |

Unknown tags are a warning (`doc-type-unknown`), not an error — they're
stored as free-form metadata for tooling.

### 4.2 Top-level items

```
TopLevelItem
  ::= Section
    | BarkSection                                   // only when archetype = bark_set
    | QuestStage                                    // only when archetype = quest
    | TimecodeBlock                                 // only when archetype = cutscene
    | Declaration
    | Definition
    | Import
    | Binding                                       // let, define at top level
    | Comment
```

---

## 5. Sections

### 5.1 Conversation sections

```
Section
  ::= '--' , IDENT? , Modifier* , Guard? , NL
  , Docstring?
  , Content*

Modifier
  ::= '.' , IDENT , ( '(' , ModArg , ')' )?

Guard
  ::= 'if' , Expr

ModArg
  ::= STRING | NUMBER | IDENT
```

An empty section ID (`--` with nothing after) declares an **anonymous
section** that is reachable only by fall-through from the previous
section.

Section modifiers (built-in):
- `.once` — section can be entered at most once per save.
- `.hub` — section returns control to itself after each played child
  rather than falling through.
- `.return` — leaving this section pops the tunnel stack.

### 5.2 Bark sections

In a `:barks` document, sections take a compact form:

```
BarkSection
  ::= '--' , Priority? , Modifier* , Guard? , NL
  , INDENT , BarkBody , DEDENT

Priority
  ::= '.critical' | '.high' | '.normal' | '.low'   // open: studios register more

BarkBody
  ::= Dialogue+                                     // ≥ 1 speaker block(s)
```

A bark section has no ID — barks are addressed by speaker + condition,
not by name.

### 5.3 Quest stages

```
QuestStage
  ::= '--' , IDENT , STRING? , Modifier* , NL
  , INDENT , (FlavorLine | ObjectiveLine | LifecycleLine)* , DEDENT

ObjectiveLine
  ::= '.objective' , IDENT , ':' , ObjectiveSpec , NL

LifecycleLine
  ::= '.on-enter'    , ActionChain , NL
    | '.on-complete' , ActionChain , NL
    | '.on-fail'     , ActionChain , NL

ObjectiveSpec
  ::= IDENT , (-NL)*                                // type-specific args
```

Built-in objective types: `talk`, `search`, `collect`, `kill`,
`reach`, `custom`. Studios register more via `IObjectiveTypePlugin`.

### 5.4 Cutscene timecode blocks

```
TimecodeBlock
  ::= 'at' , TIMECODE , NL
  , INDENT , (TrackCommand | Dialogue)+ , DEDENT

TrackCommand
  ::= ':' , IDENT , ':' , IDENT , ('(' , ArgList , ')')? , NL
  , (INDENT , Modifier+ , DEDENT)?
```

Multiple `at` blocks may share the same timecode — they fire
simultaneously.

---

## 6. Content (inside sections)

```
Content
  ::= Dialogue
    | StageDirection
    | FlavorLine
    | Choice
    | Divert
    | ActionLine
    | Annotation
    | Block
    | SExpr
    | Binding
    | Comment
```

### 6.1 Dialogue

```
Dialogue
  ::= Speaker , CharBlock? , Dual? , NL
  , Parenthetical?
  , INDENT , TextLine+ , DEDENT

Speaker
  ::= SPEAKER
    | '$' , IDENT                                    // dynamic speaker
    | '@' , IDENT                                    // entity-typed speaker

CharBlock
  ::= '{' , CharItem (',' , CharItem)* , '}'

CharItem
  ::= IDENT                                          // bare word, inferred (emotion/dynamic/profile/studio)
    | IDENT , ':' , (IDENT | STRING | NUMBER)        // explicit key:value

Dual
  ::= '^'                                            // dual-dialogue marker

Parenthetical
  ::= INDENT , '(' , TextContent , ')' , DEDENT      // single line, single-indent deeper

TextLine
  ::= TextContent , NL
```

### 6.2 Stage direction & flavor

```
StageDirection
  ::= TextContent , NL                               // at column 0 (no indent)
                                                     // and not matching any other line class

FlavorLine
  ::= '>' , TextContent , NL                         // descriptive prose with explicit marker
```

`StageDirection` and `FlavorLine` differ in renderer treatment (the
former is typeset as block prose, the latter as narration). They share
the same parser.

### 6.3 Choice

```
Choice
  ::= ('*' | '+') , Modifier* , Label , Guard? , NL
  , (INDENT , Content* , DEDENT)?

Label
  ::= TextContent

ChoiceModifiers (built-in):
  .once    .sticky    .fallback    .interrupt
  .show(STRING)        // override displayed label
```

`*` is a once-only choice (removed after taking). `+` is sticky
(remains). `.once` on `+` and `.sticky` on `*` are both legal — they
make intent explicit at the cost of redundancy.

### 6.4 Divert & return

```
Divert
  ::= '->' , Target , Modifier* , Guard? , NL

Return
  ::= '<-' , IDENT? , NL                             // <-  (return to caller)
                                                     // <- name  (thread-pull-by-id)

Target
  ::= IDENT                                          // local section
    | IDENT , '.' , IDENT                            // doc.section
    | '@' , IDENT                                    // @scene reference
    | TunnelCall

TunnelCall
  ::= IDENT , '(' , ArgList? , ')' , '->'            // call then return

DivertModifiers (built-in):
  .return    // soft divert: come back here on next yield
  .dispatch  // fire-and-forget side entry
```

A `TunnelCall` ends with `->` — the trailing arrow distinguishes it
from a regular `Divert` whose target happens to be parenthesized for
grouping. `(foo)` is grouping; `(foo)->` is a tunnel.

### 6.5 Action line

```
ActionLine
  ::= ActionPrefix? , Action , (';' , Action)* , NL

ActionPrefix
  ::= '~'                                            // optional, prefer omitting for keywords

Action
  ::= KeywordAction
    | NamespaceCall
    | MutationExpr

KeywordAction
  ::= ActionKW , ActionArgs                          // registry-driven; ActionKW from §3.1 + extensions

NamespaceCall
  ::= IDENT , ('.' , IDENT)+ , '(' , ArgList? , ')'  // e.g. Camera.pan(@lighthouse, 2)

MutationExpr
  ::= LValue , (WALRUS | PLUSEQ | MINUSEQ | PLUSPLUS) , Expr?

LValue
  ::= '$' , IDENT , (FieldChain)?
```

Built-in actions: `var`, `let`, `fire`, `advance`, `trigger`.
Studio-registered actions extend `ActionKW`. The action **must**
appear in the registry; an unknown keyword is a parse error
(`unknown-action-kw`).

`~` is optional and historical. Style: omit when the action begins
with a registered keyword; include only when ambiguity with a speaker
name might arise (`~ ALICE := …` reads more naturally than
`ALICE := …` which looks like a speaker line).

### 6.6 Annotation

```
Annotation
  ::= '@' , IDENT , AnnotationBody? , NL

AnnotationBody
  ::= (-NL)*                                         // shape per-annotation, parsed by registry
```

Built-in annotations: `@vo`, `@director`, `@status`, `@note`, `@hint`,
`@loc`. Open extension namespace: `@<vendor>:<name>` (e.g.
`@studio:rating teen`).

### 6.7 Blocks (each visit / after / when / match)

```
Block
  ::= EachVisit
    | After
    | Otherwise
    | When
    | Match
    | UserBlock                                      // registry-driven

EachVisit
  ::= 'each' , 'visit' , Modifier? , NL
  , INDENT , VisitBranch+ , DEDENT

VisitBranch
  ::= ('first' | 'then' | 'finally') , NL
    , INDENT , Content* , DEDENT
    | '/'    , NL                                    // alternates separator
    , INDENT , Content* , DEDENT

After
  ::= 'after' , Expr , NL
  , INDENT , Content* , DEDENT

Otherwise
  ::= 'otherwise' , NL
  , INDENT , Content* , DEDENT

When
  ::= 'when' , (IDENT | Expr) , NL                   // bare IDENT = event; with $ = expr
  , INDENT , Content* , DEDENT

Match
  ::= 'match' , Expr , NL
  , INDENT , MatchArm+ , DEDENT

MatchArm
  ::= (STRING | NUMBER | '_') , NL
  , INDENT , Content* , DEDENT
```

`UserBlock` is opaque to this grammar — its parse is provided by the
registry entry that registered the block keyword. The registry entry
declares its sub-keywords (e.g. `clue` / `deduce` / `confront` for an
`investigation` block) and the parser dispatches to them within the
block body.

---

## 7. Declarations & definitions

```
Declaration
  ::= ListDecl | RelationDecl | EntityDecl

ListDecl
  ::= '(' , 'list' , IDENT , ':' , ListItems , ')'

ListItems
  ::= ListItem , (',' , ListItem)*

ListItem
  ::= IDENT | '(' , IDENT , ')'                      // bare = enum, parenthesized = unique value type

RelationDecl
  ::= '(' , 'relation' , IDENT , ':' , Cardinality , ')'

Cardinality
  ::= 'one-to-one' | 'one-to-many' | 'many-to-many'

EntityDecl
  ::= '(' , 'entity' , IDENT , EntitySpec? , ')'
```

```
Definition
  ::= ConstantDef | FunctionDef | MacroDef | LetBinding | DefineBinding

ConstantDef
  ::= '(' , 'define' , IDENT , '=' , Expr , ')'

FunctionDef
  ::= '(' , 'defn' , IDENT , ParamList , Content* , ')'

MacroDef
  ::= '(' , 'defmacro' , IDENT , ParamList , Content* , ')'

ParamList
  ::= '|' , (IDENT (',' , IDENT)*)? , '|'

LetBinding
  ::= 'let' , IDENT , '=' , Expr , NL                // or inside s-expr form

DefineBinding
  ::= '(' , 'define' , IDENT , Expr , ')'

Import
  ::= '(' , 'import' , STRING , ':' , ImportSpec , ')'
  | '(' , 'import' , STRING , ')'                    // import for side-effects only

ImportSpec
  ::= '*'
    | IDENT (',' , IDENT)*

Export
  ::= '(' , 'export' , IDENT (',' , IDENT)* , ')'
```

`defn` produces a callable that may be invoked from `Content` (rendered
inline) or from a `TunnelCall`. `defmacro` is identical at the surface
but its body is splice-expanded at parse time rather than called at
runtime. Both compile to Luau closures in v2.

---

## 8. S-expressions

S-expressions are the **explicit code** form. Anything in s-expression
position must be wrapped in `( ... )`. The s-expression grammar is
a Lispy reuse of the expression grammar:

```
SExpr
  ::= '(' , SExprBody , ')'

SExprBody
  ::= Atom , Atom*                                   // first atom is the operator/keyword

Atom
  ::= IDENT
    | STRING
    | NUMBER
    | ResolveRef
    | StaticRef
    | SExpr
    | KW
    | UnaryOp , Atom                                 // (not foo)
```

`SExpr` is used inside top-level declarations (`(define ...)`),
inside expressions (`if (and (> $trust 50) $met_wren)`), and inside
text interpolation (`$(...)`).

---

## 9. Expression grammar

Expressions appear in: `Guard` lines, `MutationExpr` right-hand sides,
`Match` discriminants, `When` discriminants, `${...}` inline eval,
`$(...)` inline eval, function bodies.

The expression grammar is **Pratt-parsed** (operator precedence climb).
Precedence table, lowest to highest:

```
Level  Operators                    Associativity
─────────────────────────────────────────────────
1      or                           left
2      and                          left
3      not                          right (prefix)
4      ==  !=  is  is not           left
5      <  <=  >  >=                 left
6      has  has not                 left
7      +  -                         left
8      *  /  %                      left
9      (unary -, !)                 right (prefix)
10     .  ?.  [ ]  ( )              left   (field / safe-nav / index / call)
```

```
Expr
  ::= OrExpr

OrExpr   ::= AndExpr   ( 'or'  AndExpr )*
AndExpr  ::= NotExpr   ( 'and' NotExpr )*
NotExpr  ::= 'not' NotExpr | EqExpr
EqExpr   ::= CmpExpr   ( EqOp CmpExpr )*
EqOp     ::= '==' | '!=' | 'is' | 'is not'
CmpExpr  ::= MemberExpr ( CmpOp MemberExpr )*
CmpOp    ::= '<' | '<=' | '>' | '>='
MemberExpr ::= AddExpr ( MemberOp AddExpr )*
MemberOp ::= 'has' | 'has not'
AddExpr  ::= MulExpr  ( ('+'|'-') MulExpr )*
MulExpr  ::= UnaryExpr ( ('*'|'/'|'%') UnaryExpr )*
UnaryExpr ::= ('-' | '!') UnaryExpr | PostfixExpr
PostfixExpr ::= Atom ( Postfix )*

Postfix
  ::= '.' , IDENT                                   // field access
    | '?.' , IDENT                                  // safe nav
    | '[' , Expr , ']'                              // indexing
    | '(' , ArgList? , ')'                          // call (only at chain end)

Atom
  ::= NUMBER
    | STRING
    | 'true' | 'false' | 'nil'
    | IDENT                                          // identifier (resolved by §10)
    | ResolveRef
    | StaticRef
    | LedgerPred                                     // §10.5
    | '(' , Expr , ')'                               // grouping
    | SExpr                                          // (op args...) lisp form
```

### 9.1 Method calls don't chain

`Postfix '(' ArgList? ')'` is legal **only at the end of a postfix
chain** — i.e. `$elena.faction.getStanding($PLAYER)` parses, but
`$elena.getThing().getOther()` is a parse error. Use a `let` binding to
break it apart. (Rationale: registered method calls are typed against
their return; chained calls explode the type lattice.)

### 9.2 Implicit grouping in `if`

After the `if` keyword (at the start of a `Guard`), an unwrapped
expression is parsed as a single `Expr` ending at the first newline.
Parentheses are optional for the outermost grouping:

```
if trust > 50 and met_wren
if (trust > 50 and met_wren)         // identical
```

---

## 10. Names, references, sigils

### 10.1 Resolve reference (`$`)

```
ResolveRef
  ::= '$' , IDENT , FieldChain?                     // $trust, $elena.trust
    | '$' , IDENT , '?'                             // $elena?  (presence check)
    | '$' , IDENT , FieldChain? , '?'               // $elena.trust?  (also presence)
    | DOLLAR_BRACE , Expr , '}'                     // ${expr}
    | DOLLAR_PAREN , SExprBody , ')'                // $(expr ...)

FieldChain
  ::= ( '.' , IDENT | '?.' , IDENT | '[' , Expr , ']' )+
   ,  ( '(' , ArgList? , ')' )?                     // optional single call at chain end
```

Resolution order (scope chain), first match wins:

1. Conversation roles (`$SPEAKER`, `$LISTENER`, `$PLAYER`, `$SELF`,
   `$PARTICIPANT[n]`, and any role registered via
   `Loom.Language.registerResolveRole`).
2. Local `let` bindings in the enclosing scope.
3. Vars in the operand registry.
4. Entities in the entity registry.

UPPERCASE → looks like a role. Lowercase → looks like a var/entity.
Explicit qualifiers exist but are rarely needed:
```
$var:trust        // force var lookup
$entity:elena     // force entity lookup
$role:SPEAKER     // force role lookup
```

### 10.2 Static reference (`@`)

```
StaticRef
  ::= '@' , IDENT , ('.' , IDENT)*                  // @lighthouse, @sfx.bell
```

`@`-refs are validated at build time against the project's asset / scene
/ sound / location registries. Unknown refs are hard errors.

### 10.3 Content link (`[[ ]]`)

```
ContentLink
  ::= '[[' , LinkBody , ']]'

LinkBody
  ::= LinkType? , IDENT , ('|' , LinkDisplay)?

LinkType  ::= IDENT , ':'                           // object:elena, codex:tower_lore
LinkDisplay ::= TextContent                          // free text up to closing ]]
```

Content links never affect runtime control flow. They are stripped (or
rendered as hyperlinks) by the display layer. The build extracts a
manifest of links per document for editor navigation and dead-link
detection.

### 10.4 Inline assign (`< ... >`)

```
InlineAssign
  ::= '<' , LValue , AssignOp , Expr , '>'

AssignOp
  ::= ':=' | '+=' | '-=' | '++'
```

Fires at the exact character position during typewriter reveal.

### 10.5 Ledger predicates

Built-in identifiers that resolve to ledger queries. They are
expression atoms (parse like function calls) but **only** legal in
condition contexts:

```
LedgerPred
  ::= 'played'  , '(' , IDENT , ')'                 // any visit to entry
    | 'visits'  , '(' , IDENT , ')'                 // visit count (returns NUMBER)
    | 'chose'   , '(' , IDENT , ')'                 // any choice of given ID
    | 'last'    , '(' , LedgerField , ')'           // most recent of …
    | 'since'   , '(' , IDENT , ')'                 // duration since last fire (NUMBER s)
    | 'count'   , '(' , LedgerField , ')'           // total of …

LedgerField
  ::= 'speaker' | 'choice' | 'event'
```

Examples:
```
if visits(wren_talk) >= 3
if since(bell_rung) < 30s
if last(speaker) == @wren
```

---

## 11. Inline text grammar

Once the parser has decided a region of source is **text** (a
`TextContent` inside `Dialogue`, `Choice` label, `FlavorLine`,
`Parenthetical`, or `LinkDisplay`), it switches to the inline grammar.
This is a separate sublanguage with its own tokenizer.

```
TextContent
  ::= TextElement*

TextElement
  ::= LiteralRun
    | EscapedChar
    | ContentLink                                   // [[ ... ]]
    | TextVariation                                 // [ ... / ... ].mode
    | InlineEval                                    // ${...} or $(...)
    | ResolveRef                                    // $name, $name.field
    | StaticRef                                     // @name
    | InlineTrigger                                 // <type:args>
    | InlineAssign                                  // <$var := expr>
    | InlineDelim                                   // registry-driven: <<...>>, {{...}}, etc.
    | RangeCloser                                   // </> or </%name>
    | InlineRichSpan                                // {{tag}} … {{/tag}}  (optional, registry)

LiteralRun  ::= (TextChar - SpecialStart)+
SpecialStart ::= '$' | '@' | '[' | '<' | '\\' | '/' (only when followed by '*' or '/')
EscapedChar  ::= '\\' , [<\[{$@/\\]
```

### 11.1 Text variation

```
TextVariation
  ::= '[' , Variant , ('/' , Variant)* , ']' , VariationMode?

Variant
  ::= TextContent                                    // recursive

VariationMode
  ::= '.' , IDENT , ('(' , ArgList? , ')')?         // .cycle, .shuffle, .once, .stopping, .weighted(0.7,0.3)
```

The mode defaults to `.stopping` (advance through variants on
successive visits, stick on the last). All other modes must be
explicit.

### 11.2 Inline trigger

```
InlineTrigger
  ::= '<' , TriggerType , ':' , TriggerArgs , NamedAttrs? , RangeSpec? , '>'

TriggerType   ::= IDENT                              // registry-validated
TriggerArgs   ::= TextChar*                          // type-specific parse via registry
NamedAttrs    ::= (' ' , IDENT , ':' , TriggerArgValue)+
RangeSpec     ::= ' for:' , (NUMBER | Expr)
               | ' %' , IDENT                        // named anchor

RangeCloser
  ::= '</>'                                          // close innermost open range
    | '</%' , IDENT , '>'                            // close named range
```

A non-rangeable trigger is a point event (`<sfx:bell>`). A rangeable
trigger opens until a matching `</>` (innermost) or `</%name>` (named)
closes it. A `for:N` spec closes automatically after N characters /
seconds (type-dependent).

### 11.3 Chain trigger

A short-hand for firing multiple co-located triggers:

```
ChainTrigger
  ::= '<' , TriggerHead , ('+' , TriggerHead)+ , '>'

TriggerHead
  ::= TriggerType , ':' , TriggerArgs , NamedAttrs?
```

E.g. `<sfx:bell+camera:shake>` is equivalent to
`<sfx:bell><camera:shake>` with identical character offsets.

### 11.4 Conditional trigger

```
CondTrigger
  ::= '<?' , Expr , '>' , InlineTrigger
```

The trigger fires only if the expression is true at the moment the
typewriter reaches it.

### 11.5 Inline rich span (registry-driven)

Studios register paired delimiters that wrap a span of text with a
named visual style:

```
InlineDelim
  ::= RegisteredOpen , TextContent , RegisteredClose
```

E.g. `<<stage whisper>>` if the registry knows `<<` / `>>`,
`~~corrupted~~` if it knows `~~` / `~~`. Conflicting delimiters with
core sigils are registration-time errors.

---

## 12. Indent semantics by line class

This table is the parser's source of truth for "what can come next".

| If the current line is… | Then INDENTed children may be… |
|---|---|
| `Header` | `Property` (one level deeper) |
| `Section` | `Content` (any kind) |
| `Dialogue` (speaker line) | `TextLine` (text only) |
| `Choice` | `Content` (any kind) |
| `EachVisit` | `VisitBranch` only |
| `After` / `Otherwise` | `Content` |
| `When` / `Match` | `MatchArm` / `Content` |
| `BarkSection` | `Dialogue` only |
| `QuestStage` | `FlavorLine`, `ObjectiveLine`, `LifecycleLine` |
| `TimecodeBlock` (`at` line) | `TrackCommand`, `Dialogue` |

A line whose indent says "I am a child of X" but whose class is not in
that row is a parse error (`unexpected-child`) with a helpful message
naming the legal children.

---

## 13. Reserved space (forbidden combinations)

These combinations are syntactically expressible but semantically
forbidden. The parser accepts them with a diagnostic; the validator
rejects them as hard errors.

| Forbidden | Reason |
|---|---|
| `=` in `~` action line or `<>` inline assign | `=` is binding, not mutation. Use `:=`. |
| `:=` in `let` / `define` / `defn` | `:=` is mutation, not binding. Use `=`. |
| `==` on the left of any assignment | `==` is comparison only. |
| Two chained method calls (`$a.f().g()`) | See §9.1. |
| Section ID collides with KW | Section IDs are user-scoped; a name like `if` makes diverts ambiguous. |
| Speaker label that lowercases to a reserved KW | Same. `IF` as a speaker name parses, but is rejected. |
| Tunnel call returning to a sticky hub | The hub already loops; the tunnel return is redundant and confusing. Warn, don't error. |
| Mutation of a role (`~ $SPEAKER := …`) | Roles are read-only bindings; mutate fields, not the role. |

---

## 14. Worked example

A minimal but feature-dense fragment. The parse tree is sketched in
comments to the right.

```loom
# harbor_greeting "The Harbor"               # Document(Header(id, title))
  .actors wren, hale                         # Property
  .tags chapter-1                            # Property

'''
Opening scene at the dock.
'''                                          # Docstring

(define met_wren = false)                    # ConstantDef
let trusted = $wren_trust > 30 and $met_wren # LetBinding (reactive)

-- start                                     # Section "start"

WREN { worried }                             # Dialogue, CharBlock { emotion: worried }
  The bell went silent three days ago.       #   TextLine
  Maren's gone.                              #   TextLine

  * I'll help. -> investigate                # Choice (once), Divert
      WREN { surprised }                     # nested Dialogue
        You will?<pause:300>                 #   TextLine + InlineTrigger
      var $met_wren := true                  # ActionLine (mutation)
      -> investigate                         # Divert

  * Not my problem. -> leave  if not trusted # Choice with Guard

-- investigate                               # Section "investigate"

after $trusted                               # Block: After
  WREN { warm }                              #   Dialogue
    [[object:maren|Maren]] taught me to      #     ContentLink
    listen to <speed:0.7>the water</>.       #     RangedTrigger + Closer

otherwise                                    # Block: Otherwise (paired with After)
  WREN                                       #   Dialogue
    She just... stopped.                     #     TextLine
```

---

## 15. What this grammar deliberately doesn't specify

- **Whitespace inside expression operators.** Spaces around `>=`, `:=`,
  `==`, etc. are always legal and stripped. The lexer normalizes.
- **String quoting style.** Only `"…"` for now. Single-quote is
  reserved for a future "raw string" form (`'…'` or `'…'r`) — left
  open.
- **Number formatting.** No hex, no octal, no underscores in numerics
  in v2.0. Reserved for v2.1 if needed.
- **Localized keywords.** Keywords are ASCII. Display text is full
  Unicode. Localizing the *language* itself is out of scope.
- **The bytecode / IR.** This is a source grammar. The compiled
  `LoomDatabase` shape is defined in
  [`loom-design.md` §8](loom-design.md#8-pipeline-source--bundle--runtime).

---

## 16. Diagnostics & lint catalog

The parser emits diagnostics with these IDs. Lint rules (warnings)
above the line; hard errors below.

| ID | Severity | When |
|---|---|---|
| `indent-mixed` | error | Tab/space mixing within a single indent unit. |
| `indent-jump` | warning | Indent jumps > 1 level. |
| `indent-multiple` | warning | Indent not a multiple of the established unit. |
| `doc-header-missing` | error | File contains content but no `#` header. |
| `doc-header-id` | error | `#` followed by no identifier. |
| `doc-type-unknown` | warning | Header tag not in registry. |
| `section-duplicate` | error | Two sections with the same id in one doc. |
| `section-empty` | info | Section has no content (sometimes intentional). |
| `unknown-action-kw` | error | Action keyword not in registry. |
| `unknown-trigger-kw` | warning | Trigger type not in registry (still dispatched as generic event). |
| `unknown-role` | error | `$ROLE` references an unregistered role. |
| `unknown-var` | error | `$name` not in operand registry. |
| `unknown-ref` | error | `@name` not in any registry. |
| `divert-target-unknown` | error | `->` points at a non-existent section. |
| `method-chain-too-long` | error | More than one trailing call in a postfix chain. |
| `assign-op-mismatch` | error | `=` in mutation context or `:=` in binding context. |
| `mutation-of-role` | error | `~ $ROLE := …`. |
| `bracket-unbalanced` | error | Open bracket with no matching close. |
| `range-unclosed` | warning | Ranged trigger opens but never closes by EOL. |
| `speaker-no-text` | warning | Speaker block with no dialogue lines. |
| `stacked-conditions` | warning | Multiple `?` / `if` lines back-to-back. |
| `orphaned-condition` | warning | `if` / `?` with no entry below it. |
| `pin-action-implicit` | warning | `NextLink` with a condition but no `.skip` / `.block` modifier. |

These are stable IDs — tooling can suppress them by ID.

---

## 17. Conformance

A Loom parser is **conformant** if, for every `.loom` file in
`tests/golden/`:

1. It produces an AST `==` (deep-equal modulo source ranges) to the
   golden AST.
2. It produces a `LoomDatabase` `==` (postcard-byte-equal) to the
   golden bundle.
3. Its diagnostic set is exactly the golden diagnostic set (no
   missing, no extra).

Round-trip is **not** part of conformance — `ast::print` followed by
`parse` need only produce an `==` AST, not byte-identical source.
Loom is a one-way pipeline: source is for humans, AST is for the
compiler.
