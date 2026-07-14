"""The cue map: story events → OSC/MQTT outputs.

Rules match either a directive verb (`on: {directive: cue}`, with an
optional `match:` over the parsed target/kwargs) or a raw sim event
(`on: {event: beatEntered, beat: lockdown}` — extra keys are field
matchers). Outputs are `osc:` (address + templated args, optional
`t_exec` offset and `to:` target subset) and/or `mqtt:` (templated
topic, payload from an explicit dict/string or from the directive's
kwargs, optional retain/qos).

Everything here is pure: `evaluate_cues` returns dispatchable cue
values; the router owns sockets and clocks.
"""

from __future__ import annotations

import json
import logging
import re
from dataclasses import dataclass, field
from typing import Any

from .directives import directive_context, parse_directive
from .templating import coerce_scalar, render_text, render_value

log = logging.getLogger("stagehand.cuemap")

_T_EXEC = re.compile(r"^\+?(\d+(?:\.\d+)?)(ms|s)$")


class CueMapError(ValueError):
    pass


def parse_t_exec(text: str) -> int:
    """`"+200ms"` / `"+1.5s"` → offset in milliseconds."""
    m = _T_EXEC.match(text.strip())
    if m is None:
        raise CueMapError(f"bad t_exec {text!r} (expected e.g. '+200ms' or '+1.5s')")
    value = float(m.group(1))
    return int(value if m.group(2) == "ms" else value * 1000)


@dataclass(frozen=True)
class OscSpec:
    addr: str
    args: tuple[Any, ...] = ()
    t_exec_ms: int | None = None
    to: tuple[str, ...] | None = None  # None = every OSC target


@dataclass(frozen=True)
class MqttSpec:
    topic: str
    payload: dict[str, Any] | str | None = None
    payload_from_args: bool = False
    retain: bool = False
    qos: int = 0


@dataclass(frozen=True)
class CueRule:
    id: int
    on_directive: str | None
    on_event: str | None
    event_fields: dict[str, Any] = field(default_factory=dict)
    match: dict[str, Any] = field(default_factory=dict)
    osc: OscSpec | None = None
    mqtt: MqttSpec | None = None


@dataclass(frozen=True)
class OscCue:
    addr: str
    args: tuple[Any, ...]
    t_exec_offset_ms: int | None
    to: tuple[str, ...] | None


@dataclass(frozen=True)
class MqttCue:
    topic: str
    payload: str
    retain: bool
    qos: int


Cue = OscCue | MqttCue


def parse_cue_rule(raw: Any, idx: int) -> CueRule:
    where = f"cues[{idx}]"
    if not isinstance(raw, dict):
        raise CueMapError(f"{where}: rule must be a mapping")
    # YAML 1.1 parses a bare `on:` key as boolean True — accept both.
    on = raw.get("on", raw.get(True))
    if not isinstance(on, dict):
        raise CueMapError(f"{where}: missing 'on:' mapping")
    directive = on.get("directive")
    event = on.get("event")
    if (directive is None) == (event is None):
        raise CueMapError(f"{where}: 'on:' needs exactly one of 'directive' or 'event'")
    event_fields = {k: v for k, v in on.items() if k not in ("directive", "event")}
    if directive is not None and event_fields:
        raise CueMapError(f"{where}: use 'match:' (not extra 'on:' keys) with a directive rule")
    match = raw.get("match", {})
    if not isinstance(match, dict):
        raise CueMapError(f"{where}: 'match:' must be a mapping")

    osc_spec: OscSpec | None = None
    if (osc := raw.get("osc")) is not None:
        if not isinstance(osc, dict) or not isinstance(osc.get("addr"), str):
            raise CueMapError(f"{where}: 'osc:' needs an 'addr' string")
        args = osc.get("args", [])
        if not isinstance(args, list):
            raise CueMapError(f"{where}: 'osc.args' must be a list")
        to = osc.get("to")
        if isinstance(to, str):
            to = [to]
        if to is not None and not (isinstance(to, list) and all(isinstance(t, str) for t in to)):
            raise CueMapError(f"{where}: 'osc.to' must be a target name or list of names")
        t_exec = osc.get("t_exec")
        osc_spec = OscSpec(
            addr=osc["addr"],
            args=tuple(args),
            t_exec_ms=parse_t_exec(t_exec) if t_exec is not None else None,
            to=tuple(to) if to is not None else None,
        )

    mqtt_spec: MqttSpec | None = None
    if (mqtt := raw.get("mqtt")) is not None:
        if not isinstance(mqtt, dict) or not isinstance(mqtt.get("topic"), str):
            raise CueMapError(f"{where}: 'mqtt:' needs a 'topic' string")
        payload = mqtt.get("payload")
        from_args = bool(mqtt.get("payload_from_args", False))
        if payload is not None and from_args:
            raise CueMapError(f"{where}: 'payload' and 'payload_from_args' are mutually exclusive")
        if payload is not None and not isinstance(payload, (dict, str)):
            raise CueMapError(f"{where}: 'mqtt.payload' must be a mapping or string")
        mqtt_spec = MqttSpec(
            topic=mqtt["topic"],
            payload=payload,
            payload_from_args=from_args,
            retain=bool(mqtt.get("retain", False)),
            qos=int(mqtt.get("qos", 0)),
        )

    if osc_spec is None and mqtt_spec is None:
        raise CueMapError(f"{where}: rule has no 'osc:' or 'mqtt:' output")
    return CueRule(
        id=idx,
        on_directive=directive,
        on_event=event,
        event_fields=event_fields,
        match=match,
        osc=osc_spec,
        mqtt=mqtt_spec,
    )


def _matches(rule: CueRule, event: dict[str, Any], ctx: dict[str, Any]) -> bool:
    for key, want in rule.event_fields.items():
        if str(event.get(key)) != str(want):
            return False
    for key, want in rule.match.items():
        if key not in ctx or str(ctx[key]) != str(want):
            return False
    return True


def _render_osc(spec: OscSpec, ctx: dict[str, Any], rule_id: int) -> OscCue | None:
    args: list[Any] = []
    for arg in spec.args:
        rendered = render_value(arg, ctx)
        if rendered is None and isinstance(arg, str):
            log.warning("cues[%d]: unfilled placeholder in osc arg %r — skipped", rule_id, arg)
            return None
        args.append(rendered)
    return OscCue(addr=spec.addr, args=tuple(args), t_exec_offset_ms=spec.t_exec_ms, to=spec.to)


def _render_mqtt(spec: MqttSpec, ctx: dict[str, Any], args_payload: dict[str, Any], rule_id: int) -> MqttCue | None:
    topic = render_text(spec.topic, ctx)
    if topic is None:
        log.warning("cues[%d]: unfilled placeholder in mqtt topic %r — skipped", rule_id, spec.topic)
        return None
    payload: str
    if spec.payload_from_args:
        payload = json.dumps(args_payload)
    elif isinstance(spec.payload, dict):
        rendered: dict[str, Any] = {}
        for key, value in spec.payload.items():
            out = render_value(value, ctx)
            if out is None and isinstance(value, str):
                log.warning("cues[%d]: unfilled placeholder in mqtt payload %r — skipped", rule_id, value)
                return None
            rendered[key] = out
        payload = json.dumps(rendered)
    elif isinstance(spec.payload, str):
        out = render_text(spec.payload, ctx)
        if out is None:
            log.warning("cues[%d]: unfilled placeholder in mqtt payload %r — skipped", rule_id, spec.payload)
            return None
        payload = out
    else:
        payload = ""
    return MqttCue(topic=topic, payload=payload, retain=spec.retain, qos=spec.qos)


def evaluate_cues(rules: list[CueRule], event: dict[str, Any]) -> list[Cue]:
    """Match one sim event against every rule; return the cues to dispatch."""
    etype = event.get("type")
    pd = None
    if etype == "directive":
        pd = parse_directive(str(event.get("verb", "")), str(event.get("args", "")))

    cues: list[Cue] = []
    for rule in rules:
        if rule.on_directive is not None:
            if pd is None or pd.verb != rule.on_directive:
                continue
            ctx = directive_context(pd)
            args_payload: dict[str, Any] = {k: coerce_scalar(v) for k, v in pd.kwargs.items()}
            if pd.targets:
                args_payload.setdefault("target", pd.targets[0])
        else:
            if etype != rule.on_event:
                continue
            ctx = dict(event)
            args_payload = {k: v for k, v in event.items() if k != "type"}
        if not _matches(rule, event, ctx):
            continue
        if rule.osc is not None and (osc := _render_osc(rule.osc, ctx, rule.id)) is not None:
            cues.append(osc)
        if rule.mqtt is not None and (mqtt := _render_mqtt(rule.mqtt, ctx, args_payload, rule.id)) is not None:
            cues.append(mqtt)
    return cues
