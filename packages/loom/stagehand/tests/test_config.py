from pathlib import Path

import pytest
import yaml

from stagehand.config import ConfigError, load_config, parse_config
from stagehand.modapi import ModClient
from stagehand.router import _decode_payload

EXAMPLE = Path(__file__).resolve().parents[1] / "show.example.yaml"


def minimal(**overrides):
    doc = {
        "server": {"url": "http://127.0.0.1:7000", "mod_passcode": "x"},
        "mqtt": {"host": "127.0.0.1"},
        "osc": {"targets": {"desktop": {"host": "10.0.0.1", "port": 9000}}},
        "cues": [],
        "sensors": [],
    }
    doc.update(overrides)
    return doc


def test_example_config_loads():
    cfg = load_config(EXAMPLE)
    assert cfg.server.url.startswith("http")
    assert cfg.mqtt is not None
    assert len(cfg.cues) == 4
    assert len(cfg.sensors) == 4
    # one filter per distinct pattern; the two zone rules share one
    assert sorted(cfg.mqtt_filters) == sorted(
        ["sensors/crawlspace/proximity", "vision/+/tamper", "vision/+/zone"]
    )


def test_missing_auth_rejected():
    doc = minimal()
    del doc["server"]["mod_passcode"]
    with pytest.raises(ConfigError, match="mod_token"):
        parse_config(doc)


def test_osc_cue_without_targets_rejected():
    doc = minimal(osc=None, cues=[{"on": {"directive": "cue"}, "osc": {"addr": "/x"}}])
    doc.pop("osc")
    with pytest.raises(ConfigError, match="osc.targets"):
        parse_config(doc)


def test_unknown_osc_to_rejected():
    doc = minimal(cues=[{"on": {"directive": "cue"}, "osc": {"addr": "/x", "to": "mars"}}])
    with pytest.raises(ConfigError, match="mars"):
        parse_config(doc)


def test_mqtt_cue_without_broker_rejected():
    doc = minimal(cues=[{"on": {"directive": "p"}, "mqtt": {"topic": "t"}}])
    doc.pop("mqtt")
    with pytest.raises(ConfigError, match="broker"):
        parse_config(doc)


def test_sensors_without_broker_rejected():
    doc = minimal(sensors=[{"on": {"topic": "a/b"}, "signal": {"name": "s"}}])
    doc.pop("mqtt")
    with pytest.raises(ConfigError, match="broker"):
        parse_config(doc)


def test_bad_yaml_rejected(tmp_path):
    p = tmp_path / "bad.yaml"
    p.write_text("server: [unclosed", encoding="utf-8")
    with pytest.raises(ConfigError, match="bad YAML"):
        load_config(p)


def test_example_yaml_is_valid_yaml():
    yaml.safe_load(EXAMPLE.read_text(encoding="utf-8"))


# -- adjacent pure helpers --------------------------------------------------


def test_mod_client_paths():
    default = ModClient("http://h:7000", event="default")
    assert default.path("/api/mod/signal") == "/api/mod/signal"
    assert default.sse_url == "http://h:7000/events?role=mod&id=stagehand"
    scoped = ModClient("http://h:7000/", event="ev_42")
    assert scoped.path("/api/mod/signal") == "/e/ev_42/api/mod/signal"
    assert scoped.sse_url == "http://h:7000/e/ev_42/events?role=mod&id=stagehand"


def test_decode_payload():
    assert _decode_payload(b'{"near": true}') == {"near": True}
    assert _decode_payload(b"42") == {"value": 42}
    assert _decode_payload(b"on") == {"value": "on"}
    assert _decode_payload(b"true") == {"value": True}
    assert _decode_payload(b"\xff\xfe") == {"value": None}
