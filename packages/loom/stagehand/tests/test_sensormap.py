import pytest

from stagehand.sensormap import Debouncer, ModCall, SensorMapError, evaluate_sensors, parse_sensor_rule


def rules(*raws):
    return [parse_sensor_rule(raw, i) for i, raw in enumerate(raws)]


PROXIMITY = {"on": {"topic": "sensors/crawlspace/proximity", "when": "near == true"}, "signal": {"name": "tv_approached", "debounce_s": 10}}
TAMPER = {"on": {"topic": "vision/{cam}/tamper"}, "signal": {"name": "camera_tampered", "subject": "{cam}"}}
ZONE_ID = {"on": {"topic": "vision/{cam}/zone", "when": "guest != null && event == 'entered'"}, "arrive": {"person": "{guest}", "location": "{zone}"}}
ZONE_ANON = {"on": {"topic": "vision/{cam}/zone", "when": "guest == null && event == 'entered'"}, "signal": {"name": "movement", "subject": "{zone}"}}


def test_when_gates():
    rs = rules(PROXIMITY)
    d = Debouncer()
    assert evaluate_sensors(rs, "sensors/crawlspace/proximity", {"near": False}, d, 0.0) == []
    calls = evaluate_sensors(rs, "sensors/crawlspace/proximity", {"near": True}, d, 0.0)
    assert calls == [ModCall(kind="signal", fields={"name": "tv_approached"})]


def test_topic_capture_into_subject():
    calls = evaluate_sensors(rules(TAMPER), "vision/cam_kitchen/tamper", {"kind": "covered"}, Debouncer(), 0.0)
    assert calls == [ModCall(kind="signal", fields={"name": "camera_tampered", "subject": "cam_kitchen"})]


def test_identified_vs_anonymous_zone():
    rs = rules(ZONE_ID, ZONE_ANON)
    d = Debouncer()
    identified = evaluate_sensors(rs, "vision/cam_hall/zone", {"zone": "Hallway", "event": "entered", "guest": "g_1"}, d, 0.0)
    assert identified == [ModCall(kind="arrive", fields={"person": "g_1", "location": "Hallway"})]
    anonymous = evaluate_sensors(rs, "vision/cam_hall/zone", {"zone": "Hallway", "event": "entered", "guest": None}, d, 0.0)
    assert anonymous == [ModCall(kind="signal", fields={"name": "movement", "subject": "Hallway"})]
    left = evaluate_sensors(rs, "vision/cam_hall/zone", {"zone": "Hallway", "event": "left", "guest": "g_1"}, d, 0.0)
    assert left == []


def test_debounce_window():
    rs = rules(PROXIMITY)
    d = Debouncer()
    assert len(evaluate_sensors(rs, "sensors/crawlspace/proximity", {"near": True}, d, 0.0)) == 1
    assert len(evaluate_sensors(rs, "sensors/crawlspace/proximity", {"near": True}, d, 5.0)) == 0
    assert len(evaluate_sensors(rs, "sensors/crawlspace/proximity", {"near": True}, d, 10.5)) == 1


def test_debounce_is_per_identity():
    tamper = {"on": {"topic": "vision/{cam}/tamper"}, "signal": {"name": "camera_tampered", "subject": "{cam}", "debounce_s": 30}}
    rs = rules(tamper)
    d = Debouncer()
    assert len(evaluate_sensors(rs, "vision/cam_a/tamper", {}, d, 0.0)) == 1
    assert len(evaluate_sensors(rs, "vision/cam_b/tamper", {}, d, 1.0)) == 1  # different camera fires
    assert len(evaluate_sensors(rs, "vision/cam_a/tamper", {}, d, 2.0)) == 0  # same camera debounced


def test_unfilled_template_skips_action():
    rs = rules({"on": {"topic": "sensors/x/y"}, "signal": {"name": "s", "subject": "{nope}"}})
    assert evaluate_sensors(rs, "sensors/x/y", {}, Debouncer(), 0.0) == []


def test_beat_action():
    rs = rules({"on": {"topic": "sensors/door/open"}, "beat": {"name": "doors_open"}})
    calls = evaluate_sensors(rs, "sensors/door/open", {"value": 1}, Debouncer(), 0.0)
    assert calls == [ModCall(kind="beat", fields={"name": "doors_open"})]


def test_validation():
    with pytest.raises(SensorMapError):
        parse_sensor_rule({"on": {"topic": "a/b"}}, 0)  # no action
    with pytest.raises(SensorMapError):
        parse_sensor_rule({"on": {}}, 0)  # no topic
    with pytest.raises(SensorMapError):
        parse_sensor_rule({"on": {"topic": "a/b", "when": "a =="}, "signal": {"name": "x"}}, 0)  # bad condition
    with pytest.raises(SensorMapError):
        parse_sensor_rule({"on": {"topic": "a/b"}, "arrive": {"person": "p"}}, 0)  # missing location
    with pytest.raises(SensorMapError):
        parse_sensor_rule({"on": {"topic": "a/b"}, "signal": {"name": "x", "bogus": 1}}, 0)  # unknown key
