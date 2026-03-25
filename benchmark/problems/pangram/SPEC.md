# Pangram

**Level**: 1 (Easy)

## Description

Determine if a sentence is a pangram. A pangram is a sentence using every letter of the alphabet at least once.

The check is case-insensitive. Non-letter characters (digits, punctuation, whitespace) are ignored.

## Function Signature

```
is_pangram(sentence: String) -> Bool
```

## Examples

- `is_pangram("The quick brown fox jumps over the lazy dog")` -> `true`
- `is_pangram("The quick brown fox")` -> `false`
- `is_pangram("")` -> `false`

## Test Cases

| Input | Expected |
|-------|----------|
| `""` | `false` |
| `"abcdefghijklmnopqrstuvwxyz"` | `true` |
| `"the quick brown fox jumps over the lazy dog"` | `true` |
| `"a quick movement of the enemy will jeopardize five gunboats"` | `false` (missing 'x') |
| `"five boxing wizards jump quickly at my request"` | `false` (missing 'h') |
| `"the_quick_brown_fox_jumps_over_the_lazy_dog"` | `true` |
| `"the 1 quick brown fox jumps over the 2 lazy dogs"` | `true` |
| `"7h3 qu1ck brown fox jumps ov3r 7h3 lazy dog"` | `false` |
| `'"Five quacking Zephyrs jolt my wax bed."'` | `true` |
| `"the quick brown fox jumps over with lazy FX"` | `false` |
