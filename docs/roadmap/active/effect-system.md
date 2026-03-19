# Effect System — Auto-Inferred Capabilities

**優先度:** 1.x (情報表示) → 2.x (制限適用)
**前提:** Effect Isolation Layer 1 完了済み
**原則:** ユーザーは `fn` / `effect fn` だけ書く。コンパイラが capability を自動推論。

> 「書くコードは変わらない。コンパイラが賢くなるだけ。」

---

## 動機

### 現状: 二値 effect

```almide
fn pure_func(x: Int) -> Int = x * 2           // 純粋
effect fn io_func() -> Result[String, String] = {  // I/O
  fs.read_text("file.txt")
}
```

Layer 1 (Effect Isolation) により `fn` から `effect fn` は呼べない。これは動作する。

### 問題: 粒度が粗い

`effect fn` は「何かしらの副作用がある」としか言えない。ファイル操作もネットワークもログも全て同じ `effect fn`。

- 依存パッケージが「Net だけ使う」と言っているのに、実は fs も触れてしまう
- `almide check` で「この関数はどの capability を使うか」が分からない
- Security Layer 2-3 (Capability restriction, Package boundary) が実装できない

### 解決: 自動推論 + 境界制限

ユーザーのコードは一切変更なし。コンパイラが stdlib の呼び出しから capability を推論。

---

## Effect Categories

stdlib モジュールから自動マッピング:

| Effect | Modules | 説明 |
|--------|---------|------|
| `IO` | fs, path | ファイルシステム |
| `Net` | http, url | ネットワーク |
| `Env` | env, process | 環境変数・プロセス |
| `Time` | time, datetime | 時刻取得 |
| `Rand` | math.random | 乱数生成 |
| `Fan` | fan | 並行処理 |
| `Log` | log | ログ出力 |

### 推移的推論

```almide
effect fn download(url: String) -> Result[String, String] = {
  http.get(url)                    // → {Net}
}

effect fn download_and_save(url: String) -> Result[Unit, String] = {
  let data = download(url)         // → {Net} (推移的)
  fs.write_text("out.txt", data)   // → {IO}
}
// download_and_save の推論結果: {Net, IO}
```

---

## Phase 1: Effect 推論エンジン (1.x)

### 実装

IR 分析パスとして実装。型システムの変更なし。

```rust
// src/codegen/pass_effect_inference.rs

struct EffectInferencePass;

impl NanoPass for EffectInferencePass {
    fn name(&self) -> &str { "EffectInference" }
    fn targets(&self) -> Option<Vec<Target>> { None }

    fn run(&self, program: &mut IrProgram, _target: Target) {
        let mut effect_map: HashMap<FuncId, EffectSet> = HashMap::new();

        // 1. 直接 stdlib 呼び出しから seed
        for func in &program.functions {
            let effects = infer_direct_effects(&func.body);
            effect_map.insert(func.id, effects);
        }

        // 2. 推移的閉包 (fixpoint)
        loop {
            let mut changed = false;
            for func in &program.functions {
                let callee_effects = collect_callee_effects(&func.body, &effect_map);
                if effect_map.get(&func.id).map_or(true, |e| !e.is_superset(&callee_effects)) {
                    effect_map.entry(func.id).or_default().extend(&callee_effects);
                    changed = true;
                }
            }
            if !changed { break; }
        }

        program.effect_map = effect_map;
    }
}
```

### `almide check --effects`

```
$ almide check --effects src/server.almd

src/server.almd:
  fn handle_request    → {Net, IO, Log}
  fn parse_config      → {IO}
  fn validate_input    → {} (pure)
  fn format_response   → {} (pure)
```

情報表示のみ。制限は適用しない。

---

## Phase 2: Self-package 制限 (1.x)

### almide.toml

```toml
[package]
name = "my-api"
version = "1.0.0"
edition = "2026"

[permissions]
allow = ["Net", "IO", "Log"]  # このパッケージが使える capability
```

`permissions` を宣言すると、パッケージ内のコードがその capability 以外を使おうとするとコンパイルエラー:

```
error[E011]: capability violation
  --> src/handler.almd:15:3
   |
15 |   process.exec("rm -rf /")
   |   ^^^^^^^^^^^^^^^^^^^^^^^^
   |
   = package `my-api` is restricted to [Net, IO, Log]
   = `process.exec` requires [Env] capability
   hint: add `Env` to [permissions].allow in almide.toml
```

---

## Phase 3: Dependency 制限 (2.x)

### almide.toml

```toml
[dependencies.markdown-lib]
git = "https://github.com/example/markdown-lib"
allow = []  # pure only — no effects allowed

[dependencies.api-client]
git = "https://github.com/example/api-client"
allow = ["Net"]  # network only, no filesystem

[dependencies.logger]
git = "https://github.com/example/logger"
allow = ["Log", "IO"]
```

依存パッケージが `allow` で許可されていない capability を使うとコンパイルエラー:

```
error[E012]: dependency capability violation
  --> api-client@1.2.0/src/cache.almd:42:5
   |
42 |     fs.write_text(cache_path, data)
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
   = dependency `api-client` is restricted to [Net] by consumer
   = `fs.write_text` requires [IO] capability
   hint: add `IO` to [dependencies.api-client].allow in almide.toml
         or ask the library author to remove file system usage
```

### Security Layer 2-3 の実現

```
Layer 1: Effect Isolation        ← 完了
  fn は effect fn を呼べない

Layer 2: Capability Restriction  ← Phase 2-3
  パッケージが使える capability を宣言・制限

Layer 3: Package Boundary        ← Phase 3
  依存パッケージの capability を消費者が制限
```

---

## vibe-lang との差別化

| | vibe-lang | Almide |
|---|-----------|--------|
| ユーザー構文 | `with {Async, Error}` 明示 | `effect fn` のみ — 推論 |
| LLM 負荷 | effect 選択が必要 | 変更なし |
| 制限単位 | 関数レベル | パッケージ境界 |
| 新キーワード | `with`, `handle` | なし |
| 破壊的変更 | — | なし (additive) |

Almide の advantage: **ユーザーのコードは一切変わらない。** 制限はインフラ層 (`almide.toml`) で行う。LLM にとって `fn` / `effect fn` の二択のまま。

---

## HKT Foundation との連携

Phase 4 (hkt-foundation.md) で Effect set を型レベル表現に昇格:

```
// Phase 1-3: EffectSet は HashMap<FuncId, HashSet<Effect>>
//            型システムとは独立した分析パス

// Phase 4 (HKT Foundation): Effect を型に統合
//   FnType { params, ret, effects: Ty::Applied(TC_EFFECT, [...]) }
//   Trait system と連携し、代数的に effect を合成・制限
```

---

## タイムライン

```
Phase 1: Effect 推論エンジン         ← 1.x
  IR 分析パス (Nanopass)
  almide check --effects (情報表示)

Phase 2: Self-package 制限           ← 1.x
  almide.toml [permissions]
  コンパイルエラーで違反検出

Phase 3: Dependency 制限             ← 2.x
  almide.toml [dependencies.X].allow
  Security Layer 2-3 実現

Phase 4: 型レベル統合                ← 2.x (HKT Foundation Phase 4)
  Effect を型コンストラクタに
  Trait system と統合
```

---

## 一文で

> stdlib 呼び出しから capability を自動推論し、パッケージ境界で制限する。ユーザーは `effect fn` だけ書く。
