<!-- description: Post-0.29.0 improvement sweep — every open gap issue-ized and ordered, from face-fixes to the v0 wasm retirement -->
# Post-0.29 Improvement Sweep

0.29.0（verified-default 化）出荷直後の全方位棚卸し。改善候補を全て issue 化し、
優先度順に潰す。各項目の詳細・受入基準は issue 側が正、本書は順序と現在地の台帳。

## すぐ直す（顔の修繕・衛生）

- [ ] [#778](https://github.com/almide/almide/issues/778) README Project Status 表の陳腐化＋自己矛盾（Tests 240/232→285、MSR 23/25 vs 30/30、verified-default 未記載）
- [ ] [#779](https://github.com/almide/almide/issues/779) `almide.lock` の裁定（commit or gitignore、org 横断で統一）
- [ ] [#780](https://github.com/almide/almide/issues/780) org-trust-status の headline を resolved-strict sweep へ切替
- [ ] [#741](https://github.com/almide/almide/issues/741) `math.tanh` / `math.atan` 追加（`tanh`→`tan` の誤誘導 did-you-mean も是正）

## 正しさ（native 側の非対称を塞ぐ）

- [ ] [#757](https://github.com/almide/almide/issues/757) nested variant-tag パターンが native で inner ctor check を落とし `unreachable!()` panic（wasm は正しい）— verified が wasm 側だけの今、native の正しさは v0 rust codegen に無防備依存
- [ ] [#753](https://github.com/almide/almide/issues/753) debug-profile ANF postcondition trap（heap-typed call arg が未 lift、release は正常）
- [ ] [#783](https://github.com/almide/almide/issues/783) name-pinning postcondition が ceangal の実モジュールグラフで再発（#433 クラス、最小プローブでは再現せず）→ 構造的な根治は [#528](https://github.com/almide/almide/issues/528) QualifiedRef newtype

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
