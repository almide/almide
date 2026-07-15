<!-- description: Post-0.29.0 improvement sweep — every open gap issue-ized and ordered, from face-fixes to the v0 wasm retirement -->
# Post-0.29 Improvement Sweep

0.29.0（verified-default 化）出荷直後の全方位棚卸し。改善候補を全て issue 化し、
優先度順に潰す。各項目の詳細・受入基準は issue 側が正、本書は順序と現在地の台帳。

## すぐ直す（顔の修繕・衛生）

- [x] [#778](https://github.com/almide/almide/issues/778) README Project Status 表の陳腐化＋自己矛盾（Tests 240/232→285、MSR 23/25 vs 30/30、verified-default 未記載）
- [x] [#779](https://github.com/almide/almide/issues/779) `almide.lock` の裁定（commit or gitignore、org 横断で統一）
- [x] [#780](https://github.com/almide/almide/issues/780) org-trust-status の headline を resolved-strict sweep へ切替 — 正直な初回計測: 13/28 repos clean、24 modules wall、11 frontend-rejected（#783 クラス）。porta/almai/toml の新規 real wall が次の lowering ターゲット
- [x] [#741](https://github.com/almide/almide/issues/741) `math.tanh` / `math.atan` 追加 — vendored libm 転写を native/v0-wasm/v1-self-host の3経路に実装、C-134 + 3000点差分 fuzz。`0.0 - t` が IEEE 負零を失う罠も発見・修正（符号ビット bxor）

## 正しさ（native 側の非対称を塞ぐ）

- [x] [#757](https://github.com/almide/almide/issues/757) nested variant-tag panic — 根治: #610 box 書き換えの `matches!` guard が非 boxed の inner tag/リテラルを `_` に消していた。guard_shape 再帰で全 refutable 制約を保持。C-070 拡張 + `nested_variant_tag_box.almd`（全量ゲート実行中）
- [x] [#753](https://github.com/almide/almide/issues/753) debug-profile ANF trap — 現 develop で再現せず（0.29 サイクルの lowering 修正で解消）。両 fixture を debug バイナリで検証しクローズ。debug-only の postcondition 実行は設計どおり（pass.rs に文書化済み）
- [x] [#783](https://github.com/almide/almide/issues/783) name-pinning 再発 — 根治: repair が `map_children` ベースで ForIn/While body（`Vec<IrStmt>`）内の Bind ty を素通ししていた。canonical `IrMutVisitor` に書き換えて checker と同じ走査族に統一（enumeration drift クラスごと解消）。gate の where_ に位置粒度も追加
- [x] [#784](https://github.com/almide/almide/issues/784) 匿名 record フィールドの Unknown — 真因は**無注釈の負リテラル module 定数**のシードが Unknown（`infer_literal_type` に Unary 枝が無い）。修正で ceangal suite がコンパイル通過（残りは #433 系 cell と個別テスト失敗）。回帰: cross_module_let_test に2本追加
- [ ] [#785](https://github.com/almide/almide/issues/785) 呼び出し初期化の module 定数も Unknown leak — refresh 経路（check_decl の top_lets 再登録）が読者を救えていない。ここが本丸の契約

## 戦略級（次の大玉）

- [ ] [#782](https://github.com/almide/almide/issues/782) **Phase 3: v0 wasm emitter 退役** — org wall 0 で前提充足。残: fallback を強いる frontend バグ（#783 ほか）、almide test の wasm 経路（native fallback 14 files）、oracle 役の後継決定
- [ ] [#764](https://github.com/almide/almide/issues/764) native trust-spine 残り rung（records/variants/closures/Float）→ default flip でツイート100点化完結
- [ ] [#617](https://github.com/almide/almide/issues/617) Matrix/Bytes の RcCow 化（値セマンティクス維持でディープクローン税を消す）

## 品質基盤（コツコツ級）

- [ ] [#566](https://github.com/almide/almide/issues/566) コンパイラ構造カバレッジの ratchet 化（現状 line 65.89%、最低 control_p5 43%。F2 所見の残り）
- [ ] [#781](https://github.com/almide/almide/issues/781) cognitive complexity >100 の関数 15本 burndown（F3/#777 と相互補強）

## 発信（ユーザー手番）

- 確定発信文（#773 クローズ済み、`docs/roadmap/done/claim-wording-perceus-byte-identity.md` に全文）を 0.29.0 発表と合わせて投稿

## 関連

- flight ladder は別建て: [#586](https://github.com/almide/almide/issues/586)（#776 リファレンスアプリ / #777 lowering 信頼基底縮小 / #569 WCET 更新済み）
- [v1-release-path](v1-release-path.md) / [v1-org-byte-verification](v1-org-byte-verification.md) / [code-health-codopsy](code-health-codopsy.md)
