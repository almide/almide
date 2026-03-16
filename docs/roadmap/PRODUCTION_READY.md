# Production Ready Criteria

> **1.0 = 安定性契約。** 既存コードが壊れないことの約束。機能チェックリストではない。
>
> — Ruby, Rust, TypeScript の 1.0 全てがこの原則に従った

---

## Vision

Almide は「LLM が最も正確に書ける言語」。

単一の `.almd` ソースから Rust, TypeScript, WASM に出力し、async/await を書かずに `fan` で並行処理ができ、`effect fn` の Effect Isolation で supply chain attack を構造的に防ぐ。

LLM が犯す典型的なミス — await 忘れ (Python asyncio)、型の不一致 (JavaScript)、可変状態の競合 (Ruby Ractor)、Pin/Unpin 地獄 (Rust async) — を文法レベルで構造的に不可能にする。

---

## 現在地: v0.6.0

```
コンパイラ          84 ファイル / 19,536 行
                    生成コードは外部 crate 不要（stdlib ランタイムを自己内包）
stdlib             22 モジュール / 355 関数 / ランタイム 100%
テスト             2,033+ (96 .almd ファイル + 714 Rust unit tests)
ターゲット          Rust, TypeScript, JavaScript, npm package, WASM
Exercises          25 本 / 6 tiers
並行処理           fan { }, fan.map, fan.race, fan.any, fan.settle, fan.timeout
セキュリティ       Effect Isolation (Layer 1) — pure fn は effect fn を呼べない
Codec              auto-derive encode/decode, Value 型, JSON roundtrip
IR                 Typed IR + constant folding, dead code elimination
Borrow             use-count ベースの clone 挿入/削除
診断               file:line + context + actionable hint + error recovery
```

---

## v0.1.0 → v0.6.0

| 機能 | v0.1.0 | v0.6.0 |
|------|--------|--------|
| 型システム | Int, String, Bool | + Float, List, Map, Tuple, Record, Variant, Option, Result, Generics, Union |
| エラー処理 | なし | effect fn, auto-?, do block, guard |
| 並行処理 | なし | fan ファミリー 6 API |
| セキュリティ | なし | Effect Isolation |
| Codec | なし | auto-derive, Value, JSON roundtrip |
| ターゲット | Rust のみ | Rust + TS + JS + npm + WASM |
| パターンマッチ | 基本 | 網羅性チェック、ネスト、ガード |
| 診断 | 行番号のみ | file:line + context + hint + error recovery |
| テスト | 0 | 2,033+ |
| stdlib | 数関数 | 22 モジュール / 355 関数 |
| ツール | `almide run` のみ | run, build, test, check, fmt, clean, init |

---

## 1.0 の定義

> TypeScript 1.0 は strictNullChecks なしで出荷された。Rust 1.0 は async なしで出荷された。
> どちらも「既存コードは壊れない」という約束だけで 1.0 を名乗った。

### Almide 1.0 が約束すること

1. **構文凍結**: `effect fn`, `fan`, `do`, `guard`, `match`, `for...in` — 現在の構文は永続
2. **コア stdlib API 凍結**: 22 モジュール / 355 関数のシグネチャは不変。関数の追加はするが、既存の変更はしない
3. **クロスターゲット一致**: 同じ `.almd` が Rust と TS で同じ出力を生む
4. **edition フィールド**: `almide.toml` に `edition = "2026"` を追加。将来の破壊的変更は新 edition で吸収

### Almide 1.0 に入れないもの

> Rust は async (4.5年後), const generics (6年後), GATs (6.5年後) を延期した。
> TypeScript は strictNullChecks (2年後), conditional types (4年後) を延期した。
> どちらもエコシステムは繁栄した。

| 延期する機能 | 理由 | 予定 |
|-------------|------|------|
| LSP | `almide check` + hint が LLM の主要 UX。人間向け IDE 統合は後 | 1.x |
| FFI / Rainbow Bridge | stdlib でCLI ツール・Web API は書ける | 1.x |
| パッケージレジストリ | git ベース依存で十分。レジストリは臨界質量が必要 | 1.x |
| Go / Python / C ターゲット | Rust + TS でユースケースの 90% をカバー | 2.x |
| Security Layer 2-5 | Layer 1 (Effect Isolation) だけで十分な差別化 | 2.x |
| Self-Hosting | 信頼性の証明にはなるが、ユーザー価値は薄い | 2.x+ |
| 38+ モジュール / 700+ 関数 | 1.x で段階的に追加。22 / 355 で実用的 | 1.x incremental |
| MSR 85%+ | 計測し報告するが、リリースをブロックしない | 計測開始 |

---

## 1.0 チェックリスト

```
Almide 1.0 = ALL of:

安定性契約
  □ 構文凍結: 全キーワード・構文の最終確認
  □ stdlib API 凍結: 22 モジュール / 355 関数のシグネチャ固定
  □ edition フィールド: almide.toml に edition = "2026"
  □ 破壊的変更ポリシー: post-1.0 は compile error + migration hint のみ
                        silent な挙動変更は禁止 (Python 2→3 の教訓)

コンパイラ正確性
  □ クロスターゲット CI: 全テストを Rust + TS で実行、出力 diff = 0
  □ ICE = 0: panic/unwrap ゼロ
  ■ 生成コードが rustc/tsc 通過
  ■ stdlib ランタイム 100%

テスト
  □ テスト 2,500+                        (2,033 — あと 467)
  □ 5 つの showcase プログラムが両ターゲットで動作

パッケージ管理
  □ almide.lock で再現性保証
  □ almide.toml の [dependencies] + git ベース解決

エラー品質
  □ 安定したエラーコード (E0001-E9999)
  □ almide check --json: LLM agent 向け構造化出力
  □ hint 適用で修復できる率 70%+ (計測)

LLM 計測 (ブロックしないが計測必須)
  □ MSR 計測開始 (Grammar Lab)
  □ 初回正答率ベンチマーク (exercises ベース)

■ = 達成 (2)   □ = 未達 (12)
```

---

## 1.0 前にやるべき破壊的変更

> Pre-1.0 は破壊的変更ができる唯一の窓。LLM が構文を学習した後では指数関数的に難しくなる。
> — TypeScript が enum と namespace を後悔しているように

- [ ] Verb system reform 完了 (stdlib-verb-system.md)
- [ ] コア型 API (String, List, Map, Result, Option) の表面積を監査・凍結
- [ ] `fan` の命名最終確認
- [ ] `effect fn` マーカーの最終確認
- [ ] Rejected Patterns リスト作成（再提案を防ぐ）

---

## 1.0 への道

### Phase I: 正確性 + クロスターゲット CI

> TypeScript: "ターゲット選択がプログラムの挙動を変えてはならない" — これが最重要の品質ゲート

- クロスターゲット CI 構築（全テストを Rust + TS で実行、出力 diff）
- Borrow/Clone gaps の修正
- Unknown 伝播 hardening
- ICE ゼロ化

### Phase II: 安定性契約の準備

- Verb system reform 完了（1.0 前の最後の破壊的変更）
- コア型 API 凍結
- edition フィールド追加
- 安定エラーコード (E0001-)
- `almide check --json` 実装

### Phase III: パッケージ管理 + テスト拡充

> Cargo は Rust 1.0 の 6 ヶ月前に登場し、Rust の最大の競争優位になった

- `almide.lock` 実装
- テスト +467 → 2,500+
- 5 showcase プログラム定義・検証
- `almide test --json` 実装

### Phase IV: LLM 計測 → 1.0 リリース

- Grammar Lab で MSR 計測
- exercises ベースの初回正答率
- hint 修復率ベンチマーク
- 計測結果を公開 → **1.0 リリース**

---

## 1.x: エコシステム拡張

> Ruby: RubyGems (2004) → Rails (2005)。パッケージ基盤がキラーアプリに先行した

- stdlib 段階的拡充: csv, toml, url, html, set, sorted (first-party package として)
- LSP (diagnostics → hover → go-to-def)
- FFI / Rainbow Bridge
- `almide doc` 生成
- `assert_snapshot` + `almide test --update` (MoonBit の教訓)
- `almide run` キャッシュ（生成 .rs が同一なら rustc スキップ）

---

## Beyond 1.x

| 項目 | roadmap | 先例 |
|------|---------|------|
| Go / Python codegen | on-hold/new-codegen-targets.md | Rust: 1.0 は 1 ターゲット |
| Almide Shell | on-hold/almide-shell.md | Ruby: IRB は初期から |
| Self-Hosting | on-hold/self-hosting.md | MoonBit: ブートストラップ済み |
| Security Layer 2-5 | active/security-model.md | — |
| Async Backend | on-hold/async-backend.md | Rust: 4.5 年後 |
| Streaming | on-hold/streaming.md | — |
| LLM → IR 直接生成 | on-hold/llm-ir-generation.md | MoonBit: constrained sampler |
| Web Framework | on-hold/web-framework.md | Ruby: Rails が言語を定義した |
| Almide UI | on-hold/almide-ui.md | — |
| パッケージレジストリ | on-hold/package-registry.md | Python: PyPI は 23 年かかった |
| Typed errors | — | MoonBit: `T!ErrorType` |

---

## 他言語からの教訓 (詳細)

| 教訓 | 出典 | Almide への反映 |
|------|------|----------------|
| 1.0 = 安定性契約 | Rust, TypeScript | 構文凍結 + stdlib API 凍結 |
| 破壊的変更は compile error で、silent な挙動変更は禁止 | Python 2→3 | 破壊的変更ポリシー |
| stdlib は lean core + packageable extensions | Python dead batteries, Ruby gemification | 22 built-in + first-party packages |
| パッケージ管理はキラーアプリより先 | Ruby (RubyGems → Rails), Rust (Cargo → ecosystem) | almide.lock を 1.0 に |
| edition で将来の進化を保証 | Rust editions (2015/2018/2021/2024) | edition フィールド |
| fan は「async 未実装」ではなく「完成した並行モデル」 | Rust async の苦しみ, Python asyncio の分断 | fan を complete として文書化 |
| エラーメッセージは製品 | Rust RFC 1644, TypeScript stable codes | 安定エラーコード + JSON 出力 |
| Rejected Patterns リストで feature creep を防ぐ | Ruby (Perl 由来の後悔), Python PEP | 明示的な拒否リスト |
| dev-loop 速度 > build 速度 | Ruby (15 年遅かったが production で稼働) | `almide run` < 2 秒 |
| 定期リリースでアップグレードの信頼を構築 | Ruby (年次), Rust (6 週), TypeScript (3 ヶ月) | 1.0 後に月次/隔月リリース |

詳細: [docs/research/lang-lessons-*.md](../research/)
