"""Parsing of Loom directive args into a structured form.

The engine emits `{ type: "directive", verb, args }` for every directive
it doesn't handle natively, with `{expr}` interpolation already applied.
`args` follows the directive convention: top-level-comma-separated
segments, where a bare segment is a positional target and `key: value`
is a named argument — e.g. `<cue: projectors, scene: static_takeover>`
arrives as verb="cue", args="projectors, scene: static_takeover".
"""

from __future__ import annotations

from dataclasses import dataclass, field

_BRACKETS = {"(": ")", "[": "]", "{": "}"}
_QUOTES = "'\""


@dataclass(frozen=True)
class ParsedDirective:
    verb: str
    targets: tuple[str, ...] = ()
    kwargs: dict[str, str] = field(default_factory=dict)


def split_top_level(text: str, sep: str = ",") -> list[str]:
    """Split on `sep` occurrences outside quotes and brackets."""
    parts: list[str] = []
    cur: list[str] = []
    depth: list[str] = []
    quote: str | None = None
    for ch in text:
        if quote is not None:
            cur.append(ch)
            if ch == quote:
                quote = None
        elif ch in _QUOTES:
            quote = ch
            cur.append(ch)
        elif ch in _BRACKETS:
            depth.append(_BRACKETS[ch])
            cur.append(ch)
        elif depth and ch == depth[-1]:
            depth.pop()
            cur.append(ch)
        elif ch == sep and not depth:
            parts.append("".join(cur))
            cur = []
        else:
            cur.append(ch)
    parts.append("".join(cur))
    return [p.strip() for p in parts if p.strip()]


def find_top_level(text: str, target: str) -> int:
    """Index of the first `target` char outside quotes and brackets, or -1."""
    depth: list[str] = []
    quote: str | None = None
    for i, ch in enumerate(text):
        if quote is not None:
            if ch == quote:
                quote = None
        elif ch in _QUOTES:
            quote = ch
        elif ch in _BRACKETS:
            depth.append(_BRACKETS[ch])
        elif depth and ch == depth[-1]:
            depth.pop()
        elif ch == target and not depth:
            return i
    return -1


def unquote(text: str) -> str:
    if len(text) >= 2 and text[0] in _QUOTES and text[-1] == text[0]:
        return text[1:-1]
    return text


def parse_directive(verb: str, args: str) -> ParsedDirective:
    targets: list[str] = []
    kwargs: dict[str, str] = {}
    for seg in split_top_level(args):
        colon = find_top_level(seg, ":")
        if colon == -1:
            targets.append(unquote(seg))
            continue
        key = seg[:colon].strip()
        value = unquote(seg[colon + 1 :].strip())
        if key:
            kwargs[key] = value
        else:
            targets.append(value)
    return ParsedDirective(verb=verb, targets=tuple(targets), kwargs=kwargs)


def directive_context(pd: ParsedDirective) -> dict[str, object]:
    """Template/match context: kwargs, plus `verb` and the first target."""
    ctx: dict[str, object] = dict(pd.kwargs)
    ctx["verb"] = pd.verb
    ctx["target"] = pd.targets[0] if pd.targets else ""
    return ctx
