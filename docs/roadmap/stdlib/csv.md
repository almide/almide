<!-- description: CSV parsing and generation with header support -->
# stdlib: csv [Tier 2]

CSV パース/生成。データ処理の基本フォーマット。

## 他言語比較

| 機能 | Go (`encoding/csv`) | Python (`csv`) | Rust (`csv` crate) | Deno (`@std/csv`) |
|------|--------------------|--------------------|------|------|
| パース | `csv.NewReader(r).ReadAll()` | `csv.reader(f)` | `Reader::from_reader(r)` | `parse(text)` |
| ヘッダ付き | manual | `csv.DictReader(f)` | `ReaderBuilder::new().has_headers(true)` | `parse(text, {skipFirstRow: true})` |
| 生成 | `csv.NewWriter(w).WriteAll(records)` | `csv.writer(f).writerows()` | `Writer::from_writer(w)` | `stringify(data)` |
| デリミタ | `reader.Comma = ';'` | `csv.reader(f, delimiter=';')` | `ReaderBuilder::new().delimiter(b';')` | `parse(text, {separator: ";"})` |
| クォート | automatic | `csv.QUOTE_ALL` etc. | automatic | automatic |
| エスケープ | RFC 4180 | configurable | RFC 4180 | RFC 4180 |

## 追加候補 (~8 関数)

### P0
- `csv.parse(text) -> List[List[String]]` — CSV → 2D リスト
- `csv.parse_with_header(text) -> List[Map[String, String]]` — ヘッダ付きパース
- `csv.stringify(rows) -> String` — 2D リスト → CSV
- `csv.stringify_with_header(header, rows) -> String` — ヘッダ付き生成

### P1
- `csv.parse_with_options(text, options) -> List[List[String]]` — デリミタ指定等
- `csv.parse_line(line) -> List[String]` — 1 行パース
- `csv.escape(field) -> String` — フィールドエスケープ
- `csv.unescape(field) -> String` — フィールドアンエスケープ

## 実装戦略

self-host (.almd)。RFC 4180 準拠の文字列パーサー。Pure 実装で両ターゲット自動対応。
パフォーマンスが問題になれば @extern で Rust `csv` crate にフォールバック。
