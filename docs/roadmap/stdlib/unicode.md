<!-- description: Unicode character classification and case conversion -->
# stdlib: unicode [Tier 3]

Unicode 文字分類。Go, Python, Rust に標準で存在。

## 他言語比較

| 操作 | Go (`unicode`) | Python (`unicodedata`) | Rust (`std::char`) |
|------|---------------|----------------------|-------------------|
| カテゴリ | `unicode.IsLetter(c)` | `unicodedata.category(c)` | `c.is_alphabetic()` |
| 数字 | `unicode.IsDigit(c)` | `c.isdigit()` | `c.is_numeric()` |
| 空白 | `unicode.IsSpace(c)` | `c.isspace()` | `c.is_whitespace()` |
| 大文字 | `unicode.IsUpper(c)` | `c.isupper()` | `c.is_uppercase()` |
| 小文字 | `unicode.IsLower(c)` | `c.islower()` | `c.is_lowercase()` |
| 大文字変換 | `unicode.ToUpper(c)` | `c.upper()` | `c.to_uppercase()` |
| 小文字変換 | `unicode.ToLower(c)` | `c.lower()` | `c.to_lowercase()` |
| コードポイント | `int(c)` | `ord(c)` | `c as u32` |
| 文字名 | ❌ | `unicodedata.name(c)` | ❌ |

## 追加候補 (~8 関数)

- `string.is_alpha?(s) -> Bool` — 全文字がアルファベットか
- `string.is_digit?(s) -> Bool` — 全文字が数字か
- `string.is_whitespace?(s) -> Bool` — 全文字が空白か
- `string.is_upper?(s) -> Bool` — 全文字が大文字か
- `string.is_lower?(s) -> Bool` — 全文字が小文字か
- `string.char_at(s, index) -> Option[String]` — 指定位置の文字
- `string.codepoint(s, index) -> Option[Int]` — コードポイント
- `string.from_codepoint(n) -> String` — コードポイントから文字

## 実装戦略

string モジュールの拡張として TOML + runtime で追加。独立モジュールにする必要はない。
