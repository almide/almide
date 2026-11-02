<a id="module-system-specification"></a>

# Module System Specification

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](module-system.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](module-system.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](module-system.zh-CN.md)

> Non-normative fixed reader translation. The pinned original remains authoritative; no automatic synchronization or independent execution/security audit is implied. The last-updated date and test counts are historical source statements. Extension-method comparisons and native/wasm equality are source claims, not newly verified comparisons. Region host inputs retain their stated trust and call-lifetime limits; a safe wrapper does not certify arbitrary foreign code. The general own-length description and the later host-supplied-length exception are retained without silently reconciling them.
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/module-system.md)


> Last updated: 2026-09-30. Verified by `spec/integration/modules/` (25 tests + 4 error tests), `spec/wasm_cross/stdlib_type_shadow.almd`, `tests/module_shadow_member_test.rs`, `tests/extern_rust_abi_test.rs` and `tests/diagnostics/e090-*`.

---

<a id="1-package-structure"></a>

## 1. Package Structure

```text
mypackage/
  almide.toml              [package] name = "mypackage", version = "0.1.0"
  src/
    mod.almd               パッケージのエントリポイント
    utils.almd             サブモジュール → mypackage.utils
    parser.almd            サブモジュール → mypackage.parser
    bindings/              ディレクトリ = 名前空間（mod.almd 不要）
      python.almd          サブモジュール → mypackage.bindings.python
      go.almd              サブモジュール → mypackage.bindings.go
      internal/
        helpers.almd       サブモジュール → mypackage.bindings.internal.helpers
```

**Rules:**
- `src/mod.almd` is the package's top level. `import mypackage` lets you call `mypackage.func()`
- `.almd` files at the same level (except `mod.almd`, `lib.almd`, `main.almd`) automatically become submodules
- Subdirectories act as namespaces. Even without `mod.almd`, they become intermediate nodes in dotted paths
- Scanning recurses to arbitrary depth
- The package name is defined by `[package] name` in `almide.toml`. No module declaration is needed in source files

---

<a id="2-import"></a>

## 2. Import

<a id="21-構文"></a>

### 2.1 Syntax

```almide project
// file: almide.toml
[package]
name = "myapp"
version = "0.1.0"
// file: src/mod.almd
fn hello() -> String = "hello from myapp"
// file: src/utils.almd
fn helper() -> String = "helper"
// file: pkg/src/mod.almd
fn version() -> String = "1.0"
// file: pkg/src/sub.almd
fn func() -> String = "pkg.sub"
// file: main.almd
import pkg                    // パッケージ全体 → pkg.version()
import pkg.sub                // 特定のサブモジュール → sub.func() で呼べる
import pkg as p               // エイリアス → p.version()
import self                   // 自パッケージの mod.almd → myapp.hello()(almide.toml の name)
import self.utils             // 自パッケージのサブモジュール → utils.helper()

test "every import form resolves" {
  assert_eq(pkg.version(), "1.0")
  assert_eq(sub.func(), "pkg.sub")
  assert_eq(p.version(), "1.0")
  assert_eq(myapp.hello(), "hello from myapp")
  assert_eq(utils.helper(), "helper")
}
```

- `import pkg.sub` can be referenced by the last segment: `sub.func()`
- The last segment is **a binding name local to that file**, not module identity. Module identity is the full path from `src/` (`import self.a.util` is `src/a/util.almd`), so importing `src/a/util.almd` and `src/b/util.almd` from different files via `import self.a.util` / `import self.b.util` resolves to distinct modules (#2654). Importing both in one file collides on the binding `util` and is an error; one needs an alias such as `import self.b.util as butil`.
- Selective import `import pkg.{func}` binds only the specified name without qualification. A collision with a local function of the same name is E050 (no silent overwrite).
- Wildcard `import pkg.*` is not allowed
- Circular imports are compile errors

<a id="22-stdlib-の-auto-import-層"></a>

### 2.2 Stdlib auto-import tiers

Name resolution has auto-imported modules and modules requiring explicit imports. The current sets are defined by `TIER1_ALWAYS_ACCESSIBLE` / `AUTO_IMPORT_BUNDLED` in `crates/almide-types/src/stdlib_info.rs`.

**No import statement needed:** `string`, `list`, `map`, `set`, `int`, `float`, `bool`, `value`, `result`, `option`, `bytes`, `matrix`, `math`, `datetime`, `deque`, `heap`, and each sized integer / float module. `compute` / `duration` are similarly accessible checker surfaces for time and computation cost.

**Examples requiring explicit import:** `json`, `regex`, `testing`, `path`, `args`, `fs`, `env`, `process`, `io`, `http`, `random`, `ffi`, `bigint`. The removed `error` module is not included.

Auto-import is a name-resolution rule; it does not mean linking every implementation into every program. Bundled source is loaded according to reachability (`bundled_reach`).

<a id="23-import-ヒントと自動修正"></a>

### 2.3 Import hints and automatic fixes

When an unimported module is used, the compiler suggests candidates.

```text
error[E003]: undefined variable 'json'
  --> app.almd:3:11
  hint: Add `import json` (stdlib: JSON parsing and querying)
        Or run `almide fmt` to auto-add missing imports
```

For an external package (with a dependency in `almide.toml`):
```text
error[E003]: undefined variable 'yaml'
  --> app.almd:5:11
  hint: Add `import yaml` (dependency: almide/yaml)
        Or run `almide fmt` to auto-add missing imports
```

<a id="24-almide-fmt-の-auto-import"></a>

### 2.4 Auto-import by `almide fmt`

`almide fmt` automatically manages import statements (equivalent to Go's `goimports`).

**Automatically added:**
- A Tier 2 stdlib module used without an import → insert `import`
- An identifier matching a dependency package name in `almide.toml` → insert `import pkg`
- A dependency package's submodule (such as `python.generate()`) → insert `import pkg.bindings.python`

**Automatically removed:**
- An imported but unused module → remove `import`

**Preserved:**
- Imports with an `_` prefix and `import self` are not removed
- Tier 1 is excluded (already in scope, so not added)

**Submodule discovery:** Recursively scan the dependency package's cache directory (the package source under `~/.almide/cache/.sources/<scheme>/<host>/<owner>/<repo>/@/<ref-or-commit>/`, inside `subdir` when specified) and match the last segment of `.almd` files to identifiers in use. Local directories (`{name}/`) are also searched.

```bash
$ almide fmt app.almd
app.almd: Added `import json`
app.almd: Added `import bindgen.bindings.python`
app.almd: Removed 2 unused import(s)
Formatted app.almd
```

---

<a id="3-呼び出し"></a>

## 3. Calls

```almide project
// file: bindgen/src/mod.almd
fn version() -> String = "0.1.0"
// file: bindgen/src/scaffolding.almd
fn generate(iface: String) -> String = "scaffold:" + iface
// file: bindgen/src/bindings/python.almd
fn generate(iface: String) -> String = "py:" + iface
// file: main.almd
import bindgen
import bindgen.scaffolding
import bindgen.bindings.python

test "call by the last segment" {
  let iface = "Api"

  // トップレベル関数
  assert_eq(bindgen.version(), "0.1.0")

  // サブモジュール関数（最後のセグメント名で呼ぶ）
  assert_eq(scaffolding.generate(iface), "scaffold:Api")

  // 深いサブモジュール
  assert_eq(python.generate(iface), "py:Api")
}
```

Only direct functions of an imported module are accessible. Accessing a submodule requires a separate import (the same approach as Go and Gleam).

---

<a id="4-モジュール境界"></a>

## 4. Module Boundaries

**Only directly imported packages are accessible. Transitive dependencies are invisible.**

```almide project check-fail=E003
// file: d/mod.almd
fn func() -> String = "d"
// file: b/mod.almd
import d
fn func() -> String = "b via " + d.func()
// file: main.almd
import b       // b は内部で d を import している

fn direct() -> String = b.func()       // ✓ 直接 import した
fn phantom() -> String = d.func()      // ✗ E003 undefined variable 'd'
```

To use D, explicitly write `import D`. The design prevents npm's phantom-dependency problem.

<a id="41-convention-method-の越境定義1591-批准"></a>

### 4.1 Cross-boundary convention method definitions (ratified in #1591)

**A caller module may define additional convention methods on an imported type** (open extension — analogous to extension methods in Go / Kotlin / Swift). Since a convention method is merely UFCS sugar and has no instance-resolution coherence problem, this is an intended surface.

```almide
import self.boxlib

// boxlib.Box への後付け拡張 — 合法
fn Box.double(self) -> Int = self.n * 2
```

**However, duplicate definitions of the same method name on the same type are an error (E012).** If both the defining and calling modules declare the same `Type.method`, no winner is defined (measurements found backend divergence — #1726). Check rejects the duplicate and points to both definitions.

```almide
// boxlib が fn Box.tag を持つとき、呼び出し側で
fn Box.tag(self) -> String = "caller"   // ✗ E012: defined in more than one module
```

**Different types** with the same name (`moda.Box` and `modb.Box`) can each have independent methods. Explicit overrides of derived methods (such as derived `repr`) remain legal (#1087).

<a id="42-stdlib-所有型と同名のユーザ型-1828"></a>

### 4.2 User types sharing a name with a stdlib-owned type (#1828)

`Value` / `HttpRequest` / `HttpResponse` / `FileStat` / `ProcessStatus` / `Url` / `TcpStream` / `TcpListener` / `SafeHtml` / `IpAddr` / `Path` are types owned by stdlib modules, whose **bare names are their canonical identities** (`json.parse` always returns the stdlib `Value`). The ledger is `stdlib_info::STDLIB_OWNED_TYPES` (`tests/runtime_backed_types_matrix.rs` mechanically checks agreement with bundled declarations).

When a user declares a type with such a name, it takes the module-qualified identity from #433 — `self.Value` in the entry file, `m.Value` in module `m` — so it never replaces the type referred to by stdlib signatures. Both can coexist in one file and run byte-identically on native and wasm.

The declaration form does not matter (#1835): a newtype from the opaque alias `mod type Value = String` likewise has identity `self.Value` / `m.Value`, and constructor calls `Value(s)` and patterns `Value(s)` carry that name through to IR (native flatten mangling, wasm newtype erasure and interp see one spelling). The same plumbing lets a module's own `mod type Token = String` be constructed and deconstructed as `m.Token` (previously native produced rustc E0531 and wasm rejected both legs). Bundled stdlib's own newtypes (such as `html`'s `SafeHtml`) retain bare names.

```almide
import json
type Value = { n: Int }           // これは self.Value

effect fn main() -> Unit = {
  let mine = Value { n: 7 }       // ユーザの型
  let doc = json.parse("{}")!     // stdlib の Value
  println("${mine.n} ${json.stringify(doc)}")
}
```

`json.parse("{}")!.n` is E013 (stdlib `Value` has no fields), and the hint identifies which type it means. `let v: Value = json.parse("{}")!` is E001 (`self.Value` and `Value` are distinct types). The repr in `${mine}` displays the source spelling `Value { n: 7 }`.

Tests: `spec/wasm_cross/stdlib_type_shadow.almd`, `tests/diagnostics/e013-stdlib-type-shadowed-*/`

<a id="43-モジュール名と同名のローカル束縛-2345"></a>

### 4.3 Local bindings sharing a module's name (#2345)

**Local bindings take precedence over modules, including in member access.** If a name such as `list` / `string` / `map` / `set` is bound locally, `x` in `x.f(..)` refers to **that binding**, not the module, within that scope. Bare identifiers already behaved this way; only member position had looked at the module first.

```almide
fn main() -> Unit = {
  let list = [3, 1, 2]
  println(int.to_string(list.len(list)))   // ✗ E004
}
```

`list.len(list)` is read as “call the local `list`'s `len` method with one argument” — since the receiver is the first argument, the written argument becomes the second. The E004 hint explains this and points to the binding with a secondary span. Fix it by dropping the argument or renaming the binding.

Previously this form **passed check, reached codegen, and generated invalid Rust** (`0.62.0` responds `No errors found`).

The binding form does not matter: `let` / `var` / tuple destructuring / `for` / function parameters / lambda parameters / `match` binders all behave alike.

| Form | Result |
|---|---|
| Shadow a module explicitly brought in with `import` | Local wins; the import becomes dead and produces **E060** |
| Shadow an auto-imported module (`list`/`string`/`map`/`set`, etc.) | Local wins. **No import statement exists, so no warning** — this silent form is the usual case |
| Local's type lacks that member | **E002** (undefined method) |
| An unshadowed module call | Unaffected |

Test: `tests/module_shadow_member_test.rs` (8 cells)

<a id="44-variant-ケース名の見え方-2636"></a>

### 4.4 Visibility of variant case names (#2636)

**Only cases from the file itself, directly imported modules and bundled stdlib are visible as bare case names. Cases in transitive dependencies are invisible.** If the file itself declares a type with the same name, the bare name **always means that type**.

```almide
// file: finish/mod.almd
type Finish = | Stop | Length
// file: middle/mod.almd
import finish
fn describe() -> String = finish.name(finish.Stop)
// file: main.almd
import middle            // finish は import していない
type Stop = { message: String }

let s = Stop { message: "halt" }   // ✓ main 自身の Stop(レコード)
```

| Form | Result |
|---|---|
| Same name as a case in a transitive dependency (unimported module) | Invisible. Resolve only against own types and own imports |
| Same name for a directly imported module's case and an own type | **Own type wins**. Qualify the imported case as `finish.Stop` |
| Two directly imported modules declare the same case name | Bare name is **E019** (ambiguous). Hint gives qualified forms `a.Stop` / `b.Stop` |
| Only one directly imported module declares it | Both bare and qualified forms are allowed (as before) |

Previously case names shared one program-wide table, so a dependency library merely adding a common case name such as `Stop` caused a consumer's record type `Stop { .. }` to become E021 even when that consumer did not import it.

Tests: `spec/integration/modules/case_leak_transitive_test.almd`, `spec/integration/modules/case_leak_direct_test.almd`, `tests/ctor_ambiguity_test.rs`

<a id="45-型名プロトコル名の見え方-2715"></a>

### 4.5 Visibility of type and protocol names (#2715)

**Only declarations in the file itself, directly imported modules and stdlib are visible as bare type or protocol names.** Types from unimported modules (including transitive dependencies) do not resolve bare and produce **E029** (`type 'X' is not in scope here`), with a hint naming the module to import and the qualified form (`b.X`). Protocols follow the same rule (`type T: P` / `[S: P]`).

| Form | Result |
|---|---|
| Own declared type | Own type (declaration order does not matter) |
| Only one directly imported module declares it | That module's type; unchanged even if an unimported module declares the same name |
| Only unimported modules declare it | **E029**. To use the qualified spelling, import that module |
| Two or more directly imported modules declare it | Use qualification (`a.X` / `b.X`) |
| Same name as a builtin type (`Int` `String` `List[T]` `Map[K, V]`, etc.) or builtin protocol (`Eq` `Hash` `Codec`, etc.) | Remains the builtin. An unimported module declaring the same name (such as `type Map`) does not cause E029 (#2839) |
| Own file declares a type with a builtin type's name (`type Int` in `src/int.almd`, `type Path` in `main.almd`, etc.) | In that file, bare `Int` means the own type in every position (signature, field, record literal). Builtin `Int` cannot be written in that file, so a type using `Int` in a field becomes self-referential when written `type Int = { v: Int }`. In other files bare `Int` remains builtin; qualify the declared type (`int.Int`). A spelling with arguments (`Map[K, V]`) remains builtin (#2858). In a file declaring `type Unit`, `main`'s `-> Unit` also means that type, so a `main` returning builtin Unit cannot be written. `check` rejects it with E044 / E001, identifying which `Unit` (#2880) |

Previously bare names resolved program-wide and their meaning changed with the number of same-named types and module registration order (import order / directory order). Untouched files could break merely because another module gained a same-named type, or results could differ by OS.

Tests: `tests/bare_type_visibility_test.rs`, `tests/own_type_forward_reference_test.rs`, `tests/rust_reserved_type_name_gate_test.rs`

---

<a id="5-ダイヤモンド依存"></a>

## 5. Diamond Dependencies

```text
main → B → D
main → C → D
```

D is loaded once and included in compilation output once. B and C refer to the same D.

```almide project
// file: d/mod.almd
fn shared() -> String = "from D"
// file: b/mod.almd
import d
fn from_b() -> String = "B says: " + d.shared()
// file: c/mod.almd
import d
fn from_c() -> String = "C says: " + d.shared()
// file: main.almd
import b
import c
import d

test "one D, reached three ways" {
  assert_eq(b.from_b(), "B says: from D")   // B 経由で D を呼ぶ
  assert_eq(c.from_c(), "C says: from D")   // C 経由で D を呼ぶ
  assert_eq(d.shared(), "from D")           // 直接 D を呼ぶ
}
```

<a id="型の同一性"></a>

### Type identity

A type defined by D is treated as the same type whether reached through B or C.

```almide project
// file: d/mod.almd
type Logger = { name: String }
fn make_logger(n: String) -> Logger = Logger { name: n }
fn log_name(l: Logger) -> String = l.name
// file: b/mod.almd
import d
fn make_logger() -> d.Logger = d.make_logger("b")
// file: c/mod.almd
import d
fn process_logger(l: d.Logger) -> String = "C got " + d.log_name(l)
// file: main.almd
import b
import c
import d

test "D's type is one type through B and C" {
  let logger = b.make_logger()                       // D.Logger 型を返す
  assert_eq(c.process_logger(logger), "C got b")     // ✓ B が作った D.Logger を C が受け取れる
  assert_eq(d.log_name(logger), "b")                 // ✓ 直接 D に渡すのも同じ型
}
```

<a id="バージョン違いのダイヤモンド"></a>

### Diamonds with different versions

Managed by `PkgId(name, major)`. The same `(name, major)` is unified (MVS selects the maximum of minimum versions). Different majors coexist as distinct modules, with codegen adding versions to symbol names (`pkg_v1_func`, `pkg_v2_func`). Same-named types of different majors are incompatible.

```text
B requires D v1.x → almide_rt_D_v1_func()
C requires D v2.x → almide_rt_D_v2_func()
D_v1.Logger ≠ D_v2.Logger
```

---

<a id="6-可視性"></a>

## 6. Visibility

| Modifier | Scope | Example |
|---|---|---|
| `fn` | public — accessible from anywhere | `fn version() -> String` |
| `mod fn` | Within the same project only | `mod fn internal() -> String` |
| `local fn` | Within the same file only | `local fn helper() -> String` |

Accessing `mod fn` / `local fn` from outside causes a compile error:

```text
error: function 'internal' is not accessible from module 'extlib'
  hint: 'internal' has restricted visibility
```

---

<a id="7-import-self"></a>

## 7. `import self`

Refers to the own package's `src/mod.almd`. Used to call library functions from `main.almd`.

```almide project
// file: almide.toml
[package]
name = "mylib"
version = "0.1.0"
// file: src/mod.almd
fn exported_function() -> String = "exported"
// file: main.almd
import self as mylib

test "main reaches its own package through self" {
  assert_eq(mylib.exported_function(), "exported")
}
```

The `name` in `almide.toml` is the default module name. An alias can be set with `as`. Missing `src/mod.almd` is an error.

---

<a id="8-サブモジュールの依存解決"></a>

## 8. Submodule Dependency Resolution

Submodules may freely import stdlib or other packages. They are resolved recursively when the parent package is loaded.

```almide project
// file: extlib/src/mod.almd
fn info() -> String = "ext"
// file: mypackage/src/mod.almd
fn name() -> String = "mypackage"
// file: mypackage/src/formatter.almd
fn format_upper(s: String) -> String = string.to_upper(s)   // stdlib
// file: mypackage/src/utils.almd
import extlib
fn describe(s: String) -> String = extlib.info() + ": " + s  // 他パッケージ
// file: main.almd
import mypackage.formatter
import mypackage.utils

test "a submodule's own imports resolve when the parent is loaded" {
  assert_eq(formatter.format_upper("a"), "A")
  assert_eq(utils.describe("x"), "ext: x")
}
```

When type-checking a submodule, the stdlib / user modules it imported are recognized correctly.

---

<a id="9-依存管理"></a>

## 9. Dependency Management

<a id="almidetoml"></a>

### almide.toml

```toml
[package]
name = "myapp"
version = "0.1.0"

[dependencies]
bindgen = { git = "https://github.com/almide/almide-bindgen.git", tag = "v0.1.0" }
json = { git = "https://github.com/almide/json.git", tag = "v2.0.0" }
# 複数パッケージを持つリポジトリの 1 つ(#3381): subdir でディレクトリを指定
ceangal = { git = "https://github.com/almide-graphics/ceangal2", tag = "v0.1.0", subdir = "ceangal" }
```

Rules for `subdir` validation, locking and shared caching are in [package-system.md §6.1](./package-system.md).

<a id="cli"></a>

### CLI

```bash
almide add almide/almide-bindgen          # github.com/almide/ がデフォルト
almide add almide/almide-bindgen@v0.1.0   # バージョン指定
almide add user/repo                      # 任意の GitHub リポジトリ
almide deps                               # 依存一覧
almide dep-path bindgen                   # キャッシュディレクトリを出力
```

<a id="almidelock"></a>

### almide.lock

`almide.lock` records exact commit hashes. When `git` and `ref` match the manifest, that commit is used. Changing the source `git` is rejected rather than silently re-resolved; explicitly re-lock the new source with `almide update <name>`. Ref changes and resolution of unlocked dependencies also follow trust / `--locked` rules ([package-system](package-system.md) §6.3). Commit the lock to VCS.

The lock exists so that a third party can reconstruct the same bytes; recorded `(git, ref, commit)` must therefore be **a triple that actually held together**. Previously lookup used only the name, so raising a tag left the old commit while rewriting only the `ref` label to the new tag: the lock looked updated but the build compiled the old tag (#2522).

<a id="キャッシュ"></a>

### Cache

The current clone root is `~/.almide/cache/.sources/<scheme>/<host>/<owner>/<repo>/@/`. Source URL components are reversibly encoded, with one checkout for each ref / commit underneath. Ordinary dependencies and `subdir` dependencies on the same source/ref share a checkout and each read their package directory. Implementation: `src/project_fetch.rs::dep_cache_root`.

The previous name-keyed / `.src-<64bit hash>` / `.repos` layouts are historical and are no longer reused. Old entries merely occupy space until `almide clean`.

<a id="バージョン解決"></a>

### Version resolution

**Minimal Version Selection (MVS):** When several dependencies request the same package, select the maximum of the requested minimum versions. No SAT solver is needed; resolution is deterministic.

---

<a id="10-codegen"></a>

## 10. Codegen

<a id="rust-ターゲット"></a>

### Rust target

```rust
// トップレベル関数
pub fn almide_rt_mypackage_version() -> String { ... }

// サブモジュール関数（ドット → アンダースコア）
pub fn almide_rt_mypackage_utils_format(s: String) -> String { ... }

// 深いサブモジュール
pub fn almide_rt_mypackage_bindings_python_generate(iface: String) -> String { ... }
```

<a id="バージョン付き異-major-共存時"></a>

### Versioned symbols (when different majors coexist)

```rust
pub fn almide_rt_mypackage_v2_version() -> String { ... }
```

When set, `IrModule.versioned_name` is used as the codegen prefix.

<a id="struct--enum"></a>

### struct / enum

```rust
#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct Logger {
    pub level: i64,
    pub name: String,
}
```

The `--repr-c` flag outputs `#[repr(C)]`. Module Interface JSON includes ABI information (size, alignment, field offsets).

---

<a id="11-extern"></a>

## 11. @extern

Delegates to a target-specific implementation.

```almide
@extern(rs, "std::cmp", "min")
fn my_min(a: Int, b: Int) -> Int = if a < b then a else b   // フォールバック body

@extern(rust, "std::cmp", "max")
fn my_max(a: Int, b: Int) -> Int = if a > b then a else b // 他ターゲット用の body
```

<a id="111-externrust--の引数-abi"></a>

### 11.1 Argument ABI for `@extern(rust, …)`

`@extern(rust, "crate::host", "f")` (the old spelling `rs` is equivalent) generates a thin wrapper calling `crate::host::f(..)`. The type received by the host (the package's `native/*.rs`) is determined solely by the argument's Almide type, not inferred from the body. Read-only heap values are borrowed; scalars and value types are passed by ownership. Like `String` / `List`, `Bytes` / `Matrix` pass references to the underlying slice/value; the host never sees `AlmideRcCow`.

| Almide argument type | Host-side Rust type |
|---|---|
| `Int` / `Int8`–`Int64` / `UInt8`–`UInt64` | `i64` / `i8`–`i64` / `u8`–`u64` (owned) |
| `Float` / `Float32` / `Float64` / `Bool` / `Unit` | `f64` / `f32` / `f64` / `bool` / `()` (owned) |
| `String` | `&str` |
| `List[T]` | `&[T]` |
| `Bytes` | `&[u8]` |
| `Matrix` | `&AlmideMatrix` |
| `Map[K, V]` / `Set[T]` | `&AlmideMap<K, V>` / `&AlmideSet<T>` |
| Named record `P` | `&crate::P` |
| Variant / `Option[T]` / `Result[T, E]` / tuple | Owned (`crate::Sh` / `Option<T>` / `Result<T, E>` / `(A, B)`) |
| Function value `(A) -> B` | `&dyn Fn(A) -> B` (borrowed — called during the call) |
| `Handle` | `*mut u8` (owned) |
| `Region` | **Two arguments** `(base: *mut u8, len: usize)` — below |
| `mut v: T` | `&mut T` (`mut v: Bytes` is `&mut Vec<u8>`, `mut v: Matrix` is `&mut AlmideMatrix`. Writes are reflected into the caller's `var` through copy-on-write) |

Return values use the type's generated-code representation (`Int` → `i64`, `String` → `String`, …). Generated strings use the shared buffer `AlmideString` (ADR-0013), but the host may keep `String`: the wrapper converts at the boundary (arguments of `List` / `Set` / `Map` / `Option` / `Result` / tuple containing `String` are cloned into the host-side `String` form; `mut` arguments are written back after the call; return values are cloned into the `AlmideString` form. Hosts returning `AlmideString` directly are also accepted). `Bytes` / `Matrix` return values may be raw `Vec<u8>` / `AlmideMatrix` (the wrapper applies `.into()`).

```rust
// native/host.rs
use crate::*;
pub fn checksum(data: &[u8]) -> i64 { data.iter().map(|b| *b as i64).sum() }
pub fn fill(n: i64) -> Vec<u8> { vec![0u8; n as usize] }
```

```almide
@extern(rust, "crate::host", "checksum")
fn checksum(data: Bytes) -> Int = _

@extern(rust, "crate::host", "fill")
fn fill(n: Int) -> Bytes = _
```

The rule lives in one place, `extern_rust_borrow_mode` (`crates/almide-codegen/src/pass_borrow_inference_ownership.rs`); caller borrowing and wrapper parameter types read the same rule.

A `Region` argument (ADR-0031) passes two arguments to the host: the base address `*mut u8` and the runtime-maintained length `usize`. This is the same shape as the pair `@extern(c)` passes to C (§11.2) and the `(base, len)` that `@extern(wasm)` passes to its host; the host does not name the runtime's `Region` record type. The length is Region's own, not a caller-written value. Read with `std::slice::from_raw_parts(base, len)`; writing is permitted only for writable Regions created by `ffi.alloc` (Regions from `ffi.region(b)` are read-only). The pointer is valid only during the call.

```rust
// native/sys.rs
pub fn log(base: *mut u8, len: usize) {
    let bytes = unsafe { std::slice::from_raw_parts(base, len) };
    println!("{}", String::from_utf8_lossy(bytes));
}
```

```almide
import ffi

@extern(wasm, "sys", "log")
@extern(rust, "crate::sys", "log")
fn log(buf: Region) -> Unit = _
```

For a binding carrying both `@extern(wasm)` and `@extern(rust)` (#4353), both hosts receive the same two values at the same position. In wasm the Region argument is split into an import's `(base, len)`, and the native-side binding has no relevance to wasm.

<a id="112-externc--の型"></a>

### 11.2 Types for `@extern(c, …)`

`@extern(c, "lib", "f")` generates an `extern "C"` block and a safe wrapper. Types that may cross into C form a closed table (`almide_lang::types::extern_abi`) shared by the checker and renderer.

| Almide | C side |
|---|---|
| `Int` | `int` (`i32`, narrowed with `as`) |
| `Int8`–`Int64` / `UInt8`–`UInt64` | Integers of the same width |
| `Float` / `Float64` / `Float32` | `double` / `double` / `float` |
| `Bool` | `int` (1 / 0) |
| `Handle` | `*mut u8` (opaque foreign token) |
| `Region` (argument) | Two arguments, `uint8_t *` and `size_t` — base address and runtime-maintained length (ADR-0031 D3) |
| `Region` (return) | A pointer with length and release declared by `@region(len = …, release = "…")` |
| `String` (argument only) | `*const u8` — NUL-terminated copy through the first NUL, valid only during the call |
| `Unit` (return only) | `void` |

Other types (lists, `Bytes`, maps, records, variants, `Option`, tuples, function values, `String` returns) and `mut` arguments have no C representation and are rejected by `check` with E090. Pass those values through an `@extern(rust)` shim as in §11.1.

```almide
@extern(c, "m", "sqrt")
fn c_sqrt(x: Float) -> Float = _
```

Tests: `tests/extern_rust_abi_test.rs` (every argument type × each of `rust` / `rs` / `c`. The Rust forms are built and run together in one package using the host types in the table; C round-trips through `extern "C"` host functions, and each type absent from the table is checked for E090. An exhaustive match pins which row covers each `Ty` variant), `tests/diagnostics/e090-*`.

<a id="113-ホストとの境界での-regionadr-0031-補遺4354"></a>

### 11.3 `Region` at the host boundary (ADR-0031 addendum, #4354)

`Region` crosses the host boundary as **two values, base address and length**, `(base, len)`. The length is always Region's own maintained value, not a caller-written number. In wasm, both values use the memory address width: two `i32`s in wasm32, two `i64`s in memory64 (the current emitter declares only 32-bit memory). Native hosts receive `(*mut u8, usize)` (`@extern(rust)`, §11.1) / `(uint8_t *, size_t)` (`@extern(c)`, §11.2). No copying into `Bytes` occurs in either direction.

| Form | Passing convention | Ownership and lifetime |
|---|---|---|
| An `@export(wasm)` fn **returns** `Region` (push) | Multivalue return `(ptr, len)`: the view's data start and length, not the Region record's address | The host holds one block reference. The block is neither freed nor moved until the host returns `ptr` to the module's `__region_release`. Without release it remains for the instance's lifetime (a leak, not dangling) |
| Same, returning a partial Region from `ffi.slice` | Its own address within the parent block and its length | A module-local table maps address to block. The parent stays alive while a partial Region reference remains. Releasing an address absent from the table (already released or never issued) traps; releasing 0 does nothing. An empty Region is `(0, 0)` |
| An `@export(wasm)` fn **receives** `Region` | Host passes `(base, len)` (the same pair passed to imports) | Read-only Region lent only for the call. Owns nothing; the length is the host's claim (one trusted line). Accesses are bounds-checked against that length. Storing it in a top-level `var`, returning it, or capturing it in a closure in the body is E119 |
| Pass `Region` to `@extern(wasm)` (pull) | Import arguments `(base, len)` | Lent only for the call. For a writable Region from `ffi.alloc`, the host writes in place and the program reads it back |
| An `@export(c)` fn receives `Region` | `(const uint8_t *, size_t)`; writable `(uint8_t *, size_t)` with `@region(out = p)` | Lent only for the call. Escape is E119; if retained through another path, the process is stopped after the call |
| An `@export(c)` fn returns `Region` | None — `check` rejects it with E090 | No form lets the C caller release it. Receive a caller buffer through `@region(out = p)` |

Only Regions from `ffi.alloc` are writable (`ffi.region(b)` and `ffi.slice` are read-only). The host writes into a pushed `ffi.alloc` Region, and a later export reads through the program's copy (a top-level `var`).

```almide
import ffi

var inbox: Region = ffi.region(bytes.new())

@export(wasm, "inbox_open")
effect fn inbox_open(n: Int) -> Region = {
  inbox = ffi.alloc(n)!
  inbox
}

@export(wasm, "inbox_show")
effect fn inbox_show() -> Unit = println(bytes.to_string_lossy(ffi.read(inbox, 0, ffi.len(inbox))!))
```

```js
const [ptr, len] = instance.exports.inbox_open(16n);   // i32 二つ
new Uint8Array(instance.exports.memory.buffer, ptr, len).set(data);
instance.exports.inbox_show();
instance.exports.__region_release(ptr);                // ホストの参照を返す
```

`--host js` glue wraps Region-returning exports as `f(…) → [ptr, len]` and adds `regionRelease(ptr)`. Region-receiving exports become `f(…, base, len, …)`.

Mechanism: `crates/almide-wasm/src/extern_region.rs` (IR rewriting: a receiving wrapper calls `ffi.__lend`; a returning wrapper returns `ffi.__export_view`) and `crates/almide-wasm/src/region_export.rs` (adapters, address table, `__region_release`). Tests: `spec/wasm_host_js/region_export_push.almd` (push, partial Region and release, receiving), `spec/wasm_host_js/region_extern_pull.almd` (pull), `crates/almide-wasm/tests/ffi_host_boundary.rs`, `tests/ffi_host_boundary_test.rs` (E119, `@export(c)`).

---

<a id="12-ファイル解決順序"></a>

## 12. File Resolution Order

Resolving `import pkg`:

1. `{base_dir}/pkg.almd`
2. `{base_dir}/pkg/mod.almd`
3. `{base_dir}/pkg/src/mod.almd`
4. `{base_dir}/pkg/src/lib.almd` (deprecated)
5. `[dependencies]` in `almide.toml` → `~/.almide/cache/.sources/<encoded-source>/@/<ref-or-commit>/...` (for a `subdir` dependency, the specified directory in that checkout)

When a dependency package's `src/mod.almd` is found, recursively scan submodules and subdirectories in that directory.

---

<a id="テスト"></a>

## Tests

| File | Test count | Coverage |
|---|---|---|
| `spec/integration/modules/diamond_test.almd` | 11 | Diamond dependencies, type identity, submodules (individual imports) |
| `spec/integration/modules/alias_test.almd` | 5 | Import aliases: top-level, submodule, deep nesting, type construction |
| `spec/integration/modules/submodule_call_test.almd` | 7 | Submodules (Go-style: individual import + call by last segment) |
| `spec/integration/modules/vis_effect_test.almd` | 2 | Cross-module effect fn calls |
| `spec/integration/modules/vis_mod_error_test.almd` | error | Reject external access to `mod fn` |
| `spec/integration/modules/vis_local_error_test.almd` | error | Reject external access to `local fn` |
| `spec/integration/modules/phantom_dep_error_test.almd` | error | Reject direct access to transitive dependencies |
| `spec/integration/modules/deep_phantom_test.almd` | error | Reject submodule access in transitive dependencies |

<a id="テスト用パッケージ"></a>

### Test packages

| Package | Structure | Purpose |
|---|---|---|
| `mylib` | mod.almd + parser + formatter + utils | Submodule basics |
| `deeplib` | mod.almd + http/mod.almd + http/client.almd | Three-level nesting |
| `dmod_b`, `dmod_c`, `dmod_d` | Diamond B→D, C→D; D has type declarations + submodules | Diamond + type identity |
| `dmod_d/nested/deep.almd` | Four-level dotted call | Namespace from a directory without mod.almd |
| `extlib` | fn + mod fn + local fn | Visibility |
| `effectlib` | effect fn + pure fn | Effect functions |
| `nomod_lib` | parser.almd only (no mod.almd) | Package without a top level |
