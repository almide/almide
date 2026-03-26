from __future__ import annotations


def parse_config(content: str) -> list[tuple[str, str]]:
    """Parse a config string into key-value pairs.

    Raises ValueError with message like "line N: missing '='" on error.
    """
    # TODO: implement
    raise NotImplementedError


def merge_configs(
    base: list[tuple[str, str]], overlay: list[tuple[str, str]]
) -> list[tuple[str, str]]:
    """Merge two configs. Overlay overrides base for matching keys."""
    # TODO: implement
    return []


def serialize_config(pairs: list[tuple[str, str]]) -> str:
    """Serialize config pairs to key=value string."""
    # TODO: implement
    return ""


def lookup(pairs: list[tuple[str, str]], key: str) -> str | None:
    """Look up a key in config pairs."""
    # TODO: implement
    return None


def filter_by_prefix(
    pairs: list[tuple[str, str]], prefix: str
) -> list[tuple[str, str]]:
    """Filter config pairs by key prefix."""
    # TODO: implement
    return []
