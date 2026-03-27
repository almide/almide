<!-- description: Showcase: Markdown-to-HTML converter using variant types and match -->
<!-- done: 2026-03-18 -->
# Showcase 4: Markdown to HTML (DevTool)

**領域:** DevTool / テキスト変換
**目的:** Markdown→HTML変換。variant型 + exhaustive match の実用例。

## 仕様

```
almide run showcase/md2html.almd -- README.md > output.html
```

- Markdown サブセット: `#` 見出し, `**` 太字, `*` 斜体, `` ` `` コード, `- ` リスト, 空行で段落区切り
- variant型で AST 定義
- pattern match で HTML レンダリング

## 使う機能

- `type MdNode = | Heading { ... } | Paragraph { ... } | ...` (variant型)
- exhaustive `match`
- `string.starts_with`, `string.slice`, `string.replace`
- `list.map`, `string.join`
- `fs.read_text`, `io.print`

## 成功基準

- [ ] Tier 1 (Rust) で動作
- [ ] Tier 2 (TS/Deno) で動作
- [ ] 80行以内
- [ ] README に使い方記載
