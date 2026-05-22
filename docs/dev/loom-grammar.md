# Loom — Formal Grammar

> The grammar of `.loom` source. Precise enough to drive a parser; loose
> enough to read.

**Status:** initial draft (2026-05-22). Companion to
[`loom-design.md`](loom-design.md).

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
                                                       // logic / expressions
if  and  or  not  is  has  in  of
                                                       // bindings / definitions
var  let  define  defn  defmacro
                                                       // story actions
fire  advance  trigger  modify
                                                       // story blocks
each  visit  first  then  finally
after  otherwise  when  match
                                                       // module system
import  export
                                                       // literals
true  false  nil
                                                       // live performance
cast  cue  cohort  location
broadcast  improv  enroll  as  joins  leaves  enters  exits
participant
                                                       // characters (§7)
type  extends  knowledge  goal  disposition
mirror  on  meeting  passes  drops  below
reacts  init  range
                                                       // stats (§8)
attribute  axis  pool  stat  node  ability  rank
mode  curve  milestones  table  interpolate  lookup
                                                       // axis modes
xp_curve  use_tracking  point_buy  milestone
narrative_trigger  sdk_controlled
                                                       // reactivity / dynamics (§9)
generator  scene  loop  wait  until  at  every
yield  spawn  cancel  await  return  with_chance
random  compose  pattern  bark  from
                                                       // goal knobs
priority  active_when  completes_when  fails_when
drives  on_complete  on_fail
```

`is not`, `has not`, `each visit`, `participant joins`,
`participant leaves`, `participant enters`, `participant exits`,
`passes`, `drops below`, and `wait until` are multi-word lexical
tokens; the lexer joins them with look-ahead.

### 3.2 Speaker vs identifier

A `SPEAKER` is an all-caps identifier *at the start of a line, after
indent, with no preceding sigil*, optionally followed by a character
block (`{ ... }`) or a `^` (for dual dialogue). Anywhere else,
`[A-Z][A-Z0-9_]*` lexes as a regular `IDENT`.

A `$UPPER` form (`$NARRATOR`, `$SPEAKER`) is a **resolve reference**,
not a literal speaker — its value is resolved at runtime and used as
the speaker label. See §11.1.

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
| `:script` | stage / film script | `Section`, `SluglineScene`, declarations (no choices) |
| `:film` | film screenplay | `Section`, `SluglineScene`, `TimecodeBlock`, declarations (no choices) |
| `:immersive` | live immersive theatre | every Body item including `ParticipantLifecycle`, `LocationEvent`, `Broadcast` |
| `:character` | character archetype | `CharacterBody` items only (§7) |
| `:type` | character-type definition | `TypeBody` items only (§7.1) |
| `:stats` | stats sheet | `StatsBody` items only (§8) |
| `:tree` | progression tree | `TreeNodeDecl`+ only (§8.4) |
| `:typewriter` | typewriter profile | property-only (no body) |
| `:module` | shared module | declarations only (no sections) |

A `:script` document is a linear screenplay — choices (`*`, `+`) are a
parse error, but cues, cast, beats, and (optionally) `improv` blocks
are allowed. A `:film` document is the same plus `TimecodeBlock`s for
shot-list-style sequences. A `:immersive` document opens every
construct.

Unknown tags are a warning (`doc-type-unknown`), not an error — they're
stored as free-form metadata for tooling.

### 4.2 Top-level items

```
TopLevelItem
  ::= Section
    | SluglineScene                                 // :script, :film, :immersive
    | BarkSection                                   // :barks
    | QuestStage                                    // :quest
    | TimecodeBlock                                 // :cutscene, :film
    | CastDecl                                      // any archetype
    | CueDecl                                       // any archetype
    | LocationDecl                                  // :immersive
    | CohortDecl                                    // :immersive
    | ParticipantLifecycle                          // :immersive
    | LocationEvent                                 // :immersive
    | BroadcastBlock                                // :immersive
    | CharacterBodyItem                             // :character (§7)
    | TypeBodyItem                                  // :type (§7.1)
    | StatsBodyItem                                 // :stats (§8)
    | TreeNodeDecl                                  // :tree (§8.4)
    | GeneratorDecl                                 // any (§9.3)
    | SceneCoroutineDecl                            // any (§9.4)
    | ComposeDecl                                   // any (§9.6)
    | Declaration
    | Definition
    | Import
    | Binding
    | Comment
```

---

## 5. Performance-model declarations

The constructs that make Loom a language for performed media — cast,
cue, location, cohort, participant lifecycle, broadcast — share a
declarative shape: a keyword, an identifier, optional indented
properties, optional body. They live at the document top level (not
inside a section). The runtime engine reads them once at bundle load
and never again.

### 5.1 Cast declaration

```
CastDecl
  ::= 'cast' , CastId , STRING? , NL
  , (INDENT , CastProperty+ , DEDENT)?

CastId
  ::= SPEAKER                                       // ALICE, BELLKEEPER
    | '@' , IDENT                                   // @entity reference (for game variants)

CastProperty
  ::= '.label' , STRING , NL
    | '.voice' , IDENT , NL
    | '.bio' , DOCSTRING , NL
    | '.open' , NL                                  // any performer may bind at runtime
    | '.voiceover' , NL                             // disembodied (no physical performer)
    | '.improv' , ImprovSpec , NL
    | '.' , IDENT , PropertyValue? , NL             // open extension, registry-validated

ImprovSpec
  ::= '.latitude' , '(' , NUMBER , ')'              // 0..1
    | '.topic' , '(' , STRING , ')'
    | '.duration' , '(' , Duration , ')'

Duration
  ::= NUMBER , ('s' | 'ms' | 'm')                   // 60s, 250ms, 5m
```

A `CastDecl` introduces a `SPEAKER` slot the parser will accept on
dialogue lines. A SPEAKER appearing in dialogue without a prior
`CastDecl` is a hard error (`unknown-cast`); the slot must be declared
somewhere reachable (current file or imported module).

### 5.2 Cue declaration

```
CueDecl
  ::= 'cue' , IDENT , NL
  , INDENT , CueProperty+ , DEDENT

CueProperty
  ::= '.target' , IDENT , NL                        // crew-bus channel
    | '.preset' , IDENT , NL                        // bus-specific preset name
    | '.fade' , Duration , NL
    | '.level' , NUMBER , NL                        // 0..1
    | '.fires' , CueRef (',' , CueRef)* , NL        // group cue (composite)
    | '.group' , IDENT , NL                         // cue-list group
    | '.' , IDENT , PropertyValue? , NL             // open extension

CueRef
  ::= IDENT                                         // sibling cue
    | IDENT , ':' , (-NL)*                          // inline trigger form (sfx:bell)
```

Cue invocation, two equivalent forms (one action-line, one inline
trigger):

```
CueAction
  ::= '~'? , 'cue' , CueTarget , NL
    | '~'? , 'cue' , CueTarget , CueOverrides? , NL

CueTarget
  ::= IDENT                                         // bare cue name
    | '@' , IDENT                                   // explicit static ref

CueOverrides
  ::= '{' , CueProperty (',' CueProperty)* , '}'    // ad-hoc property overrides
```

Inline cue dispatch uses the existing trigger syntax: `<cue:name>`
inside text. The grammar for inline triggers (§12.2) recognizes
`cue` as a built-in trigger type whose argument is a `CueRef`.

### 5.3 Location declaration

```
LocationDecl
  ::= 'location' , LocationId , STRING? , NL
  , INDENT , LocationProperty+ , DEDENT

LocationId
  ::= SPEAKER                                       // UPPERCASE: BELL_TOWER, HARBOR_OFFICE
    | '@' , IDENT                                   // when re-exposing an existing @scene

LocationProperty
  ::= '.label' , STRING , NL
    | '.capacity' , NUMBER , NL
    | '.threshold' , STRING , NL                    // sensor/RFID description
    | '.connects' , LocationId (',' , LocationId)* , NL
    | '.' , IDENT , PropertyValue? , NL
```

### 5.4 Cohort declaration

```
CohortDecl
  ::= 'cohort' , IDENT , NL
  , INDENT , CohortProperty+ , DEDENT

CohortProperty
  ::= '.label' , STRING , NL
    | '.capacity' , NUMBER , NL
    | '.secret' , STRING , NL                       // designer note, not shown to participants
    | '.' , IDENT , PropertyValue? , NL
```

Cohorts are referenced by bare identifier in expressions:
`if $PARTICIPANT in singers`, `if count(singers) >= 6`. They are
**not** SPEAKER-eligible; a cohort is a set of participants, not a
voice.

### 5.5 Participant lifecycle

```
ParticipantLifecycle
  ::= 'when' , 'participant' , LifecycleVerb , LifecycleGuard? , NL
  , INDENT , Content* , DEDENT

LifecycleVerb
  ::= 'joins'  | 'leaves'

LifecycleGuard
  ::= ':' , IDENT                                   // cohort filter — joins :audience, leaves :singers
```

The synthetic variable `$PARTICIPANT` is bound to the joining/leaving
participant inside the body. Lifecycle blocks may fire `enroll`
actions (§5.7).

### 5.6 Location event

```
LocationEvent
  ::= 'when' , 'participant' , LocationVerb , StaticRef , NL
  , INDENT , Content* , DEDENT

LocationVerb
  ::= 'enters' | 'exits'
```

`$PARTICIPANT` is bound inside the body to the participant whose
movement triggered the event. The `StaticRef` must resolve to a
`LocationDecl`; unknown locations are build errors.

### 5.7 Enrollment action

```
EnrollAction
  ::= 'enroll' , EnrollSubject , 'into' , IDENT , NL

EnrollSubject
  ::= ResolveRef                                    // $PARTICIPANT, $alice
    | IDENT                                         // bare cohort name (move all)
```

`enroll <participant> into <cohort>` adds the participant to a cohort.
`enroll <cohort_a> into <cohort_b>` moves every member.

### 5.8 Broadcast block

```
BroadcastBlock
  ::= 'broadcast' , BroadcastScope , NL
  , INDENT , Content* , DEDENT

BroadcastScope
  ::= ':' , 'all'
    | ':' , ScopeAtom
    | BroadcastScope , 'and' , ':' , ScopeAtom
    | BroadcastScope , 'but' , ':' , ScopeAtom

ScopeAtom
  ::= 'cohort'      , '(' , IDENT , ')'
    | 'location'    , '(' , StaticRef , ')'
    | 'participant' , '(' , ResolveRef , ')'
    | 'cast'        , '(' , SPEAKER , ')'
```

A broadcast wraps content with a runtime audience filter — only
participants matching the scope receive entries from inside the block.
Scopes compose with `and` (intersection) and `but` (set difference).
A bare `broadcast :all` block is identical in semantics to its body
without the wrapper; it exists for readability.

### 5.9 Slugline scene (for :script and :film archetypes)

```
SluglineScene
  ::= '##' , SceneSlug , STRING? , Modifier* , NL
  , Docstring?
  , Content*

SceneSlug
  ::= IDENT                                         // scene_3
    | IDENT , '.' , IDENT                           // act_2.scene_3
```

A `SluglineScene` is a sub-section under a `:script` or `:film`
archetype. The slug carries the screenplay convention (act/scene
numbering); the optional `STRING` is the slugline (`"INT. HARBOR — LATE
AFTERNOON"`). Slugline scenes accept the same `Content` items as
sections except `Choice`.

> **Disambiguation.** The keyword `scene` is reserved for the
> *reactive scene* coroutine — see [§18.2](#182-scene-coroutine) and
> [design §9.4](loom-design.md#94-scenes). A slugline scene has no
> keyword; it is introduced by `##` and lives only inside `:script` /
> `:film` document bodies. The two never collide syntactically and
> never share a non-terminal.

### 5.10 The `as <participant>` modifier

```
ParticipantScope
  ::= 'as' , ('participant' | ResolveRef)
```

The `as participant` modifier appears on `Section` headers, `Divert`
targets, and the `LocationEvent` body to scope state to a single
participant:

```
-- bell_revelation as participant
  ...

-> bell_revelation as $PARTICIPANT
```

Inside a `ParticipantScope`d region, unqualified `$name` lookups
resolve to `$PARTICIPANT.<name>` first, then fall back to show-global.
A section without `as` is **show-global**; a section with `as
participant` is **per-participant** and may be entered concurrently by
many participants without interference.

The bare keyword `participant` (no `$`) inside a scope marker means
"the current participant, whoever they are at runtime" — equivalent to
`$PARTICIPANT` but emphasizes the scope is structural, not a
reference.

### 5.11 Improv parenthetical

```
ImprovParenthetical
  ::= '(' , 'improv' , ImprovSpec* , (' about:' , STRING)? , ')'
```

Appears in two positions:

1. **On a Speaker line** between the speaker and the line break:
   ```
   BELLKEEPER (improv .duration(45s))
     > Greet warmly. Find out why they came.
     -> next_beat
   ```
   The indented body is a directive to the performer, not literal
   dialogue (the leading `>` makes it a `FlavorLine`).

2. **As a standalone line** between two dialogue lines, scoping a beat
   of improv between scripted lines:
   ```
   BELLKEEPER
     Why are you here?
     (improv ~30s about: "what you might do about it")
     And what will you do about it?
   ```

The runtime treats both forms as an `ImprovBeat` — see
[`loom-design.md` §5.3](loom-design.md#53-improvisation) for the
playback semantics.

---

## 6. Sections

### 6.1 Conversation sections

```
Section
  ::= '--' , IDENT? , Modifier* , ParticipantScope? , Guard? , NL
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

### 6.2 Bark sections

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

### 6.3 Quest stages

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

### 6.4 Cutscene timecode blocks

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

## 7. Content (inside sections)

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

### 7.1 Dialogue

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

### 7.2 Stage direction & flavor

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

### 7.3 Choice

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

### 7.4 Divert & return

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

### 7.5 Action line

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

### 7.6 Annotation

```
Annotation
  ::= '@' , IDENT , AnnotationBody? , NL

AnnotationBody
  ::= (-NL)*                                         // shape per-annotation, parsed by registry
```

Built-in annotations: `@vo`, `@director`, `@status`, `@note`, `@hint`,
`@loc`. Open extension namespace: `@<vendor>:<name>` (e.g.
`@studio:rating teen`).

### 7.7 Blocks (each visit / after / when / match)

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

## 8. Declarations & definitions

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

## 9. S-expressions

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

## 10. Expression grammar

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
    | IDENT                                          // identifier (resolved by §11)
    | ResolveRef
    | StaticRef
    | LedgerPred                                     // §11.6
    | '(' , Expr , ')'                               // grouping
    | SExpr                                          // (op args...) lisp form
```

### 10.1 Method calls don't chain

`Postfix '(' ArgList? ')'` is legal **only at the end of a postfix
chain** — i.e. `$elena.faction.getStanding($PLAYER)` parses, but
`$elena.getThing().getOther()` is a parse error. Use a `let` binding to
break it apart. (Rationale: registered method calls are typed against
their return; chained calls explode the type lattice.)

### 10.2 Implicit grouping in `if`

After the `if` keyword (at the start of a `Guard`), an unwrapped
expression is parsed as a single `Expr` ending at the first newline.
Parentheses are optional for the outermost grouping:

```
if trust > 50 and met_wren
if (trust > 50 and met_wren)         // identical
```

---

## 11. Names, references, sigils

Loom has three reference sigils. They look similar and answer related
questions, but they commit at three different moments and fail in
three different ways. The grammar makes the distinction
non-negotiable: each sigil has its own non-terminal, its own validation
pass, and its own diagnostic family.

| Sigil | Question | Resolves at | Failure mode | Grammar |
|---|---|---|---|---|
| `$` | "What is this **right now**?" | Runtime | Build error if undeclared; runtime nil/error if missing | §11.1 |
| `@` | "Does this **exist** in the project?" | Build time | Build error if not in any registry | §11.2 |
| `[[ ]]` | "What is this **related to**?" | Author time | Build warning only — show plays correctly | §11.3 |

A practical test for which sigil to use: **delete the codepoint and
re-read the line.**

- If the meaning changes immediately and the line behaves differently:
  it's `$` (load-bearing at runtime).
- If the build now fails to validate: it's `@` (load-bearing at
  build).
- If the text reads identically and the show plays correctly: it's
  `[[ ]]` (decoration; informational only).

The remainder of this section defines each form precisely.

### 11.1 Resolve reference (`$`)

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

1. **Participant scope** (when current section / divert is
   `as participant`): a bare `$name` lookup tries
   `$PARTICIPANT.<name>` first, then falls back to step 2.
2. **Conversation roles** — `$SPEAKER`, `$LISTENER`, `$PLAYER`,
   `$SELF`, `$PARTICIPANT`, `$PARTICIPANT[n]`, and any role registered
   via `Loom.Language.registerResolveRole`.
3. **Local `let` bindings** in the enclosing scope.
4. **Vars** in the operand registry.
5. **Entities** in the entity registry.
6. **Cohorts** (for membership queries like `if $PARTICIPANT in
   singers`; the cohort name `singers` here resolves as a special
   form).

UPPERCASE → looks like a role. Lowercase → looks like a var/entity.
Explicit qualifiers exist but are rarely needed:
```
$var:trust        // force var lookup
$entity:elena     // force entity lookup
$role:SPEAKER     // force role lookup
$cohort:singers   // force cohort lookup
```

**`$` is load-bearing.** Deleting the sigil changes a runtime lookup
into a bare identifier, which usually parses as something but does
something different. Unknown `$`-references are **build errors**
(typo protection); undeclared variables can't sneak through.

### 11.2 Static reference (`@`)

```
StaticRef
  ::= '@' , IDENT , ('.' , IDENT)*                  // @lighthouse, @sfx.bell
```

`@`-refs are validated at build time against the project's static
registries. The single sigil covers everything declarable in the
project:

| `@`-ref shape | Registry checked | Example |
|---|---|---|
| `@<section>` | section IDs (current document) | `-> @harbor_intro` |
| `@<doc>.<section>` | section IDs (named document) | `-> @lighthouse.entry` |
| `@<asset>.<path>` | asset registry | `<sfx:@bell>` |
| `@<entity>` | entity registry | `cast @WREN`, `if $SPEAKER == @elena` |
| `@<location>` | location declarations (§5.3) | `when participant enters @BELL_TOWER` |
| `@<cue>` | cue declarations (§5.2) | `~ cue @lights_warm` |

A name appearing as the target of a divert, cast slot, asset
reference, location event, or cue invocation is **implicitly** an
`@`-ref — the leading `@` is recommended for clarity but optional
in those positions. In ambiguous contexts (an expression body, a
condition), `@` is required.

**`@` is load-bearing at build.** Deleting the sigil in `-> @harbor`
gives `-> harbor` which usually still works (positional context tells
the parser to look for a section), but in
`if $faction == @elena.faction` deleting the `@` would make
`elena.faction` look like a runtime entity field — different
semantics. Unknown `@`-references are **build errors**: the validator
checks every registry before parse completes.

### 11.3 Backlink (`[[ ]]`)

```
Backlink
  ::= '[[' , BacklinkBody , ']]'

BacklinkBody
  ::= BacklinkType? , BacklinkTarget , ('|' , BacklinkDisplay)?

BacklinkType    ::= IDENT , ':'                     // object:elena, codex:tower_lore
BacklinkTarget  ::= IDENT
                  | TextContent                     // free-form when type='codex' or absent
BacklinkDisplay ::= TextContent                     // free text up to closing ]]
```

Backlinks are **decoration on prose**. They appear only inside
`TextContent` (dialogue, choice labels, flavor lines,
parentheticals). They never affect runtime control flow, never
contribute to conditions, never gate diverts, never bind cast.

The build extracts a backlink manifest per document for editor
navigation and "dead link" warnings, but a dangling backlink **does
not fail the build**. This is intentional — writers create backlinks
to *aspirational* codex entries during drafting, and the codex
follows.

A `[[name]]` without a type prefix defaults to the project's primary
backlink registry (typically the codex). `[[type:name]]` qualifies:
`[[object:elena]]`, `[[event:bell_rings]]`, `[[codex:tower_lore]]`.
Studios can register additional types.

**`[[ ]]` is decoration.** Deleting the sigil and its content removes
a hyperlink hint from rendered text; the surrounding prose continues
to read correctly. This is the test: if removing the codepoint leaves
the show playing identically, `[[ ]]` is the right sigil.

### 11.4 Why not unify `@` and `[[ ]]`?

The temptation is real — both look up names against project-wide
registries. The reason they stay separate is the **failure
contract**:

- `@elena` declares: "this script *needs* Elena to exist; if Elena is
  removed, this script is broken."
- `[[elena|Elena]]` declares: "this prose *mentions* Elena; hyperlink
  her if she exists, otherwise just render the text."

Both can resolve to the same target. The sigil declares the
**coupling strength**, not the destination. Unifying them would force
a single failure mode — either every backlink becomes a build error
(which kills the drafting workflow) or every static reference becomes
a soft warning (which lets typos through to runtime). Neither
trade-off is acceptable. Two sigils, two contracts, both small.

### 11.5 Inline assign (`< ... >`)

```
InlineAssign
  ::= '<' , LValue , AssignOp , Expr , '>'

AssignOp
  ::= ':=' | '+=' | '-=' | '++'
```

Fires at the exact character position during typewriter reveal.

### 11.6 Ledger predicates

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

## 12. Inline text grammar

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

### 12.1 Text variation

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

### 12.2 Inline trigger

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

### 12.3 Chain trigger

A short-hand for firing multiple co-located triggers:

```
ChainTrigger
  ::= '<' , TriggerHead , ('+' , TriggerHead)+ , '>'

TriggerHead
  ::= TriggerType , ':' , TriggerArgs , NamedAttrs?
```

E.g. `<sfx:bell+camera:shake>` is equivalent to
`<sfx:bell><camera:shake>` with identical character offsets.

### 12.4 Conditional trigger

```
CondTrigger
  ::= '<?' , Expr , '>' , InlineTrigger
```

The trigger fires only if the expression is true at the moment the
typewriter reaches it.

### 12.5 Inline rich span (registry-driven)

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

## 13. Indent semantics by line class

This table is the parser's source of truth for "what can come next".

| If the current line is… | Then INDENTed children may be… |
|---|---|
| `Header` | `Property` (one level deeper) |
| `Section` | `Content` (any kind) |
| `SluglineScene` | `Content` (no `Choice` under `:script` / `:film`) |
| `Dialogue` (speaker line) | `TextLine` (text only); or `FlavorLine` under `(improv …)` |
| `Choice` | `Content` (any kind) |
| `EachVisit` | `VisitBranch` only |
| `After` / `Otherwise` | `Content` |
| `When` / `Match` | `MatchArm` / `Content` |
| `BarkSection` | `Dialogue` only |
| `QuestStage` | `FlavorLine`, `ObjectiveLine`, `LifecycleLine` |
| `TimecodeBlock` (`at` line) | `TrackCommand`, `Dialogue` |
| `CastDecl` / `CueDecl` / `LocationDecl` / `CohortDecl` | `CastProperty` / `CueProperty` / `LocationProperty` / `CohortProperty` |
| `ParticipantLifecycle` (`when participant joins/leaves`) | `Content` |
| `LocationEvent` (`when participant enters/exits @LOC`) | `Content` |
| `BroadcastBlock` | `Content` |
| `:character` document body | `CharacterBodyItem` (§16) |
| `:type` document body | `TypeBodyItem` (§16) |
| `:stats` document body | `StatsBodyItem` (§17) |
| `:tree` document body | `TreeNodeDecl` (§17.5) |
| `KnowledgeBlock` | `KnowledgeField` only |
| `GoalDecl` | `GoalKnob` only |
| `DispositionBlock` | `DispositionItem` only |
| `HookDecl` (`on …`) | `Content` |
| `SlotBlock` (named slot) | `SlotProperty` only |
| `AxisDecl` | `AxisProperty` only |
| `PoolDecl` | `PoolProperty` only |
| `StatDecl` (lookup form) | `LookupProperty` only |
| `TreeNodeDecl` | `TreeNodeProperty` only |
| `GeneratorDecl` | `GeneratorItem` (§18.1) |
| `SceneCoroutineDecl` | `SceneState` (§18.2) |
| `SceneState` | `SceneStateItem` |
| `LoopBlock` | `GeneratorItem` |
| `ComposeDecl` | `PatternBlock` |
| `PatternBlock` | `PatternArm` only |
| `YieldStmt` (body form) | `Content` |

A line whose indent says "I am a child of X" but whose class is not in
that row is a parse error (`unexpected-child`) with a helpful message
naming the legal children.

---

## 14. Reserved space (forbidden combinations)

These combinations are syntactically expressible but semantically
forbidden. The parser accepts them with a diagnostic; the validator
rejects them as hard errors.

| Forbidden | Reason |
|---|---|
| `=` in `~` action line or `<>` inline assign | `=` is binding, not mutation. Use `:=`. |
| `:=` in `let` / `define` / `defn` | `:=` is mutation, not binding. Use `=`. |
| `==` on the left of any assignment | `==` is comparison only. |
| Two chained method calls (`$a.f().g()`) | See §10.1. |
| Section ID collides with KW | Section IDs are user-scoped; a name like `if` makes diverts ambiguous. |
| Speaker label that lowercases to a reserved KW | Same. `IF` as a speaker name parses, but is rejected. |
| Tunnel call returning to a sticky hub | The hub already loops; the tunnel return is redundant and confusing. Warn, don't error. |
| Mutation of a role (`~ $SPEAKER := …`) | Roles are read-only bindings; mutate fields, not the role. |
| `yield` outside a generator/scene body | `yield` is a coroutine primitive. Use a story line instead. |
| `wait` / `every` / `at` outside a coroutine | Same — they yield control to the scheduler. |
| `return` outside a scene body | Only scenes return values. Use `<-` for conversation returns. |
| Knowledge field of a non-closed type (e.g. `dict<…>`) | Knowledge stays scannable; use a `var` for richer payloads. |
| Arithmetic on a knowledge field (`$c.<k> += 1`) | Knowledge isn't a counter — declare a `var`. |
| `GoalDecl` without `priority` | Resolver needs a tie-break key; explicit is better than implicit. |
| Disposition `mirror` cycle | A mirrors B mirrors A creates a feedback loop; resolver refuses. |
| Tree node `requires` cycle | DAG required for topo-sort of unlocks. |
| `stat X = expr` followed by indented body | Pick one form; mixed shape is `stat-form-ambiguous`. |
| Reactive `let` rebind in the same scope | Reactive bindings are write-once. Use `var` + `:=`. |

---

## 15. Worked examples

### 15.1 Game / branching dialogue

A minimal but feature-dense fragment. The parse tree is sketched in
comments to the right.

```loom
# harbor_greeting "The Harbor"               # Document(Header(id, title))
  .actors wren, hale                         # Property
  .tags chapter-1                            # Property

'''
Opening scene at the dock.
'''                                          # Docstring

cast WREN                                    # CastDecl
  .label "Wren the Fisher"
  .voice female_mezzo

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
    [[object:maren|Maren]] taught me to      #     Backlink
    listen to <speed:0.7>the water</>.       #     RangedTrigger + Closer

otherwise                                    # Block: Otherwise (paired with After)
  WREN                                       #   Dialogue
    She just... stopped.                     #     TextLine
```

### 15.2 Live immersive theatre

The same harbor scene as an immersive piece: shared cues, per-
participant private scenes, an improv beat, and a location-triggered
broadcast.

```loom
# bell_tower :immersive                      # Header :immersive archetype
  .cohort initiate, singers                  # Properties

cast BELLKEEPER                              # CastDecl
  .label "The Bellkeeper"
  .open                                      # any performer may bind
  .improv .latitude(0.5)

cue bell_strike_loud                         # CueDecl
  .target sound_console
  .preset bell_main
  .level 0.9

location BELL_TOWER                          # LocationDecl
  .label "The Bell Tower"
  .capacity 12

when participant joins                       # ParticipantLifecycle
  enroll $PARTICIPANT into initiate          #   EnrollAction
  -> orientation as $PARTICIPANT             #   Divert with ParticipantScope

when participant enters @BELL_TOWER          # LocationEvent
  -> bell_first_visit as $PARTICIPANT        #   per-participant section

-- bell_first_visit as participant           # Section with ParticipantScope

  BELLKEEPER (improv .duration(45s))         # ImprovParenthetical on Speaker
    > Greet warmly. Don't reveal what you    #   FlavorLine (directive, not dialogue)
    > actually do here.

  * I came for the bell.                     # Choice
      var $PARTICIPANT.intent = "bell"       #   per-participant mutation
      -> private_revelation as $PARTICIPANT

-- private_revelation as participant

  ~ cue bell_strike_loud                     # CueAction — fires to ALL audience

  broadcast :participant($PARTICIPANT)       # BroadcastBlock with scope
    NARRATOR { whispering }                  #   only this participant hears
      You hear it differently.

  broadcast :location(@BELL_TOWER) but :participant($PARTICIPANT)
    NARRATOR
      The other visitors look up.            # everyone in the tower EXCEPT us

  enroll $PARTICIPANT into singers           # cohort move
  -> rejoin_main
```

---

## 16. Character archetype

A `:character` document declares one character. Its body is a sequence
of `CharacterBodyItem`s — slot blocks, knowledge/goal/disposition
blocks, hooks, char-scoped generators. A `:type` document declares a
*type* (a subtype of `@character`, `@humanoid`, etc.) whose fields are
inherited by characters that name it via `.type`. See
[design §4](loom-design.md#4-characters).

```
CharacterBody
  ::= CharacterBodyItem*

CharacterBodyItem
  ::= SlotBlock
    | KnowledgeBlock
    | GoalDecl
    | DispositionBlock
    | HookDecl
    | GeneratorDecl                                   // §18.1
    | SceneCoroutineDecl                              // §18.2
    | LetBinding
    | Property                                        // .label / .voice / .home / .unique / .type / .bio / .stats
    | Comment
```

A `:type` document's body uses the same shape with the additional
`FieldDecl` and `SlotRequirement` items, and forbids `KnowledgeBlock`
/ `GoalDecl` / `HookDecl` (those belong on instances, not types):

```
TypeBody
  ::= TypeBodyItem*

TypeBodyItem
  ::= FieldDecl
    | SlotRequirement
    | Property                                        // .extends @parent
    | Comment

FieldDecl
  ::= '.field' , IDENT , ':' , TypeExpr , ('=' , Expr)? , 'required'? , NL

SlotRequirement
  ::= '.slot' , IDENT , (':' , 'required')? , NL

TypeExpr
  ::= IDENT                                          // int, float, bool, string
    | 'enum' , '(' , StaticRef (',' , StaticRef)* , ')'
    | 'list' , '<' , TypeExpr , '>'
    | TypeExpr , '?'                                 // nilable
```

### 16.1 Slot block

```
SlotBlock
  ::= IDENT , DictLiteral , NL                       // slot name + literal payload
    | IDENT , NL                                     // slot name only (defaults)
    , (INDENT , SlotProperty+ , DEDENT)?

SlotProperty
  ::= '.' , IDENT , PropertyValue? , NL              // shape per slot registry

DictLiteral
  ::= '{' , DictEntry (',' , DictEntry)* '}'

DictEntry
  ::= IDENT , ':' , (Expr | DictLiteral | ListLiteral)
```

Slot names are validated against the slot registry at parse time; an
unknown slot is `unknown-slot` (error). The contributing layer
(stats / voice / inventory / etc.) registers a slot schema, which
parses the slot's `SlotProperty` set.

### 16.2 Knowledge block

```
KnowledgeBlock
  ::= 'knowledge' , NL
  , INDENT , KnowledgeField+ , DEDENT

KnowledgeField
  ::= IDENT , ':' , KnowledgeType , ('=' , Expr)? , NL

KnowledgeType
  ::= 'bool'
    | 'int'
    | 'float'
    | 'string'
    | '{' , IDENT (',' , IDENT)* , '}'               // closed enum
    | 'list' , '<' , KnowledgeType , '>'
    | KnowledgeType , '?'                            // nilable
```

The narrow type set is intentional — knowledge is "what this character
knows," not a general scratchpad. Anything richer goes in a `var` or
a slot. See [design §4.5](loom-design.md#45-knowledge).

### 16.3 Goal declaration

```
GoalDecl
  ::= 'goal' , IDENT , NL
  , INDENT , GoalKnob+ , DEDENT

GoalKnob
  ::= 'priority'       , '=' , NUMBER          , NL
    | 'active_when'    , '=' , Expr            , NL
    | 'completes_when' , '=' , Expr            , NL
    | 'fails_when'     , '=' , Expr            , NL
    | 'drives'         , 'generator' , IDENT   , NL
    | 'on_complete'    , ActionChain           , NL
    | 'on_fail'        , ActionChain           , NL

ActionChain
  ::= (Action | Divert) (';' , (Action | Divert))*
```

`priority` is required; everything else is optional. A goal with no
`active_when` is always-eligible; a goal with no `completes_when` /
`fails_when` is terminal-by-imperative-call only (see design §4.6
"Imperative control"). See [design §4.6](loom-design.md#46-goals).

### 16.4 Disposition block

```
DispositionBlock
  ::= 'disposition' , DispositionTarget , NL
  , INDENT , DispositionItem+ , DEDENT

DispositionTarget
  ::= ResolveRef                                     // $PLAYER, $alice
    | StaticRef                                      // @everyone (bulk)

DispositionItem
  ::= DispositionAxis
    | DispositionReact

DispositionAxis
  ::= IDENT , '=' , NumRange , (',' , 'init' , NUMBER)? , (',' , MirrorClause)? , NL

NumRange
  ::= NUMBER , '..' , NUMBER                         // 0..100, -1..1, etc.

MirrorClause
  ::= 'mirror' , ResolveRef                          // mirror $PLAYER.disposition.trust

DispositionReact
  ::= 'reacts' , Expr , '->' , IDENT , NL            // produces a runtime tag
```

A `DispositionAxis` declares a single numeric channel with bounds and
init. A `DispositionReact` declares a tag the runtime emits when the
expression is true; the tag is then queryable via
`$X.disposition($Y).is(tag)`. See
[design §4.3](loom-design.md#43-disposition).

### 16.5 Hook declaration

```
HookDecl
  ::= 'on' , HookPattern , NL
  , INDENT , Content* , DEDENT

HookPattern
  ::= 'meeting' , ResolveRef                         // first-contact
    | Expr , 'passes' , Expr                         // upward crossing
    | Expr , 'drops' , 'below' , Expr                // downward crossing
    | Expr , '==' , Expr                             // edge-triggered equality
    | Expr , 'is' , IDENT                            // tag predicate
    | 'cue' , IDENT                                  // crew bus
    | 'event' , IDENT                                // user `fire X`
    | ResolveRef , 'enters' , StaticRef              // location enter
    | ResolveRef , 'exits' , StaticRef               // location exit
    | RegisteredHookPattern                          // registry extension
```

A `HookPattern` whose head matches an extension-registered predicate
is dispatched to the registry's parser for its tail. Unknown patterns
are `unknown-hook-pred` (error). See
[design §4.8](loom-design.md#48-hooks).

---

## 17. Stats archetype

A `:stats` document is a stats sheet — a sharable bundle of
attributes, axes, pools, and stats that characters reference via the
`stats` slot. A `:tree` document declares a progression tree (DAG of
unlockable nodes). See [design §5](loom-design.md#5-stats--progression).

```
StatsBody
  ::= StatsBodyItem*

StatsBodyItem
  ::= AttributeDecl
    | AxisDecl
    | PoolDecl
    | StatDecl
    | LetBinding
    | Comment
```

### 17.1 Attribute declaration

```
AttributeDecl
  ::= 'attribute' , IDENT , '=' , NUMBER , (',' , AttributeMod)* , NL

AttributeMod
  ::= 'range' , NumRange
    | 'min'   , NUMBER
    | 'max'   , NUMBER
```

### 17.2 Axis declaration

```
AxisDecl
  ::= 'axis' , IDENT , NL
  , INDENT , AxisProperty+ , DEDENT

AxisProperty
  ::= 'mode'        , AxisMode                      , NL
    | 'curve'       , Expr                           , NL
    | 'on'          , 'use' , StaticRef              , NL   // use_tracking
    | 'on'          , 'advance' , ActionChain        , NL
    | 'buy'         , 'from' , IDENT                 , NL   // point_buy
    | 'milestones'  , NL , INDENT , MilestoneEntry+ , DEDENT
    | 'advance'     , 'on' , 'event' , IDENT         , NL   // narrative_trigger
    | 'handler'     , StaticRef                      , NL   // sdk_controlled

AxisMode
  ::= 'xp_curve' | 'use_tracking' | 'point_buy'
    | 'milestone' | 'narrative_trigger' | 'sdk_controlled'
    | IDENT                                          // open extension

MilestoneEntry
  ::= NUMBER , ':' , Expr , NL                       // 1: played(intro)
```

### 17.3 Pool declaration

```
PoolDecl
  ::= 'pool' , IDENT , NL
  , INDENT , PoolProperty+ , DEDENT

PoolProperty
  ::= 'max'    , '=' , Expr                          , NL
    | 'min'    , '=' , Expr                          , NL
    | 'regen'  , Expr , ('/' , Duration)? , ('when' , Expr)? , NL
    | 'init'   , '=' , Expr                          , NL
```

### 17.4 Stat declaration

```
StatDecl
  ::= 'stat' , IDENT , '=' , Expr                    , NL   // expression stat
    | 'stat' , IDENT , NL                                   // lookup stat (body follows)
      , INDENT , LookupProperty+ , DEDENT
    | 'stat' , IDENT , 'pool' , PoolPropertyInline+  , NL   // pool sugar
    | 'stat' , IDENT , 'derived' , 'from' , DerivedFrom , NL
      , INDENT , 'formula' , Expr , NL , DEDENT

LookupProperty
  ::= 'lookup'     , Expr                            , NL
    | 'table'      , DictLiteral                     , NL
    | 'interpolate', IDENT                           , NL   // linear, step, cubic

PoolPropertyInline
  ::= 'max=' , Expr
    | 'regen' , Expr , ('/' , Duration)?
    | 'when' , 'not' , Expr

DerivedFrom
  ::= ResolveRef (',' , ResolveRef)*
```

The four `stat` forms are distinguished by the token after the name:
literal `=` → expression form; bare NL → lookup form; `pool` → pool
sugar; `derived` → derived form. See
[design §5.3](loom-design.md#53-stats--four-types).

### 17.5 Tree node declaration

A `:tree` document body is a sequence of `TreeNodeDecl`s.

```
TreeNodeDecl
  ::= 'node' , IDENT , NL
  , INDENT , TreeNodeProperty+ , DEDENT

TreeNodeProperty
  ::= 'cost'     , DictLiteral                        , NL
    | 'requires' , Expr                               , NL
    | 'effect'   , TreeEffect                         , NL
    | 'rank'     , NUMBER                             , NL

TreeEffect
  ::= 'stat'      , '(' , IDENT , ')' , StatEffectOp , NUMBER
    | 'attribute' , '(' , IDENT , ')' , StatEffectOp , NUMBER
    | 'var'       , '(' , IDENT , ')' , ':=' , Expr
    | 'pool'      , '(' , IDENT , ')' , 'grant'
    | 'ability'   , StaticRef
    | 'luau'      , StaticRef                          // user-registered

StatEffectOp
  ::= '+' | '-'                                       // add/subtract by NUMBER
    | '*' | '/'                                       // multiply/divide by NUMBER
```

A `TreeNodeDecl`'s `requires` may reference other nodes by bare name
(resolved within the same `:tree` document) or `@`-ref nodes in
sibling trees.

### 17.6 Modify action

The runtime stat-adjustment action, usable in any `ActionLine`:

```
ModifyAction
  ::= 'modify' , ModifyTarget , ModifyOp , NUMBER , ModifyClause* , NL

ModifyTarget
  ::= ResolveRef                                      // $player.damage

ModifyOp
  ::= '+' | '-' | '*' | '/'                           // add, subtract, multiply, divide
    | 'add' | 'multiply'                              // explicit form

ModifyClause
  ::= 'for'    , Duration                             // bounded duration
    | 'while'  , Expr                                 // condition-bound
    | 'until'  , Expr                                 // until predicate true
    | 'stack'  , 'unique' | 'add' | 'replace'         // stack policy
```

Modifiers compose; the runtime resolves the effective value on read.
See [design §5.5](loom-design.md#55-modifiers).

---

## 18. Reactivity declarations

The constructs that turn Loom from a static branching tree into a
living system. See [design §9](loom-design.md#9-reactivity--dynamics).

### 18.1 Generator declaration

```
GeneratorDecl
  ::= 'generator' , IDENT , ParamList? , NL
  , INDENT , GeneratorBody , DEDENT

GeneratorBody
  ::= GeneratorItem+

GeneratorItem
  ::= LoopBlock
    | TimeStmt
    | YieldStmt
    | WaitStmt
    | When                                           // event-driven (§7.7)
    | After                                          // condition-gated (§7.7)
    | Otherwise                                      // (§7.7)
    | Match                                          // (§7.7)
    | ActionLine
    | LetBinding
    | Content                                        // dialogue/choice from inside the loop body

LoopBlock
  ::= 'loop' , LoopBound? , NL
  , INDENT , GeneratorItem+ , DEDENT

LoopBound
  ::= NUMBER                                         // loop 5
    | 'forever'                                      // explicit form; absence implies forever

TimeStmt
  ::= 'at'    , TimeSpec                             , NL    // at 6am
    | 'every' , Duration                             , NL    // every 15m
    | 'every' , 'random' , '(' , Duration , ',' , Duration , ')' , NL

WaitStmt
  ::= 'wait'  , Duration                             , NL    // wait 30s
    | 'wait'  , 'until' , Expr                       , NL    // gate on condition

YieldStmt
  ::= 'yield' , YieldBody                            , NL
    | 'yield' , YieldBody , NL , INDENT , Content* , DEDENT

YieldBody
  ::= 'bark' , 'from' , StaticRef                    // pick from a bark set
    | 'with_chance' , '(' , NUMBER , ')'             // probabilistic yield
    | (-NL)*                                         // free content (see body indent form)

TimeSpec
  ::= NUMBER , ('am' | 'pm')                         // 6am, 11pm
    | NUMBER , ':' , NUMBER                          // 14:30 (24-hour)
```

`If` lives inside the existing block grammar as a `Guard` clause on
sections, diverts, and choices (§6.1, §7.3, §7.4) — generator items
gate via `When` / `After` / `Match` rather than a bare `if`, which
keeps the control flow visually obvious at scan time.

Generators live at the top level or inside a `:character` body.
Generators are started by `spawn <name>` (§18.5) or by the runtime at
world boot.

### 18.2 Scene coroutine

```
SceneCoroutineDecl
  ::= 'scene' , IDENT , ParamList? , NL
  , INDENT , SceneState+ , DEDENT

SceneState
  ::= IDENT , NL                                     // state label
  , INDENT , SceneStateItem+ , DEDENT

SceneStateItem
  ::= GeneratorItem                                  // reuse the generator vocabulary
    | ReturnStmt

ReturnStmt
  ::= 'return' , Expr?                               , NL
```

The first `SceneState` is the entry state. Transitions are via
`-> <state_label>` diverts inside the body — they reuse the existing
`Divert` rule (§7.4 / future §10.4).

A scene compiles to a coroutine; one scene runs at a time per scene
invocation. See [design §9.4](loom-design.md#94-scenes).

> **Naming.** Yes, `scene` is also the screenplay slugline (§5.9). The
> two never share syntactic context — sluglines are sub-section
> headers introduced by `##`, scene coroutines are top-level
> declarations introduced by the `scene` keyword. The disambiguation
> falls out of position.

### 18.3 Compose declaration

A named context-free grammar with conditional patterns. The output is
a string suitable for splicing into dialogue.

```
ComposeDecl
  ::= 'compose' , IDENT , NL
  , INDENT , ComposeBody , DEDENT

ComposeBody
  ::= PatternBlock+

PatternBlock
  ::= 'pattern' , ResolveRef? , NL
  , INDENT , PatternArm+ , DEDENT

PatternArm
  ::= ArmDiscriminant , ':' , STRING                 , NL
    | ArmDiscriminant , ':' , STRING , (',' , 'weight' , ':' , NUMBER)? , NL

ArmDiscriminant
  ::= IDENT
    | NUMBER
    | NumRange                                        // e.g. 1..5
    | '_'                                             // wildcard
```

A `compose <name>` is invoked from text via `${compose <name> <args>}`
inline eval; the grammar is the existing `InlineEval` form
(§12 / future §15). See
[design §9.6](loom-design.md#96-procedural-text).

### 18.4 List comprehension

Added to the expression atom grammar (§10):

```
ListComprehension
  ::= '[' , Expr , 'for' , IDENT , 'in' , Expr , ('where' , Expr)? , ']'
```

The comprehension expression is iterated lazily by the resolver until
materialized (by an aggregate call like `count(...)` / `any(...)` / a
field access on the result, or by `let`-binding). See
[design §9.5](loom-design.md#95-list-comprehensions).

Aggregate built-ins added to the resolve registry:

```
Aggregate
  ::= ('any' | 'all' | 'count' | 'min' | 'max' | 'closest' | 'first' | 'last')
    , '(' , Expr , ')'
```

### 18.5 Spawn / cancel / await actions

```
SpawnAction
  ::= 'spawn' , SpawnTarget                          , NL
    | 'let' , IDENT , '=' , 'spawn' , SpawnTarget    , NL    // capture handle

SpawnTarget
  ::= IDENT , '(' , ArgList? , ')'                   // generator or scene call
    | IDENT                                          // shorthand for no-arg

CancelAction
  ::= 'cancel' , CancelTarget                        , NL

CancelTarget
  ::= ResolveRef                                     // $handle
    | StaticRef                                      // @scene_id
    | IDENT                                          // bare generator/scene name

AwaitExpr
  ::= 'await' , Expr                                 // expression form
    | 'run'   , IDENT , '(' , ArgList? , ')'         // await-shorthand: `let r = run foo()`
```

`spawn` returns a *handle* (assignable via `let h = spawn ...`).
`cancel <h>` halts the spawned coroutine and runs no `on_complete` /
`on_fail`. `await` blocks the current generator/scene until the
target's `return` value resolves. See
[design §9.7](loom-design.md#97-spawning-and-joining).

### 18.6 Reactive `let` (clarification)

`let` is already in the expression grammar as `LetBinding`. The
reactive substrate clarifies the runtime contract:

- `let x = $expr` creates a `Memo<T>` keyed on the binding name.
- Re-binding (`let x = ...` twice in the same scope) is a parse
  error (`let-rebind`); use `var x ; ~ x := ...` if you want
  mutability.
- A `let` inside a generator / scene body re-runs the binding's RHS
  whenever any of its `$`-references are mutated.
- A `let` at module scope behaves identically — Loom does not
  distinguish "global reactive" from "local reactive."

The compiler lowers reactive `let` to
`prism-core::reactive::Memo<T>`. See
[design §9.1](loom-design.md#91-reactive-let).

---

## 19. What this grammar deliberately doesn't specify

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

## 20. Diagnostics & lint catalog

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
| `backlink-dead` | warning | `[[name]]` resolves to no codex entry. Never an error. |
| `unknown-cast` | error | SPEAKER appears in dialogue without a `CastDecl` (current or imported). |
| `unknown-cue` | error | `~ cue <name>` or `<cue:name>` references an undeclared cue. |
| `unknown-location` | error | `when participant enters @X` references an undeclared location. |
| `unknown-cohort` | error | `enroll … into <c>` or `:cohort(<c>)` references an undeclared cohort. |
| `choice-in-script` | error | `*` / `+` choice inside a `:script` or `:film` document. |
| `participant-scope-leaked` | warning | `as participant` section diverts to a show-global section without explicit rescoping. |
| `broadcast-empty-scope` | warning | `broadcast :cohort(...) but :all` resolves to no participants at parse time. |
| `improv-without-duration` | info | `(improv …)` with no `.duration` will hold indefinitely until manually advanced. |
| `unknown-slot` | error | Slot name on a character is not in the slot registry. |
| `unknown-hook-pred` | error | `on <pattern>` head not in the hook-pattern registry. |
| `unknown-axis` | error | `$x.<axis>` resolves to no declared axis on the entity's stats sheet. |
| `unknown-pool` | error | `$x.<pool>` resolves to no declared pool. |
| `unknown-stat` | error | `$x.<stat>` resolves to no declared stat. |
| `unknown-attribute` | error | `$x.<attr>` resolves to no declared attribute. |
| `unknown-goal` | error | `$x.pursuing(<g>)` / `$x.goal(<g>)` references an undeclared goal. |
| `unknown-generator` | error | `spawn <g>` / `cancel <g>` references an undeclared generator. |
| `unknown-scene` | error | `spawn <s>` / `run <s>` / `cancel <s>` references an undeclared scene. |
| `unknown-tree-node` | error | `node(<n>)` predicate or `requires` clause references an undeclared tree node. |
| `unknown-compose` | error | `${compose <name> …}` invokes an undeclared compose. |
| `goal-no-priority` | error | `GoalDecl` body has no `priority` knob. |
| `goal-conflicting-knobs` | warning | Both `completes_when` and `on_complete` reference the same expression — likely a copy-paste typo. |
| `disposition-axis-bounds` | error | `init N` falls outside the declared `range`. |
| `disposition-mirror-cycle` | error | Two dispositions `mirror` each other in a cycle. |
| `axis-mode-missing` | error | `AxisDecl` body has no `mode` knob. |
| `axis-curve-required` | error | `mode xp_curve` / `mode use_tracking` decl has no `curve` expression. |
| `pool-max-required` | error | `PoolDecl` body has no `max` knob. |
| `stat-form-ambiguous` | error | A `stat` decl mixes expression and body forms in one declaration. |
| `tree-cycle` | error | Tree node `requires` chain forms a cycle. |
| `tree-rank-overflow` | warning | `rank N` exceeds the declared `max_rank` on the node template. |
| `modify-stack-conflict` | warning | Two `modify` actions on the same target with conflicting `stack` policies. |
| `generator-yield-outside` | error | `yield` appears outside a `GeneratorDecl` body. |
| `wait-outside-coroutine` | error | `wait` / `every` / `at` appears outside a generator or scene. |
| `scene-no-states` | error | `SceneCoroutineDecl` body has no `SceneState`. |
| `scene-unreachable-state` | warning | A `SceneState` is declared but never reached by any `->` divert. |
| `scene-return-outside` | error | `return` appears outside a scene body. |
| `spawn-handle-discarded` | info | `spawn …` result is not bound to a `let` and the target has a non-nil `return` type. |
| `cancel-unknown-handle` | error | `cancel $h` references an unbound resolve. |
| `compose-arm-overlap` | warning | Two `pattern` arms match the same discriminant. |
| `compose-no-default` | info | A `compose` has no `_` arm and a non-exhaustive arm set. |
| `let-rebind` | error | `let x = …` declared twice in the same scope. |
| `knowledge-bad-type` | error | `KnowledgeField` uses a type outside the closed set (`bool` / `int` / `float` / `string` / closed enum / `list<T>` / nilable). |
| `knowledge-arithmetic` | error | Arithmetic op on a knowledge field (only `:=` / `+=` / `-=` on lists are legal). |
| `hook-trigger-unmatched` | warning | `passes` / `drops below` predicate has no upper expression (e.g. `on $x passes` missing the threshold). |
| `list-comprehension-bad-var` | error | List comprehension variable shadows an outer `let` binding. |

These are stable IDs — tooling can suppress them by ID.

---

## 21. Conformance

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
