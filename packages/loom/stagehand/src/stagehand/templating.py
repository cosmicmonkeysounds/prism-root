"""`{name}` template rendering against a flat context.

A template whose placeholder names a missing context key renders to
None — the caller skips that output and logs, rather than sending a
half-formed cue during a live show. A template that is *exactly one*
placeholder returns the context value with its native type preserved
(numbers stay numbers), and string values that look numeric are
coerced, so `args: ["{level}"]` sends 0.7 as a float, not "0.7".
"""

from __future__ import annotations

import re
from typing import Any

_PLACEHOLDER = re.compile(r"\{([A-Za-z_][A-Za-z0-9_]*)\}")
_INT = re.compile(r"^-?\d+$")
_FLOAT = re.compile(r"^-?\d+\.\d+$")


def coerce_scalar(value: Any) -> Any:
    """Best-effort native type for a string scalar (int/float/bool/null)."""
    if not isinstance(value, str):
        return value
    lowered = value.lower()
    if lowered == "true":
        return True
    if lowered == "false":
        return False
    if lowered == "null":
        return None
    if _INT.match(value):
        return int(value)
    if _FLOAT.match(value):
        return float(value)
    return value


def render_text(template: str, ctx: dict[str, Any]) -> str | None:
    """Substitute every placeholder; None if any key is missing."""
    missing = False

    def sub(m: re.Match[str]) -> str:
        nonlocal missing
        if m.group(1) not in ctx:
            missing = True
            return ""
        return str(ctx[m.group(1)])

    out = _PLACEHOLDER.sub(sub, template)
    return None if missing else out


def render_value(value: Any, ctx: dict[str, Any]) -> Any:
    """Render one output value: non-strings pass through; a lone
    placeholder keeps/coerces the native type; mixed text renders as str.
    Returns None when a placeholder is unfilled."""
    if not isinstance(value, str):
        return value
    m = re.fullmatch(r"\{([A-Za-z_][A-Za-z0-9_]*)\}", value)
    if m is not None:
        if m.group(1) not in ctx:
            return None
        return coerce_scalar(ctx[m.group(1)])
    return render_text(value, ctx)
