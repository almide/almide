from __future__ import annotations
# ========== V1 SOLUTION (working code — all tests pass) ==========

from enum import Enum, auto


class Light(Enum):
    Red = auto()
    Yellow = auto()
    Green = auto()
    FlashingRed = auto()


def next_light(l: Light) -> Light:
    if l == Light.Red: return Light.Green
    elif l == Light.Green: return Light.Yellow
    elif l == Light.Yellow: return Light.Red
    elif l == Light.FlashingRed: return Light.Red


def duration(l: Light) -> int:
    if l == Light.Red: return 60
    elif l == Light.Yellow: return 5
    elif l == Light.Green: return 45
    elif l == Light.FlashingRed: return 2


def describe(l: Light) -> str:
    if l == Light.Red: return "stop"
    elif l == Light.Yellow: return "caution"
    elif l == Light.Green: return "go"
    elif l == Light.FlashingRed: return "caution"


# Tests
assert next_light(Light.Red) == Light.Green, "next red"
assert next_light(Light.Green) == Light.Yellow, "next green"
assert next_light(Light.Yellow) == Light.Red, "next yellow"
assert next_light(next_light(next_light(Light.Red))) == Light.Red, "full cycle"
assert duration(Light.Red) == 60, "duration red"
assert duration(Light.Yellow) == 5, "duration yellow"
assert duration(Light.Green) == 45, "duration green"
assert describe(Light.Red) == "stop", "describe"

# ========== V2 TESTS (must also pass after modification) ==========

assert next_light(Light.FlashingRed) == Light.Red, "next flashing red"
assert duration(Light.FlashingRed) == 2, "duration flashing red"
assert describe(Light.FlashingRed) == "caution", "describe flashing red"
assert next_light(next_light(Light.FlashingRed)) == Light.Green, "cycle from flashing red"
