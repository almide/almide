# Package System Specification

> Almide's dependency management, version resolution, and module isolation.
> Design informed by Go Modules (MVS), Cargo (type identity), pnpm (boundary enforcement).

## 1. Design Principles

1. **Minimal Version Selection** — Always pick the minimum version that satisfies all constraints. No SAT solver. Deterministic, reproducible, fast. (Go Modules)
2. **Strict module boundaries** — Code can only access modules it directly imports. Transitive dependencies are invisible. No phantom dependencies. (pnpm)
3. **Type identity by (name, major)** — Two packages with the same `(name, major)` produce the same types. Different majors produce incompatible types. (Cargo)
4. **Single source of truth** — `almide.toml` declares intent, `almide.lock` records exact commits. A matching lock is respected; source changes require explicit re-locking and `--locked` refuses changes.
5. **Future-proof for registry** — Current git-based system will be supplemented by an Almide package registry. Design decisions must not preclude this.

## 2. Package Identity

```text
PkgId { name: String, major: u64 }
```

- Derived from `almide.toml`'s `[package] version` or the git tag.
- `0.x.y` uses minor as major (pre-1.0 breaking changes).
- `mod_name()` returns `"{name}_v{major}"` for codegen symbol namespacing.

## 3. Version Resolution: MVS

A package is its **origin** — the typed git source and the directory in it
(`subdir`), or the canonical local directory of a `path` dependency — not the
name it declares for itself (#4834). MVS selects among versions of one
origin. When multiple dependents request the same package (same origin, same
major):

```text
B requires D >= 1.2.0
C requires D >= 1.5.0
→ resolve to D 1.5.0 (maximum of minimums)
```

When majors differ:

```text
B requires D v1.x (major=1)
C requires D v2.x (major=2)
→ both coexist as separate modules (D_v1, D_v2)
→ D_v1.Logger ≠ D_v2.Logger (different types)
```

Two different origins that declare the same `[package] name` (at any major)
are refused, naming both origins and the chain that asked for each: `import
<name>` can name only one package, and a selection between two sources would
put one in the other's place without a word (before #4834 a transitive
package from another source that declared the root's dependency's name at a
higher version replaced it, and `[permissions] native` followed the name).
A native grant covers the origin the root manifest declares under that name.

```text
error: two sources declare the package `codec`
  path /work/jsona (required by app → codec)
  path /work/jsonb (required by app → util → jsonfork)
  note: `import codec` can name only one of them; almide never picks one over the other
```

Tests: `tests/mvs_resolution_test.rs` (one repository at two tags, both
declaration orders), `tests/dep_package_identity_test.rs` (two path origins,
and two git origins under a native grant, each refused naming both).

## 4. Module Boundaries

### 4.1 Direct vs Transitive

```text
A imports B, C
B imports D
C imports D
```

From A's perspective:
- `B.func()` ✓ — direct dependency
- `C.func()` ✓ — direct dependency
- `D.func()` ✗ — transitive only (unless A also imports D)
- `D.Logger` ✗ — type not visible unless A imports D

If A needs D's types, A must declare `import D` explicitly.

### 4.2 Enforcement

The type checker tracks which modules each file has imported via `import` statements.
`resolve_module_call` and `resolve_static_member` check against this set, not the global `user_modules`.

### 4.3 Re-exports

A module can re-export a dependency's types by wrapping:

```almide project
// file: d/mod.almd
type Logger = { name: String }
fn make(n: String) -> Logger = Logger { name: n }
// file: b/mod.almd
import d
type Logger = d.Logger  // re-export (future: explicit pub use)
fn make(n: String) -> Logger = d.make(n)
fn name_of(l: Logger) -> String = l.name
// file: main.almd
import b

test "the consumer sees D's type through B without importing D" {
  let l: b.Logger = b.make("x")
  assert_eq(b.name_of(l), "x")
}
```

## 5. Codegen: Versioned Symbols

When `IrModule.versioned_name` is set (e.g., `"json_v2"`), codegen uses it for function prefixes:

```rust
// Without versioning (same major, no conflict)
pub fn almide_rt_json_parse(...) { ... }

// With versioning (different majors coexist)
pub fn almide_rt_json_v1_parse(...) { ... }
pub fn almide_rt_json_v2_parse(...) { ... }
```

Struct names are also versioned to prevent type collisions:

```rust
pub struct JsonV1_Config { ... }
pub struct JsonV2_Config { ... }
```

## 6. Dependency Declaration

```toml
# almide.toml
[package]
name = "myapp"
version = "0.1.0"

[dependencies]
bindgen = { git = "https://github.com/almide/almide-bindgen", tag = "v0.1.0" }
json = { git = "https://github.com/almide/json", tag = "v2.0.0" }
```

**Each key appears once per table, and each table once** (#2583). TOML forbids
a repeated key, and every command that reads `almide.toml` (`check`, `run`,
`test`, `build`, …) refuses one before fetching anything or touching the lock,
naming both lines:

```text
error: almide.toml:7: dependency `almai` is declared twice in [dependencies] (first at line 6)
  hint: keep one of lines 6 and 7 and delete the other — TOML allows a key only once per table
```

The rule covers every table the manifest reader reads (`[package]`,
`[dependencies]`, `[permissions]`, `[native-deps]`), including a repeated
table header. Two spellings of the same url (`…/almai` and `…/almai.git`) are
still one dependency declared twice.

Test: `tests/manifest_duplicate_key_test.rs`.

**`[package] almide = "X.Y.Z"` is the minimum compiler** (#4172). Write it as a
plain version or as `">=X.Y.Z"`; any other operator is an invalid pin. Every
command that checks or compiles the package refuses an older compiler before
it reads a source: `check`, `emit`, `compile`, `build`, `run` and `test` on
every target, plus `bench`, `fix`, `survive`, `apply` and `verify`. It used to
be read only by the native build, so `check` and the wasm target compiled with
any compiler. Package management (`add`, `deps`, `update`, `install`) and
`fmt` still run, so the pin can be read and changed.

```text
error: package 'myapp' requires almide >= 0.70.0
  installed version: 0.69.0
```

`ALMIDE_SKIP_VERSION_CHECK=1` bypasses it. A release build honours that only
with `--allow-switch ALMIDE_SKIP_VERSION_CHECK` (#4129).

Test: `tests/version_pin_commands_test.rs`.

**A key outside its section's declaration is an error** (#3382; a warning
until dialect epoch 17). It used to be dropped without a word, so
`brnach = "main"` or `tags = "v0.1.0"` in a dependency built from the default
ref, not the one written, and `[permission] allow = []` left every capability
granted. Every command that reads `almide.toml` (`check`, `check --json`,
`run`, `build`, `test`, `bench`, `emit`, and the editor server as a
diagnostic) refuses the manifest before it does anything else, naming each
unknown key on its line, the keys that section declares, and the key one edit
away (insertion, deletion, substitution or an adjacent swap) when there is one:

```text
error: almide.toml:5: unknown key `brnach` in dependency `foo` is refused — almide.toml accepts only the keys its sections declare
  hint: did you mean `branch`? The keys dependency `foo` declares are `git`, `tag`, `branch`, `version`, `path`, `subdir`
```

| Section | Declared keys |
|---|---|
| top level | `[package]`, `[dependencies]`, `[native-deps]`, `[permissions]`, `[target]`, `[trust]` |
| `[package]` | `name`, `version`, `almide`; and `description`, `edition`, `license`, `repository`, which describe the package and change nothing |
| a `[dependencies]` entry (inline table, dotted keys or `[dependencies.<name>]`) | `git`, `tag`, `branch`, `version`, `path`, `subdir` |
| `[permissions]` | `allow`, `proc`, `native`, `fs` |
| `[trust]` | `owners`, `media` |
| `[target.<platform>]` | `native-deps` |

`[native-deps]` entries are crate names and `[trust].media` keys are package
sources, so any key is one there (a crate spec is Cargo's to judge). A
dependency entry that is not a table, or names neither `git` nor `path`,
declares nothing and is a warning (stderr; a `"level":"warning"` row in
`almide check --json`), which never changes the exit code.

The table above is `MANIFEST_SCHEMA` in `src/project.rs`, which binds each
section to the key list its reader reads; the refusal takes its vocabulary
from that one table. `tests/manifest_unknown_key_test.rs` asserts that every
listed key changes the parse (and that every metadata key does not), so a
list cannot drift from its reader, and that a fresh key in every declared
section is refused.

Short form (defaults to github.com/almide/):
```bash
almide add bindgen@v0.1.0
# → bindgen = { git = "https://github.com/almide/almide-bindgen", tag = "v0.1.0" }
```

A one-part name is a package under github.com/almide. A package's name and
its repository's name can differ (`bindgen` lives in `almide-bindgen`), so the
name is looked up in a snapshot of the org's packages embedded in the compiler
(`src/almide_packages.txt`, regenerated by `scripts/gen-almide-packages.sh`);
a name the snapshot does not list resolves to `github.com/almide/<name>`.
`almide add almide/<repo>` names the package the snapshot gives that
repository, so `almide add almide/almide-bindgen` adds `bindgen` too (#4093).

### 6.1 A package in a subdirectory of the repository: `subdir`

A repository can hold several packages, each in its own directory with its own
`almide.toml`. A git dependency names one of them with `subdir` (#3381):

```toml
[dependencies]
ceangal = { git = "https://github.com/almide-graphics/ceangal2", tag = "v0.1.0", subdir = "ceangal" }
snaidhm = { git = "https://github.com/almide-graphics/ceangal2", tag = "v0.1.0", subdir = "snaidhm" }
```

The package is named explicitly rather than found by scanning the repository
for a package of that name (Cargo's rule): the manifest says exactly which
directory is built, nothing depends on what else the repository holds, and a
reader of `almide.toml` sees it without fetching.

- **Spelling.** `subdir` is a path relative to the repository root, `/`-separated
  on every platform. It is normalized before use: `./`, repeated and trailing `/`
  are dropped (`./pkgs//c/` is `pkgs/c`), and the normalized spelling is the
  one the lock records. Every command that reads `almide.toml` refuses, on the
  line that writes it, a `subdir` that is not a string, is empty, names the root
  itself (`.` — delete `subdir` instead), is absolute (`/x`, `C:/x`), uses `\`,
  or contains `..` at all (even `a/../b`: one spelling per directory).
- **Git only.** `subdir` beside `path` is refused — a `path` already names the
  package directory; the hint gives the joined `path`.
- **Checked at fetch.** The directory must exist in the fetched tree (which
  holds no links, §6.5), and hold an `almide.toml` whose
  `[package] name` is the dependency's key — the key is the import name. Each
  refusal names the manifest line and lists the package directories the
  repository does hold (up to two levels down):

```text
error: almide.toml:6: dependency `ceangal` names subdir `snaidhm`, whose package is `snaidhm`
  hint: the dependency key is the import name and must be the package's name — rename the key to `snaidhm`, or point `subdir` at the package `ceangal`
error: almide.toml:6: subdir `pkgs/ceangal` of dependency `ceangal` does not exist in https://… at v0.1.0
  hint: the packages in this repository are `ceangal`, `snaidhm` — point `subdir` at one
```

  A git dependency WITHOUT `subdir` whose repository root is no package (no
  `almide.toml`, no `src/`, no `.almd` file) while its subdirectories are is
  refused the same way, with the `subdir = "…"` to add.
- **Identity.** A package is a source and a directory in it: two subdirs of one
  repository are two packages, walked separately for their own requirements and
  locked as two entries. `PkgId` (name, major) and MVS are unchanged — the name
  is still the package's own `[package] name`.
- **One fetch.** The fetched unit is the repository's tree at one commit,
  cached under `~/.almide/cache/trees/<sum>/` (§6.5). Dependencies on the same
  source/ref share that tree whether or not they specify `subdir`; each reads
  its package directory within it. Older layouts (`.sources/`,
  `.repos/.src-<hash>`, name-keyed) are not read (see §6.5).
- **CLI.** `almide add <repo>[@<tag>] --subdir <dir>` names the package after
  the subdir's last component (`--subdir pkgs/ceangal` → `ceangal`); with
  `--git <url>` the first argument is the name. `add` fetches and checks the
  package BEFORE it writes `almide.toml`, so a wrong `subdir` leaves the
  manifest as it was. `almide deps` prints `… (<ref>) subdir <dir>`, and
  `almide deps --path <name>` prints the package's directory inside the clone
  (its `src/` when it has one). `almide update` advances the entry like any
  other git dependency.

Tests: `tests/dep_git_subdir_test.rs` (a local repository holding two packages:
check, lock, one shared clone, `deps --path`, `deps`, `add --subdir`, every
refusal), the `project_fetch` unit tests (`package_dir_in_checks_the_subdir_against_the_checkout`,
`subdir_packages_of_one_repository_share_a_clone_root`), `tests/lock_roundtrip_test.rs`.

### 6.2 Native Rust dependencies: `[native-deps]`

A package with `native/*.rs` modules or `@extern(rust, …)` functions declares
the crates.io crates they use in `[native-deps]`. Each entry is copied verbatim
into the generated crate's Cargo.toml, on every native build route (binary,
`--cdylib`, `--repr-c`, `almide run`, `almide test`). A dependency package's
`[native-deps]` are added the same way.

```toml
[native-deps]
anyhow = "1"
serde = { version = "1", features = ["derive"] }
```

**Target-specific native deps** (#3350). A crate that builds only on some
targets goes under `[target.<platform>.native-deps]`, which uses Cargo's
syntax and meaning. It is emitted as `[target.<platform>.dependencies]`, so
Cargo compiles the crate only when the build's target (the host, or
`--target <triple>`) matches:

```toml
[target.'cfg(not(any(target_os = "android", target_os = "ios")))'.native-deps]
arboard = "3"

[target.'cfg(target_os = "android")'.native-deps]
jni = "0.21"

[target.x86_64-pc-windows-gnu.native-deps]
winapi = "0.3"
```

`<platform>` is either `cfg(<expr>)` or a target triple, exactly as Cargo
accepts them. `<expr>` is `all(…)`, `any(…)`, `not(…)`, a name (`unix`), or
`name = "value"`. Every command that reads `almide.toml` refuses a key Cargo
would refuse, and names the key and its line:

```text
error: almide.toml:4: invalid platform `cfg(target_os = android)` in [target.'cfg(target_os = android)'.native-deps]: expected a string after `target_os =`, found `android`
```

Only `native-deps` can be target-specific: any other table under
`[target.<platform>]` is an error. A crate the runtime itself needs (for
example `flate2` for `zlib`, or the TLS crates for `http`) is always declared
for every target. If you also declare that crate for one target, both
declarations are written and Cargo merges them.

Tests: `tests/manifest_toml_reader_test.rs`, `tests/native_deps_target_cfg_test.rs`,
and the `cargo_build` unit tests.

### 6.3 Trusted sources: `[trust] owners`

Models invent repository names, and an invented name repeats across runs, so
an attacker can register it before a model asks for it (#4093). The toolchain
therefore fetches a git dependency only from an owner the **root** manifest
trusts:

- `github.com/almide` is trusted by default;
- any other owner, on any host, is listed as `host/owner` in `[trust] owners`
  (or a deeper path prefix, `gitlab.com/group/sub`);
- `path =` dependencies are exempt, since nobody can register a name there
  (a `file://` or plain-path `git` source is refused outright, §6.5);
- a dependency's own `[trust]` is never read: a package cannot grant trust on
  the root's behalf.

```toml
[dependencies]
csv   = { git = "https://github.com/almide/csv", tag = "v0.1.0" }   # trusted by default
mylib = { git = "https://github.com/alice/mylib", tag = "v1.2.0" }

[trust]
owners = ["github.com/alice"]
```

The check runs before any network access, on `almide add` and on every fetch
(`build`, `run`, `test`, `bench`, `update`), for direct, transitive and
already-locked dependencies alike; a lock is not a grant. Trust is decided on
the typed source (§6.5): its host, and a grant's path as a whole-segment
prefix of the source's path (`github.com/almide` covers
`github.com/almide/json` and `github.com/Almide/json`, never
`github.com/almide-evil/json` or `github.com.evil.io/almide/json`). Path
segments compare without case. A source the grammar refuses never reaches the
check. There is no flag or environment variable that trusts
everything: a grant always names its owner, in a file reviewed with the rest
of the change.

**`almide add`** resolves the spec, checks the owner, probes that the
repository exists and has the requested tag (one ref listing over the
in-process client, §6.5; `--dry-run` stops here), fetches it, and only then
writes `almide.toml` and `almide.lock`. Any refusal or failure exits 1 and
leaves both files byte-identical:

```text
$ almide add alice/mylib@v1.2.0
Resolved mylib → https://github.com/alice/mylib (owner github.com/alice, NOT trusted)
error: refusing to add `mylib`: its owner is not trusted by this project
  source: https://github.com/alice/mylib
  owner: github.com/alice (not in [trust] owners)
  note: models often invent repository names, and attackers register the names models invent
  hint: if the user asked for this exact repository, run:
          almide add alice/mylib@v1.2.0 --trust github.com/alice
  hint: if you chose this name yourself, do not trust it; use a package from github.com/almide or ask the user
```

- `--trust <owner>` must repeat the owner the source resolves to (`host/owner`,
  or `owner` for github.com); any other owner is an error. It writes the owner
  into `[trust] owners`.
- `--dry-run` prints the resolution, the trust verdict and whether the
  repository exists, then writes and fetches nothing (exit 1 when the add
  would be refused).
- A repository that does not exist is reported as such. On a refusal or a
  not-found, and only then, a near name is suggested with a runnable line
  (`hint: did you mean \`almide add csv\`?`): an almide package of the same
  name under another owner, an almide package within edit distance 2 (and a
  third of the name) after folding case, `-`, `_`, `.` and an `almide-`
  prefix, or an owner that is a near miss of `almide`. It never substitutes.

**A transitive or hand-written dependency** from an untrusted owner stops the
build with the chain that reached it, and `almide trust <owner>` is the one
step that grants it:

```text
error: dependency `util` (required by mylib → util) is fetched from an owner this project does not trust
  source: https://github.com/bob/util
  owner: github.com/bob (not in [trust] owners)
  ...
  hint: if this dependency is expected, run `almide trust github.com/bob` (it adds "github.com/bob" to [trust] owners in almide.toml)
```

**A source changed under the lock** is refused: when a direct dependency's
lock entry names another `git` than the manifest does, the build stops with
`` `csv` is locked to <old> but almide.toml now says <new> `` and the hint
`almide update csv`, which re-locks it from the new source (a tag-pinned
dependency included). It used to re-resolve silently.

**`--locked`** on `build`, `run` and `test` refuses, before any network
access, a direct git dependency that `almide.lock` does not name, refuses a
resolution that would change the lock, and writes no lock. Use it in CI.
(The lock records direct dependencies only; a transitive one is resolved by
its ref, as before.)

**The cache is keyed by content.** A package tree lives under
`~/.almide/cache/trees/<sum>/`, named by its tree hash (§6.5), and the index
that maps a source's commit to a sum is keyed by the sha256 of the canonical
source, so a package named `svg` fetched from `github.com/evil/svg` can never
answer for `github.com/almide/svg`: their content hashes differ, and the
tree is re-hashed on every use. Directories of older layouts are never read;
`almide clean` removes them. One fetch of a `(source, commit)` runs at a
time, under a lock file beside its index entry, so concurrent `almide`
processes and threads fetching one dependency run one fetch and the others
reuse its tree.

Trust is intent, so it lives in `almide.toml`; the lock records facts and
carries no trust bit.

Tests: `tests/dependency_trust_test.rs`, the `trust`, `git_source` and
`project_fetch` unit tests.

### 6.4 Native code from a dependency needs a grant: `[permissions] native`

A dependency's native code is compiled and run by cargo during the build,
before and outside every `[permissions]` check (#4093). It reaches the build
three ways:

- its `native/*.rs` modules, and the files in `native/` subdirectories, are
  copied into the generated crate;
- its `[native-deps]` and `[target.<platform>.native-deps]` crates are added
  to the generated Cargo.toml, so their `build.rs` and proc macros run;
- an `@extern(rust, "path", "fn")` fn is emitted as a call to `path::fn`, for
  any Rust path (`std` included, with no `native/` at all), and an
  `@extern(c, …)` fn links a C symbol;
- an `@inline_rust("…")` fn's template is spliced into the crate as Rust text
  (#4100). `@intrinsic` / `@wasm_intrinsic`, which E085 refuses outside the
  stdlib, are listed too if a package carries them.

So `almide build`, `almide run`, `almide test` and `almide bench` refuse a
dependency, direct or reached through another dependency, that brings any of
these, unless the root `almide.toml` names it in `[permissions] native`. The
refusal happens before anything is compiled, on every `--target` (the rule
belongs to the dependency graph, like `[permissions]` itself, not to one
target), and exits 1:

```text
error: dependency `nat` brings native code that ./almide.toml does not grant
  nat 0.1.0 at ../nat
  native code: native/natval.rs; [native-deps] itoa; @extern fn `number` (src/mod.almd:2)
  note: native code is compiled and run by cargo during the build, before and outside every [permissions] check
  hint: if you trust `nat`, grant it in ./almide.toml:
    [permissions]
    native = ["nat"]
```

```toml
[permissions]
native = ["nat"]
```

The grant names a package, not a version, and covers only that package:
granting a direct dependency does not grant the native code of what it pulls
in. Only the root manifest's grants count; a dependency's own
`[permissions] native` is not read. The root package's own `native/`,
`[native-deps]` and `@extern` fns need no grant.

Every native build prints one line for each granted dependency that brings
native code:

```text
note: building native code from `nat` (granted by [permissions].native): native/natval.rs; [native-deps] itoa; @extern fn `number` (src/mod.almd:2)
```

`almide deps` shows, for every dependency (direct, then the ones reached
through them), its version, its capability set (the categories
`[permissions].allow` is checked on, where an `@extern` fn counts as every
category), the native code it brings and whether that code is granted:

```text
nat = path ../nat
  version: 0.1.0
  capabilities: Abort, Env.read, Env.write, FS.read, FS.write, Fan, IO.stderr, IO.stdin, IO.stdout, Net.fetch, Net.listen, Proc, Rand, Time
  native code: native/natval.rs; [native-deps] itoa; @extern fn `number` (src/mod.almd:2) (granted by [permissions].native)
```

Tests: `tests/native_grant_test.rs`.

### 6.5 Sources, the fetch and the package format (ADR-0035)

ADR-0035 removes the second reader of every dependency string: a source is
read once into a typed value, nothing but one typed launcher starts a process
on the fetch path, a package is a tree of regular files, and the lock holds
its tree hash.

**Sources (D1).** A `git =` value — in the root manifest, a dependency's
manifest, the lock, `almide add --git` or `almide tool install` — is read once
into

```text
GitSource = { transport: Https | Ssh, user: Segment?, host: Host, port: 1..65535?, path: [Segment] (1 or more) }
Host      = lowercase ASCII DNS name, 1..253 bytes, labels [a-z0-9-], not starting or ending with "-"
Segment   = [A-Za-z0-9_] [A-Za-z0-9._-]{0,99}   -- never starts with "-" or ".", never "." or ".."
```

Any git host, any path depth, a port, `https://`, `ssh://` and the scp-like
`user@host:owner/repo` are kept. The scp-like form keeps git's meaning — a
path relative to the remote user's home, a typed `root: HomeRelative` that is
part of the identity — and is printed in its own form; `user@host:/a/b` is
the absolute `ssh://user@host/a/b`;
a `.git` suffix is not part of the identity. Refused, each on its manifest
line with a diagnostic that prints the canonical form when there is one: a
component starting with `-`; a `?` or `#`; a password or token in the user
part, and any user part on https; any scheme but https and ssh (`git://`,
`http://`, `file://`, `ext::`); control, bidi or whitespace characters, raw
or percent-encoded (any `%` escape at all); an empty or dot-only segment; an
IP-literal host; a Windows reserved name. The fetch URL, the ssh argv, the
cache key and the lock's `git` are all serialized from the typed value, never
from the text. A `tag` or `branch` is read the same way into a ref name — `/`-separated
segments of that grammar (`v1.2.0`, `feat/x`) — and anything else (an
absolute path, `..`, a leading `-` or `.`) is refused on its manifest line
(#4838); a ref is never part of a path.

**The fetch (D2).** An https source is fetched by an in-process git client
(`src/fetch/`): smart HTTP over rustls (git protocol v2, v0 as fallback), one
shallow fetch of the one commit, every object of the pack re-hashed, so the
tree is bound to the commit id. It honours `HTTPS_PROXY` / `NO_PROXY` and
`SSL_CERT_FILE` / `SSL_CERT_DIR`, follows no redirect (a moved repository is
reported with its new location) and sends no credentials. An ssh source
starts exactly one program, `ssh -o SendEnv=GIT_PROTOCOL [-p PORT] [-l USER]
-- HOST git-upload-pack '/a/b'` (`'a/b'` for a home-relative source), built
only from typed fields; host-key
checking is the user's ssh configuration. Nothing else on the fetch path
starts a process.

**The package format (D3).** A fetched commit becomes a tree of regular files
before anything is written. Refused as a package-format error naming the
entry: a symlink, a submodule, anything else that is not a regular file; a
name outside `[A-Za-z0-9._-]{1,100}` (not starting with `-`, not `.`/`..`,
not `.git`, not a Windows reserved name — a leading `.` is kept, for
`.gitignore`); two paths equal under case folding. Budgets, fixed (no
manifest key, lock entry, environment variable or flag raises them):

| Budget | Value | Measured (2026-11-19, every almide-org repository, every tag and HEAD) |
|---|---|---|
| bytes per file | 16 MiB | largest library file 0.2 MB |
| bytes per package | 64 MiB | largest library 3.4 MB (porta) |
| files per package | 10 000 | largest library 359 (porta) |
| path depth | 32 | deepest library 5 |

The tree is written with directories and new regular files only.
`package_files(root)` is the one enumeration of a package on disk: the tree
hash, the module resolver and the native / media grant scans all walk with
its primitive, which never follows a link — in a dependency's cache entry a
link is tampering and is refused; in a local package it is skipped.

**The tree sum (D4).** `sum = "h1:<base64 sha256>"` over the lines
`sha256(file) + "  " + RelPath + "\n"` in RelPath order — the shape of Go's
`h1:` dirhash. It is computed when the tree is fetched, recorded in the lock,
and **checked on every use, a cache hit included**: a cache entry that no
longer hashes to its sum (a file edited, added or removed, a link planted) is
refused in cache-only mode (LSP, `almide ide`, `almide mcp` tools) and
otherwise removed and fetched again, with a warning. A fetched tree whose sum
is not the lock's is refused: the remote served other content for the locked
commit.

**Migration.** The refused source forms are refused outright, with no
dialect-epoch row; a scan of every almide-org manifest and lock (2026-11-19)
found none. A lock without `sum` still reads, and the next run that resolves
it writes the sum (read old, write new); `--locked` refuses a lock entry with
no `sum`. Cache directories of the older layouts are not read; `almide clean`
removes them.

Open in the ADR's trust boundary: TLS and its roots, the user's ssh host
keys, the in-process client, sha256, the host on the first fetch, and the
package author (held by `[permissions]`, not here). A private https
repository is not reachable (no credentials are sent): an https answer of
401 or 403 is refused with the ssh form to write instead
(`` git = "git@github.com:owner/repo" ``). Reading `~/.netrc` keyed by the
exact `(transport, host, port)` is a later step.

Tests: `tests/dep_source_hardening_test.rs` (every refused form in the root
and in a dependency's manifest; a non-hash lock commit and sum),
`tests/dep_package_format_test.rs` (a symlink, a submodule, `..`, `.git`, a
`-` name, a bidi name, a case collision and an over-budget file, each refused
naming the entry with nothing cached; a cache entry edited, extended, emptied
or given a link, refused cache-only and fetched again online; a lock sum that
does not match; `--locked` and a lock without sums),
`tests/fetch_spawn_gate_test.rs` (the spawn gate and its negative controls,
and an scp-like and an `ssh://` source fetched through a stub `ssh`, each
argv checked exactly), the
`git_source`, `package_tree` and `fetch` unit tests. Every dependency test is
served by `tests/support/git_host.rs`, an in-test https git host reached
through `HTTPS_PROXY` and `SSL_CERT_FILE`.

## 7. Lock File

```toml
# almide.lock — auto-generated, do not edit

bindgen = { git = "https://github.com/almide/almide-bindgen", ref = "v0.1.0", commit = "a629eded8d20…", sum = "h1:…" }
json = { git = "https://github.com/almide/json", ref = "v2.0.0", commit = "b8f3a1…", sum = "h1:…" }
ceangal = { git = "https://github.com/almide-graphics/ceangal2", ref = "v0.1.0", commit = "c41d…", sum = "h1:…", subdir = "ceangal" }
```

Every field is typed on read (§6.5): `git` through the source grammar
(written back in canonical form), `commit` as 40 or 64 lowercase hex, `sum`
as an `h1:` tree sum. Any other value refuses the lock and names the entry.

An entry for a `subdir` dependency (§6.1) records the normalized `subdir`, so
the lock names the package exactly; entries without one are written exactly as
before. Two packages of one repository may share `(git, ref, commit)` — the
pin is a fact about the repository at that ref, so it is looked up by
`(git, ref)` as for any dependency, and both packages build from the same
commit.

One entry per direct dependency, one line each; the writer never emits a name
twice. A lock that holds a name twice (written by a compiler before #2583
from a manifest that declared the dependency twice) is refused with the two
lines and the way out — delete one of them, keeping the one whose `git`
matches `almide.toml`, or delete the lock and let the next run rewrite it.

**The lock records the resolved result, not the request** (#2532). Each entry's
`ref` and `commit` are those of the version Minimal Version Selection chose for
that dependency's package — the checkout that was actually built — as
`Cargo.lock` does. The request lives only in `almide.toml`. When nothing raises
a dependency the two coincide. When a transitive requirement raises it (the
manifest asks `foo@v1.0.0`, a dependency asks `foo@v1.1.0`), the entry reads
`ref = "v1.1.0"` with v1.1.0's commit, even though `almide.toml` still says
`v1.0.0`; that is not drift and does not re-resolve. Every entry is one fetch's
own `(git, ref, commit)`, so it is always a triple that was true together.

A pin applies to any request for the same source at the same ref, whoever in
the graph makes it, so the locked commit is exactly what the next build
compiles. A request that was walked but not selected (the `v1.0.0` above) is
resolved from its ref — MVS reads its manifest for requirements, it does not
build it. Changing a request in `almide.toml` re-resolves as before, and the
lock is rewritten from the new selection. A lock written before #2532, which
recorded the root's request, still reads; the next run rewrites the raised
entries to the resolved record.

Test: `tests/manifest_duplicate_key_test.rs`, `tests/lock_roundtrip_test.rs`,
`tests/dep_lock_pairing_test.rs`
(`a_dependency_raised_by_mvs_is_locked_at_the_version_that_was_built`).

## 8. Resolution Algorithm

```text
1. Parse almide.toml → direct dependencies
2. For each dep:
   a. If almide.lock has an entry for this (git, ref) → use its exact commit,
      and its sum: the cached tree named by the sum (no network), re-hashed
   b. Else a tag → its commit from the index (tags never move), else from one
      ref listing; then that commit's tree, cached or fetched
   c. Else a branch, or no ref (the remote's default branch) → ask the remote
      for its current head (one ref listing) and use that commit's tree; the
      cache is not trusted for a ref that moves. A remote that cannot be
      reached falls back to the commit the ref last resolved to, with a
      warning naming it, and that commit is not written to the lock
   d. A ref the remote does not have is an error naming the dependency, the
      ref, the chain from the root that requires it and the newest tags
   e. With `subdir`, the package is that directory of the checkout (§6.1)
3. Parse dep's almide.toml → transitive dependencies
4. Recurse (depth-first, leaves first)
5. Dedup by PkgId(name, major):
   - Same (name, major): keep maximum requested version (MVS)
   - Different major: both coexist with versioned names
6. Detect impossible constraints → error with explanation
7. Write almide.lock: per direct dependency, the (git, ref, commit, sum) of
   the version step 5 selected for its package
```

## 9. Error Messages

```text
error: version conflict for package 'json'
  → myapp requires json >= 2.0.0 (via almide.toml)
  → bindgen requires json >= 1.0.0, < 2.0.0 (via bindgen/almide.toml)

  json v1.x and v2.x are different major versions and will coexist.
  However, json.Config from v1 cannot be passed to functions expecting json.Config from v2.

  hint: Update bindgen to a version that supports json v2.x,
        or add json v1.x as a separate dependency.
```

## 10. Future: Almide Package Registry

The registry will:
- Host packages with semver metadata
- Provide `almide add pkg` without git URLs
- Support `almide publish` for package authors
- Use content-addressed storage (like pnpm) for dedup
- Enforce package signing and provenance (like JSR)

The current git-based system is forward-compatible:
- `PkgId` already supports semver
- `almide.lock` already records exact commits
- `versioned_name` already supports coexistence
- Migration: `git = "..."` → `registry = "almide"` (or just name)

## References

- [Go Modules: Minimal Version Selection](https://research.swtch.com/vgo-principles)
- [How Rust Solved Dependency Hell](https://stephencoakley.com/2019/04/24/how-rust-solved-dependency-hell)
- [PubGrub: Next-Generation Version Solving](https://nex3.medium.com/pubgrub-2fb6470504f)
- [pnpm: Flat node_modules is not the only way](https://pnpm.io/blog/2020/05/27/flat-node-modules-is-not-the-only-way)
