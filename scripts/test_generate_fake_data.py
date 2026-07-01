import importlib.util
import os
import pytest

_spec = importlib.util.spec_from_file_location(
    "genfake",
    os.path.join(os.path.dirname(__file__), "generate-fake-data.py"),
)
genfake = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(genfake)
parse_dsn = genfake.parse_dsn


def test_parse_dsn_with_port():
    got = parse_dsn("https://abc123@127.0.0.1:3334/7")
    assert got == {"id": 7, "key": "abc123", "base": "https://127.0.0.1:3334"}


def test_parse_dsn_without_port():
    got = parse_dsn("http://deadbeef@example.com/42")
    assert got == {"id": 42, "key": "deadbeef", "base": "http://example.com"}


def test_parse_dsn_missing_key_raises():
    with pytest.raises(ValueError):
        parse_dsn("https://127.0.0.1:3334/7")


def test_parse_dsn_missing_pid_raises():
    with pytest.raises(ValueError):
        parse_dsn("https://abc123@127.0.0.1:3334/")


def test_parse_dsn_non_integer_pid_raises():
    with pytest.raises(ValueError):
        parse_dsn("https://abc123@127.0.0.1:3334/notanumber")
