from __future__ import annotations
# ========== V1 SOLUTION (working code — all tests pass) ==========

from enum import Enum, auto
from dataclasses import dataclass


class Status(Enum):
    Pending = auto()
    Done = auto()
    Cancelled = auto()


@dataclass
class Todo:
    id: int
    title: str
    status: Status


def create(id: int, title: str) -> Todo:
    return Todo(id=id, title=title, status=Status.Pending)


def complete(t: Todo) -> Todo:
    return Todo(id=t.id, title=t.title, status=Status.Done)


def cancel(t: Todo) -> Todo:
    return Todo(id=t.id, title=t.title, status=Status.Cancelled)


def is_done(t: Todo) -> bool:
    return t.status == Status.Done


def pending_count(todos: list[Todo]) -> int:
    return len([t for t in todos if t.status == Status.Pending])


def titles(todos: list[Todo]) -> list[str]:
    return [t.title for t in todos]


def find_by_title(todos: list[Todo], title: str) -> object:
    return next((t for t in todos if t.title == title), None)


def status_label(s: Status) -> str:
    if s == Status.Pending: return "pending"
    elif s == Status.Done: return "done"
    elif s == Status.Cancelled: return "cancelled"


def summary(t: Todo) -> str:
    return f"[{status_label(t.status)}] {t.title}"


# Tests
t = create(1, "Buy milk")
assert t.title == "Buy milk", "create todo title"
assert t.id == 1, "create todo id"

t = complete(create(1, "Buy milk"))
assert is_done(t) == True, "complete todo"

t = cancel(create(1, "Buy milk"))
assert status_label(t.status) == "cancelled", "cancel todo"

todos = [create(1, "A"), complete(create(2, "B")), create(3, "C")]
assert pending_count(todos) == 2, "pending count"

todos = [create(1, "A"), create(2, "B"), create(3, "C")]
assert titles(todos) == ["A", "B", "C"], "titles"

todos = [create(1, "Buy milk"), create(2, "Walk dog")]
t = find_by_title(todos, "Walk dog")
assert t is not None and t.id == 2, "find by title found"

todos = [create(1, "Buy milk")]
assert find_by_title(todos, "nope") is None, "find by title not found"

assert summary(create(1, "Buy milk")) == "[pending] Buy milk", "summary pending"
assert summary(complete(create(1, "Done"))) == "[done] Done", "summary done"

todos = [create(1, "A"), complete(create(2, "B")), cancel(create(3, "C"))]
pending_titles = [t.title for t in todos if t.status == Status.Pending]
assert pending_titles == ["A"], "pipe chain"

todos = [create(1, "A"), create(2, "B"), create(3, "C")]
updated = [complete(t) if t.id == 2 else t for t in todos]
assert pending_count(updated) == 2, "multiple operations pending"
assert is_done(updated[1]), "multiple operations done"

# ========== MODIFICATION INSTRUCTION ==========
# Add a `priority: int` field to the `Todo` dataclass.
# Update `create` to accept a third parameter `priority: int`:
#   `def create(id: int, title: str, priority: int) -> Todo`
# All existing functions (`complete`, `cancel`, `is_done`, etc.) must still work.
# The `summary` function should now include priority: "[pending/p3] Buy milk"
#   (format: f"[{status}/p{priority}] {title}")
# Add a new function:
#   `def high_priority(todos: list[Todo]) -> list[Todo]`
#   Returns todos with priority >= 3, sorted by priority descending.
#
# UPDATE EXISTING TESTS: All `create(id, title)` calls must become
# `create(id, title, 1)` (default priority 1).
# UPDATE `summary` test assertions to include priority in format.

# ========== V2 TESTS (must also pass after modification) ==========

t = create(1, "Buy milk", 3)
assert t.priority == 3, "create with priority"

hp = high_priority([create(1, "A", 5), create(2, "B", 1), create(3, "C", 3)])
assert len(hp) == 2, "high priority count"
assert [t.title for t in hp] == ["A", "C"], "high priority order"

assert high_priority([create(1, "A", 1), create(2, "B", 2)]) == [], "high priority empty"

t = complete(create(1, "A", 5))
assert t.priority == 5, "complete preserves priority"

t = cancel(create(1, "A", 3))
assert t.priority == 3, "cancel preserves priority"

assert summary(create(1, "Buy milk", 3)) == "[pending/p3] Buy milk", "summary with priority"
assert summary(complete(create(1, "Done", 2))) == "[done/p2] Done", "summary done with priority"
