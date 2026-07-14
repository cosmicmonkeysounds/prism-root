"""A tiny, safe `when:` expression evaluator for sensor rules.

Grammar (no arithmetic, no calls, nothing dynamic — this runs on live
show traffic and must never surprise):

    expr  := or
    or    := and ("||" and)*
    and   := unary ("&&" unary)*
    unary := "!" unary | cmp
    cmp   := primary (("==" | "!=" | ">=" | "<=" | ">" | "<") primary)?
    primary := number | 'string' | "string" | true | false | null
             | ident("." ident)*  | "(" expr ")"

Identifiers resolve against the message context (payload fields +
topic captures); a missing field evaluates to null, so
`guest != null` is the idiom for "field present and non-null".
Ordering comparisons apply to numbers only; a type mismatch is False,
never an error.
"""

from __future__ import annotations

import re
from typing import Any

_TOKEN = re.compile(
    r"""\s*(?:
        (?P<num>-?\d+(?:\.\d+)?)
      | (?P<str>'[^']*'|"[^"]*")
      | (?P<ident>[A-Za-z_][A-Za-z0-9_.]*)
      | (?P<op>&&|\|\||==|!=|>=|<=|>|<|\(|\)|!)
    )""",
    re.VERBOSE,
)

_KEYWORDS = {"true": True, "false": False, "null": None}


class ConditionError(ValueError):
    pass


def _tokenize(src: str) -> list[tuple[str, Any]]:
    tokens: list[tuple[str, Any]] = []
    pos = 0
    while pos < len(src):
        m = _TOKEN.match(src, pos)
        if m is None:
            if src[pos:].strip() == "":
                break
            raise ConditionError(f"bad token at {pos!r} in condition {src!r}")
        pos = m.end()
        if m.lastgroup == "num":
            text = m.group("num")
            tokens.append(("lit", float(text) if "." in text else int(text)))
        elif m.lastgroup == "str":
            tokens.append(("lit", m.group("str")[1:-1]))
        elif m.lastgroup == "ident":
            name = m.group("ident")
            if name in _KEYWORDS:
                tokens.append(("lit", _KEYWORDS[name]))
            else:
                tokens.append(("ident", name))
        else:
            tokens.append(("op", m.group("op")))
    return tokens


# AST nodes: ("lit", v) | ("ident", name) | ("not", node)
#          | ("cmp", op, l, r) | ("and", [nodes]) | ("or", [nodes])


class _Parser:
    def __init__(self, tokens: list[tuple[str, Any]], src: str) -> None:
        self.tokens = tokens
        self.pos = 0
        self.src = src

    def _peek_op(self, *ops: str) -> str | None:
        if self.pos < len(self.tokens):
            kind, value = self.tokens[self.pos]
            if kind == "op" and value in ops:
                return value
        return None

    def parse(self) -> tuple:
        node = self._or()
        if self.pos != len(self.tokens):
            raise ConditionError(f"trailing tokens in condition {self.src!r}")
        return node

    def _or(self) -> tuple:
        parts = [self._and()]
        while self._peek_op("||"):
            self.pos += 1
            parts.append(self._and())
        return parts[0] if len(parts) == 1 else ("or", parts)

    def _and(self) -> tuple:
        parts = [self._unary()]
        while self._peek_op("&&"):
            self.pos += 1
            parts.append(self._unary())
        return parts[0] if len(parts) == 1 else ("and", parts)

    def _unary(self) -> tuple:
        if self._peek_op("!"):
            self.pos += 1
            return ("not", self._unary())
        return self._cmp()

    def _cmp(self) -> tuple:
        left = self._primary()
        op = self._peek_op("==", "!=", ">=", "<=", ">", "<")
        if op is None:
            return left
        self.pos += 1
        return ("cmp", op, left, self._primary())

    def _primary(self) -> tuple:
        if self.pos >= len(self.tokens):
            raise ConditionError(f"unexpected end of condition {self.src!r}")
        kind, value = self.tokens[self.pos]
        if kind in ("lit", "ident"):
            self.pos += 1
            return (kind, value)
        if kind == "op" and value == "(":
            self.pos += 1
            node = self._or()
            if not self._peek_op(")"):
                raise ConditionError(f"unbalanced parens in condition {self.src!r}")
            self.pos += 1
            return node
        raise ConditionError(f"unexpected {value!r} in condition {self.src!r}")


def _lookup(name: str, ctx: dict[str, Any]) -> Any:
    if name in ctx:  # flat key (possibly containing dots) wins
        return ctx[name]
    cur: Any = ctx
    for part in name.split("."):
        if isinstance(cur, dict) and part in cur:
            cur = cur[part]
        else:
            return None
    return cur


def _is_number(v: Any) -> bool:
    return isinstance(v, (int, float)) and not isinstance(v, bool)


def _eval(node: tuple, ctx: dict[str, Any]) -> Any:
    kind = node[0]
    if kind == "lit":
        return node[1]
    if kind == "ident":
        return _lookup(node[1], ctx)
    if kind == "not":
        return not _eval(node[1], ctx)
    if kind == "and":
        return all(_eval(n, ctx) for n in node[1])
    if kind == "or":
        return any(_eval(n, ctx) for n in node[1])
    # cmp
    _, op, ln, rn = node
    left, right = _eval(ln, ctx), _eval(rn, ctx)
    if op == "==":
        return left == right
    if op == "!=":
        return left != right
    if not (_is_number(left) and _is_number(right)):
        return False
    return {
        ">": left > right,
        "<": left < right,
        ">=": left >= right,
        "<=": left <= right,
    }[op]


class Condition:
    """A compiled `when:` expression."""

    def __init__(self, source: str) -> None:
        self.source = source
        self._ast = _Parser(_tokenize(source), source).parse()

    def evaluate(self, ctx: dict[str, Any]) -> bool:
        return bool(_eval(self._ast, ctx))

    def __repr__(self) -> str:  # pragma: no cover
        return f"Condition({self.source!r})"
