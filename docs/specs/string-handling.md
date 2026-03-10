# String Handling Specification

> Verified by: string operations used throughout all exercises; heredoc/interpolation tested in stdlib and pipeline exercises.

---

## 1. String Literals

### 1.1 Regular Strings

```almide
let s = "hello world"
```

Standard double-quoted strings. May contain escape sequences and interpolation.

### 1.2 Raw Strings

```almide
let s = r"no \n escaping here"
```

Prefixed with `r`. No escape processing, no interpolation. The content is taken literally.

---

## 2. Escape Sequences

The following escape sequences are recognized in regular strings and heredocs (but not in raw variants):

| Escape | Character |
|--------|-----------|
| `\n` | Newline |
| `\t` | Tab |
| `\\` | Backslash |
| `\"` | Double quote |
| `\$` | Dollar sign (prevents interpolation) |

Any other character after `\` is passed through as-is.

---

## 3. String Interpolation

```almide
let name = "world"
let greeting = "hello ${name}"          -- "hello world"
let result = "sum is ${1 + 2}"          -- "sum is 3"
let nested = "len = ${string.length(s)}" -- function calls work
```

### 3.1 Syntax

Interpolation is triggered by `${expr}` inside a regular (non-raw) string. The expression between `{` and `}` is parsed and type-checked by the compiler.

### 3.2 Lexer Behavior

The lexer scans for `${` sequences. If found, the token type is `InterpolatedString` instead of `String`. The parser then splits the string into literal parts and expression parts for code generation.

### 3.3 Validation

Interpolated expressions are validated at the checker stage. Syntax errors or type errors inside `${...}` are reported with correct file:line location and actionable hints.

### 3.4 Escaping

Use `\$` to include a literal `$` without triggering interpolation:

```almide
let price = "costs \${amount}"   -- "costs ${amount}" literally
```

---

## 4. String Concatenation

```almide
let full = first ++ " " ++ last
let items = [1, 2] ++ [3, 4]       -- ++ also works for lists
```

The `++` operator concatenates strings. It is also used for list concatenation. This is a design choice to avoid overloading `+` (which is arithmetic-only in Almide).

---

## 5. Heredoc (Multi-line Strings)

### 5.1 Basic Syntax

```almide
let query = """
  SELECT *
  FROM users
  WHERE active = true
"""
```

Heredocs are delimited by `"""` on both ends. The implementation is entirely in the lexer — no AST, parser, or emitter changes are needed.

### 5.2 Whitespace Handling

- The first newline immediately after the opening `"""` is skipped.
- Leading whitespace is stripped based on the minimum indentation of non-empty lines (Kotlin `trimIndent` semantics).

This means the content is aligned to the least-indented line:

```almide
let text = """
    line one
      line two (extra indent preserved)
    line three
"""
-- Result:
-- "line one\n  line two (extra indent preserved)\nline three"
```

### 5.3 Heredoc with Interpolation

Interpolation works identically to regular strings:

```almide
let user_id = 42
let query = """
  SELECT *
  FROM users
  WHERE id = ${user_id}
"""
```

### 5.4 Raw Heredoc

```almide
let raw = r"""
  no ${interpolation} here
  no \n escaping either
"""
```

Prefixed with `r`. No escape processing, no interpolation. Whitespace trimming still applies.

### 5.5 Escape Sequences in Heredocs

Non-raw heredocs support the same escape sequences as regular strings: `\n`, `\t`, `\\`, `\"`, `\$`.

---

## 6. Code Generation

### 6.1 Rust Target

- Regular strings: `String::from("...")`
- Interpolation: `format!("... {} ...", expr)`
- Concatenation (`++`): `format!("{}{}", left, right)`

### 6.2 TypeScript Target

- Regular strings: `"..."`
- Interpolation: template literals `` `... ${expr} ...` ``
- Concatenation (`++`): `left + right`
