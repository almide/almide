原文の description メタデータ：Value の参照カウントを証明書の対象領域に取り込む
<a id="柱c-extension-bring-value-rc-into-the-certified-region"></a>

# 柱 C の拡張：Value RC を証明書の対象領域に取り込む

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](value-rc-cert.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](value-rc-cert.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](value-rc-cert.zh-CN.md)

> 固定原文の非規範読者用スナップショット。正本は原文であり、自動同期しない。独立した実装検証や引用指示の実行は行わない。歴史的状態、計画、比較主張は原文の範囲を保持する。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/value-rc-cert.md)

> 原文注記：CERTIFIED はモデル化した所有権証明書・checker の範囲を意味し、外部認証や普遍的なメモリ安全性ではない。冒頭の訂正は corpus に handle 由来 RC がないという旧記述に優先するが、履歴として双方を残す。Brick 1+2 の carrier 作業と load64 由来入れ子 child は別。後続 CoownLoop/CoownCompose 証明は旧証明 TODO を更新する一方、Rust provenance と関数横断対応付けはこの snapshot で未了。共有 whitelist 着地は旧手動 registry の後続案を更新する。件数・kernel 検査は再実行していない履歴であり、model 定理だけで renderer 統合や全実プログラムの安全性は確立しない。


**状態：Brick 1+2 着地 — carrier RC は証明書の対象（verify_ownership + ownership_certificate、不可分、一致）。2026-06-24。**

prim.handle から渡される Value RC の類は、今や証明書の対象領域に入った。不均衡な `rc_inc(prim.handle(v))` は、実行可能 verifier と証明済み cert の両方が捕捉する Leak になる（`i a d` → rc>0）。corpus-wall ACCEPT 18165（byte 同一）、mir 502/0、verify_ownership↔cert の一致が成立、unit test は `value_rc_carrier_balance_is_certified`。以前の範囲の訂正：コーパスには prim.handle 由来の rc_dec（`__drop_value` の `prim.rc_dec(prim.handle(v))`）が存在するが、ACCEPT する。rc 0 での RcDec を保守的に無操作とする verifier と cert の carrier projection により、borrowed param の自己消費を健全に保ち、証明済み Coq checker が検証する。残る brick は #3（load64→object slot provenance）であり、reduce_value の `__copy_value`（load64 由来 child）を実際に検証する部分。それまでは load64 由来 Value RC は差分テストの下限に留まる。

<a id="why"></a>

## 理由
Value の参照カウント操作（`prim.rc_inc`/`rc_dec`、`Op::DropValue`）は ownership checker が無操作と扱う PRIM 領域にある。そのため Value RC 均衡は cert から見えず、粗い leak-test（5000× の OOM/trap）だけが不均衡を捕捉する。これが `list.reduce_value` を止めた構造的 blocker：byte は正しく、leak-safe は 5/7 だが、heap payload の accumulator return（Str/Array 上の `(a,b)=>a`）は leak し、RC を正確に合わせる作業は検証 feedback のない粗い leak-test 相手の試行錯誤だった。corpus-wall はこのコードを ACCEPT する（prim RC 盲点）。受理されても安全でなく、leak-test だけが守る。

<a id="feasibility-4-agent-scope-validated-tractable-rust-first"></a>

## 実現可能性（4 agent で範囲検証）：取り組み可能、Rust が先
- **Coq 証明は blocker ではない。** OwnershipChecker.v の Op alphabet は既に Inc/Alias（+1）と Dec/MoveOut（−1）を持つ。`exec` は既に rc≤0 での −1 を fault（二重解放）とし、終了時 rc>0 の object を leak とする。RuntimeModel.v/WasmRcDec.v は、生成された `$rc_dec` が抽象 RC cell 書込みを実現すると既に証明する。RC のモデル化には新補題も `check_line_unroll_sound` の再証明も不要。
- **blocker は純粋に未信頼の Rust 側**：`verify_ownership`（lib.rs:844）と `ownership_certificate`（certificate.rs）が `Op::Prim{RcInc/RcDec}` を一律無操作と扱い、RC 均衡が見えない。
- **名前で指せる場合の provenance は復元可能**：`prim.handle(v)` は source object を `args[0]` に置いて lowering する（calls_p4.rs:790）ため、MIR 再設計なしで checker が `handle→object`（address carrier）を復元できる。正直な境界：`prim.load64(raw_addr)` 由来 RC（`__varr_copy`/`__drop_value` の loop 本体の要素 RC と `__copy_value` の child）は不透明なまま、slot provenance の brick（型付き slot layout / `Repr::HandleAddr`）まで差分テストの下限に留まる。

<a id="the-bricks-land-atomically-per-the-certificate_p2rs-invariant-cert--verify_ownership-agree-on-every-case"></a>

## brick（certificate_p2.rs の「cert ⟺ verify_ownership が全ケースで一致」の不変条件に従い不可分に着地）
1. **verify_ownership と ownership_certificate の両方へ carrier RC を同時導入。** verify_ownership（試作・gate 済み：corpus-wall ACCEPT 18165、mir 501/0）：`Prim{Handle,dst,[src]}` → `object_of[dst]=object_of[src]`（carrier、RC/dead なし）；`RcInc(carrier)` → `rc[o]+=1`；`RcDec(carrier)` → `rc≥1` のときだけ release（借用の rc 0 に対する rc_dec は無操作のまま — 既存コーパスを新たに拒否しない保守的逃げ道）。ownership_certificate はこれを鏡写しにする必要がある（RcInc(carrier) に `a`、RcDec(carrier,owned) に `d` を emit）。両者が一致し、既に RC 対応の Coq checker が検証するためである。コーパスへの影響：なし — 全 corpus RC 操作は load64 由来（carrier なし）なので、新しい cert byte もなく、`plus_one_events_backed` は壊れず、corpus-wall は byte 同一 ACCEPT。
2. **plus_one_events_backed 拡張**（classify_corpus.rs:331）— cert `a` の根拠として RcInc prim 操作を数える。brick 1 の cert emit と不可分に行う（そうしないと `i==allocs+heap_results && a==dups` assert が壊れる）。
3. **load64→object slot provenance** — 真の reduce_value 修正。型付き slot layout によりロードした要素 handle（`prim.load_handle(block+12+i*8)`）を block の要素 object に追跡し、`__copy_value`/`__varr_copy` の loop 本体要素 RC を検証可能にする。これは大きい brick（Repr::HandleAddr または slot 型付き load が必要）。それまでは reduce_value（および全 load64 由来 Value RC combinator）は leak-test の下限 / wall 内に留まる。

<a id="corpus-risk-mapped"></a>

## コーパスリスク（把握済み）
- `plus_one_events_backed` は `i==allocs+heap_results && a==dups` を hard-assert する。コーパス fn に RC が cert byte を emit した瞬間壊れる。corpus RC は全て load64 由来（carrier なし → cert byte なし）なので回避。Brick 1+2 は prim.handle 由来の場合だけ emit し、そのケースはまだ corpus にない。
- `__drop_value:740` は借用 param（rc 0）に `prim.rc_dec(prim.handle(v))` を行う。保守的 RcDec 規則（rc≥1 のみモデル化）はこれを無操作に保つ → 新たに拒否し得ない。（将来の精密規則「借用 carrier への RcDec = 許可された自己消費」ならモデル化するが、brick 1 には不要。）

<a id="prototyped-diff-reverted-librs-verify_ownership-prim-arm"></a>

## 試作差分（revert 済み、lib.rs verify_ownership の Prim arm）
`| Op::Prim { .. }` の無操作を `Op::Prim { kind, dst, args } => match kind { Handle => carrier; RcInc => rc+=1 if carrier; RcDec => rc-=1 if carrier && rc≥1; _ => {} }` に分割。gate 済み：cargo test -p almide-mir 501/0、corpus-wall ACCEPT 18165（byte 同一）。コーパスに対しては厳密な refinement だが、prim.handle 由来 RC では verify_ownership↔cert の乖離を生むため、対応する ownership_certificate emit とともに（brick 1+2 不可分に）着地すべきで、単独は不可。

<a id="brick-3-load64-fed-nested-element-rc--foundation-landed-the-core-composition-lemma-is-proven-proofscoownloopv-2026-06-25"></a>

## Brick 3（load64 由来の入れ子要素 RC）— 基盤着地：核の合成補題を証明済み（proofs/CoownLoop.v、2026-06-25）

概念上最も難しい部分は完了：`proofs/CoownLoop.v` が Coq kernel 上で（axiom-clean、coqchk 再検証済み、check.sh の proof gate 内）、OwnershipLoop.v が明示的に除外していた共同所有 COPY / 再帰 DROP の均衡を証明する。container の直接要素を refcount ベクトルとしてモデル化し、producer の共同所有 FILL（各 +1、rc_inc）に続く container の再帰 DROP（各 −1、<=0 なら fault）が、全 source 要素を元の RC に戻す（`coown_fill_drop_neutral`）。これにより任意の要素数について `coown_copy_no_double_free` + `coown_copy_no_leak` を得る。これは iteration ごとではなく要素数を鍵にした、別の再帰 drop で net +1 を均衡させるケース。残り（engineering）は、具体的 producer の fill event と対応する recursive-drop を container identity で組にする MIR/cert 統合（型付き入れ子要素 model + cert section）。これにより __copy_value/__varr_copy/value.merge が leak 下限に依存せず証明済み均衡を継承する。元の範囲を統合設計のため下に残す：

<a id="original-scope--the-integration-that-consumes-coownloopv-still-fresh-engineering"></a>

### 元の範囲 — CoownLoop.v を使う統合（依然新規 engineering）

小さくて健全な cert 証明 slice は存在しない。「構造的に証明済み map-fill に帰着する」案は誤り：map-fill の各 iteration 本体は net 0（rebind：旧を drop + 新を inc）で、だからこそ OwnershipLoop.v の RC 保存 Loop 規則が受理する。共同所有 copy loop は各要素で net +1 となり、OwnershipLoop.v（header 19–21 行）は入れ子共同所有を明示的に後回し / 除外する。均衡させる rc_dec は、cert が空の別 raw-handle 再帰 routine（__vdrop_arr/__drop_value）にあるため、単一関数の fold では均衡の witness を得られない。

本当の Brick 3 に必要なもの（全て新規、複数週）：
1. 型付き入れ子要素 MIR model — load64 由来要素 handle を未追跡 raw i64 から、追跡対象だが入れ子の類（`ElementOf(container)` 関係を持つ Op）へ昇格。要素 e の rc_inc を ghost rc[o]=0 でなく container の child account に記録する。
2. producer が蓄積する child inc と recursive-drop の child dec を container identity + 要素数で組にする cert section（producer loop と別 drop loop をまたぐ全体的性質で、iteration 単位ではない）。
3. Coq 開発 `CoownLoop.v`（container shape 上の整礎帰納法 — List[List[String]] は 2 階層で、leaf 数 ≠ intermediate 数）+ RuntimeModel block model（container length field + 正確に N 回の child dec を出す rt_recursive_drop）+ 合成補題 SUM(co-own incs) == SUM(recursive-drop decs) + 新 cert section を Extract.v + driver.ml に通す。

不可分：共有する入れ子要素 model + 合成補題なしに、一つの producer（reduce_value の __copy_value）だけを cert 証明できない。独立した新規形式化 brick として予定する。

<a id="containment-hardening-the-tractable-near-term-2026-06-24"></a>

## 封じ込めの強化（近期に取り組めること、2026-06-24）
信頼の下限は維持：load64 由来 RC は未モデル化の無操作だが、rc_inc/rc_dec whitelist（calls_p4.rs — 名前付きの信頼する producer/consumer だけが rc_inc/rc_dec を指定でき、他の .almd fn が呼べば lowering で拒否）と leak-loop fixture（value_array、value_as_array、list_set_value、list_sort_str、value_merge、および今回追加の value_object）が gate する。任意の追加強化（保留、価値低）：手動 whitelist を registry 化（GOTCHA 3 — 手編集 match arm で増える）。

<a id="brick-3-integration--the-precise-fresh-engineering-spec-consumes-coownloopv-2026-06-25"></a>

## Brick 3 統合 — 精密な新規 engineering 仕様（CoownLoop.v を利用、2026-06-25）

均衡は関数をまたぐ（producer `__varr_copy` が N 個の child inc を充填し、均衡用 N 個の child dec は別の `__drop_value` にある）ので、関数単位 checker の変更では動かない。`__varr_copy` だけで child RC をモデル化すると誤って拒否する（dec は別にある）。CoownLoop.v は原理を証明し、統合は両側を container ごとに組にする必要がある。設計：

1. **型付き入れ子要素 model（MIR/lib.rs）** — 追跡済み container の要素 offset から `prim.load64(container + 12 + i*8)` を load すると、`ElementOf(container)` handle を得る（任意 address の ghost rc[o]=0 ではない）。carrier RC brick の load64 に欠けた provenance がこれ。
2. **共同所有 producer 認識** — `0..len(container)` loop で各 `ElementOf(dst)` に rc_inc し `dst` に store する fn は、抽象 event `ContainerFill dst` を一つ出す（N 個の child inc、N = len）。whitelist（calls_p4.rs）が認識集合になる。
3. **再帰 drop 認識** — `0..len(v)` loop で各 `ElementOf(v)` を rc_dec する `__drop_value`/`__vdrop_arr`/`__vdrop_obj` は、`ContainerDrop v`（N 個の child dec）を emit。
4. **cert section + 対応付け** — 呼出し元で container（value.array/object/copy result）を alloc（`i`、block）+ fill（`ContainerFill`）し、後で drop（`DropValue` = block `d` + `ContainerDrop`）。証明済み checker は `ContainerFill c` と `ContainerDrop c` を container identity で組にし、CoownLoop.v が任意の N で要素ごとの均衡を与える。
5. **合成定理（Coq）** — block 均衡（OwnershipChecker.v i/d）かつ child 均衡（CoownLoop.v fill/drop）の container は、漏れも二重解放も完全にない。これが二つの model を結ぶ一つの新補題。他は全て Rust（ElementOf provenance + 二つの recognizer + cert section）。これにより __copy_value/__varr_copy/value.merge は cert 証明済みとなり、RC whitelist（calls_p4.rs）は廃止される。

それまでは whitelist + leak 下限 fixture が信頼する gate であり、今は CoownLoop.v を根拠とする（calls_p4.rs comment）— 証明済み共同所有 / 再帰 drop パターンに従う名前だけがそこに属する。

<a id="brick-3-integration--coq-half-done-rust-half-confirmed-multi-week-2026-06-25"></a>

## Brick 3 統合 — Coq 側完了、Rust 側は複数週と確認（2026-06-25）

合成側は着地し kernel 検査済み：proofs/CoownCompose.v の `lifecycle_safe`（block-balanced ∧ children-source-owned ⇒ fully safe）は OwnershipChecker.v（block）と CoownLoop.v（child）を合成する。proof gate に入っている。

ELEMENTOF（Rust cert section）側を敵対的検証で範囲査定し、期待した小 slice を反証した：
- 「既存 block i/d 対応に乗る strict refinement（corpus 回帰なし）」という主張は偽。共同所有 producer の fill（`__varr_copy`）と対応する再帰 drop（`__vdrop_arr`/`__drop_value`）は別関数なので child account は本質的に関数横断であり、関数単位の cert refinement では捕捉しない。
- reduce_value の str-a/arr-a leak は捕捉しない。その leak は load64 由来 child の解放不足（再帰 drop は存在し実行される。不均衡は関数横断・要素数を鍵にした count で、load64 由来なので block account と carrier RC brick の両方から不可視）。捕捉には完全な load64→ElementOf(container,index) provenance + 両関数間の ContainerFill↔ContainerDrop 対応が必要 — 複数週の新規 engineering と確認し、別に追跡する。

当時取り組み可能（着地済み）：実際の信頼の要である RC whitelist を、一つの共有 source（crates/almide-mir/src/coown_names.rs：COOWN_PRODUCERS / COOWN_DROPS / COOWN_SET_REPLACE）へ昇格。CoownLoop.v + CoownCompose.v に基づき、calls_p4.rs が使い、`coown_names_documented` test で固定して producer/consumer の両側が黙って乖離しないようにする。corpus-wall ACCEPT（挙動維持）、mir 503/0。
