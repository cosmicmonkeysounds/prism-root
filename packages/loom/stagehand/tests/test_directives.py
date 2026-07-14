from stagehand.directives import directive_context, parse_directive, split_top_level


def test_target_and_kwargs():
    pd = parse_directive("cue", "projectors, scene: static_takeover")
    assert pd.targets == ("projectors",)
    assert pd.kwargs == {"scene": "static_takeover"}


def test_kwargs_only():
    pd = parse_directive("vibe", "name: crt_glitch, level: 0.7")
    assert pd.targets == ()
    assert pd.kwargs == {"name": "crt_glitch", "level": "0.7"}


def test_multiple_targets():
    pd = parse_directive("prop", "tv_a, tv_b, channel: 4")
    assert pd.targets == ("tv_a", "tv_b")
    assert pd.kwargs == {"channel": "4"}


def test_quoted_comma_stays_in_value():
    pd = parse_directive("say", 'text: "one, two", to: Wren')
    assert pd.kwargs == {"text": "one, two", "to": "Wren"}


def test_bracketed_comma_stays_in_value():
    pd = parse_directive("cast", "roles: [a, b], stage: main")
    assert pd.kwargs == {"roles": "[a, b]", "stage": "main"}


def test_colon_inside_value_not_split_again():
    pd = parse_directive("cue", "clock: 6:30am")
    assert pd.kwargs == {"clock": "6:30am"}


def test_empty_args():
    pd = parse_directive("pause", "")
    assert pd.targets == ()
    assert pd.kwargs == {}


def test_split_top_level_ignores_nested():
    assert split_top_level("a, (b, c), d") == ["a", "(b, c)", "d"]


def test_context_has_verb_and_first_target():
    ctx = directive_context(parse_directive("cue", "projectors, scene: x"))
    assert ctx["verb"] == "cue"
    assert ctx["target"] == "projectors"
    assert ctx["scene"] == "x"


def test_context_empty_target():
    ctx = directive_context(parse_directive("vibe", "level: 1"))
    assert ctx["target"] == ""
