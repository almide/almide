<!-- description: Runtime contracts for value constraints -->
# Contracts — Runtime Validation

値の制約をランタイムで検証する仕組み。

## 参考

- **Nickel**: `label.rs` + `contract_eq.rs` — blame tracking 付き契約システム
  - Polarity (正/負) で契約違反の責任を追跡
  - `ty_path::Path` で制約違反の正確な位置を報告
  - RFC005: マージ時に契約がクロス適用される

## 設計案

```almide
type Score = Int | Range(0, 100)

fn validate(s: Score) -> Result[Unit, String] = ...
```

- 契約違反時に正確なエラー（どの制約が、どの値で違反したか）
- 契約の合成（複数の制約を AND/OR で組み合わせ）
- 範囲制約、型制約、カスタム述語
