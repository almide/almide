<!-- description: Take Nushell's data model for scripts; do not build an interactive shell -->
# Shell scripting stance: structured pipelines, no interactive shell

> 出発点は「Almide で Nushell みたいな使い勝手を出せないか」という問い。
> 答えは **半分イエス、半分は目指すべきではない** で、この文書はその線を引く。
> 隣接項目: [zero-committed-shell.md](./zero-committed-shell.md)（内向き）。本項は外向き。

## 決定

**Nushell から取るのはデータモデルであって、端末ではない。**

構造化データがパイプを流れる書き味 — これは取りにいく。対話シェルの層
（コマンド解決、ジョブ制御、リダイレクト、行編集、プロンプト）は**作らない**。

狙う位置は「bash スクリプトの置き換え」であって「zsh の置き換え」ではない。

## なぜ対話シェルを作らないのか

三つ、どれ単独でも十分な理由がある。

**1. MSR に一切効かない。** ジョブ制御もコマンド解決もグロブ展開も、言語の仕事ではなく
端末の仕事である。この言語の任務は modification survival rate であり
（[llm-first-language.md](./llm-first-language.md)）、対話 UI はその計測対象ですらない。
実装コストは大きく、MSR デルタはゼロ。判断基準がそう言っている以上、採らない。

**2. この路線は繰り返し負けている。** xonsh、Elvish、Ammonite — 「自分の言語を対話シェルに
する」試みは十年以上前からあり、bash/zsh を置き換えたものは一つもない。一方 Nushell が
獲れたのは対話 UI の出来ではなく**データモデルを設計し直したから**で、模倣すべきは
そちらである。勝っている部分だけを取る。

**3. 対話シェルの要求は静的型と正面衝突する。** 対話シェルの体験は「列が実行時に決まる」
ことに乗っている。Almide がそこに合わせるには型付けを緩めるしかなく、それは MSR を
直接削る（後述「動的行は入れない」）。

## いま実際どこにいるか（2026-09-12 実測、v0.62.0、Darwin 24.4.0）

| | 実測 |
|---|---|
| `almide run script.almd`（warm） | **20ms** |
| `almide run --target wasm`（warm） | 38ms |
| REPL 1 式（warm、wasm 脚） | 約 40ms |
| REPL、定義 80 行を積んだセッション | 1.57s（約 19ms/行 — 80 行までは平坦） |
| `process.exec` を含む REPL 式 | 0.82s（wasm 脚が降りて cargo 経路へ） |

**20ms は python3 と同クラスで、スクリプト言語として完全に実用域にある。**
これが本項の前提で、以降の判断はすべてこの数字に乗っている。

### 既にある材料

- `|>` と `list` の 66 関数 — `map` / `filter` / `fold` / `sort_by` / `group_by` /
  `partition` / `window` / `unique_by` / `take_while`。Nushell の
  `each` / `where` / `reduce` / `sort-by` / `group-by` は概ね対応物がある
- コールバックが `!` を含めば**そのまま fallible 形に切り替わる**（ADR-0006、#1041 で解決済）。
  effect を含むパイプラインが `var` + `for` に落ちない
- `process.*`: `exec` / `exec_in` / `exec_with_stdin` / `exec_status` /
  `exec_status_timeout`(#1040 で着地) / `spawn` / `kill` / `is_alive` / `stdin_lines`
- `fs.*`: `glob`（マッチャの穴は #1805 で解消）/ `walk` / `stat` / `list_dir` / `fold_lines` / `read_lines`
- `io.*`: `read_all` / `read_line` / `read_n_bytes` / `write`
- shebang: `#!/usr/bin/env -S almide run`（`spec/cli/shebang.almd`）
- REPL: サブコマンド無しの `almide`（`src/cli/repl.rs`）
- 行そのものが既に名前付きレコードで返る —
  `FileStat { size, is_dir, is_file, modified }`、`ProcessStatus { code, stdout, stderr }`

**bash の三大事故はすでに構造的に潰れている**: `process.exec(cmd, args)` は引数がリストなので
word splitting が起きず、クォート事故の余地がない。`Result` 返しと E042 により終了ステータスの
握り潰しがコンパイルエラーになる。ここは追加実装なしに勝っている。

### 無いもの

1. **テーブルレンダラ** — `stdlib/` に該当なし。Nushell 体験の体感的な半分がこれ
2. **境界パースの一手** — `exec` → `json.parse` → Codec decode が三手かかる
3. **遅延パイプライン合成** — `A | B` のストリーミングが無く、中間出力がメモリに載る
4. **行編集** — REPL は生の `read_line`。矢印キー・履歴呼び戻し・補完・複数行編集が全部無い
5. プロセス間パイプ構文、コマンド解決、ジョブ制御（= 作らないと決めたもの）

## 動的行は入れない — 前案の撤回

検討の初期に「`value.field(v, "name")!` を `v.name` と書ける糖衣が本丸」と置いた。**これは誤り
だったので撤回する。**

`value.field` は `Result` を返し、E042 が処理を強制する。`v.name` に糖衣をかけることは、
**検査済みの失敗を実行時の失敗に差し替える**ことに等しい。列名の打ち間違いがコンパイル時に
落ちなくなる — MSR を上げるどころか、LLM が最も犯しやすい種類の誤りを黙らせる方向である。

そして Almide には既に正解が明文化されている。[CHEATSHEET.md](../CHEATSHEET.md) §JSON & the wire:

> Typed Codec is the default path for ALL JSON work. The dynamic `json.*` API
> is for exploration and schemaless passthrough only — if you know the shape, declare a type.

外部コマンドの出力も同じ「wire」である。**境界で一度レコードに落とし、以降は静的に型が付いた
パイプラインを流す。** これは Nushell の動的テーブルより modification survival に強い:
列名を変えたとき、Nushell は実行時に気づき、Almide はコンパイル時に全参照を指す。

したがって Phase 3 は「動的アクセスを楽にする」ではなく「**境界でレコードに落とすのを一手に
する**」になる。

## zero-committed-shell との関係

重なっていない。

| | [zero-committed-shell](./zero-committed-shell.md) | 本項 |
|---|---|---|
| 向き | 内向き — このリポジトリの `.sh` を消す | 外向き — 利用者と LLM が書く言語にする |
| 対象 | 既存のゲート群 | 新規に書かれるスクリプト |
| 成否 | `git ls-files '*.sh'` が 0 | 書き味と採用 |

あちらが名指した二つのブロッカーは**両方 closed**（#1040 タイムアウト、#1041 effectful
combinator）。実行可能性はあちらが測定で示した — 7 ゲート・約 1,300 行の bash をバイト一致で
移植し、その過程で**欠陥を 8 件**発見している。本項はその上に乗る。

なお、あちらの文書が記録する `.sh` 台帳は **50 ファイル / 6,159 行**だが、現在の実測は
**124 ファイル / 13,579 行**（うち `scripts/` に 79）。ratchet が効いておらず逆方向に進んで
いる。あちらの done-criteria の問題なので本項では扱わないが、事実として記録しておく。

## Phase

### Phase 1 — `table` モジュール

`List[A]` を罫線テーブルに描く。これ一本で「Nushell っぽさ」の体感が最も上がり、かつ
言語表面に一切触らない。

実装機構は既にある: `A: Codec` なら `A.encode` で `Value` になり、`value.keys` で列名が取れる。
つまり純 Almide で `stdlib/table.almd` として書ける。

```
table.print(rows)            // 罫線つき
table.render(rows) -> String // 文字列を返す形
table.print_with(rows, cols) // 列を選ぶ
```

MSR 上の意味は見た目ではない。これが無いと **LLM は毎回ハンドロールのパディングループを書く** —
桁計算と全角幅の扱いを含む、修正に対して極めて脆いコードである。族ごと消す。

`stdlib/` 追加の手順は [CLAUDE.md](../../CLAUDE.md) の Testing Rules に従う
（self_host_registry 登録、`spec/stdlib/` のテスト、3-way oracle のブリッジ）。

### Phase 2 — REPL を `almide-interp` に載せ替える

[#1490](https://github.com/almide/almide/issues/1490) が既に「大きな UX 改善、小さな変更、
しかもオラクルを実入力で dogfood できる」として自力で 2 位に置いている項目。`crates/almide-interp`
に 12,005 行のツリーウォーク評価器が既にあり、リンク済み IR 上で走る。

現状の REPL は入力ごとにセッション全体をパイプラインに通す O(n²) 構造である。ただし
**実測では 80 定義まで約 19ms/行で平坦**で、固定オーバーヘッドが支配的なうちは二次項が
見えていない。つまりこれは「いま遅くて困っている」案件ではなく、**行編集が無いことの方が
体感の支配項**である。着手時は interp 載せ替えと行編集（rustyline 相当）を同じ単位で入れる。

`process.exec` を含む式で wasm 脚が降りて 0.82s になる段差も、interp 経路なら解消する。

### Phase 3 — 境界パースを一手にする

外部コマンドの出力を、一手でレコードにする。

```
process.exec_json[T](cmd, args) -> Result[T, String]   // T: Codec
text.columns(s) -> List[List[String]]                   // JSON を話さないコマンド向け
```

前者は `exec` → `json.parse` → `T.decode` の合成にすぎないが、**この三手こそが「型を付けるのは
面倒だ」と感じさせて動的アクセスへ逃がす原因**である。一手にすれば Codec が既定路線になる。
本項で最も MSR に効くのはここ。

### Phase 4 — 遅延パイプライン合成（保留）

`A | B` のストリーミング。zero-committed-shell が「まだどのゲートも必要としていない」と
記録しており、本項も需要が出るまで着手しない。大きな中間出力を扱う実例が一つ出たら起こす。

## 採らないもの（記録として）

- **対話シェル層** — 上記の理由により。
- **動的行アクセスの糖衣** — 上記「動的行は入れない」により。反 MSR。
- **トップレベル文**（`fn main` を不要にする） — 5 行スクリプトに 3 行の儀式が要るのは事実だが、
  **LLM はボイラープレートを苦にしない**。これは人間の書き心地の項目であって MSR の項目ではなく、
  言語表面を触る対価に見合わない。人間側の不満が実測で出てきたら再評価する。

## Done-criteria

- `table` が stdlib にあり、`spec/stdlib/` のテストと 3-way oracle のブリッジを持つ。
- REPL が rustc/cargo を一切経由せず、行編集（履歴・補完・複数行）を持つ。
- `exec` した外部コマンドの JSON 出力が**一手で**型付きレコードになる。
- 上記を使った実寸のスクリプト例が [CHEATSHEET.md](../CHEATSHEET.md) に
  ```` ```almide check ```` 付きで載っている（#1483 の規則により、載せた時点で約束になる）。

## Risks

- **R1 — 対話シェルへのスコープ漏れ。** テーブルが出ると「あとはプロンプトだけでは」と
  必ずなる。吸収: 本項の決定が境界線であり、越えるなら MSR デルタの見積りを添えて
  この文書を書き換えること。
- **R2 — `table` が表示の都合で型表面を歪める。** 吸収: `table` は `Codec` の上にのみ立ち、
  レンダリングのために型側へ要求を足さない。
- **R3 — Phase 3 の糖衣が Codec を迂回する抜け道になる。** 吸収: `exec_json[T]` は
  `T: Codec` を要求する形でのみ入れる。`Value` を返す版は作らない。
