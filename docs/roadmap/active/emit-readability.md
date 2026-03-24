# Emit Readability [ACTIVE]

**優先度:** Medium — LLM が修正する生成コードの品質に直結
**前提:** codegen v3 (TOML template walker) 完了済み
**目標:** `--target rust` / `--target ts` の出力コードを人間・LLM が読みやすい形に改善する

> 「生成コードの可読性は modification survival rate に直結する。」

---

## Why

Almide の mission は "LLM が最も正確に書ける言語"。`--target rust/ts` で emit されるコードを LLM が読み・修正するケースがある (デバッグ、統合、学習)。現状の emit は正しいが、以下の点で可読性に改善余地がある:

- ソースの構造 (空行、論理ブロック) が失われる
- 変数名は保持されるが、コメントは保持されない
- 生成コードのフォーマットが機械的で、意図が読みにくい場合がある

可読性の高い生成コードは:
1. LLM がコンテキストを掴みやすく、修正精度が上がる
2. 人間がデバッグしやすい
3. 生成コードの「ソースマップ」として機能する

---

## Design

### 保持対象

| 要素 | 現状 | 目標 |
|---|---|---|
| 変数名 | ✅ 保持 | 維持 |
| 関数名 | ✅ 保持 | 維持 |
| 空行 (論理ブロック区切り) | ❌ 除去 | ソースの空行を emit に反映 |
| doc コメント | ❌ 除去 | `/// ...` / `/** ... */` として emit |
| インラインコメント | ❌ 除去 | 将来検討 |
| ソースの関数順序 | ✅ 保持 | 維持 |
| import 順序 | △ 機械的 | 論理グループ化 |

### 非目標

- ソースコメントの完全保持 (コンパイラ内部コメントは emit しない)
- emit コードの手書き品質との完全一致 (あくまで「読みやすい機械出力」)

---

## Phases

### Phase 1: 空行の保持

- [ ] Parser: 空行位置を AST/IR に記録 (`blank_lines_before: u32`)
- [ ] IR: `IrStmt` に空行アノテーション追加
- [ ] Walker: 空行アノテーションに基づいて emit 時に空行挿入
- [ ] テスト: ソースの論理ブロック構造が emit に反映されることを確認

### Phase 2: doc コメントの保持

- [ ] Parser: doc コメント (`/// ...`) を AST に記録
- [ ] IR: 関数・型定義に doc コメントフィールド追加
- [ ] Rust emit: `/// ...` として出力
- [ ] TS emit: `/** ... */` として出力
- [ ] WASM: コメントは emit しない (バイナリなので)

### Phase 3: import のグループ化

- [ ] Rust emit: `use std::` / `use crate::` / 外部 crate でグループ化 + 空行
- [ ] TS emit: stdlib / local module でグループ化
- [ ] 論理的な順序 (stdlib → external → local)

### Phase 4: フォーマット品質

- [ ] 長い式の改行ルール改善 (builder chain, 長い引数リスト)
- [ ] match/when の腕の整列
- [ ] emit 後に `rustfmt` / `prettier` を通さなくても読めるレベルを目標

---

## Implementation Notes

- 空行保持は Parser → IR → Walker の全段を通すため、変更範囲が広い
- doc コメントは IR に `Option<String>` を追加する軽量な変更
- TOML テンプレートに空行・コメントのプレースホルダーを追加

---

## Success Criteria

- `almide app.almd --target rust` の出力がソースの論理構造を保持
- doc コメントが Rust/TS 出力に反映される
- emit 出力を LLM に渡して「この関数は何をしているか」を問うた時の正答率が向上 (定性評価)
