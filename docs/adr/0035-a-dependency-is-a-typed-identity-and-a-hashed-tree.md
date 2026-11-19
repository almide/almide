# ADR-0035: A dependency source is read once into a typed value — trust and the fetch are both derived from it, no process starts except one typed ssh launcher, the content is a tree of regular files, and the lock holds a tree hash checked on every use

- **Status**: Accepted (design), 2026-11-19. The owner answered ○ to the [Owner decisions](#owner-decisions-) after a measured comparison with other package systems. Implementation starts now, not after v0.70.0. The minimal fixes for the audited instances (#4810, #4811, #4812, #4813) land first, on bugs-final.
- **Date**: 2026-11-18 (draft), 2026-11-19 (accepted)
- **Scope**: how a dependency is named (`almide.toml`), found, fetched, unpacked, cached, locked (`almide.lock`) and walked; `src/project.rs`, `src/project_fetch.rs`, `src/trust.rs`, `src/resolve.rs`; security-table rows 14–16 and 25.
- **Related**:
  - `docs/specs/package-system.md` §6.3 (`[trust] owners`), §6.4 (`[permissions] native`), §7 (lock file), §10 (future registry).
  - `docs/roadmap/active/security-by-construction.md` rows 14, 15, 16 and 25.
  - The 2026-11-18 pre-release audit:
    - #4810 (an option-shaped git source runs a command, from the root manifest or a fetched dependency's own);
    - #4811 (`?@` / `#` passes trust while git contacts another host);
    - #4812 (a lock `commit` joined into a path unvalidated);
    - #4813 (dependency symlinks followed out of the package);
    - #4809 (the native-grant scan and the resolver walked different file sets).
  - ADR-0030 (compile cost is a contract): the package-format budgets follow its form.

## Context

Today a dependency is fetched like this:

1. `almide.toml` gives `git = "<url>"` with `tag` / `branch`. A dependency's own manifest can do the same.
2. `trust::source_owner(url)` derives an owner from the URL string.
3. `project_fetch.rs` runs the `git` program eight times, with arguments built from the URL, the ref and the lock's commit.
4. The checkout is cached under a path built from the URL. The resolver walks it as a directory tree.
5. `almide.lock` records `(git, ref, commit)` as free strings.

Each audited class sits where **one string is read by two readers that disagree**:

| Class | First reader | Second reader | How they disagree |
|---|---|---|---|
| #4810 option injection | the manifest parser: "a URL" | git's argv parser: "an option" | `--upload-pack=…` |
| #4811 trust bypass | `source_owner`: owner after `@` | git's URL parser: host before `?` | `https://evil?@github.com/almide/x` |
| #4813 symlink escape | the fetch: "files of the package" | the OS path walk: "follow the link" | `src/ext -> /` |
| #4812 lock path | the lock writer: "a commit id" | `Path::join`: "a path" | `commit = "/home/…"` |
| #4809 | the native-grant scan: "these files" | the resolver: "those files" | different directory walks |

Patching each meeting point closes the instance but not the class: every new call site is a new place for the readers to disagree. This ADR removes the second reader.

## Survey: how other package systems handled the same classes (2026-11-19)

Systems compared: Go modules, Cargo, npm, pnpm, Deno/JSR, SwiftPM, Zig, pub, Hex/Gleam, pip/uv, Nix and Bazel. The full comparison, with a citation per claim, is kept with the decision record. The findings that decide this ADR:

- **Naming.** Systems fall into three groups:
  - a path with a fixed grammar (Go, JSR, the SwiftPM registry, Bazel modules);
  - a URL plus a content hash (Zig, Nix, Bazel `http_archive`);
  - an arbitrary URL passed on as a string (Cargo git, npm, pnpm, SwiftPM, pub, Mix, Gleam, pip, uv).

  Every option-injection and credential-scope advisory in the survey is in the third group. One is exactly #4810: pnpm CVE-2026-50014. A lockfile commit reached `git fetch` with no `--`, and the fix was hex validation plus `--`. Others: git CVE-2017-1000117 and CVE-2020-5260, npm CVE-2016-3956, Go CVE-2024-45340, Deno CVE-2024-37150 and CVE-2025-21620. Only Zig feeds trust and the fetch from one parsed value.
- **In-process git is not safe by itself.**
  - libgit2 skipped the SSH host-key check (CVE-2022-46176).
  - gitoxide spawns `ssh`, and an ssh username starting with `-` became an option (CVE-2024-32884); its first fix had checked only the host and the path.

  What helps is that every value reaching a transport or an argv comes from a typed field that cannot be an option.
- **Hashes.** Most systems hash the archive bytes, which leaves git dependencies pinned by commit only. A hash over the canonical file tree (Go's `h1:`, Zig, Nix NAR) covers content from any host. The check needs its own test: Hex's lock check silently never ran across several releases (CVE-2026-32148).
- **Links.** Go's module zip, Cargo (since 1.96) and the npm registry refuse every link and have had no escape advisories of this class. The systems that allow links and check them (pub, Hex, uv, SwiftPM, Zig) keep receiving escape advisories.
- **Size caps.** Go caps 500 MiB, Cargo unpacks up to max(512 MiB, 20 × compressed), Hex 128 MiB unpacked, JSR 20 MB. None caps file count or path depth.
- **Migration.** Security narrowings of exploitable inputs ship as hard refusals in patch releases (Go CVE-2023-45285 and CVE-2024-45340, Cargo 1.96 symlinks, git 2.14.1 dash-leading hosts, pnpm hex validation). Version gates and warning windows are used for language and format changes.

## Decision

### D1. A source is written as a URL on any git host and read once into a typed `GitSource`

```
GitSource = { transport: Https | Ssh, user: Segment?, host: Host, port: 1..65535?, path: [Segment] (1 or more) }
Host      = lowercase ASCII DNS name, 1..253 bytes, labels [a-z0-9-], not starting or ending with "-"
Segment   = [A-Za-z0-9_] [A-Za-z0-9._-]{0,99}   -- never starts with "-" or ".", never "." or ".."
```

- **Kept:** any git host, self-hosted included; paths of any depth (GitLab subgroups); a port; `https://`, `ssh://` and the scp-like `user@host:path`, which is normalized into the same value; an ssh user that fits `Segment`; a `.git` suffix, normalized away.
- **Refused with a diagnostic that prints the canonical form**, when it has one:
  - any component that starts with `-`;
  - a query or a fragment;
  - a password or token in userinfo, and any userinfo on https;
  - any scheme other than https and ssh (`git://`, `http://`, `ext::`, `file://` for remote sources);
  - control characters, newline, NUL, bidi controls or whitespace, raw or percent-encoded;
  - an empty segment, a dot-only segment, or a Windows reserved name.
- **Trust is decided on `(host, path prefix)`, matching whole segments.** `[trust] owners` entries are parsed with the same grammar. The fetch URL, any argv and the cache path are serialized **only from the `GitSource`**. Credentials are keyed by the exact `(transport, host, port)`.
- `path =` dependencies are unchanged: local, and exempt from trust, as today.

### D2. No process starts on the fetch path, except one typed ssh launcher

- **https** sources are fetched by an in-process git smart-HTTP client.
- **ssh** sources start exactly one program, `ssh`, through one function whose only input is a `GitSource`. Its argv is built from typed fields that cannot start with `-`, with `--` before the positional arguments. Host-key checking stays on.
- **Gate.** A static check that the fetch modules contain no process spawn except the ssh launcher, and that the launcher takes no `String`. Negative control: adding a `Command::new("git")`, or a `String` parameter to the launcher, turns it red.
- The D4 tree hash means a transport bug cannot substitute content: a different tree fails its sum.
- A fetch proxy (content-addressed archives, a checksum log) and a name registry are later steps, each with its own ADR.

### D3. Package content is a tree of regular files, and the format cannot express anything else

- Fetched content becomes a typed tree `[(RelPath, Bytes)]` before anything touches the filesystem. A `RelPath` is a list of `Segment`s.
- **Refused** as a package-format error naming the entry:
  - a symlink, a hard link, a submodule (gitlink) or a device;
  - an absolute path, `..`, or a segment that fails the grammar;
  - two paths equal under case folding.
- **Budgets** (ADR-0030 form: counted, fixed, not raisable by the source):

  | Budget | Value | Precedent |
  |---|---|---|
  | bytes per file | 16 MiB | Go's go.mod cap |
  | bytes per package | 64 MiB | between JSR and Hex |
  | files per package | 10 000 | no precedent; to be measured against every almide-org package before landing |
  | path depth | 32 | no precedent; to be measured against every almide-org package before landing |

- The unpacker writes only regular files under the package root, and cannot create a link.
- Every reader of a dependency uses **one enumeration**, `package_files(root)`: the native-grant scan, the module loader, `almide test` discovery and the LSP. Inside a dependency root it never follows a link. A link met in the cache is tampering, and is refused.

### D4. The lock holds typed values and a tree hash, checked on every use

```toml
json = { git = "https://github.com/almide/json", ref = "v2.0.0", commit = "<40 or 64 lowercase hex>", sum = "h1:<sha256 of the canonical tree>" }
```

- `git` is re-read through D1. `commit` is a `CommitId`: 40 or 64 lowercase hex, parsed on read.
- `sum` is a sha256 over the canonical tree: sorted lines of `sha256(file) + "  " + RelPath + "\n"`, which is the shape of Go's `h1:` dirhash.
- Any other value in these fields refuses the lock and names the line.
- The cache path is computed from `sum`, never from a lock string.
- **The sum is checked on every use, including a cache hit.** A changed cache entry is refused and fetched again. A negative control tampers with one cached file.
- If re-hashing on every use is measured as too slow, the fallback (Go's model) needs its own owner ○: hash on download, read-only cache files, and an explicit `almide deps verify`.

### D5. Nothing in a dependency runs at fetch, check or test-discovery time

This holds today: there are no build scripts, and native code needs `[permissions] native`. The ADR makes it a gated contract, through the D2 spawn check and the D3 single enumeration.

## What stays trusted

- TLS and the certificate chain; SSH host keys.
- The in-process git client library.
- sha256.
- The host, on the first fetch. After that, the lock's `sum` holds the content.
- **The package author.** A malicious package's own code is held by the capability system (`[permissions]`, effect inference), not by this ADR.

## Gates and negative controls (each lands with its part)

| Part | Gate | Negative control |
|---|---|---|
| D1 | `GitSource` grammar tests over the manifest, lock and `[trust]` readers; trust consumes `GitSource` | `--upload-pack=…`, `?@`, `#`, `user:pw@`, `git://`, a `-x` segment and a bidi segment in root and dependency manifests are each refused; restoring the string-based `source_owner` turns them red |
| D2 | static spawn check over the fetch modules | adding a git spawn, or a `String` argument to the ssh launcher, turns it red |
| D3 | package-format tests | fixture trees with a symlink, `..`, an absolute path, a case collision, a bidi segment or an over-budget file are each refused; a cached link is refused |
| D3 | one-enumeration test | the native-grant scan, the loader and test discovery report the same files on a fixture tree (extends #4809's test) |
| D4 | lock-type tests; sum checked on a cache hit | a lock `commit` that is a path, and one tampered cache file, are each refused |

## Migration

- **Track 1: refuse outright.** The refused forms of D1 are refused in a normal release, with no dialect-epoch row. The diagnostic prints the canonical form. Before landing, a scan confirms that no almide-org package uses a refused form.
- **Track 2: read old, write new.** Locks without `sum` still read, and the next fetch adds `sum`. `--locked` refuses a lock that has no `sum`. The cache moves to a `sum`-keyed layout; the old tree is left alone and removed by `almide clean`.
- **Track 3: warning or epoch row.** Kept only for future narrowings that are not security fixes.

## Owner decisions (○/×)

- **2026-11-18 (draft):** 1 ×, because identities only would lose URLs on other hosts; 2–5 to be decided by comparison with other languages; 6 ×, implement now.
- **2026-11-19, all ○, after the comparison:**
  1. URLs stay, on any git host, read once into `GitSource`. Only the listed dangerous forms are refused.
  2. No process starts except one typed ssh launcher; https goes through an in-process client.
  3. The lock `sum` is a tree hash, checked on every use.
  4. The package-format rules are adopted. Byte budgets are 16 MiB per file and 64 MiB per package. The file-count and depth budgets are measured against the almide-org packages before landing.
  5. Migration follows the three tracks above.
  6. Implementation starts now, after the bugs-final minimal fixes.
