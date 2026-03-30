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
# Change `complete` and `cancel` to raise `ValueError` on invalid state transitions:
#   `complete(t)`:
#   - If t.status is Done, raise `ValueError("already done")`
#   - If t.status is Cancelled, raise `ValueError("cannot complete cancelled")`
#   - If t.status is Pending, return the completed Todo as before
#   `cancel(t)`:
#   - If t.status is Cancelled, raise `ValueError("already cancelled")`
#   - If t.status is Done, raise `ValueError("cannot cancel done")`
#   - If t.status is Pending, return the cancelled Todo as before
#
# UPDATE EXISTING TESTS:
#   - "complete todo" test: now call complete(create(1, "Buy milk")) and check result
#   - "cancel todo" test: similar update
#   - "pipe chain" and "multiple operations": update complete() calls to handle
#     the fact that complete() may raise (use try/except or construct Todo directly)

# ========== V2 TESTS (must also pass after modification) ==========

assert complete(create(1, "A")) == Todo(id=1, title="A", status=Status.Done), "complete pending succeeds"

try:
    complete(Todo(id=1, title="A", status=Status.Done))
    assert False, "complete already done should raise"
except ValueError as e:
    assert str(e) == "already done", "complete already done message"

try:
    complete(Todo(id=1, title="A", status=Status.Cancelled))
    assert False, "complete cancelled should raise"
except ValueError as e:
    assert str(e) == "cannot complete cancelled", "complete cancelled message"

assert cancel(create(1, "A")) == Todo(id=1, title="A", status=Status.Cancelled), "cancel pending succeeds"

try:
    cancel(Todo(id=1, title="A", status=Status.Cancelled))
    assert False, "cancel already cancelled should raise"
except ValueError as e:
    assert str(e) == "already cancelled", "cancel already cancelled message"

try:
    cancel(Todo(id=1, title="A", status=Status.Done))
    assert False, "cancel done should raise"
except ValueError as e:
    assert str(e) == "cannot cancel done", "cancel done message"
