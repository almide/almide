# regex

Regular expressions. import regex.

### `regex.is_match(pat: String, s: String) -> Bool`

Check if a pattern matches anywhere in a string.

```almd run
import regex

fn main() -> Unit = {
  println("${regex.is_match("[0-9]+", "abc123")}")
}
```
```output
true
```

### `regex.full_match(pat: String, s: String) -> Bool`

Check if a pattern matches the entire string.

```almd run
import regex

fn main() -> Unit = {
  println("${regex.full_match("[0-9]+", "123")}")
  println("${regex.full_match("[0-9]+", "abc123")}")
}
```
```output
true
false
```

### `regex.find(pat: String, s: String) -> Option[String]`

Find the first match of a pattern in a string.

```almd run
import regex

fn show(o: Option[String]) -> String = match o {
  some(s) => "some(\"${s}\")",
  none => "none",
}

fn main() -> Unit = {
  println(show(regex.find("[0-9]+", "abc123def")))
  println(show(regex.find("[0-9]+", "abcdef")))
}
```
```output
some("123")
none
```

### `regex.find_all(pat: String, s: String) -> List[String]`

Find all non-overlapping matches of a pattern.

```almd run
import regex

fn main() -> Unit = {
  println("${regex.find_all("[0-9]+", "a1b2c3")}")
}
```
```output
["1", "2", "3"]
```

### `regex.replace(pat: String, s: String, rep: String) -> String`

Replace all matches of a pattern with a replacement string.

```almd run
import regex

fn main() -> Unit = {
  println(regex.replace("[0-9]+", "a1b2", "X"))
}
```
```output
aXbX
```

### `regex.replace_first(pat: String, s: String, rep: String) -> String`

Replace the first match of a pattern.

```almd run
import regex

fn main() -> Unit = {
  println(regex.replace_first("[0-9]+", "a1b2", "X"))
}
```
```output
aXb2
```

### `regex.split(pat: String, s: String) -> List[String]`

Split a string by a regex pattern.

```almd run
import regex

fn main() -> Unit = {
  println("${regex.split("[,;]", "a,b;c")}")
}
```
```output
["a", "b", "c"]
```

### `regex.captures(pat: String, s: String) -> Option[List[String]]`

Extract the first match and its capture groups. **Index 0 is the whole match**;
1.. are the groups in lexical order, an unmatched optional group being `""`.

`none` means the pattern did not match — nothing else. A pattern with no groups
that DOES match answers a one-element list holding its whole match.

```almd
regex.captures("(\\w+)@(\\w+)", "user@host") // => some(["user@host", "user", "host"])
regex.captures("(x)?(y)", "y")               // => some(["y", "", "y"])
regex.captures("b+", "aabbb!")               // => some(["bbb"])
regex.captures("(z)", "abc")                 // => none
```

Executable form of the rows above — gated by `scripts/check-doc-fences.sh`
(this is the function whose documented behavior once drifted from the
implementation, #1432; the pin below cannot):

```almd run
import regex

fn show(o: Option[List[String]]) -> String = match o {
  some(xs) => "[" + (xs |> list.join(",")) + "]",
  none => "none",
}

effect fn main() -> Unit = {
  println(show(regex.captures("(\\w+)@(\\w+)", "user@host")))
  println(show(regex.captures("(x)?(y)", "y")))
  println(show(regex.captures("b+", "aabbb!")))
  println(show(regex.captures("(z)", "abc")))
}
```
```output
[user@host,user,host]
[y,,y]
[bbb]
none
```

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (8 functions)

```
regex.is_match(pat: String, s: String) -> Bool
regex.full_match(pat: String, s: String) -> Bool
regex.find(pat: String, s: String) -> Option[String]
regex.find_all(pat: String, s: String) -> List[String]
regex.replace(pat: String, s: String, rep: String) -> String
regex.replace_first(pat: String, s: String, rep: String) -> String
regex.split(pat: String, s: String) -> List[String]
regex.captures(pat: String, s: String) -> Option[List[String]]
```

<!-- END GENERATED SIGNATURE INDEX -->
