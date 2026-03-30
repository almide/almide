from __future__ import annotations
# ========== V1 SOLUTION (working code — all tests pass) ==========

# Student grade report processor
# Input format: "name:score1,score2,score3" (one student per line)
# Scores are integers 0-100


def parse_student(line: str) -> list[str]:
    """Parse a single student line like "Alice:85,92,78".
    Returns [name, score1, score2, ...] as strings.
    Raises ValueError: "empty name", "invalid score: X", "score out of range: X",
                       "no scores", "invalid format"
    """
    if ":" not in line:
        raise ValueError("invalid format")
    parts = line.split(":", 1)
    name = parts[0]
    if name == "":
        raise ValueError("empty name")
    scores_str = parts[1]
    if scores_str == "":
        raise ValueError("no scores")
    score_strs = scores_str.split(",")
    for s in score_strs:
        try:
            n = int(s)
        except ValueError:
            raise ValueError(f"invalid score: {s}")
        if n < 0 or n > 100:
            raise ValueError(f"score out of range: {s}")
    return [name] + score_strs


def parse_all(input: str) -> list[list[str]]:
    """Parse multiple students from newline-separated input.
    Raises ValueError: "empty input", line errors prefixed with "line N: ",
                       "duplicate name: NAME"
    """
    if input == "":
        raise ValueError("empty input")
    lines = input.split("\n")
    students: list[list[str]] = []
    seen_names: set[str] = set()
    for i, line in enumerate(lines, 1):
        try:
            student = parse_student(line)
        except ValueError as e:
            raise ValueError(f"line {i}: {e}")
        name = student[0]
        if name in seen_names:
            raise ValueError(f"duplicate name: {name}")
        seen_names.add(name)
        students.append(student)
    return students


def average(student: list[str]) -> int:
    """Calculate average score for a student record [name, score1, score2, ...].
    Returns integer (floor division). Raises ValueError if no scores.
    """
    scores = student[1:]
    if len(scores) == 0:
        raise ValueError("no scores")
    total = sum(int(s) for s in scores)
    return total // len(scores)


def letter_grade(avg: int) -> str:
    """Convert numeric average to letter grade.
    90-100: "A", 80-89: "B", 70-79: "C", 60-69: "D", below 60: "F"
    """
    if avg >= 90:
        return "A"
    elif avg >= 80:
        return "B"
    elif avg >= 70:
        return "C"
    elif avg >= 60:
        return "D"
    else:
        return "F"


def honor_roll(students: list[list[str]]) -> list[str]:
    """Find students on honor roll (average >= 85).
    Returns list of names, sorted alphabetically.
    """
    names = [s[0] for s in students if average(s) >= 85]
    return sorted(names)


def class_average(students: list[list[str]]) -> int:
    """Calculate class average across all students."""
    total = sum(average(s) for s in students)
    return total // len(students)


def format_student(student: list[str]) -> str:
    """Format a single student summary: "name: avg (grade)"."""
    avg = average(student)
    grade = letter_grade(avg)
    return f"{student[0]}: {avg} ({grade})"


def generate_report(input: str) -> str:
    """Generate full report as multi-line string.
    Raises ValueError on parse errors.
    """
    students = parse_all(input)
    lines = [format_student(s) for s in students]
    cavg = class_average(students)
    honors = honor_roll(students)
    return "\n".join(lines) + f"\nclass average: {cavg}\nhonor roll: {', '.join(honors)}"


def top_students(input: str, n: int) -> list[str]:
    """Return the names of the top N students by average score (descending).
    Ties in average are broken alphabetically (ascending).
    If N exceeds the number of students, return all students sorted by score desc.
    Raises ValueError on parse errors (propagated from parse_all).
    """
    students = parse_all(input)
    sorted_students = sorted(students, key=lambda s: (-average(s), s[0]))
    return [s[0] for s in sorted_students[:n]]


# Tests
assert parse_student("Alice:85,92,78") == ["Alice", "85", "92", "78"], "parse single student"
assert parse_student("Bob:100") == ["Bob", "100"], "parse student with single score"

try:
    parse_student(":85,92")
    assert False
except ValueError as e:
    assert str(e) == "empty name", "parse student empty name"

try:
    parse_student("Alice:85,abc,78")
    assert False
except ValueError as e:
    assert str(e) == "invalid score: abc", "parse student invalid score"

try:
    parse_student("Alice:85,101,78")
    assert False
except ValueError as e:
    assert str(e) == "score out of range: 101", "parse student score too high"

try:
    parse_student("Alice:85,-1,78")
    assert False
except ValueError as e:
    assert str(e) == "score out of range: -1", "parse student score negative"

try:
    parse_student("Alice:")
    assert False
except ValueError as e:
    assert str(e) == "no scores", "parse student no scores"

try:
    parse_student("Alice")
    assert False
except ValueError as e:
    assert str(e) == "invalid format", "parse student no colon"

assert parse_all("Alice:85,92\nBob:70,80") == [["Alice", "85", "92"], ["Bob", "70", "80"]], "parse all basic"

try:
    parse_all("")
    assert False
except ValueError as e:
    assert str(e) == "empty input", "parse all empty input"

try:
    parse_all("Alice:85\nAlice:90")
    assert False
except ValueError as e:
    assert str(e) == "duplicate name: Alice", "parse all duplicate names"

try:
    parse_all("Alice:85\n:90")
    assert False
except ValueError as e:
    assert str(e) == "line 2: empty name", "parse all error with line number"

assert average(["Alice", "80", "90", "100"]) == 90, "average basic"
assert average(["Bob", "75"]) == 75, "average single score"
assert average(["Carol", "80", "81"]) == 80, "average rounds down"

assert letter_grade(95) == "A", "letter grade A"
assert letter_grade(85) == "B", "letter grade B"
assert letter_grade(75) == "C", "letter grade C"
assert letter_grade(65) == "D", "letter grade D"
assert letter_grade(55) == "F", "letter grade F"
assert letter_grade(90) == "A", "letter grade boundary 90"
assert letter_grade(80) == "B", "letter grade boundary 80"

assert honor_roll([["Alice", "90", "95"], ["Bob", "70", "80"], ["Carol", "85", "90"]]) == ["Alice", "Carol"], "honor roll basic"
assert honor_roll([["Alice", "70", "75"], ["Bob", "60", "65"]]) == [], "honor roll none qualify"
assert honor_roll([["Zara", "90", "95"], ["Alice", "85", "90"]]) == ["Alice", "Zara"], "honor roll sorted"

assert class_average([["Alice", "80", "90"], ["Bob", "70", "80"]]) == 80, "class average basic"

assert format_student(["Alice", "85", "92", "78"]) == "Alice: 85 (B)", "format student"

assert generate_report("Alice:90,95,100\nBob:70,75,80") == "Alice: 95 (A)\nBob: 75 (C)\nclass average: 85\nhonor roll: Alice", "generate report"

try:
    generate_report("Alice:90\n:bad")
    assert False
except ValueError as e:
    assert str(e) == "line 2: empty name", "generate report error propagation"

try:
    generate_report("")
    assert False
except ValueError as e:
    assert str(e) == "empty input", "generate report empty"


# ========== V2 TESTS ==========

assert top_students("Alice:90,95\nBob:70,80\nCarol:85,88", 2) == ["Alice", "Carol"], "top students basic"
assert top_students("Bob:85,85\nAlice:85,85", 2) == ["Alice", "Bob"], "top students tie breaking"
assert top_students("Alice:90,95", 5) == ["Alice"], "top students n exceeds count"
try:
    top_students("", 3)
    assert False, "top students empty should raise"
except ValueError as e:
    assert str(e) == "empty input", "top students error propagation"
assert top_students("Alice:90,95\nBob:70,80\nCarol:85,88", 1) == ["Alice"], "top students single"
