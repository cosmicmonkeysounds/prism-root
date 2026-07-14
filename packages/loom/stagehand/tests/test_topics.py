import pytest

from stagehand.topics import TopicError, parse_topic_pattern


def test_literal_match():
    tp = parse_topic_pattern("sensors/crawlspace/proximity")
    assert tp.filter == "sensors/crawlspace/proximity"
    assert tp.match("sensors/crawlspace/proximity") == {}
    assert tp.match("sensors/attic/proximity") is None


def test_named_capture():
    tp = parse_topic_pattern("vision/{cam}/tamper")
    assert tp.filter == "vision/+/tamper"
    assert tp.match("vision/cam_kitchen/tamper") == {"cam": "cam_kitchen"}
    assert tp.match("vision/cam_kitchen/zone") is None


def test_anonymous_plus():
    tp = parse_topic_pattern("vision/+/tamper")
    assert tp.match("vision/x/tamper") == {}


def test_length_mismatch():
    tp = parse_topic_pattern("vision/{cam}/tamper")
    assert tp.match("vision/tamper") is None
    assert tp.match("vision/a/tamper/extra") is None


def test_hash_tail():
    tp = parse_topic_pattern("sensors/#")
    assert tp.filter == "sensors/#"
    assert tp.match("sensors/a") == {}
    assert tp.match("sensors/a/b/c") == {}
    assert tp.match("props/a") is None


def test_multiple_captures():
    tp = parse_topic_pattern("sensors/{node}/{sensor}")
    assert tp.match("sensors/crawlspace/proximity") == {"node": "crawlspace", "sensor": "proximity"}


def test_hash_not_last_rejected():
    with pytest.raises(TopicError):
        parse_topic_pattern("sensors/#/x")


def test_malformed_segment_rejected():
    with pytest.raises(TopicError):
        parse_topic_pattern("vision/{cam/tamper")
    with pytest.raises(TopicError):
        parse_topic_pattern("vision/ca+m/tamper")
