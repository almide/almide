原文の説明メタデータ：! 演算子を単一の脱糖にし、位置ごとの return-op 群を削除する
<a id="return-op-eradication-one-desugar-for--position-zoo-deleted"></a>

# Return-op の根絶：`!` の単一脱糖、位置別の群を削除

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](return-op-eradication.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](return-op-eradication.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](return-op-eradication.zh-CN.md)

> 原文コミット固定の非規範閲覧用スナップショット。原文を正本とし、自動同期しない。独立した実行、実装検証、証明検査、測定は行っていない。歴史的コマンドや設計指示は資料であり、実行した指示ではない。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/return-op-eradication.md)

> 原文注記：題名や根絶目標は R2/R3 完了を意味せず、冒頭はプローブ制御の脱糖が残ることと8月の測定を明示する。後の lifted-carrier 作業により歴史的 blocker の状況は変わる。見出しは5コンパイラ調査と呼ぶが6つを列挙し、52ファイルの理由件数の合計は51、42ファイルの表は後のスナップショットである。整合化せず保持する。初期 ret event 案と着地済み x/check_xc 表記は併存し、同等性や現在の普遍的安全性を推論しない。調査、verifier/kernel 主張、trap ゼロ件数を独立再実行していない。旧 v0 指示は現在の出荷経路の定義ではない。

<a id="probe-status-clarification"></a>

## プローブ状態の明確化

以下の R2 実測マトリクスは8月の活動のスナップショットであり、現在の全コーパス件数ではない。障害 #1437 には、その後 [lifted-carrier-abi.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/lifted-carrier-abi.md) に記述する L1 プローブ限定 wrapping と heap-family の受理が加わった。R2/R3 は完了と宣言していない。`bang_return_probe` とプローブで制御する位置別脱糖は MIR lowering に残る。v0 保持の指示は歴史的な活動を指し、現在出荷する Wasm 経路は structural（[wasm_route.rs](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/src/wasm_route.rs)）。

状態：R2 進行中（批准 ○ 2026-08-15。R1 は3358f26f6+53439f582で着地 — 両 renderer の op、verifier の分岐離脱規則、cert marker `x`、kernel format v5 の `check_xc` がすべて green。プローブ `ALMIDE_BANG_RETURN=1` が9つの位置行を制御、2e73deceb）。

R2 集計（プローブ ON、全 almd スイート、baseline 391/12/0）：
- 初期：335/64/**4 failed** — この4件は #1434 の v0 退行（Err-turbofish、52326a54eで修正）であり、規則のシグナルではない。
- clean（規則の増分1-2有効：スカラー bind + stmt 位置）：**339/64/0** — 全プローブ decline は安全にフォールバック。マトリクスは baseline 12件以外の52 fallback file（以下の `scratchpad matrix52` スナップショットは wall sweep 内にあり永続しない。ALMIDE_FALLBACK_NAMES=1で再生成）。

増分 H（3b6ceb7f8）：HeapOk×HeapOk を受理 — 同 family carrier の直接 return（tag@16）、heap payload = LoadHandle@12 + Dup + 型付き seeding（String / 平坦な heap-elem list / 追跡 variant）。Dup 後に carrier を遅延 drop する形は検証済みの定型（Lean の `oproj` 規則：親の release より必ず先に inc。ExplicitRC.lean:575-578 + 421-444 の inc-before-dec 法則）。H 後の関数レベル decline 内訳（build 到達可能）：**20 family-mismatch**（ResErrStr の REBOX 部分 — err @12 を抽出、Dup、carrier を drop、関数 family の err を構築、Return）、10 fn-family（test-block の Unit 関数 — Return ではなく ABORT の形）、3 callee-family（Option `!`）、2 heap-payload-class（record/Value）。

hoist 受理表（Lean Do/Basic.lean:805-909、移植設計）：if/while 条件、match の対象、binder なし let RHS、任意の文の部分項から HOIST。左から右、最内から外、band-allocator 一時値。lambda 本体、match arm 本体、binder を持つ let RHS、loop 本体（新しい文スコープ）、`&&`/`||` の右オペランドからは HARD-ERROR（黙って skip しない）。Lean の文書化された1つの難点は、汎用下降が短絡を黙って先行評価へ変えること。こちらは代わりに禁止する。

<a id="r2-state-after-the-second-increment-batch-209def816"></a>

### 第2増分バッチ後の R2 状態（209def816）

プローブ ON **349/54/0、trap ゼロ**、OFF **391/12/0**（baseline と完全一致）。着地した部分：cross-family rebox（20件の family-mismatch decline をすべて解消）、tail + bare-root hoist、Unit 関数の ABORT 終了（Return でなく die — Zig の `DefersToEmit` mode-flag の形）、lambda-root hoist guard、プローブを文の継続の書換えだけに限定。

プローブが表面化させた3つの実バグ（すべて既存で、プローブの副産物ではない）：
1. 末尾位置の入れ子 `!` がタグを見ずに payload を読んだ（hoist 拡張で修正 — プローブ OFF でも働く）。
2. **#1437** — lift した heap-ret carrier：`result_family` は HeapOk（tag@16）と型付けするが、lift は len-as-tag を実体化する。さらに第2段階の調査では、lift の ABI 判断自体が試験的 lowering に由来し、どの関数が Result ABI を持つかが伝播機構の現在の受理集合に依存する。lift した heap-ret 関数を decline して封じ込める。**残る42 matrix file の30件がこのクラスの背後にある**ため、#1437 が R2 のクリティカルパス。
3. void-abort 判定が channel なしの全関数で働き、外側の fallible lambda に属する `!` を持つタプル返却の pure 関数まで含んだ（fallible_lambda L1）。宣言上 `Unit` を返す場合に限定。

残るマトリクス（42）：#1437 の背後が30、callee-family が3（Option `!` — Option carrier は伝播する err channel を持たず、none⇒err(msg) 構築が必要）、heap-payload-class が1（record/Value payload）、残りは test-block 関数内だけで壁になる。

52件の壁理由分布（各 file の最初の壁）：
- 13× bind-rule decline（`unwrap ! bound to a let/var`）— 増分1の gate：heap payload / 非 Scalar の callee family / auto-wrap 関数（受理は着地）/ err 不一致。次：gate ごとに分割（ALMIDE_DBG_BANG）して heap payload を開く。
- 8× `scalar binding outside the value subset` — bind 値式内の入れ子 `!`：汎用 HOIST のクラス。
- 7× `effect-unwrap desugar's reach` + 1× call-argument `!` + 1× loop var — hoist + loop のクラス。
- 3× heap-result `if` + 1× `match` + 1× Tuple return — 下流の形。規則が err 経路を担えば解消しそう。
- 16× TEST-block 関数内だけの壁（`almide build` では出ない。codec 系列）。優先順位付けでは file ごとの `almide test` で捕捉。
法則：rot-eradication law 6 — 離脱する arm は合流継続を捨てる。
先例：`??` の経路群削除（#1418、+148/−1054）。同じ病の1段上。そちらは fixture ごとに recognizer の場合分けが増え、こちらは構文位置ごとに脱糖表の行が増える。

<a id="the-disease"></a>

## 病因

`!`（effect-unwrap-propagate）は**構文位置ごとに**脱糖される。`BRANCH_PASSES` 行表（`desugar_branch.rs`）の約9 pass と、`desugar_unwrap.rs` の stmt/tail の対：

| 位置 | pass | 支払う代価 |
|---|---|---|
| stmt / let-bind | `desugar_effect_unwrap` | 継続を ok-arm に入れ子にする |
| tail | `desugar_tail_effect_unwrap` | pass-through 特例 |
| call-arg | `desugar_callarg_unwrap` | 場当たり的 ANF hoist |
| if-arm | `desugar_if_arm_unwrap` | 専用走査 |
| `ok(e!)` | `desugar_unwrap_rewrap_identity` | 恒等変換の縮約 |
| let continuation | `desugar_let_unwrap` | 専用走査 |
| expr-nested scalar | `desugar_stmt_value_nested_unwrap`（#1183） | 第2の場当たり的 hoist |
| for-loop body | `desugar_loop_unwrap` | loop が運ぶ**エラーフラグ** + loop 後の分岐 |
| unit if/match stmt + cont | `desugar_stmt_control_unwrap` | 全 arm への**末尾複製** |

根本原因は `desugar_unwrap.rs` 自身のヘッダにある：*v1 MIR には関数途中の早期 return の Op がない*。従って各位置が別々に継続を再構成する。fixture が壁に当たるたびに行を足してきた — `??` 経路群と同じ増殖パターン。専用脱糖 file だけで約2,800行。`Unwrap`/`Try` 処理は30+の lowering file に散在。

<a id="research-verdict-5-compiler-survey-sealed-out-of-repo-2026-08-15"></a>

## 調査の結論（5コンパイラ調査、repo 外で封印、2026-08-15）

調査対象：rustc、Zig、Roc、Lean 4、Koka、Grain。核心は全員一致：

1. **単一の脱糖箇所、位置非依存** — exit を第一級の終端（noreturn）式にすることで可能になる。値位置では lowering が「何も代入せず、以後は到達不能」とする（rustc `lower_expr_try` が唯一の `?` 箇所。Zig `tryExpr` は error 本体が `ret_node` で終わる block を出力。Roc は `?` を一度だけ `match + e_return` に脱糖）。
2. **exit 経路の後始末は導出し、糖衣ごとに手書きしない** — exit 箇所で live/scope 状態を辿る。rustc：共有 DropTree に至る7行の scope walk。Zig：`genDefers(inner→outer, mode)` — exit の*行き先*が走査の外側境界。Grain（RC + 構造化 wasm、こちらと同じ profile）：`return` = *返却値を除く*全 live managed binding の decref を return 前に出力。`break`/`continue` = loop レベルの binding のみ、live map は変更しない。
3. **末尾複製 / エラーフラグは primitive 欠如の税。** 参照先が継続を複製する場合も*閾値付き最適化*（Koka：non-return branch ≤1 の場合のみ。Lean：通常 exit >1 の場合のみ join）であって、基本戦略ではない。
4. **新 op を追加しない道は実証上行き止まり。** Lean は「sum-type result + single dispatch」を約5年出荷した後、置換した。variant 爆発（結果型 × exit 種別の組合せ）、各 exit で変更変数の tuple を受け渡す必要、コード膨張は未解決、結局内部で join point を再発明した。
5. **`!` では block 対象より frame 対象。** Zig の `ret_node` は関数 frame を指定するため、どんな深さでも外側 block の協力は不要で、error edge は noreturn なので merge/phi 調整もない。完全な join point（Roc/Lean）は一般機構だが、scope 不変条件と ownership 契約に実際の費用がかかる。error edge が常に frame を指す `!` には過剰。後続に保留（非目標参照）。

<a id="the-cure-opreturn-one-hoist-one-rule"></a>

## 治療：`Op::Return`、単一 hoist、単一規則

<a id="new-op"></a>

### 新 op

```rust
/// Frame-targeted early exit: return `val` (None = Unit) to the caller.
/// TERMINAL — nothing may follow it in the enclosing arm (mir_wellformed).
Return { val: Option<ValueId> }
```

コードコメント訳：frame を対象とする早期終了。val（None = Unit）を呼出し元へ返す。終端なので外側 arm 内で後続は禁止（mir_wellformed）。

Render：wat `return`（任意の block 深さで有効）、native `return v;`。両 target に命令は既にあり、render コストはゼロ。

<a id="lowering-rule-the-one--rule"></a>

### Lowering 規則（唯一の `!` 規則）

以下の hoist 後は、すべての `!` が let/stmt 位置にある：

```
let x = f()!   ⇒   v = lower(f())
                   IfThen(err_tag(v)) {
                     drops(live_heap_handles ∖ {err_carrier})   // derived, not authored
                     Return(err_carrier)
                   } EndIf
                   x = ok_payload(v)                            // straight-line, no nesting
```

コードコメント訳：drop は手書きでなく導出。ok payload 後は直線的で入れ子にしない。

`live_heap_handles` は lowerer に既存 — 参照先が drop を導出する scope walk そのものである。継続は線形のまま：入れ子 match、末尾複製、loop flag は不要。loop 本体の `!` に loop の協力は一切要らない（wat `return` はすべてを抜け、drop は loop の live 状態を覆う）。

<a id="the-hoist-kills-the-position-dimension"></a>

### Hoist（位置の次元を消す）

単一の汎用 ANF 走査が `desugar_callarg_unwrap` + `desugar_stmt_value_nested_unwrap` + if-arm/let 変種を置換する。式内に入れ子の全 `Unwrap` を独立した bind-position stmt に引き上げる。位置知識は pass でなく小さい表（禁止 binder、区切り種別）になる。

<a id="verifier-the-part-only-we-must-prove"></a>

### Verifier（こちらだけが証明すべき部分）

- `lib_c.rs` branch frame に `diverged: bool` を追加。`Return` で終わる arm は (a) `check_branch_agreement` に参加せず、join は他方 arm の状態だけから続く。(b) `Return` で独立に frame-exit 義務を満たす必要がある：全所有 object が rc 0、ただし返却値は呼出し元へ move（`m`）として計上。
- 証明書：`Return` は stream segment を閉じる `ret` event を出力。incs-before-decs 規律は不変（drop は `Return` に先行）。
- Rocq kernel（`OwnershipChecker.v`）：one-shot-branch の受理に対応規則を追加。実行が `ret` で終わる arm は障害なく exit 義務に到達した場合のみ受理。agreement が必要なのは非離脱 arm 間だけ。witness corpus に専用 fixture を追加（both-arms-ret、ret-with-live-handle ⇒ reject、ret-in-loop-body）。

<a id="stages"></a>

## 段階

- **R1 — op を端から端まで、未使用で。** `Op::Return` + `mir_wellformed`（終端：arm 内に後続なし）+ 両 renderer + certificate event + verifier の離脱規則 + kernel 規則 + witness fixture。gate：op を一度も出力しない状態で全試験 green、kernel が新 witness を証明書検査で受理。
- **R2 — プローブの背後の規則。** hoist + 単一 lowering 規則を `ALMIDE_BANG_RETURN=1` で制御（#1418 readiness-probe 手順）。全試験 + 3-way oracle の decline matrix を測り、law-4 修正（型付き seeding、ANF retries）でゼロにする。両方が受理する場合は旧経路とバイト A/B 比較。
- **R3 — 削除。** matrix が空 ⇒ default 切替、位置行を削除（`desugar_effect_unwrap` の入れ子 match 本体、`desugar_loop_unwrap` の error flag、`desugar_stmt_control_unwrap` の末尾複製、if-arm / let / nested / callarg 変種）。`desugar_unwrap_rewrap_identity` は最適化としてまだ有効なら残す。同じ PR で台帳まで追従：scalar-read audit、STAGE-STATUS、TCB 主張、walled-real ratchet、caps 計数（Return は call を出さず、件数不変）。
- **R4 — 後続（別 issue、本活動外）。** 同じ離脱 arm 規則で #1421 の match lowering を受理。loop 内の `guard … else <value>`（現在は正直な壁）。loop-as-join 一般化。wasm サイズが退行したら render 段階で drop-chain suffix を共有。

<a id="non-goals"></a>

## 非目標

- 本活動では一般 join point / labeled break は扱わない。`!` の error edge は常に frame 対象。一般機構にはまだ不要な scope 不変条件と join 対応 ownership 契約の費用がある。将来の道は残す（R4）。
- v0 は変更しない。v0 側は既に `!` を Rust `?` へ lower する。op を加えるのは v1 MIR 側のみ。3-way oracle が両側を検査する。

<a id="exit-criteria"></a>

## 完了条件

1. `BRANCH_PASSES` の `!` 専用行がゼロ。`desugar_unwrap*.rs` を hoist + 恒等変換（残す場合）に縮小。
2. 新しい `!` 位置（将来構文）で新しい脱糖コードはゼロ — hoist + 単一規則が吸収。（rustc の例：`do yeet` は `?` の機構を無償で再利用。）
3. Kernel 証明書検査：witness corpus は ret 離脱 arm の受理例と拒否例を覆い、verify-trust green。
4. 削除効果：`lower/` 全体で正味 ≥1,500 行減。
