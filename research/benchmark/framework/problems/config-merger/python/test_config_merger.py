import pytest
from solution import parse_config, merge_configs, serialize_config, lookup, filter_by_prefix


def test_parse_basic_config():
    assert parse_config("host=localhost\nport=8080") == [
        ("host", "localhost"),
        ("port", "8080"),
    ]


def test_parse_with_comments_and_blanks():
    content = "# this is a comment\n\nhost=localhost\n# another comment\nport=8080"
    assert parse_config(content) == [("host", "localhost"), ("port", "8080")]


def test_parse_empty_string():
    assert parse_config("") == []


def test_parse_only_comments():
    assert parse_config("# comment\n# another") == []


def test_parse_missing_equals():
    with pytest.raises(ValueError, match="line 2: missing '='"):
        parse_config("host=ok\nbadline")


def test_parse_empty_key():
    with pytest.raises(ValueError, match="line 1: empty key"):
        parse_config("=value")


def test_parse_duplicate_key():
    with pytest.raises(ValueError, match="line 2: duplicate key: host"):
        parse_config("host=a\nhost=b")


def test_parse_value_with_equals():
    assert parse_config("formula=a=b+c") == [("formula", "a=b+c")]


def test_merge_no_overlap():
    base = [("host", "localhost")]
    overlay = [("port", "8080")]
    assert merge_configs(base, overlay) == [
        ("host", "localhost"),
        ("port", "8080"),
    ]


def test_merge_with_override():
    base = [("host", "localhost"), ("port", "3000")]
    overlay = [("port", "8080")]
    assert merge_configs(base, overlay) == [
        ("host", "localhost"),
        ("port", "8080"),
    ]


def test_merge_empty_base():
    assert merge_configs([], [("port", "8080")]) == [("port", "8080")]


def test_merge_empty_overlay():
    assert merge_configs([("host", "localhost")], []) == [("host", "localhost")]


def test_serialize_config():
    pairs = [("host", "localhost"), ("port", "8080")]
    assert serialize_config(pairs) == "host=localhost\nport=8080"


def test_serialize_empty():
    assert serialize_config([]) == ""


def test_lookup_found():
    pairs = [("host", "localhost"), ("port", "8080")]
    assert lookup(pairs, "port") == "8080"


def test_lookup_not_found():
    pairs = [("host", "localhost")]
    assert lookup(pairs, "port") is None


def test_filter_by_prefix():
    pairs = [("db_host", "localhost"), ("db_port", "5432"), ("app_port", "8080")]
    assert filter_by_prefix(pairs, "db_") == [
        ("db_host", "localhost"),
        ("db_port", "5432"),
    ]


def test_filter_by_prefix_no_match():
    pairs = [("app_port", "8080")]
    assert filter_by_prefix(pairs, "db_") == []
