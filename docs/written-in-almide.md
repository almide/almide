# Written in Almide

The standard library is the largest Almide program in this repository. The
projects below show uses elsewhere. This is a directory, not a claim that every
listed platform, host, or package was built and tested with the current compiler.
The repository/LOC census below is the dated 2026-09-30 observation.

Targeted source checks on 2026-10-31:
- [Bindgen's target registry](https://github.com/almide/almide-bindgen/blob/b563e8b9076ff511b9b20b03e2c2e475af2c4c0b/src/mod.almd)
  lists 21 languages including Almide; this is a source inventory, not 21
  independently tested bindings.
- [Porta's architecture](https://github.com/almide/porta/blob/123bef319c99610dd94fb73c28f6443c5a03fc73/README.md)
  distinguishes native process sandboxing from its separate Wasm mode. The
  directory does not establish the security of a deployment or combine both
  modes into a single universal guarantee.
Other descriptions remain project pointers; this targeted check did not repeat
all platform builds or the 144-repository census.

- **Coding agents** — [comide](https://github.com/O6lvl4/comide), a conversation in a directory · [golemide](https://github.com/O6lvl4/golemide), one static binary, observe-then-edit, every write through a syntax gate · [homullus](https://github.com/almide-ai/homullus), a modular agent runtime · [manus](https://github.com/almide-ai/manus), computer use on macOS
- **Agent infrastructure** — [porta](https://github.com/almide/porta), native process sandboxing (macOS Seatbelt / Linux Landlock, seccomp and namespaces), with a separate Wasm execution mode · [ctxgate](https://github.com/O6lvl4/ctxgate), a context firewall living in the tool hooks · [hew](https://github.com/O6lvl4/hew), structure-aware `cat`/`grep`/`sed` for agents · [almai](https://github.com/almide-ai/almai), one LLM interface over every provider
- **Parsing** — [gramide](https://github.com/O6lvl4/gramide), syntax trees for coding agents, with grammar packages for [TypeScript](https://github.com/O6lvl4/gramide-typescript), [Python](https://github.com/O6lvl4/gramide-python), [Rust](https://github.com/O6lvl4/gramide-rust), [Go](https://github.com/O6lvl4/gramide-go), [JavaScript](https://github.com/O6lvl4/gramide-javascript) and [Almide](https://github.com/O6lvl4/gramide-almide), each checked against that language's own parser · [parsegen](https://github.com/almide/parsegen), a tree-sitter compatible parser generator with no C and no dynamic loading
- **Graphics** — [ceangal2](https://github.com/almide-graphics/ceangal2), the snaidhm compute-shader-first 2D/3D renderer and the ceangal layout and widgets on top of it, one Almide program on the web, iOS, macOS, Android, Windows and Linux (start from [ceangal-hello](https://github.com/almide-graphics/ceangal-hello)) · [obsid](https://github.com/almide-graphics/obsid), Canvas 2D / WebGL / 3D through WASM · [nn](https://github.com/almide-graphics/nn) and [slabhra](https://github.com/almide-graphics/slabhra), neural-network primitives (with the kernels that left the stdlib `matrix`, in `nn.kernels`) and reverse-mode autograd · [cruth](https://github.com/almide-graphics/cruth), VRM/glTF characters · [lumen](https://github.com/almide-graphics/lumen), graphics math that needs no `@extern`
- **Desktop** — [almide-shell](https://github.com/almide-graphics/almide-shell), a Hyprland shell: bar, launcher, notifications, on-screen display · [sprid](https://github.com/O6lvl4/sprid), a terminal emulator with scrollback under a hard memory budget
- **Bindings and packaging** — [almide-bindgen](https://github.com/almide/almide-bindgen), FFI generation for 21 listed language targets, including Almide · [almide-wasm-bindgen](https://github.com/almide/almide-wasm-bindgen) · [almide-lander](https://github.com/almide/almide-lander), Almide as a native shared library · [almide-web](https://github.com/almide/almide-web), browser APIs
- **Libraries** — [toml](https://github.com/almide/toml) · [yaml](https://github.com/almide/yaml) · [csv](https://github.com/almide/csv) · [svg](https://github.com/almide/svg) · [dfa](https://github.com/almide/dfa) · [bigint](https://github.com/almide/bigint) · [aes](https://github.com/almide/aes) · [rsa](https://github.com/almide/rsa) · [sha1](https://github.com/almide/sha1) · [base64](https://github.com/almide/base64) · [almide-sqlite](https://github.com/almide/almide-sqlite) · [llm](https://github.com/almide/llm), streaming clients for LLM providers
- **Conformance and measurement** — [als](https://github.com/almide/als), the normative specification with its own judge, run against any `almide` binary · [almide-dojo](https://github.com/almide/almide-dojo), the LLM-writability ground, harness written in Almide · [bonsai-almide](https://github.com/almide/bonsai-almide), a 1-bit LLM in the browser

Measured 2026-09-30: 144 public repositories outside this one carry Almide source. Hand-written, excluding generated output: [nn](https://github.com/almide-graphics/nn) 21,815 lines, the [gramide](https://github.com/O6lvl4/gramide) family 19,780 across eight repositories, [almide-shell](https://github.com/almide-graphics/almide-shell) 8,502, [snaidhm](https://github.com/almide-graphics/snaidhm) 6,800, [almide-bindgen](https://github.com/almide/almide-bindgen) 5,871, [ceangal](https://github.com/almide-graphics/ceangal) 5,099, [sprid](https://github.com/O6lvl4/sprid) 4,565, [porta](https://github.com/almide/porta) 4,419. snaidhm and ceangal were counted in their own repositories, which have since been archived and moved into ceangal2.

## Tooling

- [almide-grammar](https://github.com/almide/almide-grammar) — the syntax description the editor and tree-sitter tooling is generated from (keywords, operators, precedence, TextMate scopes), written in Almide; it mirrors the compiler's lexer rather than feeding it, and the weekly `downstream-sync` workflow (`scripts/check-downstream-grammar.sh`) fails when the two token sets differ
- [vscode-almide](https://github.com/almide/vscode-almide) · [tree-sitter-almide](https://github.com/almide/tree-sitter-almide) (Neovim, Helix, Zed) · [playground](https://github.com/almide/playground)
