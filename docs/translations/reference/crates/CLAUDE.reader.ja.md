<a id="almide-compiler-crates"></a>

# Almide コンパイラのクレート

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](CLAUDE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](CLAUDE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](CLAUDE.reader.zh-CN.md)

> 固定原文のエージェント指示を読むための非活性・非規範スナップショット。原文を正本として変更しない。CLAUDE.reader は自動読込の指示ファイル名ではない。引用規則は実行・コード変更・commit・push・公開の許可ではない。自動同期せず、実装・安全性・性能を独立再検証していない。2026-08-13 の数字は当該ツリー・release・5回最良値の範囲を保持し、普遍的性能保証ではない。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/CLAUDE.md)


> **Codegen 理想形（完了済みの設計記録）**: [docs/roadmap/done/codegen-ideal-form.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/codegen-ideal-form.md) — 新しいパスや emit 修正の設計判断はこの記録に整合させる。

<a id="pipeline"></a>

## パイプライン

```
Source (.almd)
  → almide-syntax    Lex + parse → AST
  → almide-frontend  Type check + lower → IR
  → almide-optimize  Monomorphize + DCE → IR
  → almide-codegen   Nanopass + emit → Rust
  → almide-mir       MIR lowering → v1 native Rust render + certificate witnesses
  → almide-wasm      structural leg → WASM bytes (the only wasm renderer)
```

<a id="dependency-graph"></a>

## 依存グラフ

```
almide-base           Interned strings (Sym), spans, diagnostics
  ↕
almide-types          Ty enum, unification, stdlib info
almide-syntax         Lexer, parser, AST nodes
  ↕
almide-lang           Re-export facade (types + syntax)
  ↕
almide-ir             Typed IR, VarTable, visitors
  ↕
almide-frontend       Type checker, constraint solver, AST→IR lowering
almide-optimize       Monomorphization, DCE, constant propagation
almide-codegen        Nanopass pipeline, TOML templates, walker（WGSL は @gpu 関数の限定的なソース出力）
almide-mir            v1 Middle IR: ownership/layout SoT, MIR lowering + native renderers +
                      certificate witnesses (proofs/gate.sh; `almide verify` judges the
                      structural leg's since #4011); no WASM/WAT
                      renderer since #2761
almide-interp         Pre-codegen IR tree-walker — 3rd cross-target oracle / executable spec
almide-tools          Formatter, module interface (.almdi). NOT the LSP — that
                      lives in src/cli/lsp*.rs at the workspace root, and
                      lsp-types is a root Cargo.toml dependency, so a
                      crates/-scoped survey will not find it
almide-layout         THE single source for heap block layout — every consumer
                      (almide-wasm, almide-interp's arena) derives from it
almide-wasm           Commissioned structural wasm emitter: typed IR → wasm
                      bytes via wasm-encoder (no WAT text). The only
                      `--target wasm` leg (routing: `almide::wasm_route` in
                      the root lib — the CLI and the playground call the
                      same `route_wasm`, #2554)
almide-wasi           The `to_wasi` transform that makes build artifacts
                      stock-runtime, and the p1 host-op set it serves — pure
                      wasm-encoder/wasmparser, builds for wasm32 (#2554);
                      re-exported as `almide_wasm_run::wasi`
almide-wasm-run       The embedded almide.* host (wasmtime) + the p2/p3
                      component transforms
almide-anchor-check   The byte-level translation check of #4011 (d): decodes a
                      shipped module itself (wasmparser only, no compiler
                      code) and holds every located bounds / init witness
                      event to the instruction it names; run per artifact by
                      proofs/structural-wall.sh
almide-wasm-vm        The qualification-scoped wasm interpreter (#865): runs a
                      shipped `to_wasi` artifact with an instruction set pinned
                      to the emitter's, fixed buffers and fuel; std only.
                      Requirements: almide-wasm-vm/REQUIREMENTS.md
almide-spine          Salsa-cached front queries + the parity gates; its s5
                      driver re-exports `almide::wasm_leg` (root lib), so the
                      product leg and the gates judge ONE implementation
almide-corpus         Corpus path resolution (in-tree, or the als/ judge mount
                      in the greenfield form)
almide-str            ADR-0013's shared string buffer (AlmideStr: one block
                      [rc: u32, spare: u32, len | bytes], a 16-byte header,
                      edited through a unique view). Raw allocation, so it
                      does not take the workspace's `forbid(unsafe_code)`;
                      CI runs its tests under Miri (`miri-almide-str`).
                      Every generated native program splices it in as its
                      Almide String (`AlmideString = AlmideStr`;
                      docs/roadmap/active/adr-0013-string-sharing.md)
```

`almide-interp` はリンク済み IR の*兄弟コンシューマー*であり、コンパイルパイプラインの一部ではない。`lower → optimize →
mono → ir_link` の**後**、`almide-codegen` のいずれのターゲット lowering パスよりも**前**という切断点の IR を実行するため、どちらのバックエンドとも codegen パスを共有しない。[almide-interp/CLAUDE.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-interp/CLAUDE.md) を参照。

<a id="core-design-principles"></a>

## 中核となる設計原則

1. **型検査器が正本。** すべての式の型は almide-frontend が設定する `TypeMap` から得る。lowering と codegen はそれを信頼し、型を再推論しない。

2. **IR が完全な型情報を持つ。** すべての `IrExpr` に `ty: Ty` フィールドがある。codegen が emit 時に型検査器へ問い合わせる必要を作らない。

3. **VarId がシャドーイングを解消する。** lowering 中に全変数へ一意の `VarId(u32)` を割り当てる。IR と codegen で文字列ベースの変数検索を行わない。

4. **lowering で一度だけ脱糖する。** pipe（`|>`）、UFCS（`x.method()`）、文字列補間（`"${expr}"`）を almide-frontend の lowering パスで脱糖する。codegen はこれらの形式を見ない。

5. **Nanopass の分離。** 各 codegen パスは意味変換を一つだけ行う。walker はターゲット非依存で、`if target == Rust` のような検査をしない。ターゲット差はパス選択と TOML テンプレートに表す。

6. **すべてで文字列を intern する。** 全識別子・型名・フィールド名は `Sym`（intern 済み、`Copy`）。文字列パターンでなく `==` で比較する。intern に `almide_base::intern::sym()`、読み取りに `.as_str()` を使う。

<a id="layout-discipline-new-long-lived-structures"></a>

## レイアウト規律（新しい長寿命構造）

**規則（#1316）。** コンパイルパイプライン中の**新しい**長寿命データ構造はすべて、モジュールキャッシュ形式、MIR→CLIF 構造、一つのコンパイル段階より長く存続する成果物を含め、**平坦配列 + `u32` ID + intern 済みアトム**で構築する。`Box`/`Rc` の多い木、`String` フィールド、読込時に要素ごとの再構築が必要な map は使わない。既存の AST/IR は明確に対象外。まだ存在しないコードを拘束し、既存の作り替えを要求しない方針である。

`scripts/check-layout-discipline.sh` が `proofs/layout-ledger.toml` に照らしてゲートする。cache/CLIF モジュールの型、または凍結済み既存ファイル以外で `Serialize`/`Deserialize` を導出する型には、台帳行が必要。`FLAT`（毎回ソースから再導出するため行に虚偽を置けない）、または `DEVIATION` と、そのコストと解消条件を述べる一段落の注記を記す。`DEVIATION` 件数は減少しか認めない上限。**ゲートで判定できないもの — SoA と AoS、関数本体、型 alias — はスクリプト冒頭に列挙される。その部分はレビューが担い、判断理由は deviation 注記に書く。** 以下の測定は 2026-08-13、このツリーで `--release`、5 回の最良値。

1. **所有テキストでなく intern 済みアトム。** 上の原則 6 を長寿命の*行*でも守る。`Option<Sym>` は niche packing で 4 B、`Option<String>` は 24 B。この一フィールドだけで `VarInfo` は 120 B → 96 B（−20%）になる。`VarInfo.module_origin:
   Option<String>` が持つのはモジュール*名*であり、IR のほかの箇所では `Sym` である。問いただすものがなかったため入ってしまった。このフィールドを埋めた 100k 行の表の clone は、所有型で 1.95 ms、intern 型で 0.57 ms（3.4 倍）。

2. **埋込み木でなく整数 ID** — 等値判定を整数比較にする。`Applied(UserDefined(String), [Int])` の `Ty` 等値比較は 4.33 ns/cmp（`TypeConstructorId::UserDefined` の payload は所有 `String` なので、同じユーザー型を `strcmp` で比較する）、3 段入れ子の `Ty` は 7.28 ns、一方 `u32` は 0.57 ns で、7.6 倍・13 倍。`DefInfo` は intern 済みフィールド四つと一つの `ty: Ty` で、この一フィールドのために `DefTable` をそのままバイト列として読み込めない。

3. **平坦配列なら読込は read になる。** `spec/stdlib/stdlib-test.almd` の IR は 1.93 MB の JSON。`serde_json::from_str::<IrProgram>` と同じバイト列の `fs::read` を比較すると、一回は 4.269 ms 対 0.059 ms、別の回は 6.648 ms 対 0.088 ms — 72 倍・76 倍。ポインタの多いキャッシュは毎ビルド・モジュールごとにこのコストを払い、平坦なものは read のコストを払う。

4. **パスが一列を走査するなら SoA。** 200k 行の `use_count` 合計は `VarInfo` の 120 B stride で 0.234 ms、`Vec<u32>` の列なら 0.090 ms（2.6 倍）。これはゲートが*強制できない*条項。平坦な行の `Vec<Row>` は合格するが array-of-structs である。レビューで決定し、その判断を記録する。

手本となる形は既にある。`almide-ir` の `VarId`/`VarTable` と `DefId`/`DefTable`（`u32` newtype + `Vec<Row>` + `get(id)`）、`almide-base` の `Sym`。

<a id="when-adding-a-new-feature"></a>

## 新機能を追加するとき

- **新構文** → almide-syntax（parser）→ almide-frontend（checker + lowering）→ almide-codegen（passes + templates）。
- **新 stdlib 関数** → `stdlib/<module>[_<part>].almd` の pure-Almide 実装。本体の `fn` の上に `@implements("module.fn")` を置いて WASM + interp 向けに登録する（build が行を生成。`self_host_registry.rs` は縮小のみの legacy 表）。必要な場合だけ native intrinsic を `runtime/rs/src/<module>.rs` + `@intrinsic` 宣言に置く。インタープリタは対応済みの最小機能範囲で登録 self-hosted 本体を自動評価する。その範囲では関数を扱えない場合に限り手動管理の bridge glue を足し、3-way oracle が棄権せずカバーし続けることを確認する。[almide-interp/CLAUDE.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-interp/CLAUDE.md#coverage-model--does-a-new-stdlib-fn-get-covered-automatically) を参照。
- **新しい型** → almide-types（Ty variant）→ almide-frontend（推論規則）→ almide-ir（IR nodes）→ almide-codegen（出力）。
- **新 codegen ターゲット** → almide-codegen（パスパイプライン + TOML テンプレート + ターゲットエントリ）。
- **複数の stdlib 関数が共有する性質**（入力を消費する、in-place に変更する、寿命が尽きる入力を再利用してよい等）→ 各関数の `.almd` 宣言の属性（`@consume`、`@borrow_ref` 等）として、その関数の場所で宣言し、全実行経路にその宣言を読ませる。名前一覧（`matches!(f, "set" | "swap")`、`&["almide_rt_…", …]`）で答えない。一覧は性質の第二のコピーであり、後から加わった関数が黙って漏れる。`tests/name_keyed_sites_test.rs` が `proofs/name-keyed-sites.toml` に照らしてゲートする。ゲート以前の名前一覧は縮小のみの `legacy` 行であり、新規のものは拒否する。

## 原文の図中説明の訳

パイプライン：.almd ソースから、syntax の字句・構文解析で AST、frontend の型検査・lowering で IR、optimize の単相化・DCE で IR、codegen の Nanopass・emit で Rust、mir の MIR lowering で v1 native Rust 出力と証明書 witness、wasm の構造的経路で WASM バイト列へ至る。最後が唯一の wasm renderer と記される。

依存図の説明を順に読むと：

- almide-base：intern 済み文字列 Sym、span、診断。
- almide-types：Ty enum、単一化、stdlib 情報。almide-syntax：lexer、parser、AST ノード。
- almide-lang：types と syntax を再エクスポートする facade。
- almide-ir：型付き IR、VarTable、visitor。
- almide-frontend：型検査器、制約 solver、AST→IR lowering。almide-optimize：単相化、DCE、定数伝搬。
- almide-codegen：Nanopass pipeline、TOML テンプレート、walker。WGSL は @gpu 関数に限定したソース出力。
- almide-mir：v1 Middle IR。所有権・layout の正本、MIR lowering と native renderer、証明書 witness。proofs/gate.sh を使い、#4011 以降 almide verify は構造的経路のものを判定する。#2761 以降 WASM/WAT renderer は持たない。
- almide-interp：codegen 前の IR tree-walker。第三の cross-target oracle／実行可能仕様。
- almide-tools：formatter と .almdi モジュール interface。LSP ではない。LSP は workspace root の src/cli/lsp*.rs にあり、lsp-types は root Cargo.toml の依存なので crates/ 限定調査では見つからない。
- almide-layout：ヒープブロック layout の唯一の正本。almide-wasm と almide-interp の arena を含む全コンシューマーがそこから導出する。
- almide-wasm：採用済みの構造的 wasm emitter。型付き IR を wasm-encoder で wasm バイト列に変換し、WAT text は使わない。唯一の --target wasm 経路。root lib の almide::wasm_route が振り分け、CLI と playground は同じ route_wasm を呼ぶ（#2554）。
- almide-wasi：成果物を stock runtime 向けにする to_wasi 変換と、それが提供する p1 host-op 集合。純粋な wasm-encoder/wasmparser で wasm32 向けに build する（#2554）。almide_wasm_run::wasi として再エクスポートする。
- almide-wasm-run：組込み almide.* host（wasmtime）と p2/p3 component 変換。
- almide-anchor-check：#4011(d) のバイト単位翻訳検査。出荷モジュール自体を wasmparser のみで decode し、コンパイラコードを使わず、位置を特定できた各 bounds/init witness event を、その名指す命令と照合する。proofs/structural-wall.sh が成果物ごとに実行する。
- almide-wasm-vm：適格性評価の範囲を定めた wasm interpreter（#865）。emitter と一致するよう固定した命令集合、固定バッファ、fuel で出荷 to_wasi 成果物を実行し、std のみを使う。要件は almide-wasm-vm/REQUIREMENTS.md。
- almide-spine：Salsa キャッシュ付き frontend query と parity gate。s5 driver は root lib の almide::wasm_leg を再エクスポートするため、製品経路とゲートは一つの実装を判定する。
- almide-corpus：ツリー内または greenfield 形態の als/ judge mount にある corpus のパス解決。
- almide-str：ADR-0013 の共有文字列バッファ。AlmideStr は図の rc・spare・len・bytes の一ブロック、16バイト header、unique view 経由で編集する。raw allocation のため workspace の forbid(unsafe_code) を採用しない。CI は miri-almide-str で Miri テストを実行する。生成される全 native プログラムへ Almide String として組み込む（AlmideString = AlmideStr）。参照先は docs/roadmap/active/adr-0013-string-sharing.md。
