<a id="github-linguist-への言語追加--準備台帳"></a>

# Adding a language to GitHub Linguist — preparation ledger

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](linguist-submission.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](linguist-submission.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](linguist-submission.zh-CN.md)



> Source: [docs/project/linguist-submission.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/project/linguist-submission.md) (pinned commit `35297a612fb29e3f78f6587aba253aa91d51f466`, Japanese original). This is a non-normative reader snapshot translation, not automatically synchronized. It preserves the source’s dates, scope, and claims; no independent measurements or implementation verification were performed for this translation. This is the source’s 2026-09-22 preparation record, not a current eligibility determination. Usage counts, review criteria, registry status, and translated maintainer quotations have not been rechecked; current submission decisions require checking the upstream sources. Its adoption estimates are planning estimates, not acceptance guarantees.

A plan to have GitHub recognize `.almd` as a language. **Not ready to submit as of 2026-09-22.** There is only one reason: insufficient third-party adoption. The rest—grammar, samples, color—can be prepared today.

Sources are [linguist/CONTRIBUTING.md](https://github.com/github-linguist/linguist/blob/main/CONTRIBUTING.md) and the [PR template](https://github.com/github-linguist/linguist/blob/main/.github/PULL_REQUEST_TEMPLATE.md). The figures were measured using the GitHub API on 2026-09-22.

<a id="1-条件と現状"></a>

## 1. Requirements and current status

| Requirement | Criterion | Status (2026-09-22) | Assessment |
|---|---|---|---|
| Extension usage | **At least 2,000 `.almd` files indexed within one year**, excluding forks | 5,512 overall | Apparently met |
| Usage excluding owners | Reviewers exclude primary owners with `-user:` when evaluating | 904 with `-user:almide -user:O6lvl4`; **0** after also excluding `-user:almide-graphics` | **Not met** |
| Distribution of owners / repositories | Randomly clicked results should be distributed | Of 904 files outside the owners, 296 are in a single repository, `almide-graphics/nn`. Effectively one organization | **Not met** |
| Syntax-highlighting grammar | A TextMate-compatible grammar in a public repository with an **accepted license** | `syntaxes/almide.tmLanguage.json` in `almide/vscode-almide` (`scopeName: source.almide`). No LICENSE at the initial inspection, but a same-day update records the addition of MIT (§2) | Addressed the same day (§2) |
| Samples | Real code. Hello world is rejected. Explicit license | Many candidates. `almide/almide` is dual MIT / Apache-2.0 | Can be met |
| Extension collision | No conflict with existing languages | No `.almd` in `languages.yml` | No problem |

Accepted licenses are apache-2.0, bsd-2-clause, bsd-3-clause, cc0-1.0, isc, mit, mpl-2.0, ncsa, permissive, unlicense, wtfpl, zlib.

**Submitting prematurely is counterproductive.** About 240 language-addition PRs were submitted to Linguist in 2026; 31 were merged, and the rest were closed for insufficient usage. Closed PRs remain on record.

<a id="通った言語は実際に何ファイルあったか2026-09-22-実測"></a>

### How many files did accepted languages actually have? (Measured 2026-09-22)

| Language | Merged | Total files from `extension:` |
|---|---|---:|
| Blueprint | 2026-06-01 | 42,624 |
| Gno | 2026-07-20 | 41,536 |
| Tape | 2026-08-06 | 24,704 |
| MeTTa | 2026-02-22 | 12,736 |
| Bend | 2026-09-20 | 9,440 |
| FPP | 2026-08-19 | 9,424 |
| Tolk | 2026-06-21 | 8,128 |
| BAML | 2026-05-13 | 6,704 |
| Quint | 2026-05-06 | 5,824 |
| **Almide** | — | **5,512 (0 excluding owners)** |

**In total volume, Almide already matches the lower end of accepted languages. The only difference is owner distribution.** Among 100 `.qnt` search results, at least 8 owners appear, and 5,648 files remain even after excluding the maintaining organization. All 100 `.almd` results belonged to `almide`.

Bend PR #8218 shows exactly how evaluation works. Its body reports 8,640 for `extension:bend`, and 2,664 after excluding the maintainer and the top 2 contributors. Quint's PR also supplied the count excluding the maintaining organization.

The maintainer's wording is explicit too. One rejection says: “2000 is the threshold. In addition, when assessing popularity we exclude the language's owner and the top contributors. We would have excluded you and your organization eventually anyway.”

<a id="15-文法の検査結果2026-09-22-実測"></a>

## 1.5 Grammar inspection results (Measured 2026-09-22)

The grammar is not the obstacle to Linguist. It was measured by actually tokenizing with the TextMate engine (`vscode-textmate` + `vscode-oniguruma`).

| Inspection | Result |
|---|---|
| Structure | `scopeName: source.almide`, `fileTypes: ["almd"]`, all 16 includes resolve, no orphan definitions |
| Coverage (3 files) | 83.4% of non-whitespace characters receive a scope |
| Coverage (examples + stdlib + spec, 577 files, 1.77 million characters) | **87.8%. Tokens without scopes are only punctuation such as brackets, commas, and colons** |
| Check of 20 language constructs | `effect fn`, `test`, the `=>` in `match`, `fan`, heredoc, `${}` interpolation, `!` `?` `??`, `@intrinsic`, type declarations—all receive scopes |

**One defect was found.** Function-definition names were `variable.other`, not `entity.name.function`. The cause was top-level pattern order: `#keyword` precedes `#function-def`, so it consumes `fn` first and `#function-def` never fires. TextMate uses the **first match in definition order**, not the longest match.

**The fix is one line.** In `vscode-almide`'s `generator/almide_textmate.almd`, move `Include("#function-def")` before `Include("#keyword")`. Verified locally:

- The generated diff is only that one-line move (comparing before/after output of `almide run generator/main.almd`).
- 30 function names across 3 files change to `entity.name.function.almide`.
- The `keyword.declaration` count remains 104, and overall coverage is unchanged.

**One more finding is inconsistent notation.** The grammar colors `strict` as a modifier keyword, but the compiler's `KEYWORDS` (34 words) does not include `strict`; it appears only in typo hints in `keyword_typo.rs`. There is no practical harm, but the entry in the single source of truth, `grammar/tokens.toml`, is broader than the implementation.

<a id="2-今日そろえられるもの"></a>

## 2. What can be prepared today

1. ~~**Place a LICENSE in `almide/vscode-almide`**~~ — **Landed 2026-09-22**. Added MIT, and GitHub's classification changed to `MIT`. Added the same to `almide/tree-sitter-almide` (main, 5ed0dc5). This meets Linguist's grammar-import requirement.
2. ~~**The one-line fix in §1.5**~~ — **Landed 2026-09-22** (vscode-almide develop, d4a1438). Fixed the generator and regenerated `syntaxes/almide.tmLanguage.json`. Rechecked the public file: all 20 constructs pass, and function names are now `entity.name.function.almide`. **Pending**: develop → main triggers a VS Code extension release, so release is on hold.
2. **Choose samples.** Real code representative of the language's constructs. Candidates selected by measurement:

   | File | Lines | Contents |
   |---|---:|---|
   | `examples/lisp.almd` | 268 | 27 `match` occurrences, 6 pipes. Best representative of volume and structure |
   | `examples/api-client.almd` | 79 | `import`, `effect fn`, `!` propagation |
   | `examples/csv-to-json.almd` | 52 | 7 pipe chains, `effect fn`, 3 `test` blocks |

   Avoid `stdlib/list.almd`. It is mostly `@inline_rust` directives, has no pipes, and does not look like consumer code. State in the PR body: “dual MIT / Apache-2.0, sourced from almide/almide.”
3. **Choose a color.** Proposed: `#7c5cbf`. The basis is the playground and docs site's accent color; `src/styles/custom.css` in `almide/docs` says `/* Purple accent from playground: #7c5cbf / #9b7de0 */`. No identical value exists in `languages.yml`.
4. **Draft entry** (leave out `language_id`, which `script/update-ids` assigns):

```yaml
Almide:
  type: programming
  color: "#7c5cbf"
  extensions:
  - ".almd"
  tm_scope: source.almide
  ace_mode: text
```

<a id="3-条件を満たす唯一の道第三者の採用"></a>

## 3. The only way to meet the requirements: third-party adoption

All currently indexed `.almd` files are under three owners: `almide`, `O6lvl4`, and `almide-graphics`. `almide-graphics` is an organization I belong to, and a single repository accounts for one third of the non-owner portion. The distribution requirement fails at this point. **Mass-producing your own repositories to inflate the count games the evaluation; reviewers exclude them with `-user:`**, so it is pointless as well.

An approximate target: at 20–40 files per repository, **50–100 third-party repositories** reach 2,000. This can simultaneously satisfy the distribution requirement. Realistic sources are:

- **Package authors.** Other people writing external packages installed with `almide add`. The packages currently under `almide/` all belong to me, so they count as zero in the evaluation.
- **Dojo participants.** Task-solution repositories contain `.almd` files. If third parties put them in their own accounts, they count.
- **Tutorials and examples.** Advent of Code, community tracks on Exercism, example repositories for language-introduction articles.
- **Editor users.** People who install the VS Code extension placing `.almd` in their own repositories.

Measuring progress (actual count excluding owners):

```bash
gh api -X GET search/code -f q='extension:almd -user:almide -user:O6lvl4 -user:almide-graphics' \
  -f per_page=1 --jq .total_count
```

Reviewers look at the Web UI count, so just before submitting, also open `https://github.com/search?type=code&q=NOT+is%3Afork+path%3A*.almd` while logged in and check it.

<a id="4-出す日の手順"></a>

## 4. Procedure on submission day

1. Fork `github-linguist/linguist` and create a branch.
2. Add the entry above to `languages.yml`. Do not write `language_id`.
3. Run `script/add-grammar https://github.com/almide/vscode-almide`. Docker is required. Grammar problems cause rejection; fix them there.
4. Place samples in `samples/Almide/`.
5. Assign `language_id` with `script/update-ids`.
6. Pass `bundle exec rake test`.
7. **Fill out the entire** PR template. Otherwise it will not be read. In the “Adding a new language” section, provide the search-results URL, sample sources and licenses, grammar-repository URL, and color with its rationale.

PR-body outline:

> Almide is a statically-typed language that compiles to Rust and WebAssembly.
> Search results: `https://github.com/search?type=code&q=NOT+is%3Afork+path%3A*.almd+effect+fn`
> Samples: taken from `almide/almide`, dual MIT / Apache-2.0.
> Grammar: `https://github.com/almide/vscode-almide` (MIT), `scopeName: source.almide`.
> Color `#7c5cbf`: the accent color of the Almide playground and documentation site.

<a id="5-先に取れる条件の軽いレジストリ"></a>

## 5. Registries with lighter requirements to pursue first

Linguist can come last. There are places to submit highlighting first. **All have review**, but they ask for grammar completeness and continued maintenance, not file counts. nvim-treesitter leaves “feature-complete, tried by users, and maintained” to maintainer discretion. Zed also explicitly says submissions are reviewed and not all are accepted.

| Destination | Required | Effect |
|---|---|---|
| Zed extensions registry | tree-sitter grammar + PR defining the extension | Highlighting in Zed |
| nvim-treesitter / Helix | PR registering a tree-sitter grammar | Highlighting in Neovim / Helix |
| Shiki | Add TextMate grammar | Highlighting on documentation and static sites |
| Pygments / Chroma | Lexer implementation | Sphinx, Hugo, many blogging platforms |
| VS Code Marketplace | Already published | Already obtained |

Being listed there is itself supporting evidence in a Linguist PR that this is a real language.
