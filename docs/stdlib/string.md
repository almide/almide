# string

String manipulation. auto-imported.

### `string.trim(s: String) -> String`

Remove leading and trailing whitespace.

```almd run
fn main() -> Unit = {
  println(string.trim("  hello  "))
}
```
```output
hello
```

### `string.split(s: String, sep: String) -> List[String]`

Split a string by separator into a list of substrings.

```almd run
fn main() -> Unit = {
  println("${string.split("a,b,c", ",")}")
}
```
```output
["a", "b", "c"]
```

### `string.join(list: List[String], sep: String) -> String`

Join a list of strings with a separator.

```almd run
fn main() -> Unit = {
  println(string.join(["a", "b", "c"], "-"))
}
```
```output
a-b-c
```

`len`, `get`, `slice`, and search-result positions count Unicode codepoints,
not UTF-8 bytes or grapheme clusters. `to_bytes` returns UTF-8 byte values as
`List[Int]`; prefer `bytes.from_string` for a compact buffer. Byte offsets from
a lexer belong to `byte_slice`, not `slice`.

### `string.len(s: String) -> Int`

Return the number of characters in a string.

```almd run
fn main() -> Unit = {
  println(int.to_string(string.len("hello")))
}
```
```output
5
```

### `string.contains(s: String, sub: String) -> Bool`

Check if a string contains a substring.

```almd run
fn main() -> Unit = {
  println("${string.contains("hello world", "world")}")
}
```
```output
true
```

### `string.starts_with(s: String, prefix: String) -> Bool`

Check if a string starts with a prefix.

```almd run
fn main() -> Unit = {
  println("${string.starts_with("hello", "hel")}")
}
```
```output
true
```

### `string.ends_with(s: String, suffix: String) -> Bool`

Check if a string ends with a suffix.

```almd run
fn main() -> Unit = {
  println("${string.ends_with("hello", "llo")}")
}
```
```output
true
```

### `string.byte_slice(s: String, start: Int, end: Int) -> Option[String]`

Extract a half-open UTF-8 byte range. Returns `none` for negative, reversed,
out-of-bounds, or non-codepoint-boundary endpoints. Valid empty ranges return
`some("")`. Only the requested bytes are copied; the rest of the string is not
scanned. `string.byte_slice("aé🦀z", 1, 3)` returns `some("é")`.

### `string.slice(s: String, start: Int, end: Int) -> String`

Extract a substring by codepoint start and optional end index.

```almd run
fn main() -> Unit = {
  println(string.slice("hello", 1, 4))
}
```
```output
ell
```

### `string.pad_start(s: String, n: Int, ch: String) -> String`

Pad a string on the left to reach a target length.

```almd run
fn main() -> Unit = {
  println(string.pad_start("42", 5, "0"))
}
```
```output
00042
```

### `string.to_bytes(s: String) -> List[Int]`

Convert a string to a list of UTF-8 byte values.

```almd run
fn main() -> Unit = {
  println("${string.to_bytes("Hi")}")
}
```
```output
[72, 105]
```

### `string.capitalize(s: String) -> String`

Capitalize the first character of a string.

```almd run
fn main() -> Unit = {
  println(string.capitalize("hello"))
}
```
```output
Hello
```

### `string.to_upper(s: String) -> String`

Convert all characters to uppercase.

```almd run
fn main() -> Unit = {
  println(string.to_upper("hello"))
}
```
```output
HELLO
```

### `string.to_lower(s: String) -> String`

Convert all characters to lowercase.

```almd run
fn main() -> Unit = {
  println(string.to_lower("HELLO"))
}
```
```output
hello
```

### `string.replace(s: String, from: String, to: String) -> String`

Replace all occurrences of a substring.

```almd run
fn main() -> Unit = {
  println(string.replace("aabbcc", "bb", "XX"))
}
```
```output
aaXXcc
```

### `string.get(s: String, i: Int) -> Option[String]`

Get the character at a given codepoint index, or none if out of bounds.

```almd run
fn show(o: Option[String]) -> String = match o {
  some(s) => "some(\"${s}\")",
  none => "none",
}

fn main() -> Unit = {
  println(show(string.get("hello", 1)))
  println(show(string.get("hello", 9)))
}
```
```output
some("e")
none
```

### `string.lines(s: String) -> List[String]`

Split a string into lines.

```almd run
fn main() -> Unit = {
  println("${string.lines("a\nb\nc")}")
}
```
```output
["a", "b", "c"]
```

### `string.to_list(s: String) -> List[String]`

Split a string into individual characters.

```almd run
fn main() -> Unit = {
  println("${string.to_list("abc")}")
}
```
```output
["a", "b", "c"]
```

### `string.index_of(s: String, needle: String) -> Option[Int]`

Find the first codepoint index of a substring, or none if not found.

```almd run
fn show(o: Option[Int]) -> String = match o {
  some(n) => "some(${n})",
  none => "none",
}

fn main() -> Unit = {
  println(show(string.index_of("hello", "ll")))
  println(show(string.index_of("hello", "z")))
}
```
```output
some(2)
none
```

### `string.repeat(s: String, n: Int) -> String`

Repeat a string n times.

```almd run
fn main() -> Unit = {
  println(string.repeat("ab", 3))
}
```
```output
ababab
```

### `string.from_bytes(bytes: List[Int]) -> String`

Create a string from a list of UTF-8 byte values.

```almd run
fn main() -> Unit = {
  println(string.from_bytes([72, 105]))
}
```
```output
Hi
```

### `string.is_digit(s: String) -> Bool`

Check if all characters are ASCII digits.

```almd run
fn main() -> Unit = {
  println("${string.is_digit("123")}")
}
```
```output
true
```

### `string.is_alpha(s: String) -> Bool`

Check if all characters are alphabetic.

```almd run
fn main() -> Unit = {
  println("${string.is_alpha("abc")}")
}
```
```output
true
```

### `string.is_alphanumeric(s: String) -> Bool`

Check if all characters are alphanumeric.

```almd run
fn main() -> Unit = {
  println("${string.is_alphanumeric("abc123")}")
}
```
```output
true
```

### `string.is_whitespace(s: String) -> Bool`

Check if all characters are whitespace.

```almd run
fn main() -> Unit = {
  println("${string.is_whitespace("  ")}")
}
```
```output
true
```

### `string.is_upper(s: String) -> Bool`

Check if all characters in the string are uppercase.

```almd run
fn main() -> Unit = {
  println("${string.is_upper("ABC")}")
}
```
```output
true
```

### `string.is_lower(s: String) -> Bool`

Check if all characters in the string are lowercase.

```almd run
fn main() -> Unit = {
  println("${string.is_lower("abc")}")
}
```
```output
true
```

### `string.to_codepoint(s: String) -> Option[Int]`

Return the Unicode codepoint of the first character, or none for empty string.

```almd run
fn show(o: Option[Int]) -> String = match o {
  some(n) => "some(${n})",
  none => "none",
}

fn main() -> Unit = {
  println(show(string.to_codepoint("A")))
  println(show(string.to_codepoint("")))
}
```
```output
some(65)
none
```

### `string.from_codepoint(n: Int) -> String?`

Create a single-character string from a Unicode codepoint; none when n is
not a Unicode scalar value (negative, a surrogate, or past U+10FFFF).

```almd run
fn main() -> Unit = {
  println(string.from_codepoint(65) ?? "")
}
```
```output
A
```

### `string.pad_end(s: String, n: Int, ch: String) -> String`

Pad a string on the right to reach a target length.

```almd run
fn main() -> Unit = {
  println(string.pad_end("hi", 5, "."))
}
```
```output
hi...
```

### `string.trim_start(s: String) -> String`

Remove leading whitespace.

```almd run
fn main() -> Unit = {
  println(string.trim_start("  hello"))
}
```
```output
hello
```

### `string.trim_end(s: String) -> String`

Remove trailing whitespace.

```almd run
fn main() -> Unit = {
  println(string.trim_end("hello  "))
}
```
```output
hello
```

### `string.count_matches(s: String, sub: String) -> Int`

Count occurrences of a substring.

```almd run
fn main() -> Unit = {
  println(int.to_string(string.count_matches("banana", "an")))
}
```
```output
2
```

### `string.is_empty(s: String) -> Bool`

Check if a string is empty.

```almd run
fn main() -> Unit = {
  println("${string.is_empty("")}")
}
```
```output
true
```

### `string.reverse(s: String) -> String`

Reverse the characters in a string.

```almd run
fn main() -> Unit = {
  println(string.reverse("hello"))
}
```
```output
olleh
```

### `string.strip_prefix(s: String, prefix: String) -> Option[String]`

Remove a prefix if present, returning none if not found.

```almd run
fn show(o: Option[String]) -> String = match o {
  some(s) => "some(\"${s}\")",
  none => "none",
}

fn main() -> Unit = {
  println(show(string.strip_prefix("hello", "hel")))
  println(show(string.strip_prefix("hello", "lo")))
}
```
```output
some("lo")
none
```

### `string.strip_suffix(s: String, suffix: String) -> Option[String]`

Remove a suffix if present, returning none if not found.

```almd run
fn show(o: Option[String]) -> String = match o {
  some(s) => "some(\"${s}\")",
  none => "none",
}

fn main() -> Unit = {
  println(show(string.strip_suffix("hello", "llo")))
  println(show(string.strip_suffix("hello", "he")))
}
```
```output
some("he")
none
```

### `string.replace_first(s: String, from: String, to: String) -> String`

Replace the first occurrence of a substring.

```almd run
fn main() -> Unit = {
  println(string.replace_first("aabaa", "a", "X"))
}
```
```output
Xabaa
```

### `string.last_index_of(s: String, needle: String) -> Option[Int]`

Find the last codepoint index of a substring, or none if not found.

```almd run
fn show(o: Option[Int]) -> String = match o {
  some(n) => "some(${n})",
  none => "none",
}

fn main() -> Unit = {
  println(show(string.last_index_of("abcabc", "bc")))
  println(show(string.last_index_of("abcabc", "z")))
}
```
```output
some(4)
none
```

### `string.first(s: String) -> Option[String]`

Get the first character of a string.

```almd run
fn show(o: Option[String]) -> String = match o {
  some(s) => "some(\"${s}\")",
  none => "none",
}

fn main() -> Unit = {
  println(show(string.first("hello")))
  println(show(string.first("")))
}
```
```output
some("h")
none
```

### `string.last(s: String) -> Option[String]`

Get the last character of a string.

```almd run
fn show(o: Option[String]) -> String = match o {
  some(s) => "some(\"${s}\")",
  none => "none",
}

fn main() -> Unit = {
  println(show(string.last("hello")))
  println(show(string.last("")))
}
```
```output
some("o")
none
```

### `string.take(s: String, n: Int) -> String`

Take the first N characters.

```almd run
fn main() -> Unit = {
  println(string.take("hello", 3))
}
```
```output
hel
```

### `string.take_end(s: String, n: Int) -> String`

Take the last N characters.

```almd run
fn main() -> Unit = {
  println(string.take_end("hello", 3))
}
```
```output
llo
```

### `string.drop(s: String, n: Int) -> String`

Drop the first N characters.

```almd run
fn main() -> Unit = {
  println(string.drop("hello", 2))
}
```
```output
llo
```

### `string.drop_end(s: String, n: Int) -> String`

Drop the last N characters.

```almd run
fn main() -> Unit = {
  println(string.drop_end("hello", 2))
}
```
```output
hel
```

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (88 functions)

```
// s without leading/trailing whitespace.
// @since 0.5.0 or earlier
string.trim(s: String) -> String

// Pieces around sep; empty pieces kept.
// @since 0.5.0 or earlier
string.split(s: String, sep: String) -> List[String]

// Split at the FIRST occurrence of `sep`: `(before, after)`, none ⇔ `sep` absent.
// @since 0.57.0
string.split_once(s: String, sep: String) -> Option[(String, String)]

// Items with sep between; "" for [].
// @since 0.5.0 or earlier
string.join(list: List[String], sep: String) -> String

// Codepoint count (not bytes).
// @since 0.5.0 or earlier
string.len(s: String) -> Int

// Alias of string.len.
// @since 0.19.2 or earlier
string.length(s: String) -> Int   (deprecated — use string.len)

// sub occurs in s; true for empty sub.
// @since 0.6.0 or earlier
string.contains(s: String, sub: String) -> Bool

// s begins with prefix; "" always matches.
// @since 0.6.0 or earlier
string.starts_with(s: String, prefix: String) -> Bool

// s ends with suffix; "" always matches.
// @since 0.6.0 or earlier
string.ends_with(s: String, suffix: String) -> Bool

// Codepoints [start, end), clamped to s.
// @since 0.5.0 or earlier
string.slice(s: String, start: Int, end: Int) -> String

// Byte range [start, end); none if bad or mid-char.
// @since 0.63.0
string.byte_slice(s: String, start: Int, end: Int) -> Option[String]

// Left-padded to n chars with ch; never truncates.
// @since 0.6.1 or earlier
string.pad_start(s: String, n: Int, ch: String) -> String

// UTF-8 bytes of s as Ints 0..255.
// @since 0.5.0 or earlier
string.to_bytes(s: String) -> List[Int]

// First char uppercased, rest untouched.
// @since 0.5.13 or earlier
string.capitalize(s: String) -> String

// Unicode uppercase; may lengthen s.
// @since 0.5.0 or earlier
string.to_upper(s: String) -> String

// Unicode lowercase, not just ASCII.
// @since 0.5.0 or earlier
string.to_lower(s: String) -> String

// Every from replaced by to.
// @since 0.5.0 or earlier
string.replace(s: String, from: String, to: String) -> String

// Codepoint at i, or none if out of range.
// @since 0.6.1 or earlier
string.get(s: String, i: Int) -> Option[String]

// Split on \n or \r\n; endings dropped.
// @since 0.5.0 or earlier
string.lines(s: String) -> List[String]

// Each codepoint as a one-char string.
// @since 0.5.0 or earlier
string.to_list(s: String) -> List[String]

// Codepoint index of first needle, or none.
// @since 0.5.0 or earlier
string.index_of(s: String, needle: String) -> Option[Int]

// s repeated n times; empty for n <= 0.
// @since 0.5.0 or earlier
string.repeat(s: String, n: Int) -> String

// Lossy UTF-8 decode; bad sequences become U+FFFD.
// @since 0.5.0 or earlier
string.from_bytes(bytes: List[Int]) -> String

// All ASCII 0-9; false for empty s.
// @since 0.6.0 or earlier
string.is_digit(s: String) -> Bool

// All Unicode letters; false for empty s.
// @since 0.6.0 or earlier
string.is_alpha(s: String) -> Bool

// All letters/digits; false for empty s.
// @since 0.6.0 or earlier
string.is_alphanumeric(s: String) -> Bool

// All whitespace; true for empty s.
// @since 0.6.0 or earlier
string.is_whitespace(s: String) -> Bool

// Has letters, all uppercase; others ignored.
// @since 0.6.0 or earlier
string.is_upper(s: String) -> Bool

// Has letters, all lowercase; others ignored.
// @since 0.6.0 or earlier
string.is_lower(s: String) -> Bool

// Scalar value of first char; none if s empty.
// @since 0.6.0 or earlier
string.to_codepoint(s: String) -> Option[Int]

// One-char string; "" if n is not a scalar.
// @since 0.6.0 or earlier
(string.from_codepoint(n: Int) ?? "") -> String

// Right-padded to n chars with ch; never truncates.
// @since 0.6.1 or earlier
string.pad_end(s: String, n: Int, ch: String) -> String

// s without leading whitespace.
// @since 0.5.0 or earlier
string.trim_start(s: String) -> String

// s without trailing whitespace.
// @since 0.5.0 or earlier
string.trim_end(s: String) -> String

// Non-overlapping sub count; len+1 if sub empty.
// @since 0.5.0 or earlier
string.count_matches(s: String, sub: String) -> Int

// True only for ""; " " is not empty.
// @since 0.6.0 or earlier
string.is_empty(s: String) -> Bool

// Codepoints reversed; not grapheme-aware.
// @since 0.5.0 or earlier
string.reverse(s: String) -> String

// s without prefix, or none if s lacks it.
// @since 0.5.0 or earlier
string.strip_prefix(s: String, prefix: String) -> Option[String]

// s without suffix, or none if s lacks it.
// @since 0.5.0 or earlier
string.strip_suffix(s: String, suffix: String) -> Option[String]

// First from replaced by to, if any.
// @since 0.5.0 or earlier
string.replace_first(s: String, from: String, to: String) -> String

// Codepoint index of last needle, or none.
// @since 0.5.0 or earlier
string.last_index_of(s: String, needle: String) -> Option[Int]

// Leading codepoint, or none if empty.
// @since 0.7.0 or earlier
string.first(s: String) -> Option[String]

// Final codepoint, or none if empty.
// @since 0.7.0 or earlier
string.last(s: String) -> Option[String]

// First n codepoints; all of s if n < 0.
// @since 0.7.0 or earlier
string.take(s: String, n: Int) -> String

// Last n codepoints; all of s if n < 0.
// @since 0.7.0 or earlier
string.take_end(s: String, n: Int) -> String

// s minus first n codepoints; "" if n < 0.
// @since 0.7.0 or earlier
string.drop(s: String, n: Int) -> String

// s minus last n codepoints; "" if n < 0.
// @since 0.7.0 or earlier
string.drop_end(s: String, n: Int) -> String

// (char, count) per run of equal codepoints.
// @since 0.14.6 or earlier
string.run_length_encode(s: String) -> List[(String, Int)]

// Appends suffix to s in place.
// @since 0.23.3 or earlier
string.push(mut s: String, suffix: String) -> Unit

// Empties s in place.
// @since 0.23.3 or earlier
string.clear(mut s: String) -> Unit

// Negative if a sorts before b, 0 if equal, positive after; codepoint order.
// @since unreleased
string.compare(a: String, b: String) -> Int

// UTF-8 byte count (string.len counts codepoints).
// @since unreleased
string.byte_len(s: String) -> Int

// UTF-16 code units; a codepoint past U+FFFF is a surrogate pair.
// @since unreleased
string.to_utf16(s: String) -> List[Int]

// String from UTF-16 code units; err on an unpaired surrogate or a unit outside 0..65535.
// @since unreleased
string.from_utf16(xs: List[Int]) -> Result[String, String]

// The characters of s for which f is true.
// @since unreleased
string.filter(s: String, f: (String) -> Bool) -> String

// f folded over the characters of s, left to right.
// @since unreleased
string.fold(s: String, init: B, f: (B, String) -> B) -> B

// s with x inserted before character i; i outside 0..=len aborts.
// @since unreleased
string.insert(s: String, i: Int, x: String) -> String

// s without character i; i outside 0..len aborts.
// @since unreleased
string.remove_at(s: String, i: Int) -> String

// The empty string.
// @since unreleased
string.new() -> String

// The strings of xs joined with nothing between.
// @since unreleased
string.from_list(xs: List[String]) -> String

// (index, character) pairs from 0.
// @since unreleased
string.enumerate(s: String) -> List[(Int, String)]

// Characters paired with ys; stops at the shorter.
// @since unreleased
string.zip(s: String, ys: List[B]) -> List[(String, B)]

// f over characters paired with ys; stops at the shorter.
// @since unreleased
string.zip_with(s: String, ys: List[B], f: (String, B) -> C) -> List[C]

// The some results of f over the characters, in order.
// @since unreleased
string.filter_map(s: String, f: (String) -> Option[B]) -> List[B]

// The first some result of f over the characters, or none.
// @since unreleased
string.find_map(s: String, f: (String) -> Option[B]) -> Option[B]

// f folded over the characters from the first; none if s is empty.
// @since unreleased
string.reduce(s: String, f: (String, String) -> String) -> Option[String]

// The running fold over the characters, init excluded.
// @since unreleased
string.scan(s: String, init: B, f: (B, String) -> B) -> List[B]

// (characters where f is true, the rest), each in order.
// @since unreleased
string.partition(s: String, f: (String) -> Bool) -> (List[String], List[String])

// Characters grouped by f's key, each group in order.
// @since unreleased
string.group_by(s: String, f: (String) -> B) -> Map[B, List[String]]

// The characters in codepoint order.
// @since unreleased
string.sort(s: String) -> List[String]

// The characters ordered by f's key; equal keys keep their order.
// @since unreleased
string.sort_by(s: String, f: (String) -> B) -> List[String]

// The characters ordered by cmp (negative = first argument first); stable.
// @since unreleased
string.sort_with(s: String, cmp: (String, String) -> Int) -> List[String]

// The smallest character in codepoint order; none if empty.
// @since unreleased
string.min(s: String) -> Option[String]

// The largest character in codepoint order; none if empty.
// @since unreleased
string.max(s: String) -> Option[String]

// The first character with the smallest key; none if empty.
// @since unreleased
string.min_by(s: String, f: (String) -> K) -> Option[String]

// The last character with the largest key; none if empty.
// @since unreleased
string.max_by(s: String, f: (String) -> K) -> Option[String]

// The first smallest character by cmp; none if empty.
// @since unreleased
string.min_with(s: String, cmp: (String, String) -> Int) -> Option[String]

// The last largest character by cmp; none if empty.
// @since unreleased
string.max_with(s: String, cmp: (String, String) -> Int) -> Option[String]

// Index of the first character where f is true, or none.
// @since unreleased
string.find_index(s: String, f: (String) -> Bool) -> Option[Int]

// The longest prefix whose characters all satisfy f.
// @since unreleased
string.take_while(s: String, f: (String) -> Bool) -> String

// s after the longest prefix whose characters all satisfy f.
// @since unreleased
string.drop_while(s: String, f: (String) -> Bool) -> String

// Runs of n characters, the last may be shorter; n <= 0 aborts.
// @since unreleased
string.chunk(s: String, n: Int) -> List[String]

// Every run of n consecutive characters; [] if n > len; n <= 0 aborts.
// @since unreleased
string.window(s: String, n: Int) -> List[String]

// (first n characters, the rest); n past the end gives (s, ""); n < 0 aborts.
// @since unreleased
string.split_at(s: String, n: Int) -> (String, String)

// s with each run of equal adjacent characters collapsed to one.
// @since unreleased
string.dedup(s: String) -> String

// The characters of s with sep between each pair.
// @since unreleased
string.intersperse(s: String, sep: String) -> String

// The first occurrence of each character, in order.
// @since unreleased
string.unique(s: String) -> String

// The first character of each key f gives, in order.
// @since unreleased
string.unique_by(s: String, f: (String) -> K) -> String
```

<!-- END GENERATED SIGNATURE INDEX -->
