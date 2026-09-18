<!-- description: 全軸制覇への突破点台帳 — 近隣アリーナの実測と 18 近隣コンパイラの機構読みから、どの軸で負けていて何を突破すれば世界一かを、勝利条件・ゲート・順序つきで固定する -->
# 全軸制覇への突破点 — アリーナと近隣比較から導いた設計指針

> **位置づけ**: [ALL_AXES.md](../ALL_AXES.md)（6 軸の勝利条件、2026-08-13）の**後継デルタ**。
> 軸の定義と「降りる軸」はそちらが正で、本書は **2026-09-13 時点の実測順位**と、
> 順位を変えるために**突破しなければならない点**だけを書く。台帳の decade 割付は
> [ROAD_TO_1_0.md](../ROAD_TO_1_0.md)。
>
> **出典**（全て実測、日付つき）: `../almide-references/RESEARCH-neighbours.md` §7–§16
> （2026-09-06〜09-11 の全 field アリーナ、15 ツールチェイン）、同 `RESEARCH-championship.md`
> （2026-08-16、9 コンパイラ深読み — 本書で **2026-09-13 に全 18 項目を再検証**）、
> 近隣 8 言語のクローン深読み（2026-09-13、file:line 引用は本書末尾の付録）、
> stdlib 連結レジームの追加計測（2026-09-13、§2.4）。
>
> **本書の原則は ALL_AXES と同じ二つ**: 「勝利条件を数字で書けない軸には着手しない」、
> 「勝ったと書けるのは CI ゲートかラチェットがその主張を守っているときだけ」。
> 加えて本書が足す原則が一つ: **敗北と未計測を同じ表に載せる**。未計測は勝ちではない。

---

## 0. 一枚の結論

**Almide は今日、証拠の軸（証明・台帳・ゲート・検証済み成果物）で無競合の首位、
実行性能とサイズの軸で首位圏、そして自分のテーゼの軸（LLM writability）で
"測っていない首位" にいる。** 星の数（34）はカテゴリ最下位で、これは症状であって原因ではない。

世界一を「全軸で第三者が再現できる首位」と定義すると、突破点は 9 つに絞れる。
致命（負けたらテーゼが立たない）が 3、優勢を確定させるものが 4、防衛が 2。

| # | 突破点 | 軸 | 種別 | 今日の順位 | 勝利条件（数字） |
|---|---|---|---|---|---|
| **T1** | MSR を土俵にする — 公開ハーネス・弁別する課題集・他言語同条件 | D | **致命** | 首位を主張、**測定 5 ヶ月古い** | 第三者が 5 言語を同条件で走らせ、Almide が首位で、その数字が README にラチェット付きで載る |
| **T2** | 生存デルタ API — 編集を適用する前に「この編集は生き残るか」を返す | D/E | **致命** | 誰も持たない（近隣 1 社が契約層で保有） | `almide survive`（LSP/MCP/CLI）が check+test+contract の差分を返し、Dojo のループ 1 回あたり試行数が 20% 減る |
| **T3** | 診断 → 修復の構造化 — 修復フィールド必須・機械適用 4/73 → 全 mechanical | D/E | **致命** | 2 位（rustc の後ろ）、機械修正は **4 code** | `## Fix-it verdict: mechanical` を宣言する全 code が `MachineApplicable` を emit、silent ケース付き修復ラチェット緑 |
| **T4** | stdlib が自分の抽象機構を使う — 123 の `_str` 双子を protocol で退役 | D/F | 優勢確定 | **10 位/10**（唯一 0 件） | `_str` 双子ラチェット 123 → 0、`protocol` 宣言が stdlib に ≥ 5 |
| **T5** | 推奨イディオム = 最速 — 綴りゲートの全面化、fusion 点火、fan 実並列、Map の線形走査 | B | 優勢確定 | 床では首位、**推奨形は 1.4–4.3× 遅く、stdlib 連結の native は Map 3.7× / JSON 2.2× 負け**（§2.4） | perf 全 13 行で idiomatic/imperative ≤ 1.15、wasm ratchet が CI で裁く、stdlib 連結行 3 本が ratchet 下 |
| **T6** | サイズに物語を持つ — native 488 KB（7–9× 負け）、float printer 3.5 KB、size ratchet 無し | B | 優勢確定 | wasm 首位 / **native 中位** | native hello < 200 KB の `--release-small`、wasm size ratchet が CI で裁く、float 印字 ≤ 2.5 KB |
| **T7** | インクリメンタル — `.almdi` を読む者がいない、native 134 ms は go の 92 に負ける | A | 優勢確定 | **10 位/10**（唯一 0） | 未変更モジュールの再 check が 0 IR、native 編集ループ p95 < go の同規模ビルド |
| **T8** | 信頼の可搬性 — 検証器を別バイナリに、配布に署名、抑制は非沈黙、intrinsic に権限 | C | 防衛 | 首位、ただし**輸出形式が無い** | `almide-verify` が独立版数で証明書を裁く、install 一行に attestation、798 intrinsic の宣言に権限ゲート |
| **T9** | エコシステムの非対称 — エージェントが書き、ゲートが通し、人が承認する公開経路 | F | 防衛 | issue **0 件**、docs は on-hold で陳腐化 | 経路を通った外部パッケージ 10 本（ALL_AXES 軸 F と同一） |

T1–T3 は同じ一つのこと — **「LLM が書ける」を第三者が測れる数字にし、その数字を毎編集で動かす機構を言語に内蔵する** — を三段で書いたもの。
T1 なしに T2/T3 は主張であり、T2/T3 なしに T1 は他言語に追いつかれる（実際、近隣の一言語が
MSR ハーネスを名指しで移植済み: 付録 A-5）。

---

## 1. 今日の順位表（2026-09-13、全て実測）

### 1.1 アリーナ（§13/§14、M4 Pro、出力照合済み、median 9）

**native、ms**（低いほど良い）

| | hello | fib(35) | binarytrees d19×12 | spectralnorm n=1200 |
|---|---:|---:|---:|---:|
| **Almide（命令形）** | 2.3 | **15.4** | **51.4** | 39.3 |
| **Almide（推奨イディオム）** | — | — | 49.7 | **62.9** |
| Rust | 2.0 | 18.3 | 193.7 | 38.8 |
| MoonBit | 2.0 | 19.1 | 62.2 | **38.0** |
| Zig | 2.2 | 19.7 | 66.5 | 38.1 |
| Go | 2.7 | 23.2 | 63.4 | 54.0 |
| Jacquard（C 経由） | 1.9 | 50.7 | 61.8 | — |
| Aver（Rust 経由） | 2.1 | 79.1 | 287.9 | 836.2 |
| 手書き Rust arena（対照） | | | **31.3** | |

読み: binarytrees だけが**機構の差**（region window、#1991）で、他は騒音圏の順位。
**推奨イディオムで書くと spectralnorm は Go に負ける**（§15、#2098 は 09-12 に解消 — 再計測が T5 の最初の仕事）。

**stock-WASI、ms / bytes**

| | fib | binarytrees | spectralnorm | hello B (+opt) | fib B | btrees B | snorm B |
|---|---:|---:|---:|---:|---:|---:|---:|
| **Almide** | 37.0 | **67.1** | **82.3** | 1,337 (**407**) | **1,144** | **1,924** | 8,585 |
| MoonBit | 36.9 | 195.3 | 93.5 | 2,522 (2,106) | 3,797 | 3,826 | 11,198 |
| Zig（wasm、§11） | **41–45** | 124–126 | — | 46,640 | 47,651 | 48,132 | — |
| Wado `-Os`（own host） | — | — | — | 2,013 | 2,524 | 2,648 | **7,185** |
| vibe（own host） | — | — | 不能（heap 4.4 GiB） | 519 | **823** | 2,164 | — |

**native バイナリ、KB**: Swift 51（動的リンク）· **Jacquard 53** · MoonBit 196–219 · Zig 387–404 · Rust 455–475 · **Almide 457–495** · Aver 467–557 · Go 2,373。

**コンパイル、ms（cache を確実に外した 1 ファイル）**: Aver→Rust source 6 · **Almide→Rust source 23** · **Almide→wasm 33** · vibe→wasm 56 · rustc -O 59 · MoonBit→wasm 61 · go build 92 · **Almide→native 134** · swiftc 135 · Wado 183 · MoonBit→native 304 · Jacquard 682 · Gleam 728 · Grain 1,918 · Zig 3,612。

### 1.2 工学的性質（championship 18 項目、2026-09-13 再検証）

| 項目 | 08-16 の順位 | 09-13 の状態 | 証拠 |
|---|---|---|---|
| 機械検査済みの自己証明 | 1 位/10 無競合 | 維持 | Coq 25 ファイル 0 Admitted、Lean 3 ベルト 0 sorry |
| compile-speed ゲート | 1 位 | 維持 | `check-edit-loop-scale.sh` 傾き 1.13 |
| 検証済み出荷成果物 | 1 位（構造的） | 維持 | wasm-opt は検証包絡の外と明記 |
| fmt の AST 保存検証 | 1 位 | 維持 | `verify_format` 常時 on |
| MCP / agent 面 | 1 位（0/9 が保有） | 維持、ただし近隣が追いついた（付録 A） | `almide mcp` 5 ツール |
| 診断規律 | 2 位 | 維持。**機械修正 4 code / 73** | `with_machine_fix` 非テスト呼び出し 6 箇所 |
| `list.sort` O(n²) | 10 位 | **解消** | 6 ファイル全て bottom-up mergesort |
| パターン言語（or/as/rest） | 10 位 | **解消** | `ast.rs:110-136` `As` `Or` `List{rest}` |
| MVS | 8 位（文書のみ） | **解消** | `project_fetch.rs:245-313` semver 最大選択 |
| lock 形式 | 手書き行形式 | **解消** | TOML、引用符付き |
| snapshot / `test --json` | 未内蔵 | **解消** | `snapshot.rs`、`test_report.rs:223` |
| ablation レッグ | 無し | **解消** | `VICTORY_ABLATION_FLOOR`、`ALMIDE_REGION_OFF` |
| **インクリメンタル** | **10 位（唯一 0）** | **未着手** | `read_almdi` 呼び出し **0** |
| **stdlib の protocol 使用** | **10 位** | **逆行**: `_str` 双子 119 → **123** | ラチェットは凍結、退役 2 案とも stage 0 |
| LSP 幅 | 9 位 | 部分解消: 13 メソッド、INCREMENTAL sync、references/rename 有 | code action **1 種**（Gleam ≈ 60） |
| doc generator | 8× 薄い | `--check` のみ、46 ファイル手書き | #1469 |
| intrinsic 境界マーカー | 無し | link ゲートのみ、**宣言権限ゲート無し**（798 宣言） | `check-intrinsic-boundary.sh` |
| native サイズ設定 | — | `strip`/`panic=abort`/`opt-level=z` **どれも無し** | `cargo_build.rs:38-45` |

### 1.3 テーゼの軸（LLM writability）

| 指標 | 値 | 鮮度 | 問題 |
|---|---|---|---|
| Dojo MSR | 100%（30/30、Sonnet 4.6） | **2026-04-12** | #1963 P-critical: 5 ヶ月古く、しかも saturate |
| MiniGit 同一モデル | 20/20、233 LOC（5 言語中最小）、**573 s / $3.19** | 2026-07-15 | Rust 297 s / $1.45、Ruby 73 s / $0.36 — **8.9× 高く 7.8× 遅い** |
| 近隣の反応 | vibe が MSR ハーネスを名指しで移植 | 2026-09 | 指標を独占していない。土俵は今が作り時 |
| 星 | almide 34 · vibe 35 · aver 60 · wado 116 · jacquard 119 · hexa 196 · vera 413 · moonbit/core 1,205 · gleam 21,907 | 2026-09-13 | カテゴリ最下位 |

573 s / $3.19 は「訓練データが無い言語を、コンパイラのループだけで書かせた」費用そのものである。
**この費用を下げる機構が T2/T3 であり、下がったことを示す数字が T1 である。**

---

## 2. 敗北と未計測の全件台帳

「勝った」の裏側。各行に、原因・所在・担当 issue を付ける。issue が無い行は §4 で起票する。

### 2.1 実測で負けている行

| 行 | 相手 | 差 | 原因（実測） | issue |
|---|---|---|---|---|
| native binarytrees | 手書き Rust arena 31.3 ms | 1.6× | 推論 arena は区間を 1 回巻き戻すだけ。手書きは `Vec<u32>` インデックス | region-inference.md Phase 3（on-hold）; #1997 `scoped:` |
| native spectralnorm（推奨形） | Go 54.0 | 1.16× | 捕獲 list の per-closure clone + `enumerate` の tuple 実体化（#2098、解消済 — **再計測未**） | T5 |
| wasm fib | Zig 41–45 ms | 1.2–1.3× | 呼び出しコスト。構造レッグの call/return 形 | #2150 |
| wasm spectralnorm bytes | Wado `-Os` 7,185 | 1.19× | **float printer が +3,531 B**（§16）。math は `f64.sqrt` で無料 | #2099 |
| wasm fib bytes（own-host 級） | vibe 823 | 1.39× | 級が違う（own host import）。ただし差の中身は未分解 | #2150（級差の分解は T6） |
| native バイナリ | Jacquard 53 KB | 7–9× | backend 由来（bare rustc hello 466 KB）。`strip`/`panic=abort`/`opt-level=z` 未設定 | #2092（P-low → 昇格） |
| native コンパイル | go 92 ms | 1.46× | rustc がパスに居る。0.6x（cranelift）が答え | #1005 |
| Rust source 生成 | Aver 6 ms | 3.8× | 23 ms のうち frontend 以外の内訳未計測 | #2151 |
| エージェント費用 | Ruby $0.36 / 73 s | 8.9× / 7.8× | 訓練データ 0 をコンパイラループで補っている | #1963、T2/T3 |
| 星 | gleam 21,907 | 640× | 配布と物語の不在（§3 T9） | T9 |

### 2.2 実測で未計測の行（勝ちではない）

| 行 | 状態 | 誰が埋めるか |
|---|---|---|
| stdlib 連結レジーム（Map / String / JSON）の速度とサイズ | §16 が「両方向で未計測」と明記。ratchet は 13 行中 7 行しか見ない（#2142）、size ratchet 無し（#2141） | **§2.4 で初回計測済** → #2154 #2155 #2156 #2157、T5/T6 |
| 他言語同条件の MSR | 自分だけが測っている | T1 |
| effect 宣言の MSR 効果 | dojo#2 が文献初の数字になるはずだが未実施 | T1 |
| 他言語との**コンパイル時間**比較（等 LOC） | 軸 A の勝利条件は自己相対のみ | T7 |
| wasm perf の穴（fft の list-write 崖 ~3,500×、mandelbrot ~130×） | perf README「先に直す必要」— **issue 無し** | T5 |
| `AlmideMap` の線形走査 | perf README「Not yet covered」、map-data-structure-roadmap は HAMT を research 扱い | T5 |
| 推奨イディオムが 13 行全部で命令形と同速か | `IDIOM_CEILING=1.15` は listbuild 1 行だけ | T5 |
| 4 プログラム中 3 つが 2.7 KB 未満 = 床の測定 | 「小さい」は 4 ファイルの観察 | T6 |

### 2.3 近隣が持っていて Almide に無い機構（付録 A の要約）

| 機構 | 保有者 | Almide の状態 | 吸収先 |
|---|---|---|---|
| 投機編集の**証明デルタ**（適用前に newly_broken/newly_fixed） | 1 社（LSP 独自メソッド） | 無し | **T2** |
| 検証と適用を一つの呼び出しに融合（`force` 明示） | 同上 | 無し | T2 |
| 修復ラチェット（`diag.grep` + **silent ケースは診断が出たら FAIL**） | 1 社（Almide の MSR を移植した側） | `tests/diagnostics` は正例のみ | **T3** |
| 診断の必須構造フィールド（cause / next-step ただ一つ / contrast） | 1 社（224 code） | hint は String で必須、修復は散文 | T3 |
| `Repair{primary, alternatives, example}` を JSON 診断に | 1 社（Rust 経由の直接競合） | `MachineApplicable` 4 code | T3 |
| `explain --diagnostic` 無引数で**全 code 列挙**（mnemonic + 状態） | MoonBit | `almide explain E0xx` は個別のみ | T3 |
| LSP 機能を CLI サブコマンドで（hover/definition/references/inlay-hints） | 1 社 | LSP のみ | T2 |
| 言語ガイドをバイナリに `include_str!`、マーカー所有の冪等書き出し | 1 社 | `llms.txt` はリポジトリ側 | T9 |
| 非沈黙・期限付きの抑制（毎コンパイル再告知 + commit trailer） | 1 社 | 抑制構文自体が無い（良いが、来たときの形を先に決める） | T8 |
| 独立版数の証明書検証バイナリ | 1 社 | 検証器はコンパイラ同梱 | T8 |
| install 一行に attestation / SHA256SUMS | 1 社 | 無し | T8 |
| capability を build 時に const-fold + DCE（未許可はバイナリから消える） | 1 社 | check 時 deny-all のみ | T8 |
| 公開インタフェース ファイル（`.mbti` / package-interface JSON） | MoonBit、Gleam | `almide compile --json` は有、**diff 対象として checked-in されていない** | T7/T9 |
| 生成系 code action（missing patterns / decoder 生成 / labels 埋め） | Gleam（≈ 60） | 1 種 | T3 |
| mutation score を公開バッジに | 1 社 | `check-mutation-gate.sh` は有、公開値無し | T8 |
| hosted registry（アカウント lifecycle を CLI に） | MoonBit、Gleam、Wado（OCI） | git deps のみ | T9 |

### 2.4 stdlib 連結レジームの初回計測（2026-09-13）

3 プログラム（wordfreq = 2M 語を Map で数えて top 10 / strchurn = 20 万文字列の補間→split→trim→to_upper→join /
jsonround = 5 万レコードの stringify→parse→Σ）を Almide・MoonBit・Rust で書き、全 26 セルで stdout バイト一致を確認してから
計測（M4 Pro、median 7、almide 0.62.0、wasmtime 47）。コーパスと runner は
`../almide-references/bench-sources/stdlib-regime/`、読みは RESEARCH-neighbours §17。

**native、ms**

| | Almide 推奨形 | Almide 命令形 | MoonBit | Rust |
|---|---:|---:|---:|---:|
| wordfreq | 507 | 404 | **136** | **79** |
| strchurn | **166** | — | 216 | **82** |
| jsonround | 307 | — | **137** | **129** |

**stock-WASI、ms / bytes（verified → `--wasm-opt`）**

| | Almide | MoonBit（→ `wasm-opt -Oz`） |
|---|---:|---:|
| wordfreq 推奨形（`group_by`） | **22,016** · 7,472 → 5,647 B | 262 · 16,132 → 11,380 B |
| wordfreq 命令形（`var` Map） | **201** · 8,210 → 6,239 B | （同セル） |
| strchurn | **166** · 28,153 → 25,713 B | 726 · 22,010 → 14,564 B |
| jsonround | **132** · 25,575 → 17,704 B | 477 · 71,395 → 51,590 B |

**読み（4 点、全部 issue 化済）**:
1. **native は §13 と逆転する**: stdlib を連結すると Map で MoonBit に 3.7×、JSON で 2.2×、Rust には全行 2–6× 負け。
   勝つのは strchurn だけ。「首位圏」は床プログラムと allocation 行の話だった。
2. **wasm は逆に決定的**: MoonBit に 1.3× / 4.4× / 3.6× 速く、bytes は 0.5–1.3×。「小さい」は連結レジームでも生き、「速い」は新事実。
3. **wasm の優位は native への告発でもある**: 同じプログラムが wasmtime 上で native の 1.5–2.3× 速い（wordfreq 404 → 201、
   jsonround 307 → 132）。Map 無しの対照ループが native 163 ms なので、native の `map.get_or` + 添字代入が 2M 回で
   ~240 ms — 構造レッグの Map はプログラム全体を 201 ms で終える。→ **#2157**（native Map 索引・Value 表現 #1679）。
4. **推奨綴りは wasm で罠**: `list.group_by` は構造レッグで 1 要素 ~11 µs（22 s 対 201 ms、N に線形、出力同一）→ **#2156**。
   タプル sort key `(0 - c, w)`（count 降順・word 昇順の唯一の綴り）は**構造レッグで wall、incumbent で miscompile**:
   `almide check --target wasm` は "No errors found"、build は "verified"、wasmtime は `list.sort_by_rc` で
   `indirect call type mismatch` trap、native は答えを印字 → **#2154（I-divergence、release blocker）**、
   wall と `list.group_by_x` 未 link は **#2155**。

**T5/T6 への帰結**: この 3 本を `research/benchmark/perf/` の ratchet 行に（#2142 が名指しする未監視 6 行そのもの）、
size 梯子（T6）の String / Map / JSON 段はこの 3 本で埋まる。「全軸で圧倒」の native 側の残債は**codegen ではなく
Map と Value のデータ構造**である。

---

## 3. 突破点の設計指針

各項: **なぜ致命か / 機構 / 勝利条件 / ゲート / 依存 / 起票**。既存の roadmap・issue が
カバーする部分はポインタで済ませ、本書が新規に決めることだけを本文にする。

### T1. MSR を土俵にする（致命）

**なぜ致命か.** ミッション文は「LLM が最も正確に書ける言語」で、その指標を自分だけが 5 ヶ月前に
測った。近隣がハーネスを移植した今、**定義者の座は「先に公開した側」に移る**。ROAD_TO_1_0 の
0.59 行は Issue 欄が `—` のまま（担当無し）。

**機構.**
1. **弁別する課題集**（#1963）: MiniGit は 5 言語 20/20 で saturate。課題は「初回実装」ではなく
   **「既存コードへの仕様変更 N 回の生存」**で、正解率が言語間で割れる難度に置く。難度の指標は
   「最強モデルで 60–80% が生き残る」。
2. **他言語同条件**: Dojo のランナーを言語プラグイン化（Almide / Rust / Go / TypeScript / Zig / Gleam / MoonBit）。
   Zig は std が 0.15→0.16 で動き、訓練データからの書き込みが失敗する（§7 実見）— MSR が
   「訓練データ量」と「言語の安定性」を分離できることの実証になる。
3. **第三者再現**: `git clone && make msr` 1 コマンド、モデル・プロンプト・温度・seed をマニフェストに、
   結果 JSON を dojo に commit。
4. **effect A/B**（dojo#2）を同じハーネスの 1 条件として同梱 — 文献初の数字。
5. **README 数字はラチェット**: `check-readme-numbers.sh` に msr 行を足し、鮮度 90 日を超えたら赤。

**勝利条件.** 第三者が 5 言語以上を同条件で走らせた表が存在し、Almide が首位で、README の数字が
90 日以内。

**ゲート.** Dojo の PR gate が本リポジトリの CI で subset を回す（CLAUDE.md 「The bridge」）。

**依存.** T2/T3 が数字を動かす側。順序は T1 の課題集 → T3 → T2 → T1 の再計測。

**起票.** #1963 / #1617 は課題集と再計測。**公開ハーネス（他言語同条件・第三者再現）**が未起票 → #2146。

### T2. 生存デルタ API（致命）

**なぜ致命か.** MSR を殺す失敗は「検証せずに適用した編集」。近隣の一言語は LSP に
`speculativeEdit`（適用前の証明デルタ）と `proposeEdit`（検証→適用の融合）を持ち、
これは**MSR を API にしたもの**である。Almide はこの形を持たず、`almide check` と `almide test`
を別々に呼ぶ。

**機構.**
1. `almide survive <file> --with <patch-or-full-text> --json`（CLI）: 現在のツリーに編集を**メモリ上で**
   当て、`check`（診断差分）・`test`（合否差分）・契約（`// @contract:` の fixture 差分）を
   `{unchanged, newly_broken, newly_fixed, removed}` で返す。書き込まない。
2. 同じ関数を **LSP 独自メソッド** `almide/survive` と **MCP ツール** `almide_survive` に載せる。
   MCP 5 ツールは全て既存 CLI の JSON をサブプロセスで読む方針（docs/mcp.md）なので、CLI が先。
3. `almide apply --if-survives`（`force` は明示、非 boolean は fail-closed）: 検証と適用を一手に。
4. `almide query hover|definition|references|inlay-hints --line --column --json`: 既存 LSP の
   ハンドラを CLI に露出。**エージェントは編集の前に型を知る**ことで失敗を診断ではなく予防する。

**勝利条件.** Dojo の 1 課題あたり平均試行回数が、`survive` を使う条件で使わない条件より 20% 少ない
（同一モデル、A/B）。

**ゲート.** `survive` の JSON schema に `schema_version`、fixture: 既知 3 編集の期待デルタを golden に。

**依存.** T3 の構造化診断が `newly_broken` の中身になる。

**起票.** 全て未起票 → #2147、#2148。

### T3. 診断 → 修復の構造化（致命）

**なぜ致命か.** ALL_AXES 軸 D が引く文献: フィードバック品質の序列は 複合 63.6% > テスト失敗 57.9% >
最小 53.1% > **生コンパイラエラー 49.2%**。Almide は hint を型で必須にした（`hint: String`）が、
**修復は散文**で、機械適用は 73 code 中 4（E013 E031 E049 E052）。`## Fix-it verdict: mechanical`
を宣言する doc は 6 あるが、gate は soft-report。

**機構.**
1. **修復フィールドをスキーマにする**: `Diagnostic { hint, repair: Option<Repair{primary, alternatives, example}> }`。
   `mechanical` verdict の code は `repair.primary` が `MachineApplicable` でなければ
   `diagnostic_coverage_test.rs` を **hard fail** に（現 `:234` の soft を外す）。
2. **修復ラチェット（両方向）**: `tests/diagnostics/<code>/repair.grep`（診断が含むべき語）に加え、
   `tests/diagnostics/silent/*.almd` — **コンパイルが通るべき**プログラム。新しい診断が出たら FAIL。
   「静かに受理される誤り」（#2097 の族、vera の右→左スロット、jacquard W0301 の族）を面で押さえる。
3. `almide explain --list --json`: 全 code を `code | mnemonic | severity | since | fix-it verdict` で列挙。
   `since` は版数（`_since` 表）。エージェントは code を知らずに orient できる。
4. **生成系 code action** を 1 種から増やす。Gleam の ≈ 60 のうち MSR に効く順: `Add missing patterns`
   （網羅性チェッカと同一ソース）、`Fill labels`、`Generate decoder`、`Convert to pipe`。
   on-hold/lsp-code-actions.md を active に戻す判断はこの行。

**勝利条件.** mechanical 宣言 code 100% が機械適用、silent ケース ≥ 30 本で緑、`explain --list`
の行数 = `with_code` の distinct 数（drift ゲート）。

**ゲート.** `check-diagnostic-code-coverage.sh` に repair/silent を追加。

**起票.** #2097 は 1 事例。スキーマ化・silent ラチェット・`explain --list` が未起票 → #2149。

### T4. stdlib が自分の抽象機構を使う（優勢確定）

**なぜ.** championship の言葉で「codebase 最大の MSR 欠陥」。`list.chunk_by` を足すモデルは
5 つの表現接尾辞のどれを書くか当てなければならない。数字は 119 → **123** に逆行、
退役 2 案（selfhost-link-v2.md、protocol-any-existentials.md）は共に stage 0。

**機構.** 本書は新機構を足さない。**順序と締切を決める**:
1. selfhost-link-v2 stage 1（parity harness = 反証器）を **T5 の綴りゲート拡張と同じ PR 群**で着地
   （どちらも「同じ workload の別綴りが同じ出力」を測る器具で、共有できる）。
2. `_str` ラチェットの baseline を**四半期ごとに −25%** 下げる shrink スケジュールを
   `scripts/str-twin-baseline.txt` の隣に書く（ラチェットは凍結だけでなく降下を強制）。
3. `protocol` の stdlib 初出は `Encode`/`Decode`/`Numeric` の**文書化**（protocol-any pre-step）から。

**勝利条件.** 123 → 0。`protocol` 宣言 ≥ 5 in stdlib。

**起票.** #1460 / #1589 が担う。shrink スケジュールは #1460 にコメントで追記。

### T5. 推奨イディオム = 最速（優勢確定）

**なぜ.** §15 の教訓: 「スタイルガイドは**何も測らない約束**」。CLAUDE.md が勧める綴りが 1.4–4.3× 遅く、
それを書いたコーパスがガイドに従っていたので誰も気づかなかった。#2098 は解消したが、
ゲートは listbuild 1 行だけ。

**機構.**
1. **綴りゲートの全面化**: perf 13 行のうち意味のある全行に `imperative` と `idiomatic` の 2 綴りを置き、
   `IDIOM_CEILING=1.15` を行ごとに適用。§15 の spectralnorm 4 綴りをそのまま `spectralnorm/` に常設。
2. **wasm ratchet を CI で裁く**（#2143）: `RATIO_VERDICT=0` の理由（クロスエンジン比は hardware を
   打ち消さない）は正しいので、**wasm/wasm の同一実行内比**（idiomatic/imperative、fan/sequential、
   region on/off）だけを裁く — 軸 A の無次元化と同じ手。
3. **ratchet 被覆 7/13 → 13/13**（#2142）: Map / JSON / String の 6 行を先に。§2.4 の 3 プログラムを
   perf suite に移す。
4. **fusion 点火**（#2045、0.56 行）と **fan 実並列**（#2044）— 既存 issue。順序は fusion が先
   （推奨イディオム `|>` チェーンの速度を決めるのはこれ）。
5. **`AlmideMap` 線形走査**: 未起票。native の `Vec<(K,V)>` lookup を、insertion-ordered を保ったまま
   index 付きに（compact-ordered-dict の hash 索引側）。wasm 側は既に O(1)。§2.4 の wordfreq 行が判定器。
6. **wasm perf の崖 2 件**（fft list-write ~3,500×、mandelbrot ~130×）: 未起票。ratchet の前提。
7. **wasm fib の呼び出しコスト**（Zig に 1.2–1.3× 負け）: 未起票。構造レッグの call 形を disasm で分解。
8. **stdlib 連結レジーム**（§2.4）: `group_by` の wasm 定数（#2156）、native Map/Value（#2157）、タプル sort key の wall と
   divergence（#2155 / #2154 — 後者は release blocker なので順序は最初）。

**勝利条件.** 13 行全部で idiomatic/imperative ≤ 1.15、wasm 同一実行内比 3 種が CI で赤になれる、
stdlib 連結 3 行が ratchet 下、wasm 崖 2 件が ≤ 3×。

**起票.** #2045 #2044 #2142 #2143 は既存。Map 線形走査・wasm 崖・wasm fib call → #2150。連結レジーム → #2154–#2157。

### T6. サイズに物語を持つ（優勢確定）

**なぜ.** wasm では 4/4 で最小だが、**4 プログラム中 3 つが 2.7 KB 未満**で「床」を測っているだけ。
唯一 Float を印字する行は負ける（§16）。native は 488 KB で 53 KB の近隣に 7–9× 負け、
`strip`/`panic=abort`/`opt-level=z` のどれも emit プロファイルに無い（`cargo_build.rs:38-45`）。

**機構.**
1. **`almide build --release-small`**（または `[profile.small]`）: `opt-level="z"`, `lto="fat"`,
   `codegen-units=1`, `panic="abort"`, `strip=true`。bare rustc hello が 466 KB なので、backend の
   床がどこまで下がるかを**まず測る**（#2092 の「measured against anything」を満たす）。
   目標は hello < 200 KB（MoonBit 級）。53 KB 級は libc 動的リンク or `no_std` ランタイムの話で、
   flight-subset（`--profile critical`）の `no_std` 化と合流する — ここでは目標にしない。
2. **wasm size ratchet**（#2141）: `research/benchmark/perf/wasm-size/` 5 本を「stdlib を段階的に
   引き込む梯子」に置き換える — hello / Int 印字 / Float 印字 / String 操作 / List / Map / JSON / regex。
   各段の verified と `--wasm-opt` の両方を shrink-only。
3. **float printer**（#2099）: 3,531 B → ≤ 2,000 B。Ryu/Grisu の shortest-roundtrip を wasm 自前で
   書き、Dragon4 は `--exact` 経路に退ける。近隣の同項目は +4,326 B なので、−28% で勝ち越す。
4. **RC ヘルパの無条件同梱**は #1962 で解消済。`hello` の残り 251 B は物語に含めない（切り捨て）。

**勝利条件.** native hello < 200 KB（small profile）、size 梯子 8 段が ratchet 下、
spectralnorm wasm ≤ 7,000 B（Wado `-Os` 7,185 を下回る）。

**起票.** #2092（P-low → P-high に昇格）、#2141、#2099。small profile は #2092 に追記。

### T7. インクリメンタル（優勢確定）

**なぜ.** 9/9 の参照コンパイラがモジュール単位再ビルドを持ち、Almide だけ 0。`.almdi` は
書かれるが **`read_almdi` の呼び出し元が 0**。いっぽう軸 A の規模非依存性は達成（傾き 1.13）
— つまり **check は十分速く、遅いのは rustc を含む native の 134 ms** で、go の 92 ms に負ける。

**機構.**
1. **粗粒度で足りる**（ALL_AXES の判断を維持: salsa 級は採らない）。`.almdi` の消費者を書く:
   near-peer の 4 部述語（config digest 一致 ∧ source が artifact より古い ∧ 全 import の interface
   digest 一致 ∧ artifact 可読）+ **interface digest が同じなら書かない**（mtime を動かさず下流を止める
   15 行の early cutoff）。テストは in-memory unit test ~26 本、プロセス起動ではなく。
2. **native の 134 ms** は 0.6x（cranelift、#1005）でしか消えない。本書は順序だけ決める:
   **T7-1（`.almdi` 消費）は 0.5x 内で着地、cranelift は 0.6x のまま**。
3. **Rust source 生成 23 ms の内訳**を `--timings` で出す（Aver の 6 ms との差 17 ms は frontend か
   render か不明）。
4. **等 LOC の他言語コンパイル時間**をアリーナの常設行に（§13 の表を `bench-sources/` の runner で
   再現可能にし、`check-readme-numbers.sh` の鮮度対象に）。

**勝利条件.** 未変更モジュールの再 `check` が IR を 1 つも作らない（`--timings` で確認）、
native 編集ループ p95 が go の同規模 `go build` 以下。

**起票.** #1003 が per-module cache、#1005 が cranelift。`.almdi` 消費者と `--timings` は
#1003 の子として起票 → #2151。

### T8. 信頼の可搬性（防衛）

**なぜ.** 証拠の軸は無競合だが、**輸出形式**が無い: 検証器はコンパイラ同梱、配布に署名が無く、
`@intrinsic` は 798 宣言がユーザコードからも書ける（capability 物語をすり抜ける）。
直接競合は検証器を**独立版数の別バイナリ**にし、`cert verify` はサブプロセス shim だけにしている。

**機構.**
1. **`almide-verify`**: PCC checker（1,400 行）を別 crate・別版数・別バイナリに。`almide verify` は
   shim のみ、リンクフォールバック無し。trust-layer.md L3 の「第三者が `make verify`」がこれで
   バイナリ配布でも成立する。
2. **install 一行に attestation**: release.yml で `SHA256SUMS.txt` + GitHub artifact attestation、
   README の一行に `gh attestation verify --repo almide/almide` を含める。
3. **intrinsic 宣言権限**: `@intrinsic` は `stdlib/` と `runtime/` 配下の module でのみ合法、
   それ以外は新 E-code（hint: 「intrinsic は stdlib に置く。ユーザコードは effect fn で包む」）。
   `check-intrinsic-boundary.sh` は link 側、これは宣言側。
4. **抑制の形を先に決める**: Almide は抑制構文を持たない（良い）。将来 `@allow` 相当が来るなら、
   **非沈黙**（毎コンパイル再告知）・**期限付き**（`until=`）・**理由必須**・**commit trailer で ack**
   の 4 条件を ADR にしておく。
5. **mutation score を公開**: `check-mutation-gate.sh` の結果を `proofs/` に stamp し README のバッジに。
6. **capability の build 時 DCE**: `--profile critical --allow IO` で拒否された capability の import が
   wasm から**消える**ことを `check-wasi-pins.sh` で断言（現状は check 時 deny のみ）。

**勝利条件.** `almide-verify` が独立リリース、attestation 検証が README 一行、
`@intrinsic` のユーザコード宣言が E-code、mutation バッジ、DCE 断言ゲート。

**起票.** 全て未起票 → #2152。

### T9. エコシステムの非対称（防衛）

**なぜ.** ALL_AXES 軸 F の勝利条件（エージェントが書き、ゲートが通し、人が承認 → 外部 10 本）は
**issue 0 件**、on-hold の 2 文書は陳腐化。星 34 は配布の不在の症状。近隣は
「言語ガイドをバイナリに埋め込み冪等に書き出す」「per-commit ベンチを Pages に公開」
「skills を別リポジトリに」を持つ。

**機構.**
1. **`almide agent-connect`**: `llms.txt` と CHEATSHEET を `include_str!` でバイナリに埋め、
   `.claude/skills/almide/SKILL.md` と `AGENTS.md` の**マーカー区間**に書き出す。マーカー不整合は
   推測せず名指しで拒否、再実行は無差分。**コンパイラと prompt の版ずれが構造的に不可能**になる。
2. **公開インタフェース ファイル**: `almide compile --json` の出力を `almide.interface.json` として
   checked-in にし、`check-interface-diff.sh` の入力にする（既に release 時に使う索引を、PR 単位の
   diff 対象に）。
3. **エージェント執筆パッケージの経路**: `almide/pkg-template` + Dojo の MSR gate + 人の承認 = publish。
   レジストリは**最初は GitHub org + タグ**（MVS は git ref で足りる）。hosted registry は
   外部 10 本の後。
4. **per-commit ベンチ公開**: `research/benchmark/perf/results/*.json` を Pages にプロット。
   「数字が動いた」が外から見える。

**勝利条件.** ALL_AXES 軸 F と同一（経路を通った外部パッケージ 10 本）。中間指標: `agent-connect`
を使う外部リポジトリ数。

**起票.** `agent-connect` とパッケージ経路が未起票 → #2153。

---

## 4. 起票（2026-09-13）

本書と同時に起票した issue（2026-09-13）。

| § | 題 | 種別 | 突破点 |
|---|---|---|---|
| 4-1 #2146 | Public MSR harness: same-condition runs across five languages, third-party reproducible, README freshness ratchet | enhancement, mob | T1 |
| 4-2 #2147 | `almide survive`: the survival delta of a proposed edit (check + test + contract) before it is applied, on CLI, LSP and MCP | enhancement | T2 |
| 4-3 #2148 | `almide query`: hover / definition / references / inlay hints as CLI subcommands over the existing LSP handlers | enhancement | T2 |
| 4-4 #2149 | Repair as a schema field, a two-way repair ratchet with silent cases, and `explain --list` | enhancement | T3 |
| 4-5 #2150 | The native Map is a linear scan, two wasm perf cliffs have no issue, and wasm fib pays a call cost the field does not | enhancement | T5 |
| 4-6 #2151 | Consume the `.almdi` interface artifact: the four-part freshness predicate and the unchanged-digest early cutoff | enhancement | T7 |
| 4-7 #2152 | Portable trust: an independently versioned verifier binary, attestation in the install line, and an authority gate on `@intrinsic` | enhancement, proof | T8 |
| 4-8 #2153 | `almide agent-connect`: the language guide shipped inside the binary, written out idempotently under owned markers | enhancement | T9 |

§2.4 の計測から起票（同日）:

| 題 | 種別 | 突破点 |
|---|---|---|
| #2154 タプル sort key が incumbent で "verified" のまま `indirect call type mismatch` trap、native は答えを出す | **I-divergence**, P-high | T5（release blocker） |
| #2155 推奨 word-count 形が stock WASI で build できない: タプル key の wall + `list.group_by_x` 未 link | enhancement | T5 |
| #2156 `list.group_by` が構造レッグで 1 要素 ~11 µs（命令形の 110×） | enhancement, P-high | T5 |
| #2157 native の Map と Value が wasm 双子より遅い（同プログラムが wasmtime で 1.5–2.3× 速い） | enhancement, P-high | T5 |

既存 issue への追記: #2092（P-low → P-high、small profile の測定手順）、#1460（shrink スケジュール）。

---

## 5. 順序 — 何を先に、なぜ

```
T1 課題集(#1963) ─┐
T3 修復スキーマ+silent ratchet ─┼─→ T2 survive API ─→ T1 公開ハーネス再計測   … 致命の 3 つは一本の鎖
                                 │
T5 綴りゲート全面化 ─→ T4 stage 1(同じ器具) ─→ fusion(#2045) ─→ fan(#2044)
T6 size 梯子 + small profile ─→ float printer(#2099)
T7 .almdi 消費(0.5x) ────────────────────────→ cranelift(0.6x, 据え置き)
T8 / T9 は並走可（コンパイラ本体を触らない）
```

**致命の鎖を先に**。理由は §1.3 の一行: 573 s / $3.19 は他言語の 4–9 倍で、これを下げずに
「LLM が最も正確に書ける」は数字で立たない。性能・サイズ・編集ループは既に首位圏で、
落とさなければよい（ratchet がある）。

---

## 6. 降りる軸の再入条件

ALL_AXES の「明示的に降りる軸」表には再入条件が無かった。ここで付ける。

| 降りた軸 | 再入を検討する測定 |
|---|---|
| zero-shot / pass@1 | Dojo で pass@1 と MSR の順位が**一致**する課題集ができたとき（＝ pass@1 が MSR の安価な代理になる） |
| 汎用スカラー codegen で LLVM 超え | アリーナの騒音圏（fib / spectralnorm）で**同一機構による**負けが 2 機種以上で再現したとき |
| MLIR 採用 | `--target wgsl`（#1331）が 1 本の実 workload で CPU 同一出力を出し、次の backend が必要になったとき |
| salsa 級クエリエンジン | `check-edit-loop-scale.sh` の傾きが 1.5 を超えるか、T7-1 の粗粒度キャッシュで dogfood フルビルドが 2–3 s を割れなかったとき |
| stack switching 前提の並行 | WASI 0.3 async が stable で、fan の実並列（#2044）が native だけ速くなったとき |
| 巨大 stdlib | T9 の外部パッケージ経路が 10 本を通し、stdlib 追加要求の 8 割がそこで吸収できると示せたとき |

---

## 7. この文書の維持

- 数字は全て日付付き。§1 の表を更新するときは出典（RESEARCH-neighbours §番号 / BENCHMARKS.md /
  dojo）を必ず併記する。
- 突破点を「達成」と書けるのは、§3 各項の**ゲート**が CI で赤になれる状態になってから。
- ALL_AXES の勝利条件を変えたら同じ PR で本書の表 0 を更新する。逆も同じ。
- 近隣の再計測は `bench-sources/` の runner で。**cache を外したことを書かない compile 時間は信じない**（§13 の罠）。

---

## 付録 A. 近隣機構の引用（2026-09-13 深読み、`R = ../almide-references`）

近隣を名指しで批評するのは本書の目的ではないので、ここでは**Almide が吸収する機構の所在**だけを
file:line で残す。星と tip 日付は 2026-09-13 の `gh api`。

**A-1. 直接の思想競合（Rust 経由、Lean 証明書）** — `R/aver/`
- ガイドをバイナリに `include_str!`、`agent-connect` がマーカー所有で冪等書き出し: `src/main/agent_connect.rs:29-46, 380-395`
- 診断に `Repair{primary, alternatives, example}` / `conflict` / `related` / `intent`: `src/diagnostics/model.rs:63-105`
- 128 行の slug カタログ（Fires when / Repair 列）: `docs/diagnostics-slugs.md`
- NDJSON + `schema_version` + 末尾 summary: `docs/diagnostics-schema.md:25-37`
- `context --budget 10kb --json`（トークン予算付き構造マップ）: `docs/cli.md:218-247`
- `audit --hostile`（敵対 world を別 slug で）: `docs/cli.md:210-217`
- 独立版数の証明書検証バイナリ、本体は shim: `docs/cli.md:330-372`、`aver-cert/src/verifier.rs`
- `--explain-passes --json`（関数ごとの alloc-free / TCO を CI で断言可能）: `docs/cli.md:315-325`
- 整数除算は `Result` を返す関数、演算子ではない: `docs/diagnostics-slugs.md:25`

**A-2. 契約層に MSR を API 化した言語（変数名が無い）** — `R/vera/`
- `speculativeEdit`（適用前の証明デルタ）: `LSP_SERVER.md:123-153`
- `proposeEdit`（検証→適用融合、`force` 明示・fail-closed）: `LSP_SERVER.md:155-177`
- `strengthenContract`（契約変更の呼び出し側監査）: `LSP_SERVER.md:179-193`
- `builtins|effects|errors --json` がレジストリを直接読む（文書の数字は length）: `vera/introspect.py:1-16`
- 全 158 code に `since` 版数: `vera/_since.py`
- 公開 Lark 文法 + 「文法 ≡ 仕様散文 ≡ editor 文法」の CI ゲート: `scripts/check_grammar_alignment.py:1-30`
- mutation score 83.3% を公開値に: `mutation.json`、`MUTATION.md:1-22`
- 右→左スロット番号は型検査を通る誤りを産む（§13 実見） — 変数名を消すと名前不一致という検出器も消える

**A-3. 「モデルが書き、人が査読する」体制向け（OCaml、C 経由）** — `R/jacquard/`
- 224 code、`(domain, code)` は再利用・改番しない、emit 全 code がカタログに在ることをテスト: `docs/errors.md:1-6`
- 診断は構造体: summary / `Cause:` / **ただ一つの** `Next step:` / `Contrast:`: `docs/errors.md:7-22`
- W0301（値種にまたがる曖昧束縛の告知）: `docs/errors.md:104`
- `--allow net` の実行時 capability、`fs`/`eval` grant は設計で拒否: `README.md:48-53, 164-166`
- 正規構造ハッシュ + 意味 diff + content-addressed テストキャッシュ: `bin/main.ml:2332, 2373`、`docs/warp-testing.md:298-336`
- 多発 fault シミュレーション（2ⁿ 経路）と relational case: `docs/warp-testing.md:207-297`
- 53 KB の native バイナリ（§14）

**A-4. Wasm GC + Component Model + WASI 0.3 のみ** — `R/wado/`
- LSP 機能を CLI に（`query diagnostics|references|definition|hover|inlay-hints`）: `wado-cli/src/query.rs:9-49`
- install 一行に `SHA256SUMS.txt` + `gh attestation verify`: `README.md:29-45`
- OCI artifact への publish、workspace-root のみ: `wado-cli/src/publish.rs:1-26`
- 診断 reason chain の設計文書が文献（type-error ablation × agent）を引く: `docs/wep-2026-06-02-diagnostic-reason-chains.md:1-40`
- per-commit ベンチを Pages に: `README.md:169-176`
- Almide を既に調査済み: `docs/research-language-survey-almide.md`

**A-5. 自己ホスト（effect row）、Almide の MSR を移植した側** — `R/vibe/`
- MSR ハーネス移植（名指し）: `eval/msr/README.md:1-12`
- 8 次元ルーブリックの AI 言語レビューを毎ラウンド: `eval/lang-review/rubric.md`
- **修復ラチェット両方向**（`diag.grep` + `silent` は診断が出たら FAIL）: `eval/lang-review/run_repair.sh:1-16`、`repair/09_silent_builtin_arity`
- 「どのコンパイラが答えたか」罠を hard fail: `run_repair.sh:24-40`
- capability flag を build 時 const-fold + DCE: `README.md:100-108`
- 実行可能な本（出力を markdown に埋め戻す）: `README.md:61-66`
- P0 = silently wrong > P1 = crash: `AGENTS.md:23-29`

**A-6. 引用強制・自己ホスト（2026-07-19 で停滞）** — `R/hexa/`
- `@grace(code, until=, reason=)` + 毎コンパイル HX9000 再告知 + `Acked-grace:` trailer: `README.md:109, 333`
- 文法・落とし穴・変更履歴を 1 つの JSONL SoT に: `doc/grammar.jsonl`、`CHANGELOG.jsonl`
- gen3 ≡ gen4 バイト一致の自己ホスト固定点ゲート: `README.md:24`

**A-7. 主流小コホートの上位 2** — MoonBit（`~/.moon`）、`R/gleam/`
- `moon explain --diagnostic`（無引数で 172 行の全 code 列挙、mnemonic + 状態）、`--attribute`
- `moon test -u --limit 256`、`--test-failure-json`、`moon check --patch-file`、`moon info`（`.mbti`）、`moon prove`
- Gleam: ≈ 60 code action（`Add missing patterns` / `Generate dynamic decoder` / `Fill labels` / `Convert to pipe`）: `language-server/src/code_action.rs`
- Gleam: `export package-interface --out x.json`: `compiler-cli/src/lib.rs:695-700`
- Gleam: 網羅性チェッカが code action と同一ソース: `compiler-core/src/exhaustiveness/`
- Gleam: `test-community-packages/` を CI でコンパイル（エコシステム破壊の回帰ゲート）

## 付録 B. 08-16 サーベイの再検証で見つけた注記

- `stdlib/list_sort_by_str_key.almd:3,104` と `list_sortby_float.almd:4` のコメントは「insertion sort」のまま（実装は mergesort、#1459）。コメントだけ直す。
- `cargo fmt --check` はどの workflow にも無い（clippy ラチェットは有）。`.almd` 側は厳格、Rust 側は無ゲート — 主流の逆。
- `check-pass-isolated.sh` は rustc の `test-mir-pass` を名指しで手本にしつつ、**出力同一性**を選んでいる（意図的、`:23-28`）。MIR golden は持たない。
- ALL_AXES 軸 E の #1312/#1313/#1314 は全て closed。ROAD_TO_1_0 0.4x 表の #928/#999/#1334/#917 も closed。両文書は本 PR で注記。
