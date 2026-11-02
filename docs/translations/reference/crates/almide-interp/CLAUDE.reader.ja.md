<a id="almide-interp"></a>

# almide-interp

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](CLAUDE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](CLAUDE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](CLAUDE.reader.zh-CN.md)

> [正本のエージェント指示](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-interp/CLAUDE.md)を 35297a612fb29e3f78f6587aba253aa91d51f466 に固定した、非活性・非規範的な読者向け翻訳です。原本 CLAUDE.md は正本のまま変更せず、別名 CLAUDE.reader は自動読込される指示ファイルではありません。引用した規則は歴史的な参照資料であり、新たな指示階層や、コマンド実行・コード変更・commit・push・公開の権限ではありません。自動同期されず、実装・coverage・独立性・安全性の主張を現行版で独立に実行・検証していません。inline code は完全に保持しています。

**codegen 前**の `IrProgram` を対象に木をたどるインタープリタ — クロスターゲット oracle の第三レッグであり、実行可能な仕様です。

<a id="why-it-exists"></a>

## 存在する理由

クロスターゲットゲート（`spec/wasm_cross/*.almd`、`tests/wasm_runtime_cross_target.rs::wasm_cross_target_spec` が強制）は **native** と **WASM** のバックエンドを比較し、`(stdout, stderr, exit)` のバイト一致を要求します。この二者投票は、*両方が同じように間違う*バグに構造的に盲目です。codegen と WASM emitter が共有する lowering pass が誤っていれば、挙動が誤っていても両者は一致し、ゲートは緑のままです。

この crate は第三の独立した判定者を加えます。`lower → optimize → mono → ir_link` の**後**、ただし `almide-codegen` の target-lowering pass（`ClosureConversion` / `Perceus` / `StdlibLowering` / `IterChain` / …）のいずれよりも**前**の切断点で IR を評価します。そのため、どちらのバックエンドとも codegen のターゲットパスを*まったく*共有しません。codegen が挿入する約 22 の `IrExprKind` variant はここでは到達不能で、`eval.rs` は境界を守るため `unreachable!` として assert します。

`tests/wasm_runtime_interp_oracle.rs::interp_cross_target_spec` が三者 harness です（root-package binary。コーパスのソース `tests/wasm_runtime_test_parts/corpus.rs` を equivalence・wasm-opt ゲートと共有し、各 binary が独自の fixture 表を構築します）。
- `interp == native == wasm` → codegen バグを共有し得ない仕様による裏付け。
- `native != wasm` → **二者の `@xt-allow` ゲートが担当する**バックエンドの分裂。この harness は interp がどちらに付くかを記録するだけ（同点決着の診断）で、失敗にはしません。
- `native == wasm` だが interp は異なる → **明確な失敗**（`BOTH-BACKENDS-WRONG` banner）。診断してください。インタープリタを修正すべきか、二者ゲートには見えない両バックエンドのバグを発見したかのいずれかです。

<a id="module-map"></a>

## モジュール対応表

- `value.rs` — 動的な `Value` モデルと 2 つの表示モード（`display_bare` = `println`/Display、`almide_repr` = 複合・コンテナ形式）。両者は `almide_repr_prelude`（`crates/almide-codegen/src/lib.rs`）を再現し、native とバイト一致します。
- `env.rs` — `VarId` をキーとする、`Rc` 共有の frame。native の `RcCow` capture 意味論を再現します。
- `eval.rs` — 評価可能な全 IR node の tree-walker、fuel 計上、pattern engine（切断点後も残る list pattern を含む）、レコード repr の nominal name 復元。
- `dispatch.rs` — `Call` の routing（`Named` / `Module` / `Method` / `Computed`）、variant-ctor registry、**HOF allowlist**（`is_hof`）。
- `hofs.rs` — interp 内の HOF（map/filter/fold/…）、interp-native の container op、in-place mutation の述語。
- `inplace.rs` — `mut` receiver の mutator（`list.push`、`set.insert`、…）。receiver の束縛に対する読み取り → 変換 → 書き戻しとしてモデル化します。束縛自身の格納 slot に対する `Rc::make_mut` が COW 規則そのものです。唯一の owner は in-place に変更（push ループは線形のまま）、alias があるとコピーを強制します（C-033）。
- `bridge.rs` — `prim.*` の scalar floor、pool 層の内部で self-hosted body が基盤として使う scalar/string arm、境界での 2 種の fallback（固定幅数値の `to_string` observer、`bytes.zeros`）。各 arm はコーパスが到達するもので、fallback 台帳でゲート検査します。
- `surface.rs` — 同梱モジュールに書かれた公開 fn（registry source ではないため pool に本体はありません）。registry body に転送する薄い wrapper（`int.pow` → `math.__pow`）、再記述した本体（`int.parse_radix`、`bytes.get_*`/`next_*`、`list.binary_search`、`deque.*`）、引数の静的型で `Numeric` pivot を選ぶ generic な `<T>.from` 系列、そしてバックエンドが lower する `matrix.*` 呼び出しとしての Matrix 演算子。
- `stdlib_pool.rs` — self-hosted な stdlib 本体（wasm レッグがリンクするものと同じソース）を 1 回 lower します。container 層より下では、これらが投票を担います（#2185）。

<a id="coverage-model--does-a-new-stdlib-fn-get-covered-automatically"></a>

## カバレッジモデル — 新しい stdlib fn は自動的に対象になるか

**scalar/heap prim floor 上で self-hosted なら、なります。interp は登録された本体そのものを評価します。** dispatch の順序は次のとおりです。

1. `dispatch.rs::is_hof` — クロージャを取る combinator（allowlist）。
2. `hofs.rs::eval_container_op` — 非 HOF の構造的 container op（ここでは container は線形メモリでなく `Value` なので、この層は interp-native のまま）。
3. `dispatch_module.rs::prim_floor` — 実行ごとのインタープリタ状態を保持する prim（block heap、argv/env、sandbox 化された fs、`prim.die`）。
4. **lower 済みの self-hosted body**（`stdlib_pool.rs` — registry source、両バックエンドが実行するものと同じ定義）— pool 境界（`pool_depth == 0`、fixture code が stdlib を呼ぶ位置）で使用します。本体自身の helper call は層の内部にあり、bridge を先に使います。そこでは bridge は `prim.*` と同様に本体が使う基盤です（block heap 上に第二の本体を入れ子にすることは、この層がまだ忠実にできないことです。最初の body-first 測定は 9 fixture で誤投票し、2 fixture でクラッシュしました）。
5. `bridge.rs::dispatch` — 境界では、手順 4 に本体がない、呼び出しの arity を受けられない、または辞退した場合だけ使用します。固定幅数値の `to_string` observer と `bytes.zeros` です。#2185 以降、いずれの層でも bridge が応答する非 `prim` 名はすべて `RunOutcome::bridge_fallbacks` に記録され（floor または fallback のタグ付き）、`tests/wasm_runtime_interp_ledger.rs::interp_bridge_fallback_ledger` が `interp-bridge-fallback-ledger.txt` との一致を両方向で維持します。本体の代わりに Rust のコピーが投票することはレビューを受けた台帳編集であり、コーパスが到達しなくなった arm は削除すべき死んだ arm です。

処理されないものはすべて `Flow::Unsupported(...)` へ落ち、三者ゲートが**データに基づく、理由付き skip** として記録します（具体的理由を表示し、ハードコードした skip-list はありません）。カバレッジの縮小は無言ではありません。abstain 集合は `tests/wasm_runtime_interp_ledger.rs::interp_abstain_ledger` により、commit 済みの `interp-abstain-ledger.txt` と照合監査されます（バックエンド不要、自ら skip することはありません。その binary は interp レッグだけを含み、コーパス表は決して含みません。表に触れると native+wasm ビルドが起動し、その性質を壊すからです）。台帳項目なしで fixture が辞退すると CI は失敗します。つまり interp が実行できない fixture を持つ新しい stdlib fn は、同じ PR で interp glue またはレビュー済み台帳項目を必要とします。今や評価できる fixture の古い項目も失敗になります（台帳は縮小のみ、CG-1 gap-audit ratchet、issue #564）。本体が使える prim floor を広げることが手段です。処理できる prim を 1 つ増やすたびに skip が本物の第三票へ変わり、台帳の行が消えます。

本体自身のステップは fixture ではなく pool の予算（`POOL_FUEL`、10 プログラム分）で計量します。実行可能な仕様のコストは仕様自身のものであり、fixture の `DEFAULT_FUEL` は引き続き fixture 自身のループを制限します。ここで `almide_rt` に path 依存してはいけません。その crate は interp が必要としない rustls/webpki（network/TLS）と effect を伴う表面を取り込みます。

<a id="rules"></a>

## 規則

- **interp は抽象的に「正しい」のではなく、バックエンドと一致しなければなりません。** バックエンドが癖を共有する箇所（例えば匿名レコードのフィールドはソート順に表示、`${float}` は `.0` なしの単純な `{}` Display を使用、`0.30000000000000004` の最短 roundtrip）では interp もその癖を再現します。ここでの相違は第三票であり、誤った第三票は skip より悪いものです。
- **誤投票は正直な skip より悪いものです。** interp が忠実にモデル化できない場合（この frame から束縛へ代入できない `mut` receiver の変更 — `mut` パラメータで、その effect をバックエンドは呼び出し元の slot に返す場合（C-132、issue #1022）、または record-field/一時値 receiver。バックエンド同梱の musl-libm と最後の ULP が異なる platform-libm の超越関数。非決定的な `fan.*`）、理由付きの `Flow::Unsupported` を返し、決して推測しません。
- **系列ではなく名前で辞退します。** capability のある形をモデル化でき、別の形をできないとき、abstain はどの形が欠けているかを示さなければなりません。台帳が系列全体を諦めるのでなく、誰かが埋められる穴を記録するためです。まさにこの理由で `inplace.rs` は 3 つに分かれます。書き戻し、`mut` パラメータの receiver（#1022）、バイト単位の buffer writer（#1021）。
- **codegen 前の切断点を維持します。** codegen が挿入した IR node（`Clone`、`Borrow`、`IterChain`、`ClosureCreate`、`RuntimeCall`、…）は構成上 `unreachable!` です。到達可能になったなら切断点が移ったのであり、黙って処理せず境界を修正します。
- **skip はデータに基づき、明示します。** 各 skip はインタープリタ自身の `RunStatus::Unsupported`/`FuelExhausted`（または harness が他のレッグと対応付けられない `StackExhausted`）で、理由とともにログに残します。harness にハードコードした skip-list を追加してはいけません。
- **fuel は必須です。** 各 eval step は 1 単位（`DEFAULT_FUEL`）を消費します。深い再帰はインタープリタが宣言した C-196 のスタック閾値 `MAX_DEPTH` で制限し、`StackExhausted`（`Error: stack overflow`、exit 1 — step fuel とは別）で終了します。敵対的ループは `FuelExhausted` で終了しなければならず、ハングさせません。
- **表示契約は codegen から出典付きで導出します。** `value.rs` の表示に触れるときは、再現する `almide-codegen` / `runtime/rs` の正確な行を指します（既存コメントのように）。

## 原本に関する注意

原本の独立性の主張は codegen 前の切断点とターゲット lowering pass の非共有についてであり、独立にソースを解析する oracle を意味しません。より前の共有段階と self-hosted stdlib source の共有は明記されたままです。この harness は二者ゲートが担当する native/wasm の分裂を失敗にせず、interp の不一致もバックエンドだけでなく interp のバグを示し得ます。pool 境界の body-first dispatch は、層内部の helper call が bridge-first であることと区別します。歴史的な誤投票・クラッシュ、fallback と abstain 台帳の双方向検査、fixture/pool 別の fuel とスタック枯渇、正直な unsupported 結果を保持します。バックエンドの癖に合わせる指示は引用した原本規則であり、一致が普遍的な正しさを証明するという主張ではありません。ここでコードや台帳を変更することはありません。
