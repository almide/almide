# The SAME modification mistake in Python: Triangle class added, area's
# isinstance chain forgets it.
from dataclasses import dataclass

@dataclass
class Circle:   r: int
@dataclass
class Rect:     w: int; h: int
@dataclass
class Triangle: base: int; height: int

def area(s):
    if isinstance(s, Circle): return 3 * s.r * s.r
    if isinstance(s, Rect):   return s.w * s.h
    # Triangle silently falls through -> returns None

print(area(Triangle(4, 6)))
