<!-- description: 決定性 belt の歴史的設計、廃止した type-state 境界、現行証拠の限界 -->
<a id="the-determinism--purity-belt"></a>

# 決定性／純粋性 Belt

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](determinism-belt.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](determinism-belt.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](determinism-belt.zh-CN.md)

> 非規範的な歴史参照資料。固定原本は変更せず正本として保持する。自動同期、新たな証明・ランタイム・測定・CI 実行はしていない。進捗層と旧実装の説明は履歴として読む。cert／モデル内の証明と範囲限定の実験的確認は、普遍的な成果物安全性や航空認証ではない。決定性 belt 原文は v0 type-state 境界を廃止済みと明記する。強い旧引用と安定 sort／入力順に関する主張も、新たな保証でなく歴史主張として保持する。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/determinism-belt.md)

<a id="reading-this-historical-belt-design"></a>

## この歴史的 belt 設計の読み方

以下の `Verified → Canonical → emit_wasm` type-state chain は廃止した v0 emitter のもの。**現行の出力境界ではない**。[codegen/lib.rs](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-codegen/src/lib.rs) に #782/#930 での廃止を記録し、現行 Wasm 経路は [wasm_route.rs](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/src/wasm_route.rs)。impurity lint と差分 determinism gate は対象範囲内の証拠であり、browser compiler が決して crash しない、全 host が同じ bytes を出す、という定理ではない。以下は元の実装記録と限界である。

> compiler の**出力決定性＋target 可搬性**のための Perceus 類似物。AlmidePerceusBelt は「出力すれば RC は釣り合う」を定理にし、こちらは「出力すれば出力は (IR, target) の純粋関数」を構築により強制する。したがって wasm32（browser playground）の compile は crash も x86-64 との乖離もしない、という構想。

<a id="why"></a>

## なぜ必要か

compiler は browser playground 内で **wasm32-unknown-unknown** に compile されて動く。全 verifier（stack-effect、Perceus、spec suite）が見逃した2つの bug が出荷された。どちらも同じ分類で、codegen が `(IR, target)` の純粋関数でなく非決定的／impure／非可搬な源を読んでいた。

1. `std::time::Instant::now()` の無条件呼び出し（ALMIDE_PROFILE 計時）→ clock がない wasm32-unknown-unknown で panic → 全 browser 内 compile が crash。（v0.23.13 は呼び出しを修正。Belt はその記述自体を不可能にするという構想。）
2. `HashMap`/`HashSet` の反復が WASM function/table/index/offset 順序を割り当てた。hashbrown の反復は host pointer 幅に依存（h2 = `hash >> (usize_bits-7)`）し、さらに `Sym` key の map は intern 順序（process 履歴）にも依存する。→ wasm32 が異なる／trap する module を出力。

spec suite は x86-64 上でしか compiler を動かさない。最初の determinism gate は WASI time を**持つ** wasm32-wasip1 を使い、bug #1 を見逃した。Belt は構造的な回答である。

<a id="the-four-layers-strong-inner-gates-carry-the-load-broad-outer-gates-catch-what-types-cant"></a>

## 4つの層（強い内側 gate が支え、広い外側 gate が型で捕捉できないものを捕捉）

| 層 | 内容 | 強度 | 状態 |
|---|---|---|---|
| **L1 Purity wall** | compile 経路の crate は clock/RNG/thread/atomic-counter 源を*名指しできない*（grep gate `scripts/check-forbidden-impurities.sh`＋計画中の `clippy.toml` `disallowed-methods`）。計時は `almide_base::profile::ProfileTimer` のみ（cfg-gated、wasm-safe） | lint/deny（文字列ベース、alias に弱い。Perceus 級 type-state **ではない**） | **完了（Phase 0/1）** |
| **L2 `DetMap`/`DetSet` prelude** | codegen 経路 crate が*反復*できる collection は content 順（BTreeMap ベース）のものだけ。通常 `HashMap` は監査済み `// DET-ESCAPE` の後ろの lookup-only cache にだけ残る | type-state | roadmap |
| **L3 `Canonical<IrProgram>`** | 終端の `CanonicalizePass` が emit 順の function Vec を content 由来順に sort。`emit` は `Canonical`（`Verified` の兄弟）のみ受け入れ、`emit_wasm::emit` を `pub(crate)` にして gate の迂回を防ぐ。上流 pass が hash 順 Vec を作っても再正規化 | type-state（真の Perceus spine） | **完了** |
| **L4 Runtime tripwire** | `check-host-determinism.sh`（wasm32-wasip1）＋`check-browser-determinism.sh`（node 経由 wasm32-unknown-unknown）が compiler の出力 bytes を native と比較 | test（open-world の後ろ盾、恒久） | **完了** |
| L5 Lean proof | `canonicalize` は冪等で入力順非依存 | proof | 任意 |

<a id="done-so-far-v02313--this-work"></a>

## ここまでの完了事項（v0.23.13＋本作業）

- **L1**：`almide_base::profile::ProfileTimer`（唯一許可された wasm-safe clock）。codegen の clock 3箇所をそこへ通した。`scripts/check-forbidden-impurities.sh` は `crates/{almide-codegen,almide-frontend,almide-optimize,almide-ir}/src` 内の生の `std::time|Instant|SystemTime|thread::spawn|thread_rng|fastrand|AtomicU64|fetch_add` で CI を失敗させる（`/generated/` `/runtime/` は除外。それらはユーザー native program に出力される runtime code）。CI の `checks` job に接続。
- **実際に発現していたデータ順漏れ**：`record_fields`/`variant_info` → `BTreeMap`。`closures.rs`/variant-eq/name-section を sort。**`almide-optimize/src/mono/mod.rs` の `all_instances`/`new` → `BTreeMap`**（特殊化関数の append 順 → WASM function index が HashMap 順で、host 幅＋intern 順依存だった）。
- **L4**：両 gate を CI に接続。`spec/wasm_cross/` fixture に `playground_default.almd`、`generics_mono.almd`（単相化）、`anon_records_and_fusion.almd`（匿名 record 命名＋egg-fused pipeline）を含む。

<a id="l3-shipped-this-pr--the-type-state-spine"></a>

## L3 を出荷（この PR）— type-state spine

実 codebase の監査（emit 境界＋Sym 反復面＋egg の実効性＋全 emit 順 collection）に基づいて実装し、**2つの前提を訂正**した。

- **`CanonicalizePass`**（`pass_canonicalize.rs`）：終端 WASM pass。`program.functions` と各 `module.functions` を `(is_test, name)` で安定 sort。emit 時に関数は名前で解決するので安全（WASM func index は並べ替えるが意味は保存）。`program.modules`/`top_lets` は意図的に並べ替え**ない**（top-level init は逐次で、後の `let` が前の値を観測し得るため順序に意味がある）。`type_decls` も同様（variant-tag 順序）。冪等。
- **`Canonical<'a>` type-state**（`lib.rs`、`Verified` の兄弟）：`Canonical::certify` は `Verified` を消費し（RC 検証済みが前提）、`is_canonical` を assert。WASM emit は `Canonical` だけを受け入れ、**`emit_wasm::emit` を `pub(crate)` に降格**。誰でも直接 `emit` を呼べた迂回路を閉じる。codegen 経路は `codegen → Verified → Canonical → emit`。
- **egg counter — 実際の漏れだったが修正済み**：`EGG_FRESH_COUNTER`（`AtomicU64`）が長寿命 process の compile 間で `__egg_v{n}` を変動させた（Rust target で確認。name section が関数だけを名付ける WASM では潜在）。`vt.len()`（直後の `alloc` が割り当てる `VarId` と同じ）へ置換。一意で、compile ごとに決定的で、global state なし。*これにより、egg-lab が対象に入れば L1 grep が検出する生 atomic も除いた。*
- **匿名 record 命名 — 「地雷原」を解決**：`__anon_record_{record_fields.len()}`（走査順 counter）→ `__anon_record_{FNV1a64(field-shape)}`。content の純粋関数（`RandomState` を避けるため `DefaultHasher` でなく FNV）。「全域 content key がない」懸念を解消する。code がすでに計算する dedup key が、その全域 key である。
- **`default_fields`**（Rust struct-literal emit）：`HashMap.keys()` 順 → sort。最後の RandomState 分類の emit 順箇所（Rust target）。

検証済み：debug assert 有効で `spec/` suite 全 **240/240**（`Canonical`/事後条件検査は一度も発火せず）。繰り返す compile 間で Rust＋WASM 出力が**byte-identical**。

<a id="deferred-honest-limits"></a>

## 延期事項（正直な限界）

- **L1 は alias に弱く `#[allow]` で迂回できる**。得られるのは coverage＋CI 失敗で、Perceus 級の不可能性ではない。真の spine は L3（当時出荷済み）。
- **`Sym` interner の危険は実在するが、今は潜在で実発現していない。** `Sym: Hash` は process-global の `ThreadedRodeo` 上の intern 順 `Spur` id なので、`HashMap<Sym,_>` 反復順は process 履歴に依存する。しかし監査では compile 経路内で真に `Sym` key の map は**すべて** lookup-only、diagnostic 用、または出力 bytes に到達しない `DefId` 割り当て用であり、当時反復順から漏れるものはない。潜在的落とし穴は残る（出力へ流れる次の `for (sym, _) in map` で発現する）。L2 は `Sym` key map の反復を禁止すべきで、`lower/mod.rs` の `env.top_lets` を `Sym::as_str()` で防御的に sort するのは安価な保険。まだ未実施。
- **L3 は WASM 専用**（`Verified` と同じ範囲）。Rust target は module を flatten し、egg＋`default_fields` 修正で覆われるが type-state gate ではない。Rust emit への `Canonical` 拡張は将来作業。
- **L3 sort key の全域性**：`(is_test, name)` の**安定** sort で処理する。真の名前衝突はすでに決定的な上流順へ fallback するため、canonicalize は冪等・入力順非依存を保つ、という説明。地雷原は anon-record counter であり、今は content key 化。
- **どの層も成果物そのものを認証しない**。決定性は由来の性質であり、bytes を手にする時点では失われている。構築層が制約するのは*過程*。emitter の忠実性＋未知の N+1 番目の源の軸は、L4 の経験的確認だけで閉じる。runtime gate への依存は過渡的でなく恒久。

<a id="next-highest-leverage-step"></a>

## 次に最も効果的な一歩

L3 が構造的不足を閉じるため、残る作業は外側の強化。（1）`clippy.toml` の `disallowed-methods` deny（Instant::now、SystemTime::now、thread_rng、fetch_add）＋`#![deny(clippy::disallowed_methods)]` で L1 を grep から compiler 検査へ。（2）L2 `DetMap`/`DetSet` で潜在的な `Sym` 反復の危険を記述不能にする。（3）任意の L5 Lean proof で `canonicalize` の冪等性＋入力順非依存を証明する。
