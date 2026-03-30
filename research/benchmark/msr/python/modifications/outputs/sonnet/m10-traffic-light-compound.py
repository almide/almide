from __future__ import annotations
# ========== V1 SOLUTION (working code — all tests pass) ==========

from enum import Enum, auto


class Light(Enum):
    Red = auto()
    Yellow = auto()
    Green = auto()
    Broken = auto()


def next_light(l: Light) -> Light:
    match l:
        case Light.Red: return Light.Green
        case Light.Green: return Light.Yellow
        case Light.Yellow: return Light.Red
        case Light.Broken: return Light.Red


def duration(l: Light) -> int | None:
    match l:
        case Light.Red: return 60
        case Light.Yellow: return 5
        case Light.Green: return 45
        case Light.Broken: return None


def describe(l: Light) -> str:
    match l:
        case Light.Red: return "stop"
        case Light.Yellow: return "caution"
        case Light.Green: return "go"
        case Light.Broken: return "out of order"


# Tests
assert next_light(Light.Red) == Light.Green, "next red"
assert next_light(Light.Green) == Light.Yellow, "next green"
assert next_light(Light.Yellow) == Light.Red, "next yellow"
assert next_light(next_light(next_light(Light.Red))) == Light.Red, "full cycle"
assert duration(Light.Red) == 60, "duration red"
assert duration(Light.Yellow) == 5, "duration yellow"
assert duration(Light.Green) == 45, "duration green"
assert describe(Light.Red) == "stop", "describe"

# ========== MODIFICATION INSTRUCTION ==========
# Make TWO changes simultaneously:
#
# 1. Add a `Broken` member to the `Light` enum.
#    - `next_light(Light.Broken)` returns `Light.Red` (reset when repaired)
#    - `describe(Light.Broken)` returns `"out of order"`
#
# 2. Change `duration` to return `int | None` instead of `int`.
#    - `duration(Light.Red)` returns `60`
#    - `duration(Light.Yellow)` returns `5`
#    - `duration(Light.Green)` returns `45`
#    - `duration(Light.Broken)` returns `None`
#
# UPDATE EXISTING TESTS: duration tests remain the same (60, 5, 45 are still valid
# since int is a valid `int | None`). No changes needed to existing duration tests.

# ========== V2 TESTS (must also pass after modification) ==========

assert next_light(Light.Broken) == Light.Red, "next broken"
assert describe(Light.Broken) == "out of order", "describe broken"
assert duration(Light.Broken) is None, "duration broken"
assert next_light(next_light(Light.Broken)) == Light.Green, "cycle from broken"
assert describe(next_light(Light.Broken)) == "stop", "describe after broken"
