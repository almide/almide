<a id="cross-target-completeness-the-lid"></a>

# クロスターゲット完全性（蓋）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](cross-target-completeness.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](cross-target-completeness.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](cross-target-completeness.zh-CN.md)

> 非規範の読者用スナップショット。固定原文を正本とし、自動同期や現在の独立実行・検証はしない。歴史的な一覧と日付付き比較調査であり、現在の欠陥・保証・独立検証済み競争上の主張ではない。以下の「Active」「稼働中」「すでに閉じている」は原文当時の表現を保持し、冒頭の証拠の境界がそれら歴史的箇所にも適用される。運用指示は実行しない。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/cross-target-completeness.md)

原文の説明メタデータ：歴史的なクロスターゲット解消戦略、日付付き一覧、差分証拠の限界


<a id="historical-drain-queue-and-present-evidence-boundary"></a>

## 歴史的な解消キューと現在の証拠の境界

以下の 2026 年 6 月の一覧は、現在の欠陥キューではない。そこで数えた v0 ランタイムのレジストリは廃止され、native base64、セルフホストされたアルゴリズム、インタープリター／ファザーが現在は存在する。現在の操作の可用性と例外範囲は [target availability](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/target-availability.toml) と [the contract ledger](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/contracts.toml) に記録されている。

「四つの封印」は保証の戦略であり、あらゆる不一致が不可能だという証明ではない。有限のコーパス／ファズの一致では、その全称命題を確立できない。文書化されたホストの観測や資源制限にも範囲がある。言語比較調査と唯一性の主張は、著者による日付付き調査として保持し、再検証した競争上の主張とはしない。以下は歴史的な測定と提案された解消策である。

> Native(Rust) と WASM は、すべてのプログラムで **バイト単位で同一の (stdout, stderr, exit)** を生成しなければならない。
> ゲートは存在し、**既存分として容認した例外はゼロ**。このロードマップは「発見したものはすべて修正した」から「不一致は構造的に不可能」へ至る方法と、なお列挙されているものの稼働中の解消キューを示す。

状態：**Active** — 解消フェーズ。ゲート、レジストリ、ラチェットはすべて稼働中。

<a id="verified-positioning-2026-06-05-audit-11-languages-surveyed"></a>

## 検証した位置付け（2026-06-05 の監査、11 言語を調査）

ほかの複数ターゲット言語は、このベンチマークを主張していない。業界の三つの姿勢はいずれも弱い。免責して委譲する方式（Gleam の「by design」、Haxe、Dart）、バイト差分なしの意味論レベルの共有テスト（Scala.js、Kotlin MP）、単一ターゲットによる回避（Elm、ReScript は native backend を廃止）である。最も近いアーキテクチャ（直接 wasm 出力と Perceus 系 RC）を持つ MoonBit は、同値性をまったく主張していない。

> **「Almide は、二つの backend にバイト単位で同一の (stdout, stderr, exit-code) 出力を CI ゲートで要求する唯一の言語だ。すべての不一致は追跡され縮小する負債であり、当然として容認されるものではない。」**

<a id="current-state-2026-06-06"></a>

## 現状（2026-06-06）

| 仕組み | 状態 |
|---|---|
| クロスターゲットゲート（`tests/wasm_runtime_cross_target.rs::wasm_cross_target_spec`） | 65 以上のコーパスファイルをバイト比較、**0 @xt-allow** |
| 文書化された不一致リスト | **N = 0**（最後の項目 `fan.timeout` は 0.29.0 で言語から削除され、C-006 が active へ移った） |
| oracle ペアリングレジストリ — v0 emitter とともに廃止（#782）。`scripts/check-rt-oracle-registry.sh` は廃止の痕跡 | spec/wasm_cross と interp の三者 oracle に置換（v0 最終集計：検証済み 76／既存分として容認 42） |
| 構成上のテーブル（出力時に Rust std から導出する Σ-probe と全スカラーの CI 固定） | case folding、空白、UTF-8 分類、Unicode プロパティ（Alphabetic/Alphanumeric/Uppercase/Lowercase） |
| CI 内の Lean カーネル検査 | 41 定理（Perceus RC／closure env／heap）。sorry を grep するだけでなく `lake build` |
| その他のラチェット | fmt 往復性質（skip なし）、ホストアーキテクチャに依存しない決定的出力、スナップショット群 |

当初の 8 クラスター一覧（46 バグ）は**すべて解消済み**（#363–#385）。
既存容認分の一掃一覧（未検証 48 routine 内の実在不一致 47 件）は、**13 件解消、34 件を以下に列挙**。

<a id="the-lid-four-seals-each-finite"></a>

## 蓋：四つの封印、それぞれ有限

新しい不一致が存在するには、これらすべてをすり抜ける必要がある：

1. **routine の封印** — `grandfathered` を **0** まで減らす。すべての wasm runtime routine を、(a) 構成から導出（テーブル）、(b) 網羅的差分テスト（小領域）、または (c) N 百万回のファズ差分テスト（大領域）のいずれかにする。#382 ゲートは未登録の新規 routine をすでに阻止する。件数が 0 になったら、既存容認分を新たに増やさない CI assert を加える。
2. **合成の封印** — routine が同一でも合成は封じられない（fan.map の inline-lambda の罠は CHECKER のバグだった）。構成要素：ConcretizeTypes の**強制的な** AllTypesConcrete 事後条件（Unknown→i32 の無言の誤コンパイルがビルドエラーになる）。続いて約 30 ノードの IR core に対する**参照インタープリター**（実行可能な仕様。三者差分でどの backend が不一致かを特定）と、**毎晩の生成ファズ**（型誘導、約 44k–175k プログラム／夜、自動最小化、自動コーパス昇格）。「不一致ゼロ」は、ファザーが新たな不一致を出さなくなってからのみ宣言できる。
3. **将来コードの封印** — すでに閉じている。クロスターゲットゲート、oracle ペアリングゲート、Lean カーネル検査、fmt 往復をすべての PR で実行する。
4. **カーネルの封印**（長期）— アルゴリズム系 stdlib を Almide でセルフホストし、両ターゲットが同じソースを動かすようにする（「二つの実装がずれる」源を閉じる）。証明対象を約 30 IR op と RC と layout へ縮小し、それらを証明する（操作ごとの定理）。**セルフホストはここでは決定により保留（2026-06-05）。着手が指示されるまでロードマップのみ。** `research/spike/v1-mir/` を参照（旧 `research/selfhost/` probes は削除された）。

<a id="live-drain-queue-all-enumerated-coordinates-known"></a>

## 稼働中の解消キュー（すべて列挙済み、位置特定済み）

<a id="pr-d--encodings--json-path-13-divergences-next-up"></a>

### PR-D — エンコーディングと json path（約 13 不一致、次の対象）
- **base64 は両ターゲットで壊れている**。native はビルド失敗（E0425 — `almide_rt_base64_encode/decode` が存在しなかった）、wasm は空を返す／正しい入力を拒否する。native を一から実装し、wasm を修正する。
- `int.from_hex`：`0X` 接頭辞、`'_'` の読み飛ばし、繰返し `0x` の意味論が不一致。
- `bytes.read_f16_le`：wasm は ±inf を ±f32::MAX（有限値！）として表す。
- `hex.decode` のエラーメッセージ詳細（native には位置／長さの接尾辞がある）。
- `json.get/set/remove_path`：wasm に負添字の正規化がない。`set_path` で object に対する Index はヒープのごみを読む。範囲外 Index は **trap 134** となるが、native は失敗しない no-op。これらすべてが入れ子で伝播する。

<a id="pr-b--wasm-regex-engine-port-11-divergences-biggest-chip"></a>

### PR-B — wasm regex engine の移植（11 不一致、最大の塊）
wasm engine は構造的に壊れている。バイト単位（複数バイトの検索対象 → U+FFFD のごみ）、`|` をリテラルとして扱う、escape atom は `\d\w\s` だけ（`\.` `\n` `\D` `\W` `\S` や escape されたリテラルは一致しない）、クラス escape がない（`[\d]` = `{'\', 'd'}`）、capture が誤った group を返す。
修正は native の手書き engine（`runtime/rs` の `rx_parse_*`、scalar 単位）を wasm 出力へ忠実に移植し、差分ファズゲート（ランダム pattern × 複数バイトを含む検索対象）を加えること。
バックトラッキング engine（fuel 上限付き）について Lean の**停止性定理**を検討する。網羅テストもファズも確立できない性質である。

<a id="pr-c--math-determinism-16-divergences-needs-a-contract-decision"></a>

### PR-C — math の決定性（16 不一致、契約判断が必要）
wasm の多項式近似は native libm と至る所でずれる（sin(π) は 8 桁ずれる）。契約にかかわらず明白なバグ：`log(0)`=NaN（native は -inf）、`exp(-745)`=0（native は非正規化数）、`fpow(-2, 0.5)`=1.414（abs のバグ、native は NaN）、`fpow(0,-1)`=0（native は inf）、`fpow(2, inf)` は **trap 134**、`to_fixed(x, ≥19)` は **trap 134**、`to_fixed` の丸めが誤る（0.05@1dp → 0.0 対 0.1）。
**契約の推奨案（要確認）**：native libm 自体がプラットフォーム依存（macOS と glibc の最終 ulp）なので、決定的な pure-Rust libm（musl 移植の `libm` crate）を NATIVE runtime に同梱し、同じアルゴリズムを wasm に移植する。native のプラットフォーム間決定性も得られる。先例は Java の `StrictMath`/fdlibm。

<a id="small-items-queued"></a>

### 小項目（待ち行列）
- enum 名で record variant を構築：native は拒否（E0574）するが **wasm は受理して実行**。frontend は両方で拒否すべき。
- 定数畳込みされた浮動小数の inf/NaN が、Rust として不正な識別子 `inff64`/`NaNf64` を出力（E0425 native ビルド失敗 = ICE 漏出クラス）。
- 幅指定整数の record フィールド（`a: Int8` を持つ `M { a: 5 }`）：native は E0308、wasm は次のフィールドを無意味な値として読む（#368 のクラス、構築箇所の残り）。
- wasm 上の再帰 generic 型表現（自己参照する `Tree[T]`）：T の payload を誤って表す（panic はなく、試す spec もない）。
- `Box` というユーザー型名と Box 間接参照が必要な再帰 enum が native 上で衝突する（既存問題）。

<a id="order-of-work"></a>

## 作業順序

PR-D → PR-B → PR-C（先に契約判断）→ 小項目 → AllTypesConcrete の強制的事後条件 → 参照インタープリター → 毎晩のファザー。各変更には `spec/wasm_cross/` 内のコーパスファイル、新テストを引用するレジストリの grandfathered→verified 切替、検証クラスの明示（`exhaustive` / `fuzz-N` / `lean` / `by-construction`）を伴わせる。

<a id="related"></a>

## 関連

- [correctness-guarantee-gaps.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/correctness-guarantee-gaps.md) — ホワイトボックスの層別証明軸（WasmIR、ANF、closure env、Perceus 適合）
- [determinism-belt.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/determinism-belt.md) — 構成によるコンパイラ出力の決定性
- [closure-cross-target-completeness.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/closure-cross-target-completeness.md) — closure の側面（大部分を解消済み）
- [llm-first-language.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/llm-first-language.md) — バイト決定性が MSR に役立つ理由
