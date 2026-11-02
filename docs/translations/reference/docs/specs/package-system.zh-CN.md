<a id="package-system-specification"></a>

# 包系统规范

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](package-system.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](package-system.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](package-system.zh-CN.md)

> 非规范性的固定读者快照。以固定原文为准，不自动同步，也不表示已经独立执行验证。§10 的注册表是未来计划，不表示服务已经实现。信任与原生代码授权是原文描述的机制，不构成安全审计或沙箱保证。§6.4 称有三种途径却列出四项，保留原文差异。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/package-system.md)


> Almide 的依赖管理、版本解析和模块隔离。
> 设计参考 Go Modules（MVS）、Cargo（类型身份）和 pnpm（边界约束）。

<a id="1-design-principles"></a>

## 1. 设计原则

1. **Minimal Version Selection** — 始终选择满足所有约束的最低版本。不使用 SAT 求解器。确定、可复现、快速。（Go Modules）
2. **严格的模块边界** — 代码只能访问其直接导入的模块。传递依赖不可见。不存在幽灵依赖。（pnpm）
3. **以 (name, major) 标识类型身份** — 两个具有相同 `(name, major)` 的包生成相同类型。不同 major 生成不兼容的类型。（Cargo）
4. **单一事实来源** — `almide.toml` 声明意图，`almide.lock` 记录确切提交。遵守匹配的锁；更改来源需要显式重新锁定，`--locked` 拒绝更改。
5. **为未来的注册表预留空间** — 当前基于 git 的系统将由 Almide 包注册表补充。设计决策不得阻碍这一方向。

<a id="2-package-identity"></a>

## 2. 包身份

```text
PkgId { name: String, major: u64 }
```

- 从 `almide.toml` 的 `[package] version` 或 git 标签导出。
- `0.x.y` 将 minor 用作 major（1.0 之前的破坏性变更）。
- `mod_name()` 返回 `"{name}_v{major}"`，用于代码生成的符号命名空间。

<a id="3-version-resolution-mvs"></a>

## 3. 版本解析：MVS

当多个依赖方请求同一包（相同名称、相同 major）时：

```text
B requires D >= 1.2.0
C requires D >= 1.5.0
→ resolve to D 1.5.0 (maximum of minimums)
```

当 major 不同时：

```text
B requires D v1.x (major=1)
C requires D v2.x (major=2)
→ both coexist as separate modules (D_v1, D_v2)
→ D_v1.Logger ≠ D_v2.Logger (different types)
```

<a id="4-module-boundaries"></a>

## 4. 模块边界

<a id="41-direct-vs-transitive"></a>

### 4.1 直接依赖与传递依赖

```text
A imports B, C
B imports D
C imports D
```

从 A 的角度看：
- `B.func()` ✓ — 直接依赖
- `C.func()` ✓ — 直接依赖
- `D.func()` ✗ — 仅为传递依赖（除非 A 也导入 D）
- `D.Logger` ✗ — 除非 A 导入 D，否则该类型不可见

如果 A 需要 D 的类型，A 必须显式声明 `import D`。

<a id="42-enforcement"></a>

### 4.2 强制执行

类型检查器通过 `import` 语句跟踪每个文件导入了哪些模块。
`resolve_module_call` 和 `resolve_static_member` 针对此集合检查，而不是全局的 `user_modules`。

<a id="43-re-exports"></a>

### 4.3 重新导出

模块可以通过包装重新导出依赖的类型：

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

<a id="5-codegen-versioned-symbols"></a>

## 5. 代码生成：带版本的符号

设置 `IrModule.versioned_name` 时（例如 `"json_v2"`），代码生成将其用作函数前缀：

```rust
// Without versioning (same major, no conflict)
pub fn almide_rt_json_parse(...) { ... }

// With versioning (different majors coexist)
pub fn almide_rt_json_v1_parse(...) { ... }
pub fn almide_rt_json_v2_parse(...) { ... }
```

结构体名称也带版本，以避免类型冲突：

```rust
pub struct JsonV1_Config { ... }
pub struct JsonV2_Config { ... }
```

<a id="6-dependency-declaration"></a>

## 6. 依赖声明

```toml
# almide.toml
[package]
name = "myapp"
version = "0.1.0"

[dependencies]
bindgen = { git = "https://github.com/almide/almide-bindgen", tag = "v0.1.0" }
json = { git = "https://github.com/almide/json", tag = "v2.0.0" }
```

**每个键在同一表中只出现一次，每个表也只出现一次**（#2583）。TOML 禁止重复键，所有读取 `almide.toml` 的命令（`check`、`run`、`test`、`build`、…）都会在获取任何内容或触及锁之前拒绝重复，并指出两处所在的行：

```text
error: almide.toml:7: dependency `almai` is declared twice in [dependencies] (first at line 6)
  hint: keep one of lines 6 and 7 and delete the other — TOML allows a key only once per table
```

此规则涵盖清单读取器读取的所有表（`[package]`、`[dependencies]`、`[permissions]`、`[native-deps]`），包括重复的表头。同一 URL 的两种写法（`…/almai` 和 `…/almai.git`）仍是同一依赖的重复声明。

测试：`tests/manifest_duplicate_key_test.rs`。

**`[package] almide = "X.Y.Z"` 指定最低编译器版本**（#4172）。可写为纯版本号或 `">=X.Y.Z"`；其他运算符都是无效的固定指定。所有检查或编译包的命令都会在读取源码前拒绝过旧的编译器：所有目标上的 `check`、`emit`、`compile`、`build`、`run`、`test`，以及 `bench`、`fix`、`survive`、`apply` 和 `verify`。过去只有原生构建读取它，因此 `check` 和 wasm 目标会使用任意编译器编译。包管理（`add`、`deps`、`update`、`install`）及 `fmt` 仍可运行，以便读取和修改该指定。

```text
error: package 'myapp' requires almide >= 0.70.0
  installed version: 0.69.0
```

`ALMIDE_SKIP_VERSION_CHECK=1` 可绕过检查。发布构建仅在同时指定 `--allow-switch ALMIDE_SKIP_VERSION_CHECK` 时才接受它（#4129）。

测试：`tests/version_pin_commands_test.rs`。

**没有任何读取器读取的键会产生警告**（#3382）。过去它会被无声丢弃，因此依赖中的 `brnach = "main"` 或 `tags = "v0.1.0"` 会从默认引用构建，而不是写下的引用。现在每个读取 `almide.toml` 的命令都会警告一次，给出行号、键、该表接受的键，以及存在时距离一次编辑的键（插入、删除、替换或相邻交换）：

```text
warning: unknown key `brnach` in dependency `foo` is ignored — it changes nothing
  --> almide.toml:5
  hint: did you mean `branch`? The keys dependency `foo` accepts are `git`, `tag`, `branch`, `version`, `path`, `subdir`
```

| 表 | 接受的键 |
|---|---|
| 顶层 | `[package]`, `[dependencies]`, `[native-deps]`, `[permissions]`, `[target]`, `[trust]` |
| `[package]` | `name`, `version`, `almide`；以及描述包但不被任何逻辑读取的 `description`, `edition`, `license`, `repository`（不警告） |
| `[dependencies]` 项（内联表、点分键或 `[dependencies.<name>]`） | `git`, `tag`, `branch`, `version`, `path`, `subdir` |
| `[permissions]` | `allow`, `proc`, `native`, `fs` |
| `[trust]` | `owners` |

非表形式的依赖项，或既未指定 `git` 也未指定 `path` 的项，不声明任何依赖，并以同样方式警告。`[native-deps]` 的项是 crate 名称，所以任意键都视为 crate 名称（其规格由 Cargo 判断）；`[target.<platform>]` 下的未知表已经是错误（§6.2）。

构建仍完全按照该键不存在的情况运行，退出码不变（`check --deny-warnings` 也不计入它）。`almide check
--json` 在 stdout 上将警告作为 `"level":"warning"` 行输出；其他所有命令将其输出到 stderr。计划先发布包含警告的版本，再在后续 dialect epoch 中将其升级为错误；目前还没有 epoch 条目。键列表与其读取器一起位于 `src/project.rs`，`tests/manifest_unknown_key_test.rs` 断言每个列出的键都会改变解析结果（而每个元数据键都不会），因此列表不能偏离读取器。

简写形式（默认为 github.com/almide/）：
```bash
almide add bindgen@v0.1.0
# → bindgen = { git = "https://github.com/almide/almide-bindgen", tag = "v0.1.0" }
```

单段名称指 github.com/almide 下的包。包名可能不同于仓库名（`bindgen` 位于 `almide-bindgen`），因此会在编译器内嵌的组织包快照中查询名称（`src/almide_packages.txt`，由 `scripts/gen-almide-packages.sh` 重新生成）；快照中没有的名称解析为 `github.com/almide/<name>`。`almide add almide/<repo>` 使用快照为该仓库给出的包名，因此 `almide add almide/almide-bindgen` 也会添加 `bindgen`（#4093）。

<a id="61-a-package-in-a-subdirectory-of-the-repository-subdir"></a>

### 6.1 仓库子目录中的包：`subdir`

一个仓库可以包含多个包，每个包有自己的目录及 `almide.toml`。git 依赖使用 `subdir` 指定其中一个包（#3381）：

```toml
[dependencies]
ceangal = { git = "https://github.com/almide-graphics/ceangal2", tag = "v0.1.0", subdir = "ceangal" }
snaidhm = { git = "https://github.com/almide-graphics/ceangal2", tag = "v0.1.0", subdir = "snaidhm" }
```

包通过显式位置指定，而不是在仓库中扫描同名包（Cargo 的规则）：清单精确指出构建哪个目录，不依赖仓库中还有什么，`almide.toml` 的读者无需获取仓库即可看出。

- **写法。** `subdir` 是相对于仓库根目录的路径，所有平台都用 `/` 分隔。使用前会规范化：去除 `./`、重复和尾随的 `/`（`./pkgs//c/` 变为 `pkgs/c`），锁记录规范化后的写法。所有读取 `almide.toml` 的命令都会拒绝以下 `subdir`，并指出其所在行：不是字符串、为空、指向根目录自身（`.` — 应删除 `subdir`）、绝对路径（`/x`、`C:/x`）、使用 `\`，或包含任何 `..`（甚至 `a/../b` 也不行：每个目录只用一种写法）。
- **仅用于 git。** 与 `path` 并列的 `subdir` 会被拒绝，因为 `path` 已指定包目录；提示会给出拼接后的 `path`。
- **获取时检查。** 目录必须在检出内容中存在、保持在其内部（拒绝指向克隆外的符号链接），并包含 `[package] name` 与依赖键一致的 `almide.toml`；该键就是导入名称。每次拒绝都会指出清单行号，并列出仓库实际包含的包目录（向下最多两层）：

```text
error: almide.toml:6: dependency `ceangal` names subdir `snaidhm`, whose package is `snaidhm`
  hint: the dependency key is the import name and must be the package's name — rename the key to `snaidhm`, or point `subdir` at the package `ceangal`
error: almide.toml:6: subdir `pkgs/ceangal` of dependency `ceangal` does not exist in https://… at v0.1.0
  hint: the packages in this repository are `ceangal`, `snaidhm` — point `subdir` at one
```

  没有 `subdir` 的 git 依赖，如果仓库根目录不是包（没有 `almide.toml`、`src/`、`.almd` 文件），而子目录中存在包，也会以同样方式拒绝，并给出应添加的 `subdir = "…"`。
- **身份。** 一个包是一个来源及其中的目录：同一仓库的两个 subdir 是两个包，各自遍历自身要求，并锁定为两个条目。`PkgId`（name, major）与 MVS 不变；名称仍是包自身的 `[package] name`。
- **一个克隆。** 检出内容以来源为键，放在 `~/.almide/cache/.sources/<encoded-source>/@/<ref-or-commit>/` 下。依赖相同来源/引用时，无论是否指定 `subdir` 都共享检出内容，各自读取其中的包目录。不复用旧的 `.repos/.src-<hash>` 和以名称为键的布局（见 §6.3）。
- **CLI。** `almide add <repo>[@<tag>] --subdir <dir>` 以 subdir 的最后一个路径分量命名包（`--subdir pkgs/ceangal` → `ceangal`）；使用 `--git <url>` 时第一个参数就是名称。`add` 在写入 `almide.toml` **之前**获取并检查包，因此错误的 `subdir` 不会改变清单。`almide deps` 输出 `… (<ref>) subdir <dir>`，`almide dep-path <name>` 输出克隆内的包目录（存在 `src/` 时输出该目录）。`almide update` 像其他 git 依赖一样更新条目。

测试：`tests/dep_git_subdir_test.rs`（包含两个包的本地仓库：check、lock、共享的一个克隆、`dep-path`、`deps`、`add --subdir`、所有拒绝情形）、`project_fetch` 单元测试（`package_dir_in_checks_the_subdir_against_the_checkout`、`subdir_packages_of_one_repository_share_a_clone_root`）、`tests/lock_roundtrip_test.rs`。

<a id="62-native-rust-dependencies-native-deps"></a>

### 6.2 原生 Rust 依赖：`[native-deps]`

包含 `native/*.rs` 模块或 `@extern(rust, …)` 函数的包，在 `[native-deps]` 中声明它们使用的 crates.io crate。每个条目都会在所有原生构建路径（二进制、`--cdylib`、`--repr-c`、`almide run`、`almide test`）中原样复制到生成 crate 的 Cargo.toml。依赖包的 `[native-deps]` 也以相同方式添加。

```toml
[native-deps]
anyhow = "1"
serde = { version = "1", features = ["derive"] }
```

**针对特定目标的原生依赖**（#3350）。只能在某些目标上构建的 crate 放入 `[target.<platform>.native-deps]`，使用 Cargo 的语法和语义。它会被输出为 `[target.<platform>.dependencies]`，因此 Cargo 仅在构建目标（宿主或 `--target <triple>`）匹配时编译该 crate：

```toml
[target.'cfg(not(any(target_os = "android", target_os = "ios")))'.native-deps]
arboard = "3"

[target.'cfg(target_os = "android")'.native-deps]
jni = "0.21"

[target.x86_64-pc-windows-gnu.native-deps]
winapi = "0.3"
```

`<platform>` 是 Cargo 所接受的 `cfg(<expr>)` 或目标三元组。`<expr>` 是 `all(…)`、`any(…)`、`not(…)`、名称（`unix`）或 `name = "value"`。所有读取 `almide.toml` 的命令都会拒绝 Cargo 会拒绝的键，并指出该键及行号：

```text
error: almide.toml:4: invalid platform `cfg(target_os = android)` in [target.'cfg(target_os = android)'.native-deps]: expected a string after `target_os =`, found `android`
```

只有 `native-deps` 可以针对特定目标：`[target.<platform>]` 下的其他表都是错误。运行时自身所需的 crate（例如 `zlib` 使用的 `flate2`，或 `http` 使用的 TLS crate）始终对所有目标声明。如果还为一个目标声明该 crate，两份声明都会写入，由 Cargo 合并。

测试：`tests/manifest_toml_reader_test.rs`、`tests/native_deps_target_cfg_test.rs` 及 `cargo_build` 单元测试。

<a id="63-trusted-sources-trust-owners"></a>

### 6.3 可信来源：`[trust] owners`

模型会虚构仓库名称，而且虚构的名称会在多次运行中重复，因此攻击者可以在模型请求前注册它（#4093）。所以工具链仅从**根**清单信任的所有者处获取 git 依赖：

- 默认信任 `github.com/almide`；
- 其他所有者，无论在哪个主机上，都以 `host/owner` 列入 `[trust] owners`；
- `path =` 依赖和本地文件系统上的 git 来源（`file://…` 或普通路径）豁免，因为没人能在那里注册名称；
- 从不读取依赖自身的 `[trust]`：包不能代替根包授予信任。

```toml
[dependencies]
csv   = { git = "https://github.com/almide/csv", tag = "v0.1.0" }   # trusted by default
mylib = { git = "https://github.com/alice/mylib", tag = "v1.2.0" }

[trust]
owners = ["github.com/alice"]
```

检查发生在任何网络访问之前，覆盖 `almide add` 以及每次获取（`build`、`run`、`test`、`bench`、`update`），对直接、传递和已锁定的依赖一视同仁；锁不等于授权。所有者名称比较不区分大小写。路径包含 `.`、`..` 分量或百分号转义的 URL 会直接被拒绝，因为无法从文本中读出其获取来源的所有者。没有信任一切的标志或环境变量：授权始终在与其余变更一起审核的文件中明确指定所有者。

**`almide add`** 解析指定、检查所有者、探测仓库是否存在（`git ls-remote`，关闭提示）、获取仓库，然后才写入 `almide.toml` 和 `almide.lock`。任何拒绝或失败都会以 1 退出，并使两个文件逐字节保持不变：

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

- `--trust <owner>` 必须再次指定来源解析得到的所有者（`host/owner`，github.com 也可用 `owner`）；其他所有者都是错误。它将所有者写入 `[trust] owners`。
- `--dry-run` 输出解析结果、信任判定及仓库是否存在，随后既不写入也不获取（如果添加会被拒绝，退出码为 1）。
- 不存在的仓库会明确报告。仅在拒绝或未找到时，才会用可执行的一行（`hint: did you mean \`almide add csv\`?`）建议相近名称：另一所有者下同名的 almide 包；对大小写、`-`、`_`、`.` 和 `almide-` 前缀进行归一化后，编辑距离不超过 2 且不超过名称长度三分之一的 almide 包；或与 `almide` 拼写接近的所有者。它从不自动替换。

来自不可信所有者的**传递依赖或手写依赖**会停止构建，并显示通向它的依赖链。`almide trust <owner>` 是授予信任的单步操作：

```text
error: dependency `util` (required by mylib → util) is fetched from an owner this project does not trust
  source: https://github.com/bob/util
  owner: github.com/bob (not in [trust] owners)
  ...
  hint: if this dependency is expected, run `almide trust github.com/bob` (it adds "github.com/bob" to [trust] owners in almide.toml)
```

**锁保留但来源被更改**会被拒绝：当直接依赖锁条目的 `git` 与清单不同，构建会停止并显示 `` `csv` is locked to <old> but almide.toml now says <new> `` 及提示 `almide update csv`；该命令从新来源重新锁定，包括固定标签的依赖。过去会静默重新解析。

`build`、`run`、`test` 上的 **`--locked`** 会在任何网络访问前拒绝 `almide.lock` 未列出的直接 git 依赖，拒绝会改变锁的解析，且不写锁。应在 CI 中使用。（锁仅记录直接依赖；传递依赖仍像以前一样从引用解析。）

**缓存以来源为键。** 检出内容位于 `~/.almide/cache/.sources/<scheme>/<host>/<owner>/<repo>/@/<ref or commit>`，URL 每个部分均采用单射编码，因此从 `github.com/evil/svg` 获取的名为 `svg` 的包绝不会替代 `github.com/almide/svg`。旧布局将 URL 哈希为 64 位（FNV-1a），不具备抗碰撞性。绝不读取旧布局目录；`almide clean` 会删除它们。

信任是意图，所以放在 `almide.toml` 中；锁记录事实，不含信任位。

测试：`tests/dependency_trust_test.rs`，以及 `trust` 和 `project_fetch` 单元测试。

<a id="64-native-code-from-a-dependency-needs-a-grant-permissions-native"></a>

### 6.4 依赖中的原生代码需要授权：`[permissions] native`

依赖的原生代码在构建过程中由 cargo 编译并运行，发生在所有 `[permissions]` 检查之前且不受其覆盖（#4093）。它通过三种途径进入构建：

- `native/*.rs` 模块及 `native/` 子目录中的文件会被复制到生成 crate；
- `[native-deps]` 和 `[target.<platform>.native-deps]` crate 会被加入生成的 Cargo.toml，因此它们的 `build.rs` 和 proc macro 会运行；
- `@extern(rust, "path", "fn")` fn 会被输出为对任意 Rust 路径的 `path::fn` 调用（包括 `std`，完全没有 `native/` 也可），而 `@extern(c, …)` fn 链接一个 C 符号；
- `@inline_rust("…")` fn 的模板以 Rust 文本形式拼入 crate（#4100）。如果包带有 `@intrinsic` / `@wasm_intrinsic`，也会列出（E085 在 stdlib 之外拒绝它们）。

因此，`almide build`、`almide run`、`almide test` 和 `almide bench` 会拒绝带入上述任何内容的依赖，无论直接依赖还是通过其他依赖到达，除非根 `almide.toml` 在 `[permissions] native` 中列出它。拒绝发生在任何编译之前，覆盖所有 `--target`（规则属于依赖图，如同 `[permissions]` 本身，而不是只属于一个目标），退出码为 1：

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

授权指定包而不是版本，且仅覆盖该包：授权直接依赖并不授权它引入的其他包的原生代码。只有根清单的授权有效；不读取依赖自身的 `[permissions] native`。根包自身的 `native/`、`[native-deps]` 和 `@extern` fn 不需要授权。

每次原生构建都会为每个带入原生代码的已授权依赖输出一行：

```text
note: building native code from `nat` (granted by [permissions].native): native/natval.rs; [native-deps] itoa; @extern fn `number` (src/mod.almd:2)
```

`almide deps` 为每个依赖（先直接依赖，再是通过它们到达的依赖）显示版本、能力集合（`[permissions].allow` 所检查的类别，其中 `@extern` fn 计为所有类别）、它带入的原生代码，以及这些代码是否获得授权：

```text
nat = path ../nat
  version: 0.1.0
  capabilities: Abort, Env.read, Env.write, FS.read, FS.write, Fan, IO.stderr, IO.stdin, IO.stdout, Net.fetch, Net.listen, Proc, Rand, Time
  native code: native/natval.rs; [native-deps] itoa; @extern fn `number` (src/mod.almd:2) (granted by [permissions].native)
```

测试：`tests/native_grant_test.rs`。

<a id="7-lock-file"></a>

## 7. 锁文件

```toml
# almide.lock — auto-generated, do not edit

bindgen = { git = "https://github.com/almide/almide-bindgen", ref = "v0.1.0", commit = "a629eded8d20..." }
json = { git = "https://github.com/almide/json", ref = "v2.0.0", commit = "b8f3a1..." }
ceangal = { git = "https://github.com/almide-graphics/ceangal2", ref = "v0.1.0", commit = "c41d...", subdir = "ceangal" }
```

`subdir` 依赖（§6.1）的条目记录规范化后的 `subdir`，使锁精确指向包；没有 subdir 的条目仍按原样写入。同一仓库的两个包可以共享 `(git, ref, commit)`：固定提交是关于该引用处仓库的事实，所以像其他依赖一样按 `(git, ref)` 查找，两个包都从同一提交构建。

每个直接依赖一个条目，每项一行；写入器绝不会重复输出名称。如果锁包含同名两次（由 #2583 之前的编译器从重复声明依赖的清单写出），会被拒绝，并给出两处行号及解决方法：删除一个，保留 `git` 与 `almide.toml` 匹配的那个；或删除锁，让下次运行重写。

**锁记录解析结果，而非请求**（#2532）。每个条目的 `ref` 和 `commit` 属于 Minimal Version Selection 为该依赖包选中的版本，即实际构建的检出内容，与 `Cargo.lock` 一样。请求仅存在于 `almide.toml`。没有任何要求提升版本时，两者相同。当传递要求提升版本时（清单要求 `foo@v1.0.0`，依赖要求 `foo@v1.1.0`），即使 `almide.toml` 仍写着 `v1.0.0`，条目也写作 `ref = "v1.1.0"` 及 v1.1.0 的提交；这不是漂移，也不会重新解析。每个条目都是一次获取自身的 `(git, ref, commit)`，因此始终是曾同时成立的三元组。

固定提交适用于图中任何位置对相同来源、相同引用的请求，因此锁定提交就是下次构建实际编译的提交。被遍历但未选中的请求（上面的 `v1.0.0`）从引用解析；MVS 读取其清单以获取要求，但不构建它。更改 `almide.toml` 中的请求仍会像以前一样重新解析，并根据新的选择重写锁。#2532 之前记录根请求的锁仍可读取；下次运行会将被提升的条目重写为解析后的记录。

测试：`tests/manifest_duplicate_key_test.rs`、`tests/lock_roundtrip_test.rs`、`tests/dep_lock_pairing_test.rs`（`a_dependency_raised_by_mvs_is_locked_at_the_version_that_was_built`）。

<a id="8-resolution-algorithm"></a>

## 8. 解析算法

```text
1. Parse almide.toml → direct dependencies
2. For each dep:
   a. If almide.lock has an entry for this (git, ref) → use its exact commit
   b. Else fetch tag/branch
   c. With `subdir`, the package is that directory of the checkout (§6.1)
3. Parse dep's almide.toml → transitive dependencies
4. Recurse (depth-first, leaves first)
5. Dedup by PkgId(name, major):
   - Same (name, major): keep maximum requested version (MVS)
   - Different major: both coexist with versioned names
6. Detect impossible constraints → error with explanation
7. Write almide.lock: per direct dependency, the (git, ref, commit) of the
   version step 5 selected for its package
```

<a id="9-error-messages"></a>

## 9. 错误消息

```text
error: version conflict for package 'json'
  → myapp requires json >= 2.0.0 (via almide.toml)
  → bindgen requires json >= 1.0.0, < 2.0.0 (via bindgen/almide.toml)

  json v1.x and v2.x are different major versions and will coexist.
  However, json.Config from v1 cannot be passed to functions expecting json.Config from v2.

  hint: Update bindgen to a version that supports json v2.x,
        or add json v1.x as a separate dependency.
```

<a id="10-future-almide-package-registry"></a>

## 10. 未来：Almide 包注册表

注册表将：
- 托管包含 semver 元数据的包
- 提供不需要 git URL 的 `almide add pkg`
- 为包作者支持 `almide publish`
- 使用内容寻址存储（如 pnpm）进行去重
- 强制包签名与来源溯源（如 JSR）

当前基于 git 的系统具有前向兼容性：
- `PkgId` 已支持 semver
- `almide.lock` 已记录确切提交
- `versioned_name` 已支持共存
- 迁移：`git = "..."` → `registry = "almide"`（或只用名称）

<a id="references"></a>

## 参考资料

- [Go Modules: Minimal Version Selection](https://research.swtch.com/vgo-principles)
- [How Rust Solved Dependency Hell](https://stephencoakley.com/2019/04/24/how-rust-solved-dependency-hell)
- [PubGrub: Next-Generation Version Solving](https://nex3.medium.com/pubgrub-2fb6470504f)
- [pnpm: Flat node_modules is not the only way](https://pnpm.io/blog/2020/05/27/flat-node-modules-is-not-the-only-way)
