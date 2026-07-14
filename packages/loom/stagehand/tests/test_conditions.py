import pytest

from stagehand.conditions import Condition, ConditionError


def test_equality_bool():
    assert Condition("near == true").evaluate({"near": True})
    assert not Condition("near == true").evaluate({"near": False})


def test_missing_field_is_null():
    assert not Condition("near == true").evaluate({})
    assert Condition("guest == null").evaluate({})
    assert not Condition("guest != null").evaluate({})
    assert Condition("guest != null").evaluate({"guest": "g_1"})


def test_explicit_null_field():
    assert Condition("guest == null").evaluate({"guest": None})


def test_string_compare():
    cond = Condition("event == 'entered'")
    assert cond.evaluate({"event": "entered"})
    assert not cond.evaluate({"event": "left"})


def test_numeric_ordering():
    assert Condition("mm < 500").evaluate({"mm": 410})
    assert not Condition("mm < 500").evaluate({"mm": 900})
    assert Condition("mm >= 410").evaluate({"mm": 410})


def test_ordering_type_mismatch_is_false():
    assert not Condition("mm < 500").evaluate({"mm": "410"})
    assert not Condition("mm < 500").evaluate({})


def test_and_or_not():
    cond = Condition("guest != null && event == 'entered'")
    assert cond.evaluate({"guest": "g", "event": "entered"})
    assert not cond.evaluate({"guest": None, "event": "entered"})
    assert Condition("a == 1 || b == 2").evaluate({"b": 2})
    assert Condition("!(a == 1)").evaluate({"a": 2})


def test_parens_precedence():
    cond = Condition("a == 1 && (b == 2 || c == 3)")
    assert cond.evaluate({"a": 1, "c": 3})
    assert not cond.evaluate({"a": 1, "b": 9, "c": 9})


def test_dotted_lookup():
    assert Condition("sensor.near == true").evaluate({"sensor": {"near": True}})


def test_bare_ident_truthiness():
    assert Condition("near").evaluate({"near": True})
    assert not Condition("near").evaluate({"near": 0})


def test_bad_syntax_raises():
    with pytest.raises(ConditionError):
        Condition("a ==")
    with pytest.raises(ConditionError):
        Condition("a == 1 &&")
    with pytest.raises(ConditionError):
        Condition("(a == 1")
    with pytest.raises(ConditionError):
        Condition("a ~ 1")
