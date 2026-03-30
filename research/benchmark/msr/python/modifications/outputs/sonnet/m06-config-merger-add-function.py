from __future__ import annotations
# ========== V1 SOLUTION (working code — all tests pass) ==========

import os
import tempfile


def parse_config(content: str) -> list[list[str]]:
    """Parse a config string into key-value pairs.
    Skip blank lines and lines starting with #.
    Raises ValueError: "line N: missing '='", "line N: empty key", "line N: duplicate key: KEY"
    """
    if content == "":
        return []
    pairs: list[list[str]] = []
    seen_keys: set[str] = set()
    for i, line in enumerate(content.split("\n"), start=1):
        trimmed = line.strip()
        if trimmed == "" or trimmed.startswith("#"):
            continue
        if "=" not in trimmed:
            raise ValueError(f"line {i}: missing '='")
        key, value = trimmed.split("=", 1)
        if key == "":
            raise ValueError(f"line {i}: empty key")
        if key in seen_keys:
            raise ValueError(f"line {i}: duplicate key: {key}")
        seen_keys.add(key)
        pairs.append([key, value])
    return pairs


def merge_configs(base: list[list[str]], overlay: list[list[str]]) -> list[list[str]]:
    """Merge two parsed configs: entries in overlay override base for matching keys.
    Non-matching keys from both are preserved. Order: base keys first, then new overlay keys.
    """
    overlay_map = {pair[0]: pair[1] for pair in overlay}
    base_keys = set()
    merged = []
    for pair in base:
        key = pair[0]
        base_keys.add(key)
        if key in overlay_map:
            merged.append([key, overlay_map[key]])
        else:
            merged.append(list(pair))
    for pair in overlay:
        if pair[0] not in base_keys:
            merged.append(list(pair))
    return merged


def serialize_config(pairs: list[list[str]]) -> str:
    """Serialize config pairs back to string format (key=value lines)."""
    return "\n".join(f"{pair[0]}={pair[1]}" for pair in pairs)


def load_config(path: str) -> list[list[str]]:
    """Read and parse a config file.
    Raises on file read errors or parse errors (prefixed with filename).
    """
    with open(path) as f:
        content = f.read()
    try:
        return parse_config(content)
    except ValueError as e:
        raise ValueError(f"{path}: {e}") from e


def save_config(path: str, pairs: list[list[str]]) -> None:
    """Write config pairs to a file."""
    with open(path, "w") as f:
        f.write(serialize_config(pairs))


def merge_files(paths: list[str]) -> list[list[str]]:
    """Load multiple config files and merge them in order (left to right).
    First file is base, each subsequent file overlays on top.
    """
    result: list[list[str]] = []
    for path in paths:
        overlay = load_config(path)
        result = merge_configs(result, overlay)
    return result


def merge_and_save(paths: list[str], output: str) -> int:
    """Full pipeline: load configs from paths, merge, save to output path.
    Returns number of keys in final config.
    """
    pairs = merge_files(paths)
    save_config(output, pairs)
    return len(pairs)


def lookup(pairs: list[list[str]], key: str) -> str | None:
    """Lookup a key in config pairs, returns None if not found."""
    for pair in pairs:
        if pair[0] == key:
            return pair[1]
    return None


def filter_by_prefix(pairs: list[list[str]], prefix: str) -> list[list[str]]:
    """Filter config pairs by key prefix."""
    return [pair for pair in pairs if pair[0].startswith(prefix)]


def validate_keys(pairs: list[list[str]], required: list[str]) -> None:
    """Checks that all keys in `required` are present in `pairs`.
    If a key is missing, raise ValueError("missing required key: KEY") for the first missing key.
    If all required keys are present, return None.
    """
    present = {pair[0] for pair in pairs}
    for key in required:
        if key not in present:
            raise ValueError(f"missing required key: {key}")


def load_and_validate(path: str, required: list[str]) -> list[list[str]]:
    """Loads a config file and validates that all required keys are present.
    File read errors and parse errors propagate as usual.
    If validation fails, raise the validation error.
    If everything succeeds, return the parsed pairs.
    """
    pairs = load_config(path)
    validate_keys(pairs, required)
    return pairs


# Tests
assert parse_config("host=localhost\nport=8080") == [["host", "localhost"], ["port", "8080"]], "parse basic config"

assert parse_config("# this is a comment\n\nhost=localhost\n# another comment\nport=8080") == [["host", "localhost"], ["port", "8080"]], "parse with comments and blanks"

assert parse_config("") == [], "parse empty string"

assert parse_config("# comment\n# another") == [], "parse only comments"

try:
    parse_config("host=ok\nbadline")
    assert False, "parse missing equals should raise"
except ValueError as e:
    assert str(e) == "line 2: missing '='", "parse missing equals"

try:
    parse_config("=value")
    assert False, "parse empty key should raise"
except ValueError as e:
    assert str(e) == "line 1: empty key", "parse empty key"

try:
    parse_config("host=a\nhost=b")
    assert False, "parse duplicate key should raise"
except ValueError as e:
    assert str(e) == "line 2: duplicate key: host", "parse duplicate key"

assert parse_config("formula=a=b+c") == [["formula", "a=b+c"]], "parse value with equals"

assert merge_configs([["host", "localhost"]], [["port", "8080"]]) == [["host", "localhost"], ["port", "8080"]], "merge no overlap"

assert merge_configs([["host", "localhost"], ["port", "3000"]], [["port", "8080"]]) == [["host", "localhost"], ["port", "8080"]], "merge with override"

assert merge_configs([], [["port", "8080"]]) == [["port", "8080"]], "merge empty base"

assert merge_configs([["host", "localhost"]], []) == [["host", "localhost"]], "merge empty overlay"

assert serialize_config([["host", "localhost"], ["port", "8080"]]) == "host=localhost\nport=8080", "serialize config"

assert serialize_config([]) == "", "serialize empty"

assert lookup([["host", "localhost"], ["port", "8080"]], "port") == "8080", "lookup found"

assert lookup([["host", "localhost"]], "port") is None, "lookup not found"

assert filter_by_prefix([["db_host", "localhost"], ["db_port", "5432"], ["app_port", "8080"]], "db_") == [["db_host", "localhost"], ["db_port", "5432"]], "filter by prefix"

assert filter_by_prefix([["app_port", "8080"]], "db_") == [], "filter by prefix no match"

# File I/O tests
td = tempfile.gettempdir()

f = os.path.join(td, "almide_test_base.conf")
with open(f, "w") as fh:
    fh.write("host=localhost\nport=3000")
assert load_config(f) == [["host", "localhost"], ["port", "3000"]], "load and parse config file"

try:
    load_config(os.path.join(td, "almide_test_nonexistent_file.conf"))
    assert False, "load nonexistent should raise"
except Exception:
    pass  # expected

f_bad = os.path.join(td, "almide_test_bad.conf")
with open(f_bad, "w") as fh:
    fh.write("good=ok\nbadline")
try:
    load_config(f_bad)
    assert False, "load config with parse error should raise"
except ValueError as e:
    assert str(e) == f_bad + ": line 2: missing '='", "load config with parse error"

f_out = os.path.join(td, "almide_test_output.conf")
save_config(f_out, [["host", "localhost"], ["port", "8080"]])
with open(f_out) as fh:
    assert fh.read() == "host=localhost\nport=8080", "save config file"

f1 = os.path.join(td, "almide_test_m1.conf")
f2 = os.path.join(td, "almide_test_m2.conf")
with open(f1, "w") as fh:
    fh.write("host=localhost\nport=3000")
with open(f2, "w") as fh:
    fh.write("port=8080\ndebug=true")
assert merge_files([f1, f2]) == [["host", "localhost"], ["port", "8080"], ["debug", "true"]], "merge two files"

f1 = os.path.join(td, "almide_test_t1.conf")
f2 = os.path.join(td, "almide_test_t2.conf")
f3 = os.path.join(td, "almide_test_t3.conf")
with open(f1, "w") as fh:
    fh.write("host=a\nport=1")
with open(f2, "w") as fh:
    fh.write("port=2\nmode=dev")
with open(f3, "w") as fh:
    fh.write("mode=prod\nlog=true")
assert merge_files([f1, f2, f3]) == [["host", "a"], ["port", "2"], ["mode", "prod"], ["log", "true"]], "merge three files"

f1 = os.path.join(td, "almide_test_e1.conf")
f2 = os.path.join(td, "almide_test_e2.conf")
with open(f1, "w") as fh:
    fh.write("ok=yes")
with open(f2, "w") as fh:
    fh.write("badline")
try:
    merge_files([f1, f2])
    assert False, "merge files error should propagate"
except ValueError as e:
    assert str(e) == f2 + ": line 1: missing '='", "merge files error propagation"

f1 = os.path.join(td, "almide_test_p1.conf")
f2 = os.path.join(td, "almide_test_p2.conf")
out = os.path.join(td, "almide_test_merged.conf")
with open(f1, "w") as fh:
    fh.write("host=localhost\nport=3000")
with open(f2, "w") as fh:
    fh.write("port=8080\ndebug=true")
assert merge_and_save([f1, f2], out) == 3, "merge and save full pipeline"
with open(out) as fh:
    assert fh.read() == "host=localhost\nport=8080\ndebug=true", "merge and save content"

f1 = os.path.join(td, "almide_test_ep1.conf")
out = os.path.join(td, "almide_test_should_not_exist.conf")
with open(f1, "w") as fh:
    fh.write("ok=yes")
if os.path.exists(out):
    os.remove(out)
try:
    merge_and_save([f1, os.path.join(td, "almide_test_nonexistent_file.conf")], out)
    assert False, "merge and save error should propagate"
except Exception:
    pass
assert not os.path.exists(out), "merge and save error should not create output"

# ========== V2 TESTS (must also pass after modification) ==========

assert validate_keys([["host", "localhost"], ["port", "8080"]], ["host", "port"]) is None, "validate keys all present"

assert validate_keys([["a", "b"]], []) is None, "validate keys empty required"

try:
    validate_keys([["host", "localhost"]], ["host", "port"])
    assert False, "validate keys missing should raise"
except ValueError as e:
    assert str(e) == "missing required key: port", "validate keys missing"

try:
    validate_keys([["host", "localhost"]], ["port", "debug"])
    assert False, "validate keys first missing should raise"
except ValueError as e:
    assert str(e) == "missing required key: port", "validate keys first missing reported"

td = tempfile.gettempdir()

f = os.path.join(td, "almide_test_m06_valid.conf")
with open(f, "w") as fh:
    fh.write("host=localhost\nport=8080")
assert load_and_validate(f, ["host"]) == [["host", "localhost"], ["port", "8080"]], "load and validate success"

f = os.path.join(td, "almide_test_m06_partial.conf")
with open(f, "w") as fh:
    fh.write("host=localhost")
try:
    load_and_validate(f, ["host", "port"])
    assert False, "load and validate missing key should raise"
except ValueError as e:
    assert str(e) == "missing required key: port", "load and validate missing key"
