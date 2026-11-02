# almide-codegen

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](CLAUDE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](CLAUDE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](CLAUDE.reader.zh-CN.md)

> [正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-codegen/CLAUDE.md)（固定コミット 35297a612fb29e3f78f6587aba253aa91d51f466）の非規範・不活性な読者用スナップショット。履歴参照資料であり、有効なエージェント指示や生成用プロジェクトテンプレートではなく、コマンド実行権限を与えない。自動同期されない。原文の主張は独立した現時点の測定ではなく、コマンド実行やモデル呼び出しは行っていない。

nanopass パイプラインとテンプレート walker を通じて IR を Rust ソースへ変換し、限定された `@gpu` WGSL ソース出力も行う。出荷される Wasm バイナリの経路はここにはない。`src/wasm_route.rs` が `crates/almide-wasm` の structural emitter に振り分ける。旧 `emit_wasm/` emitter と MIR Wasm renderer は廃止済み。MIR は引き続き native verified-renderer の部分集合を担当する。

> **理想形リファクタのロードマップ**: [docs/roadmap/done/codegen-ideal-form.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/codegen-ideal-form.md)
> 新しい codegen の修正を入れる前に、そこに挙げられた「場当たり修正を避けるべきポイント」を確認すること。特に: 関数解決は独立パスに、stdlib emit は宣言駆動、`emit_stub_call` による実行時 trap は避ける。

<a id="three-layer-architecture"></a>

## 3 層アーキテクチャ

<a id="1-nanopass-pipeline-pass_rs"></a>

### 1. Nanopass パイプライン（`pass_*.rs`）

それぞれが 1 つの仕事をする 25 個以上の意味変換パスを、`target.rs::build_pipeline` でターゲットごとに組み合わせる。Rust パイプラインの順序：UnifyVarTables →
ListPatternLowering → LambdaTypeResolve → ConcretizeTypes → MeteredRegionDelegate → MeterCharge →
PatternLiteralGuard → ResolveCalls → RegionWindow → BoxDeref → LICM → EggSaturation →
MatrixShapeSpec → ConstFold → IntrinsicLowering → BorrowInsertion →
OwnedSourceHoist → TailCallOpt → CaptureClone → CloneInsertion → MatchSubject → EffectInference →
StdlibLowering → StreamFusion → ResultPropagation → BuiltinLowering →
DecodeSlotHint → DecodeErrFrame → Peephole → RustLowering → FanLowering → NormalizeRuntimeCalls →
IrLinkFlatten → SharedCellBorrow → VarStorage → RangeCountingVars → TopLetStorage → BorrowLowering →
SliceBinders → StrMapKey → ThreadShareMarks → BoxTakeLowering → StrViewRebind

- 各パス：`impl NanoPass { fn run(&self, program, target) -> PassResult }`

<a id="2-toml-templates-templatestoml"></a>

### 2. TOML テンプレート（`templates/*.toml`）

各ターゲットの構文パターン。walker が `{var}` プレースホルダーを置換する。ターゲット間の違いは walker ではなく、ここに置く。

<a id="3-walker-walker"></a>

### 3. Walker（`walker/`）

ターゲット非依存の IR renderer。**`if target == Rust` のチェックはゼロ。** ターゲットに関する判断はすべて、パスとテンプレートで行う。

<a id="rules"></a>

## ルール

- **walker はターゲット非依存を維持すること。** ターゲット固有の動作が必要なら、nanopass またはテンプレート guard を追加する。
- **nanopass の各パスは独立している。** 各パスは IrProgram を読み書きする。順序について仮定できるのは宣言した内容だけである。すなわち `depends_on` / `run_before` の辺、または表現の境界（`UnifyVarTables`、`ConcretizeTypes`、`IrLinkFlatten`）を示す `barrier()`、そして `Postcondition` である。後者は単調であり、いったんパスが確立した条件は、すべてのプロファイル（release を含む。走査のコストはファイル当たり約 8 ms）で、後続の各パスの後に再検証される。そのため先行パスの不変条件を壊すパスは、先行パスとともに名指しされる。`ALMIDE_IR_FAULT=<pass>` は指定したパスの後に違反を注入する。これは release バイナリの陰性対照である。宣言された辺が依存関係のすべてを表しているかも検証する。`scripts/check-pass-shuffle.sh` は、辺が許すシード付きランダム順序（`ALMIDE_SHUFFLE_PASSES=<seed>`）でパイプラインを実行し、生成 Rust のバイト差分を取る。相違は宣言漏れを示し、`scripts/pass-shuffle-bisect.py` が該当する組を示す。新しいパスは、それを追加するのと同じ変更で辺も宣言する。
- **ここに `Target::Wasm` は存在しない。** この variant、その 26 パスのパイプライン、wasm 専用パス（完全な 2 つ目の Perceus を含む）、`Verified`/`Canonical` 証明書チェーンは #930 で削除された。tombstone 期の `unreachable!` 分岐も含む。Wasm リクエストはこの crate に到達する前に `almide-wasm` へ振り分けられる。ここに wasm 出力ロジックを追加しないこと。

<a id="history-note-the-rt-oracle-registry"></a>

## 履歴注記：rt-oracle registry

`rt-oracle-registry.toml` と差分テストのゲートは、廃止された v0 の `emit_wasm/` ランタイムが native ランタイムから乖離するのを防いでいた。現在、`scripts/check-rt-oracle-registry.sh` は RETIRED と通知して早期終了する。代わりに、v1 の self-hosted 標準ライブラリ実装は `spec/wasm_cross` fixture と almide-interp oracle によって検証される。

---

パイプライン一覧と過去の性能／実装に関する主張は固定原文の記述であり、新たに調べた現行ビルドの説明ではない。この資料で廃止済み Wasm emitter を復活させることはない。
