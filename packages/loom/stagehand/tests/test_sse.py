from stagehand.sse import SSEParser


def test_single_frame():
    p = SSEParser()
    frames = p.feed('event: sim\ndata: {"type":"signal"}\n\n')
    assert frames == [("sim", '{"type":"signal"}')]


def test_frame_split_across_chunks():
    p = SSEParser()
    assert p.feed("event: sim\nda") == []
    assert p.feed('ta: {"a":1}\n\nevent: snapshot\n') == [("sim", '{"a":1}')]
    assert p.feed("data: {}\n\n") == [("snapshot", "{}")]


def test_comments_and_pings_ignored():
    p = SSEParser()
    assert p.feed(":ok\n\n") == []
    assert p.feed(':ping\n\nevent: sim\ndata: {"x":1}\n\n') == [("sim", '{"x":1}')]


def test_crlf_normalized():
    p = SSEParser()
    frames = p.feed("event: sim\r\ndata: {}\r\n\r\n")
    assert frames == [("sim", "{}")]


def test_crlf_torn_across_chunks():
    p = SSEParser()
    assert p.feed("event: sim\r\ndata: {}\r") == []
    assert p.feed("\n\r\n") == [("sim", "{}")]


def test_default_event_name():
    p = SSEParser()
    assert p.feed("data: hello\n\n") == [("message", "hello")]


def test_multi_line_data_joined():
    p = SSEParser()
    assert p.feed("event: e\ndata: a\ndata: b\n\n") == [("e", "a\nb")]


def test_multiple_frames_one_chunk():
    p = SSEParser()
    frames = p.feed("event: a\ndata: 1\n\nevent: b\ndata: 2\n\n")
    assert frames == [("a", "1"), ("b", "2")]
