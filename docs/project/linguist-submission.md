# GitHub Linguist への言語追加 — 準備台帳

`.almd` を GitHub に言語として認識させるための計画。**2026-09-22 時点では出せない**。
理由は一つだけで、第三者の採用が足りないこと。文法・サンプル・色などの残りは今日そろえられる。

出典は [linguist/CONTRIBUTING.md](https://github.com/github-linguist/linguist/blob/main/CONTRIBUTING.md)
と [PR テンプレート](https://github.com/github-linguist/linguist/blob/main/.github/PULL_REQUEST_TEMPLATE.md)。
数字は 2026-09-22 に GitHub API で実測した。

## 1. 条件と現状

| 条件 | 基準 | 現状(2026-09-22) | 判定 |
|---|---|---|---|
| 拡張子の利用実績 | **1 年以内にインデックスされた `.almd` が 2,000 ファイル以上**、fork を除く | 全体 5,512 | 見かけ上は満たす |
| 所有者を除いた実績 | レビュアは主要所有者を `-user:` で除外して評価する | `-user:almide -user:O6lvl4` で 904、さらに `-user:almide-graphics` を除くと **0** | **未達** |
| 所有者 / リポの分散 | 無作為にクリックして分散していること | 非所有者分 904 のうち 296 が `almide-graphics/nn` の 1 本。実質 1 組織 | **未達** |
| 構文強調文法 | TextMate 互換文法が**許容ライセンス**の公開リポにあること | `almide/vscode-almide` の `syntaxes/almide.tmLanguage.json`(`scopeName: source.almide`)。ただし **LICENSE ファイルが無い** | あと一歩 |
| サンプル | 実コード。hello world は拒否。ライセンス明記 | 候補多数。`almide/almide` は MIT / Apache-2.0 のデュアル | 満たせる |
| 拡張子の衝突 | 既存言語と衝突しないこと | `languages.yml` に `.almd` は無い | 問題なし |

許容ライセンスは apache-2.0, bsd-2-clause, bsd-3-clause, cc0-1.0, isc, mit, mpl-2.0, ncsa, permissive, unlicense, wtfpl, zlib。

> **要確認**: `almide/almide` の `LICENSE-MIT` の著作権表記は `Copyright (c) 2026 mizchi` になっている。
> `almide/almide-dojo` は `Almide contributors`。新しく置いた 2 つの LICENSE は後者に合わせたが、
> 本体の表記が意図どおりかは持ち主の判断。テンプレート由来の残りであれば直した方がよい。

**早出しは逆効果**。2026 年に Linguist へ出された言語追加 PR は約 240 本、マージは 31 本で、
残りは利用実績不足で閉じられている。閉じられた PR は記録に残る。

### 通った言語は実際に何ファイルあったか(2026-09-22 実測)

| 言語 | マージ | `extension:` の総ファイル数 |
|---|---|---:|
| Blueprint | 2026-06-01 | 42,624 |
| Gno | 2026-07-20 | 41,536 |
| Tape | 2026-08-06 | 24,704 |
| MeTTa | 2026-02-22 | 12,736 |
| Bend | 2026-09-20 | 9,440 |
| FPP | 2026-08-19 | 9,424 |
| Tolk | 2026-06-21 | 8,128 |
| BAML | 2026-05-13 | 6,704 |
| Quint | 2026-05-06 | 5,824 |
| **Almide** | — | **5,512(所有者を除くと 0)** |

**総量では Almide は既に通った言語の下位と並ぶ。違うのは所有者の分散だけ。**
`.qnt` の検索結果 100 件には 8 以上の所有者が現れ、維持組織を除いても 5,648 ファイル残る。
`.almd` の 100 件は 100 件とも `almide` だった。

Bend の PR #8218 は評価のされ方をそのまま見せている。本文に
`extension:bend` が 8,640、維持者と上位 2 名を除いて 2,664 と書いてある。
Quint の PR も維持組織を除いた数を併記していた。

メンテナの言葉も明示的である。ある却下では
「2000 が基準だ。加えて人気を評価するときは言語の所有者と最上位の貢献者を除外する。
どのみちあなたとあなたの組織はいずれ除外していた」と書かれている。

## 1.5 文法の検査結果(2026-09-22 実測)

文法は Linguist の障害ではない。TextMate エンジン(`vscode-textmate` + `vscode-oniguruma`)で
実際にトークン化して測った。

| 検査 | 結果 |
|---|---|
| 構造 | `scopeName: source.almide`、`fileTypes: ["almd"]`、include 16 個すべて解決、孤立した定義なし |
| 被覆(3 ファイル) | 非空白文字の 83.4% にスコープが付く |
| 被覆(examples + stdlib + spec、577 ファイル、177 万字) | **87.8%。スコープの付かないトークンは括弧・カンマ・コロンなど記号だけ** |
| 言語構造 20 種の確認 | `effect fn`、`test`、`match` の `=>`、`fan`、heredoc、`${}` 補間、`!` `?` `??`、`@intrinsic`、型宣言 — すべてスコープが付く |

**見つかった欠陥は 1 件。** 関数定義の名前が `entity.name.function` ではなく
`variable.other` になっていた。原因は最上位パターンの順序で、`#keyword` が `#function-def` より
前にあるため `fn` だけが先に消費され、`#function-def` が一度も発火しない。TextMate は
最長一致ではなく**記述順の最初の一致**を採るため。

**修正は 1 行。** `vscode-almide` の `generator/almide_textmate.almd` で
`Include("#function-def")` を `Include("#keyword")` の前に移す。手元で検証済み:

- 生成物の差分はその 1 行の移動だけ(`almide run generator/main.almd` の出力を before/after で比較)。
- 3 ファイルで 30 箇所の関数名が `entity.name.function.almide` に変わる。
- `keyword.declaration` の数は 104 のまま変わらず、全体の被覆率も変わらない。

**もう 1 件は表記の揺れ。** 文法は `strict` を修飾子キーワードとして塗るが、
コンパイラの `KEYWORDS`(34 語)に `strict` は無く、`keyword_typo.rs` の誤字ヒントにしか出てこない。
実害は無いが、単一の真実である `grammar/tokens.toml` 側の記載が実装より広い。

## 2. 今日そろえられるもの

1. ~~**`almide/vscode-almide` に LICENSE を置く**~~ — **2026-09-22 着地**。MIT を追加し、
   GitHub の判定も `MIT` に変わった。`almide/tree-sitter-almide` にも同じものを入れた(main、5ed0dc5)。
   これで Linguist の文法取り込み要件は満たす。
2. ~~**§1.5 の 1 行修正**~~ — **2026-09-22 着地**(vscode-almide develop、d4a1438)。
   生成器を直して `syntaxes/almide.tmLanguage.json` を再生成済み。公開ファイルで再検証し、
   構造 20 種すべて合格、関数名は `entity.name.function.almide` になった。
   **未了**: develop → main が VS Code 拡張のリリースを起こすため、リリースは保留している。
2. **サンプルを決める**。実コードで、言語の構造を代表するもの。実測で選んだ候補:

   | ファイル | 行 | 含むもの |
   |---|---:|---|
   | `examples/lisp.almd` | 268 | `match` 27 箇所、パイプ 6 箇所。分量と構造の代表として最良 |
   | `examples/api-client.almd` | 79 | `import`、`effect fn`、`!` 伝搬 |
   | `examples/csv-to-json.almd` | 52 | パイプ鎖 7 箇所、`effect fn`、`test` ブロック 3 個 |

   `stdlib/list.almd` は避ける。`@inline_rust` 指示が主で、パイプが 1 つも無く、利用側のコードに見えない。
   PR 本文には「MIT / Apache-2.0 デュアル、出典は almide/almide」と明記する。
3. **色を決める**。提案は `#7c5cbf`。根拠は playground と docs サイトのアクセント色で、
   `almide/docs` の `src/styles/custom.css` に `/* Purple accent from playground: #7c5cbf / #9b7de0 */` とある。
   `languages.yml` に同じ値は無い。
4. **エントリ草案**(`language_id` は `script/update-ids` が採番するので空で出す):

```yaml
Almide:
  type: programming
  color: "#7c5cbf"
  extensions:
  - ".almd"
  tm_scope: source.almide
  ace_mode: text
```

## 3. 条件を満たす唯一の道:第三者の採用

インデックスされている `.almd` は現在すべて、`almide`、`O6lvl4`、`almide-graphics` の三つに入っている。
`almide-graphics` は自分が所属している組織で、しかも 1 本のリポジトリが非所有者分の 3 分の 1 を占める。
分散の条件はこの時点で落ちる。**自分でリポジトリを量産して数を作るのは
評価方法の裏をかく行為で、レビュアは `-user:` で除外する**ため無意味でもある。

数の目安。1 リポジトリあたり 20〜40 ファイルとすると、**第三者リポジトリが 50〜100 本**で 2,000 に届く。
分散の条件もこれで同時に満たせる。現実的な供給源は次のとおり。

- **パッケージ作者**。`almide add` で入る外部パッケージを他人が書くこと。現在 `almide/` 配下の
  パッケージ群はすべて自分の所有なので、評価上はゼロと数えられる。
- **Dojo の参加者**。タスクの解答リポジトリは `.almd` が並ぶ。第三者が自分のアカウントに置けば数に入る。
- **チュートリアル・作例**。Advent of Code、Exercism のコミュニティトラック、言語紹介記事の作例リポ。
- **エディタ利用者**。VS Code 拡張を入れた人が自分のリポジトリに `.almd` を置く。

進捗の測り方(所有者を除いた実数):

```bash
gh api -X GET search/code -f q='extension:almd -user:almide -user:O6lvl4 -user:almide-graphics' \
  -f per_page=1 --jq .total_count
```

レビュアが見るのは Web UI の数字なので、出す直前にログイン状態で
`https://github.com/search?type=code&q=NOT+is%3Afork+path%3A*.almd` も開いて確認する。

## 4. 出す日の手順

1. `github-linguist/linguist` を fork してブランチを切る。
2. `languages.yml` に上のエントリを追加する。`language_id` は書かない。
3. `script/add-grammar https://github.com/almide/vscode-almide` を実行する。Docker が要る。
   文法に問題があると弾かれるので、その場で直す。
4. `samples/Almide/` にサンプルを置く。
5. `script/update-ids` で `language_id` を採番する。
6. `bundle exec rake test` を通す。
7. PR テンプレートを**全部埋める**。埋めないと読まれない。「新しい言語を追加する」の節に、
   検索結果 URL、サンプルの出典とライセンス、文法リポの URL、色と根拠を書く。

PR 本文の骨子:

> Almide is a statically-typed language that compiles to Rust and WebAssembly.
> Search results: `https://github.com/search?type=code&q=NOT+is%3Afork+path%3A*.almd+effect+fn`
> Samples: taken from `almide/almide`, dual MIT / Apache-2.0.
> Grammar: `https://github.com/almide/vscode-almide` (MIT), `scopeName: source.almide`.
> Color `#7c5cbf`: the accent color of the Almide playground and documentation site.

## 5. 先に取れる、条件の軽いレジストリ

Linguist は最後でよい。強調表示だけなら先に置ける場所がある。**どこも審査はある**が、
求められるのは文法の完成度と保守の継続であって、ファイル数ではない。
nvim-treesitter は「機能が揃い、利用者に試され、保守されていること」を maintainer の裁量で見る。
Zed も提出は審査され、全部が通るわけではないと明記している。

| 先 | 要るもの | 効果 |
|---|---|---|
| Zed の extensions レジストリ | tree-sitter 文法 + 拡張定義の PR | Zed で強調表示 |
| nvim-treesitter / Helix | tree-sitter 文法の登録 PR | Neovim / Helix で強調表示 |
| Shiki | TextMate 文法の追加 | ドキュメントサイトや静的サイトの強調表示 |
| Pygments / Chroma | lexer の実装 | Sphinx、Hugo、多くのブログ基盤 |
| VS Code Marketplace | 公開済み | 既に取得済み |

これらに載っていること自体が、Linguist の PR で「実在する言語である」ことの傍証になる。
