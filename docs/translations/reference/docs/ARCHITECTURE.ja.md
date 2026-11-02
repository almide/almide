<a id="architecture"></a>

# アーキテクチャ

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](ARCHITECTURE.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](ARCHITECTURE.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](ARCHITECTURE.zh-CN.md)

> [固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/ARCHITECTURE.md)の非規範的な読者用翻訳です。原文を正本とし、自動同期は行いません。




> 非規範の読者向け翻訳。原文コミット `35297a612fb29e3f78f6587aba253aa91d51f466` の固定スナップショットで、自動同期しない。原文と参照実装を正とし、コード・コメント・コマンドは保持する。性能値・所有権検査・証明に関する記述は原文の適用範囲と時点に限定され、翻訳時の独立実行・安全保証ではない。表 B の `Target` の説明と codegen 仕様に残る Wasm tombstone の表記差、行 37 の構造的経路セル欠落、図と本文の歴史的な経路記述は原文のまま保持する。

> 原文が interp を「実行可能仕様」と呼ぶ表現と、表 B 行 21 の Result/String・Try の要約も保持している。規範的な効力と文脈別の戻り値・伝播規則については、正本の [SPEC.md](../../../SPEC.md) と [STABILITY.md](../../../STABILITY.md) を参照すること。これらの要約を独立した仕様や無条件の String エラー・自動伝播の保証として解釈しない。

Almide は、コンパイラ、ランタイム、バックエンドの crate と独立した検証ツールを含む Rust ワークスペースである。ワークスペースの構成員と除外対象の正本は [`Cargo.toml`](../../../../Cargo.toml) であり、既定ビルドは `almide` と別のチェッカー `almide-verify` の両方を含む。Lean 4 証明プロジェクト `almide-perceus-belt` は Cargo crate ではない。主要依存: `serde` + `serde_json`（AST シリアライズ）、`toml`（テンプレート読み込み）、`clap`（CLI）、`lasso`（文字列インターン）、`wat` + `wasmparser` + `wasm-encoder`（WASM アセンブル／検証）、`lsp-server` + `lsp-types`（言語サーバー）。

<a id="pipeline"></a>

## パイプライン

```
  .almd source
       │
       ▼
  ┌─────────┐   ┌──────────┐   ┌─────────┐   ┌──────────────┐
  │  Lexer   │──▶│  Parser  │──▶│   AST   │──▶│ Type Checker │   almide-syntax / almide-frontend
  └─────────┘   └──────────┘   └─────────┘   └──────┬───────┘
                                                     │  expr_types + env
                                                     ▼
                               ┌───────────────────────────────┐
                               │   Lowering (AST → Typed IR)   │   almide-frontend
                               └──────────────┬────────────────┘
                                              ▼
                               ┌───────────────────────────────┐
                               │ optimize → verify → mono →    │   almide-optimize
                               │ ir_link                       │
                               └──────────────┬────────────────┘
                          ┌───────────────────┼──────────────────────┐
                          ▼                   ▼                      ▼
                 Rust target          WASM target             WGSL target
                 codegen v3           structural leg          emit_wgsl
                 (nanopass + TOML     (wasm_leg →             (almide-codegen)
                 templates + walker)  almide-wasm, direct
                                      emit; wall = E082)
                          │                   │
                          ▼                   ▼
                 native trust-spine      .wasm (optional
                 (v1 Rust render,        --wasm-opt with
                 v0 fallback on wall)    parity gate)
                          │
                          ▼
                 rustc/cargo → binary
```

`almide-interp` はバックエンドと並ぶ第 3 の実行器であり、リンク済み IR を木走査するインタープリターである（`lower → optimize → mono → ir_link` の後、ターゲット固有の lowering パスの前）。どちらのバックエンドともコード生成パスを共有せず、ターゲット間のオラクル／実行可能仕様として機能する。

<a id="targets"></a>

### ターゲット

- **native（既定）** — codegen v3 が Rust ソースを出力する。lowering 可能な範囲は v1 native trust-spine レンダラー（`almide-mir`）が置き換え、wall では v0 codegen ソースへフォールバックする。`rustc`/`cargo` がバイナリを生成する。
- **`--target wasm`** — 経路は構造的経路の 1 つだけ（`src/cli/build.rs::render_wasm_module_routed`）。低コストの PROJECT-SHAPE 検査を先に行い、構造的 wall は後述の強制的な E082 エラーになる。ルーティングは 1 つのライブラリ関数 `almide::wasm_route::route_wasm`（src/wasm_route.rs、#2554）に集約される。CLI は環境から probe スイッチを `RouteOptions` に読み込み、`RouteError` を描画する。wasm32 利用側（playground）は解析済みのタブ（`ModuleSource::Provided`）を渡して `render_wasm_routed` を呼び、同じ経路を得る（`tests/wasm_route_playground_test.rs` がこの形式を試験する）。
  - **正式採用された構造的経路**（既定）: `almide::wasm_leg`（parse→check→lower→self-host link→`link_ir`）が `almide-wasm::emit_program` に入力し、Wasm バイト列を構造的に出力する（wasm-encoder を使い、WAT テキストは使わない）。差分ゲートは版管理された wasm_cross コーパスで観測可能な出力を native と比較する。`almide run` は組み込み `almide-wasm-run` ホスト上で、その対応操作範囲内を実行する。`almide build` は `to_wasi` を通じてコアモジュールを、またはフラグとホスト操作に応じてコンポーネントをパッケージ化する（`src/cli/build_wasm.rs::WasmPackage`）。serve 形の HTTP プログラムは WASI 0.3 handler コンポーネントになる。標準ランタイムは成果物のインターフェースに対応している必要があり、プロセス操作には Almide のプライベートホスト capability が必要である。標準ランタイムの parity ゲートは、宣言された fixture 部分集合を対象とする。`almide-wasm-vm` が受理する閉じたコアモジュール部分集合は、認定範囲を限定したそのインタープリター（#865）で実行できる。これはコンポーネントや任意ホストのランナーではない。読み込み時の検証器は未対応の形を拒否する。`tests/wasm_vm_parity_test.rs` は受理コーパスを標準ランタイムと比較し、Critical プロファイルの fixture を native と比較する。
  - **従来の WAT trust-spine**（`almide-mir` による WAT 描画）は存在しない。#2752 が最後の経路を除去し、#2761 がレンダラーを削除した。`almide-mir` は MIR lowering（native 経路）、native レンダラー、MIR 証明書の witness 生成器を保持する（proofs/gate.sh、corpus-wall.sh。#4011 以降 `almide verify` は構造的経路の bundle を判定する）。
  ツリーが読むすべての `ALMIDE_*` スイッチ（ゲートの迂回、アブレーション、トレースチャネル）は 1 つのレジストリ `almide_base::env::SWITCHES` に属する（`almide switches` で一覧表示し、`docs/specs/cli.md` に表を埋め込む。#2205）。コンパイラ本体は `almide_base::env::{flag, var}` だけを通して読む。真偽値の意味論は 1 つであり、環境から引き継いだ強制経路やゲート迂回は stderr に 1 回通知され、その状態で出した判定であると分かる。
  構造的 WALL は診断付きの強制エラー `error[E082]` であり、`wall: <reason>` 行と `--> in <fn>` の場所行を出す。決してフォールバックしない（#782 の問題は未検証 v0 codegen に落ちたことだった。`ALMIDE_VERIFIED_DEBUG=1` は wall 名を示す）。`--wasm-opt` は明示的に有効化するもので、検証済みモジュールとの差分 parity ゲートで保護される（`tests/wasm_runtime_opt_parity.rs::wasm_opt_parity_spec`）。
- **`--target wasm32` / `wasi`** — 生成 Rust ソースを直接 `rustc --target wasm32-wasip1` でコンパイルする（SIMD128 有効）。`--target wasm` とは別物である。
- **`--target wgsl`** — almide-codegen の `emit_wgsl` による GPU 計算シェーダー。

`--verified` が既定である（`--no-verified` は非推奨で警告する）。`almide run --target wasm` と `almide build --target wasm` は `compile_to_wasm_bytes` を共有するため、run と build は同じ出力処理を使う。構造的経路では BUILD 成果物が選択した core/component 経路向けにパッケージ化されるため、組み込みホストのモジュールと出荷成果物のバイト列は同一でなくてもよい。

<a id="crate-structure"></a>

## crate 構成

以下は主要コンポーネントを示す案内図であり、完全なワークスペース一覧ではない。全構成員は [`Cargo.toml`](../../../../Cargo.toml) を参照。

```
almide/                    Workspace root
├── src/                   CLI binary (almide)
│   ├── main.rs            Subcommands: init, run, build, test, check, lsp,
│   │                      explain, fmt, compile, clean, add, deps, dep-path,
│   │                      install, self-update, ide, fix, docs-gen
│   │                      (no subcommand → REPL)
│   ├── compile_driver.rs  Shared front half: parse → check → lower → optimize
│   ├── resolve.rs         Module resolution (filesystem + git deps)
│   ├── project.rs         almide.toml parsing, PkgId
│   ├── project_fetch.rs   Git dependency fetching
│   └── cli/               Subcommand implementations; lsp.rs is the LSP server
│                          (lsp-server over stdio); repl.rs; ide.rs = semantic
│                          queries for agents (outline, doc, stdlib-snapshot)
│
├── crates/
│   ├── almide-base/       Foundation: Sym interning (lasso), Span, Diagnostic
│   ├── almide-syntax/     Lexer, recursive-descent parser, untyped AST
│   ├── almide-types/      Resolved Ty, unification, protocol defs,
│   │                      stdlib_info.rs (module registry, auto-import lists,
│   │                      bundled stdlib sources via generated/embedded.rs)
│   ├── almide-lang/       Facade crate: re-exports syntax + types (no logic)
│   ├── almide-frontend/   Type checker (Infer → Solve → Resolve), AST→IR
│   │                      lowering, canonicalize, ir_link
│   ├── almide-driver/     Shared post-typecheck driver: optimize → mono → ir_link
│   ├── almide-ir/         Typed IR: IrProgram/IrExpr (every node carries ty),
│   │                      VarId/VarTable, visitors
│   ├── almide-mir/        v1 Middle IR — ownership/layout for its native
│   │                      renderer (Perceus). #![forbid(unsafe_code)].
│   │                      MIR lowering + the native trust-spine renderer
│   │                      + MIR certificate witnesses (proofs/gate.sh).
│   │                      No wasm/WAT renderer since #2761.
│   ├── almide-optimize/   Monomorphization, DCE, constant propagation,
│   │                      stream fusion
│   ├── almide-codegen/    Codegen v3 for Rust (+ WGSL): nanopass pipeline,
│   │                      TOML template renderer, target-agnostic walker;
│   │                      templates/rust.toml = Rust syntax templates (~330 rules)
│   ├── almide-wasm/       Structural wasm emitter and emission witnesses
│   ├── almide-wasm-run/   Embedded host for emitted wasm modules
│   ├── almide-wasi/       Transform emitted modules into stock-WASI artifacts
│   ├── almide-verify/     Independent certificate checker, shipped beside almide
│   ├── almide-anchor-check/  Check located witness events against shipped wasm bytes
│   ├── almide-interp/     Pre-codegen IR interpreter — 3rd cross-target oracle
│   ├── almide-tools/      Formatter, module interface (almide compile), ALMDI
│   ├── almide-dialect/    Pure-Rust MLIR dialect schema (no FFI)
│   ├── almide-egg-lab/    Equality-saturation engine used by EggSaturationPass
│   ├── almide-kernel/     [cargo-excluded] target-specific SIMD numeric kernels
│   └── almide-perceus-belt/  [Lean 4, not a cargo crate] Perceus RC/ownership
│                             proof belt
│
├── grammar/               Git submodule → almide/almide-grammar: descriptive
│                          keyword/precedence data consumed by the tree-sitter
│                          and TextMate generators (not by the compiler build)
├── stdlib/                Self-hosted .almd modules (see below)
└── runtime/rs/src/        Native Rust runtime for @intrinsic functions
```

<a id="codegen-v3-rust-target-three-layer-architecture"></a>

## Codegen v3（Rust ターゲット）: 3 層アーキテクチャ

すべての意味論上の判断はテキストを出力する前に IR 上で行う。walker が見るのは型付き IR ノードだけであり、どのターゲット向けに描画しているかを検査することはない。

1. **Nanopass パイプライン** — 各 `pass_*.rs` は `&mut IrProgram` を受け取り、1 つの意味論的書き換え（StdlibLowering、ResultPropagation、CloneInsertion、BuiltinLowering、FanLowering など）を行う。順序は単に記述するのではなく宣言する。各パスは自身の前に必要なパス（`depends_on`）と、自身より後に必要なパス（`run_before`）を名指しし、3 つの表現境界が障壁となる。`scripts/check-pass-shuffle.sh` は宣言が許す seed 付きランダム順序で実行し、出力 Rust をバイト差分比較することで宣言がすべてを表すことを確かめる（#2186）。
2. **テンプレートレンダラー** — TOML ファイルが構文パターンを定義し、walker が `templates.render_with("if_expr", ...)` を呼ぶ。すべての文字列描画はここで行う。
3. **Walker** — ターゲット非依存の IR 木レンダラー。`if target == Rust` 検査はゼロで、独自の所有権判断も行わない（#2186）。ある箇所で値を move、clone、borrow、own のどれにするかはパスの判定であり、IR ノード（`Borrow`、`Clone`、`Deref`、所有するメソッド呼び出し）または `CodegenAnnotations` エントリー（`var_storage`、`shared_mut_vars`、`borrowed_loop_vars`、`param_borrows` など）として walker に渡す。walker は格納クラスごとに表記を選び、各ノードの種類だけから描画する。解析走査を行わず、関数ごとの所有権集合を持たず、今描画したテキストから判断もしない。`scripts/check-walker-reads-annotations.sh` がこの境界を保つ。所有権判定自体は fixture だけでなく性質によっても支える。`tests/native_borrow_oracle_test.rs` は借用可能な型ごとに、本体が仮引数に行えるすべての使い方と各呼び出し箇所の形の組み合わせを生成し、プログラムが検査に通り native ビルドでき、wasm 経路と同じ出力をすることを検査する。また `ALMIDE_ALLOC_COUNT=1`（native ビルドが注入する計数アロケーター）では、プログラムごとに `tests/golden/native-borrow-oracle-alloc.txt` へ固定した (allocs, deallocs) の組と正確に一致することを検査する。借用で足りる箇所の clone は stdout もビルドも変えず、この回数だけを変える（#2228）。wasm 経路にも対応物がある。`ALMIDE_WASM_ALLOC_COUNT=1` は構造的エミッターに `$alloc` / `$free` を 4 つの export グローバルで計数させる（無効時はゼロではなく存在しない）。`crates/almide-wasm/tests/alloc_ledger.rs` はコーパス fixture ごとの `allocs reused bytes frees` を `__heap` の最高水位とともに固定するため、最高水位からは見えない churn にもラチェットを掛ける（#2407、[docs/wasm/WASM-OUTPUT.md](../../../wasm/WASM-OUTPUT.md)）。
   判定自体はすべての DEBUG native ビルド、および要求した release ビルドで認証する（`ALMIDE_CERTIFY_OWNERSHIP=report|fail|off`、`crates/almide-codegen/src/certify_ownership.rs`、#2231）。最終 IR を再走査し、各仮引数の借用モードと各 `Clone` を実際の出現箇所の働きと照合する。借用仮引数の消費、所有値の最終使用での clone、所有を必要とする場所のない所有仮引数を検査する。コーパス台帳（`proofs/ownership-certifier-baseline.txt`、`scripts/check-ownership-certifier.sh`）は縮小だけを許し、空である。違反は欠陥であり、debug ビルドはその出力を拒否する。

<a id="wasm-backend-structural-leg"></a>

## WASM バックエンド（構造的経路）

wasm バックエンドは構造的経路（`src/wasm_leg.rs` → `almide-wasm::emit_program`）である。以前 `almide-mir` が提供していた v1 MIR→WAT パイプラインは #2761 で削除された。

- 所有権／RC は Perceus に従う（線形メモリ内の Perceus 型 RC）。Coq の定理 WasmEncode/WasmExec/WasmRcDec が対象とするのはモデル化されたランタイム（廃止された従来実装のもの）である。構造的ランタイムのバイト列は `proofs/check-structural-bytes.sh` により `StructuralDecode.v` と照合する。構造的経路はビルドごとの witness を記録し、`almide verify` は独立した `almide-verify` にその検査を委ねる。対応するコア成果物では（コンポーネントは `almide verify` が拒否）、指定された成果物または既定パスで見つかった成果物を SHA-256 で bundle に結び付ける（#4011）。認証対象コーパスは `proofs/structural-wall.sh` で試験する。これは意味論的正しさや成果物全体のメモリ安全性の証明ではない。対応範囲と記録に関する仮定が残る。[証明済みと信頼対象](../../../contracts/proven-vs-trusted.md) および [メモリ安全性のギャップ表](../../../design/MEMORY-SAFETY.md#the-gap-table-2026-10-24-4011) を参照。
- stdlib 呼び出しは *self-host* される。`stdlib/*.almd` の実装をユーザーコードと一緒にコンパイルする。新しい self-host 本体は `@implements("module.fn")` を宣言し、ビルドはその宣言から `crates/almide-types/src/generated/implements.rs` を生成する。`self_host_registry.rs` は宣言エントリーと縮小専用のレガシーテーブルを組み合わせる（`tests/name_keyed_sites_test.rs`）。新しい本体のために拡張すべきテーブルではない。未リンクの stdlib 呼び出しは wall（強制エラー）となる。
- `wasmparser::validate` がテストハーネスを保護する。`almide test --target wasm` は `build`/`run --target wasm` と同じ経路で描画する（#2179）。構造的経路（`almide-wasm`。共通の `__test_runner` 合成と `almide_driver::test_runner` による構造的なテスト内 assert lowering を含む）を使い、wall となったファイルは WALL と報告し、その後既定の `almide test` 経路が native で実行する。

<a id="optimization-pass-roster-per-target"></a>

## ターゲットごとの最適化パス一覧

すべての最適化／書き換えパスについて、実行箇所とターゲットを示す（#929）。ゲート `scripts/check-pass-roster.sh` は `almide-codegen` の `pass_*.rs` ファイルと `NanoPass::name()` 文字列、および `almide-optimize` のパスモジュールと `ALMIDE_ONLY_PASS` 軸名を列挙し、この節に欠けるものがあれば失敗する。したがって一覧はコードから乖離できない。乖離の計測コストは [docs/benchmarks/wasm-runtime.txt](../../../benchmarks/wasm-runtime.txt) の各プログラムの wasm/native 実行時間比であり、`scripts/check-wasm-runtime-ratio.sh` がラチェットで管理する。

各経路で何を実行するか:

| 経路 | 選択条件 | 分岐前（共通） | ターゲット側パス |
|---|---|---|---|
| native、v1 MIR 描画 | `almide run`/`build` の既定（`--verified`）で、`almide-mir` が lowering 可能な範囲 | `link_ir` | almide-mir native 段階（表 D の native 行） |
| native、codegen v3 | `almide test`（native）、v1 native wall のフォールバック、`--target rust` 出力、`--target wasm32`/`wasi` | `link_ir` | almide-codegen nanopass パイプラインの Rust 分岐（表 B） |
| `--target wasm`、構造的（唯一の wasm 経路） | `src/wasm_leg.rs` → `almide-wasm::emit_program` | `link_ir` | almide-wasm エミッターの書き換え（表 C） |
| `--target wgsl` | `almide-codegen::emit_wgsl` | `link_ir` | almide-codegen nanopass パイプラインの Wgsl 分岐（表 B、4 パス） |
| interp（オラクル） | `almide-interp` | `link_ir` | 設計上なし |

<a id="a-shared-pre-split--almide-driverlink_ir-cratesalmide-optimize"></a>

### A. 共通・分岐前 — `almide-driver::link_ir`（`crates/almide-optimize`）

すべての経路と interp オラクルが使う唯一の段階順序（`tests/one_driver_test.rs` は別表記があれば失敗する）。`optimize_half` = 行 0–7、`link_half` = 行 8–9。`ALMIDE_DISABLE_OPT=1` は 4 つの性能行をスキップする（性能ラチェットのアブレーション経路）。`ALMIDE_ONLY_PASS=sink|fold|dce|propagate` はそのうちちょうど 1 つを実行する（`spec/pass_isolated/` を対象とする `scripts/check-pass-isolated.sh`。CI の `checks` ジョブが実行）。

| # | パス | モジュール | 種類 | 処理 |
|---|---|---|---|---|
| 0 | `sink` | `optimize/sink_inline.rs` | 最適化（性能） | 本体全体が、唯一の仮引数を第 1 引数とする 1 つの `list.*` 呼び出しである root 関数（読み取り 1 回、他の自由変数なし）を、引数が `list.*` 呼び出しである呼び出しへインライン化し、束縛変数を新鮮な名前へ変える。各経路固有の融合が 1 本のチェーンを見るようにする（#3584、#3590）。採用しない経路は呼び出しそのものとして評価する |
| 1 | `fold` | `optimize/mod.rs` (`constant_fold`) | 最適化（性能） | リテラル算術を下から上へ wrapping で評価し、それ以前の不変スカラー top-let を代入する（#809） |
| 2 | `dce` | `optimize/dce.rs` | 最適化（性能） | 値が純粋な未使用束縛を除去し、伝播後にも再実行する |
| 3 | `propagate` | `optimize/propagate.rs` | 最適化（性能） | リテラルに束縛された変数をそのリテラルで置き換える |
| 4 | 符号なし再畳み込み | `optimize/mod.rs` (`refold_unsigned_lane`) | 正しさ | 伝播で露出した `UInt64` の `/`/`%` リテラル対を符号なしで再畳み込みする（#872）。アブレーションでは決して除外しない |
| 5 | optional-chain の脱糖 | `optimize/optional_chain.rs` | lowering の基盤 | `p?.f` → 両バックエンドが証明する合成末尾ヘルパーへの呼び出し。アブレーションでは決して除外しない |
| 6 | 分岐の持ち上げ | `optimize/branch_lift.rs` | lowering の基盤 | ヒープ型の `let` 束縛 `if`/`match` → 末尾ヘルパー関数（trust-spine レンダラーが lowering する形）。アブレーションでは決して除外しない |
| 7 | top-let 再分類 | `almide_ir::reclassify_top_lets` | 解析 | 畳み込み後の top-let の const／実行時分類 |
| 8 | monomorphize | `mono/` (`monomorphize`) | 特殊化 | 構造的境界付きジェネリック関数を呼び出し箇所の型ごとに複製する。最後に `mutual_tco`（`mutual_tco.rs`: 相互再帰の末尾呼び出し → 1 ループ）を実行する |
| 9 | ir_link | `almide-frontend::ir_link` | リンク | 単相化した呼び出しグラフができた後、依存モジュールを root へリンクする |

<a id="b-almide-codegen-nanopass-pipeline-native-codegen-v3--wgsl"></a>

### B. `almide-codegen` nanopass パイプライン（native codegen v3 / WGSL）

実行順序は `crates/almide-codegen/src/target.rs::build_pipeline` が定める。パスの `Target` 列挙型は `{Rust, Wgsl}` であり、**wasm 経路はこのパイプラインに決して入らない**。Targets 列の「all」は `targets()` が宣言する Rust + Wgsl を意味し、分岐前に実行する資格があることを示すだけで、そうすべき根拠ではない。Wgsl 分岐は W と印した 4 行である。分類:

- **設計上 Rust 用** — Rust の所有権、借用、`Result`、マクロ、`LazyLock` のために存在し、wasm への移植に意味はない。
- **基盤** — TOML walker が必要とする書き換え／解析。構造的経路には独自の対応物（名前を記載）があり、移植不要。
- **最適化** — 性能のための書き換え。「構造的経路」列が対応物の有無を示し、分岐前へ移す判断は E に続く。

| # | パス（`name()`） | ファイル | ターゲット | 分類 | 処理 | 構造的 wasm 経路 |
|---|---|---|---|---|---|---|
| 1 | `UnifyVarTables` | `pass_unify_var_tables.rs` | すべて、W | 基盤 | すべての `IrModule.var_table` をプログラムのテーブルへ統合 | モジュール別テーブルを読む（`build_globals`） |
| 2 | `ListPatternLowering` | `pass_list_pattern.rs`, `pass_list_pattern_nested.rs` | すべて | 基盤 | リストパターン → 長さ検査 + 添字アクセス。コンストラクター／option／レコード位置の下にあるリストはその場で書き換える（#3413） | `IrPattern::List` を直接 lowering（`patterns.rs`） |
| 3 | `LambdaTypeResolve` | `pass_lambda_type_resolve.rs`, `pass_lambda_type_lookup.rs` | すべて、W | 基盤 | stdlib 呼び出し先シグネチャからクロージャー仮引数型を導く（トップダウン） | `TypeTable` （`types_table.rs`） |
| 4 | `ConcretizeTypes` | `pass_concretize_types.rs`, `pass_concretize_types_call_ret.rs`, `pass_concretize_types_signatures.rs`, `pass_concretize_types_unresolved.rs`, `pass_concretize_types_walker.rs` | すべて、W | 基盤 | 各 `IrExpr.ty` を正本となる具体型と同期 | `TypeTable` |
| 4b | `MeteredRegionDelegate` | `pass_metered_region_delegate.rs` | Rust | 設計上 Rust 用 | 計測対象リージョン関数（`fan.bounded` / `fan.min` / `fan.timeout`）を、プログラム全体が v1 で wall になる場合に CLI がリンクする v1 描画リージョンモジュールへの委譲へ変える（#3669）。メーターは MIR にあり、この codegen 自体にはない | 該当なし（構造的経路は自分のリージョンを自分で計測） |
| 4c | `MeterCharge` | `pass_meter_charge.rs` | Rust | 設計上 Rust 用 | CLI がこの codegen でプログラムを計測する場合（v1 部分集合外のリージョン、#3673）、構造的経路が消費量を計上する IR 箇所に決定的な消費量を課す。共有 `almide_ir::meter_plan` が免除しないエントリー、ループ先頭、クロージャーホップ、文字列連結に、それぞれの cut とともに適用する。`MeteredRegionDelegate` と `LICM` は退く | 同じ計画（`fuel.rs` が `almide_ir::meter_plan` を読む） |
| 5 | `PatternLiteralGuard` | `pass_pattern_literal_guard.rs` | Rust | 設計上 Rust 用 | ペイロード内にネストした文字列リテラルをガードへ持ち上げる（対象は `as_deref` 形式） | 該当なし |
| 6 | `ResolveCalls` | `pass_resolve_calls.rs` | すべて | 基盤 | すべての `CallTarget::Module` を検証し、既知の関数へ書き換える | `FnTable` 解決（`emit.rs`） |
| 7 | `RegionWindow` | `pass_region_window.rs`, `pass_region_window_clone.rs` | Rust | 最適化 | `consume(produce(scalars))` 箇所を、前置部分のスレッドローカル bump arena 内の `__rgn_` 対応関数と `Copy` 対応 enum で実行する（#1991）。v1 は root モジュール関数と、scalar／region-enum のタプルペイロードを持つ root variant enum を受理する | 対応物: `region.rs`（アロケーター上の `RegionSave`/`RegionRestore`、#1961） |
| 8 | `BoxDeref` | `pass_box_deref.rs` | Rust | 設計上 Rust 用 | `Box` フィールドから束縛されたパターン変数に `*deref` | 該当なし |
| 9 | `LICM` | `pass_licm.rs`, `pass_licm_hoist.rs`, `pass_licm_purity.rs` | すべて | 最適化 | ループ不変の純粋式をループ前の `let` へ持ち上げる | **対応物なし** — E 参照 |
| 10 | `EggSaturation` | `pass_egg_saturation.rs` | すべて | 最適化 | 行列チェーンを等式飽和で融合（`almide-egg-lab`、規則は stdlib `@rewrite`）。リスト分岐は #2045 以降無効（ラムダ置換がコールバックの副作用を重複させたため） | **対応物なし** — E 参照 |
| 11 | `MatrixShapeSpec` | `pass_matrix_shape_spec.rs` | Rust | 設計上 Rust 用 | 小形状の行列積 → 完全展開 `InlineRust` | 該当なし（手書きカーネル、`matrix_kernels.rs`） |
| 12 | `ConstFold` | `pass_const_fold.rs` | すべて | 最適化 | 行 10–11 が残したリテラル算術を畳み込む | A.1（`fold`）で代替済み — E 参照 |
| 13 | `IntrinsicLowering` | `pass_intrinsic_lowering.rs` | すべて | 基盤 | `@intrinsic` stdlib 呼び出し → `RuntimeCall { symbol }` | self-host レジストリのリンク（`src/wasm_leg.rs`）。intrinsic は wall |
| 13b | `StreamFusion` | `pass_stream_fusion.rs` | Rust | 最適化 | ラムダリテラル付きの `RuntimeCall { almide_rt_list_* }` → `IterChain`。`BorrowInsertion` より前に行い、チェーンのラムダは borrow／capture-clone／clone パスからクロージャーに見えないスコープになるため、読むものを借用し `move` なしで描画する。反復元は消費から始まり、`BorrowInsertion` は対応物の `@consume` 引数位置ではなく各要素への操作からモードを決める（#2287: `element_reads_only` — `Copy` 要素、または受け取る全ラムダが読むだけのヒープ要素なら反復元は `&[T]` のままで、clone パスが `.iter()` から `&T` を束縛する）。`\|>` チェーンを 1 つの iterator 式へ平坦化するのは全コールバックが純粋でインターリーブが観測不能な場合だけ（#2045）。`ALMIDE_STREAM_FUSION_OFF` でアブレーション | **対応物なし** — 仕様順序は段階ごと。E 参照 |
| 14 | `BorrowInsertion` | `pass_borrow_inference.rs`, `pass_borrow_inference_call_sites.rs`, `pass_borrow_inference_ownership.rs`, `pass_borrow_inference_scoped_closures.rs`, `pass_borrow_inference_seq_hoist.rs` （`pass.rs` 内のラッパー） | Rust | 設計上 Rust 用 | Roc 型の借用を既定とするシグネチャ、呼び出し箇所の `Borrow` ノード。反復元は本体が要素に何をするかに従う（#2287）。関数型仮引数は呼び出し可能値が逃れなければ `&dyn Fn`、その位置のラムダリテラルは `&(lambda)` と表すスコープとなる（#2288）。ネストした `mut` 引数呼び出しより前に読むオペランド（補間部分、ネスト呼び出しの引数）は左から右へ持ち上げる（#3230） | RC-3 借用／fresh 分類器（`rc_ownership.rs`） |
| 14b | `OwnedSourceHoist` | `pass_owned_source_hoist.rs` | Rust | 最適化 | 後続引数が反復元をスカラーとしてのみ読む範囲操作（`list.slice(xs, 1, list.len(xs))`）は、それらを先に束縛し、TailCallOpt / CloneInsertion が使用終了した元を `_owned` 対応関数へ move できるようにする（#3519） | 該当なし |
| 15 | `TailCallOpt` | `pass_tco.rs`, `pass_tco_loop_rewrite.rs`, `pass_tco_owned_reads.rs` | すべて | 最適化 | 自己再帰の末尾呼び出し → ループ | 対応物: `tco.rs`（エンコード済み本体の `loop_convert`、それ以外は `return_call`） |
| 16 | `CaptureClone` | `pass_capture_clone.rs`, `pass_capture_clone_bindings.rs`, `pass_capture_clone_mut_param.rs` | Rust | 設計上 Rust 用 | `move` クロージャーが捕捉する変数を事前クローン | 該当なし |
| 17 | `CloneInsertion` | `pass_clone.rs`, `pass_clone_calls.rs`, `pass_clone_interp.rs`, `pass_clone_compare.rs`, `pass_clone_places.rs`, `pass_clone_projection.rs`, `pass_clone_head.rs`, `pass_clone_select.rs`, `pass_clone_record_fields.rs`, `pass_clone_loops.rs` | Rust | 設計上 Rust 用 | ヒープ型の再利用、および証明された射影の借用／move に `Clone` ノード（リスト先頭読み取りは借用、#3453。生存するローカルの読み取り専用選択は借用、#3532） | RC inc/share ガード（`rc_ownership.rs`） |
| 17b | `ChainSourceBorrow` | `pass_chain_source_borrow.rs` | Rust | 最適化 | clone パスがクローンする必要があったチェーン元（`(v.clone()).into_iter()`、`&Table` の `t.sizes`、借用元の下にある生存ローカル）、または消費されるものとしてそのまま使う `&[T]` 仮引数は、コールバックがその変数を書き換えない限り借用から反復する（`.iter().cloned()`、#2098） | 該当なし |
| 18 | `MatchSubject` | `pass_match_subject.rs` | Rust | 設計上 Rust 用 | match 対象に `.as_str()` / `.as_deref()` | 該当なし |
| 19 | `EffectInference` | `pass_effect_inference.rs` | すべて | 解析 | 推移的な stdlib 使用から capability カテゴリを推論 | 別経路で共有: `cli::check_permissions` が各経路の mono 前 IR にこのパスを単独で実行 |
| 20 | `StdlibLowering` | `pass_stdlib_lowering.rs`, `pass_stdlib_lowering_ufcs.rs` | Rust | 設計上 Rust 用 | `Module` 呼び出し → 引数装飾付きの `Named` ランタイム呼び出し | self-host stdlib 本体を wasm 関数として出力 |
| 21 | `ResultPropagation` | `pass_result_propagation.rs` | すべて | 設計上 Rust 用 | effect 関数 `T → Result[T, String]`、呼び出し箇所に `Try` | 独自の effect lowering（`effect_raw`、`emit.rs`） |
| 22 | `BuiltinLowering` | `pass_builtin_lowering.rs` | Rust | 設計上 Rust 用 | `assert_eq`/`println`/… → `RustMacro` | 該当なし |
| 23 | `DecodeSlotHint` | `pass_decode_slot_hint.rs` | Rust | 設計上 Rust 用 | 導出された `T.decode` の借用フィールド検索に宣言インデックス（`@codec_slots`）を持たせ、`almide_rt_value_field_ref_at` として描画する（#1679）。ランタイムは走査前に `pairs[i]` を試す。エラーは不変（C-084） | 該当なし（wasm 経路は属性も `_at` シンボルも読まない） |
| 23b | `DecodeErrFrame` | `pass_decode_err_frame.rs` | Rust | 設計上 Rust 用 | 導出された `T.decode` のフィールドごとのエラーフレーム（`T___erratw_*`、#1675）を、そのフィールド自身の結果に対する `.map_err(..)` とし、成功経路をペイロードの clone もキー割り当てもない単純な `?` にする（#2050） | 該当なし（wasm 経路は worker 自体を lowering） |
| 24 | `Peephole` | `pass_peephole.rs` | すべて | 最適化 | 典型的なリストループ → `ListSwap`/`ListReverse`/`ListRotateLeft`/`ListCopySlice` ノード | **これらのノードの lowering なし** — E 参照 |
| 25 | `RustLowering` | `pass_rust_lowering.rs`, `pass_rust_lowering_stmts.rs`, `pass_rust_lowering_fan.rs`, `pass_fan_local_state.rs` | Rust | 設計上 Rust 用 | push 最適化、借用 index の持ち上げ。`fan.map` と `fan { }` 内のリスト操作で、Send 安全なスカラー上の純粋ラムダをコアごとのスレッドランタイム対応物へ振り分ける（`almide_rt_fan_map_par`、`almide_rt_list_par_*`。`ALMIDE_FAN_SEQUENTIAL=1` でアブレーション、#2044）。クロージャーがリテラルであるコールバック操作（`map.upsert`）→ static-closure の `_fn` 対応物とし、呼び出しごとの boxing をなくす | 該当なし |
| 26 | `FanLowering` | `pass_fan_lowering.rs` （`pass.rs` 内のラッパー） | すべて、W | 基盤 | fan spawn クロージャーから auto-try を除去 | 独自の fan lowering（`fan.rs`） |
| 27 | `NormalizeRuntimeCalls` | `pass_normalize_runtime_calls.rs` | Rust | 設計上 Rust 用 | 旧 `Named { almide_rt_* }` → `RuntimeCall` | 該当なし |
| 28 | `IrLinkFlatten` | `pass_ir_link_flatten.rs` | Rust | 設計上 Rust 用 | walker 用にモジュールを root へ平坦化 | モジュールと修飾名を保持 |
| 29 | `SharedCellBorrow` | `pass_shared_cell_borrow.rs` | Rust | 設計上 Rust 用 | 文により安全と証明された読み取りで、捕捉したセルをその場で借用する（#1143） | 該当なし |
| 30 | `VarStorage` | `pass_var_storage.rs` | Rust | 設計上 Rust 用 | クロージャーが捕捉する非 Copy の `var` ローカルを判定し、`AlmideRcCow` 内に置く（`var_storage`）。以前は walker 自身のプログラム準備走査だった（#2186） | RC-3 借用／fresh 分類器（`rc_ownership.rs`） |
| 31 | `RangeCountingVars` | `pass_range_counting.rs` | Rust | 最適化 | `for-in` の先頭としてだけ読む `let` 束縛 range は実体化 `Vec<i64>` でなく裸の `Range<i64>` のままとする（#1857）。MIR の #1400 `range_counting_vars` 受理規則に対応し、集合が最終 IR の名前を指すよう最後に実行する | `ranges.rs` のカウントループ（#1400）— 実装済み |
| 32 | `TopLetStorage` | `pass_top_let_storage.rs` | すべて | 解析 | walker 用の統一 top-let 格納属性（§4 Stage 1） | 独自のグローバル計画（`build_globals`） |
| 33 | `BorrowLowering` | `pass_borrow_lowering.rs`, `pass_borrow_lowering_take.rs` | Rust | 設計上 Rust 用 | 各 `Borrow` / `Clone` / 参照渡し仮引数の格納読み取りに対する最終判断。`&T` 仮引数がすでに持つ `&` を除去し、所有する読み取りには `.to_string()` / `.to_vec()` / `.clone()`、spread 元には `.clone()`、リスト内部への借用（`almide_index_ref!`）、`_ref` フィールド検索対応物、`&String` ループ束縛の `.as_str()`、等値比較の `&T` 仮引数の逆参照を適用し、`param_borrows` を公開する。walker は見たものを描画する（#2186）。代入で自身の古い値から上書きされる `mut` 仮引数は、右辺の何も空所を観測できない場合に `std::mem::take` で move して取り出す（#3170） | 該当なし（wasm 経路には参照がない） |
| 34 | `SliceBinders` | `pass_slice_binders.rs` | Rust | 最適化 | `split_once` / `strip_prefix` / `strip_suffix` に対する `match` で、ペイロード束縛が `&str` として読むか消費するだけなら、対象のスライスとして束縛する（ランタイムの `_ref` 対応物）。消費する束縛は消費箇所でコピーし、それ以外の使用があれば所有する対応物を維持する | 該当なし（wasm 経路には参照がない） |
| 35 | `StrMapKey` | `pass_str_map_key.rs` | Rust | 最適化 | キーが `k.to_string()` である String キーの `map.set` / `map.upsert` はランタイムの `_str` 対応物を通じて `&k` を受け取り、挿入時だけキーをコピーする | 該当なし |
| 36 | `ThreadShareMarks` | `pass_thread_share.rs` | Rust | 設計上 Rust 用 | ADR-0013: spawn された fan アームやスレッド境界を越すランタイムヘルパー呼び出し（`crosses_threads`。`RuntimeCall` または `fan.map_par` のような `Call { Module }`）が到達する全変数を、スレッド開始前に `InlineRust` ブロックの `(v).almide_share();` で印付けする。walker は描画するだけで独自の `free_vars` 走査はしない（#2186） | 該当なし |
| 37 | `BoxTakeLowering` | `pass_box_take.rs` | Rust | 設計上 Rust 用 | #4251: 再帰型の循環を閉じるフィールドは `AlmideBox`（drop / clone / `==` のスタック使用が有界）となり、`*b` で move して取り出せない。各読み取り（box パターン束縛、再帰レコードの boxed フィールド）は、親が見るだけなら場所 `(*x)`、消費するなら `almide_box_take(x)` と表す |
| 38 | `StrViewRebind` | `pass_str_view_rebind.rs` | Rust | 設計上 Rust 用 | #4296: `String` 対象の `&str` ビュー上の match（`match &*x { "a" => .., other => .. }`）の束縛アームで、束縛が値として必要、またはアームが対象の変数に触れる場合、自身の所有コピーで始める。最終 IR から `str_view_owned_binders` を公開し walker が表記する | 該当なし |

行 14、16、17 は 1 つの走査 `use_kind.rs`（#2186）から所有権の事実を読む。すべてのローカルのすべての出現に、親が置く位置（`Site`）と、クロージャー、融合チェーン、`&mut` の内部かどうかを付記する。`BorrowInsertion` の「この仮引数に所有／`&mut` が必要か」は、不動点スナップショットから呼び出し先引数モードを供給する `SlotOracle` と、その位置集合上の述語で決める。`CaptureClone` の「クロージャーがどの捕捉を書き換えるか」、`CloneInsertion` の使用回数とループ束縛判定は、明示借用オラクルを使う同じ走査上の述語である。不動点は単調上昇（`Ref` → `RefMut` → `Own`。ラウンドが公開する全キーを楽観的に初期化）で、ラウンド上限はない。引数位置が下降したり、束の高さを超えて実行したりすれば ICE となる。

<a id="c-structural-wasm-leg--cratesalmide-wasm-default---target-wasm"></a>

### C. 構造的 wasm 経路 — `crates/almide-wasm`（既定 `--target wasm`）

エミッターがリンク済み IR またはエンコード済み本体に適用する書き換えである。nanopass の意味での IR→IR パスはなく、各処理は `emit_program` 内の経路である。

| パス | ファイル | 処理 |
|---|---|---|
| 透明 newtype の消去 | `newtype.rs` | 透明な別名を消去し、両出力パスが同じ木を読むようにする（#1423 stage 4） |
| 到達可能性 DCE | `emit.rs`（2 パス） | パス 1 は `main` が到達する範囲を記録し、パス 2 は到達可能集合だけで再出力 |
| 決定的メーター計画 | `fuel.rs` | どの関数に消費量を課し、どの入口を免除するか（ALS-DT2、interp に対応） |
| 自己末尾呼び出しのループ変換 | `tco.rs` | `return_call $self` → 仮引数の `local.set` + 囲む `loop` への `br` |
| map/filter → fold 融合 | `list_fuse.rs` | 観測のないコールバックに対する中間リスト除去（B.10 のリスト部分） |
| 計数 while の部分展開 | `unroll.rs` | native 経路で LLVM が得るループ制御の余地 |
| RC-3 所有権ガード | `rc_ownership.rs` | 借用／fresh 分類、drop 可能集合、inc/share/arg ガード（B.14/B.17 の wasm 対応物） |
| ヒープ上限 | `heap_cap.rs` | ハーネスが設定する線形メモリ上限（#1729）。最適化ではない |

この経路はどこでも SIMD（`v128`）を出力しない。行列ルーチンはスカラーカーネル（`matrix_kernels.rs`）である。判断により #929 の対象外とした。

<a id="d-v1-native-trust-spine--cratesalmide-mir-retiring-under-1696"></a>

### D. v1 native trust-spine — `crates/almide-mir`（#1696 で廃止へ）

完全性のため掲載する。一覧ゲートはこの crate を列挙しない。廃止時に各行もなくなるためである。wasm 段階（WAT レンダラーと WAT レベルのパス: オペランド融合、peephole、前置部分 DCE、境界検査の省略、`br_table` ディスパッチ、ローカルスロット再利用、self-append 書き換え、スカラーラッパーのインライン化、リージョン割り当て）は #2761 で削除された。

| パス | ファイル | 段階 | 処理 |
|---|---|---|---|
| 消費量 probe | `charge_probe.rs` | native（`ALMIDE_FUEL_PROBE` ビルド） | 関数入口と各 `LoopStart` の後に `Charge` |
| 不要な `MakeUnique` の除去 | `alias_safety.rs` | native | 別名のない copy-on-write ガードを除去（#824） |
| native Result carrier 書き換え | `native_result_rewrite.rs` | native | Rust 描画の T1-3 Result producer/consumer 書き換え |

<a id="e-pre-split-decision-closure-of-929"></a>

### E. 分岐前へ移す判断（#929 の完結）

共通部分集合は**すでに分岐前に実行する**。表 A が `almide-optimize` の全体であり、最初の表の各経路は唯一の `link_ir` を通してそれを使う。問題は、表 B のターゲット中立な行がここに加わるべきかどうかだけである。候補ごとの判定:

| パス | 今分岐前へ移すか | 障害／理由 |
|---|---|---|
| `ConstFold` (B.12) | 移動不要 | A.1 が分岐前に同じリテラル算術を畳み込む。Rust 分岐では他経路にない B.10–B.11 の生成物を整理するだけ |
| `TailCallOpt` (B.15) | 移動不要 | 構造的経路はエンコード済み本体レベルで自己末尾呼び出しを変換し（`tco.rs`）、残りに `return_call` を維持する。同じ一定スタック保証 |
| `LICM` (B.9) | **今は移さない — 続報作業** | (1) 階層: 分岐点を所有する `almide-driver` は `almide-codegen` ではなく `almide-optimize` に依存し、当該パスは前者の `NanoPass`/`Target` に対して書かれている。1 行のパイプライン変更でなく 3 ファイルの移設になる。(2) 分岐点は interp オラクルと v1 native 描画への入力でもあるため、分岐前への持ち上げ導入前に `ALMIDE_ONLY_PASS=licm` 軸と `spec/pass_isolated/` 行が必要。(3) 構造的経路の利得は未計測。まずループ支配の台帳行（`nbody`、`spectralnorm`）で測る |
| `EggSaturation` (B.10) | **今は移さない — 続報作業** | `almide-egg-lab` を driver より下へ引き込む。行列部分は Rust stdlib lowering だけが使う融合 `matrix.*` 形式へ書き換え、リスト部分には構造的対応物（`list_fuse.rs`）がすでにある |
| `Peephole` (B.24) | **今は移さない — 続報作業** | 出力ノード（`ListSwap` など）は `almide-wasm` に lowering がない。分岐前ではパターンに当たる全 fixture が wall になる（利用側は almide-mir の `lower/mod_p5.rs` と interp だけ） |

`--target wasm32`/`wasi` は別物である。codegen v3 の Rust ソースを `rustc` でコンパイルするため、表 B の Rust 分岐全体と LLVM が適用される。

<a id="stdlib"></a>

## 標準ライブラリ

標準ライブラリは self-host されており、全関数は `stdlib/<module>[_part].almd` に置かれる（旧 `stdlib/defs/*.toml` パイプラインは廃止済み）。利用経路は 2 つある。

- **Native / 型シグネチャ** — モジュールソースはコミット済みの `almide-types/src/generated/embedded.rs` テーブルに埋め込まれ、チェックアウトでは `almide-types/build.rs` が `.almd` ファイルから再生成する。`stdlib_info.rs` がそれを公開し、フロントエンドはシグネチャ、codegen は `@inline_rust` テンプレートを抽出する。`@intrinsic("almide_rt_*")` 宣言は `runtime/rs/src/<module>.rs` の手書き Rust に振り分ける。
- **WASM** — `almide-types/src/self_host_registry.rs` に登録された純 Almide 実装（interp オラクルと共有）を、構造的経路がユーザーコードと一緒にコンパイルする。

自動インポートは `almide-frontend/src/import_table.rs` の seed 一覧と `stdlib_info.rs` の `AUTO_IMPORT_BUNDLED` の和集合である。`json`、`fs`、`http`、`env`、`io`、`random`、`regex`、`testing` などのモジュールには明示的な `import` が必要である。

<a id="build-system"></a>

## ビルドシステム

ルートの `build.rs` は `almide explain` 用の診断文書を埋め込み、バージョン／来歴行を設定する。crate 固有のビルドスクリプトには以下がある。

- `almide-codegen/build.rs` — 書き換え宣言から埋め込みランタイムレジストリと行列脱糖のフォールバックを生成する。旧 `arg_transforms.rs` テーブルは廃止済み。
- `almide-types/build.rs` — `stdlib/*.almd` からコミット済みの埋め込み stdlib ソースと宣言由来テーブルを生成する。
- `almide-frontend/build.rs` — dialect 台帳と regex ランタイムをコミット済み生成ファイルへコピーする。シグネチャは同梱ソースから抽出し、生成 `stdlib_sigs.rs` テーブルからは取らない。

これらの crate ジェネレーターは、チェックアウト内でのみチェックアウト入力を再生成する。単独パッケージはコミット済み生成コピーを使う。

<a id="type-system"></a>

## 型システム

即時単一化を伴う制約ベース推論:

1. **Infer** — AST を走査し、新しい型変数を割り当てて制約を集める
2. **Solve** — 制約を単一化し、解を伝播する
3. **Resolve** — `expr_types` の推論変数を具体型で置き換える

チェッカーの UFCS 解決はレシーバーとスコープを考慮する。例えば List の `xs.map(f)` は `list.map(xs, f)` に解決できる。convention メソッドとローカル／インポート関数にはそれぞれの解決規則がある（`calls_ufcs.rs`）。パイプと解決済み呼び出しは codegen より前に lowering する。文字列補間は `IrExprKind::StringInterp` として残る場合があり、Rust walker が明示的に描画する（`walker/expressions.rs`）。

<a id="module-system"></a>

## モジュールシステム

1. **Resolve**（`src/resolve.rs`）— 走査するのは `import` 宣言で、`.almd` ファイルを探す（ローカル、git 依存。stdlib は同梱ソースで処理）
2. **Register** — モジュールシグネチャを接頭辞付きで TypeEnv に追加
3. **Check** → **Lower** → 独立名前空間を持つモジュールごとの IR
4. **Codegen** — モジュール関数を `almide_rt_{module}_{func}` 接頭辞で出力

<a id="diagnostics"></a>

## 診断

[診断リファレンス](../../../diagnostics/README.md) に現在のコードを列挙する。診断は位置とソース文脈を報告し、既知なら修正ヒントを添える。出力は人間向け形式または `--json` である。

```
error[E005]: argument 'xs' expects List[Int] but got String
  at line 5
  in call to list.sort()
  hint: Fix the argument type
  |
5 | let sorted = list.sort("hello")
  |                        ^^^^^^^
```

<a id="verification-spine"></a>

## 検証基盤

- **契約** — 観測可能なターゲット間の約束はすべて、[contracts/contracts.toml](../../../contracts/contracts.toml) の名前付き `[[contract]]` として実行可能な根拠（fixture / fuzz / Σ-probe / 定理）を持つ。
- **3 者オラクル** — native、wasm、`almide-interp` は stdout/stderr/終了コードで一致しなければならない。
- **Lean belt** — `almide-perceus-belt` が `almide-mir` の実装する Perceus RC 規律を証明する。
