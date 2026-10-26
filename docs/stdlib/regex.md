# regex

Regular expressions. import regex.

A pattern becomes a `Regex` once, and every operation takes the `Regex`
first and the subject second (#3942):

- `regex.new(pattern)` — for a pattern the program knows is valid. A string
  literal is checked by `almide check` (E102); a pattern built at run time
  that is refused aborts with `Error: invalid regex: <reason>`, exit 1.
- `regex.compile(pattern)` — for a pattern from input: `ok(re)`, or
  `err(reason)` when it is refused.

Because the first argument is a `Regex`, piping the subject into an
operation is a type error: `s |> regex.find("[0-9]+")` used to search the
pattern for the subject without a word, and now points at
`regex.find(regex.new("[0-9]+"), s)`. Make the `Regex` once and reuse it in a
loop.

### `regex.new(pattern: String) -> Regex` and `regex.compile(pattern: String) -> Result[Regex, String]`

```almd run
import regex

fn reason(p: String) -> String = match regex.compile(p) {
  ok(_) => "ok",
  err(e) => e,
}

fn main() -> Unit = {
  let digits = regex.new("[0-9]+")
  println("${regex.is_match(digits, "abc123")}")
  println(reason("[0-9]+"))
  println(reason("(a|b"))
  println(reason("a{"))
  println(reason("(?P<year>\\d{4})"))
  println(reason("((a){200}){200}"))
}
```
```output
true
ok
unclosed group
malformed repetition
named groups are not supported
pattern too large: more than 20000 states
```

The reasons, first violation from the left: `unclosed group`, `unmatched )`,
`unclosed character class`, `nothing to repeat`, `malformed repetition`,
`repetition minimum exceeds maximum`, `invalid class range`,
`trailing backslash`, `unsupported escape \X` (and `… in a class`),
`named groups are not supported`, `lookaround is not supported`,
`flags are only supported at the start of the pattern`,
`unsupported group syntax`, `groups nested deeper than 250`,
`pattern too large: more than 20000 states` (C-395).

### `regex.is_match(re: Regex, s: String) -> Bool` and `regex.is_full_match(re: Regex, s: String) -> Bool`

`is_match`: re matches somewhere in s. `is_full_match`: re matches the whole
of s (`regex.full_match` is its deprecated old name, removed at epoch 16).

```almd run
import regex

fn main() -> Unit = {
  let digits = regex.new("[0-9]+")
  println("${regex.is_match(digits, "abc123")}")
  println("${regex.is_full_match(digits, "123")}")
  println("${regex.is_full_match(digits, "abc123")}")
}
```
```output
true
true
false
```

### `regex.find(re: Regex, s: String) -> Option[String]` and `regex.index_of(re: Regex, s: String) -> Option[Int]`

The first match, and where it starts — a code-point index, the unit
`string.index_of` uses.

```almd run
import regex

fn main() -> Unit = {
  let digits = regex.new("[0-9]+")
  println("${regex.find(digits, "abc123def")} ${regex.find(digits, "abcdef")}")
  println("${regex.index_of(digits, "日本123")} ${regex.index_of(digits, "abc")}")
}
```
```output
some("123") none
some(2) none
```

### `regex.find_all(re: Regex, s: String) -> List[String]`

All non-overlapping matches, in order.

```almd run
import regex

fn main() -> Unit = {
  println("${regex.find_all(regex.new("[0-9]+"), "a1b22c333")}")
}
```
```output
["1", "22", "333"]
```

### `regex.replace` / `replace_first` / `replace_with` / `replace_first_with`

Replacement is {all, first} × {literal text, a function of the match}. The
literal text is inserted as written — `$1` is not read. A function receives
`[whole, group 1, ...]`, a group that took no part being `none`.

```almd run
import regex

fn main() -> Unit = {
  let num = regex.new("([0-9])([0-9])?")
  println(regex.replace(num, "a1b22", "#"))
  println(regex.replace_first(num, "a1b22", "$1"))
  println(regex.replace_with(num, "a1b22", (g) => "<" + ((list.get(g, 2) ?? none) ?? "-") + ">"))
  println(regex.replace_first_with(num, "a1b22", (g) => "[" + ((list.get(g, 0) ?? none) ?? "") + "]"))
}
```
```output
a#b#
a$1b22
a<->b<2>
a[1]b22
```

### `regex.split(re: Regex, s: String) -> List[String]`

The text between matches; `[s]` when there is no match.

```almd run
import regex

fn main() -> Unit = {
  println("${regex.split(regex.new("[,;]"), "a,b;c")}")
}
```
```output
["a", "b", "c"]
```

### `regex.captures(re: Regex, s: String) -> Option[List[Option[String]]]` and `regex.captures_all`

The first match's `[whole, group 1, ...]`, or `none` when nothing matched.
Index 0 is the whole match and is always `some`; a group that took no part is
`none` (R-regex-08), which an empty match of a group is not.
`captures_all` answers the same list for every match, in order.

```almd run
import regex

fn show(cs: List[String?]) -> String = "[" + (cs |> list.map((c) => c ?? "none") |> list.join(",")) + "]"

fn main() -> Unit = {
  println("${regex.captures(regex.new("(\\w+)@(\\w+)"), "user@host") |> option.map((cs) => show(cs))}")
  println("${regex.captures(regex.new("(x)?(y)"), "y") |> option.map((cs) => show(cs))}")
  println("${regex.captures(regex.new("(a*)b"), "b") |> option.map((cs) => show(cs))}")
  println("${regex.captures(regex.new("(z)"), "abc") |> option.map((cs) => show(cs))}")
  println("${regex.captures_all(regex.new("(\\d)(\\d)?"), "1 23") |> list.map((cs) => show(cs))}")
}
```
```output
some("[user@host,user,host]")
some("[y,none,y]")
some("[b,]")
none
["[1,1,none]", "[23,2,3]"]
```

### `regex.escape(s: String) -> String`

`s` with every metacharacter escaped, so a pattern built from text matches
that text.

```almd run
import regex

fn main() -> Unit = {
  let user = "1+1=2?"
  println(regex.escape(user))
  println("${regex.is_match(regex.new(regex.escape(user)), "is 1+1=2?")}")
}
```
```output
1\+1=2\?
true
```

## Supported syntax

A match is the leftmost one, and among those starting there the first in this
order: alternatives left to right, greedy quantifiers longest-first, lazy
quantifiers shortest-first, a group or quantifier that leads the rest of the
pattern to fail retried with its next choice. A repeated group's capture is its
last instance; an instance that matched nothing counts but ends the
repetition. Native and wasm agree byte for byte.

Matching takes time proportional to the pattern's states times the subject's
length, whatever the pattern (#4019, #4083), and an accepted pattern has at
most 20,000 states, so no call takes longer than 20,000 × the subject's
length steps: `(a+)+$` or `(a|aa)*b` on 100,000 characters answers at once,
and the worst case — every state live at every character, `a{0,9000}b` on
100,000 `a`s — takes about 8 s native and a minute on wasm. Each call first
runs a backtracking matcher under a step budget of 4 × (pattern length + 1)
× (subject length + 1) — it is the faster engine on ordinary patterns — and,
if the budget runs out, a Pike VM (Thompson's NFA with the same preferences,
at most one thread per state) gives the same answer. A pattern's states are
its VM program's instructions — about its length, with each counted
repetition contributing one copy of what it repeats per count — times one
more than the deepest nesting of repeated groups that can match empty:
`a{0,9999}b` has exactly 20,000, and nested counts multiply, so
`((a){200}){200}` has about 120,000 and is refused. They are counted over the
pattern without expanding it.

| Syntax | Meaning |
|---|---|
| `a`, `\.`, `\\` | literal characters (escape a metacharacter with `\`) |
| `.` | any character except `\n` |
| `[abc]`, `[a-z]`, `[^0-9]` | character class, range, negated class (a leading `]` is a member) |
| `\d` `\w` `\s` and `\D` `\W` `\S` | digit, word (`[A-Za-z0-9_]`), whitespace, and their negations; inside a class `\d` `\w` `\s` `\n` `\t` |
| `\n` `\t` `\r` | control characters |
| `^`, `$` | start / end of the string |
| `\b`, `\B` | word boundary / non-boundary (ASCII word characters) |
| `x|y` | alternation |
| `(x)`, `(?:x)` | capturing / non-capturing group, nested at most 250 deep |
| `x*` `x+` `x?` `x{n}` `x{n,}` `x{n,m}` | greedy quantifiers |
| `x*?` `x+?` `x??` `x{n,m}?` | lazy quantifiers |
| `(?i)`, `(?m)`, `(?im)` at the start of the pattern | `i`: an ASCII letter matches in either case; `m`: `^` and `$` also match after / before a `\n` |

Refused (C-395): lookaround, backreferences and other escapes of a letter or
digit not listed above, named groups, flags anywhere but the start, a `{` that
is not a whole quantifier, a quantifier with nothing to repeat, and more than
20,000 states.

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (16 functions)

```
// The Regex for `pattern`; aborts with `Error: invalid regex: <reason>` when it is refused.
// @since unreleased
regex.new(pattern: String) -> Regex

// The Regex for `pattern`, or err(reason) when it is refused.
// @since unreleased
regex.compile(pattern: String) -> Result[Regex, String]

// re matches somewhere in s.
// @since 0.6.0 or earlier
regex.is_match(re: Regex, s: String) -> Bool

// re matches the whole of s.
// @since unreleased
regex.is_full_match(re: Regex, s: String) -> Bool

// re matches the whole of s (renamed regex.is_full_match).
// @since 0.6.0 or earlier
regex.full_match(re: Regex, s: String) -> Bool   (deprecated — use regex.is_full_match)

// Leftmost match of re in s, or none.
// @since 0.5.0 or earlier
regex.find(re: Regex, s: String) -> Option[String]

// All non-overlapping matches, in order.
// @since 0.5.0 or earlier
regex.find_all(re: Regex, s: String) -> List[String]

// Code-point index where the leftmost match starts, or none.
// @since unreleased
regex.index_of(re: Regex, s: String) -> Option[Int]

// All matches replaced by literal rep (`$1` is not read).
// @since 0.5.0 or earlier
regex.replace(re: Regex, s: String, rep: String) -> String

// First match replaced by literal rep.
// @since 0.5.0 or earlier
regex.replace_first(re: Regex, s: String, rep: String) -> String

// All matches replaced by f([whole, group1, ...]).
// @since unreleased
regex.replace_with(re: Regex, s: String, f: (List[Option[String]]) -> String) -> String

// First match replaced by f([whole, group1, ...]).
// @since unreleased
regex.replace_first_with(re: Regex, s: String, f: (List[Option[String]]) -> String) -> String

// Text between matches; [s] if no match.
// @since 0.5.0 or earlier
regex.split(re: Regex, s: String) -> List[String]

// [whole, group1, ...] of the first match, or none; a group that took no part is none.
// @since 0.5.0 or earlier
regex.captures(re: Regex, s: String) -> Option[List[Option[String]]]

// [whole, group1, ...] of every match, in order.
// @since unreleased
regex.captures_all(re: Regex, s: String) -> List[List[Option[String]]]

// s with every regex metacharacter escaped, so it matches itself.
// @since unreleased
regex.escape(s: String) -> String
```

## Type index (1 types)

```
// A pattern accepted by `regex.new` or `regex.compile`.
// @since unreleased
type regex.Regex = { pattern: String }
```

<!-- END GENERATED SIGNATURE INDEX -->
