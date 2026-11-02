<a id="map--set-data-structure-roadmap"></a>

# Map / Set のデータ構造 roadmap

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](map-data-structure-roadmap.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](map-data-structure-roadmap.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](map-data-structure-roadmap.zh-CN.md)

> 非規範の固定読者用スナップショット。変更しない原文を正本とし、機械入力として置き換えない。自動同期や独立実行はしない。歴史的測定・提案・原文時点の実装更新を区別して保持し、現在の正しさや性能の独立証明にはしない。コードとコメントは原文どおり保持する。 冒頭の現行表現の節を踏まえて歴史的本文を読む。native Map/Set は現在 index を持てるが、小規模・hash 不可能な場合は線形経路、hash-flood は順序付き fallback、永続更新はコピーを残す。旧本文の無限定 O(1)・Vec 専用・dense Set や HAMT の「唯一の構造」は歴史的主張として保持し、現在の最悪時保証や独立比較調査にはしない。HAMT を実装済みとしない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/map-data-structure-roadmap.md)

> 元 metadata の非活性要約：却下した seq-in-entry 設計とその誤りの理由を含む Map / Set データ構造 roadmap。

<a id="current-representation-versus-the-earlier-design"></a>

## 現在の表現と以前の設計

native Map と Set の実装は現在、挿入順の entry を保持しつつ**index を追加できる**。native はもはや恒久的に Vec 専用ではない。[map.rs](../../../../../../runtime/rs/src/map.rs) と [set.rs](../../../../../../runtime/rs/src/set.rs) を参照。小さな collection と、hash 化できない key/element 型（Float を含む）は線形経路を維持する。index 付き lookup は無条件の最悪 O(1) 保証ではない。hash-flood 処理にも順序付き fallback があり、永続的更新は依然データをコピーする。

残りは、以前の compact-ordered-dict 選択、却下した seq-in-entry 実験、HAMT 調査の記録である。v0 emitter のパス、dense Set 前提、提案された native index は歴史的記述であって現在のソース構成ではない。本更新は HAMT 案の実装済みを宣言しない。

`Map[K,V]`（および `Set[T]`）をどう表現し、どこへ向かわせるか。

<a id="decision"></a>

## 決定

**`Map` は O(1) の挿入順 hash map、compact-ordered-dict（②）とする。** これは標準的な「正しい」map（Python dict / `indexmap`）であり、「どちらも同じ大規模一括書き換えなら、より良い構造にする」という判断。`Set` は dense list のままとする（要素は `PartialEq` で `Float` の可能性もあり、hash index を求めない）。両方とも**挿入順**で反復する。ターゲット間等価性（native の反復 == wasm）は gate で強制する厳密な不変条件である。

ここまでに出荷したもの:
- **native** の `Set` + `Map`（`runtime/rs/src/{set,map}.rs`、PR #354）: `Vec<T>` / `Vec<(K,V)>`。初出時の挿入順を保ち、`insert` は位置を維持して値を上書き、または新規追加し、`remove` は残存要素の順序を保つ。等値性は順序に非依存。key/elem 境界は `PartialEq`（`Eq + Hash` ではない）であるため、`Set[Float]` や非 `Hash` key の map が動く。**これは反復順序に対するターゲット横断の参照オラクル**であり、native は `Vec` のままとする（native hash index を持たない理由は②参照）。
- **wasm `Set`**: dense な `[len][cap][data…]` リスト（挿入順）。

**出荷済み: wasm `Map` → compact-ordered-dict（②）。** ブランチ `xtarget-wasm-map-order`（layout 定数 `709b6764`、helper `715acc1e`、op 切替 `89a5833e`）。dense な `(key,val)` entry（挿入順、`0..len` を反復）と、O(1) lookup 用の別の hash INDEX 領域（tag + 1 始まりの slot pointer）。集約した `emit_dict_*` helper と名前付き `layout::map` 定数が全 offset に名前を与える（magic number の解消）。全 map-layout 箇所を同時切替した（`calls_map.rs` + `calls_map_closure.rs` + `control.rs` の for-in、`expressions.rs` の literal、`equality.rs` の eq、`calls_list_closure2.rs` の group_by、`calls_http.rs` の headers）。`calls_value.rs` は map ではないと確認した（その `=8` offset は `List[(String,Value)]` object payload）。全 spec suite と全 `spec/wasm_cross/*.almd` で native==wasm の byte-identical を検証した。新しい `map_insertion_order.almd` が順序保証を gate に固定する（従来コーパスは順序非依存の件数だけを検査していた）。使われなくなった Swiss helper（`emit_map_resize`/`emit_alloc_table`/`emit_swiss_setup`）は削除され、`set` は現在 load factor に応じて拡張する（16 entry 超の overflow を修正）。完全な記録: memory `project_wasm_compact_ordered_dict`。

> ⚠️ 以前の **seq-in-entry** 試行は破棄した。entry の *stride* を変えた誤った小細工で、手書き stride 箇所すべてに手を入れる必要が生じ、magic number が増殖し、6 つの corruption trap を引き起こした。正しい COD は entry stride を変えない。entry は `ks+vs` の dense 配置を維持し、INDEX は*別領域*とする。

<a id="rejected-alternative--①-plain-dense-list-on"></a>

### 却下した代案 — ① 単純な dense list（O(n)）

wasm Set + native Vec をそのまま模した、dense な `Vec` 風 map（線形走査 lookup）。最も単純で、操作ごとのリスクが最小、magic number はゼロ。却下理由は、`Map` が密かに O(n) となり、可変操作・読み取り中心のコードに落とし穴を作ること。hash map は O(1) *であるべき*だからである。（永続的 clone-on-write モデルではどちらにしても*構築*が O(n²) となり、②の O(1) 利得が弱まる。①が魅力的に見えた唯一の理由はこれだった。）

<a id="②-compact-ordered-dict--the-chosen-map-structure"></a>

## ② Compact-ordered-dict — 選択した Map 構造

「正しい hash map」— Python 3.7+ `dict`、Ruby `Hash`、Rust `indexmap`。**dense な挿入順 entry** 配列に対する hash **INDEX**。lookup は O(1)、反復は依然挿入順（dense entry を走査）となる。

- wasm: TAGS（h2 による高速拒否）+ INDEX（dense entry への 1 始まり pointer）+ DENSE ENTRIES。lookup は index を probe し、反復は dense entry を走査する。拡張時に entry は移動しない（index だけを rehash）。完全設計と集約した `dict_*`/`layout::map::{INDEX_SLOT_SIZE,EMPTY_INDEX,INITIAL_CAP,GROWTH_SHIFT,LOAD_NUM,LOAD_DEN,H2_SHIFT,H2_MASK}` 定数は既に足場を作ってある（workflow `wf_1a3614dd`、memory `project_wasm_compact_ordered_dict` 参照）。
- native: dense な `Vec<(K,V)>` + hash index。標準の `HashMap<K,usize>` index は `K: Eq+Hash` を強制し、**`Map[Float]` と `Map[record-without-derive-Hash]` を拒否**するため、raw byte で任意の key を hash する wasm と不一致になる。②でターゲット間 parity を維持するには、両ターゲットに普遍的な **「map key は hashable+eq でなければならない」**契約を採用するか（簡潔だが現在の `PartialEq` 緩和を再制限）、wasm の hash に一致するカスタム**byte-hashing** index を native に設けるか（コードは増えるが任意の key をサポートし、真に等価な選択肢）、どちらかにする。

**選択理由:** 「key は hashable でなければならない」は普遍的な標準（本格的な言語はどれも要求する）であり、O(n) map は落とし穴だから、②が正しい *map* 設計となる。②と①のどちらも同じ大規模一括書き換えなので、優れた構造を選ぶ。**受け入れた注意点:** 永続的 clone-on-write モデルでは O(n²) の構築が支配的なので、②の O(1) lookup が主に効くのは可変の `insert`/`delete` チェーンと、安定した map への繰り返し `get`。真の理由は profile ではなく構造的正しさ（「Map は O(1)」）である。seq-in-entry の小細工で問題になった magic number のリスクは、②を*正しく*行うことで対処する。entry stride を変えず別 index 領域を設け、`dict_*` helper に集約し、inline のバイト計算は決して使わない。カスタム byte-hashing index を追加しない限り、それまでは native 側を `Vec`（順序オラクル）のままにする。標準 `HashMap` index は採用しない（その `K:Hash` 境界が上記の回帰を生む）。

<a id="③-hamt--the-true-persistent-ideal-research"></a>

## ③ HAMT — 真の永続構造の理想（研究）

永続 map の最終目標: **hash array mapped trie**（Clojure / Scala / immutable.js）。**構造共有**を伴う O(log n) 操作により、`map.set` は O(n) clone をしない。これは①と②に共通する O(n²) の永続構築を直す唯一の構造である。代償は圧倒的に大きい実装であり、native（Rust）と手書き出力 wasm ランタイムで byte-identical にするだけで独立プロジェクトになる（これは「単一 runtime」という最終目標を強く支持する。一つの Rust runtime を wasm32 へコンパイルするか、map 自体を Almide で定義する）。永続 collection の長期方向として追跡する。

<a id="cross-cutting"></a>

## 横断事項

表現が何であれ、**厳密な不変条件は観測可能なターゲット間等価性**である（native の反復 == wasm の反復。`tests/wasm_runtime_cross_target.rs::wasm_cross_target_spec` が強制する）。①→②→③ のどの移行でも、全 `spec/wasm_cross/map_*` ケースを byte-identical に保ち、`@xt-allow` を追加してはならない。Set は dense list のままでもよい（Set[Float] が必要とするのは `PartialEq` だけ）。index の問題は Map 固有である。
