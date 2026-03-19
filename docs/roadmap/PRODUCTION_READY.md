# Production Ready Criteria

> **1.0 = 安定性契約。** 既存コードが壊れないことの約束。機能チェックリストではない。
>
> — Ruby, Rust, TypeScript, Go, Gleam の 1.0 全てがこの原則に従った
>
> **日付で切る。** Feature gate で延期し続けると Zig のように 10 年 pre-1.0 になる。

---

## Positioning

**"Better Rust for LLMs"** — Kotlin が "Better Java" で成功したように、Almide は Rust の型安全性・パフォーマンスを維持しながら、LLM が躓く複雑さ（borrow checker, async/await, Pin/Unpin, lifetime annotations）を構造的に排除する。

Almide が**既に回避した**他言語の失敗:

| 他言語の失敗 | Almide の設計 |
|-------------|--------------|
| Python asyncio: 関数カラーリング問題 | `fan` — async/await なし。コンパイラが自動挿入 |
| Rust async: Pin/Unpin, runtime 選択 | `fan` — thread backend。ユーザーに runtime を見せない |
| Go: `if err != nil` 地獄 | `effect fn` + auto-`?` + `do` block |
| Go: nil panic | `Option[T]` — null は存在しない |
| Go: sum type 不在 | `type | Variant` + exhaustive `match` |
| Ruby: 可変デフォルト + monkey patching | immutable values, no metaprogramming |
| Python 2→3: str の意味変更 | String は常に UTF-8。core type の意味は変えない |
| Swift: 3 バージョン連続 breaking | 全 breaking change を 1.0 前に完了 |
| TypeScript: enum/namespace の後悔 | Rejected Patterns リストで再提案を防ぐ |
| Zig: feature gate で 1.0 が来ない | 日付で 1.0 を切る |

---

## 現在地: v0.8.0

```
コンパイラ          84 ファイル / 19,536 行
                    生成コードは外部 crate 不要（stdlib ランタイムを自己内包）
stdlib             22 モジュール / 355 関数 / ランタイム 100%
テスト             110/110 .almd テストファイル全通過 + 714 Rust unit tests
ターゲット          Rust, TypeScript, JavaScript, npm package, WASM
Exercises          25 本 / 6 tiers
並行処理           fan { }, fan.map, fan.race, fan.any, fan.settle, fan.timeout
セキュリティ       Layer 1 (Effect Isolation) + Layer 2 (Capability Restriction via almide.toml [permissions])
Effect推論         自動capability推論 (IO/Net/Env/Time/Rand/Fan/Log) + almide check --effects
エラー処理         Option + Result の 2 機構のみ (Swift の 3 機構の失敗を回避)
Codec              auto-derive encode/decode, Value 型, JSON roundtrip
IR                 Typed IR + constant folding, dead code elimination + 12 nanopass
最適化             Stream Fusion (map+map, filter+filter, map+fold) — 代数法則ベースの中間alloc消滅
Borrow             use-count ベースの clone 挿入/削除
診断               file:line + context + actionable hint + error recovery
Codegen            v3: TOML templates, is_rust()=0, 106/106 cross-target
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
> Gleam 1.0 は 19 stdlib モジュールで出荷された。Go 1.0 は generics なしで 12 年間繁栄した。
> 全て「既存コードは壊れない」という約束だけで 1.0 を名乗った。

### Almide 1.0 が約束すること

1. **構文凍結**: `effect fn`, `fan`, `do`, `guard`, `match`, `for...in` — 現在の構文は永続
2. **コア stdlib API 凍結**: 22 モジュール / 355 関数のシグネチャは不変。関数の追加はするが、既存の変更はしない
3. **クロスターゲット一致**: 同じ `.almd` が Rust と TS で同じ出力を生む
4. **edition フィールド**: `almide.toml` に `edition = "2026"` を追加。将来の破壊的変更は新 edition で吸収 (Rust editions の教訓)
5. **永続互換性**: 今日コンパイルできる `.almd` は永遠にコンパイルできる (Go 1 compatibility promise)

### Almide 1.0 に入れないもの

| 延期する機能 | 理由 | 先例 | 予定 |
|-------------|------|------|------|
| LSP | `almide check` + hint が LLM の主要 UX | Gleam: LSP は 1.0 前だが最小限 | 1.x |
| FFI / Rainbow Bridge | stdlib で CLI・Web API は書ける | TypeScript: .d.ts は後から | 1.x |
| パッケージレジストリ | git ベース依存で十分 | Go: module proxy は 1.13 | 1.x |
| Go / Python ターゲット | Rust + TS で 90% カバー | Kotlin MP: JVM 優先、他は後 | 2.x |
| Security Layer 2-5 | Layer 1 だけで十分な差別化 | — | 2.x |
| Self-Hosting | ユーザー価値薄い | Zig: 自前 backend は罠 | 2.x+ |
| 700+ 関数 | 355 で実用的。Gleam は 19 モジュールで 1.0 | 全言語 | 1.x |
| Algebraic effects | `effect fn` は I/O マーカーに留める | Gleam: 効果系なしで成功 | 検討しない |
| User-defined generics | 型宣言の generics は動く。関数は後 | Go: 12 年後 | 1.x |

### fan は「完成した並行モデル」

> Rust の async は 1.0 から 4.5 年かかった。Python の asyncio はエコシステムを分断した。
> Swift は 5.5 まで 7 年待った。Go の goroutine だけが初日から機能した。

Almide の `fan` は Go の goroutine と同じカテゴリ — 1.0 で完成品として出荷する。「async は未実装」ではなく「async/await という概念自体が不要な設計」。6 API (fan, map, race, any, settle, timeout) が揃っている。

---

## 1.0 チェックリスト

```
Almide 1.0 = ALL of:

安定性契約
  ■ 構文凍結: 全キーワード・構文の最終確認 (verb reform 完了)
  ■ stdlib API 凍結: FROZEN_API.md で 22 モジュール / 355 関数を文書化
  ■ edition フィールド: almide.toml に edition = "2026" 実装済み
  ■ 破壊的変更ポリシー: BREAKING_CHANGE_POLICY.md
  ■ Rejected Patterns リスト: REJECTED_PATTERNS.md (20+ 項目)

コンパイラ正確性
  ■ クロスターゲット CI: 106/106 (100%) — GitHub Actions 自動化済み
  □ ICE = 0: panic/unwrap ゼロ (継続改善)
  ■ 生成コードが rustc/tsc 通過
  ■ stdlib ランタイム 100%

ターゲット品質
  ■ Tier 1 (Rust): 110/110 テストファイル全通過、全 exercises 動作
  ■ Tier 2 (TS/JS): 106/106 pass (100%)
  ■ Tier 3 (WASM): smoke test pass (fibonacci + fizzbuzz + list.map, 305KB)

テスト
  □ テスト 2,500+                        (2,033 — あと 467)
  □ 5 showcase プログラムが Tier 1 + Tier 2 で動作

パッケージ管理
  ■ almide.lock で再現性保証 (実装済み)
  ■ almide.toml [dependencies] + git ベース解決 (実装済み)

エラー品質
  ■ 安定エラーコード: E001-E010 実装済み
  ■ almide check --json: 構造化出力 実装済み
  ■ almide check < 1 秒: 298 行で 14ms (debug) / 25ms (release)
  □ hint 適用修復率 70%+ (未計測)

LLM 計測 (ブロックしないが計測必須)
  □ MSR 計測開始 (Grammar Lab)
  □ 初回正答率ベンチマーク (exercises ベース)

■ = 達成 (15)   □ = 未達 (2+2 計測)
```

---

## 1.0 前にやるべき破壊的変更

> Pre-1.0 は破壊的変更ができる唯一の窓。
> Swift は 1→2→3 で 3 回 breaking change があり、移行コストが大きかった。TypeScript は enum の設計を振り返っている。
> LLM が構文を学習した後では指数関数的に難しくなる。

- [x] Verb system reform 完了 (stdlib-verb-system.md)
- [x] コア型 API 監査・凍結: `docs/FROZEN_API.md`
- [x] `fan` の命名最終確認 — fan { }, fan.map, fan.race, fan.any, fan.settle, fan.timeout
- [x] `effect fn` マーカーの最終確認 — Effect Isolation (Layer 1) 実装済み
- [x] Rejected Patterns リスト: `docs/REJECTED_PATTERNS.md` (20+ 項目)
- [x] Hidden operations 文書化: `docs/HIDDEN_OPERATIONS.md`

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
- `almide check` を 500 行で < 1 秒に (Go の教訓)

### Phase III: パッケージ管理 + テスト拡充

> Cargo は Rust 1.0 の 6 ヶ月前に登場。Go は GOPATH の 6 年間を後悔している。

- `almide.lock` 実装
- テスト +467 → 2,500+
- 5 showcase プログラム定義・検証
- `almide test --json` 実装

### Phase IV: LLM 計測 → 1.0 リリース

- Grammar Lab で MSR 計測
- exercises ベースの初回正答率
- hint 修復率ベンチマーク
- Rejected Patterns リスト公開
- 計測結果を公開 → **1.0 リリース**

---

## 1.x: エコシステム拡張

> Ruby: RubyGems (2004) → Rails (2005)。パッケージ基盤がキラーアプリに先行した。
> Rust: 1.0 後に 6 週間リリースで段階的に強化。

- stdlib 段階的拡充: csv, toml, url, html, set, sorted (first-party package として)
- User-defined generic functions (Go: 12 年待ったがもっと早くてよかった)
- LSP (diagnostics → hover → go-to-def)
- FFI / Rainbow Bridge
- `almide doc` 生成 (MoonBit: doc が community adoption を加速)
- `assert_snapshot` + `almide test --update` (MoonBit の教訓)
- `almide run` キャッシュ（生成 .rs が同一なら rustc スキップ — MoonBit/Zig の教訓）
- 月次/隔月リリース (Rust train model)

---

## Beyond 1.x

| 項目 | roadmap | 先例 |
|------|---------|------|
| Go / Python codegen | on-hold/new-codegen-targets.md | Kotlin MP: JVM 優先、他は段階的 |
| Almide Shell | on-hold/almide-shell.md | Ruby IRB, Gleam: tooling > marketing |
| Self-Hosting | on-hold/self-hosting.md | Zig: 自前 backend は罠。MoonBit: 成功 |
| Security Layer 2-5 | active/security-model.md | — |
| Async Backend | on-hold/async-backend.md | Rust: 4.5 年後。急ぐ必要なし |
| Streaming | on-hold/streaming.md | — |
| LLM → IR 直接生成 | on-hold/llm-ir-generation.md | MoonBit: constrained sampler |
| Web Framework | on-hold/web-framework.md | Ruby: Rails が言語を定義した |
| Almide UI | on-hold/almide-ui.md | — |
| パッケージレジストリ | on-hold/package-registry.md | Python: 23 年。Go: module proxy は 1.13 |
| Typed errors | — | MoonBit: `T!E`, Zig: error union |
| WASM playground | — | Gleam: playground が adoption を加速 |

---

## 10 言語からの教訓

| 教訓 | 出典 | Almide への反映 |
|------|------|----------------|
| 1.0 = 安定性契約 | Rust, TS, Go, Gleam | 構文凍結 + stdlib API 凍結 |
| 日付で 1.0 を切る | Zig (10 年 pre-1.0) | Feature gate で遅延しない |
| 破壊的変更は compile error のみ | Python 2→3 | Silent な挙動変更は禁止 |
| stdlib は lean core | Python, Ruby, Gleam (19 modules) | 22 built-in で十分 |
| パッケージ管理 > キラーアプリ | Ruby, Rust, Go | almide.lock を 1.0 に |
| edition で将来の進化を保証 | Rust | edition フィールド |
| fan は完成した並行モデル | Rust async, Python asyncio, Swift 5.5 | 「async 未実装」ではない |
| エラーメッセージは製品 | Rust, TS, MoonBit | エラーコード + JSON 出力 |
| check 速度が採用を決める | Go (sub-second compile) | `almide check` < 1 秒 |
| Rejected Patterns で feature creep 防止 | Ruby, Python PEP | 明示的な拒否リスト |
| ターゲット品質階層 | Swift (iOS >> server >> WASM) | Rust > TS > WASM |
| エラー機構は 2 つまで | Swift (3 つで混乱) | Option + Result のみ |
| hidden ops を文書化 | Zig (no hidden control flow) | clone, auto-?, runtime |
| 定期リリース | Rust (6 週), Ruby (年次), TS (3 ヶ月) | 月次/隔月 |
| tooling > marketing | Gleam (playground, cheatsheets) | WASM playground |

詳細: [docs/research/lang-lessons-*.md](../research/)
