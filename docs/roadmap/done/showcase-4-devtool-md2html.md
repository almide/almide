<!-- description: Showcase: Markdown-to-HTML converter using variant types and match -->
<!-- done: 2026-03-18 -->
# Showcase 4: Markdown to HTML (DevTool)

**Domain:** DevTool / text conversion
**Purpose:** Markdown to HTML conversion. Practical example of variant types + exhaustive match.

## Specification

```
almide run showcase/md2html.almd -- README.md > output.html
```

- Markdown subset: `#` headings, `**` bold, `*` italic, `` ` `` code, `- ` lists, blank lines for paragraph breaks
- AST defined with variant types
- HTML rendering via pattern match

## Features Used

- `type MdNode = | Heading { ... } | Paragraph { ... } | ...` (variant types)
- exhaustive `match`
- `string.starts_with`, `string.slice`, `string.replace`
- `list.map`, `string.join`
- `fs.read_text`, `io.print`

## Success Criteria

- [ ] Works on Tier 1 (Rust)
- [ ] Works on Tier 2 (TS/Deno)
- [ ] Under 80 lines
- [ ] Usage documented in README
