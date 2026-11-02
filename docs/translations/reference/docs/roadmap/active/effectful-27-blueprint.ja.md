歴史的説明：effect/raw-pointer の青写真。byte テスト済み tier は全入力の意味論的証明ではない。
<a id="blueprint--making-the-27-effectful--raw-pointer-stdlib-fns-functional-in-v1"></a>

# 青写真 — 約27個の effectful / raw-pointer stdlib 関数を v1 で FUNCTIONAL にする

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](effectful-27-blueprint.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](effectful-27-blueprint.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](effectful-27-blueprint.zh-CN.md)

> 変更していない固定原本の非規範的な歴史的読者スナップショット。自動同期、テスト/証明検査/実装の独立再実行はしていない。6月17日青写真と6月25日追記の host/API/capability 前提を分けて保持し、SAFE-NOW・PROVEN・検証済みのラベルは実装の許可や現在の対応状況を意味しない。有限 byte-golden 回帰テストや pointer round-trip は全入力の意味論/メモリ安全証明ではない。過大宣言が安全という表現はモデル化 capability 性質に関するもので、別記の所有権/RC・scratch memory・runner・host・provenance 義務を消さない。予測値や旧計数はそのままの位置づけ。原文はレビュー修正を§7と指すが列挙は§8にあるため、参照を黙って訂正せず保持する。歴史的構築コマンド、memory参照、優先順は実行指示や権限ではない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/effectful-27-blueprint.md)

<a id="evidence-terminology-in-this-historical-blueprint"></a>

## この歴史的青写真での証拠用語

6月17日の設計では **PROVEN** を tier-1 の有限 byte-golden 回帰テストのラベルとして使った。
このラベルは**引用されたケースで byte テスト済み**と読み、すべての実行に対する数学的証明と読んではならない。
モデル化した capability 定理は別種の証拠で、明記された前提を保持する。
ポインタの round-trip テストも任意のメモリ安全性を証明しない。

以下の旧 host 可用性、raw-pointer API、543/621 の予測値は歴史的な設計入力で、現在の対応状況や安全指針ではない。
現在の正本と raw-memory 境界は[capability 概説](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/wasm/capability-system.md)と
[ADR-0031](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0031-raw-pointers-carry-provenance.md)にある。
歴史的な SAFE-NOW ラベルは、削除された raw-address API の復活を許可しない。

設計のみ（計画であり、ビルドではない）。6-agent の設計分担と敵対的レビューで作成し、
実ツリーに接地（2026-06-17）。**判定：SOUND-with-fixes — これを出発点に構築するのは安全だが、
そのままではない。** §7 のレビュー修正は仕様の一部。

> **誠実さの規則（不可欠）：** 約27個は一つの問題ではなく、単一の「完了」数にしてはならない。
> 3 tier、3種類の検証上限。「proven」と呼べるのは tier-1 だけ。
> [[../../../../.claude .../feedback_done_means_three_tiers]]（memory）を参照。

<a id="1-three-mechanisms-each-its-own-ceiling"></a>

## 1. 三つの機構（それぞれ固有の上限）

| | 追加するもの | sandbox 退出 | 上限 |
|---|---|---|---|
| **(A) WASI host-import 基盤** | 新しい `PrimKind` を `(call $wasi_fn)` として render し、`preamble()` に新しい `(import "wasi_snapshot_preview1" …)` を置く。唯一既存の `fd_write`（`render_wasm.rs:890`）に倣う。その上で effectful 本体を Almide で self-host | YES | **declared-caps-safe**（動作 + caps ACCEPT。出力は性質上証明不能） |
| **(B) Capability 語彙** | `enum Capability`（`lib.rs:471`、当時は `Stdout=0` のみ）+ `Capability::id` + Coq ids を `{Stdout,Fs,Net,Clock,Random,Env,Process}` へ拡張。`cap_witness`+`declared_caps` を誠実に帰属させる | 該当せず（A の会計） | 証明基盤 |
| **(C) raw-pointer / mem** | `repr_of` のアーム一つ（`Ty::RawPtr => Word`）。`bytes.*` の ptr 関数 = `prim.handle(b)+12`。`$bump` 上の非cap `prim.heap_mark/heap_reset` | NO（linear-mem のアドレス算術） | **offset-functional**（round-trip。byte 同値は適用不能） |

**実測による根拠 (A)：** v0 の `emit_wasm/runtime.rs:14-145` は、v1 テストと同じ wasmtime 上で
preview1 全セット（`fd_write, clock_time_get, random_get, proc_exit, fd_read, path_open, …`）を
既に登録・実行している。採用は発見ではなく、*実証済み呼出を prim 基盤へ昇格すること*。

<a id="2-reachability-reality-the-hard-honest-limit"></a>

## 2. 到達可能性の現実（厳しい誠実な限界）

v1 runner は素の `wasmtime run <file>`。既定 WASI ctx、`--dir` なし、`--env` なし、stdio は継承
（`tests_part3.rs:525/:547`）。したがって：

- **今到達可能：** stdout（完了）、`clock_time_get`、`random_get`、`proc_exit`、`fd_read`（stdin）。
- **runner 変更後だけ到達可能**（`--dir .` / `--env`）：すべての `fs.*`、`env.get/cwd`、
  `process.env`。wasm ではなく*起動方法*が障壁（機構は v0 `calls_fs.rs` で実証済み）。
- **preview1（当時の対象）では BLOCKED — 第4の指標列であり「未実装」ではない：**
  すべての `http.*`（24）、`net.*` 生 TCP（12）、SSE/LLM streaming
  （`sock_connect`/外向き socket なし）、`env.set`（`environ` は読取専用）。約36関数。
  preview1 では native-only。**恒久ではない：** [`wasm-platform-frontier.md`](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/wasm-platform-frontier.md) は
  WASI 0.2/0.3 + Component Model 移行を計画（`wasi-http` は wasmtime host 実装あり。
  `wasi:sockets` = capability 範囲付きネットワークで、この青写真 §1(B) の capability 語彙に直接つながる。
  [`effect-system-capability.md`](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/effect-system-capability.md) 参照）。その対象では
  **declared-caps-safe**（tier-2）になる。つまり「preview1 では阻まれるが frontier roadmap で解除」
  であり、**独自の指標列（§5）として表示し、未実装数に混ぜてはならない**。
  そうしないと約36個は target 移行が必要なのに「残り78」が「実行可能78」と読まれる。
  preview1 での「27→functional」の実態は縮む：clock/random/exit/stdin は今、
  fs/env は runner flag 後、http/net は WASI 0.2/p2 後のみ。

<a id="3-build-order-safe-now-vs-needs-fresh"></a>

## 3. 構築順（SAFE-NOW 対 NEEDS-FRESH）

`NEEDS-FRESH` = Coq registry / `cap_witness` / `declared_caps` / `is_known_free` /
`repr_of` / `$bump` 不変条件に触れる → 十分な集中と敵対的レビューが必要。

**Phase 0 — 足場と誠実な表示の配線**
- **B0** tier manifest `docs/stdlib/tier-manifest.toml` + `check-tier-manifest.sh` + `stdlib-tier-report.sh`（3列、混ぜない）。**SAFE-NOW**
- **B1** `effect_map`（`frontend/lower/mod.rs:412` は当時捨てる）を、関数ごとの直接/推移的 `Effect` 集合で埋める。**SAFE-NOW**
- **B2** `Effect::IO` を Stdout と Fs に分離。**NEEDS-FRESH**（review #3：declared_caps に不可欠。B6 と原子的に束縛。埋めたが分離していない effect map → 誠実な declared_caps = 過少宣言 = accept-but-unsafe）。
- **B3** self-host 本体を草案化（未リンク）：`datetime.now/monotonic_ns`、`random.*`、`io.read_*`、さらに**pure** な `http` builder（`response/json/with_headers/req_*/query_params` — 純データ、cap なし、直ちに self-host 可）。**SAFE-NOW**

**Phase 1 — capability 基盤（前提 gate、原子的）**
- **B4** Task #35：`Capability::id` を単一正本化 + Coq id block を生成/検査 + CI drift gate（`check-stdlib-purity-registry.sh` 同様）。id 拡張の前に必須（誤番号 id = 黙って accept-but-unsafe）。**NEEDS-FRESH。** *（review #9：PROVEN 列も守る基盤衛生。強い証明のキューと並行し、後回しにしない。）*
- **B5** capability 語彙 + caps-witness の誠実さ — **原子単位、NEEDS-FRESH：**
  - `lib.rs:471` の `enum Capability` と `Capability::id` アームを拡張。
  - `proofs/CapabilityBound.v` で新 id と敵対的 `Example` を列挙。**定理変更なし** — `subset_check_sound`（`Subset.v`）は `nat` 全般に成立。
  - `certificate.rs:167` の `CallIndirect` taint：ハードコードした `Stdout` → **`Capability::ALL`**（review #2：単一の `ALL` 定数から導出。B4 drift gate が `CallIndirect taint == Capability::ALL` を検査しなければ、忘れた8番目のcap = witness に載らない closure）。
  - `classify_corpus.rs:465` の `is_known_free`：`n.contains('.')` を一律信頼するのを止める。ドット付き ⇒ known-free は **`purity::is_pure(module,func)` のときだけ**。**（review #1：B7 より先に必須。capability を持つドット付き op が存在した瞬間、`contains('.')` が `datetime.now` を caps-clean として通し accept-but-unsafe になる。`is_known_free` の狭化と最初の capability op は一つの原子単位。両方の fold closure — `reaches_capability_or_unknown` と `reachable_caps_or_tainted` — に適用。）**
  - `TRUSTED_BASE.md` + `corpus-wall.sh` + コメント（#34 claim-drift gate）の「未宣言 **Stdout** effect なし」を、原子的に集合全体へ書き換える。
- **B6** 誠実な `declared_caps`：`lower/mod.rs:128` の `if is_effect { [Stdout] }` を `transitive Effect → Capability` map に置換。B1+B2 に依存。過少宣言 = unsafe、過大宣言 = parity を壊す。**NEEDS-FRESH。**

**Phase 2 — 今到達可能な host prim**（各々 `fd_write` の鎖に従う：`prim.almd` 宣言 → `lower_prim_call` アーム → `PrimKind` variant → render アーム → `preamble` import → `cap_witness` アーム）。すべて **NEEDS-FRESH**（各々 capability を持つ）：
- **B7** `ClockTimeGet{clock_id}` → `Capability::Clock` — `datetime.now/monotonic_ns`、`env.unix_timestamp/millis`。Tier-2。
- **B8** `RandomGet` → `Capability::Random` — `random.int/float/choice/shuffle`。Tier-2。
- **B9** `ProcExit` → `Capability::Process` — `process.exit -> Never`。Tier-2。
- **B10** `FdRead{fd}` → `Capability::Stdin` — `io.read_*`、`process.stdin_lines`。**review #6：先に重複しない scratch-region 定数表の宣言が必要**（read 出力 buffer は、交互の `println(int.to_string(read_line()))` で print iovec@8 / itoa scratch と衝突してはならない。衝突すると trap より悪い無言の誤出力）。Tier-2。

**Phase 3 — raw-pointer / mem**
- **B11** `bytes.data_ptr(b)->Int = prim.handle(b)+12` — **SAFE-NOW**（cert/cap/所有権イベントなし。イディオムは `bytes_core.almd` で既出荷。RawPtr でなく Int を返すので B12 不要）。**ただし tier-3 OFFSET であり、proven 数を上げる無料の成果ではない**（review #4：`LIST_HEADER=12` をハードコード、v0 の +4 と異なる → byte 一致できない。round-trip テストが証明するのは v1 自身の layout のみ）。
- **B12** `repr_of` の `Ty::RawPtr => ScalarWidth::Word`。**NEEDS-FRESH**（この subset で初の新アーム。敵対的レビュー：RawPtr Scalar が `verify_ownership` で heap 値に化けないこと）。
- **B13** `as_ptr/as_mut_ptr -> RawPtr`（B11 と同本体、RawPtr 戻り値 → B12 が必要）。Tier-3。
- **B14** `from_raw_ptr`/`copy_to_ptr` — 初の RawPtr パラメータ経路（`bind_params` が scalar として配置し、heap borrow にはしない）。Tier-3 UNSAFE-contract。
- **B15** `heap_mark/heap_reset` + `mem.save/restore` + `bytes.heap_save/restore` — **難しい。review #5：現証明の下では stdlib から到達可能な prim にしてはならない。** mark より下への `heap_reset` は `$rc_dec` を迂回し `$freelist` を壊す。所有権証明は見えない（`restore` 後も残る live ref = 同じ binary 内の accept-but-unsafe use-after-free）。**既定：arena-escape-freedom 証明ができるまで全面延期**（「gated UNSAFE-floor-only」ではない。それでも検証面に accept-but-unsafe prim を置いてしまう）。

**Phase 4 — runner で解除 / blocked**
- **B16** runner `wasmtime run --dir . --env …` — **NEEDS-FRESH（意図的）：** 証明全体が対象とする sandbox を暗黙に変える。単一で最大の解除（全 `fs.*` + `env.get/cwd` + `process.env` を有効化）。これがないと fs prim は compile+validate 後 runtime で `ENOTCAPABLE` trap = v0 native との無言の parity 崩れ。
- **B-BLOCKED：** `http.*`/`net.*`/`env.set` — wasm-unsupported / native-only と記載（§2）。

<a id="4-soundness-story-why-vocab-expansion-keeps-no-undeclared-effect"></a>

## 4. 健全性の説明（語彙拡張しても「未宣言 effect なし」を保つ理由）

第4の性質 `within_bound = subset_prop allowed used` は `Subset.v` で `nat` リスト上に定義され、
**全称量化され、個々の capability を列挙しない**。したがって id 追加は証明を**無償で**一般化する。
`subset_check_sound` は byte 同一で、Coq の編集は registry コメントと非本質的な `Example` のみ。
**責務全体は未信頼の emission 側へ移る：`cap_witness` は実際の host effect を過大近似しなければならない**
（host 到達のたびに id を `used` へ寄与）。これを満たすもの：網羅的な `RtFn::capability` match
（variant の忘れ = compile error）+ host-prim ごとの `cap_witness` アーム + `Capability::ALL` の
`CallIndirect` taint + effectful なすべての `CallFn` 経路の拒否または cap emission。
唯一の破綻は host op が cap に到達するのに id を寄与しないこと → その id について空虚な成立
（accept-but-unsafe）、全面的に emission 側の問題。だから B4+B5+B6 は原子的 NEEDS-FRESH であり、
対応 cap + witness のない sandbox-exit prim を禁じる。C は capability を持たないのでこの性質を弱めない
（危険は所有権/RC 証明、B15 という別の不変条件）。

**★ FRESH-BUILD 敵対的質問 Q1（固定 — この計画の全 brick に対する唯一の健全性上の重要質問）：**
*「emitter がどこかで capability を過少宣言していないか — つまり host effect（fs/net/clock/random/…）に触れるのに
`used` へ id を寄与しない経路があるか？」* 健全性はいま `cap_witness` の過大近似に全面的に依存するため、
**過大宣言（保守的）は常に安全、過少宣言だけが accept-but-unsafe の穴。**
すべての fresh 敵対的レビューはここから始める。反証すべき過少宣言経路は正確に：
(1) `cap_witness` アームのない新 host-prim、(2) `is_known_free` に通されるドット付き effectful `CallFn`（B5）、
(3) `Capability::ALL` より狭い `CallIndirect` taint（B5/B4）、(4) 到達数より少ない cap に effect を写す
`declared_caps`（B6/B2）、(5) `RtFn::capability` variant の忘れ（compile-error なので構造的に閉じている）。
どれも過少宣言しなければ brick は健全 — 他の性質（ownership/names）は B15（別不変条件）以外で capability 作業の影響を受けない。

<a id="5-honest-3-tier-metric-resolves-100-needs-redefining"></a>

## 5. 誠実な3-tier指標（「100%の再定義が必要」を解消）

```
STDLIB FUNCTIONAL COVERAGE  543 / 621
  ├─ PROVEN (byte-matches v0)               : 543  ← the ONLY column any claim may call "proven"
  ├─ DECLARED-CAPS-SAFE (runs, caps⊇used)   :   0
  ├─ OFFSET-FUNCTIONAL (ptr round-trip)     :   0
  ├─ BLOCKED-IN-WASM-preview1 (native-only) : ~36  ← http/net/env.set; needs WASI 0.2/p2 (frontier)
  └─ unimplemented (buildable on this target):  ~42
```
**第4列は誠実さに不可欠：** 約36個は「この target で残る作業」ではなく、WASI-0.2 移行が必要
（`wasm-platform-frontier.md`）。`unimplemented` に隠すと「残り78」が「実行可能78」と読まれ、
3列分離で禁じたのと同じ過大主張になる。WALL にも物理分離の規律を適用する。
`check-tier-manifest.sh`（B0）は `blocked_target = "preview1"` フィールドからこの列を計算し、
壁を散文でなく数値にする。
- **Tier-1 PROVEN** — render→wasm→wasmtime の byte-match `#[test]`（`assert_eq!(out, <v0 golden>)`）。
  **review #7：`check-tier-manifest.sh` は引用テスト内の v0-golden byte assertion（`assert_eq!`+golden marker を grep）を検査し、
  同名の `#[test]` があるだけでは不可。** そうしなければ `proven` と誤ラベルした smoke test が、
  動作したものを証明済みと報告する重大な誤りに達する。
- **Tier-2 DECLARED-CAPS-SAFE** — caps ACCEPT（`reachable ⊆ declared`、gate 再利用）+
  trap せず動く smoke（exit-0、**stdout bytes について何も assert しない**）。
  主張は「宣言したことだけを行う」であり、「Xを生成する」ではない。
- **Tier-3 OFFSET-FUNCTIONAL** — layout 非依存の機能 law（`load8(store8(a,v))==v`）。literal byte 同値ではない。

**再定義した目標：** **「3 tier 全体で 621/621 FUNCTIONAL — うち約594が PROVEN」。**
見出し数値は PROVEN の部分件数。約594は*予測*（tier-report script で自己補正）で、達成を主張していない。
**API も文書も混合した「621 proven」を出してはならない。**

<a id="6-smallest-genuinely-safe-first-step"></a>

## 6. 本当に安全な最小の第一歩

**B11**（`bytes.data_ptr`）— proof/gate/cap/foundation に触れない唯一の項目。
ただし **OFFSET**（tier-3）と記し、proven の無料成果にしない。他はすべて強い証明キューの後の NEEDS-FRESH（優先度節）。

<a id="7-priority--this-is-the-plan-not-a-queue-jump"></a>

## 7. 優先度 — 計画であり、キューへの割込みではない

この作業の上限は tier-2/3（弱い証明）。**強い証明の必須項目を先にする**。
それぞれ PROVEN として着地し見出しを上げる：`string.split`（List[String] 構築）、
`list.map/filter/fold`（高階 self-host）、**#60 StringInterp**（言語中核の表面）。§3 はその後に。
例外：**B11** は機会があれば、**B4**（review #9）は並行（proven 列も守る）であり後回しにしない。

<a id="8-review-must-fixes-folded-into-25-above"></a>

## 8. レビュー必須修正（上の§2〜§5へ反映済み）

1. `is_known_free` 狭化は B7 より先、両 fold closure を含む（さもなくばドット付き cap call = accept-but-unsafe）。2. `CallIndirect` taint = `Capability::ALL`（導出）+ B4 が被覆検査。3. B2 を NEEDS-FRESH に変更、B6 と原子的。4. B11 は tier-3 OFFSET であり「安全で完了」ではない。5. B15 = 延期（UNSAFE-floor-only ではない）。6. B10 は先に静的 scratch-region map。7. tier 検査はテスト存在でなく byte assertion を検証。8. `process.args` は test-runner と `almide run --` で異なる（本番 parity）。9. B4 を strong-proof と並行へ引上げ。

<a id="envargs-v1--validated-design--the-entanglement-2026-06-25"></a>

## env.args v1 — 検証済み設計と絡み合い（2026-06-25）

env.args は v1 の WASI 基盤初の I/O capability。実証済み random.int 機構（当時唯一許可された effectful call）を模倣する：self-host stdlib 関数 → Capability を持つ `prim.*` op → 推移的 cap_witness → `used ⊆ declared` cert 検査 → render が WASI call を出力。具体要素：
1. `Capability::CliArgs`（lib.rs:599 enum。lib.rs:614 の Coq registry 対応は単射+安定を保つ：Stdout=0、Entropy=1、CliArgs=2）— FORMAL な trust-spine 追加。
2. `PrimKind::ArgsSizesGet` + `PrimKind::ArgsGet`（lib.rs:443 付近）が `Capability::CliArgs` を持つ（certificate.rs:178 の cert 会計、random_get→Entropy に倣う）。
3. `stdlib/env_args.almd` — self-host `env_args() -> List[String]`：`prim.args_sizes_get(argc_p, bufsize_p)` → argv+buf 確保 → `prim.args_get` → argc 個の null 終端文字列をループして `List[String]` にする（buffer parse は random.int の単一 i64 読取りより複雑）。
4. 2 prim の render_wasm WASI runtime（render_wasm は WASI 基盤を持ち、random.int の random_get は v1 で動く）。
5. calls.rs:187 で `env.args` を許可（`is_admitted_effectful` predicate を拡張）。

🚨 絡み合い（単純な capability 追加より大きい理由）：BYTE-MATCH ORACLE が壊れている。
`almide run --target wasm -- file.csv` は `--` 引数を wasm 実行へ渡さない
（env.args() は EMPTY → csv-to-json は JSON でなく「usage」を表示）。引数配線は完全な emit backend でも壊れている。
wasm_cross harness は pure fixture しか実行しない（program args 非対応）。よって env.args を cert-byte 検証する前に、
引数配線（CLI runner 転送 + harness が native と wasmtime に同一 argv を渡す）を修正する必要がある。
これが「段階的に」の本当の第一歩で、trust-spine capability 作業とは別の CLI/test-infra brick。

順序：(a) 引数配線 + wasm_cross harness の args 対応（byte-match oracle）を修正し、
その後 (b) random.int に倣う env.args capability（手順1〜5）。csv-to-json はさらに fs.read_text
（path_open+fd_read self-host + Capability::FsRead）が必要で、almide-grep と fs.walk もそれぞれ独自 capability brick。
この2アプリ向けの全 WASI file-I/O 基盤は約3〜4 capability brick + infra。
集中した fresh 作業として推奨（capability ごとに委任+gate）。

保存した英語指標図の説明：STDLIB FUNCTIONAL COVERAGE は543/621。PROVEN（v0とbyte一致）は543で、原図でprovenと呼べる唯一の列。DECLARED-CAPS-SAFE（動作しcaps⊇used）0、OFFSET-FUNCTIONAL（ptr round-trip）0、BLOCKED-IN-WASM-preview1（native-only）約36はhttp/net/env.setでWASI 0.2/p2移行が必要、当該targetで構築可能な未実装は約42。この図の旧ラベルは冒頭の有限テスト限定で読む。
