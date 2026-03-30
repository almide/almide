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
    priority: int


def create(id: int, title: str, priority: int) -> Todo:
    return Todo(id=id, title=title, status=Status.Pending, priority=priority)


def complete(t: Todo) -> Todo:
    return Todo(id=t.id, title=t.title, status=Status.Done, priority=t.priority)


def cancel(t: Todo) -> Todo:
    return Todo(id=t.id, title=t.title, status=Status.Cancelled, priority=t.priority)


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
    label = status_label(t.status)
    return f"[{label}/p{t.priority}] {t.title}"


def high_priority(todos: list[Todo]) -> list[Todo]:
    hp = [t for t in todos if t.priority >= 3]
    return sorted(hp, key=lambda t: t.priority, reverse=True)


# Tests
t = create(1, "Buy milk", 1)
assert t.title == "Buy milk", "create todo title"
assert t.id == 1, "create todo id"

t = complete(create(1, "Buy milk", 1))
assert is_done(t) == True, "complete todo"

t = cancel(create(1, "Buy milk", 1))
assert status_label(t.status) == "cancelled", "cancel todo"

todos = [create(1, "A", 1), complete(create(2, "B", 1)), create(3, "C", 1)]
assert pending_count(todos) == 2, "pending count"

todos = [create(1, "A", 1), create(2, "B", 1), create(3, "C", 1)]
assert titles(todos) == ["A", "B", "C"], "titles"

todos = [create(1, "Buy milk", 1), create(2, "Walk dog", 1)]
t = find_by_title(todos, "Walk dog")
assert t is not None and t.id == 2, "find by title found"

todos = [create(1, "Buy milk", 1)]
assert find_by_title(todos, "nope") is None, "find by title not found"

assert summary(create(1, "Buy milk", 1)) == "[pending/p1] Buy milk", "summary pending"
assert summary(complete(create(1, "Done", 1))) == "[done/p1] Done", "summary done"

todos = [create(1, "A", 1), complete(create(2, "B", 1)), cancel(create(3, "C", 1))]
pending_titles = [t.title for t in todos if t.status == Status.Pending]
assert pending_titles == ["A"], "pipe chain"

todos = [create(1, "A", 1), create(2, "B", 1), create(3, "C", 1)]
updated = [complete(t) if t.id == 2 else t for t in todos]
assert pending_count(updated) == 2, "multiple operations pending"
assert is_done(updated[1]), "multiple operations done"

# ========== V2 TESTS ==========

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
