# value

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](value.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](value.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](value.zh-CN.md)



> 原文：[value.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/value.md)。非規範的な読者向け固定スナップショットです。自動生成・自動同期されません。署名・コード例・API 索引のコメントは原文を保持し、説明の訳を末尾に併記しています。計測・動作・例をこの翻訳作業で独立に再検証していません。

動的な `Value` **データモデル**のコンストラクタとアクセサ。自動 import されます。`json` module はこのモデル上の形式（`json.parse` / `json.stringify`）です。[json.md](../../../../stdlib/json.md) を参照してください。

### `value.as_string(v: Value) -> Result[String, String]`

Value から String を取り出します。Str でなければ err。

### `value.as_int(v: Value) -> Result[Int, String]`

Value から Int を取り出します。Int でなければ err。

### `value.as_float(v: Value) -> Result[Float, String]`

Value から Float を取り出し、Int payload は Float に拡張します。他の種類は err。

### `value.as_bool(v: Value) -> Result[Bool, String]`

Value から Bool を取り出します。Bool でなければ err。

### `value.as_array(v: Value) -> Result[List[Value], String]`

Value から List[Value] を取り出します。Array でなければ err。

### `value.string(s: String) -> Value`

String から Value を作ります。

### `value.int(n: Int) -> Value`

Int から Value を作ります。

### `value.float(f: Float) -> Value`

Float から Value を作ります。

### `value.bool(b: Bool) -> Value`

Bool から Value を作ります。

### `value.object(pairs: List[(String, Value)]) -> Value`

キーと値の組のリストから Value object を作ります。object が同じキーを二つ保持することはありません（C-401、#4112）。リストで重複したキーは最初の出現位置と最後の値を保持します（C-380）。そのため `value.get`、`value.as_object`、全 serializer は一つの entry を見ます。`json.stringify(value.object([("role", value.string("user")), ("role", value.string("admin"))]))` は `{"role":"admin"}` であり、`role` が二度ある文書にはなりません。

### `value.array(items: List[Value]) -> Value`

Value のリストから Value array を作ります。

### `value.null() -> Value`

null Value を作ります。

### `value.pick(v: Value, keys: List[String]) -> Value`

Object から指定キーを選び、それ以外を捨てます。object でない値にはどのキーもないため、選択結果は `{}` になります（C-401）。object でない値への allowlist は何も通しません。

### `value.omit(v: Value, keys: List[String]) -> Value`

Object から指定キーを削除します。

### `value.merge(a: Value, b: Value) -> Value`

二つの Object を統合します。b のキーが a のキーを上書きします。



<a id="signature-index-29-functions"></a>

## 固定スナップショットの API 索引 (29 関数)

```
// Wraps xs as an Array Value.
// @since 0.6.0 or earlier
value.array(xs: List[Value]) -> Value

// Array items; err if v is another kind.
// @since 0.6.0 or earlier
value.as_array(v: Value) -> Result[List[Value], String]

// Bool payload; err otherwise (no truthiness).
// @since 0.6.0 or earlier
value.as_bool(v: Value) -> Result[Bool, String]

// Float payload (Int widens); err otherwise.
// @since 0.6.0 or earlier
value.as_float(v: Value) -> Result[Float, String]

// Int payload; err on Float, never truncated.
// @since 0.6.0 or earlier
value.as_int(v: Value) -> Result[Int, String]

// Str payload; err otherwise (no coercion).
// @since 0.6.0 or earlier
value.as_string(v: Value) -> Result[String, String]

// Value holding the Bool b.
// @since 0.6.0 or earlier
value.bool(b: Bool) -> Value

// Value holding the Float f.
// @since 0.6.0 or earlier
value.float(f: Float) -> Value

// Value holding the Int n.
// @since 0.6.0 or earlier
value.int(n: Int) -> Value

// Shallow merge, b's keys win; b if not both objects.
// @since 0.6.0 or earlier
value.merge(a: Value, b: Value) -> Value

// The JSON null Value.
// @since 0.6.0 or earlier
value.null() -> Value

// Wraps the pairs xs as an Object; a repeated key keeps its first position, last value.
// @since 0.6.0 or earlier
value.object(xs: List[(String, Value)]) -> Value

// v minus the listed keys; non-objects as-is.
// @since 0.6.0 or earlier
value.omit(v: Value, keys: List[String]) -> Value

// Only the listed keys of v; a non-object picks to {}.
// @since 0.6.0 or earlier
value.pick(v: Value, keys: List[String]) -> Value

// Value holding the String s.
// @since 0.6.0 or earlier
value.string(s: String) -> Value

// Object pairs in order; err if v is another kind.
// @since unreleased
value.as_object(v: Value) -> Result[List[(String, Value)], String]

// Value at key; none if absent or v is not an Object.
// @since unreleased
value.get(v: Value, key: String) -> Option[Value]

// String at key; none if absent or another kind.
// @since unreleased
value.get_string(v: Value, key: String) -> Option[String]

// Int at key; none if absent or not an Int (a Float is never truncated).
// @since unreleased
value.get_int(v: Value, key: String) -> Option[Int]

// Float at key (an Int widens); none if absent or another kind.
// @since unreleased
value.get_float(v: Value, key: String) -> Option[Float]

// Bool at key; none if absent or another kind.
// @since unreleased
value.get_bool(v: Value, key: String) -> Option[Bool]

// Array items at key; none if absent or another kind.
// @since unreleased
value.get_array(v: Value, key: String) -> Option[List[Value]]

// Object pairs at key; none if absent or another kind.
// @since unreleased
value.get_object(v: Value, key: String) -> Option[List[(String, Value)]]

// The empty path: the whole value.
// @since unreleased
value.root() -> Path

// p extended by the object key name.
// @since unreleased
value.key(p: Path, name: String) -> Path

// p extended by the array index i; a negative i is out of range.
// @since unreleased
value.index(p: Path, i: Int) -> Path

// Value at p, or none if a step misses.
// @since unreleased
value.get_path(v: Value, p: Path) -> Option[Value]

// v with x at p; missing keys are created, a missed index leaves v as is; err when the result would nest past 1000.
// @since unreleased
value.set_path(v: Value, p: Path, x: Value) -> Result[Value, String]

// v minus the entry at p; unchanged if absent; null for the root.
// @since unreleased
value.remove_path(v: Value, p: Path) -> Value
```



## API 説明の訳（上記署名順）

- `value.array(xs: List[Value]) -> Value` — xs を Array Value に包みます。
- `value.as_array(v: Value) -> Result[List[Value], String]` — Array の要素。v が別種なら err。
- `value.as_bool(v: Value) -> Result[Bool, String]` — Bool payload。それ以外は err（truthiness なし）。
- `value.as_float(v: Value) -> Result[Float, String]` — Float payload（Int は拡張）。それ以外は err。
- `value.as_int(v: Value) -> Result[Int, String]` — Int payload。Float は err、切捨て変換しません。
- `value.as_string(v: Value) -> Result[String, String]` — Str payload。それ以外は err（型強制変換なし）。
- `value.bool(b: Bool) -> Value` — Bool b を保持する Value。
- `value.float(f: Float) -> Value` — Float f を保持する Value。
- `value.int(n: Int) -> Value` — Int n を保持する Value。
- `value.merge(a: Value, b: Value) -> Value` — 浅い merge。b のキーが優先。どちらか一方でも object でなければ b。
- `value.null() -> Value` — JSON null Value。
- `value.object(xs: List[(String, Value)]) -> Value` — pairs xs を Object に包みます。重複キーは最初の位置と最後の値を保持。
- `value.omit(v: Value, keys: List[String]) -> Value` — v から列挙したキーを除きます。object 以外はそのまま。
- `value.pick(v: Value, keys: List[String]) -> Value` — v の列挙したキーだけ。object でなければ {}。
- `value.string(s: String) -> Value` — String s を保持する Value。
- `value.as_object(v: Value) -> Result[List[(String, Value)], String]` — 順序通りの Object の組。v が別種なら err。
- `value.get(v: Value, key: String) -> Option[Value]` — key の Value。存在しないか v が Object でなければ none。
- `value.get_string(v: Value, key: String) -> Option[String]` — key の String。存在しないか別種なら none。
- `value.get_int(v: Value, key: String) -> Option[Int]` — key の Int。存在しないか Int でなければ none（Float の切捨て変換はしません）。
- `value.get_float(v: Value, key: String) -> Option[Float]` — key の Float（Int は拡張）。存在しないか別種なら none。
- `value.get_bool(v: Value, key: String) -> Option[Bool]` — key の Bool。存在しないか別種なら none。
- `value.get_array(v: Value, key: String) -> Option[List[Value]]` — key の Array 要素。存在しないか別種なら none。
- `value.get_object(v: Value, key: String) -> Option[List[(String, Value)]]` — key の Object の組。存在しないか別種なら none。
- `value.root() -> Path` — 空の path。値全体。
- `value.key(p: Path, name: String) -> Path` — p を object のキー name で延長します。
- `value.index(p: Path, i: Int) -> Path` — p を array の index i で延長します。負の i は範囲外。
- `value.get_path(v: Value, p: Path) -> Option[Value]` — p の Value。どこかの step が見つからなければ none。
- `value.set_path(v: Value, p: Path, x: Value) -> Result[Value, String]` — p に x を設定した v。不足キーは作成、index が見つからなければ v はそのまま。結果の入れ子が 1000 を超えるなら err。
- `value.remove_path(v: Value, p: Path) -> Value` — v から p の entry を除きます。不在なら変更なし。root なら null。
