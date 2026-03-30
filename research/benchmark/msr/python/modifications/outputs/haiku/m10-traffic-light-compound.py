from __future__ import annotations
from enum import Enum, auto


class Light(Enum):
    Red = auto()
    Yellow = auto()
    Green = auto()
    Broken = auto()


def next_light(l: Light) -> Light:
    if l == Light.Red: return Light.Green
    elif l == Light.Green: return Light.Yellow
    elif l == Light.Yellow: return Light.Red
    elif l == Light.Broken: return Light.Red


def duration(l: Light) -> int | None:
    if l == Light.Red: return 60
    elif l == Light.Yellow: return 5
    elif l == Light.Green: return 45
    elif l == Light.Broken: return None


def describe(l: Light) -> str:
    if l == Light.Red: return "stop"
    elif l == Light.Yellow: return "caution"
    elif l == Light.Green: return "go"
    elif l == Light.Broken: return "out of order"


# Tests
assert next_light(Light.Red) == Light.Green, "next red"
assert next_light(Light.Green) == Light.Yellow, "next green"
assert next_light(Light.Yellow) == Light.Red, "next yellow"
assert next_light(next_light(next_light(Light.Red))) == Light.Red, "full cycle"
assert duration(Light.Red) == 60, "duration red"
assert duration(Light.Yellow) == 5, "duration yellow"
assert duration(Light.Green) == 45, "duration green"
assert describe(Light.Red) == "stop", "describe"

# V2 Tests
assert next_light(Light.Broken) == Light.Red, "next broken"
assert describe(Light.Broken) == "out of order", "describe broken"
assert duration(Light.Broken) is None, "duration broken"
assert next_light(next_light(Light.Broken)) == Light.Green, "cycle from broken"
assert describe(next_light(Light.Broken)) == "stop", "describe after broken"
