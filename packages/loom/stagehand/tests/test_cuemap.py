import json

import pytest

from stagehand.cuemap import (
    CueMapError,
    MqttCue,
    OscCue,
    evaluate_cues,
    parse_cue_rule,
    parse_t_exec,
)


def rule(raw, idx=0):
    return parse_cue_rule(raw, idx)


def directive_event(verb, args):
    return {"type": "directive", "verb": verb, "args": args}


PROJECTOR_RULE = {
    "on": {"directive": "cue"},
    "match": {"target": "projectors"},
    "osc": {"addr": "/cue/scene", "args": ["{scene}"], "t_exec": "+200ms"},
}


def test_directive_to_osc():
    cues = evaluate_cues([rule(PROJECTOR_RULE)], directive_event("cue", "projectors, scene: static_takeover"))
    assert cues == [OscCue(addr="/cue/scene", args=("static_takeover",), t_exec_offset_ms=200, to=None)]


def test_directive_match_filters():
    cues = evaluate_cues([rule(PROJECTOR_RULE)], directive_event("cue", "lights, scene: x"))
    assert cues == []


def test_verb_mismatch():
    assert evaluate_cues([rule(PROJECTOR_RULE)], directive_event("prop", "projectors, scene: x")) == []


def test_non_directive_event_ignored_by_directive_rule():
    assert evaluate_cues([rule(PROJECTOR_RULE)], {"type": "beatEntered", "beat": "x"}) == []


def test_unfilled_placeholder_skips_output():
    cues = evaluate_cues([rule(PROJECTOR_RULE)], directive_event("cue", "projectors"))
    assert cues == []


def test_directive_to_mqtt_payload_from_args():
    r = rule({"on": {"directive": "prop"}, "mqtt": {"topic": "props/{target}/set", "payload_from_args": True, "retain": True}})
    cues = evaluate_cues([r], directive_event("prop", "crawlspace_tv, channel: 13"))
    assert len(cues) == 1
    cue = cues[0]
    assert isinstance(cue, MqttCue)
    assert cue.topic == "props/crawlspace_tv/set"
    assert cue.retain is True
    assert json.loads(cue.payload) == {"channel": 13, "target": "crawlspace_tv"}


def test_payload_from_args_coerces_scalars():
    r = rule({"on": {"directive": "vibe"}, "mqtt": {"topic": "show/vibe", "payload_from_args": True}})
    [cue] = evaluate_cues([r], directive_event("vibe", "crt_glitch, level: 0.7, on: true"))
    assert json.loads(cue.payload) == {"level": 0.7, "on": True, "target": "crt_glitch"}


def test_explicit_payload_dict_templated():
    r = rule({"on": {"directive": "flash"}, "mqtt": {"topic": "props/lights/set", "payload": {"mode": "{target}", "n": 3}}})
    [cue] = evaluate_cues([r], directive_event("flash", "strobe"))
    assert json.loads(cue.payload) == {"mode": "strobe", "n": 3}


def test_event_rule_with_field_match():
    r = rule({"on": {"event": "beatEntered", "beat": "lockdown"}, "osc": {"addr": "/cue/scene", "args": ["lockdown"]}})
    assert evaluate_cues([r], {"type": "beatEntered", "beat": "lockdown", "setting": None}) == [
        OscCue(addr="/cue/scene", args=("lockdown",), t_exec_offset_ms=None, to=None)
    ]
    assert evaluate_cues([r], {"type": "beatEntered", "beat": "other", "setting": None}) == []


def test_event_rule_templates_event_fields():
    r = rule({"on": {"event": "signal"}, "osc": {"addr": "/signal", "args": ["{name}", "{subject}"]}})
    [cue] = evaluate_cues([r], {"type": "signal", "name": "router_unlocked", "subject": "g_1"})
    assert cue.args == ("router_unlocked", "g_1")


def test_lone_placeholder_keeps_native_type():
    r = rule({"on": {"directive": "cue"}, "osc": {"addr": "/x", "args": ["{level}"]}})
    [cue] = evaluate_cues([r], directive_event("cue", "level: 0.7"))
    assert cue.args == (0.7,)


def test_osc_to_subset():
    r = rule({"on": {"directive": "cue"}, "osc": {"addr": "/x", "args": [], "to": "desktop"}})
    [cue] = evaluate_cues([r], directive_event("cue", ""))
    assert cue.to == ("desktop",)


def test_both_outputs_emit():
    r = rule({"on": {"directive": "cue"}, "osc": {"addr": "/x", "args": []}, "mqtt": {"topic": "show/scene", "payload": "x"}})
    cues = evaluate_cues([r], directive_event("cue", ""))
    assert len(cues) == 2


def test_parse_t_exec():
    assert parse_t_exec("+200ms") == 200
    assert parse_t_exec("+1.5s") == 1500
    assert parse_t_exec("2s") == 2000
    with pytest.raises(CueMapError):
        parse_t_exec("later")


def test_rule_validation():
    with pytest.raises(CueMapError):
        parse_cue_rule({"on": {}}, 0)  # neither directive nor event
    with pytest.raises(CueMapError):
        parse_cue_rule({"on": {"directive": "cue", "event": "signal"}}, 0)  # both
    with pytest.raises(CueMapError):
        parse_cue_rule({"on": {"directive": "cue"}}, 0)  # no output
    with pytest.raises(CueMapError):
        parse_cue_rule({"on": {"directive": "cue", "beat": "x"}, "osc": {"addr": "/x"}}, 0)  # stray on-key
    with pytest.raises(CueMapError):
        parse_cue_rule(
            {"on": {"directive": "p"}, "mqtt": {"topic": "t", "payload": {}, "payload_from_args": True}}, 0
        )  # exclusive payloads
