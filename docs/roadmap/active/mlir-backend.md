<!-- description: MLIR + LLVM backend arc — unified codegen, GPU target, toolchain distribution design -->
# MLIR / LLVM Backend arc

Almide の codegen を MLIR dialect + LLVM backend に再構築し、`emit_rust` / `emit_wasm` の 2 系統並列実装を解消、同時に WebGPU ターゲットを compiler infra の内側に取り込む arc。狙いは「言語 1 つで CPU / WASM / GPU 全部出せる、世界最高 compiler」というブランド完成。本 arc の成果物が `bonsai-almide/docs/PERF_ROADMAP.md` の軸 C / D / E を貫通する。

## 経緯 (2026-04-22)

- 現行 codegen は **nanopass + TOML template (Rust)** / **hand-rolled wasm-encoder (WASM)** の 2 系統
- PR #227 (WASM mha_core mask 修正) が示した通り、2 系統並行実装は**取りこぼし型バグ**の温床 — native 側は修正済みだったが WASM 側が数ヶ月放置された
- bonsai-almide が reference (webml-community/bonsai-webgpu, 51 tok/s) に届くには **WebGPU backend が必須**、CPU WASM の物理上限は 10-15 tok/s
- WebGPU shader を手書きする方針 (Path X) は「compiler が出す」Almide ブランドを壊すため却下
- MLIR を compiler infra に据えて CPU / WASM / GPU 全部 lowering pass で吸収する方針 (Path Y) を採用

前提リサーチ: `~/Downloads/llvm-embed-language-research-2026-04-22.md` (Rust / Zig / Mojo / Swift / Julia / Crystal / Nim / Odin / Pony / Vale の LLVM 統合 / 配布パターン比較、3540 語)。

## 設計決定 (事前合意済)

### 決定 A — LLVM は upstream tracking、fork しない

根拠: Rust/Swift/Julia/Mojo 全て LLVM fork で維持コスト払ってるが、Almide 初期は小チーム。Zig / Crystal / Nim の upstream tracking で十分、独自 pass は後付け可能。
実装: LLVM 18 LTS を submodule pin、四半期ごとに自動 bump PR を CI が起票。

### 決定 B — 標準 MLIR dialect + 最小限の custom

根拠: Mojo の KGEN 独自 dialect 路線はコミュニティ資産 (`linalg` fusion rule 等) と切り離す代償が大きい。  
採用: 標準 `func` / `arith` / `memref` / `linalg` / `vector` / `scf` / `gpu` dialect をフル活用。Almide 固有は **AlmideMatrix** dialect 1 個だけ (現行 `AlmideMatrix` ランタイム表現の MLIR bridge)。egg rewrite は MLIR **PDLL** (Pattern Description Language Lite) で記述、upstream への貢献余地を残す。

### 決定 C — Binary size は Zig 路線、50-80MB default

根拠: Zig は 53 MiB tar.xz で LLVM フル同梱。単 target LLVM (`LLVM_TARGETS_TO_BUILD=host`)、LTO、clang 抜き、LLD 同梱で実現。Rust 300MB / Swift 700MB / Mojo 500MB は商用 / enterprise 規模の言語、Almide が真似る必要なし。  
採用: default installation は **host target + LLD + WASM target のみ**、`-DLLVM_TARGETS_TO_BUILD=host;WebAssembly`、LTO on、clang 抜き。rustup 型 `almide target add X` で追加 target を後載せ。

### 決定 D — Dual backend、emit_wasm は dev 用に残す

根拠: Rust + Cranelift、Zig + self-hosted backend はどちらも「LLVM は重い → dev 時は軽い backend、release 時のみ LLVM」。LLVM が compile 時間の 70% を占めるという Rust の内部計測。  
採用: **現行 emit_wasm を debug/dev backend として残し、MLIR/LLVM path は release/production backend として追加**。`almide build --debug` は emit_wasm (1-2 秒 compile)、`almide build --release` は MLIR 経由 (数秒 compile、最適化フル)。WASM target も LLVM wasm32-unknown-unknown で統一、長期的に emit_wasm は opt-out に。

### 決定 E — LLD 同梱、system linker 非依存

根拠: Pony が 2026-03 に embedded LLD 採用、Zig は以前から同梱。system ld / macOS ld / lld-14 バージョン不整合に振り回される UX を切る。  
採用: LLD を LLVM と一緒に bundle、`almide build` が system linker なしで完結。cross-compile (linux x86 ↔ macOS aarch64) が default で通るようにする。

### 決定 F — Toolchain 配布は rustup 型

根拠: Zig の一括同梱は「全 target 常に欲しい」ユーザには OK、LLM 用途 / 組み込み用途でサイズが問題になる場合に柔軟性なし。rustup の target add 方式はサイズと利便のバランスが良い。  
採用: `curl | sh` で default binary + host target 取得 (50-80MB)、`almide target add webgpu` で追加 (30-50MB 差分)、versioned toolchain (`stable` / `nightly`) をサポート。詳細は **別 arc (toolchain-dist)** で起こす。

## 非目標

- **LLVM fork**: 初期は avoid、必要になったら判断
- **custom Mojo KGEN 相当 dialect**: AlmideMatrix 以外は作らない
- **runtime JIT**: AOT 専用、browser 実行時 LLVM 同梱は禁止
- **自作 LLVM alternative**: Odin + Tilde が 2026-04 に撤退した教訓に従う
- **Bazel / Buck 依存**: CMake + Cargo で閉じる、build system 複雑化避ける

## アーキテクチャ

```
Almide source (.almd)
  ↓ parse (almide-syntax)
AST
  ↓ typecheck + lower (almide-frontend)
Almide IR (typed, VarTable-ed)
  ↓
  ├─ [debug build] → almide-codegen (nanopass + TOML template) → Rust / WASM
  │                    ↑ 現行、残す
  │
  └─ [release build] → almide-mlir (dialect lowering)
                         ↓
                       MLIR module (AlmideMatrix + linalg + vector + scf + gpu + ...)
                         ↓ (MLIR pass pipeline: egg PDLL rewrite, shape spec, fusion, vectorize)
                       lowered MLIR (linalg → vector → llvm / spirv)
                         ↓
                         ├─ LLVM IR → LLD → native binary / wasm32
                         └─ SPIR-V → WGSL → WebGPU compute shader
```

Crate 構成 (予定):
```
crates/
  almide-mlir/          ← 新規、feature = "mlir" gate
    src/
      lower/            ← Almide IR → MLIR dialect 変換
      dialect/          ← AlmideMatrix custom dialect 定義
      passes/           ← shape spec、egg rewrite、fusion
      emit/
        llvm_native.rs  ← LLVM IR → native binary
        llvm_wasm.rs    ← LLVM IR → wasm32
        spirv_wgpu.rs   ← SPIR-V → WGSL
  almide-toolchain/     ← 別 arc、rustup 相当
```

## Stage 計画

### Stage 0 — 現状 (2026-04-22)

- `emit_wasm` / `emit_rust` 2 系統で稼働中
- bonsai-almide で WASM SIMD128 Q1_0 まで到達 (PR #229)、1.4 tok/s
- reference (WebGPU) 51 tok/s、gap 36x

### Stage 1 — melior + MLIR 最小 spike (2-3 週)

**ゴール**: `fn add(a: Int, b: Int) -> Int = a + b` を Almide IR → MLIR `func`+`arith` → LLVM IR → native binary まで通す。既存 `emit_rust` の出力と binary level で一致確認。

- [ ] `crates/almide-mlir/` skeleton 作成、`Cargo.toml` に `mlir` feature 追加
- [ ] `melior` crate 依存追加、LLVM 18 LTS を要求 (`llvm-sys` 版指定)
- [ ] build script に `llvm-config` 検出、なければ親切なエラー
- [ ] `lower::int_arith` で Int 演算を MLIR `arith.addi` / `arith.muli` / etc に落とす
- [ ] `emit::llvm_native` で MLIR → LLVM IR → object file → executable
- [ ] spec test: `spec/lang/mlir_smoke_test.almd` で `fn main() -> Int = 42` が動く
- [ ] CI: `mlir` feature を matrix に追加 (Linux のみ、macOS は後回し)

**Blocker / 判断点**:
- melior 0.x が stable か確認、もし不安定なら `llvm-sys` 直叩きに fallback
- LLVM 18 vs 19 の選択: LLVM 19 で MLIR linalg の breaking 有無を確認

### Stage 2 — 型システム全対応 (4-6 週)

**ゴール**: Almide の全 built-in 型を MLIR で表現、`almide test` 221 ファイルが `mlir` feature で通る。

- [ ] `String` / `Bytes` → MLIR `memref<?xi8>` + length prefix 慣習
- [ ] `List[T]` → MLIR `memref<?xT>` + length prefix
- [ ] `Record` → MLIR `llvm.struct` 型
- [ ] `Matrix` (既存 `AlmideMatrix` enum) → **AlmideMatrix dialect** 新設、`!almidematrix.tensor<?x?xf64>` 型
- [ ] `Result[T, E]` / `Option[T]` → tagged union 表現
- [ ] lowering pass: Almide IR の各 node を対応 MLIR op に変換
- [ ] `ConcretizeTypes` 相当の MLIR pass (型推論結果の MLIR propagate)
- [ ] stdlib runtime fn を extern 宣言 (Rust ABI で link)
- [ ] spec test 全 pass

**Blocker / 判断点**:
- AlmideMatrix dialect の設計: row-major / col-major / 汎用 strided?
- stdlib runtime の link 戦略: LTO で inline か、別 `.a` static link か

### Stage 3 — optimization passes (4-6 週)

**ゴール**: MLIR 経由で出した WASM が現行 `emit_wasm` と**同等以上の速度**。bonsai-almide bench で比較。

- [ ] shape specialization pass: Bonsai の hidden=2048 等を const propagation
- [ ] egg declarative rewrite を MLIR PDLL で記述、saturation 駆動
- [ ] auto-vectorize pass: MLIR `vector` dialect に lower して SIMD 出力
- [ ] LICM / DCE / inline 等 LLVM 標準 pass pipeline 構築
- [ ] bench: bonsai-almide の `bench_wasm_kv_stream` が MLIR 経由でも同等 tok/s
- [ ] bench: native も 1.5-2x の perf gain を期待 (LLVM vectorizer の威力)

**判断点**:
- ここで `emit_wasm` が劣るなら、emit_wasm 廃止判断。劣らないなら dual 運用継続

### Stage 4 — GPU target (6-8 週)

**ゴール**: bonsai-almide の matmul が WebGPU compute shader で実行、**browser 50+ tok/s** (reference parity)。

- [ ] MLIR `gpu` dialect lowering pipeline
- [ ] `@gpu` function attribute を Almide に追加、lower 時 GPU dialect に落とす
- [ ] SPIR-V emit (MLIR `spirv` dialect 経由)
- [ ] WGSL translator (SPIR-V-Cross or tint 利用、もしくは MLIR spirv → wgsl direct lowering)
- [ ] JS bridge: WebGPU.Device を bonsai-almide WASM が呼び出す API
- [ ] bonsai-almide: `@gpu fn linear_q1_0_row_no_bias(...)` で GPU dispatch
- [ ] bench: 50+ tok/s 到達、reference parity

**Blocker**:
- WGSL は Vulkan-flavored SPIR-V の subset なので直接 lowering 難、tint (Chrome の WGSL parser) 依存が現実解
- WebGPU spec の browser 対応状況確認 (Safari 2026 対応、Firefox 2025 末〜)

### Stage 5 — emit_wasm 廃止判断 (時期未定)

**ゴール**: MLIR path が Stage 3-4 で実用性を証明したら、`emit_wasm` を削除。並行実装取りこぼしバグを構造的に消す。

- [ ] MLIR wasm32-unknown-unknown output の質を bench で確認
- [ ] dev build の compile speed 比較 (MLIR の方が遅ければ debug 用途で残す判断)
- [ ] emit_wasm 削除 PR、`crates/almide-codegen/src/emit_wasm/` 消去
- [ ] 影響範囲: `--target wasm` option は MLIR path へ redirect

### Stage 6 — toolchain 配布 (別 arc)

**ゴール**: `almide target add webgpu` で binary swap + download が動く。

- [ ] `docs/roadmap/active/toolchain-dist.md` を別 arc として新規作成、詳細はそちら
- [ ] `almide-toolchain` crate: rustup 相当
- [ ] CI matrix で per-target prebuilt を GitHub Releases に publish
- [ ] `install.sh`、manifest 仕様、Sigstore 署名
- [ ] 最終的に `curl -fsSL https://almide.dev/install.sh | sh` で everything

## Risks / Open questions

### Risk 1: melior の stability

melior は LLVM 18-19 対応だが version skew がある。Stage 1 の spike で確認、ダメなら `llvm-sys` + `mlir-sys` 直叩き (安全性は犠牲)。

### Risk 2: LLVM upstream 追随負荷

LLVM は年 2 回 major release。Almide が pin 更新を怠ると「古い LLVM に取り残される」。**四半期ごとの bump PR 自動化**で対策。

### Risk 3: Compile time regression

MLIR を通すと compile が遅くなる (Rust の Cranelift 救済策の前例)。Stage 3 で計測、遅ければ dual backend 維持継続、debug は emit_wasm のまま。

### Risk 4: Binary size creep

50-80MB 目標から逸脱しないよう Stage 1 以降で CI に `binary size regression check` を追加。20% 以上の増加は PR block。

### Risk 5: AlmideMatrix dialect 設計ミス

Matrix 型は Almide の中心的 ML データ構造。dialect 設計で後戻り困難な選択がある。Stage 2 前に **独立した prototype** で 2 週間かけて dialect 仕様を固める。

### Open question 1: LLVM 18 LTS か 19 latest か

- 18 LTS: 長期サポート、bugfix 安心、MLIR API 枯れ
- 19 latest: 最新 vectorizer、新 dialect 導入、API 変動リスク

Stage 1 spike で両方試して決定。

### Open question 2: melior vs 手書き FFI

melior は safe Rust wrapper で生産性高いが、crate のバージョン追随に依存。LLVM 毎に breakage 起きた時に自分で patch する覚悟が要る。Stage 1 で評価。

### Open question 3: `emit_wasm` 廃止のタイミング

Stage 3 で MLIR 同等性確認 → 即廃止 vs 半年 dual 運用。後者が安全、前者が潔い。merge 時に判断。

### Open question 4: AlmideMatrix dialect vs `tensor` dialect

MLIR 標準 `tensor` dialect と自前 `AlmideMatrix` dialect を **同時運用する**か、あるいは `tensor` のみで済ませるか。bonsai-almide の `linear_q1_0_row_no_bias` みたいな Q1_0 packed 専用 op は独自型必須、標準 `tensor` では表せない。結論: **AlmideMatrix 必要**、ただし標準 `tensor` との相互変換 conversion pass を提供。

### Open question 5: GPU lowering 先行着手?

MLIR 勉強コスト高いので、先に Stage 4 (GPU) だけ spike して「本当に WGSL 自動生成できるか」確認してから Stage 1-3 に戻る方が健全かもしれない。Stage 1 終了時に判断。

## 参考資料

- [リサーチレポート](~/Downloads/llvm-embed-language-research-2026-04-22.md) — Rust / Zig / Mojo / Swift / Julia / Crystal / Nim / Odin / Pony / Vale の LLVM 統合パターン比較
- [bonsai-almide PERF_ROADMAP](https://github.com/almide/bonsai-almide/blob/main/docs/PERF_ROADMAP.md) — 高速化 5 軸、本 arc は軸 E
- `project_mlir_egg_arc.md` memory — 過去の MLIR + egg arc (Stage 1 終了)
- `project_bonsai_llama_parity.md` memory — Bonsai 性能追い込み、本 arc の最大ユーザケース
- [melior crate](https://github.com/raviqqe/melior) — Rust から MLIR を叩く safe wrapper
- [Mojo dialects](https://docs.modular.com/mojo/) — KGEN dialect の参考 (ただし非 open source の部分あり)
- [Zig self-hosted backend](https://ziglang.org/devlog/) — LLVM 依存削減の進捗
- [Rust Cranelift backend](https://github.com/rust-lang/rustc_codegen_cranelift) — dev 用 fast backend の先例
- [MLIR PDLL](https://mlir.llvm.org/docs/PDLL/) — declarative rewrite

## Sibling arcs

- `toolchain-dist` (未作成、本 arc Stage 6 から派生)
- `project_matrix_dtype_design.md` の P5 BLAS dispatch と合流予定 (Stage 2 の Matrix dialect 設計)
- `project_mlir_egg_arc.md` の Stage 1 成果 (AlmideExpr に matrix 13 variants + 7 egg rewrites) を Stage 3 PDLL 化のベースに

## 成功基準

1. **Stage 1 完了時**: `fn main = 42` が MLIR 経由で native binary になる、spec smoke test pass
2. **Stage 2 完了時**: `almide test` 全 221 ファイルが `mlir` feature で green
3. **Stage 3 完了時**: bonsai-almide bench が MLIR 経由で現行 `emit_wasm` 同等以上
4. **Stage 4 完了時**: bonsai-almide browser で 50+ tok/s、reference parity
5. **Stage 5 完了時**: `emit_wasm` 削除 or 明示的 opt-out 化、構造的負債消滅
6. **Stage 6 完了時**: `curl | sh` + `almide target add webgpu` で **non-developer が 5 分で demo 動かせる**

## 一言要約

Almide の codegen を MLIR dialect lowering に統一、LLVM を upstream tracking で embed、Zig 路線の 50-80MB binary で配布、WebGPU は MLIR gpu dialect → SPIR-V → WGSL で自動導出、既存 `emit_wasm` は段階的に廃止。「1 言語で CPU/WASM/GPU 全部出せる、世界最高 compiler」の看板完成。
