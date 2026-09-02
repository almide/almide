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

### `string.chars(s: String) -> List[String]`

Split a string into individual characters.

```almd run
fn main() -> Unit = {
  println("${string.chars("abc")}")
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

### `string.codepoint(s: String) -> Option[Int]`

Return the Unicode codepoint of the first character, or none for empty string.

```almd run
fn show(o: Option[Int]) -> String = match o {
  some(n) => "some(${n})",
  none => "none",
}

fn main() -> Unit = {
  println(show(string.codepoint("A")))
  println(show(string.codepoint("")))
}
```
```output
some(65)
none
```

### `string.from_codepoint(n: Int) -> String`

Create a single-character string from a Unicode codepoint.

```almd run
fn main() -> Unit = {
  println(string.from_codepoint(65))
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

### `string.count(s: String, sub: String) -> Int`

Count occurrences of a substring.

```almd run
fn main() -> Unit = {
  println(int.to_string(string.count("banana", "an")))
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

## Signature index (49 functions)

```
string.trim(s: String) -> String
string.split(s: String, sep: String) -> List[String]
string.split_once(s: String, sep: String) -> Option[()]
string.join(list: List[String], sep: String) -> String
string.len(s: String) -> Int
string.length(s: String) -> Int   (deprecated — use string.len)
string.contains(s: String, sub: String) -> Bool
string.starts_with(s: String, prefix: String) -> Bool
string.ends_with(s: String, suffix: String) -> Bool
string.slice(s: String, start: Int, end: Int) -> String
string.pad_start(s: String, n: Int, ch: String) -> String
string.to_bytes(s: String) -> List[Int]
string.capitalize(s: String) -> String
string.to_upper(s: String) -> String
string.to_lower(s: String) -> String
string.replace(s: String, from: String, to: String) -> String
string.get(s: String, i: Int) -> Option[String]
string.lines(s: String) -> List[String]
string.chars(s: String) -> List[String]
string.index_of(s: String, needle: String) -> Option[Int]
string.repeat(s: String, n: Int) -> String
string.from_bytes(bytes: List[Int]) -> String
string.is_digit(s: String) -> Bool
string.is_alpha(s: String) -> Bool
string.is_alphanumeric(s: String) -> Bool
string.is_whitespace(s: String) -> Bool
string.is_upper(s: String) -> Bool
string.is_lower(s: String) -> Bool
string.codepoint(s: String) -> Option[Int]
string.from_codepoint(n: Int) -> String
string.pad_end(s: String, n: Int, ch: String) -> String
string.trim_start(s: String) -> String
string.trim_end(s: String) -> String
string.count(s: String, sub: String) -> Int
string.is_empty(s: String) -> Bool
string.reverse(s: String) -> String
string.strip_prefix(s: String, prefix: String) -> Option[String]
string.strip_suffix(s: String, suffix: String) -> Option[String]
string.replace_first(s: String, from: String, to: String) -> String
string.last_index_of(s: String, needle: String) -> Option[Int]
string.first(s: String) -> Option[String]
string.last(s: String) -> Option[String]
string.take(s: String, n: Int) -> String
string.take_end(s: String, n: Int) -> String
string.drop(s: String, n: Int) -> String
string.drop_end(s: String, n: Int) -> String
string.run_length_encode(s: String) -> List[()]
string.push(s: String, suffix: String) -> Unit
string.clear(s: String) -> Unit
```

<!-- END GENERATED SIGNATURE INDEX -->
