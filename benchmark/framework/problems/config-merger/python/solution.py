from __future__ import annotations


def parse_config(content: str) -> list[tuple[str, str]]:
    """Parse a config string into key-value pairs.

    Raises ValueError with message like "line N: missing '='" on error.
    """
    if not content:
        return []

    pairs: list[tuple[str, str]] = []
    seen_keys: set[str] = set()

    for i, line in enumerate(content.split("\n"), 1):
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        if "=" not in stripped:
            raise ValueError(f"line {i}: missing '='")
        key, value = stripped.split("=", 1)
        if not key:
            raise ValueError(f"line {i}: empty key")
        if key in seen_keys:
            raise ValueError(f"line {i}: duplicate key: {key}")
        seen_keys.add(key)
        pairs.append((key, value))

    return pairs


def merge_configs(
    base: list[tuple[str, str]], overlay: list[tuple[str, str]]
) -> list[tuple[str, str]]:
    """Merge two configs. Overlay overrides base for matching keys."""
    overlay_dict = dict(overlay)
    base_keys = set()
    result = []

    for key, value in base:
        base_keys.add(key)
        if key in overlay_dict:
            result.append((key, overlay_dict[key]))
        else:
            result.append((key, value))

    for key, value in overlay:
        if key not in base_keys:
            result.append((key, value))

    return result


def serialize_config(pairs: list[tuple[str, str]]) -> str:
    """Serialize config pairs to key=value string."""
    return "\n".join(f"{k}={v}" for k, v in pairs)


def lookup(pairs: list[tuple[str, str]], key: str) -> str | None:
    """Look up a key in config pairs."""
    for k, v in pairs:
        if k == key:
            return v
    return None


def filter_by_prefix(
    pairs: list[tuple[str, str]], prefix: str
) -> list[tuple[str, str]]:
    """Filter config pairs by key prefix."""
    return [(k, v) for k, v in pairs if k.startswith(prefix)]
