<!-- description: Playground UI revamp: file tabs, share URLs, TS-style example gallery -->
# Playground UI Revamp

## Vision

サーバーサイド playground(Go/Rust 方式)の機能一覧を埋めるのではなく、
**完全ブラウザ内 WASM 実行だから構造的に可能な体験**に振り切る:

1. **常時実行** — コンパイルがローカルでタダなので、打鍵アイドルで自動コンパイル→即座にエラー表示
2. **画面出力** — stdout しか返せないサーバー式に対し、ユーザーのブラウザで動くので canvas/DOM に直接描ける
3. **無料で無限に埋め込める** — 静的配信のみなので、docs/lander/記事のコードブロックを全部実行可能にしてもコストゼロ

## 決定事項 (2026-07-26)

- **サーバーサイド実行は導入しない**。共有は Gist ではなく URL hash(`CompressionStream` deflate + base64url、依存ゼロ)
- **マルチファイルはファイルタブ方式**(Go の `-- file --` 区切りでも、フルファイルツリーでもなく)。
  タブ名が `import self.<name>` に対応し、モジュールシステムの実演を兼ねる。
  タブ内容は browser_wasi_shim の仮想 FS に流し込み、`fs.read` でデータファイル(csv/toml/yaml/json)も読める
- **サンプルギャラリーはカテゴリ付き + TS Playground 方式**:
  - 解説をサイドバーではなくコードのコメント本文に埋める(説明 7 割・コード 3 割)
  - 「ここを書き換えて Run してみて」というエラー体験の誘導を入れる
  - 末尾に次サンプルへのリンク(`?example=<id>`)で数珠つなぎ → 将来の言語 Tour の素材になる
- **サンプル 1 本 = URL 1 本**(`?example=csv-parse`)。promo 投稿・docs から「ブラウザで今すぐ動く」deep link を貼れる
- **ギャラリーのサンプルは native/WASM byte-parity CI**(playground repo `tests/run.mjs`)**に合流**させ、腐らせない
- 追わないもの: 外部パッケージ取得、clippy/miri 的ツール群、Compiler Explorer 式マルチコンパイラ比較ペイン

## Current State (playground repo, 2026-07-25 時点)

- 素の HTML + ES Modules 単一ファイル(index.html 1365 行)、エディタは textarea + 透明ハイライト重ねの自前実装
- 実行は完全ブラウザ内: almide-mir `try_render_wasm_source` → `wat` crate → browser_wasi_shim(WASI)。コンパイラ wasm 1.6MB
- あるもの: Shiki ハイライト、Output/Compiled Rust/AST の 3 タブ、AI 生成 + 自動修復ループ(BYOK 3 プロバイダ)
- ないもの: 共有リンク、サンプル集、マルチファイル、補完、インラインエラー、Ctrl+Enter、Worker 隔離(無限ループでタブが固まる)

## Progress

**2026-07-26: Tier 1 + Tier 2 実装完了**(playground repo、ブラウザ E2E + ハーネス検証済み)

- [x] 1.1 Worker 実行基盤 — worker.js/runner.js、stdout ストリーミング、Stop = terminate+respawn、30s タイムアウト。無限ループ→Stop→再実行を E2E 確認
- [x] 1.2 CodeMirror 6 + インライン診断 — esm.sh importmap(ビルドレス維持)、Almide StreamLanguage、`check_project`(新 crate API、span 付き JSON 診断)をアイドル 500ms で実行、hint 併記、`try:` snippet は一click fix action、Ctrl/Cmd+Enter で Run
- [x] 1.3 ファイルタブ + 仮想 FS — crate にインメモリ `import self.*` リゾルバ追加(`compile_project_to_*(files, entry)`)、非 .almd タブは browser_wasi_shim の PreopenDirectory("."), `fs.read_text("data.csv")` がブラウザで動作確認済み
- [x] 2.1 URL hash 共有 — CompressionStream deflate-raw + base64url、タブ一式、ラウンドトリップ E2E 確認
- [x] 2.2/2.3 TS 方式ギャラリー — 6 本(pattern-matching / pipes-and-lists / error-handling / modules / mini-markdown / csv-report)、カテゴリ付きメニュー、`?example=<id>` deep link、`tests/run.mjs` に examples セクション追加(native/wasm byte-parity、wall は即 fail)

**2026-07-26: Tier 3 実装完了**(同日、E2E + ハーネス検証済み)

- [x] 3.1 Visual 出力 — 設計を「stdout に SVG / PPM P3 を印字したらプレイグラウンドが画像レンダリング」に確定。コンパイラ変更ゼロ、native では同じプログラムが正規の .svg/.ppm を吐くので byte-parity CI がそのまま適用される。Graphics カテゴリ(Mandelbrot PPM / 生成 SVG)追加、8/8 examples green。almide-web バインディング統合(インタラクティブ Canvas)は別 arc
- [x] 3.2 embed モード — `?embed=1`(ヘッダ/AI バー非表示 + スリム Run バー + Open in Playground)、`&hide=` で hidden setup(タブ非表示・コンパイルには含む)、embed 時は localStorage 不使用
- [x] 3.3 stdin + zip — `stdin.txt` タブが fd0 になる(io.read_line 動作確認、io.read_all は wasm registry 未登録で wall — 小ギャップ)。Export ボタンで `almide run src/main.almd` 可能なプロジェクト zip(依存ゼロの store 方式 zip writer)

発見した課題(別対応):
- `almide run --target wasm` が guest cwd を `$PWD` から導出しており、`getcwd` と食い違うと相対 fs read が wasm でだけ ENOENT(execFileSync 等 PWD を更新しない親から顕在化)。ハーネス側は PWD を明示して回避済み。**almide 本体で current_dir 参照に直すべき**
- cross-module ADT(module 定義の variant 型を entry で使う形)は v1 wasm レンダラの wall(`main is outside the MIR-lowering subset`)。modules サンプルは関数モジュール(stats)に設計変更で回避
- AI システムプロンプトの example が旧 `effect fn main(_args)` 形で E028 を誘発していたのを修正

## Roadmap

### Tier 1 — 土台(done 2026-07-26)

#### 1.1 Worker 実行基盤
コンパイル + 実行を Web Worker に隔離。無限ループで UI が固まらない、キャンセルボタン、
stdout ストリーミング、実行タイムアウト。自動コンパイル(1.2)と常時実行の前提。

#### 1.2 CodeMirror 6 化 + インライン診断
自前 textarea 実装を CodeMirror 6 に置換(Monaco より軽量・モバイル対応)。
コンパイラのエラー span を CM6 diagnostics に接続し、アイドル時自動コンパイルで波線 + 行番号表示。
Ctrl+Enter で Run。ハイライトは既存 tmLanguage を流用。

#### 1.3 ファイルタブ + 仮想 FS
`main.almd` 固定 + タブ追加/リネーム/削除。タブ一式を仮想 FS に配置してコンパイル。
`import self.<sub>` が動くこと、`data.csv` タブを `fs.read` できることがゴール。

### Tier 2 — 共有とギャラリー

#### 2.1 URL hash 共有
タブ一式(`{name: content}`)を deflate + base64url で `#code=...` に載せる。
Share ボタン = URL コピー。単一ファイルでもマルチファイルでも同一機構。

#### 2.2 TS 方式サンプルギャラリー
カテゴリ付きドロップダウン/サイドバー。サンプルはリポジトリ内の素朴なディレクトリ
(1 サンプル = .almd ファイル群 + manifest)で管理し、CI でギャラリー用 JSON にパック。
カテゴリ構成(全部既存資産で埋まる):

| カテゴリ | サンプル候補 |
|---|---|
| 言語機能 | パターンマッチ、pipe、result/エラー処理、`import self.*` マルチファイル構成 |
| パーサ・データ処理 | Mini Markdown(現デフォルト)、CSV 集計、TOML/YAML/JSON、base64/hex |
| 数値・Matrix | matmul ベンチをブラウザで実測、Transformer 1-block |
| Canvas・グラフィクス | wasm-canvas 描画、ライフゲーム、SVG 生成(Tier 3 の Canvas 出力後) |
| 暗号 | sha1 / aes / rsa + bigint |
| AI(重量級) | bonsai 小モデル推論。weights 遅延ロードで別枠 |

#### 2.3 サンプルフォーマット(TS 方式)
```almide
// パターンマッチ入門
//
// Almide の match は全ケース網羅をコンパイラが検査します。
// 試しに "float" の腕を消して Run してみてください —
// エラーが「どのケースが漏れたか」を教えてくれます。

fn rust_type(kind: String) -> String =
  match kind {
    "int"    => "i64",
    "float"  => "f64",
    "string" => "String",
    _        => "unknown",
  }

// 次: ?example=pipe-basics
```

### Tier 3 — 見せ場と配布

#### 3.1 Canvas 出力モード
出力ペインに Console と並べて Canvas タブ。almide-web / wasm-canvas 系サンプルの受け皿。
揃ってきたら出力サムネイル付きカードギャラリーに格上げ。

#### 3.2 embed モード
`?embed=1` で iframe 埋め込み用のミニマル UI(Kotlin Playground 方式)。
hidden setup(helper ファイルを隠して見せたい数行だけ表示)はマルチファイルの応用で実現。
lander / docs / dev.to 記事のコードブロックへ展開。

#### 3.3 stdin / 実行引数入力欄、zip エクスポート
タブ一式を `almide run` できる構造の zip で落とせる「プロジェクトとして持ち帰る」出口。

### 関連

- AI 修復ループの残項目(Accept/Reject、部分修復、修復履歴)は done/playground-repair.md Tier 2 を参照。
  ローカルコンパイルで検証がタダになる本 arc の土台(1.1)の上に載せる。
- on-hold/rumbling.md の「playground が adoption を加速する」戦略項目の実行編。
- active/determinism-belt.md — ブラウザ wasm32 での決定性はこの arc の前提。

## Success Metric

Reddit/X の投稿に `?example=csv-parse` の deep link を貼り、初見の読者が
インストールなしで「開く→動く→書き換える→エラーが出る→直る」まで 1 分で体験できること。
サンプルが CI で守られており、コンパイラ更新でギャラリーが壊れないこと。
