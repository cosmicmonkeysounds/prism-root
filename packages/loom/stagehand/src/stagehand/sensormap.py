"""The sensor map: MQTT messages → journaled story mutations.

Rules subscribe to a topic pattern (named captures allowed), gate on an
optional `when:` condition over the JSON payload + captures, and emit
mod-API calls: `signal:` (name, subject), `beat:` (name, subject), or
`arrive:` (person, location). Anonymous evidence should be a signal;
identified evidence an arrive — the story decides what either means.

Debounce is per rule + rendered identity, so two different cameras
tripping the same tamper rule each fire, while one flapping sensor
doesn't spam the journal.
"""

from __future__ import annotations

import logging
from dataclasses import dataclass
from typing import Any

from .conditions import Condition
from .templating import render_text
from .topics import TopicPattern, parse_topic_pattern

log = logging.getLogger("stagehand.sensormap")


class SensorMapError(ValueError):
    pass


@dataclass(frozen=True)
class ActionSpec:
    kind: str  # "signal" | "beat" | "arrive"
    fields: dict[str, str]  # templated
    debounce_s: float


@dataclass(frozen=True)
class SensorRule:
    id: int
    topic: TopicPattern
    when: Condition | None
    actions: tuple[ActionSpec, ...]


@dataclass(frozen=True)
class ModCall:
    kind: str  # "signal" | "beat" | "arrive"
    fields: dict[str, str]


_ACTION_FIELDS = {
    "signal": {"required": ("name",), "optional": ("subject",)},
    "beat": {"required": ("name",), "optional": ("subject",)},
    "arrive": {"required": ("person", "location"), "optional": ()},
}


def parse_sensor_rule(raw: Any, idx: int) -> SensorRule:
    where = f"sensors[{idx}]"
    if not isinstance(raw, dict):
        raise SensorMapError(f"{where}: rule must be a mapping")
    # YAML 1.1 parses a bare `on:` key as boolean True — accept both.
    on = raw.get("on", raw.get(True))
    if not isinstance(on, dict) or not isinstance(on.get("topic"), str):
        raise SensorMapError(f"{where}: missing 'on: {{topic: …}}'")
    try:
        topic = parse_topic_pattern(on["topic"])
    except ValueError as err:
        raise SensorMapError(f"{where}: {err}") from err
    when_src = on.get("when")
    if when_src is not None and not isinstance(when_src, str):
        raise SensorMapError(f"{where}: 'when:' must be a string expression")
    try:
        when = Condition(when_src) if when_src else None
    except ValueError as err:
        raise SensorMapError(f"{where}: {err}") from err

    actions: list[ActionSpec] = []
    for kind, shape in _ACTION_FIELDS.items():
        spec = raw.get(kind)
        if spec is None:
            continue
        if not isinstance(spec, dict):
            raise SensorMapError(f"{where}: '{kind}:' must be a mapping")
        fields: dict[str, str] = {}
        for key in shape["required"]:
            if not isinstance(spec.get(key), str) or spec[key] == "":
                raise SensorMapError(f"{where}: '{kind}:' needs a '{key}' string")
            fields[key] = spec[key]
        for key in shape["optional"]:
            if key in spec:
                fields[key] = str(spec[key])
        known = {*shape["required"], *shape["optional"], "debounce_s"}
        for key in spec:
            if key not in known:
                raise SensorMapError(f"{where}: unknown '{kind}.{key}'")
        actions.append(ActionSpec(kind=kind, fields=fields, debounce_s=float(spec.get("debounce_s", 0))))
    if not actions:
        raise SensorMapError(f"{where}: rule has no action (signal/beat/arrive)")
    return SensorRule(id=idx, topic=topic, when=when, actions=tuple(actions))


class Debouncer:
    """Rate-limits (rule, identity) pairs; pass a monotonic clock in."""

    def __init__(self) -> None:
        self._last: dict[tuple[int, int, str], float] = {}

    def allow(self, rule_id: int, action_idx: int, identity: str, window_s: float, now: float) -> bool:
        if window_s <= 0:
            return True
        key = (rule_id, action_idx, identity)
        last = self._last.get(key)
        if last is not None and now - last < window_s:
            return False
        self._last[key] = now
        return True


def evaluate_sensors(
    rules: list[SensorRule],
    topic: str,
    payload: dict[str, Any],
    debouncer: Debouncer,
    now: float,
) -> list[ModCall]:
    """Match one MQTT message against every rule; return mod calls to make."""
    calls: list[ModCall] = []
    for rule in rules:
        captures = rule.topic.match(topic)
        if captures is None:
            continue
        ctx: dict[str, Any] = {**payload, **captures, "topic": topic}
        if rule.when is not None and not rule.when.evaluate(ctx):
            continue
        for action_idx, action in enumerate(rule.actions):
            rendered: dict[str, str] = {}
            unfilled = False
            for key, template in action.fields.items():
                out = render_text(template, ctx)
                if out is None:
                    log.warning("sensors[%d]: unfilled placeholder in %s.%s %r — skipped", rule.id, action.kind, key, template)
                    unfilled = True
                    break
                rendered[key] = out
            if unfilled:
                continue
            identity = "\x1f".join(rendered.get(k, "") for k in sorted(rendered))
            if not debouncer.allow(rule.id, action_idx, identity, action.debounce_s, now):
                continue
            calls.append(ModCall(kind=action.kind, fields=rendered))
    return calls
