<!-- description: Plan to make Almide the language LLMs write most accurately, measured by dojo MSR -->
# LLM-first Language

Almide の **mission**: "The language LLMs can write most accurately." この roadmap はそこに向けた 5 軸の設計判断と、段階的な実装順をまとめる。**every change is measured against almide-dojo MSR delta** — 言語機能を追加する基準は「retry-success が上がるか」 で決める。

## Design rule

> 人間にとって美しいが LLM にとって罠な構文は採用しない。
>
> 機能の価値 = **MSR delta** (dojo で 70b / 8b / Sonnet の retry-success 変動) で測る。
> 綺麗だが MSR を 3pt 以上下げる機能は不採用、既存機能でも要検討。

## 5 軸

### Axis 1 — 間違える機会を減らす
- **UFCS** (`n.abs()` → `int.abs(n)` 自動解決): 最大のベット、method-call + list field の 30-40% 自動解消。別 proposal 扱い (後述)。
- **Auto-import top 30 stdlib**: 現 Tier 1 の拡大、E002 系減らす。副作用 (imports の透明性) とトレードオフ。
- **One canonical form**: ドキュメント / lint レベル。言語変更なし。

### Axis 2 — パーサが救う
- **Error-recovering parser**: 1 つの `)` 抜けで 30 行カスケードしない。Tree-sitter 的 panic-mode。
- **Cascade suppression** (v0.14.5 に先行実装済 ✓): parse error 配下の `undefined function` 抑制。型エラー方向に拡張余地あり。

### Axis 3 — エラーが自己修復可能
- **`try:` code snippet** (Elm 流): hint テキストだけでなく、貼り付け可能な修正コード片を error に同梱。
- **`almide fix`**: try snippet が機械適用可能なら `cargo fix` 相当を提供。LLM の retry を compiler が代行。
- **Single source of truth for cheat-sheet**: `SYSTEM_PROMPT` / `CHEATSHEET.md` / `llms.txt` を `SPEC.md` から自動生成。dojo と本体と外部 tool が同じ文面を見る。

### Axis 4 — Modification を安全に (MSR の "M")
- **Block boundary marker** (MoonBit `///|` 相当): 1 関数の修正が他関数を壊さない。parser touching あり。
- **Per-fn dependency 宣言**: `fn name(...) uses [list, int]` — LLM の局所修正を safe にする。ROI 不明瞭、後回し。
- **`#deprecated` + `#alias` による長期 migration**: 言語進化が壊滅的にならない。

### Axis 5 — MSR を first-class metric に
- **dojo を本体 CI に内蔵**: 各 PR が MSR delta を計算、`-2pt 以上` の回帰で auto-block (初期は warning-only で運用試行)。
- **Release notes に MSR スコア**: 各リリースが retry-success に責任を持つ。
- **新機能の意思決定軸**: 「綺麗だが MSR -3pt」な機能は採用しない。

## 実装 Phase 順 (staged plan, tooling-first per MoonBit findings)

MoonBit の経験則: **「LLM-friendly な言語」だけでは不十分。「LLM-friendly な開発環境」** が同等以上に効く。Almide も tooling を言語拡張より先に積む。

### Phase 1 (完了)
- ✅ `try:` snippet + Diagnostic 拡張 (f87b7d0b)
- ✅ DESIGN rule 明文化 (9a583e72)
- ✅ roadmap 整備

### Phase 2 — Tooling (MoonBit-inspired, 着手中)
2-1. ✅ **`almide ide outline <file>`** — package の pub fn / type / let を 1 行ずつ列挙。grep 撲滅、E002/E003 の hallucination 消す。
2-2. ✅ **`almide ide doc <symbol> [--file <f>]`** — stdlib / user fn の signature + docstring を返す。`string.to_upper` を探す時に `grep` 不要。
2-3. **`almide ide peek-def <symbol>`** — 定義の snippet のみ返す (body あり)。
2-4. **`almide ide find-refs <symbol>`** — 参照一覧。
2-5. **AGENTS.md を `almide new` に同梱** — dojo SYSTEM_PROMPT を全 project に配布。MoonBit 同様「最初に読む 1 ファイル」。
2-6. **runnable `*.almd.md` cheat-sheet** — `almide check *.almd.md` 通過保証。仕様書が drift しない。

### Phase 3 — Fix tooling + stdlib expansion
3-1. **`almide fix`** — Phase 1 の `try:` snippet を機械適用。構文系 (let-in / let rec / while-do / return) から。
3-2. **stdlib pattern expansion** — dojo 11 fail タスクのうち「stdlib が足りないだけ」を解消: `list.binary_search`, `string.run_length_encode`, `list.window`, `list.partition`, etc.
3-3. **llms.txt** — SPEC.md から自動生成、1 URL で全情報。外部 LLM tool が参照。

### Phase 4 — Language-level changes (tooling で不足分を見てから)
4-1. **UFCS 採用判断**: Phase 2-3 の MSR 改善を踏まえて。dojo 8b が依然 parse-err 多いなら GO。70b の改善は tooling で取れてる想定。
4-2. **Error-recovering parser 強化**: カスケード抑制を型エラー方向に拡張。
4-3. **`?` operator / Option chain 強化** — 70b の type-err 9 件対策。

### Phase 5 (長期)
5-1. **dojo を CI 内蔵**: PR レベルで MSR delta を自動計算。
5-2. **`///|` block boundary** (MoonBit 風): 局所 refactor を構文で保証。
5-3. **Skill marketplace / playbook**: refactor / bug-fix / new-feature の mode 別 playbook を plugin 化。

## UFCS 別 proposal (保留理由)

- **現状 (v0.14.3+)**: `n.to_uppercase()` は error + hint `Write 'string.to_upper(x)'` を出す。v0.14.4 dojo data では retry-success ±0。
- **論点**: error improvement に天井がある。UFCS は「間違えた形を正として受け入れる」言語セマンティクス変更で、別次元の effect。
- **コスト**: parser で `.method()` を UFCS sugar として解釈、checker がモジュール dispatch。 estimated 5-10 iter。
- **リスク**: LLM 訓練データが 3 形式 (`int.abs(n)`, `n.abs()`, `n |> int.abs`) に分散して収束しない可能性。One canonical form 原則の放棄。
- **判断基準**: Phase 1-2 の MSR が +10pt 未満 → UFCS で残りを取りに行く。+10pt 超えた → UFCS 不採用、canonical form 維持。

## 計測

- **dojo runs**: 各リリース後に 70b / 8b / (optional) Sonnet で 30 タスクを T=0 で 3 回実行。中央値を採用。
- **MSR delta table** (Release notes に載せる):
  | Release | 70b | 8b | Notes |
  |---|---|---|---|
  | v0.14.5 | 17/30 | 13/30 | retro — baseline for llm-first roadmap |
- **retry budget**: dojo data で `pass-3 = 0 across all runs` を確認。`max_retries=3` は無駄、**2 で十分**。余った budget はタスクを 2 周して variance 除去に回す。

## dojo data から学んだ構造 (2026-04-15)

### "両モデル必ず通る" 12 タスク = sweet spot
`clamp`, `fizzbuzz`, `factorial`, `flatten-nested`, `repeat-string` 等 — 単一関数 / プリミティブ I/O / 浅い再帰。100% pass。**MSR-first 言語のベースライン保証ライン**。

### "両モデル必ず落ちる" 11 タスク = 設計の中心課題
3 つの機能群に集中:
- **ADT / sum types**: `anagram-check`, `custom-linked-list`, `mini-json-query`
- **Stateful loops** (low/high ポインタ等): `binary-search`, `matrix-ops`
- **String algorithm pipelines**: `balanced-parens`, `caesar-cipher`, `roman-numeral`, `run-length-encoding`, `string-reverse`, `result-pipeline`

→ **機能を捨てるな**。stdlib で "common pattern as a function" を増やして LLM が algorithm を書かずに合成できるようにする:
- `list.binary_search`, `string.run_length_encode`, `list.window`, `list.partition`, etc.
- 新 Phase として **Phase 1.5: stdlib pattern expansion** を追加する判断。

### 8b > 70b 逆転 = one canonical form の証拠
`max-of-list`, `sum-digits` で 8b が通り 70b が落ちる。70b は賢いがゆえに `xs.head` method chain / `let rec` を過剰に書く。**小モデルでも書ける = 大モデルでも勝手に過剰に書かない**。Elm の "only one way" と MSR-first が整合。

### parse-error vs type-error の model-size 依存
| | parse | type |
|---|---|---|
| 8b | 9 | 3 |
| 70b | 0 | 9 |

→ **小モデル向け**: 構文 forgive (UFCS、auto-import、optional 括弧?) が効く
→ **大モデル向け**: 型推論強化、Option/Result chain (`?` operator) が効く
→ 両方を別々の Phase で積む。

## UFCS 再考 (data を見た後)

- 8b の parse-error 9 件 vs 70b の 0 件 = **UFCS が効くのは 8b に強い、70b にはほぼ無関係**。
- 70b の type-error 9 件 = 型推論 / Option chain の改善が効く。
- 結論: UFCS は「8b の底上げ」目的なら採用価値大。70b の天井を破るには別方針 (型推論強化、`?` chain) が必要。
- **決定**: Phase 1-2 終了後、8b MSR が +5pt 未満なら UFCS 検討。70b の改善は type-inference Phase として別立て。

## 非目標 (明示)

- Haskell-class の型システム美 (Linear / Dependent / etc.) の追求。
- "Almide is pure FP" のような純粋性ブランディング。
- Elm ほど厳格な "no method syntax" 世界観 (UFCS 採用なら捨てる)。
- 「full-stack framework」的 lock-in。
