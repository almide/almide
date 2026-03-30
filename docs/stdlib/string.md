# string

String manipulation. auto-imported.

### `string.trim(s: String) -> String`

Remove leading and trailing whitespace.

```almd
string.trim("  hello  ") // => "hello"
```

### `string.split(s: String, sep: String) -> List[String]`

Split a string by separator into a list of substrings.

```almd
string.split("a,b,c", ",") // => ["a", "b", "c"]
```

### `string.join(list: List[String], sep: String) -> String`

Join a list of strings with a separator.

```almd
string.join(["a", "b", "c"], "-") // => "a-b-c"
```

### `string.len(s: String) -> Int`

Return the number of characters in a string.

```almd
string.len("hello") // => 5
```

### `string.contains(s: String, sub: String) -> Bool`

Check if a string contains a substring.

```almd
string.contains("hello world", "world") // => true
```

### `string.starts_with(s: String, prefix: String) -> Bool`

Check if a string starts with a prefix.

```almd
string.starts_with("hello", "hel") // => true
```

### `string.ends_with(s: String, suffix: String) -> Bool`

Check if a string ends with a suffix.

```almd
string.ends_with("hello", "llo") // => true
```

### `string.slice(s: String, start: Int, end: Int) -> String`

Extract a substring by start and optional end index.

```almd
string.slice("hello", 1, 4) // => "ell"
```

### `string.pad_start(s: String, n: Int, ch: String) -> String`

Pad a string on the left to reach a target length.

```almd
string.pad_start("42", 5, "0") // => "00042"
```

### `string.to_bytes(s: String) -> List[Int]`

Convert a string to a list of UTF-8 byte values.

```almd
string.to_bytes("Hi") // => [72, 105]
```

### `string.capitalize(s: String) -> String`

Capitalize the first character of a string.

```almd
string.capitalize("hello") // => "Hello"
```

### `string.to_upper(s: String) -> String`

Convert all characters to uppercase.

```almd
string.to_upper("hello") // => "HELLO"
```

### `string.to_lower(s: String) -> String`

Convert all characters to lowercase.

```almd
string.to_lower("HELLO") // => "hello"
```

### `string.replace(s: String, from: String, to: String) -> String`

Replace all occurrences of a substring.

```almd
string.replace("aabbcc", "bb", "XX") // => "aaXXcc"
```

### `string.get(s: String, i: Int) -> Option[String]`

Get the character at a given index, or none if out of bounds.

```almd
string.char_at("hello", 1) // => some("e")
```

### `string.lines(s: String) -> List[String]`

Split a string into lines.

```almd
string.lines("a\nb\nc") // => ["a", "b", "c"]
```

### `string.chars(s: String) -> List[String]`

Split a string into individual characters.

```almd
string.chars("abc") // => ["a", "b", "c"]
```

### `string.index_of(s: String, needle: String) -> Option[Int]`

Find the first index of a substring, or none if not found.

```almd
string.index_of("hello", "ll") // => some(2)
```

### `string.repeat(s: String, n: Int) -> String`

Repeat a string n times.

```almd
string.repeat("ab", 3) // => "ababab"
```

### `string.from_bytes(bytes: List[Int]) -> String`

Create a string from a list of UTF-8 byte values.

```almd
string.from_bytes([72, 105]) // => "Hi"
```

### `string.is_digit(s: String) -> Bool`

Check if all characters are ASCII digits.

```almd
string.is_digit("123") // => true
```

### `string.is_alpha(s: String) -> Bool`

Check if all characters are alphabetic.

```almd
string.is_alpha("abc") // => true
```

### `string.is_alphanumeric(s: String) -> Bool`

Check if all characters are alphanumeric.

```almd
string.is_alphanumeric("abc123") // => true
```

### `string.is_whitespace(s: String) -> Bool`

Check if all characters are whitespace.

```almd
string.is_whitespace("  ") // => true
```

### `string.is_upper(s: String) -> Bool`

Check if all characters in the string are uppercase.

```almd
string.is_upper("ABC") // => true
```

### `string.is_lower(s: String) -> Bool`

Check if all characters in the string are lowercase.

```almd
string.is_lower("abc") // => true
```

### `string.codepoint(s: String) -> Option[Int]`

Return the Unicode codepoint of the first character, or none for empty string.

```almd
string.codepoint("A") // => some(65)
```

### `string.from_codepoint(n: Int) -> String`

Create a single-character string from a Unicode codepoint.

```almd
string.from_codepoint(65) // => "A"
```

### `string.pad_end(s: String, n: Int, ch: String) -> String`

Pad a string on the right to reach a target length.

```almd
string.pad_end("hi", 5, ".") // => "hi..."
```

### `string.trim_start(s: String) -> String`

Remove leading whitespace.

```almd
string.trim_start("  hello") // => "hello"
```

### `string.trim_end(s: String) -> String`

Remove trailing whitespace.

```almd
string.trim_end("hello  ") // => "hello"
```

### `string.count(s: String, sub: String) -> Int`

Count occurrences of a substring.

```almd
string.count("banana", "an") // => 2
```

### `string.is_empty(s: String) -> Bool`

Check if a string is empty.

```almd
string.is_empty("") // => true
```

### `string.reverse(s: String) -> String`

Reverse the characters in a string.

```almd
string.reverse("hello") // => "olleh"
```

### `string.strip_prefix(s: String, prefix: String) -> Option[String]`

Remove a prefix if present, returning none if not found.

```almd
string.strip_prefix("hello", "hel") // => some("lo")
```

### `string.strip_suffix(s: String, suffix: String) -> Option[String]`

Remove a suffix if present, returning none if not found.

```almd
string.strip_suffix("hello", "llo") // => some("he")
```

### `string.replace_first(s: String, from: String, to: String) -> String`

Replace the first occurrence of a substring.

```almd
string.replace_first("aabaa", "a", "X") // => "Xabaa"
```

### `string.last_index_of(s: String, needle: String) -> Option[Int]`

Find the last index of a substring, or none if not found.

```almd
string.last_index_of("abcabc", "bc") // => some(4)
```

### `string.first(s: String) -> Option[String]`

Get the first character of a string.

```almd
string.first("hello") // => some("h")
```

### `string.last(s: String) -> Option[String]`

Get the last character of a string.

```almd
string.last("hello") // => some("o")
```

### `string.take(s: String, n: Int) -> String`

Take the first N characters.

```almd
string.take("hello", 3) // => "hel"
```

### `string.take_end(s: String, n: Int) -> String`

Take the last N characters.

```almd
string.take_end("hello", 3) // => "llo"
```

### `string.drop(s: String, n: Int) -> String`

Drop the first N characters.

```almd
string.drop("hello", 2) // => "llo"
```

### `string.drop_end(s: String, n: Int) -> String`

Drop the last N characters.

```almd
string.drop_end("hello", 2) // => "hel"
```
