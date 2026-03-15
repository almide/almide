# CLI-First: Almide で CLI ツールを快適に書ける状態を作る [ACTIVE]

## Vision

Almide で実用的な CLI ツールを書き、開発時は `almide run` で即実行、配布時は `almide build` で単一ネイティブバイナリを生成できる。同じコードが TS パスでも Rust パスでも動くことを @extern + glue が保証する。

```
開発:  almide run app.almd           → TS → Deno 即実行（go run 相当）
配布:  almide build app.almd -o app  → Rust → 単一ネイティブバイナリ（go build 相当）
WASM:  almide build app.almd --target wasm → Rust → .wasm
```

---

## 現状の実力（v0.5.13）

### 書ける CLI ツール

| カテゴリ | 使える機能 | 具体例 |
|---|---|---|
| **引数解析** | `args.positional()`, `args.flag()`, `args.option()` | `mytool --verbose -o out.json input.csv` |
| **ファイル処理** | `fs.read_text/write/read_lines/glob/walk` | ファイル変換、ログ集約、静的サイト生成 |
| **構造化データ** | `json.parse/stringify`, `csv.parse_with_header`, `toml.parse` | JSON/CSV/TOML 変換ツール |
| **パス操作** | `path.join/dirname/basename/extension/normalize` | クロスプラットフォームパス処理 |
| **正規表現** | `regex.is_match/find_all/replace/captures` | テキスト検索・置換ツール |
| **プロセス実行** | `process.exec/exec_status/exec_with_stdin` | ビルドスクリプト、タスクランナー |
| **HTTP** | `http.get/post/get_json`, `http.serve` | API クライアント、Webhook サーバー |
| **環境** | `env.get/cwd/os`, `process.exit` | 環境依存の分岐、終了コード |
| **エラー処理** | `Result[T, E]`, `effect fn`, `do` block | ファイル不在、パース失敗の適切な報告 |

**実証済み**: exercises/ に config-merger (317行)、pipeline、isbn-verifier 等の CLI 的プログラムが動作。

### 書けない CLI ツール

| 不足 | 影響 | 解決手段 |
|---|---|---|
| **ターミナル装飾** | 色付き出力、進捗バー、スピナーがない | `term` モジュール拡充（ANSI エスケープ） |
| **async / 並行処理** | 複数ファイル並列処理、複数 API 同時呼出しができない | structured concurrency (既存 roadmap) |
| **インタラクティブ入力** | Y/N 確認、メニュー選択ができない | `io.prompt()`, `io.confirm()` 追加 |
| **パッケージ依存** | 外部ライブラリを使えない | パッケージレジストリ（後回し可） |
| **DB 接続** | SQLite/PostgreSQL を直接使えない | @extern で Rust crate をラップ |
| **シグナル処理** | Ctrl+C のハンドリングができない | `process.on_signal()` 追加 |

---

## ゴール: 5つの CLI ツールが書ける状態

以下の CLI ツールが Almide で自然に書けることをゴールとする。

### 1. ファイル変換ツール（今すぐ書ける）

```almide
// csv2json: CSV → JSON 変換
effect fn main() =
  let args = args.positional()
  let input = args.get(0).unwrap_or("input.csv")
  let content = fs.read_text(input)
  let rows = csv.parse_with_header(content)
  let json_out = json.stringify_pretty(json.from(rows))
  println(json_out)
```

**ステータス: 今すぐ書ける ✅**

### 2. プロジェクト初期化ツール（ほぼ書ける）

```almide
// init: ディレクトリ構造を作成
effect fn main() =
  let name = args.option_or("name", "my-project")
  fs.mkdir_p("{name}/src")
  fs.mkdir_p("{name}/tests")
  fs.write("{name}/almide.toml", "[project]\nname = \"{name}\"\nversion = \"0.1.0\"")
  fs.write("{name}/src/main.almd", "fn main() =\n  println(\"Hello, {name}!\")")
  println("Created project: {name}")
```

**ステータス: 今すぐ書ける ✅**

### 3. API クライアント（async が要る）

```almide
// ghstats: GitHub API から統計を取得
effect fn main() = do {
  let token = env.get("GITHUB_TOKEN").unwrap_or("")
  let repos = args.positional()
  // 全リポジトリの情報を並列取得したい
  async let results = repos.map((repo) =>
    http.get_json("https://api.github.com/repos/{repo}")
  )
  for result in await results {
    let name = json.get_string(result, "full_name")
    let stars = json.get_int(result, "stargazers_count")
    println("{name}: {stars} stars")
  }
}
```

**ステータス: async が必要 🔶** — sync の逐次実行なら今でも書ける

### 4. ファイル検索ツール（ほぼ書ける）

```almide
// find: パターンでファイルを検索し、中身をgrep
effect fn main() = do {
  let pattern = args.positional().get(0).unwrap_or("*.almd")
  let query = args.option("grep")
  let files = fs.glob(pattern)
  for file in files {
    match query {
      some(q) => {
        let content = fs.read_text(file)
        let lines = string.lines(content)
        for (i, line) in lines.enumerate() {
          if string.contains(line, q) {
            println("{file}:{i + 1}: {line}")
          }
        }
      }
      none => println(file)
    }
  }
}
```

**ステータス: ほぼ書ける ✅** — `enumerate` があれば完全（なければ手動カウンタで代用可）

### 5. ビルドスクリプト / タスクランナー（ターミナル装飾が欲しい）

```almide
// tasks.almd: プロジェクトのタスクを定義・実行
effect fn main() = do {
  let task = args.positional().get(0).unwrap_or("help")
  match task {
    "build" => {
      println("[build] Compiling...")     // 色付きにしたい
      let result = process.exec("cargo", ["build", "--release"])
      println("[build] Done: {result}")
    }
    "test" => {
      let result = process.exec_status("almide", ["test"])
      process.exit(result.code)
    }
    "clean" => {
      fs.remove_all("target")
      println("[clean] Removed target/")
    }
    _ => {
      println("Available tasks: build, test, clean")
    }
  }
}
```

**ステータス: 書ける（色なし）✅** — term モジュール拡充で色付きにしたい

---

## ギャップ分析と優先度

### Must Have（CLI ゴール達成に必須）

| 機能 | 今の状態 | やること | 既存 roadmap |
|---|---|---|---|
| **@extern + glue** | 設計完了 | 実装 Step 1〜3 | [Runtime Architecture Reform](stdlib-self-hosted-redesign.md) |
| **? suffix 廃止** | ✅ 完了 | — | [API Surface Reform](stdlib-verb-system.md) Step 1 |
| **Result の TS 統一表現** | 設計完了 (glue) | glue runtime 実装 | [Runtime Architecture Reform](stdlib-self-hosted-redesign.md) |

### Should Have（CLI 体験を良くする）

| 機能 | 今の状態 | やること | 既存 roadmap |
|---|---|---|---|
| **ターミナル色/スタイル** | `term` モジュール最小 | ANSI エスケープラッパー追加 | なし（新規） |
| **async / parallel** | 設計完了 | Phase 0〜1 実装 | [Platform Async](platform-async.md), [Structured Concurrency](structured-concurrency.md) |
| **インタラクティブ入力** | `io.read_line` のみ | `io.prompt()`, `io.confirm()` 追加 | なし（新規） |
| **Verb System** | 設計完了 | Step 2〜5 | [API Surface Reform](stdlib-verb-system.md) |

### Nice to Have（CLI 以降のユースケースに効く）

| 機能 | やること | 既存 roadmap |
|---|---|---|
| DB 接続 | @extern で SQLite/PostgreSQL ラップ | なし |
| シグナル処理 | `process.on_signal("SIGINT", handler)` | なし |
| パッケージレジストリ | 外部依存の解決 | on-hold |

---

## 開発 / 配布モデル

### 開発時: TS パスで高速イテレーション

```bash
almide run app.almd              # TS → Deno 即実行
almide run app.almd arg1 arg2    # 引数も渡せる（既存機能）
```

- Rust コンパイル不要、即実行
- async は JS の async/await にそのまま写る（検証が楽）
- エラーメッセージも即座にフィードバック

### 配布時: ネイティブバイナリ

```bash
almide build app.almd -o mytool          # 単一バイナリ
almide build app.almd --target wasm      # WASM
```

- `./mytool` で動く。依存ランタイムなし
- Go / Rust の CLI と同じ配布体験
- async は tokio が吸収（ユーザーは意識しない）

### async の検証戦略

**TS パスで先に検証、Rust パスは後から**:

```
Almide async let → JS Promise.all    # ほぼ 1:1 で写る。先にここで意味論を固める
Almide async let → tokio::spawn      # 意味論が固まってから Rust に変換
```

TS パスの方がコンパイラの仕事が軽い（JS が async を持っているので構文変換だけ）。Rust パスは tokio の Send + 'static 制約等の難しさがあるので、意味論が固まってから取り組む方が安全。

---

## Implementation Steps

### Step 1: ショーケース CLI ツール（今すぐ）

既存機能だけで書ける CLI ツールを 2〜3 本作り、exercises/ または examples/ に置く。「Almide で CLI が書ける」ことを実証する。

候補:
- `csv2json` — CSV → JSON 変換（args + fs + csv + json）
- `project-init` — プロジェクト初期化（args + fs + path）
- `grep-lite` — ファイル内テキスト検索（args + fs + glob + string/regex）

### Step 2: ターミナル装飾（term モジュール拡充）

ANSI エスケープコードのラッパーを `stdlib/term.almd` に追加:

```almide
term.red("Error: file not found")
term.green("✓ Done")
term.bold("Building...")
term.dim("(3 files)")
```

純粋 Almide で実装可能（ANSI エスケープは文字列操作）。@extern 不要。

### Step 3: インタラクティブ入力

```almide
let name = io.prompt("Project name: ")
let confirm = io.confirm("Create {name}?")   // Y/n
```

`io.read_line` の上に .almd で構築可能。

### Step 4: async（既存 roadmap に従う）

[Platform Async](platform-async.md) / [Structured Concurrency](structured-concurrency.md) の Phase 0〜1 を CLI 文脈で実装。

検証ツール: `ghstats`（複数 API 並列呼出し）を TS パスで動かす。

### Step 5: @extern + glue（既存 roadmap に従う）

[Runtime Architecture Reform](stdlib-self-hosted-redesign.md) の Step 1〜3 を実装。CLI ツールの TS/Rust 両パスでの動作を保証。

---

## Success Criteria

- exercises/ に 3 本以上の実用 CLI ツールが動作する
- `almide run tool.almd` で TS パス即実行できる
- `almide build tool.almd -o tool` でネイティブバイナリが出る
- 同じ .almd ファイルが TS パスでも Rust パスでも同じ結果を返す
- ターミナルに色付き出力ができる
- `io.prompt()` / `io.confirm()` でインタラクティブ入力ができる

## Related

- [Runtime Architecture Reform](stdlib-self-hosted-redesign.md) — @extern + glue
- [API Surface Reform](stdlib-verb-system.md) — 動詞体系
- [Platform Async](platform-async.md) — 透過的 async
- [Structured Concurrency](structured-concurrency.md) — async let
