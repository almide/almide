<a id="module-system-specification"></a>

# 模块系统规范

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](module-system.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](module-system.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](module-system.zh-CN.md)

> 非规范性的固定读者译本。以固定原文为准，不自动同步，也不表示已独立执行验证或安全审计。最后更新日期和测试数量是原文时点的记录。扩展方法比较与 native/wasm 一致性是原文主张，不是新验证的比较。保留 Region 宿主输入的信任条件和调用期间的生命周期限制；安全包装不等于认证任意外部代码安全。保留一般性的自身长度描述与后文宿主申报长度的例外，不擅自统一。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/module-system.md)


> 最后更新：2026-09-30。由 `spec/integration/modules/`（25 个测试 + 4 个错误测试）、`spec/wasm_cross/stdlib_type_shadow.almd`、`tests/module_shadow_member_test.rs`、`tests/extern_rust_abi_test.rs` 和 `tests/diagnostics/e090-*` 验证。

---

<a id="1-package-structure"></a>

## 1. 包结构

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

**规则：**
- `src/mod.almd` 是包的顶层。`import mypackage` 后可用 `mypackage.func()` 调用
- 同层的 `.almd` 文件（除 `mod.almd`、`lib.almd`、`main.almd` 外）自动成为子模块
- 子目录用作命名空间；即使没有 `mod.almd`，也能成为点分路径的中间节点
- 递归扫描至任意深度
- 包名由 `almide.toml` 的 `[package] name` 定义。源码中不需要 module 声明

---

<a id="2-import"></a>

## 2. 导入

<a id="21-構文"></a>

### 2.1 语法

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

- `import pkg.sub` 可以通过末段名称引用：`sub.func()`
- 末段名称是**仅在该文件内有效的绑定名**，不是模块身份。模块身份是从 `src/` 开始的完整路径（`import self.a.util` 对应 `src/a/util.almd`），因此从不同文件用 `import self.a.util` / `import self.b.util` 导入 `src/a/util.almd` 和 `src/b/util.almd` 时，会解析为不同模块（#2654）。在同一文件导入两者会使绑定名 `util` 冲突并报错，需要给其中一个设置别名，例如 `import self.b.util as butil`。
- 选择性导入 `import pkg.{func}` 只把指定名称绑定为非限定名。与同名本地函数冲突时为 E050（不隐式覆盖）。
- 不允许通配符 `import pkg.*`
- 循环导入是编译错误

<a id="22-stdlib-の-auto-import-層"></a>

### 2.2 stdlib 的自动导入层级

名称解析区分自动导入的模块和需要显式导入的模块。当前集合由 `crates/almide-types/src/stdlib_info.rs` 中的 `TIER1_ALWAYS_ACCESSIBLE` / `AUTO_IMPORT_BUNDLED` 定义。

**无需 import 语句：** `string`、`list`、`map`、`set`、`int`、`float`、`bool`、`value`、`result`、`option`、`bytes`、`matrix`、`math`、`datetime`、`deque`、`heap`，以及各定宽整数/浮点模块。`compute` / `duration` 同样可直接使用名称，是检查器的时间与计算量接口。

**需要显式导入的示例：** `json`、`regex`、`testing`、`path`、`args`、`fs`、`env`、`process`、`io`、`http`、`random`、`ffi`、`bigint`。已删除的 `error` 模块不在此列表中。

自动导入是名称解析规则，不表示将所有实现链接进所有程序。Bundled source 根据可达性加载（`bundled_reach`）。

<a id="23-import-ヒントと自動修正"></a>

### 2.3 导入提示与自动修复

使用未导入的模块时，编译器会给出候选。

```text
error[E003]: undefined variable 'json'
  --> app.almd:3:11
  hint: Add `import json` (stdlib: JSON parsing and querying)
        Or run `almide fmt` to auto-add missing imports
```

对于外部包（在 `almide.toml` 中有依赖）：
```text
error[E003]: undefined variable 'yaml'
  --> app.almd:5:11
  hint: Add `import yaml` (dependency: almide/yaml)
        Or run `almide fmt` to auto-add missing imports
```

<a id="24-almide-fmt-の-auto-import"></a>

### 2.4 `almide fmt` 的自动导入

`almide fmt` 自动管理 import 语句（相当于 Go 的 `goimports`）。

**自动添加：**
- 已使用但未导入的 Tier 2 stdlib 模块 → 插入 `import`
- 与 `almide.toml` 中依赖包名相同的标识符 → 插入 `import pkg`
- 依赖包的子模块（如 `python.generate()`）→ 插入 `import pkg.bindings.python`

**自动删除：**
- 已导入但未使用的模块 → 删除 `import`

**保留：**
- 不删除带 `_` 前缀的 import 和 `import self`
- Tier 1 不在处理范围内（已经在作用域中，不添加）

**发现子模块：** 递归扫描依赖包的缓存目录（`~/.almide/cache/.sources/<scheme>/<host>/<owner>/<repo>/@/<ref-or-commit>/` 下的 package source，存在 `subdir` 时在其内部），将 `.almd` 文件的末段名称与正在使用的标识符匹配。本地目录（`{name}/`）也在搜索范围内。

```bash
$ almide fmt app.almd
app.almd: Added `import json`
app.almd: Added `import bindgen.bindings.python`
app.almd: Removed 2 unused import(s)
Formatted app.almd
```

---

<a id="3-呼び出し"></a>

## 3. 调用

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

只能访问已导入模块的直接函数。访问子模块需要单独导入（与 Go、Gleam 相同）。

---

<a id="4-モジュール境界"></a>

## 4. 模块边界

**只能访问直接导入的包。传递依赖不可见。**

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

要使用 D，必须显式写 `import D`。此设计防止 npm 的幽灵依赖问题。

<a id="41-convention-method-の越境定義1591-批准"></a>

### 4.1 跨边界定义 convention method（#1591 批准）

**调用方模块可以为导入的类型追加定义 convention method**（开放扩展，与 Go / Kotlin / Swift 的扩展方法同型）。convention method 只是 UFCS 语法糖，不存在实例解析的一致性问题，因此这是有意提供的接口。

```almide
import self.boxlib

// boxlib.Box への後付け拡張 — 合法
fn Box.double(self) -> Int = self.n * 2
```

**但对同一类型重复定义同名方法是错误（E012）。** 若定义方与调用方都声明同一个 `Type.method`，不定义谁胜出（实测造成过后端分歧——#1726）。重复定义在 check 时被拒绝，并指出两个定义位置。

```almide
// boxlib が fn Box.tag を持つとき、呼び出し側で
fn Box.tag(self) -> String = "caller"   // ✗ E012: defined in more than one module
```

同名的**不同类型**（`moda.Box` 与 `modb.Box`）可分别拥有独立方法。显式覆盖派生方法（如派生 `repr`）仍合法（#1087）。

<a id="42-stdlib-所有型と同名のユーザ型-1828"></a>

### 4.2 与 stdlib 所有类型同名的用户类型（#1828）

`Value` / `HttpRequest` / `HttpResponse` / `FileStat` / `ProcessStatus` / `Url` / `TcpStream` / `TcpListener` / `SafeHtml` / `IpAddr` / `Path` 是 stdlib 模块拥有的类型，其**裸名就是规范身份**（`json.parse` 的返回值始终是 stdlib 的 `Value`）。台账是 `stdlib_info::STDLIB_OWNED_TYPES`（`tests/runtime_backed_types_matrix.rs` 机械检查其与 bundled 声明的一致性）。

用户声明同名类型时，该类型采用 #433 的模块限定身份——入口文件中为 `self.Value`，模块 `m` 中为 `m.Value`——因此绝不会替换 stdlib 签名所指的类型。两者可以共存于同一文件，并在 native 与 wasm 上逐字节一致地运行。

声明形式不限（#1835）：opaque alias `mod type Value = String` 形成的 newtype 也以相同的 `self.Value` / `m.Value` 为身份，构造调用 `Value(s)` 和模式 `Value(s)` 将此名称一直带到 IR（native 的 flatten mangle、wasm 的 newtype 消除、interp 看到同一写法）。通过相同机制，模块自身的 `mod type Token = String` 也能作为 `m.Token` 构造和解构（以前 native 为 rustc E0531，wasm 两个 leg 均拒绝）。Bundled stdlib 自身的 newtype（例如 `html` 的 `SafeHtml`）仍使用裸名。

```almide
import json
type Value = { n: Int }           // これは self.Value

effect fn main() -> Unit = {
  let mine = Value { n: 7 }       // ユーザの型
  let doc = json.parse("{}")!     // stdlib の Value
  println("${mine.n} ${json.stringify(doc)}")
}
```

`json.parse("{}")!.n` 是 E013（stdlib `Value` 没有字段），提示会指出是哪个类型。`let v: Value = json.parse("{}")!` 是 E001（`self.Value` 与 `Value` 是不同类型）。`${mine}` 中的 repr 显示源码写法 `Value { n: 7 }`。

测试：`spec/wasm_cross/stdlib_type_shadow.almd`、`tests/diagnostics/e013-stdlib-type-shadowed-*/`

<a id="43-モジュール名と同名のローカル束縛-2345"></a>

### 4.3 与模块同名的局部绑定（#2345）

**局部绑定优先于模块，成员访问也一样。** 在局部绑定 `list` / `string` / `map` / `set` 等名称后，该作用域中 `x.f(..)` 的 `x` 指的是**该绑定**，而非模块。裸标识符以前就如此，只有成员位置过去优先查模块。

```almide
fn main() -> Unit = {
  let list = [3, 1, 2]
  println(int.to_string(list.len(list)))   // ✗ E004
}
```

`list.len(list)` 被解释为“以一个实参调用局部 `list` 的 `len` 方法”——接收者是第一个实参，所以写出的实参成为第二个。E004 提示会说明这一点，并用次级 span 指向绑定位置。修复方式是去掉实参，或重命名绑定。

过去这种形式**通过 check 到达 codegen，并生成无效 Rust**（`0.62.0` 回答 `No errors found`）。

绑定形式不限：`let` / `var` / 元组解构 / `for` / 函数参数 / lambda 参数 / `match` 绑定均相同。

| 形式 | 结果 |
|---|---|
| 遮蔽显式 `import` 的模块 | 局部优先，import 失效并产生 **E060** |
| 遮蔽自动导入模块（`list`/`string`/`map`/`set` 等） | 局部优先。**没有 import 语句，因此不警告**——这种静默形式才是通常情况 |
| 局部类型没有该成员 | **E002**（未定义方法） |
| 未被遮蔽的模块调用 | 不受影响 |

测试：`tests/module_shadow_member_test.rs`（8 个单元格）

<a id="44-variant-ケース名の見え方-2636"></a>

### 4.4 variant case 名称的可见性（#2636）

**裸 case 名仅能看到文件自身、直接导入模块和内置 stdlib 的 case；传递依赖的 case 不可见。** 若文件自身声明了同名类型，裸名**始终指自己的类型**。

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

| 形式 | 结果 |
|---|---|
| 与传递依赖（未导入模块）的 case 同名 | 不可见。只在自身类型与自身导入项中解析 |
| 直接导入模块的 case 与自身声明类型同名 | **自身类型优先**。导入的 case 要限定为 `finish.Stop` |
| 两个直接导入模块声明同名 case | 裸名是 **E019**（歧义）。提示给出 `a.Stop` / `b.Stop` 限定形式 |
| 只有一个直接导入模块声明 | 裸名或限定名均可（与以前相同） |

过去 case 名放在整个程序的一张表中，因此依赖库只要添加 `Stop` 这样的常见 case 名，就会使未导入它的使用方记录类型 `Stop { .. }` 变成 E021。

测试：`spec/integration/modules/case_leak_transitive_test.almd`、`spec/integration/modules/case_leak_direct_test.almd`、`tests/ctor_ambiguity_test.rs`

<a id="45-型名プロトコル名の見え方-2715"></a>

### 4.5 类型名与协议名的可见性（#2715）

**裸类型名、协议名只能看到文件自身、直接导入模块和 stdlib 的声明。** 未导入模块（含传递依赖）的类型不以裸名解析，而是 **E029**（`type 'X' is not in scope here`），提示给出应导入模块及限定形式（`b.X`）。协议也相同（`type T: P` / `[S: P]`）。

| 形式 | 结果 |
|---|---|
| 自身声明的类型 | 自身类型（不论声明顺序） |
| 只有一个直接导入模块声明 | 该模块的类型；即使未导入的另一模块声明同名也不变 |
| 仅未导入模块声明 | **E029**。要用限定写法，必须导入该模块 |
| 两个以上直接导入模块声明 | 使用限定形式（`a.X` / `b.X`） |
| 与内置类型（`Int` `String` `List[T]` `Map[K, V]` 等）或内置协议（`Eq` `Hash` `Codec` 等）同名 | 仍是内置项。即使未导入模块声明同名（如 `type Map`），也不会成为 E029（#2839） |
| 自身文件声明与内置类型同名的类型（如 `src/int.almd` 中的 `type Int`、`main.almd` 中的 `type Path`） | 在该文件中，裸 `Int` 在所有位置（签名、字段、记录字面量）都指自身类型。该文件无法书写内置 `Int`，因此字段使用 `Int` 的类型写成 `type Int = { v: Int }` 就是自引用。其他文件中裸 `Int` 仍是内置类型，声明的类型要限定（`int.Int`）。带参数的写法（`Map[K, V]`）仍是内置项（#2858）。声明 `type Unit` 的文件中，`main` 的 `-> Unit` 也指该类型，因此无法写出返回内置 Unit 的 `main`。`check` 用 E044 / E001 指出是哪个 `Unit` 并拒绝（#2880） |

过去裸名在整个程序中解析，其含义受同名类型的数量和模块注册顺序（导入顺序、目录顺序）影响。未修改的文件可能仅因另一个模块新增同名类型而损坏，或结果因 OS 而异。

测试：`tests/bare_type_visibility_test.rs`、`tests/own_type_forward_reference_test.rs`、`tests/rust_reserved_type_name_gate_test.rs`

---

<a id="5-ダイヤモンド依存"></a>

## 5. 菱形依赖

```text
main → B → D
main → C → D
```

D 只加载一次，也只在编译输出中包含一次。B 和 C 引用同一个 D。

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

### 类型身份

D 定义的类型，无论经由 B 还是 C，都视为同一类型。

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

### 不同版本的菱形

由 `PkgId(name, major)` 管理。相同 `(name, major)` 统一为一个（MVS：选择最低版本要求中的最大值）。不同 major 作为不同模块共存，codegen 给符号名加版本（`pkg_v1_func`、`pkg_v2_func`）。不同 major 的同名类型不兼容。

```text
B requires D v1.x → almide_rt_D_v1_func()
C requires D v2.x → almide_rt_D_v2_func()
D_v1.Logger ≠ D_v2.Logger
```

---

<a id="6-可視性"></a>

## 6. 可见性

| 修饰符 | 范围 | 示例 |
|---|---|---|
| `fn` | public——任何位置均可访问 | `fn version() -> String` |
| `mod fn` | 仅同一项目内 | `mod fn internal() -> String` |
| `local fn` | 仅同一文件内 | `local fn helper() -> String` |

从外部访问 `mod fn` / `local fn` 会产生编译错误：

```text
error: function 'internal' is not accessible from module 'extlib'
  hint: 'internal' has restricted visibility
```

---

<a id="7-import-self"></a>

## 7. `import self`

引用自身包的 `src/mod.almd`，用于从 `main.almd` 调用库函数。

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

`almide.toml` 的 `name` 是默认模块名，可用 `as` 设置别名。缺少 `src/mod.almd` 时是错误。

---

<a id="8-サブモジュールの依存解決"></a>

## 8. 子模块依赖解析

子模块可自由导入 stdlib 或其他包，在加载父包时递归解析。

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

对子模块做类型检查时，会正确识别它导入的 stdlib / 用户模块。

---

<a id="9-依存管理"></a>

## 9. 依赖管理

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

`subdir` 验证、锁定和缓存共享规则见 [package-system.md §6.1](./package-system.zh-CN.md)。

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

`almide.lock` 记录确切提交哈希。当 `git` 和 `ref` 与清单一致时使用该提交。来源 `git` 的变更会被拒绝，不静默重新解析；用 `almide update <name>` 显式从新来源重新锁定。ref 更改和未锁定依赖的解析也遵循 trust / `--locked` 规则（[package-system](package-system.zh-CN.md) §6.3）。锁应提交到 VCS。

锁是为了让第三方重建相同字节，因此记录的 `(git, ref, commit)` 必须是**实际同时成立的三元组**。过去仅按名称查找，升级 tag 后仍保留旧提交，只把 `ref` 标签改成新 tag，导致锁看似更新而构建仍编译旧 tag（#2522）。

<a id="キャッシュ"></a>

### 缓存

当前 clone root 为 `~/.almide/cache/.sources/<scheme>/<host>/<owner>/<repo>/@/`。对来源 URL 分量进行可逆编码，在其下按 ref / commit 放置检出内容。同一来源/ref 的 `subdir` 依赖和普通依赖共享检出，各自读取包目录。实现为 `src/project_fetch.rs::dep_cache_root`。

旧的 name-keyed / `.src-<64bit hash>` / `.repos` 布局是历史，不再复用。旧条目只会占用空间，直到执行 `almide clean`。

<a id="バージョン解決"></a>

### 版本解析

**Minimal Version Selection（MVS）：** 多个依赖要求同一个包时，选择所要求最低版本中的最大值。不需要 SAT 求解器，具有确定性。

---

<a id="10-codegen"></a>

## 10. 代码生成

<a id="rust-ターゲット"></a>

### Rust 目标

```rust
// トップレベル関数
pub fn almide_rt_mypackage_version() -> String { ... }

// サブモジュール関数（ドット → アンダースコア）
pub fn almide_rt_mypackage_utils_format(s: String) -> String { ... }

// 深いサブモジュール
pub fn almide_rt_mypackage_bindings_python_generate(iface: String) -> String { ... }
```

<a id="バージョン付き異-major-共存時"></a>

### 带版本（不同 major 共存时）

```rust
pub fn almide_rt_mypackage_v2_version() -> String { ... }
```

设置 `IrModule.versioned_name` 时，将其作为 codegen 前缀。

<a id="struct--enum"></a>

### struct / enum

```rust
#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct Logger {
    pub level: i64,
    pub name: String,
}
```

`--repr-c` 标志输出带 `#[repr(C)]` 的代码。Module Interface JSON 包含 ABI 信息（size、align、field offset）。

---

<a id="11-extern"></a>

## 11. @extern

委托给目标特定的实现。

```almide
@extern(rs, "std::cmp", "min")
fn my_min(a: Int, b: Int) -> Int = if a < b then a else b   // フォールバック body

@extern(rust, "std::cmp", "max")
fn my_max(a: Int, b: Int) -> Int = if a > b then a else b // 他ターゲット用の body
```

<a id="111-externrust--の引数-abi"></a>

### 11.1 `@extern(rust, …)` 的参数 ABI

`@extern(rust, "crate::host", "f")`（旧写法 `rs` 也相同）生成调用 `crate::host::f(..)` 的薄包装。宿主（包的 `native/*.rs`）接收的类型只由参数的 Almide 类型决定，不从本体推断。只读堆值借用传递，标量和值类型以所有权传递。与 `String` / `List` 一样，`Bytes` / `Matrix` 也以内部切片/值的引用传递，宿主不会看到 `AlmideRcCow`。

| Almide 参数类型 | 宿主侧 Rust 类型 |
|---|---|
| `Int` / `Int8`～`Int64` / `UInt8`～`UInt64` | `i64` / `i8`～`i64` / `u8`～`u64`（拥有） |
| `Float` / `Float32` / `Float64` / `Bool` / `Unit` | `f64` / `f32` / `f64` / `bool` / `()`（拥有） |
| `String` | `&str` |
| `List[T]` | `&[T]` |
| `Bytes` | `&[u8]` |
| `Matrix` | `&AlmideMatrix` |
| `Map[K, V]` / `Set[T]` | `&AlmideMap<K, V>` / `&AlmideSet<T>` |
| 命名记录 `P` | `&crate::P` |
| variant / `Option[T]` / `Result[T, E]` / 元组 | 拥有（`crate::Sh` / `Option<T>` / `Result<T, E>` / `(A, B)`） |
| 函数值 `(A) -> B` | `&dyn Fn(A) -> B`（借用——在调用期间调用） |
| `Handle` | `*mut u8`（拥有） |
| `Region` | **两个参数** `(base: *mut u8, len: usize)`——见下文 |
| `mut v: T` | `&mut T`（`mut v: Bytes` 为 `&mut Vec<u8>`，`mut v: Matrix` 为 `&mut AlmideMatrix`。写入通过 copy-on-write 反映到调用方的 `var`） |

返回值使用该类型的生成代码表示（`Int` → `i64`、`String` → `String` …）。生成代码的字符串是共享缓冲 `AlmideString`（ADR-0013），但宿主仍可用 `String`：包装在边界转换（含 `String` 的 `List` / `Set` / `Map` / `Option` / `Result` / 元组参数，克隆成宿主侧 `String` 形式传入；`mut` 参数在调用后写回；返回值克隆成 `AlmideString` 形式。也接受直接返回 `AlmideString` 的宿主）。`Bytes` / `Matrix` 返回值可用原始 `Vec<u8>` / `AlmideMatrix`（包装执行 `.into()`）。

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

规则仅位于 `extern_rust_borrow_mode`（`crates/almide-codegen/src/pass_borrow_inference_ownership.rs`）一处，调用侧借用与包装参数类型读取同一规则。

`Region` 参数（ADR-0031）作为两个参数传给宿主：起始地址 `*mut u8` 与运行时维护的长度 `usize`。其形式与 `@extern(c)` 传给 C 的一对值（§11.2）、`@extern(wasm)` 传给宿主的 `(base, len)` 相同，宿主不需要指名运行时的 `Region` 记录类型。长度是 Region 自身的长度，不是调用方写下的值。读取用 `std::slice::from_raw_parts(base, len)`；只有 `ffi.alloc` 创建的可写 Region 才允许写入（`ffi.region(b)` 的 Region 只读）。指针仅在调用期间有效。

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

同时带 `@extern(wasm)` 与 `@extern(rust)` 的绑定（#4353）中，两边宿主都在同一位置接收相同的两个值。wasm 中 Region 参数拆成 import 的 `(base, len)`，原生侧绑定与 wasm 无关。

<a id="112-externc--の型"></a>

### 11.2 `@extern(c, …)` 的类型

`@extern(c, "lib", "f")` 生成 `extern "C"` 块和安全包装。可跨到 C 的类型构成封闭表（`almide_lang::types::extern_abi`），检查器与渲染器读取同一表。

| Almide | C 侧 |
|---|---|
| `Int` | `int`（`i32`，以 `as` 缩窄） |
| `Int8`～`Int64` / `UInt8`～`UInt64` | 相同宽度的整数 |
| `Float` / `Float64` / `Float32` | `double` / `double` / `float` |
| `Bool` | `int`（1 / 0） |
| `Handle` | `*mut u8`（不透明 foreign token） |
| `Region`（参数） | `uint8_t *` 与 `size_t` 两个参数——起始地址与运行时维护的长度（ADR-0031 D3） |
| `Region`（返回值） | 用 `@region(len = …, release = "…")` 声明长度与释放的指针 |
| `String`（仅参数） | `*const u8`——截至第一个 NUL 的 NUL 结尾副本，仅在调用期间有效 |
| `Unit`（仅返回值） | `void` |

其他类型（列表、`Bytes`、映射、记录、variant、`Option`、元组、函数值、`String` 返回值）与 `mut` 参数没有 C 表示，`check` 用 E090 拒绝。此类值通过 §11.1 的 `@extern(rust)` shim 传递。

```almide
@extern(c, "m", "sqrt")
fn c_sqrt(x: Float) -> Float = _
```

测试：`tests/extern_rust_abi_test.rs`（`rust` / `rs` / `c` 各目标 × 全部参数类型。Rust 系用表中宿主类型合并到一个包中构建运行；C 经 `extern "C"` 宿主函数往返；表中没有的类型逐一确认产生 E090。用穷尽 match 固定每个 `Ty` variant 由哪行覆盖）、`tests/diagnostics/e090-*`。

<a id="113-ホストとの境界での-regionadr-0031-補遺4354"></a>

### 11.3 宿主边界的 `Region`（ADR-0031 补遗，#4354）

`Region` 以**起始地址与长度两个值** `(base, len)` 跨越宿主边界。长度始终是 Region 自身维护的值，不是调用方填写的数字。wasm 中两个值按内存地址宽度传递：wasm32 为两个 `i32`，memory64 为两个 `i64`（当前发射器只声明 32 位内存）。原生宿主接收 `(*mut u8, usize)`（`@extern(rust)`，§11.1）/ `(uint8_t *, size_t)`（`@extern(c)`，§11.2）。两个方向均不发生向 `Bytes` 的复制。

| 形式 | 传递方式 | 所有权与生命周期 |
|---|---|---|
| `@export(wasm)` fn **返回** `Region`（push） | 多值返回 `(ptr, len)`，是视图数据起始位置与长度，不是 Region 记录的地址 | 宿主持有块的一个引用。直到宿主将 `ptr` 交回模块的 `__region_release`，块都不会释放或移动。不归还则保留到实例生命周期结束（是泄漏，不是悬空） |
| 同上，返回 `ffi.slice` 的部分 Region | 自身在父块中的地址及长度 | 模块内的表从地址查找块。部分 Region 引用仍存在时父块也存活。释放表中没有的地址（已释放或未发出）会 trap；释放 0 不做任何事。空 Region 为 `(0, 0)` |
| `@export(wasm)` fn **接收** `Region` | 宿主传入 `(base, len)`（与传给 import 的一对相同） | 只在调用期间借出的只读 Region。不拥有任何内容；长度由宿主声明（需要信任的一行）。访问按该长度做边界检查。本体将其放入顶层 `var`、返回或捕获到闭包中是 E119 |
| 向 `@extern(wasm)` 传入 `Region`（pull） | import 参数 `(base, len)` | 仅在调用期间借出。若是 `ffi.alloc` 的可写 Region，宿主原地写入，程序读回 |
| `@export(c)` fn 接收 `Region` | `(const uint8_t *, size_t)`；带 `@region(out = p)` 时为可写的 `(uint8_t *, size_t)` | 仅在调用期间借出。逃逸为 E119；若经其他路径留存，调用后停止进程 |
| `@export(c)` fn 返回 `Region` | 不支持——`check` 以 E090 拒绝 | 没有 C 调用方可释放的形式。通过 `@region(out = p)` 接收调用方缓冲区 |

只有 `ffi.alloc` 的 Region 可写（`ffi.region(b)` 与 `ffi.slice` 只读）。宿主写入通过 push 收到的 `ffi.alloc` Region，下一次 export 通过程序侧副本（顶层 `var`）读取。

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

`--host js` glue 将返回 Region 的 export 包装为 `f(…) → [ptr, len]`，并添加 `regionRelease(ptr)`。接收 Region 的 export 变为 `f(…, base, len, …)`。

机制：`crates/almide-wasm/src/extern_region.rs`（IR 改写：接收使用调用 `ffi.__lend` 的包装，返回使用返回 `ffi.__export_view` 的包装）与 `crates/almide-wasm/src/region_export.rs`（适配器、地址表、`__region_release`）。测试：`spec/wasm_host_js/region_export_push.almd`（push、部分 Region 与释放、接收）、`spec/wasm_host_js/region_extern_pull.almd`（pull）、`crates/almide-wasm/tests/ffi_host_boundary.rs`、`tests/ffi_host_boundary_test.rs`（E119、`@export(c)`）。

---

<a id="12-ファイル解決順序"></a>

## 12. 文件解析顺序

`import pkg` 的解析：

1. `{base_dir}/pkg.almd`
2. `{base_dir}/pkg/mod.almd`
3. `{base_dir}/pkg/src/mod.almd`
4. `{base_dir}/pkg/src/lib.almd`（已弃用）
5. `almide.toml` 的 `[dependencies]` → `~/.almide/cache/.sources/<encoded-source>/@/<ref-or-commit>/...`（`subdir` 依赖是该检出内容中的指定目录）

找到依赖包的 `src/mod.almd` 后，递归扫描同目录中的子模块与子目录。

---

<a id="テスト"></a>

## 测试

| 文件 | 测试数 | 覆盖范围 |
|---|---|---|
| `spec/integration/modules/diamond_test.almd` | 11 | 菱形依赖、类型身份、子模块（分别导入） |
| `spec/integration/modules/alias_test.almd` | 5 | import alias：顶层、子模块、深层嵌套、类型构造 |
| `spec/integration/modules/submodule_call_test.almd` | 7 | 子模块（Go 方式：分别导入 + 按末段名称调用） |
| `spec/integration/modules/vis_effect_test.almd` | 2 | effect fn 跨模块调用 |
| `spec/integration/modules/vis_mod_error_test.almd` | error | 拒绝从外部访问 `mod fn` |
| `spec/integration/modules/vis_local_error_test.almd` | error | 拒绝从外部访问 `local fn` |
| `spec/integration/modules/phantom_dep_error_test.almd` | error | 拒绝直接访问传递依赖 |
| `spec/integration/modules/deep_phantom_test.almd` | error | 拒绝访问传递依赖的子模块 |

<a id="テスト用パッケージ"></a>

### 测试包

| 包 | 结构 | 目的 |
|---|---|---|
| `mylib` | mod.almd + parser + formatter + utils | 子模块基础 |
| `deeplib` | mod.almd + http/mod.almd + http/client.almd | 三层嵌套 |
| `dmod_b`, `dmod_c`, `dmod_d` | B→D、C→D 的菱形；D 中有类型定义和子模块 | 菱形 + 类型身份 |
| `dmod_d/nested/deep.almd` | 四段点分调用 | 没有 mod.almd 的目录命名空间 |
| `extlib` | fn + mod fn + local fn | 可见性 |
| `effectlib` | effect fn + pure fn | 效应函数 |
| `nomod_lib` | 仅 parser.almd（无 mod.almd） | 无顶层的包 |
