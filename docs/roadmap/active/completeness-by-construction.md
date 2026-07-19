<!-- description: Make compiler correctness structural (by construction) rather than convention-enforced, extending the Perceus/borrow-checker completeness pattern compiler-wide -->
# Completeness by Construction

> Perceus や Rust の borrow checker が持つ「機械的に完全性を担保する」性質を、
> コンパイラの細部にまで宿らせる。規約 (convention) ではなく検証器 (verifier)、
> 検証器よりも構造 (by construction) — 間違った状態を**表現できなく**する。

Status: **Active** — 2026-06-10 起草。既存の防衛帯
([correctness-guarantee-gaps](./correctness-guarantee-gaps.md) = codegen 連鎖、
[determinism-belt](./determinism-belt.md) = 決定性、
[certification-grade](./certification-grade.md) = 意味論の権威、
[almide-perceus-belt](./almide-perceus-belt.md) = RC)を**フロントエンドの規則一貫性と
名前同一性**へ拡張する。

## 原則

1. **One rule, one place.** 同じ意味規則 (型強制・名前修飾・unwrap) が N 箇所に
   コピーされていれば、N-1 箇所はいずれ乖離する。規則は 1 つの関数に置き、
   全構文位置がそれを呼ぶ。
2. **Producers over consumers.** 間違った値 (bare name, target-blind wrap) を
   下流でパッチするのではなく、生産者を 1 つに絞って正す。
3. **Accept ⟹ correct, reject ⟹ actionable.** チェッカーが受理したプログラムは
   両ターゲットで正しく動くか、`[COMPILER BUG]` で停止する。rustc エラーや
   実行時 trap に「受理済みプログラム」が到達したら、それは設計上の穴。
4. **Verifier first, then tighten the types.** まず検証ゲートで穴を観測可能にし、
   ゼロを達成したら型 (newtype / type-state) で再発を表現不能にする。
   (Verified→Canonical の前例どおり。)

## 実証: 2026-06-10 の 1 セッションで踏んだ穴 (全て修正済み)

この日 1 日で見つかった 7 件は、全てが「機械的保証の不在」に正確に対応する。
どの保証機構が欠けていたかの実地サンプルとして残す。

| バグ | クラス | 欠けていた機構 |
|------|--------|----------------|
| #484 variant payload が bare 名で emit (E0425) | 名前修飾の producer 漏れ | 修飾は resolver 1 箇所で (§1) + cross-module 形状マトリクス (§2) |
| #485 `x = effectCall()` だけ unwrap されない (E001) | 規則の構文位置間乖離 | binding 強制の単一チョークポイント (§3) |
| #486 cross-module top-let が LazyLock から move (E0507) | 表現変更時の旧述語残骸 (`starts_with("ALMIDE_RT_")` が dead 化) | storage-class 判定の単一化 (§4) + マトリクス (§2) |
| record リテラル型が bare のまま IrTopLet.ty へ → native E0425 / **wasm 実行時 trap** | 名前修飾の最後の producer 漏れ | §1 + emit の silent-trap 禁止 (§5) |
| paren 形 record 構築 `Cfg(name: x)` が args を黙って捨てる (チェッカー素通し) | 受理経路の無検証フォールスルー | TypeName 呼び出しの全数検証 (§6) |
| `almide fmt` が型位置のみで使われた import を削除 | フォーマッタの意味保存が無保証 | fmt 意味保存ゲート (§7) |
| record-variant の paren パターンが native E0164 / wasm 受理 | 同上 (§6 の片割れ) | §6 |

教訓: #484/#486 は **12 リリース間** 誰にも気付かれなかった。spec コーパスに
cross-module 形状がほぼ無かったから。コーパスが踏まない形状は存在しないのと同じ
— だから形状を**生成**するゲート (§2) が要る。

## フロンティア (優先順)

### §1 QualifiedRef: 名前修飾を構築時に強制 — #433/#484 クラスの根絶
- **不変条件**: 名前解決を過ぎた IR に bare な cross-module 型/関数名は存在しない。
- **(a) ✅ 完了 (2026-06-10)**: `verify_names.rs::assert_names_resolvable` — codegen
  入口 (pipeline 前、両ターゲット・両プロファイル) で IrProgram 全域の Ty 位置を走査し、
  「bare 宣言が無く修飾宣言だけがある bare `Ty::Named`」を `[COMPILER BUG]` 停止。
  **設置した瞬間に実弾検出**: derive 署名 (`register_derive_sigs`) の value 型が bare の
  まま caller の var_table に届いていた (無症状で潜伏)。producer を修正し、検証器は
  オフェンダー 0 で常駐。純関数検出器 + 単体テスト 3 本。
- **(b) 残**: ゼロ実証の継続後、resolver でしか構築できない `QualifiedRef` newtype に
  置換し、`MODULE_METHOD_FNS` 等の per-pass 再導出を削除。
- **prior art**: rustc DefId / GHC Name vs OccName。 effort: (b) weeks。

### §2 Cross-module 形状マトリクスゲート — 「rustc エラー = コンパイラバグ」の機械化
- **不変条件**: 生成された形状マトリクス (定義サイト × 参照形状 × 型クラス ×
  バインディング位置) の全セルが `--target rust` でコンパイルでき、wasm で trap しない。
- **根拠**: #484/#486/N2/N3 は全てこのマトリクスのセル。1 日で 4 セルの空白を踏んだ。
- **✅ 完了 (2026-06-10)**: `tests/crossmod_matrix_test.rs` — 20 セル (定義サイト ×
  参照形状) を temp プロジェクトに生成展開し、native ビルド (rustc エラー = ゲート赤) +
  実行 + wasmtime バイト一致をハードゲート化。`KnownBroken` ラチェット付き (修正したら
  フラグ除去を強制、単調減少のみ)。**初走行で実穴検出**: cross-module の paren-named
  構築 `m.Cfg(name:)` が E002 — #488 正規化が `TypeName` callee のみ対応で、Member
  callee (`m.Cfg(...)`) を見ていなかった。同日修正、20/20 green。
- **拡張余地**: 2 パッケージ構成 (#433 同名型クラス)、generic record、effect 形状の追加セル。

### §3 Binding 強制の単一チョークポイント — #485 クラスの根絶
- **不変条件**: `let x = e` が受理される ⟺ `x = e` が受理される (mutability を除く)。
  effect-Result unwrap・coercion は全 binding 位置で同一。
- **現状**: 半分達成 (2026-06-10) — checker は `effect_unwrap_rhs` 1 関数、lowering は
  `coerce_to_target` 1 関数に集約。ただし checker↔lowering の 2 相が一致することは
  fixture (C-064) で固定しただけで、メタモルフィックには検証していない。
- **残り**: xtarget-fuzz の項生成器を流用し、受理プログラムの binding 形を相互
  書き換えして受理等価性を assert するメタモルフィック CI。lambda 本体内の auto-?
  欠落 (checker は auto_unwrap を維持、auto_try は Lambda を素通し — 既知の共有バグ)
  もこのゲートが検出する。
- **prior art**: EMI / metamorphic compiler testing。 effort: week。

### §4 Top-let storage-class 決定表 — #486 クラスの根絶
- **Stage 0 ✅ (2026-06-11)**: 形状マトリクスに (mutability × module-origin) セルを増設した
  瞬間に 4 実害バグが落ちた — #500 (cross-module `var` 両ターゲット死、wasm は typed-zero
  黙殺), #501 (module fn 内 self-append が捨てクローンに push), #502 (spread base
  Unknown), #505 (直接代入 `m.x = v` が VarId(0) フォールバックで偽 lvalue)。全修正済み
  (v0.27.1)、マトリクス 29/29。読み・代入・キャプチャの global 解決は共有ヘルパへ一本化
  (`lookup_global` / `module_top_let_var`)。
- **Stage 1 ✅ (2026-06-11)**: `almide_ir::top_let_storage` = THE 決定表 (CopyClass 単一述語 /
  classify_storage 全域 match / static_name 唯一のフォーマットサイト / init_can_abort 唯一の
  abort 述語 / alias 解決全域性)。`TopLetStoragePass` が両パイプライン末尾で 1 回計算し、
  **walker 側 agreement verifier** が legacy 述語 (lazy_top_let_names / const_top_let_vars /
  eager_force order / var_storage) との一致を毎ビルド assert — 次の drift は silent
  miscompile でなく `[COMPILER BUG]` ビルド拒否。unresolvable な module-origin 参照は
  パスが構造化エラーで拒否 (#500 クラスの全域性)。
- **Stage 2 ✅ (2026-06-11, v0.27.2 + endgame)**: 全 consumer 反転 + legacy 完全削除。
  walker pre-index/register/agreement-verifier 撤去、lazy_vars / lazy_top_let_names /
  const_top_let_vars / eager_force_top_lets / var_storage_by_name フィールド削除、
  `get_var_storage` は VarId-only (名前 fallback 消滅)、**VarStorage は locals-only**
  (ModuleCell/ModuleRc variant 削除 — module-global storage は TopLetStorage が唯一)。
  pass_clone の `__cap_`/`__licm` name-prefix 述語は producer 側 VarId スナップショット
  (`always_clone_vars`、CaptureClone/LICM が marking) に置換。全反転 byte-diff zero ×342。
- **Stage 2c (残, 挙動レビュー付き)**: CopyClass 正準化 (Float32/Unit/numeric-tuple の
  Cell 化など 4 述語の統一)。
- **不変条件**: top-let の native ストレージ (const / LazyLock / ModuleCell / ModuleRc)
  と参照様式 (move / clone / borrow) は、(型の Copy 性 × 同/異モジュール × 可変性 ×
  使用様式) の**全域 match** で 1 回だけ決まり、emit はその属性を消費するだけ。
- **現状**: 判定が pass_clone (always/eligible)、walker (lazy_top_let_names,
  const_top_let_vars, var_storage_by_name)、lowering (module_origin) に分散。
  #486 は分散ゆえの取りこぼしだった。
- **形**: VarStorageClassification (v0.22) の前例に従い `TopLetStorage` 属性パスへ
  集約。§2 のマトリクスがセルを fixture 化する。 effort: days。

### §5 Silent-trap 禁止: emit の miss-arm を `[COMPILER BUG]` に
- **不変条件**: emit_wasm のルックアップ失敗 (record_fields miss, func_map miss 等) は
  コンパイル時エラーであり、`unreachable` 命令として実行時まで潜伏しない。
- **根拠**: N3 の wasm trap は emit_member の field_offset miss が黙って
  `unreachable` を emit した結果。resolved な型での miss は 100% コンパイラバグ。
- **✅ 完了 (2026-06-10)**: 6 サイト変換 — emit_member の field miss (resolved 型のみ、
  Unknown は dead-code 残渣として trap 維持)、user-fn の func_map miss、**未対応
  IrExprKind の catch-all `_ => unreachable`** (最大の穴)、FnRef wrapper miss、
  ClosureCreate table miss、value dispatch catch-all。すべて `[ICE]` ビルド失敗に。
  コーパス 264/264 で誤爆ゼロ。**残**: equality.rs:797-798 (Unknown-in-dead-code
  リスク高のため保留)、calls.rs 994/1139 (closure-call の非 Fn 型 else)。

### §6 コンストラクタ呼び出しの全数検証 — accept-then-explode の根絶
- **不変条件**: `TypeName(...)` / `TypeName { ... }` は、(a) 検証済み構築に正規化
  されるか、(b) actionable な E-code で拒否されるかの二択。チェッカーを素通りして
  rustc / wasm trap に到達する経路は存在しない。
- **現状の穴** (2026-06-10 実測): check/calls.rs:131 の無条件 `else` フォールスルー。
  paren 形 record 構築は named args が**黙って捨てられ**、brace 形も
  unknown/duplicate/missing-field 検証が無い (rustc E0560/E0063 リーク)。
  record-payload variant の paren パターンは native/wasm で挙動が割れる。
- **形**: (1) paren-NAMED 構築をチェッカーで brace パイプラインへ AST 正規化
  (LLM の事前分布は `Cfg(name: x)` を多発する — MSR 上、受理が正しい)。
  (2) positional-on-record は新 E-code で拒否 (フィールド並べ替えで意味が変わる
  anti-MSR 形)。(3) Record arm に field-set 検証 (unknown/did-you-mean/duplicate/
  missing-without-default) を追加 — brace 形と正規化後の paren 形が共有。
  (4) calls.rs:131 のフォールスルーを診断に置換。(5) record-payload case の
  paren パターンを `SetEmotion { .. }` ヒント付きで拒否。
- effort: week。**次の着手候補 #3** (根本原因調査・修正設計は完了済み)。

### §7 fmt 意味保存ゲート
- **不変条件**: fmt は意味を変えない — 入力が型検査を通るなら出力も通る。
- **根拠**: fmt が型位置のみで使われた import を「unused」と誤判定して削除していた
  (2026-06-10 修正)。roundtrip ゲートは「再 parse 可能 + fixpoint」しか見ないので、
  意味を変える整形は素通りする — 設計済みの盲点。
- **✅ 完了 (2026-06-10)**: `fmt_output_typechecks_single_file_specs` — 単一ファイル系
  spec 341 本の fmt 出力を `almide check` にかけるゲート + 今日のバグクラスの直接
  ユニットピン 4 本。**設置した瞬間に 5 クラスの意味破壊を検出**(昨日の 2 件に追加):
  (1) match-subject の `json.parse` 使用でも import 削除 (AST ウォーカーの別の穴) →
  unused 判定を**トークン走査の上位集合**に置換 (by construction で漏れない; 削除は
  recall、追加は precision の非対称設計)。(2) `mut` パラメータ欠落 → E007。(3) module
  レベル `var` を `let` に書換え。(4) **test `where` 節を丸ごと削除**。(5) stdlib
  モジュール名と同名のローカル変数への UFCS に偽 import を注入 → ADD 側に bundled-source
  関数存在検証ゲート。fmt スイート 48/48。**残**: multi-module ファイルの再型検査
  (sibling 解決が必要)、`Decl::TestWhereDef` printer (module スコープ where、構文未使用)。

### §8 Stdlib 意味論マニフェスト — #419 クラスの根絶
- **不変条件**: stdlib 関数の文書化された契約次元 (index の単位 codepoint/byte、
  境界クランプ、エラー variant、Unicode 範囲) は stdlib 定義の構造化フィールドで
  あり、ドキュメントはそこから生成され、次元ごとの property fixture が両ターゲットで
  主張を検証する。
- **根拠**: #419 — docs は `string.len` を「文字数」と明記、実装はバイト数。
  `index_of`(byte) と `take/drop`(char) の単位混在は multibyte で黙って壊れる。
  doc↔impl の一致を見る機械は今日まで存在しない。
- **形**: 第 1 歩は #419 自体の修正 (len/index_of/last_index_of を codepoint に統一、
  native+wasm 同時、multibyte spec/wasm_cross fixture + 契約)。第 2 歩で
  stdlib/*.almd の @semantics 注釈 → 生成ドキュメント + 派生 property fixture
  (runtime-registry regen ゲートと同じ regen-and-diff 形)。 effort: 第 1 歩 days、
  第 2 歩 weeks。

### §9 almide-interp 第三審を配線 — native=oracle の循環を切る
- certification-grade CG-1 と同一 (詳細はそちら)。**更新 (2026-07-19)**: `oracle::InterpOracle`
  は実装ゼロではなくなった — `tools/xtarget-fuzz/src/main.rs` の worker ループ (~247行目) と
  replay パス (~416行目) の両方に配線済みで、native/wasm の2-way投票に対する**生きた
  3rd/4th オラクル脚**として毎 fuzz 実行で走っている (commit 7e8afb14, 2026-07-19,
  "Normalize the to_fixed decimals domain abort across all four legs" — "four legs" が
  native/wasm/interp の合流を裏付ける)。「2-way 投票は両方同じに間違うバグに構造的に盲目」
  という問題そのものは解消済み。**残**: interp が abstain するケースの網羅性監査、
  3-way 不一致時の自動 triage/minimize 経路、CI 常時化 (現状 nightly fuzz worker のみ)。
  effort: days (監査) 〜 week (triage 自動化)。

### §10 Release-parity: debug-only 検証器の常時化
- **第一歩 ✅ (2026-06-10)**: wasmparser::validate を**常時 + 致死**化。§2 マトリクスの
  CI 走行が実害を実証した直後に断行 — Unit tail var の幻スタック値で **emitter が不正
  モジュールを出荷**しており、wasm-opt のある環境だけが偶然修復・wasm-opt の無い CI
  ジョブで wasmtime が拒否 (validate が print-only だったため出荷を止められなかった)。
  emitter 側も修正 (Unit 型 Var は値を emit しない = ty_to_valtype 契約に従う)。
  生モジュール一括検証: wasm_cross 106/106 valid。
- **残**: per-pass postcondition / MonoVerify の release 昇格、post-emit RC カウント
  (現状 debug-only)。 effort: days。

## 残存地図 (post-v0.27.0, 2026-06-11)

v0.27.0 (true Perceus: wasm frees デフォルト ON) 時点で「完全性が確保された」と
**言えない**ものの台帳。柱は立ったが、宣言ではなく方向 — 以下が残り。

### メモリ回収の既知有界例外 2 つ (契約 C-066 に明記済み)
1. **TCO 自己再帰ループの反復毎リーク** — Stage C 再設計待ち
   ([wasm-frees-ownership-discipline](./wasm-frees-ownership-discipline.md) 参照。
   旧 M2 の cherry-pick は free list を汚染し棄却済み; acceptance =
   `spec/churn/tco_loop_churn.almd`)。
2. **construct-from-temp の参照 +1 リーク** — `emit_stored_field` の alias dup が
   moved-out temp (Dec 免除) と対で過剰計上。リージョンリセット圏内のループでは
   arena が吸収するが圏外では 1 構築 1 参照のリーク。実測: 2M 反復で ~48MB
   (リセット無効時)。根治 = Koka 流 dup/drop の inc/dec 同時精密化。
   どちらも**安全方向 (リークであって破損ではない)** — ただし O(1) メモリ主張には
   この脚注が付く。

### フロントエンド規則一貫性の残り (本文 §; 優先順)
- **§4** top-let storage-class 決定表 (#486 クラスの構造的根絶) — days
- **§3** メタモルフィック binding ゲート (`let x = e` ⟺ `x = e` 受理等価) — week
- **§1b** QualifiedRef 型化 (bare 名を構築時に表現不能へ) — weeks
- **§9** interp 第三審の配線 (2-way 投票は両者同罪バグに盲目) — week
- **本丸 #433**: 型同一性の per-package 名前空間化 — 型 identity が bare 名で
  end-to-end な構造的負債。§1b はその機械化に相当 — weeks

### 証明の被覆ギャップ
- **Lean Perceus belt は IR レベルの Inc/Dec 均衡まで** — 0.27.0 で実体化した
  ランタイム frees 本体 (free-list push/reuse、センチネル、リージョンリセットの
  アロケータ状態不変量) は**証明圏外**。churn/byte ゲートが現状の唯一の番人。
  → [almide-perceus-belt](./almide-perceus-belt.md) の次フェーズ候補。
- emitter 級 rc 操作 (stored-field dup、ランタイム内 inc/dec) は IR 検証器から
  **不可視** — 検証器が数えるのは IR ノードのみ。

## 着手順序 (更新 2026-06-10 第 2 ラウンド)

1. ~~§2 マトリクスゲート + §5 silent-trap 禁止~~ ✅
2. ~~§6 コンストラクタ全数検証~~ ✅ (v0.26.20)
3. ~~§7 fmt 意味保存 + §8 第 1 歩 (#419)~~ ✅
4. ~~§1(a) NameResolutionTotal 検証器~~ ✅ → 次: §4 決定表 → §3 メタモルフィック
5. §1(b) QualifiedRef 型化、§9 interp 第三審、§10 release-parity — 帯の恒久化

## 進捗ログ

- 2026-06-10: 起草。#484/#485/#486 + record-literal 修飾 + fmt import 削除を修正
  (PR #487)。`canonical_user_type_sym` 一本化、`effect_unwrap_rhs` /
  `coerce_to_target` チョークポイント化、C-064 契約。§6 の根本原因調査完了
  (check/calls.rs:131 フォールスルー、lower/calls.rs:83-98 named-args 黙殺)。

- 2026-06-10 (第 2 ラウンド): §5・§1a・§2・§7 の 4 ゲート設置完了。設置作業そのもので
  潜伏バグ 8 件を検出 (§1a: derive 署名の bare 名 producer / §2: Member-callee
  paren 構築の E002 / §7: fmt 意味破壊 5 クラス + 検出器の恒久化)。
  「コーパスが踏まない形状にバグは住み続ける」仮説の実証。
