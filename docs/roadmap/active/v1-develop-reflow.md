<!-- description: Strategy for flowing the develop-v1 trust spine back into develop — what moves, in what order, and what stays branch-local until its gate exists -->
# v1 → develop Reflow Strategy

> **Position**: develop-v1 に閉じている v1 成果（MIR trust spine・self-host 群・証拠ゲート群）を
> いつ・どの単位で develop へ還流するかの方針案。**実行はユーザーの Go 判断待ち** —
> 本書は判断材料の整理。

## 還流資産の棚卸し（2026-07-03 時点）

| 資産 | develop への価値 | リスク | 提案 |
|---|---|---|---|
| **証拠ゲート群**（toolchain 刻印 / LC_ALL=C + solo-retry parity / ratchet 分離 lefthook / coverage.sh / 3点比較） | v0 の検証も同じ穴を持つ（ロケール照合順・バイナリドリフトは branch 非依存） | ほぼゼロ — ゲートは実装に触れない | **第1波**: そのまま cherry-pick 可能。v0 単独でも即効 |
| **flight-evidence-gaps / TRUSTED_BASE 境界図 / roadmap 文書** | 監査台帳は全ブランチ共通の資産 | ゼロ | **第1波** |
| **v0 にも効く compiler 修正**（равi-recursive unify guard / structural-twin merge / cross-module @inline_rust — 既に v1 で検証済みの frontend/codegen 層） | v0 の実バグ修正 | 中 — v0 の full スイートでの再検証必須 | **第2波**: 1修正=1PR で develop CI に載せる |
| **MIR spine 全体**（almide-mir crate + registry self-host 群 + render_program） | v1 の中核。develop に持ち込むと「2つ目のバックエンド」として並存 | 大 — develop の CI 時間・保守面積が倍化 | **第3波**: 「v1 が v0 の親を置き換える」判断と同時。それまで branch 常駐 |
| **stdlib self-host（.almd）** | v0 経路では未使用（registry 専用） | 低（v0 に影響なし）だが単独では無意味 | 第3波と同時 |

## 順序の原則

1. **証拠から先に**（第1波）: 検証インフラは実装より先に合流させる — v0 の green の
   信頼度が上がり、後続の実装還流を受け止める網になる。
2. **1修正=1PR**（第2波）: v1 作業中に見つけた v0 バグの修正は、v1 と切り離して
   単体で説明可能な粒度に分解する（governing directive は「v0 バックポート不要」
   だったため、これは方針転換の明示承認が要る）。
3. **spine は置き換えの日に**（第3波）: MIR spine の還流は「emit_wasm を retire して
   render path を生産経路にする」決定と一体。それまで develop-v1 が本籍。

## 未解決の判断（ユーザー入力待ち）

- 第1波の実行時期（いつでも可能 — CI 追加のみ）
- 第2波の対象リスト精査（equi-recursive guard は v0 checker 挙動を変える —
  spec 全量 + org スイープでの v0 再検証が前提）
- 第3波のトリガー条件（例: parity baseline が spec 実行可能集合の 90% 超、
  org wall=0 が 26/26、Matrix/closure brick 完了）
