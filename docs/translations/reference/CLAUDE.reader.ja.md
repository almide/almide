<a id="mission"></a>

# ミッション

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](CLAUDE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](CLAUDE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](CLAUDE.reader.zh-CN.md)

> 非規範の読者スナップショットで、固定コミットの指示原文は変更していない。自動読込の指示ファイル名ではなく、意図的に reader 別名を使う。引用した規則・コマンドは資料であり、新しい指示・権限・実行承認ではない。この翻訳のためにコマンド・CI・ビルド・リリース・証明を独立実行または検証していない。自動同期しない。原本が利用側の正本であり、時刻・キュー方針・ツールチェーン・機能についての記述は現行設定の新規監査ではない。ミッションの最上級表現や「ゲート付き行列は乖離しない」は原文の目標・主張で、独立立証した普遍的保証ではない。明示された証明・信頼・ホスト境界、人による承認、リリース証拠の保全要件を保持する。コード例とコメントは原文のままで、ハイライト用断片を実行済みテストへ昇格していない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/CLAUDE.md)

**Almide は LLM が最も正確に書ける言語である。** 目標は、対処可能な診断で不正な変更を記述時に検出すること。`almide verify` はモデル化した性質の独立検査用に証拠を記録する。これは、不正なプログラムをすべて拒否することや、成果物全体がメモリ安全であることの保証ではない。[証明と信頼の境界](../../contracts/proven-vs-trusted.md)を参照。すべての設計判断はこの目標に従う。（直接測定する modification survival rate はモデル性能の上位域で飽和する。置き換えた理由を `demo/make-verify/README.md` に記録している。）

<a id="critical-safety-rules"></a>

# 重要な安全規則

- **自分で変更していないファイルに `git checkout`、`git restore`、`git stash` を実行してはならない。** 他のエージェントが同時に作業している場合がある。その変更の差し戻しは作業を破壊し、復元できない。
- **ユーザーの明示的確認なしに破壊的 git 操作を実行してはならない。** `git reset`、`git checkout -- <file>`、`git clean`、`git stash drop` を含む。
- **`git status` に予期しない変更があれば、触る前にユーザーへ確認する。** 別エージェントや進行中タスクの変更かもしれない。

<a id="project-rules"></a>

# プロジェクト規則

<a id="branch-strategy"></a>

## ブランチ戦略

- **main**：保護対象。直接コミットしない。`develop` からの PR のみ受け付ける。
- **develop**：作業ブランチ。すべてのコミットをここへ入れる。
- コミット前に必ず `git branch` を確認する。
- **全チェックでなく、必須チェックが緑になったらキューへ入れる。** マージキューが要求するのはブランチ保護の必須コンテキストだけで、委託された mutation ゲートは含まれない（pull_request のみで動作し、着地後の develop push はすべて `mutation-sweep.yml` が評価する）。これを待つことはキューが要求しない唯一のジョブを待つことで、2026-09-21の測定では56分、最も遅い必須 shard は27分だった。`scripts/enqueue-when-required-green.sh <pr> [--wait]` を使う。API から必須集合を読み、PR の集計結果をその集合だけと比較し、GraphQL mutation でキューへ入れる。`gh pr merge --auto` で設定した自動マージも自らキューへ入るが、すべてのチェックが緑になってからである（その method は REBASE キューに対して MERGE と表示されるが、GitHub が正規化するため無害）。スクリプトなら早い時点で入れられる。PR の mutation ゲートが赤なら、それでも後から develop sweep の発見事項として読む。緑のゲートが `PARTIAL: ran N of M
  in-scope mutants` と表示した場合、実行したのは N 個だけ（28分予算、#2814）であり、名前を挙げた延期分は PR でなく develop の sweep が評価する。

<a id="git-commit-rules"></a>

## Git コミット規則

- コミットメッセージは**英語のみ**。
- 接頭辞（feat:、fix: など）は付けない。
- 簡潔な1行にする。
- 理由でなく変更内容に焦点を当てる。
- コミットメッセージは**英語のみ**（`english-only` commit-msg フックで強制）。

<a id="release-procedure"></a>

## リリース手順

リリース workflow（`.github/workflows/release.yml`、`v*` タグ push で起動）が**リリース作成を担当する**。タグ push 後に手動で `gh release create` を実行してはならない。workflow と競合し、「a release with the same tag name already exists」でそのステップが失敗する。

正しい流れ：

1. `develop` で `Cargo.toml` のバージョンを上げ、commit、push する。
2. `develop` の CI が緑になるまで待つ。
3. `develop → main` の PR を作りマージする（緑の CI が必要。リリースを強制マージしない）。
4. `git tag vX.Y.Z <merge-commit>` と `git push origin vX.Y.Z`。
   - **リリースブロッカーが最終タグを制限する**（#1482）：`I-unsound` / `I-miscompile` / `I-divergence` / `regression` ラベルの issue が未解決なら workflow は正式リリースを拒否する。[docs/project/ISSUE-TAXONOMY.md](../../project/ISSUE-TAXONOMY.md) の閉じた集合である。`bash scripts/count-release-blockers.sh` で事前確認する。
   - **RC チャネル**（#1484）：言語の公開機能またはコンパイラ挙動を変えるリリースでは、先に `vX.Y.Z-rc1` をタグ付けする。GitHub PRERELEASE として公開され（「latest」から除外、ブロッカーゲートは表示するが失敗させない）、安定化観察の期間を得る。問題がないと確認して正式な `vX.Y.Z` をタグ付けする。期間はリリースごとに人が判断する。
   - **下流 canary**（#2839）：`vX.Y.Z-rc1` と正式版の両方について、タグ前に候補コミットで `gh workflow run downstream-canary.yml -f candidate_ref=<sha>` を実行する（baseline は最新の非 rc リリースが既定）。約45の実在する公開下流プロジェクトを HEAD で両コンパイラによりビルドする（`scripts/downstream-canary.toml`）。**REGRESSION 行が1つでもあれば、トリアージするまでタグを止める**。`regression` ラベルの issue を作成するか、意図した破壊なら `proofs/dialect-epochs.toml` の項目として記録する。`infra` 行は判定ではないため、その経路を再実行する。両側で失敗しても以前のリリースで成功したコマンド（既知正常台帳 `proofs/downstream-canary-known-good.toml`）も REGRESSION（`good/FAIL`）となる。すでに出荷した破壊も破壊である。CLEAN な canary の後だけ、`bash scripts/downstream-canary-record.sh <run-id>` で台帳を進めコミットする。それ以外では進めない。
   - **インターフェース差分**（#1488）：正式タグ前に `bash scripts/check-interface-diff.sh vPREV vX.Y.Z` を実行する。コミット済みシグネチャ索引から標準ライブラリ公開面を identical / additive / breaking に分類し、`--allow-breaking` 宣言なしの破壊的差分を拒否する（削除には `@deprecated` 期間、および既存コードを壊し得る場合は `proofs/dialect-epochs.toml` の項目が必要）。
5. **タグ前にリリースノートを書く**：`docs/releases/vX.Y.Z.md`（すべての `-rc` タグも対象）を `develop` → `main` に入れ、タグ対象コミットに含める。1行目 `# vX.Y.Z — <headline>` がタイトル、残りが本文で、テーマ別にまとめる。自動生成の「What's Changed」PR 一覧は使わない。形式は [docs/releases/README.md](../../releases/README.md)。`bash scripts/check-release-notes.sh vX.Y.Z` で確認する。
6. **workflow にリリースを作成させる。** ファイルからタイトルと本文を公開し、ファイルがないか形式不正ならビルド前にタグを拒否する。後日の訂正は先にファイルへ入れ、次に `bash scripts/check-release-notes.sh vX.Y.Z out && gh release edit vX.Y.Z --title "$(cat out/title)" --notes-file out/body.md` を実行する。
7. **固定記録された台帳件数を更新する**：`bash scripts/gen-ledger-counts.sh` を実行し、`proofs/ledger-counts.toml` と再描画される4文書をコミットする。fixture / 契約 PR では更新しない。夜間の `scripts/check-ledger-counts.sh` が赤になるのが、もう1つの実行機会である。
8. **リリース証拠を封印する**（監査凍結）：`bash scripts/release-seal.sh gen vX.Y.Z` を実行し、`[recorded]` フィールド（release-gate fuzz 実行、asset 一覧）を埋め、`proofs/releases/vX.Y.Z.toml` を `develop` へコミットする。以後 CI は各 `[derived]` フィールドをそのタグに照らして継続再計測する。seal はリリースの不変の証拠記録である。

リリース失敗時は、未公開のまま失敗したビルドと、すでに公開されたリリースをまず区別する。ビルド失敗だけでは、公開済みリリースやタグを削除する権限にならない。コミット済み seal は元のタグを取得可能なまま保持する必要があり、削除すると証拠検査が壊れる。

以前の「削除して次のパッチ」という手順ではこの制約を解決していなかった。撤回や破壊的な復旧の前に、公開済みタグ・asset・attestation・seal に関する、所有者が明示承認した計画を得る。このチェックリストは証拠保全の例外を決めない。修正は `develop` で準備し、通常のバージョン付きリリース手順を用いる。公開済みバージョンを黙って再利用しない。

<a id="development-setup"></a>

## 開発環境の準備

clone 後、サブモジュールを取得し git フックをインストールする。

```bash
git submodule update --init --recursive
brew install lefthook  # or: https://github.com/evilmartians/lefthook
lefthook install
```

pre-push の clippy ラチェットが判定を出すのは、CI の固定ツールチェーン（1.96.0）を使う場合だけである。固定版外では clippy リリース間の件数を比較できないため `NO VERDICT` を表示してスキップする。rustup 管理環境なら `rustup toolchain install 1.96.0 --component
clippy` で判定可能になる。qusp 管理マシン（rustup なし）では CI だけの判定に留まり、緑の push は clippy の判定ではない。同じフックの run_parity はどの環境でも判定する。

`lefthook install` なしでは pre-commit / pre-push ゲートは一切動かず、フックが数秒で検出できた失敗ごとに CI 1周（30〜56分）を費やす。2026-09-21に一晩3周と測定した。セッション最初の push 前に `which lefthook` を確認する。

サブモジュール（`actions/checkout` は取得しないため CI から見えない。ローカル用の便宜であり、ビルドはどれにも依存しない）：

| パス | リポジトリ | 利用先 |
|---|---|---|
| `grammar/` | [almide/almide-grammar](https://github.com/almide/almide-grammar) | エディター / tree-sitter リポジトリが使う文法定義 |
| `research/benchmark/lang-bench/upstream/` | [mame/ai-coding-lang-bench](https://github.com/mame/ai-coding-lang-bench) | `/almide-lang-bench` |

<a id="project-overview"></a>

## プロジェクト概要

Almide は、純 Rust コンパイラを通じ、2ターゲット（Rust、WASM）向けにコード生成するプログラミング言語（.almd ファイル）である。

- **アーキテクチャ**：[docs/ARCHITECTURE.md](../../ARCHITECTURE.md)：コンパイラ・パイプライン、モジュールマップ。
- **言語リファレンス**：[docs/CHEATSHEET.md](../../CHEATSHEET.md)：構文、標準ライブラリ、慣用形（AI コード生成向け）。
- **標準ライブラリ**：`stdlib/` の自ホスト `.almd` モジュール。レジストリは `crates/almide-types/src/stdlib_info.rs`、モジュール別文書は [docs/stdlib/](../../stdlib/)。
- **モジュールシステム**：[docs/specs/module-system.md](../../specs/module-system.md)：インポート、サブモジュール、ダイヤモンド依存。
- **パッケージシステム**：[docs/specs/package-system.md](../../specs/package-system.md)：依存管理、MVS、バージョン共存。

<a id="building--installing"></a>

## ビルドとインストール

コンパイラソース変更後は常に再ビルド・インストールし、PATH 上のバイナリを最新にする。

```bash
make install   # cargo build --release + install to ~/.local/bin/almide
```

<a id="usage"></a>

## 使い方

クロスターゲットの出力比較は、対応する契約および一致するホスト・入力の前提の下で適用する。[等価性の範囲](../../design/EQUIVALENCE.md)を参照。

```bash
cargo build --release

almide run app.almd              # Compile + execute
almide run app.almd --target wasm  # Execute on embedded Wasmtime; parity has contract/host conditions
almide run app.almd -- arg1 arg2 # Program args go after --
almide build app.almd -o app     # Build binary
almide build app.almd --target wasm  # Build WASM
almide test                      # Find all .almd with test blocks (recursive)
almide test spec/lang/           # Run tests in a directory
almide test spec/lang/expr_test.almd  # Run a single test file
almide test --run "pattern"      # Filter tests by name
almide bench app.almd            # Verify-then-time (median; --target wasm for the embedded host)
almide compile                    # Module interface (project)
almide compile parser             # Module interface (by name)
almide compile app.almd --json    # Module interface (JSON)
almide check app.almd             # Type check only
almide check app.almd --target wasm# + the wasm build route: E081/E082 at check time (#1922)
almide check app.almd --profile critical --allow IO  # Critical profile (#567): bounded rules on every fn, capabilities deny-all
almide fmt app.almd               # Format source
almide fmt --check spec/          # Formatting gate (non-zero on drift)
almide fmt --no-import-edit stdlib/  # Format WITHOUT touching imports (splice-context sources)
almide clean                     # Clear dependency cache + native build scratch (#2500)
almide add almide/pkg@v0.1.0    # Add dependency (github.com/almide/ default)
almide update [dep]              # Advance a locked git dep to its ref's remote head (tags never move)
almide deps                      # List dependencies
almide dep-path bindgen          # Print cached source dir of a dependency
almide app.almd --target rust    # Emit Rust source
almide app.almd --target rust --repr-c  # Emit with #[repr(C)]
almide app.almd --emit-ast       # Emit AST as JSON
```

<a id="test-structure"></a>

## テスト構成

`almide test` は `test` ブロックを含む全 `.almd` ファイルを再帰的に探す。

- **インラインテスト**：任意の `.almd` に `test "name" { }` を書く。
- **テストファイル**：専用テストファイルは慣例として `*_test.almd` 接尾辞を使う。

```
spec/
├── lang/            Language feature tests (*_test.almd)
├── stdlib/          Stdlib tests (*_test.almd)
└── integration/     Multi-module / integration tests
tests/               Rust compiler unit tests (.rs, Cargo auto-discovery)
```

テスト実行：
```bash
almide test                      # All .almd with test blocks (recursive)
almide test spec/lang/           # Language tests only
almide test spec/stdlib/         # Stdlib tests only
```

<a id="testing-rules"></a>

## テスト規則

コンパイラ変更は**すべての演習とテスト**で検証しなければならない。

```bash
almide test
```

標準ライブラリ関数を追加・変更するとき（標準ライブラリは自ホスト化しており、`stdlib/defs/*.toml` と `runtime/rust/` は存在しない）：
- `stdlib/<module>[_<part>].almd` に純 Almide で実装する。
- WASM/v1 対応では、`stdlib/<module>_<part>.almd` の `fn` 直上に `@implements("module.fn")` を宣言する。ビルドがレジストリ行（`crates/almide-types/src/generated/implements.rs`）を生成し、ファイルが `prim.` に一度も到達しない本体は一覧項目なしで wasm 経路に受け入れられる。`self_host_registry.rs` への行や `crates/almide-wasm/src/whitelist.rs` への名前追加は禁止。どちらも縮小のみ許す旧名称一覧である（`tests/name_keyed_sites_test.rs`）。必要なら `crates/almide-mir/src/purity.rs` を調整する。未リンクの標準ライブラリ呼び出しは wasm レンダラーで拒否エラーとなる。
- ネイティブ intrinsic が必要な場合のみ、`runtime/rs/src/<module>.rs` に `almide_rt_*` を実装し、モジュールの `.almd` に `@intrinsic("almide_rt_*")` で宣言する。
- 新規モジュールでは `crates/almide-types/src/stdlib_info.rs` の `STDLIB_MODULES` / `BUNDLED_MODULES` / `bundled_source()`（自動インポートなら `AUTO_IMPORT_BUNDLED` も）を更新する。
- `spec/stdlib/` にテスト（`*_test.almd` またはインライン `test` ブロック）を書き、3方向 oracle がカバーするよう almide-interp の橋渡しコードを追加する。
- **API ファミリーは点ごとでなく行列として拡張する。** 関数がファミリーに属する場合（数値変換3種 `to_T`/`to_T_saturating`/`to_T_checked`、型別の境界、Codec 対応など）、完備性規則（必要なセルと意図的な省略。例：情報損失あり⇒3種、損失なし⇒通常版のみ）を示し、同じ PR でその規則を検査する実行可能な行列ゲートを導入・拡張する。手作業で形を維持する公開面は乖離するが、ゲート付き行列は乖離しない。

コード生成変更時：
- 所有権をテストする。`for...in` 後にも使う変数が動かなければならない。
- effect 関数をテストする（ADR-0008）。伝搬は明示的である。`fs.read_text(p)!` はコンパイルして伝搬し、裸の `fs.read_text(p)` 文は E042（must-use）、注釈なしの `let x = fs.read_text(p)` は E041。`let _ = f()` は伝搬せず破棄する。
- 生成 Rust が警告なしでコンパイルできることをテストする。

<a id="writing-idiomatic-almide"></a>

## 慣用的な Almide の書き方

`.almd` コード（標準ライブラリ、パッケージ、例）では次の慣用形に従う。

<a id="prefer-match-over-ifelse-chains"></a>

### if/else 連鎖より match を優先
```almide
// ✗ avoid
if kind == "int" then "i64"
else if kind == "float" then "f64"
else if kind == "string" then "String"
else "unknown"

// ✓ use match
match kind {
  "int"    => "i64",
  "float"  => "f64",
  "string" => "String",
  _        => "unknown",
}
```

<a id="prefer-list-combinators-over-var--for"></a>

### var + for よりリストコンビネーターを優先
```almide
// ✗ avoid
var result: List[String] = []
for item in items {
  result = result + [transform(item)]
}
result

// ✓ use map / flat_map / filter_map
items |> list.map((item) => transform(item))

// ✓ with index: list.enumerate — destructure the pair in the parameter
cases |> list.enumerate |> list.map(((idx, case)) => "${idx}: ${case}")

// ✓ FALLIBLE body: the callback's `!` instantiates the fallible form
//   (first-err short-circuit, ADR-0006) — never var + for
files |> list.map((f) => read_meta(f)!)!
```

<a id="prefer-listfind-over-var--for-search"></a>

### var + for 検索より list.find を優先
```almide
// ✗ avoid
var result = json.null()
for t in types {
  if get_str(t, "name") == name then result = t else result = result
}
result

// ✓ use list.find
types |> list.find((t) => get_str(t, "name") == name) ?? json.null()
```

<a id="prefer-recursion-over-var--while--flag"></a>

### var + while + フラグより再帰を優先
反復回数が事前に分かる場合は範囲 `for`（`for _ in 0..<n`）を書き、`var` で状態を受け渡す。再帰は早期終了（`break`/`continue` の形）や上限が事前不明のループ向けであり、回数を数える `f(n - 1)` ループ向けではない。
```almide
// ✗ avoid
var i = p
var go = true
while i < len and go {
  let c = peek(t, i)
  if is_ws(c) then { i = i + 1 }
  else { go = false }
}
i

// ✓ use recursion
fn skip_ws(t: String, p: Int) -> Int =
  if p < string.len(t) and is_ws(peek(t, p)) then skip_ws(t, p + 1)
  else p

// ✓ or use scan_while for common patterns
fn scan_while(t: String, p: Int, pred: (String) -> Bool) -> Int =
  if p < string.len(t) and pred(peek(t, p)) then scan_while(t, p + 1, pred)
  else p
```

<a id="use-heredoc-for-static-text-blocks"></a>

### 静的テキストブロックには heredoc を使用
```almide
// ✗ avoid: array of strings joined
let code = [
  "#[no_mangle]",
  "pub extern \"C\" fn alloc(len: i32) -> *mut u8 {",
  "    let buf = Vec::<u8>::with_capacity(len as usize);",
  "    buf.as_mut_ptr()",
  "}",
] |> list.join("\n")

// ✓ use heredoc: no escapes, reads like actual code
let code = """
  #[no_mangle]
  pub extern "C" fn alloc(len: i32) -> *mut u8 {
      let buf = Vec::<u8>::with_capacity(len as usize);
      buf.as_mut_ptr()
  }
  """
```

<a id="use-pipe-for-data-transformation-chains"></a>

### データ変換連鎖には pipe を使用
```almide
// ✓ pipe chains
fields
  |> list.map((f) => "${get_str(f, "name")}: ${go_type(get_type(f))}")
  |> list.join(", ")
```

<a id="use--for-fallback--for-resultoption--for-unwrap"></a>

### フォールバックは ??、Result→Option は ?、unwrap は !
```almide
value.get(v, key) ?? default_val      // Result/Option fallback
json.get(v, "field")?                  // Result → Option
parse_int(s)!                          // unwrap and propagate into the current fallible fn or lambda
```

<a id="imports"></a>

### インポート
- 標準ライブラリ・モジュール（`string`、`int`、`float`、`list`、`value`、`map`、`set`、`math`、`datetime`、`bytes`、定幅整数など）は自動インポートされるため、`import string` を書かない。正本一覧は `import_table.rs` の seed と `stdlib_info.rs` の `AUTO_IMPORT_BUNDLED`。
- `json`、`fs`、`http`、`env`、`io`、`random`、`regex`、`process`、`testing`、`url` は明示的な `import` が必要。
- 外部パッケージは `import pkg_name` が必要。
- パッケージ自己参照：`import self as pkg_name`。

<a id="key-design-decisions"></a>

## 主な設計判断

- **マルチターゲット**：同じ IR から `--target rust|wasm` で Rust / WASM を出力する（TS コード生成は削除済み）。
- **Codegen v3**：Nanopass パイプライン（意味的な書き換え）と TOML テンプレート・レンダラー（構文）。
- **Effect fn（Rust）**：`effect fn f() -> T` の呼び出しは既定で `Result<T, String>`。明示的な `Result[T, E]` 戻り型はそのエラー型を保持する。純粋な失敗可能関数は `-> T!` / `-> T!E`。伝搬は **明示のみ**（ADR-0008）— `!` の Rust 側の実現は文脈による（Result/Option の伝搬、test の unwrap など）。lambda の境界を含む詳細は [結果・Option・エフェクト仕様](../../specs/result-option-effect.md) の文脈表を参照。暗黙伝搬は E041、statement 位置の握り潰しは E042。
- **`==`/`!=`**：通常の二項等価比較は Rust の `almide_eq!` に出力する。convention `eq` および `Secret` の比較は、その前に lowerer が関数呼び出しへ解決する（`crates/almide-frontend/src/lower/expressions_access.rs`）。
- **`+`**：文字列とリストの連結（加算と多重定義）。
- **診断**：すべてのエラーに file:line、文脈、対処可能なヒントを含める。

<a id="repo-boundary-almide-vs-almide-dojo"></a>

## リポジトリ境界：almide と almide-dojo

- **このリポジトリ**＝コンパイラの正しさ。`spec/` テスト、`cargo test`、grammar-lab 実験、lang-bench。
- **[almide/almide-dojo](https://github.com/almide/almide-dojo)**＝LLM の書きやすさ。日次 MSR 測定、タスク集、悪意あるヒント検出、診断フィードバックループ。
- MSR 作業はすべて Dojo へ置く。`research/benchmark/msr/` と `research/benchmark/framework/` はこのリポジトリから削除した（2026-08-08）。ローカルハーネスは2026年4月の引き継ぎ以降 `results/` が空で、実行すると Dojo の公開値をローカル測定値で上書きするためである。`git log --diff-filter=D -- research/benchmark/msr/` で回収できる。
- ここに残り稼働しているもの：`research/benchmark/perf/`（`scripts/check-perf-ratio.sh` で制御）、`research/benchmark/lang-bench/`、`research/benchmark/exercises/`（`tools/v1_gap_measure.py` 用コーパス）、`research/grammar-lab/`、`research/spike/`。
- 橋渡し：Dojo の PR ゲートがこのリポジトリの CI の一部としてタスク部分集合を実行する予定（将来）。

<a id="behavior-contracts"></a>

## 振る舞い契約

観測可能なクロスターゲットの約束（ネイティブ Rust ⇄ wasm32：stdout、stderr、終了コード）は、それぞれ [docs/contracts/contracts.toml](../../contracts/contracts.toml) の名前付き `[[contract]]` であり、実行可能な証拠（`spec/wasm_cross/*.almd` fixture、差分 fuzz、出力時 Σ-probe、または Lean 定理）へ追跡できる。索引は [docs/contracts/README.md](../../contracts/README.md)（自動生成）。

**観測可能な挙動を変える＝同じ PR で契約台帳を更新する。**

- 新しい挙動＝新しい `C-NNN` と1つ以上の fixture。fixture はヘッダー行 `// @contract: C-NNN` で宣言する（逆リンクは必須かつ対称）。
- 差異の除去＝同じ PR で `status` を `active` にし、フラグを除き、ラチェットを下げる。`flagged-for-revision` 件数は減少のみ許す。
- **証明済みと信頼に依存する部分**：[docs/contracts/proven-vs-trusted.md](../../contracts/proven-vs-trusted.md)：境界マップと各ゲートが主張すること・しないこと。
- `scripts/check-contracts.sh`（CI `checks` ジョブと lefthook pre-commit フック）は、各契約が fixture 以上のクラスの証拠を持つこと、各 fixture が契約を明記すること、双方向リンクであることを強制する。証拠クラスの用語は `scripts/lib/contract-classes.txt` を通じて rt-oracle-registry と共有する。
- **新しい `C-NNN` は先に [almide/als](https://github.com/almide/als) へ入れる。** judge が規範台帳を保持する。`scripts/check-als-pin.sh`（CI `checks`）は、`proofs/als-pin.txt` に固定したコミットの judge 台帳にない契約 ID を拒否する。順序は als PR（契約＋fixture）のマージ → 単独コミットで pin 更新 → 実装 PR。
- **集計件数は固定記録し、PR ごとには再生成しない。** 生成文書が引用する「N contracts / N fixtures」の総数はすべて、日付付き `counts:generated` ブロック内で `proofs/ledger-counts.toml` から描画する。fixture / 契約 PR は構造台帳（行、manifest、索引）を再生成し、ブロックはそのままにする。リリース時、または夜間の `scripts/check-ledger-counts.sh` が乖離を報告したときに `bash scripts/gen-ledger-counts.sh` で更新する。

<a id="the-llm-facing-surface-is-the-stable-surface-1483"></a>

## LLM 向け公開面が安定した公開面である（#1483）

`docs/CHEATSHEET.md` と `llms.txt` はモデルが学習・コンテキストデータとして読む。**そこに現れるものはすべて約束**であり、約束する意図がないものを載せてはならない。主な読者は文書と実態の穴を見抜けないため、ゲートで防ぐ。

- ```` ```almide check ```` ラベルの fence は「完全で検査可能なプログラム」を表明し、`scripts/check-llm-surface.sh`（CI）がすべてに `almide check` を実行する。古くなった例は学習用コーパスのバグではなく CI 失敗である。通常の ```` ```almide ```` は断片や意図的な ✗ 例のハイライト用ラベルに留める。
- カバー範囲の拡大は例を完成させ、fence ごとにマーカーを昇格することを意味する。ゲートの許可一覧は縮小のみ。
- cheatsheet に文書化する新しい言語機能は、同じ変更で spec によるカバーも導入する。上の契約台帳規則を文章にも適用する。

<a id="documentation"></a>

## 文書

- 言語仕様：`docs/specs/` — 規則は [docs/specs/CLAUDE.md](../../specs/CLAUDE.md)。
- コンパイラ設計：`docs/ARCHITECTURE.md`。
- 言語リファレンス：`docs/CHEATSHEET.md`。
- ロードマップ：`docs/roadmap/`。
- 振る舞い契約：`docs/contracts/` — クロスターゲット等価性の契約台帳。
