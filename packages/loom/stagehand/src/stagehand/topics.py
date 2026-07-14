"""MQTT topic patterns with named captures.

A sensor rule's topic may name segments it wants to capture:
`vision/{cam}/tamper` subscribes as `vision/+/tamper` and matching
`vision/cam_kitchen/tamper` yields `{"cam": "cam_kitchen"}`. Plain
`+` and a trailing `#` work as in MQTT (anonymous).
"""

from __future__ import annotations

import re
from dataclasses import dataclass

_NAMED = re.compile(r"^\{([A-Za-z_][A-Za-z0-9_]*)\}$")


class TopicError(ValueError):
    pass


@dataclass(frozen=True)
class TopicPattern:
    pattern: str
    # per segment: ("lit", text) | ("+", capture-name-or-None) | ("#", None)
    segments: tuple[tuple[str, str | None], ...]

    @property
    def filter(self) -> str:
        """The MQTT subscription filter this pattern needs."""
        return "/".join("+" if kind == "+" else name if kind == "lit" else "#" for kind, name in self.segments)

    def match(self, topic: str) -> dict[str, str] | None:
        parts = topic.split("/")
        captures: dict[str, str] = {}
        for i, (kind, name) in enumerate(self.segments):
            if kind == "#":
                return captures
            if i >= len(parts):
                return None
            if kind == "lit":
                if parts[i] != name:
                    return None
            elif name is not None:
                captures[name] = parts[i]
        if len(parts) != len(self.segments):
            return None
        return captures


def parse_topic_pattern(pattern: str) -> TopicPattern:
    raw = pattern.split("/")
    segments: list[tuple[str, str | None]] = []
    for i, seg in enumerate(raw):
        if seg == "#":
            if i != len(raw) - 1:
                raise TopicError(f"'#' must be the last segment in {pattern!r}")
            segments.append(("#", None))
        elif seg == "+":
            segments.append(("+", None))
        elif (m := _NAMED.match(seg)) is not None:
            segments.append(("+", m.group(1)))
        elif "+" in seg or "#" in seg or "{" in seg or "}" in seg:
            raise TopicError(f"bad topic segment {seg!r} in {pattern!r}")
        else:
            segments.append(("lit", seg))
    return TopicPattern(pattern=pattern, segments=tuple(segments))
