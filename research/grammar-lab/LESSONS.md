# Grammar Lab: 開発で学んだこと

Runner を Almide で書く過程でぶつかった問題と解決策の記録。Almide の実用上の知見。

---

## 1. stdlib API の名前

LLM（自分含む）が間違えやすい API 名:

| 書いてしまう | 正しい API | 理由 |
|---|---|---|
| `io.println(x)` | `io.print(x ++ "\n")` | `println` は存在しない |
| `args.collect()` | `env.args()` | `args` モジュールは存在しない。`env` モジュール |
| `time.now()` | `env.unix_timestamp()` | `time` モジュールは存在しない。`env` にある |
| `fs.exists(path)` | `fs.exists?(path)` | `?` 付き |
| `fs.create_dir(dir)` | `fs.mkdir_p(dir)` | POSIX 準拠の命名 |
| `process.run(cmd, args)` | `process.exec_status(cmd, args)` | `run` は存在しない |

**教訓:** Almide の stdlib 命名は他言語と微妙に違う。LLM が間違えやすいポイントは Language Context の Layer 1 に入れるべき。

---

## 2. effect fn 内の auto-unwrap

effect fn 内で `Result` を返す関数を呼ぶと、自動的に `?` 相当の unwrap が起きる。

```almide
// effect fn 内
let config = json.parse(config_text)  // Result[Json, String] → Json に auto-unwrap
json.get_string(config, "name")       // config は Json 型として使える
```

**ハマったパターン:** `match` で手動 unwrap すると、変数の型が `Result` のままになって後続のコードで型不一致になる。

```almide
// ❌ 冗長で型が合わなくなる
let config = match json.parse(config_text) {
  ok(j) => j,
  err(e) => err("Failed: ${e}"),
}
// config の型が曖昧になる

// ✅ effect fn 内なら直接代入
let config = json.parse(config_text)
// auto-unwrap で config は Json 型
```

---

## 3. effect fn の match 分岐と型の統一

effect fn 内で `match` や `if` の分岐が `Result` 型と非 `Result` 型を混ぜると codegen エラーになる。

```almide
// ❌ コンパイルは通るが Rust codegen で失敗
effect fn call_llm(provider: String, ...) -> Result[String, String] = {
  match provider {
    "anthropic" => call_anthropic(...),  // auto-unwrap → String
    _ => err("Unknown"),                  // Result[String, String]
  }
}
```

**解決:** `guard` で早期 return してから分岐。

```almide
// ✅
effect fn call_llm(provider: String, ...) -> Result[String, String] = {
  guard provider == "anthropic" or provider == "openai" else err("Unknown provider")
  if provider == "anthropic" then {
    ok(call_anthropic(...))
  } else {
    ok(call_openai(...))
  }
}
```

---

## 4. nested loop での ownership move (Rust codegen)

4 重の `for` ループ（models × variants × tasks × trials）で、外側ループの変数を内側で `json.object` に渡すと Rust の ownership move が起きる。

```almide
// ❌ Rust codegen で move error
for model in models {
  for variant in variants {
    for task in tasks {
      for trial in 1..=n {
        json.object([("model", json.s(model_name)), ...])
        // ↑ model_name が move されて次の trial で使えない
      }
    }
  }
}
```

**解決:** json 構築を別関数に分離して、引数として渡す（Almide の borrow inference が関数境界で clone を挿入する）。

```almide
fn make_result_json(experiment: String, variant: String, ...) -> Json =
  json.object([("experiment", json.s(experiment)), ...])

// ループ内
let result = make_result_json(experiment_name, variant_name, ...)
```

---

## 5. `process.exec_status` の戻り値

`process.exec_status` は `Result[{code: Int, stdout: String, stderr: String}, String]` を返す。

- effect fn 内では auto-unwrap されて `{code: Int, stdout: String, stderr: String}` になる
- `match ok(r) => ... / err(e) => ...` で手動 unwrap すると codegen エラー

```almide
// ❌
let result = process.exec_status(cmd, args)
match result {
  ok(r) => r.code,    // Rust codegen: expected record, found Result
  err(e) => ...,
}

// ✅ effect fn 内なら直接フィールドアクセス
let r = process.exec_status(cmd, args)
if r.code != 0 then { ... } else { ... }
```

---

## 6. `println` がない

`io.print` しかない。改行付き出力は自分で書く。

```almide
effect fn println(s: String) -> Unit = io.print(s ++ "\n")
```

`fn` ではなく `effect fn` にしないとコンパイルエラー（`io.print` が effect function だから）。

---

## 7. `json.parse` の auto-unwrap と後続の `json.get_*`

`json.parse` は `Result[Json, String]`。effect fn 内で auto-unwrap されると `Json` になる。しかし `json.get_*` 系も `Result` を返す場合がある（`json.parse` の結果を直接渡す文脈で）。

テストコード（`json_test.almd`）を見ると:
```almide
let obj = json.parse("{\"n\": 42}")
assert_eq(json.get_int(obj, "n").unwrap_or(0), 42)
```

test block 内でも同様の auto-unwrap が効く。

---

## 8. `string.to_int` は `Result[Int, String]`

pure fn 内で使う場合は `match` で unwrap が必要。

```almide
// ❌ pure fn 内では auto-unwrap されない
fn parse_int(s: String, default: Int) -> Int =
  if s == "" then default else string.to_int(s)

// ✅
fn parse_int(s: String, default: Int) -> Int =
  if s == "" then { default }
  else { match string.to_int(s) { ok(n) => n, err(_) => default } }
```

---

## 9. `env.args()` は argv[0] を含む

`env.args()` は Rust の `std::env::args()` そのままなので、最初の要素はバイナリパス。

```almide
let raw_args = env.args()         // ["/tmp/mybin", "arg1", "arg2"]
let args = list.drop(raw_args, 1) // ["arg1", "arg2"]
```

`almide run` 経由だと argv[0] が一時バイナリのパスになるので、知らずに使うとパスが壊れる。

---

## 10. effect fn main の Err が silent exit(1) になる

`effect fn main() -> Result[Unit, String]` が `err(msg)` を返すと、エラーメッセージが出ずに exit code 1 だけで終了する。

**原因:** codegen が `let _ = e; std::process::exit(1);` を生成している（`src/emit_rust/program.rs:258`）。

**回避策:** main で match して手動で出力。

```almide
effect fn main() -> Result[Unit, String] = {
  match run_experiment() {
    ok(_) => ok(()),
    err(e) => {
      println("Error: ${e}")
      process.exit(1)
      ok(())
    },
  }
}
```

**TODO:** コンパイラ側で `eprintln!("{}", e)` を生成すべき。

---

## 11. `string.pad_right` が ownership を取る (codegen)

Rust codegen で `almide_rt_string_pad_right(s: String, ...)` が `s` の ownership を取るため、同じ変数を後で使うとmoveエラー。

**回避策:** 変数を使う順番を工夫するか、先に clone 相当の操作（`list.map` で中間結果を作る等）を行う。

---

## 全体の教訓

1. **effect fn の auto-unwrap に頼る** — 手動 `match ok/err` は型の不一致を生みやすい
2. **nested loop で json 構築するなら関数分離** — ownership move を回避
3. **stdlib の正確な API 名を確認する** — LLM は他言語の癖で間違える。これ自体が Grammar Lab の Layer 1 に反映すべき知見
4. **codegen エラーは Almide の checker を通った後に出る** — checker pass ≠ Rust compile pass。特に ownership 周り
