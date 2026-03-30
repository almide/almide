# regex

Regular expressions. import regex.

### `regex.is_match(pat: String, s: String) -> Bool`

Check if a pattern matches anywhere in a string.

```almd
regex.is_match("[0-9]+", "abc123") // => true
```

### `regex.full_match(pat: String, s: String) -> Bool`

Check if a pattern matches the entire string.

```almd
regex.full_match("[0-9]+", "123") // => true
```

### `regex.find(pat: String, s: String) -> Option[String]`

Find the first match of a pattern in a string.

```almd
regex.find("[0-9]+", "abc123def") // => some("123")
```

### `regex.find_all(pat: String, s: String) -> List[String]`

Find all non-overlapping matches of a pattern.

```almd
regex.find_all("[0-9]+", "a1b2c3") // => ["1", "2", "3"]
```

### `regex.replace(pat: String, s: String, rep: String) -> String`

Replace all matches of a pattern with a replacement string.

```almd
regex.replace("[0-9]+", "a1b2", "X") // => "aXbX"
```

### `regex.replace_first(pat: String, s: String, rep: String) -> String`

Replace the first match of a pattern.

```almd
regex.replace_first("[0-9]+", "a1b2", "X") // => "aXb2"
```

### `regex.split(pat: String, s: String) -> List[String]`

Split a string by a regex pattern.

```almd
regex.split("[,;]", "a,b;c") // => ["a", "b", "c"]
```

### `regex.captures(pat: String, s: String) -> Option[List[String]]`

Extract capture groups from the first match.

```almd
regex.captures("(\\w+)@(\\w+)", "user@host") // => some(["user@host", "user", "host"])
```
