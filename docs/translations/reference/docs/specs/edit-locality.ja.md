# 編集の局所性

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](edit-locality.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](edit-locality.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](edit-locality.zh-CN.md)



> 正本：[docs/specs/edit-locality.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/edit-locality.md)、固定コミット `35297a612fb29e3f78f6587aba253aa91d51f466`。非規範的な読者向けスナップショット訳で、自動同期しない。原仕様とその明示する規範上の位置付けは変更しない。モデル証明・実装の主張・歴史的状態・証拠の範囲は原文の限定を保ち、この翻訳による独立の実行・測定・適合性／安全性証明はない。コードブロック内のコードとコメントは原文のまま。

> 最終更新：2026-09-23

Almide のミッション指標を支える不変条件。MSR（modification survival rate、変更生存率）は LLM の編集が生き残るかを経験的に測る。この文書は、編集を生き残らせる意味論的性質を定め、既存の言語規則をその強制に果たす役割へ対応付け、現在の実装がなお違反する箇所を `file:line` の証拠付きで記録する。この記述を基盤とする研究計画（Lean カーネル、バックエンドの精緻化、MSR 予測ループ）は `docs/roadmap/active/edit-locality-theory.md` にある。本ファイルはモデルの主張、現在の規則、以下に保存された実装上の問題探索を区別する。

観測可能なものの集合は、契約台帳（`docs/contracts/contracts.toml`）が一度定める：**標準出力のバイト列、標準エラー出力のバイト列、終了コード**。以下の「観測可能」は常にこれだけを意味する。

**ここでの `L<n>` は編集局所性の段階であり、このリポジトリには別の段階体系もある。** `docs/roadmap/active/trust-layer.md` は独自の L0–L4 を使う。L1 は「バイナリが壁の内側で何をできるかを証明する」、L2–L4 は「誰かの言葉を信じなくても再導出できる」である。ロードマップ文書で `L3` を見た読者が、この文書ではなくそちらの段階を読んでいる可能性も同程度にある。両体系は無関係であり、こちらの何かがあちらの前提になることはない。文脈から明白でなければ、どちらの体系かを明記する。

**以下の L1〜L5 は使用済みである。** この体系で新しい証明義務を追加するなら **L6** 以降を使い、空いて見える小さい番号を使わない。almide/almide#2009 と #1998 は、このファイルが L1 しか命名していなかった旧版に基づき、ここでの定義とは異なる義務に「L2」「L3」を提案している（#2009 では宣言フレームの保存と、合成可能な検査済み lowering）。それらは §1a の **L4 と L5** であり、#1997 が量化対象の `scoped` 構文を確定した後に採用された。番号を別の意味に再利用することこそ、この文書がプログラムから排除しようとしている欠陥であり、文書自身にあってはならない。

## 1. 不変条件

**L1 — 編集フレーム。** `P` を正しく型付けされたプログラムとし、`e` を定義 `f` 一つの本体への編集とする。この編集は `f` のシグネチャ、すなわち名前、引数型、戻り値型、`effect` マーカー、可視性を保つものとする。このとき、トレースが一度も `f` に入らない `P[e]` の各実行は、`P` の対応する同じ実行と同一の観測結果を持つ。

**L2 — ターゲット間の一致。** 観測結果はターゲットではなくソースだけの関数であり、native と wasm はバイト単位で一致する。これは契約台帳が継続的に約束する事項（現在の契約行、`spec/wasm_cross/` フィクスチャ、差分ファズ）である。L1 が「観測結果」を量化できるのは、L2 がその表現を明確に定義するからである。

**L3 — 診断の局所性。** シグネチャを保つ編集によってプログラムが型不正になった場合、診断位置は未編集のコードではなく、編集した定義の内部になる。（E006 は effect を呼ぼうとした純粋な `fn` の内部で発生し、E041/E042 は編集した本体内の問題の文で発生する。）

中心を支えるのは L1 である。実装済みの言語でこれを満たすものはほぼない。オーバーロード解決、暗黙のインスタンス、マクロ、プログラム全体の型推論、動的スコープの effect ハンドラーはいずれも、離れた編集によって未編集コードの意味が変わることを許す。MSR 上の理由から一つずつ採用された Almide の規則は、まさに L1 の前提条件である。§2 の表はその主張を確認可能にしたものだ。

カーネル計算体系 λ_almd では、L1 は機械検証済みの定理である。`crates/almide-edit-belt`（3 番目の 0-sorry Lean ベルト、CI の `lean-proofs` ジョブ）は、`edit_frame` と決定性（観測結果が同一となる形）、`typing_modular`（同一シグネチャの本体交換は、他のすべての導出に触れずに型の正しさを保つ）、`pure_silent`（`effect fn` の外で型付けされたコードのトレースは空）を証明する。言語全体についての実装レベルの主張は、`docs/contracts/README.md` に従い、引き続き §2 の証拠による制約を受ける。

> 訳注：実装済み言語一般についての上記比較は原文の主張であり、この翻訳で独立に比較検証した結果ではない。カーネルの定理を全実装の証明へ拡張しない。

## 1a. L1 より上：宣言フレームとその合成

L1 は、編集した定義を実際に通る実行について何も述べないし、述べることもできない。正当な置換でも別の値を計算できるからである。そこに残るのは値ではなく、宣言された契約が定義の周囲に描く**フレーム**である。これを表す二つの義務が L4 と L5 だ。

**L4 — 宣言フレームの保存。** `f` の宣言契約を満たす `f` の置換は、その契約が対象とするフレームを保つ。

- *Effect フレーム — λ_almd で証明済み。* `f` が純粋と宣言されていれば、変更していない `f` のシグネチャに対して型検査を通るどの置換本体も、編集後のプログラムのどの環境でも観測結果を生じない。`effect` フラグは置換が越えられない境界である。証拠：`crates/almide-edit-belt/AlmideEditBelt/Contract.lean` の `l4_pure_replacement_silent`。これは `typing_modular`（編集後もプログラムが正しく型付けされる）と `pure_silent` の合成である。その隣には二つの検査済み例がある：`l4_witness`（四つの仮定が同時に成立し、定理が空虚でないことを示す）と `l4_loud_replacement_prints`（自身の *effectful* なシグネチャに対して検査を通る置換は `"hi"` を出力する。純粋性の仮定を外すと結論は偽になる）。
- *リソースフレーム（`scoped`、#1997）— フィクスチャによる証拠であり、証明ではない。* λ_almd にはヒープも領域もないため、領域契約をそこで量化する対象はない。この半分を機械化するには、まずカーネルへ領域の構成要素を加える必要がある。実装レベルでは三つの契約からなり、それぞれ独自の証拠を持つ。
  - C-362 — `scoped` ブロックの観測結果は本体の観測結果であり、領域は観測されない。`spec/wasm_cross/scoped_region_value.almd`。
  - C-363 — `scoped fn` は領域の内外で同じ答えを計算する。`spec/wasm_cross/scoped_worker_in_and_out.almd`。
  - C-364 — scoped フラグメント外の形は、両ターゲットで同じように検査時に拒否される。`tests/diagnostics/e087-scope-unscoped-callee/broken.almd`、`tests/scoped_region_test.rs`。

L4 は業務上の振る舞いを何も主張しない。純粋な置換が別の値を返し、その値によって次に*呼び出し元*が出力する内容が変わってもよい。L4 が述べるのは、置換そのものがトレースへ何も追加しないことだけだ。

**L5 — 実現と合成。**

- *合成 — λ_almd で証明済み。* シグネチャを保つ置換列は、正しく型付けされたプログラムの型の正しさを維持する。全置換の後も、純粋と宣言されたすべての定義は無音のままだ。契約を保つ編集の連鎖がどれほど長くても effect の境界は開かない。証拠：`crates/almide-edit-belt/AlmideEditBelt/Contract.lean` の `l5_edits_compose` と `l5_pure_silent_after_edits`。編集リストに関する帰納法による。
- *両ターゲットでの実現 — 信頼対象。* 検査済み lowering（#1995 の ExitPlan、#1996 の E-OWN-LOWERING）が native と wasm でこれらの観測を保つことは、λ_almd と二つのバックエンド間の精緻化義務である。本ファイルのあらゆるバックエンドの主張と同様、`spec/wasm_cross` フィクスチャと契約台帳の制約を受け、`docs/contracts/README.md` では証明済みではなく信頼対象として列挙される。

## 2. 現在それを強制するもの

| 規則 | 局所性での役割 | 証拠 |
|---|---|---|
| `fn`／`effect fn` の戻り値型は構文上必須（厳格な構文解析エラー。AST に `Option<TypeExpr>` はない） | 呼び出し元が見るシグネチャは推論結果ではなくテキストなので、本体の編集で変わらない | `crates/almide-syntax/src/parser/fn_decls.rs:69-82`, `crates/almide-syntax/src/ast.rs:401` |
| `effect` はシグネチャの一部。検査器は本体ではなく `sig.is_effect` を読む | effect の範囲は「シグネチャを保つ」ことで制限される。通常の effect 呼び出しにはマーカーが必要であり、六つの console／abort 組み込みには ADR-0022 の例外がある | `crates/almide-frontend/src/check/calls.rs:381-408` — 仕様：`effect-system.md` |
| 明示的な伝播のみ（ADR-0008）。E041/E042 は呼び出し先が**宣言した** `Result` 戻り値型に基づく | 呼び出し先でエラーパスが増減しても、呼び出し元の制御フローを暗黙に配線し直せない。伝播点はすべて呼び出し元自身のテキスト（`!`）に見える | `crates/almide-frontend/src/check/post_solve_validation.rs:399-434` — テスト：`tests/expr_stmt_diag_test.rs`, `spec/wasm_cross/expr_stmt_comment.almd` (C-252) |
| glob インポートなし。自動インポート集合はコンパイル時定数 | モジュールに定義を追加しても、別の場所の既存の非修飾参照を捕捉できない | `crates/almide-syntax/src/parser/declarations.rs:75-125`, `AUTO_IMPORT_BUNDLED` in `crates/almide-types/src/stdlib_info.rs:39-56` — 仕様：`module-system.md` |
| モジュール間アクセスは修飾するか明示的に選択インポートする。推移的アクセスはない | 参照はその場所で、または固定されたインポート束縛を通じてモジュールを指定する。追加によってその束縛を暗黙に置き換えられない | `crates/almide-frontend/src/import_table.rs:23-25` — テスト：`spec/integration/modules/` |
| オーバーロード、ユーザー定義演算子、暗黙の型変換、インスタンス宣言はない。protocol 適合は宣言（`deriving`）し、検索は固定規約に従う | 非局所の情報によって選択結果が変わるディスパッチがない | `docs/design/DESIGN.md:27`, `crates/almide-frontend/src/canonicalize/registration_validate.rs:237-265`, E012 (`docs/diagnostics/E012.md`) |
| 固定された UFCS の優先順位、標準ライブラリ優先 | 新しいユーザー `fn len` が `xs.len()` を奪えない | `crates/almide-frontend/src/check/calls_ufcs.rs:36-67` — テスト：`spec/lang/user_fn_namespace_collision_test.almd` |
| 型推論はモジュールごとに隔離。呼び出しの戻り値型は宣言シグネチャから取得。単相化のキーは呼び出し位置の引数型のみ | 制約はモジュール境界を越えず、呼び出し元の型のために呼び出し先の本体を参照しない | `crates/almide-frontend/src/check/module_inference.rs:16-19`, `check/calls.rs:276-310`, `crates/almide-optimize/src/mono/discovery.rs:28-42` |
| 標準ライブラリの純粋性はハードコードされ、機械的なゲートを持つレジストリであり、本体からは推論しない | 最適化の利用側は標準ライブラリの本体編集によらず固定の答えを見る | `crates/almide-mir/src/purity.rs:51,303-306`, ゲート `proofs/check-stdlib-purity-registry.sh` |
| LICM が移動するのは投機的実行に安全な式だけ。effect がなく、トラップの可能性がある演算（添字、map アクセス、非リテラル整数の除算／剰余）もない。式の述語と本体の不動点の両方で強制する | 最適化は、ソースが実行しない経路（反復ゼロのループ）にトラップを漏らせない。推論された純粋性を変える本体編集でも新しい観測を作れない | `crates/almide-codegen/src/pass_licm_purity.rs` (`binop_may_trap` + トラップを考慮した純粋集合, #1424) — テスト：`spec/wasm_cross/licm_zero_trip_hoist.almd` (C-279) |
| ローカル `fn` は選択インポートした関数と非修飾名を共有できない。インポート表の構築時に厳格な E050 エラー | 一つの非修飾名には厳密に一つの解決先が必要。定義追加で既存の呼び出し先を暗黙に変更できない（修正前は検査器と lowering の不一致により、型検査したものとは別の関数が実行された。#1425） | `crates/almide-frontend/src/import_table.rs` (`check_selective_import_fn_collisions`) — テスト：`tests/selective_import_collision_test.rs`, `docs/diagnostics/E050.md` |
| コンストラクター解決は所有者スコープ。修飾された `mod.Ctor` は `mod` 内だけで解決（`lookup_ctor_owned`）。非修飾名は現モジュール自身のケースを優先し、そのファイル自身、インポート先、バンドル済みのケースだけを見る。推移的なモジュールのものは見ず、ファイル自身の同名型を越えても選ばない（#413、#2636、`lookup_ctor_in`。lowering も同じ）。同一モジュール内のケース名重複は登録時の厳格な E019 エラー | 別の場所への variant 追加で既存のコンストラクター参照を再束縛できない。モジュール内では一つのケース名の宣言は厳密に一つ | `crates/almide-frontend/src/type_env.rs` (`lookup_ctor_owned`), `canonicalize/registration.rs` (登録時の E019, #1426) — テスト：`tests/ctor_ambiguity_test.rs`, `spec/integration/modules/qualified_ctor_test.almd` |

## 3. 現時点で破れる箇所（2026-08-15 の探索）

以下の探索記録は実装史として保存されている。旧 MIR／従来系のパスは現在出荷中の障害一覧ではない。未解決の欠陥として扱う前に現行ソースを確認する必要がある。借用推論の行は現行ソルバーへ更新済みである。仕組みを併記する。分類：**V** = 言語／コンパイラのバグで、L1 は成立すべきもの。**2026-08-15 に見つかった三つの V はすべて修正済みで、§2 の行へ昇格している**（#1424 LICM の投機、#1425 選択インポートの衝突、#1426 コンストラクターの所有者スコープ。この未修正件数はゼロを維持しなければならない）。**R** = バックエンドの精緻化義務（ロードマップ Stage 3）。**S** = 修正するのではなく §4 で宣言する側条件。

### R — バックエンドの精緻化義務

これらは表面言語より下にあり、「精緻化としてのバックエンド」（ロードマップ Stage 3）が解消すべき義務である。元の探索はビルド失敗や誤コード生成の仕組みを記録した。廃止された MIR／従来系パイプラインに言及する行は、その過去の経路について述べる。

| 発見 | 仕組み | 証拠 |
|---|---|---|
| セルフホストの splice はソースファイル全体を推移的に不動点までリンクする。標準ライブラリ関数 `A` の編集で、同じファイルの `B` だけを呼ぶプログラムにも新しいファイルが取り込まれる | `link_self_host_runtime_to_fixpoint` | `crates/almide-mir/src/pipeline_link.rs:229-262` |
| splice リンクの衝突は厳格なコンパイルエラー。標準ライブラリの本体編集で、編集した関数を呼ばないプログラムも壊れる（実例：`__hex_fill`、almide#1068） | `dedup_linked_by_name` | `crates/almide-mir/src/pipeline_link.rs:184-222` |
| 全体的な impl 名→呼び出し名の書き換えがユーザー識別子を捕捉する（ユーザーの `fn int_abs` が `int.abs` に向け直される） | **全** MIR 関数に対する `rewrite_impl_names_to_call_names` | `crates/almide-mir/src/pipeline_link.rs:272-286` |
| 借用推論は本体にわたるプログラム全体の単調不動点。本体編集で呼び出し位置の所有権注記が変わりうる。旧反復上限は撤去され、スロットは Ref → RefMut → Own と上昇し、単調性と束の高さを検査する | `infer_borrow_signatures` | `crates/almide-codegen/src/pass_borrow_inference.rs:455-497,568-582`, `pass_borrow_inference_call_sites.rs:3-18` |
| `inline_pure_call_globals` は推移的な本体純粋性によって、トップレベルの `let X = f()` 初期化式を使用位置へ代入するか決める。その結果、潜在的トラップが「起動時に常に」から「読まれるときだけ」へ移動する | 呼び出し先本体へ再帰する `expr_is_pure` | `crates/almide-mir/src/lower/newtype_erase.rs:385-462,517-521,643-656` |
| `test` ブロックの追加／削除が `library_mode` を切り替え、プログラム全体の wasm DCE ルートを変える | `reachable_fn_names` | `crates/almide-codegen/src/reachability.rs:168-183` |
| `Method`／`Computed` の呼び出し先は到達可能性の名前に寄与しない。UFCS 経由で到達する関数への最後の `Named` 参照を本体編集で削除すると、関数が誤って除去される（パス自身の健全性注記によれば wasm トラップ） | 到達可能性の収集 | `crates/almide-codegen/src/reachability.rs:15-17,66-72` |
| 潜在的問題：本体からの全プログラム effect 推論がある（現在は解析のみ）。診断や権限がそれを利用した瞬間、本体編集が呼び出し元に見える出力を変える | `pass_effect_inference` | `crates/almide-codegen/src/pass_effect_inference.rs:12,47-70` |

## 4. 側条件

§1 の L1 は、以下に宣言する例外を**除いて**成立する。例外はここに記されているか、存在しないかのどちらかである。書かれていない例外はバグだ。

- **S1 — 型注釈のないトップレベルの `let`／`var`。** `TopLet` の型は初期化式から推論され、モジュールを越えて伝播する（`crates/almide-syntax/src/ast.rs:406`、`crates/almide-frontend/src/check/module_inference.rs:65-90`）。したがってトップレベル初期化式の編集は、明示的なシグネチャがなくても*シグネチャを変える*編集である。そのような束縛に保存したラムダも同様だ。（`fn`／`effect fn` は除外される。シグネチャは必須のテキストである。§2 の第 1 行。）
- **S2 — `test` ブロックはフレームの外。** ファイルスコープの `mod test where { path = expr }` オーバーライドは、そのファイル内の全テストで参照を意図的に再束縛する（`crates/almide-syntax/src/ast.rs:409-432`、`crates/almide-frontend/src/lower/mod.rs:420-423,489-495`）。これは L1 がプログラムコードで禁じる動的スコープを、範囲を限定して宣言した形である。テストブロックは DCE のルートにもなる（§3-R）。L1 の量化対象はプログラムの実行であって、テストの実行ではない。
- **S3 — モデルの純粋性と実装境界。** ADR-0009 の当初の高階 effect の穴は過去のものだ。関数型の effect マーカーと値フロー検査は、現在は effect の偽装を拒否する（`effect-system.md`、`tests/effect_value_laundering_test.rs` 参照）。ただし、これによってモデルの `pure_silent` 定理がすべての Almide `fn` についての定理になるわけではない。ADR-0022 はその内部で六つの console／abort の prelude 組み込みを許可する。モデル定理は自身の型付け前提を保ち、実装の capability 集計は ADR-0026 に従う。

## 5. ゲート

言語またはコンパイラの変更は、取り込む前に一つの問いへ答える：**L1 を保つか？** 保つなら取り込む。側条件が必要なら同じ PR で §4 に書く。違反するなら、それは設計からの「不可」である。（動的スコープのハンドラー、暗黙のインスタンス、glob 再エクスポート、本体から推論するあらゆるものはこのゲートを通れない。それがゲートの役割だ。）
