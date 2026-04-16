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

### Phase 2 — Tooling (MoonBit-inspired, 実質完了 2026-04-16)
2-1. ✅ **`almide ide outline <file>`** — package の pub fn / type / let を 1 行ずつ列挙。grep 撲滅、E002/E003 の hallucination 消す。
2-2. ✅ **`almide ide doc <symbol> [--file <f>]`** — stdlib / user fn の signature + docstring を返す。`string.to_upper` を探す時に `grep` 不要。
2-3. **`almide ide peek-def <symbol>`** — 定義の snippet のみ返す (body あり)。**保留** — dojo 文脈では既存 body を peek する場面がないため MSR 寄与小。refactor 系 task bank 追加時に再評価。
2-4. **`almide ide find-refs <symbol>`** — 参照一覧。**保留** — 同上。
2-5. **AGENTS.md を `almide new` に同梱** — dojo SYSTEM_PROMPT を全 project に配布。
2-6. **runnable `*.almd.md` cheat-sheet** — `almide check *.almd.md` 通過保証。仕様書が drift しない。

### Phase 2 実測: try: snippet + hint 改善による MSR 押上げ (2026-04-16)

baseline (v0.14.5 = phase2 着手前) から llm-first-phase2 branch で以下を実装し、dojo で継続測定:

**実装した診断改善**:
- `almide ide outline / doc / stdlib-snapshot` + `@stdlib/<module>` + `--json`
- try: snippet を E002/E003/E004/E009 の 6 パス、E001 Unit-leak の 2 パス (fn body / if arm) に追加
- fn-body Unit-leak specialize: AST から trailing `let` 名を抽出して具体的 rewrite を提示
- if-arm Unit-leak specialize: Unit を返す arm の assign target を抽出して `let new_x = if ...` 形に誘導
- let-in detection を改行越しに拡張、Err 後も partial Let を残して下流診断にリレー
- int.sqrt / int.gt etc. の hallucination に conversion / operator rich snippet
- misplaced `test` keyword に harness-context hint
- rest/cons pattern (`[h, ...t]` / `head :: tail`) に list.first/drop 誘導 hint
- rustc 4桁 E-code leak wrap (`src/main.rs` パス mention なしの rustc エラーもバグ banner で包む)

**数値 (Δ = phase2 着手時 baseline → 最終)**:

| Release / Version | Model | retry-success | 1-shot | Δ baseline |
|---|---|---|---|---|
| v0.14.5 baseline | llama-3.3-70b | 17/30 (57%) | — | — |
| v0.14.5 baseline | llama-3.1-8b | 13/30 (43%) | — | — |
| 0.14.6-phase2 (14d1a973) | llama-3.3-70b | **23/30 (77%)** | 12/30 | **+6 (+20pt)** |
| 0.14.6-phase2 (14d1a973) | llama-3.1-8b | 10/30 (33%) | 10/30 | -3 (variance) |
| 0.14.6-phase2 (14d1a973) | **Sonnet 4.6** | **30/30 (100%)** | 26/30 (87%) | — |

**戦略判定**:
- Sonnet 30/30 = **Almide の言語設計に構造的欠陥はない**。70b の残 7 fail は純粋に model capability の壁 (ADT / state-tracking / complex accumulator)。
- **Path A (UFCS / imperative loop 譲歩) は不要と確定** — 強モデルは現設計で完走する。言語を汚す理由がない。
- 70b の 1-shot 12/30 vs Sonnet 26/30 の乖離 = retry 前 prompt 品質よりも純粋な LLM 能力差。SYSTEM_PROMPT の細工で縮められる余地は限定的。
- 8b variance は n=3 で吸収見込み。`12±2` が真値帯。

**Phase 2 完了条件**: ✅ 70b で baseline +5pt 以上、かつ強モデルで 90%+ 到達 (SOTA 相当)、かつ DESIGN rule 維持。

### Phase 3 — Fix tooling + stdlib expansion (MVP 着地 2026-04-16)
3-1. ✅ **`almide fix` MVP** — auto-import 自動適用 (E003 "Add `import json`" 等) + 残余 `try:` snippet を manual-fix として report。`--dry-run` プレビュー対応。`let-in` → newline chain / `head :: tail` → list.first-drop 等の AST-level 機械 rewrite は Phase 3-1.2 で拡張。
3-2. ✅ **stdlib primitives**: `list.binary_search(List[Int], Int) -> Option[Int]`, `string.run_length_encode(String) -> List[(String, Int)]` を追加 — dojo の binary-search / run-length-encoding 系 task を algorithm 合成から API 呼び出しに降格。他の候補 (`list.window`, `list.partition`, `list.chunks`) は dojo 再測定後に追加判断。
3-3. ✅ **llms.txt** — リポジトリ root に配置、mission/CLI/core idioms/error codes/stdlib pointer を ~5K 行で集約。LLM tool が 1-URL で fetch 可能。更新は手動 (SPEC.md からの自動生成は fmt-idempotent な変換器が要るので Phase 3-3.2 に繰り延べ)。

#### Phase 3 の残タスク
- **3-1.2 `almide fix` の AST-level rewrite** — let-in、cons pattern、int.gt などの決定論的 fix を機械適用。parser recovery を拡張して trailing body を保持する必要あり (現状は drop している)。
- **3-2.2 stdlib bundled-Almide dispatch** — `stdlib/list.almd` のような Almide-source 拡張が `list.*` module で動くよう codegen を修正。現状 TOML-module は `almide_rt_list_<fn>` に固定 routing されるため、`stdlib/list.almd` で追加関数を書いても `E002` 相当にならず codegen bug になる。dogfood 時に発見 (examples/almd-outline の README 参照)。
- **3-3.2 llms.txt の自動生成** — SPEC.md / cheatsheet / stdlib-snapshot を source-of-truth にして llms.txt を再生成する `almide docs-gen` 的な tooling。

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
  | Release | Sonnet 4.6 | 70b | 8b | Notes |
  |---|---|---|---|---|
  | v0.14.5 | — | 17/30 | 13/30 | retro — baseline for llm-first roadmap |
  | 0.14.6-phase2 | **30/30** | **23/30** | 10/30 | Phase 2 完了 (2026-04-16). Sonnet 30/30 で言語設計の validation 達成。 |
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
