# Editor & GitHub Integration

### TextMate Grammar + Editor Extensions

Repository: [almide/almide-editors](https://github.com/almide/almide-editors)

- [x] Create `.tmLanguage.json` for Almide syntax highlighting
- [x] VS Code extension ("Almide") — working, not yet published to Marketplace
- [x] Chrome extension ("Almide Highlight") — working, highlights `.almd` files on GitHub + `\`\`\`almd` / `\`\`\`almide` code blocks on any website
- [ ] Publish VS Code extension to VS Code Marketplace
- [ ] Publish Chrome extension to Chrome Web Store
- [ ] Dark mode theme switching (re-highlight on toggle without reload)

### GitHub Linguist Registration

Goal: get `.almd` recognized as "Almide" on GitHub (language bar, syntax highlighting, search).

**Requirements** (from [linguist CONTRIBUTING.md](https://github.com/github-linguist/linguist/blob/main/CONTRIBUTING.md)):
- 2,000+ `.almd` files indexed on GitHub in the past year (excluding forks)
- Reasonable distribution across unique `user/repo` combinations (not dominated by the language author)
- TextMate grammar with an approved license
- Real-world code samples (no "Hello world")

**Tracking metrics:**
| Metric | Current | Target |
|--------|---------|--------|
| `.almd` files on GitHub | ~10 | 2,000+ |
| Unique repos with `.almd` | ~2 | 200+ |
| Unique users with `.almd` | ~1 | 50+ |
| TextMate grammar | ✅ done | required |
| VS Code extension published | created (unpublished) | recommended |
| Chrome extension | ✅ working | interim solution |

**Interim workaround:** `.gitattributes` with `*.almd linguist-language=OCaml` for approximate highlighting.
