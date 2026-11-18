# ADR-0035: A dependency is a typed identity and a hashed tree of regular files — fetching runs no subprocess, trust is judged on the value that is fetched, and the lock holds only hashes

- **Status**: Proposed (draft). 2026-11-19 owner answers: decision 1 ×, decisions 2–5 to be settled by a measured comparison with other languages' package systems, decision 6 × (implement now, not after v0.70.0). The minimal fixes for the audit findings land before v0.70.0 on bugs-final.
- **Date**: 2026-11-18
- **Scope**: how a dependency is named (`almide.toml`), found, fetched, unpacked, cached, locked (`almide.lock`) and walked; `src/project.rs`, `src/project_fetch.rs`, `src/trust.rs`, `src/resolve.rs`; security-table rows 14–16 and 25.
- **Related**:
  - `docs/specs/package-system.md` §6.3 (`[trust] owners`), §6.4 (`[permissions] native`), §7 (lock file), §10 (future registry).
  - `docs/roadmap/active/security-by-construction.md` rows 14 (typo'd dependency), 15 (dependency code at build time), 16 (compromised CI action) and 25 (the toolchain acts on an untrusted project).
  - The 2026-11-18 pre-release audit found four classes on develop `4b61cc720c`. v0.69.1 behaves the same.
    - **Option injection.** A git source written as an option (`--upload-pack=…`) runs a command at `check`, `test` or `deps` time. This works from the root manifest and from a fetched dependency's own `almide.toml`.
    - **Trust judged on a different string.** `https://other.invalid?@github.com/almide/json`, and the `#` form, pass the trusted-owner check, but git then contacts `other.invalid`.
    - **Symlinks.** The walker follows a symlink inside a dependency out of the package. A symlink to `/` hangs `almide check`.
    - **Lock values used as paths.** A lock `commit` written as an absolute path is joined and used as the package directory.
  - #4809: the native-grant scan and the resolver walked different file sets.
  - ADR-0030: compile cost is a contract. The package-format budgets below follow its form.

## Context

Today a dependency is fetched like this:

1. `almide.toml` gives `git = "<url>"` with `tag` / `branch`. A dependency's own manifest can do the same for its dependencies.
2. `trust::source_owner(url)` derives an owner from the URL string. It refuses the URL unless that owner is `github.com/almide` or is listed in the root's `[trust] owners`.
3. `project_fetch.rs` runs the `git` program as a subprocess eight times with arguments built from the URL, the ref and the lock's commit: `ls-remote`, `clone`, `checkout`, `rev-parse`.
4. The checkout lands in `~/.almide/cache/.sources/<scheme>/<host>/<owner>/<repo>/@/<commit>`. The resolver then walks it as a directory tree.
5. `almide.lock` records `(git, ref, commit)` as free strings.

Each of the four classes the audit found sits where a **string is read twice by two different readers**:

| Class | First reader | Second reader | How they disagree |
|---|---|---|---|
| Option injection | the manifest parser: "a URL" | git's argv parser: "an option" | `--upload-pack=…` |
| Trust bypass | `source_owner`: owner after `@` | git / curl URL parser: host before `?` | `https://evil?@github.com/almide/x` |
| Symlink escape | the fetch: "files of the package" | the OS path walk: "follow the link" | `src/ext -> /` |
| Lock path | the lock writer: "a commit id" | `Path::join`: "a path" | `commit = "/home/…"` |
| #4809 | the native-grant scan: "these files" | the resolver: "those files" | different directory walks |

Patching each place where the two readers meet closes the instance we found. It does not close the class, because a new call site is a new place for the readers to disagree. This ADR removes the second reader.

## Survey: how other package systems closed the same classes

These are drawn from published advisories and design documents. Before acceptance, each row is to be checked against its primary source.

| System | Identity | Fetch | Content form | Lock | What it learned |
|---|---|---|---|---|---|
| **Go modules** | module path with a fixed grammar (`module.CheckPath`): lowercase host, restricted characters, no `..`, no leading `-` or `.` | through a module proxy over HTTPS by default. Direct VCS is restricted per path pattern (`GOVCS`), and the command line passes the VCS no user string it could read as an option | a module **zip** with fixed rules: no symlinks, no absolute paths or `..`, no case-folding collisions, size and file-count caps | `go.sum`: a hash of the zip and of `go.mod` per version, checked against a public append-only checksum log | `go get` once ran code through cgo flags (CVE-2018-6574). Fetching and building were then separated, and the cgo flags put on an allow-list |
| **Cargo** | registry name, or `git = url` | registry: `.crate` tarballs over HTTPS, using an index | tarball; unpacking rejects paths outside the target | `Cargo.lock` with a `checksum` (sha256) per registry package | build scripts run at build time, which is a known open risk. Unpacking has had path-escape advisories |
| **npm / pnpm** | registry name | tarball over HTTPS | tarball; pnpm stores files content-addressed | `integrity` (SRI sha512) per package | install scripts run by default, the largest source of incidents; `--ignore-scripts` is the opt-out |

What they agree on:

- **One parser for the identity.** The fetch location is derived from the parsed identity, never taken from the user as a URL.
- **No VCS subprocess on the default path.** Content arrives as an archive with its own rules.
- **The archive format cannot express a link or an escape.** Validation does not run after unpacking: the unpacker only knows how to write regular files under the root.
- **The lock holds a content hash.** It is checked whenever the content is used, not only when it is downloaded.
- **Nothing in a dependency runs while it is fetched or checked.** Almide already holds this (no build scripts; native code needs `[permissions] native`).

## Decision

### D1. A dependency's identity is a typed value, parsed once

```
Identity  = Host "/" Owner "/" Repo [ "/" Subdir ]
Host      = DNS name, lowercase, 1..253 bytes, labels [a-z0-9-] not starting or ending with "-"
Owner     = Segment
Repo      = Segment
Subdir    = Segment { "/" Segment }
Segment   = [a-z0-9] [a-z0-9._-]{0,99}     -- no leading "-" or ".", never "." or ".."
```

- `almide.toml` and `almide.lock` hold an `Identity`, never a URL. Today's `git = "https://github.com/almide/json"` still reads: it is parsed into the identity `github.com/almide/json`. A value that does not parse is refused with a diagnostic naming the identity form. That covers a query, a fragment, userinfo, a port, a scheme other than `https`, and anything starting with `-`.
- **Trust is judged on the `Identity`**, as `Host/Owner`. The fetch location is computed from that same value (D2). The string trust reads and the string the network reads are therefore one value.
- `path =` dependencies are unchanged: local, and exempt from trust, as today.

### D2. Fetching runs no subprocess

The fetch path (`add`, `update`, `build`, `run`, `test`, `bench`, `check`, `deps`, the LSP) starts no process. Content comes over HTTPS from a location computed from the `Identity` and an exact commit. How refs resolve to commits and how the tree is fetched is [Owner decision 2](#owner-decisions-):

- **(A) An in-process git client** (a Rust library speaking git's smart-HTTP protocol). Any git host works as today. There is no argv to inject into. The client library joins the trusted base.
- **(B) Host archive endpoints.** A fixed table maps a host to its tarball and ref-listing endpoints. This is simple, but only the hosts in the table work.
- **(C) An Almide module proxy** (registry §10): content-addressed archives, served and signed. This is the long-term end state, and it needs hosting.

Whichever is chosen, the gate is a check that the fetch modules contain no `Command::new` or equivalent. Its negative control re-adds one and the check turns red.

### D3. Package content is a tree of regular files, and the format cannot express anything else

The fetched content is turned into a typed tree before anything touches the filesystem: entries of `(RelPath, Bytes)`, where `RelPath` is a list of `Segment`s by the D1 grammar.

The tree is refused, as a package-format error naming the entry, when it holds any of:
- a symlink, a hard link, a submodule (gitlink) or a device;
- an absolute path, `..`, or a segment that fails the grammar (control characters, bidi controls and NUL included);
- two paths equal under case folding.

**Budgets** (ADR-0030 form: counted, fixed, not raisable by the source):
- files per package;
- bytes per file and per package;
- path depth.

The values are [Owner decision 4](#owner-decisions-).

The unpacker writes only regular files under the package root. It cannot create a link, because no tree entry can hold one. The resolver walks a dependency through **one enumeration**, `package_files(root)`. The native-grant scan, the module loader and `almide test` discovery all consume it (#4809 generalised). Inside a dependency root it does not follow a link; if it meets one in the cache, that is tampering and it refuses.

### D4. The lock holds only typed hashes, and the cache is keyed by them

```toml
json = { id = "github.com/almide/json", ref = "v2.0.0", commit = "b8f3a1…40 hex…", sum = "sha256:…64 hex…" }
```

- `commit` is a `CommitId`: 40 or 64 lowercase hex, parsed on read. `sum` is the sha256 of the canonical encoding of the D3 tree. Anything else in either field refuses the lock with the line.
- The cache path is computed from `sum`, never from a lock string: `~/.almide/cache/pkg/sha256/<sum>`.
- **The sum is checked on every use**, including a cache hit. A changed cache entry is refused and fetched again, never used.
- An old lock without `sum` still reads. The next fetch records `sum`, and `--locked` refuses a lock that has no `sum`.

### D5. Nothing in a dependency runs at fetch, check or test-discovery time

Almide already holds this. The ADR states it as a contract so it has a gate: the D2 no-subprocess check, plus the existing `[permissions] native` rule (row 15). Every command that reads a dependency reads it through `package_files`, so no command walks a different set of files.

## What stays trusted

- TLS and the host's certificate chain; the host serving the bytes for a commit. D4's `sum` turns that into trust on first fetch: later fetches are checked against the lock.
- sha256.
- Under D2 (A), the in-process git client library.
- **The package author.** A malicious package's code is held by the capability system (`[permissions]`, effect inference), not by this ADR.

## Gates and negative controls (each lands with its part)

| Part | Gate | Negative control |
|---|---|---|
| D1 | identity-grammar tests over the manifest and lock readers; the trust check consumes `Identity` | `--upload-pack=…`, `?@`, `#`, userinfo, port and `-x` in root and dependency manifests are each refused; restoring the string-based `source_owner` turns them green-to-red |
| D2 | static check that the fetch modules start no process | adding a `Command::new` to the fetch module turns the gate red |
| D3 | package-format tests | a fixture archive with a symlink, `..`, an absolute path, a case collision, a bidi segment, or an over-budget file is refused; the walker given a cached link refuses |
| D3 | one-enumeration test | the native-grant scan, the loader and test discovery report the same file set on a fixture tree (#4809's test extended) |
| D4 | lock-type tests; cache-hit sum check | a lock `commit` that is a path, and a changed cache file, are each refused |

## Migration

- The manifest needs no rewrite: `git = "https://host/owner/repo"` parses into an `Identity`. Forms that no longer parse are refused with the identity they should be. Whether this needs a `proofs/dialect-epochs.toml` row is [Owner decision 5](#owner-decisions-).
- The lock gains `sum` and `id`. Old locks read, and are rewritten on the next fetch.
- The cache moves to `pkg/sha256/<sum>`. The old `.sources` tree is left alone, and `almide clean` removes it.

## Owner decisions (○/×)

Answered 2026-11-19: 1 ×; 2, 3, 4 and 5 pending a measured cross-language comparison; 6 × (implement now).

1. **D1: identities only, no URLs in the manifest or lock.** A non-https scheme, a port and userinfo are refused. Recommended ○.
2. **D2: how to fetch without a subprocess.** Recommended (A), the in-process git client, now; (C), the Almide proxy, as a later ADR with the registry.
3. **D4: `sum` in the lock, checked on every use, including cache hits.** Recommended ○.
4. **D3: budget values.** Proposed: 10 000 files, 16 MiB per file, 64 MiB per package, depth 32. Recommended ○ with these values, to be re-measured against every almide-org package before landing.
5. **Migration: forms that no longer parse are refused without a dialect-epoch row.** They were never valid identities, and none appears in the almide org's packages. Recommended ○, after a scan of those packages confirms it.
6. **Timing: after v0.70.0.** The pre-release fixes close the four audited instances. This ADR closes their classes. Recommended ○.
