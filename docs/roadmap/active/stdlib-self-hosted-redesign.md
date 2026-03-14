# Stdlib Self-Hosted Redesign (Zig方式) [ACTIVE]

## Summary

stdlib の定義方式を TOML + build.rs コード生成から、**stdlib を「ただの .almd ファイル」として書く** Zig 方式に移行する。コンパイラが提供するのは ~60 の `@builtin` プリミティブのみ。残り ~280 関数は純粋な Almide コードとして実装し、既存のマルチターゲット codegen がそのまま Rust/TS 両方に出力する。

## Motivation

### 現状の問題

1. **TOML 二重管理**: 1関数につき Rust テンプレート + TS テンプレートの2つを書く
2. **build.rs の複雑さ**: TOML → Rust コード生成器が 1000 行超、デバッグ困難
3. **src/generated/ の不透明さ**: 生成コードが 3 ファイル、手動編集禁止だが理解も困難
4. **ユーザーが同じ機構を使えない**: stdlib だけ特別扱いで、ユーザーは UFCS 拡張できない

### Zig 方式で解決

- `@import("std")` はただのパス解決 → Almide でも `import stdlib/list` はただの .almd ファイル読み込み
- stdlib は特別扱いしない → ユーザーコードと同じ型チェック・codegen パイプラインを通る
- コンパイラ魔法を最小化 → stdlib の進化がコンパイラに縛られない

### 既に動いている証拠

以下のモジュールは既に純粋 Almide で実装済み:
- `hash.almd` — SHA-256/SHA-1/MD5 をビット演算だけで実装 (189行)
- `csv.almd` — ステートマシン CSV パーサー (70行)
- `url.almd` — RFC 3986 準拠 URL パーサー (220行)
- `path.almd` — パス正規化、結合、分解
- `encoding.almd` — base64/hex エンコード・デコード
- `args.almd` — CLI 引数パーサー

## Design

### Architecture

```
コンパイラ (Rust)
├── @builtin 関数テーブル (~60 関数)
│   ├── Tier 1: 型プリミティブ (~10)     string.len, list.get, int.to_string
│   ├── Tier 2: クロージャ最適化 (~20)   list.map, filter, sort_by
│   └── Tier 3: プラットフォーム (~30)   fs.read_text, http.get, io.print
└── 通常の .almd コンパイルパイプライン

stdlib/ (純粋 .almd ファイル)
├── string.almd      trim, split, replace, capitalize...
├── list.almd        contains, reverse, take, drop, zip...
├── int.almd         abs, clamp, to_hex, sign...
├── float.almd       round, ceil, floor...
├── math.almd        pow, sqrt, log, sin, cos...
├── map.almd         merge, filter_keys, map_values...
├── result.almd      map, flat_map, unwrap_or...
├── option.almd      map, unwrap_or, is_some...
├── json.almd        get_path, set_path, query...
├── hash.almd        sha256, sha1, md5           ← 既存
├── csv.almd         parse, stringify             ← 既存
├── url.almd         parse, build                 ← 既存
├── path.almd        join, normalize, ext         ← 既存
├── encoding.almd    base64, hex                  ← 既存
├── args.almd        parse, flag, positional      ← 既存
└── term.almd        color, style                 ← 既存
```

### @builtin の設計

言語構文として `@builtin` アトリビュートを追加:

```almide
// stdlib/list.almd

// @builtin: コンパイラがターゲット別に最適な実装を挿入
@builtin
fn map[A, B](xs: List[A], f: Fn(A) -> B) -> List[B]

@builtin
fn filter[A](xs: List[A], f: Fn(A) -> Bool) -> List[A]

// 純粋 Almide: コンパイラは普通にコンパイルする
fn contains[A](xs: List[A], value: A) -> Bool {
  for x in xs {
    if x == value { return true }
  }
  false
}

fn reverse[A](xs: List[A]) -> List[A] {
  var result: List[A] = []
  let n = xs.len()
  var i = n - 1
  do i >= 0 {
    result = result ++ [xs[i]]
    i = i - 1
  }
  result
}
```

### @builtin のコンパイラ側実装

```
パーサー: @builtin アトリビュート付き fn 宣言を認識
チェッカー: 型シグネチャを普通に型チェック
Lower:   @builtin → IrFunction { body: Builtin("list.map") }
Codegen: Builtin("list.map") → ターゲット別コード出力
         Rust: almide_rt_list_map(...)
         TS:   __almd_list.map(...)
```

コンパイラ内部のビルトインテーブル:

```rust
// src/builtin.rs (~200 行)
fn emit_builtin_rust(name: &str, args: &[String]) -> String {
    match name {
        "list.map" => format!("almide_rt_list_map({}.to_vec(), |{}| {{ {} }})", ...),
        "fs.read_text" => format!("std::fs::read_to_string(&*{})?", ...),
        "io.print" => format!("println!(\"{{}}\", {})", ...),
        ...
    }
}
```

### 移行で消えるもの

| 消えるもの | 行数 | 代替 |
|---|---|---|
| `stdlib/defs/*.toml` (14 ファイル) | ~2000 行 | stdlib/*.almd |
| `build.rs` の stdlib 生成部分 | ~1000 行 | なし |
| `src/generated/stdlib_sigs.rs` | ~800 行 | パーサーが直接読む |
| `src/generated/emit_rust_calls.rs` | ~1200 行 | src/builtin.rs (~200 行) |
| `src/generated/emit_ts_calls.rs` | ~600 行 | src/builtin.rs に統合 |
| **合計** | **~5600 行** | **~200 行** |

## Phases

### Phase 0: @builtin 構文の追加
- パーサーに `@builtin` アトリビュートを追加
- チェッカーで @builtin fn の型シグネチャを処理
- Lower で `IrFunction::Builtin(name)` を生成
- `src/builtin.rs` に Rust/TS のエミッタを実装
- テスト: 1 関数（`io.print`）を @builtin で動かす

### Phase 1: プラットフォームモジュールの移行
- fs, http, io, env, process, random → @builtin + .almd ラッパー
- これらは全関数が @builtin になる（OS アクセス必須）
- TOML 定義を削除、.almd に型シグネチャを移動
- 既存テストが全て通ることを確認

### Phase 2: 純粋ロジックモジュールの移行
- string, int, float, math, map, result → 純粋 .almd 実装
- ~95 関数を Almide で書き直す
- 型プリミティブ（string.len, list.get 等 ~10 関数）だけ @builtin
- パフォーマンス比較: Rust 直書きと .almd 経由で有意差がないことを確認

### Phase 3: クロージャ関数の移行
- list.map, filter, sort_by, flat_map 等 ~20 関数
- @builtin として残すか、コンパイラの最適化で .almd 化するか判断
- 判断基準: `for + append` パターンの最適化パスが Phase 時点で実装済みか
- 実装済みなら .almd 化、未実装なら @builtin に残す

### Phase 4: 生成コード撤去
- `stdlib/defs/*.toml` 全削除
- `build.rs` から stdlib 生成ロジックを削除
- `src/generated/` から stdlib 関連ファイルを削除
- `src/stdlib.rs` を簡素化（UFCS 解決は .almd のエクスポートから自動導出）

### Phase 5: ユーザー @builtin 開放 (future)
- ユーザーが自分のパッケージで `@builtin` を使えるようにする
- FFI 的な用途: Rust crate や npm パッケージを直接ラップ
- `@builtin` → `@extern` にリネームし、ターゲット指定を追加:
  ```almide
  @extern(rust, "chrono::Utc::now().to_rfc3339()")
  @extern(ts, "new Date().toISOString()")
  fn now_iso() -> String
  ```

## @builtin 全リスト (~60 関数)

### Tier 1: 型プリミティブ (10)
```
string: len, char_at, slice, from_chars
list:   len, get, push, set
int:    to_string, parse
```

### Tier 2: クロージャ最適化 (20)
```
list:   map, filter, find, any, all, each, sort_by, flat_map,
        filter_map, take_while, drop_while, reduce, group_by,
        fold, scan, zip_with, partition, count, find_index, update
```

### Tier 3: プラットフォーム (30)
```
fs:      read_text, read_bytes, write, write_bytes, append,
         mkdir_p, exists, remove, list_dir, is_dir, is_file,
         copy, rename, walk, stat, glob, temp_dir
http:    get, post, put, patch, delete, request
io:      print, read_line
env:     get, set, args, cwd, os
process: exec, exec_status, exit
random:  int, float, bytes, choice
json:    parse, stringify
regex:   new, is_match, find, find_all, replace, split, captures
```

## Success Criteria

- `almide test` が全テスト通過
- TOML 定義ファイル 0
- build.rs に stdlib 生成コードなし
- src/generated/ に stdlib 関連ファイルなし
- 新しい stdlib 関数の追加が .almd ファイル編集だけで完了

## Dependencies

- [IR Optimization Passes](ir-optimization.md) — Phase 3 の判断に影響（for+append 最適化）
- [Codegen Refinement](codegen-refinement.md) — .almd 生成コードの品質

## Supersedes

- [Stdlib Strategy](stdlib-strategy.md) の戦略 1 (TOML + ランタイム) と戦略 2 (@extern)
  - 戦略 3 (self-host) と戦略 4 (x/ パッケージ) は引き続き有効
- [Stdlib Architecture: 3-Layer Design](../on-hold/stdlib-architecture-3-layer-design.md) の内部実装方式

## Files
```
src/builtin.rs (new, ~200 lines)
src/parser/mod.rs (add @builtin parsing)
src/check/ (handle @builtin fn signatures)
src/lower.rs (emit Builtin IR nodes)
src/emit_rust/ (dispatch @builtin → Rust code)
src/emit_ts/ (dispatch @builtin → TS code)
stdlib/*.almd (rewrite from TOML definitions)
```
