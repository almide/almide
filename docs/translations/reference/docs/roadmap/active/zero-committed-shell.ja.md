原文の説明メタデータ：Almide に属するゲートと bash に残すゲート、およびその境界の維持
<a id="the-gate-language-boundary"></a>

# ゲートの言語境界

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](zero-committed-shell.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](zero-committed-shell.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](zero-committed-shell.zh-CN.md)

> 原文コミット固定の非規範閲覧用スナップショット。原文を正本とし、自動同期しない。独立した実行、実装検証、証明検査、測定は行っていない。歴史的コマンドや設計指示は資料であり、実行した指示ではない。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/zero-committed-shell.md)

> 原文注記：目標変更と歴史的測定の記録であり、本翻訳で実行した指示ではない。移植時のローカルなバイト一致受入条件と、未実装の継続的 CI twin/原本/生成物比較は別で、後者が完了したとは主張しない。reads 分類・サブコマンド列挙の実装と、なお必要な twin 昇格も別。件数・費用・欠陥・旧 CI 位置は原文時点の観測であり最新棚卸しではない。ここでは50 + 81 - 7 = 124であり、追加数は純増ではない。旧 timeout 名と保証は現行参照の限定と合わせて読み、発火時の挙動一致は host 間での発火判断一致を保証しない。

> **2026-09-12 に目標を差し替えた。** 旧目標は「このリポジトリに `.sh` を1本も置かない」。
> 六週間の実測がそれを否定したので、線を引き直した。経緯と旧目標の記録は下の
> 「なぜ目標を差し替えたか」に残してある。ファイル名は参照を切らさないため据え置き。
>
> 外向きの対の項目: [shell-scripting-surface.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/shell-scripting-surface.md)
> （利用者と LLM が書く言語としてのシェルスクリプト）。本項は内向き
> — このリポジトリ自身のゲートをどの言語で書くか。

<a id="決定"></a>

## 決定

**構造化データを読むゲートは Almide で書く。grep して終わるゲートは bash のままでよい。**

「committed shell をゼロにする」ではない。ゼロは目標として間違っていた。

<a id="なぜこれ以外にないのか"></a>

## なぜこれ以外にないのか

決め手は好みではなく、**限界コストと再利用の非対称**である。

bash のゲートは1本あたりが安い。新規ファイル、CI に1行、`proofs/gate-verification.toml`
に1行 — **触るのは3箇所**で、ファイルは完全に独立している。その代わり**再利用がゼロ**で、
60本の `scripts/check-*.sh` はそれぞれ独立に `grep` と `awk` と `sed` を書き直している。

Almide 側は初期コストが高かった。`tools/almide-gates/src/toml/mod.almd` は 291 行の
サブセット TOML リーダ、`src/mdmeta/mod.almd` は 68 行の markdown フロントマター読み。
bash なら `grep` 三発で済むものを、型の付いたパーサとして書き起こしている。

**その投資はもう払い終わっている。** 次に TOML や markdown や台帳を読むゲートを書く人は、
359 行のパーサをタダで使える。つまり構造化データを読むゲートに限れば、**いまや Almide の
ほうが安い**。そして Unit 0.46 の移植で見つかった欠陥 8 件は、全部この種の「判定ルール」の
側に出ている（後述）。

逆に `grep -q foo file || exit 1` の形のゲートには、この投資が一切効かない。触る箇所が
3 から 5〜6 に増え、共用の 245 行ファイルを編集し、得るものが無い。**そこに線がある。**

<a id="線の定義"></a>

## 線の定義

| ゲートが読むもの | 言語 | 例 |
|---|---|---|
| 構造化データ（TOML / markdown / 台帳 / 生成物の突き合わせ） | **Almide** | contracts.toml の整合、ロードマップ README の再生成、台帳カウント |
| 文字列の有無・単純な形（grep して落とす） | **bash** | 禁止綴りの検出、退役 API の再出現、ファイル存在 |

判定に迷うものが出たら、それは「構造化」側に倒す。パーサを書いているなら構造化である。

<a id="どう守るか"></a>

## どう守るか

規則だけでは守られない。それがこの六週間の実測結果そのものである（下記）。

`proofs/gate-verification.toml` を使う。行数は台帳を正本とし、各行に `path` /
`class`（KERNEL_PROVEN / MUTATION_TESTED / NEGATIVE_TESTED / EXERCISED / UNVERIFIED）/
`evidence` を持ち、`scripts/check-gate-verification.sh` が列挙・欠落・陳腐化を落とす。
UNVERIFIED は **縮小のみ許す上限**で burn-down される。

分類フィールド `reads` は実装済みで、`structured` / `scalar` / `UNCLASSIFIED` を取る。
**`reads = "structured"` と宣言された `.sh` は検査で拒否する。**

- `UNCLASSIFIED` は台帳の上限で縮小管理される。固定の 79 行という旧値を複写しない。
- 新規ゲートは宣言必須で、欠落・未知の分類・根拠不足を検査する。
- `tools/almide-gates/src/main.almd` のサブコマンドも
  `tools/almide-gates:<cmd>` として列挙済み。導入そのものは残件ではない。

**`.sh` の本数を数える ratchet は作らない。** 旧目標の done-criteria に書かれていたが、
grep-and-exit のゲートは bash のままで正しいと決めた以上、本数は正当に増える。本数は
もう指標ではない。

<a id="twin-は検証してから昇格する"></a>

## twin は、検証してから昇格する

Unit 0.46 は 7 本のゲートを Almide に移植した。その 7 本 — `contracts-readme` /
`conformance` / `roadmap-readme` / `check-contracts` / `output-parity` / `stamp` /
`fuzz-track-record` — は `tools/almide-gates/` に存在し、`almide test` でユニットテストが
走る。

**しかし `.sh` 原本との突き合わせは存在しない。** CI が実際に走らせているのは `.sh` の側
だけで（`ci.yml:1132`、`ci.yml:1240`、`ci-cross-target.yml:142`)、ライブのゲートとして
使われている twin は `bench` の 1 本だけである。`main.almd` のバイト一致テストは
`docs/contracts/conformance.md` と `docs/contracts/README.md` の 2 本を覆うのみ。

この文書は以前「バイト差分検査は CI で動く」と書いていた。**書いていただけで、実装されて
いなかった。** 記録として残す — リスク吸収策を宣言して実装しないのは、宣言しないより悪い。

だから順序を固定する:

1. **diff を作る。** 該当ゲートについて「Almide twin の出力 == `.sh` 原本の出力 ==
   コミット済み生成物」を CI で検証する。
2. **緑を確認する。**
3. **そこで初めて昇格する。** Almide 側をライブのゲートにし、`.sh` を消す。

根拠なしに `.sh` を消してはならない。壊れても気づけないからである。

**1 本目は `roadmap-readme`。** 7 本で最小・最安全（markdown を読んで表を出すだけ、副作用
なし）で、かつ `main.almd` のデフォルトサブコマンドでありながら検証が一つも無い。そして
`docs/roadmap/README.md` は実際にドリフトしていた（2026-09-12 に 8 項目の欠落を発見）ので、
この 1 ステップで **README の staleness と twin の無検証が同時に片づく**。

<a id="なぜ目標を差し替えたか"></a>

## なぜ目標を差し替えたか

旧目標は「`git ls-files '*.sh'` が 0」だった。2026-08-01 の起草時点で 50 ファイル /
6,159 行。

**2026-09-12 の実測: 124 ファイル / 13,579 行。**

| | |
|---|---|
| 追加 | 81 |
| 削除 | 7 |
| **うち `scripts/check-*.sh`** | **50** |

新しい CI ゲートは、六週間のあいだ**一本残らず bash で書かれた**。移植した 7 本に対して、
新規流入が約 7 倍。ポートが遅かったのではなく、蛇口が開いたままだった。

そして誰も気づかなかった。旧 done-criteria の「本数が増えるとゲートが CI を失敗させる」が
**実装されていなかった**からである。

ここから読めることは二つある。一つ、**規則を掲げるだけでは守られない** — だから上の
「どう守るか」は台帳による宣言と機械検査にした。二つ、**六週間の実際の選択は、書き手が
毎回正しく判断した結果かもしれない** — grep 一発のゲートに Almide を使う理由は本当に無い。
ゼロという目標のほうが現実と合っていなかった。

<a id="残すもの--unit-046-の測定"></a>

## 残すもの — Unit 0.46 の測定

目標は差し替えたが、この項目の最も価値ある部分は測定であり、そこは変わらない。

7 本のゲートを **byte-identity を受け入れ条件として**移植し、最後の 1 本は追加で
**15 の mutation** に対しても検証した。

| | |
|---|---|
| 置き換えた bash | 7 ゲート、約 1,300 行 |
| 書いた Almide | 約 2,300 行、14 モジュール |
| テスト | 47。bash では I/O 経由でしか到達できなかった判定ルールに対して |
| 移植が見つけた欠陥 | **8** |

8 件のうち **5 件が同じ形**だった — **抽象化より複製のほうが安いから複製され、書いた日には
全部が一致していた規則**。11 のスクリプトに散ったロケール固定、5 つの抽出器に散った
unquote 式、2 箇所に書かれた evidence クラスの enum。

この形は書き手の怠慢ではない。shell にはモジュールも型検査も無いので、**複製が本当に最も
安い選択肢**である。移植が見つけられたのは賢かったからではなく、byte-diff が
「もっともらしい言い換え」を拒否したからである。

<a id="タイムアウト--当初の経緯と現在の境界"></a>

## タイムアウト — 当初の経緯と現在の境界

現在の子プロセス期限は `ProcessCommand.timeout_ms`（[process reference](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/process.md)）。
`fan.timeout(duration.ms(n)) { body }` も oracle 層として存在し、撤去されたままではない。
決定的な計算量上限は `fan.bounded(compute.ms(n))`、clock による選択は
[ADR-0028](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0028-fan-selection-names-and-cost-contract.md) の `fan.min` を使う。
以下は #1040 時点の設計経緯であり、現行構文の可否を決める表ではない。

`output-parity` はサブプロセスに制限時間をかける必要があり、当時 `perl -e 'alarm …; exec …'`
に外注していた。当時 `fan.timeout` は 0.29.0 で撤去されていた — **純粋な計算**に制限時間をかけると
結果が機械の関数になり、native ⇄ wasm のバイト一致が壊れるからである。

引ける線はこうだった。**制限時間が許されるのは、それが囲む操作が既にバイト一致契約の外に
ある場合に限る。**

| 囲む対象 | 既に非決定的か | 制限時間 |
|---|---|---|
| `process.exec_status`（プロセスを起動する） | yes | **可** |
| `http.get`（ネットワーク） | yes | **可** |
| パイプ / ソケットの読み取り | yes | **可** |
| 純粋な `fan` の兄弟 | **no** | **不可 — 0.29.0 が撤去したのはこれ** |

契約は二つに分けて述べられる:

> **バイト一致を保証する**: 制限時間が発火した場合、両ターゲットは同じエラー値、同じ
> メッセージ、同じ終了経路を出す。
> **保証しない**: 発火するかどうか。それはホストの関数である。

[#1040](https://github.com/almide/almide/issues/1040) で **着地済み** —
当時は `process.exec_status_timeout` として効果表面に入った。現在の名前と
`fan.timeout` の oracle 層への再導入は、この節の冒頭の現行参照に従う。

<a id="解けた障害"></a>

## 解けた障害

旧版が「残りを阻む二つの primitive」として挙げた 2 件は、**両方とも closed**。

- [#1040](https://github.com/almide/almide/issues/1040) — 上記のタイムアウト。着地済み。
- [#1041](https://github.com/almide/almide/issues/1041) — `list.map` の callback が純粋で、
  effect を含むパイプラインが `var` + `for` に落ちる問題。解決済み（ADR-0006、callback 内の
  `!` が fallible 形を instantiate する）。

ただし `tools/almide-gates/` には `var` + `for` が 13 箇所残っている
（`parity_sweep.almd:71-79` ほか）。これは #1041 以前に書かれたコードであって、言語の制約
ではない。書き換えは摩擦の実測と合わせて進める。

<a id="既知の摩擦almide-側でゲートを書くときのコスト"></a>

## 既知の摩擦（Almide 側でゲートを書くときのコスト）

線を引いた以上、Almide 側にゲートが増える。実測された摩擦を記録しておく。

- **新規ゲート 1 本で 5〜6 箇所**（bash は 3）。うち一つが**全ゲート共用の 245 行
  `main.almd`** — import の追加、`cmd` の match アーム追加、root デフォルトの match アーム追加。
- **罠**: ゲート型（終了コードを返す）のコマンドは、`main.almd` 末尾のジェネレータ向け
  `println` を避けるため、アームの中で `process.exit` しなければならない。知らないと踏む。
- **雛形が無い**（bash 側にも無い）。
- **CI で Almide のゲートを動かすと Rust toolchain が要る**。bash は何も要らない。
- `list.sort_by` は比較器ではなくキー抽出器を取り、`string.compare` が無いので降順は
  「昇順にして reverse」になる（`main.almd:70` に記録あり）。

これらは**想像で先に直さない**。次に書く 1 本（`roadmap-readme` の diff）で実際に踏み、
踏んだ順に直す。

<a id="done-criteria"></a>

## 完了条件

- `proofs/gate-verification.toml` に分類フィールドがあり、構造化と宣言されたゲートが
  bash で書かれていたら CI が落ちる。`UNCLASSIFIED` の上限は縮小のみ。
- 台帳の列挙が `tools/almide-gates/` のサブコマンドを含む。
- 7 本の twin それぞれについて「Almide 出力 == `.sh` 出力 == コミット済み生成物」の diff が
  CI にあり、緑。
- 緑が確認された twin から順に昇格し、対応する `.sh` が消えている。
- 新規に追加される構造化ゲートが Almide で書かれている（台帳が強制する）。

<a id="risks"></a>

## リスク

- **R1 — 移植のための移植。** 誰も走らせないスクリプトはプログラムにする価値がない。
  吸収: 線の定義が範囲を決める。grep-and-exit は対象外であって、移植しないことが正解。
- **R2 — twin が `.sh` オラクルから乖離する。** 吸収: **diff を実装する**（旧版はこれを
  宣言だけして実装しなかった。それが R2 が現実になった経路である）。diff が緑になるまで
  `.sh` は消さない。
- **R3 — 決定的な計算量上限と環境期限を混同する。** 吸収: `Compute` と
  `Duration` の契約を区別し、壁時計の発火有無をターゲット不変とは主張しない。
  現行の SPEC / ADR-0028 を正本にし、0.29.0 の撤去を現在の禁止と読み替えない。
- **R4 — 線が判定不能になる。** 「構造化か grep か」が毎回議論になると、台帳のフィールドが
  形骸化する。吸収: 迷ったら構造化に倒す、という既定を上に書いた。パーサを書いているなら
  構造化である。
