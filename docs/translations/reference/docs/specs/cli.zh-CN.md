<a id="cli-specification"></a>

# CLI 规范

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](cli.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](cli.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](cli.zh-CN.md)

> [日英混合正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/cli.md)在固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466` 的非规范性读者译本。不自动同步，也不是生成器或检查器输入；不替代原文权威。下述命令、运行时标志、测量、日期、实现状态及安全主张保留固定原文的陈述，并未针对当前发布版独立执行或验证。代码块及其注释保持原样。生成表标记转为不生效的读者注记。编译预算超限处理不意味着编译器所有操作均不崩溃、OOM 或挂起的全面保证；profile、能力、平台与产物边界保留原文范围。


> 最后更新：2026-10-31

<a id="overview"></a>

## 概览

```
almide <command> [options] [arguments]
```

项目根目录存在 `almide.toml` + `src/main.almd` 时，可省略文件参数，自动使用 `src/main.almd`。

---

<a id="commands"></a>

## 命令

<a id="almide-run"></a>

### `almide run`

编译后立即运行。默认 native 目标生成 Rust 源码 → `cargo build` → 执行二进制。

```bash
almide run                              # src/main.almd を実行
almide run app.almd                     # 指定ファイルを実行 (native)
almide run app.almd --target wasm       # wasm をビルドして埋め込み wasmtime ホストで実行
almide run -- --flag value              # -- 以降はプログラムの引数
almide run app.almd -- arg1 arg2        # ファイル指定 + プログラム引数
```

| 选项 | 说明 |
|---|---|
| `--target rust\|wasm` | 执行目标：`rust`（默认，原生二进制）或 `wasm`（结构化发射器生成 WASM，由嵌入式 wasmtime 宿主执行；无需外部 `wasmtime` CLI 或安装 rustc）。不支持的程序会以带诊断的 wall 停止，不回退到其他渲染器。跨目标等价性的范围和限制见 [WASM-OUTPUT](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/wasm/WASM-OUTPUT.md) |
| `--no-check` | 跳过类型检查 |
| `--locked` | 拒绝获取 `almide.lock` 中没有的依赖，以及会改变锁的解析，不写锁（#4093）。面向 CI |
| `--release` | 优化构建：opt-level=3、fat LTO、codegen-units=1。目标为 x86_64 时，**针对 `x86-64-v3`（AVX2、FMA；Intel 从 2013 年 Haswell 起，AMD 从 2017 年 Zen 起）调优**（#3604）。实测（spectralnorm，运行时 `n`）通用 → v3 在 AMD Zen3 上为 87.8 → 67.2 ms，在 Intel Xeon 8581C 上为 57.1 → 45.4 ms。`native` 在 Zen3 上反而为 109.8 ms，曾设为默认后撤销（#3727 → #3734）。没有 AVX2 的 CPU 会因 `SIGILL` 失败，分发到这些 CPU 应使用 `--target-cpu portable`。aarch64 等其他目标保持通用设置 |

`almide` 自身的标志（`--target` / `--no-check` / `--release`）在 `--` 前解释；`--` 后原样传入程序（与 `cargo run` 相同约定）。程序调用 `env.args()`，会以 `List[String]` 返回 `--` 后的参数。

测试：`tests/run_target_flag_test.rs`

**无执行时间上限**（#2615）：`almide run` 在两个目标上都让程序运行至结束。如同原生二进制没有时间限制，`--target wasm` 的嵌入式宿主也不设置 watchdog（过去在 30 秒后触发 `wasm trap: interrupt`，还在每个循环头支付 epoch 检查成本）。时间限制由 harness 自己设置：进程内测试运行器（`run_wasm` / `run_wasm_with` / `run_wasm_capped`）有 30 秒 watchdog，可用 `ALMIDE_WASM_WATCHDOG_SECS` 调整。fuzz（`tools/xtarget-fuzz` 的 `--timeout`）与扫描脚本自行设置进程级限制。测试：`tests/wasm_run_no_time_limit_test.rs`

**native 构建缓存**（#2500）：native 目标在共用临时目录（`ALMIDE_RUN_PROJECT_DIR`，默认 `<cache base>/run`）构建生成的 Rust，将二进制放在按生成代码内容哈希命名的 `target/<profile>/almide-<hash>`。相同生成代码不论来自 `almide run` 还是 `almide build` 都复用这一个二进制，不调用 cargo。目录内的所有写入（构建、淘汰、清理）在 `.almide-build.lock` 下串行，只有缓存命中不加锁运行。

- **位置**（#4180）：native 构建缓存（`run`、`build-cdylib`、运行时 rlib `almide-rtlib-<key>`、`almide test` 工作器 `test/native`）全部位于每用户私有基目录 `<temp>/almide-<euid>` 下。以 mode 0700 排他创建基目录。若已存在，仅当它是真实目录、归运行用户所有，且组和其他用户无权限时使用。否则拒绝（exit 1）而不修复，因为内容可能已由他人写入。这堵住 Linux `/tmp` 等共享临时空间中，他人预建目录并植入二进制后以你的权限执行的漏洞。
- **命中条件**（#4180）：构建时将二进制 SHA-256 记录到 `almide-<hash>.sha256`，仅当记录与二进制内容一致时命中。没有记录或不匹配则重建。测试：`tests/build_cache_private_test.rs`

- **淘汰**：每次命中刷新二进制 mtime；下次未命中时，在锁内删除 7 天未使用的 `almide-<hash>`（以及 `deps/` 中同一代的 `almide_out-*` 对象）。每天扫描一次（`.almide-evict-stamp`）。不触及 incremental 会话。
- **大小上限**（#2608）：上述 `almide-<hash>` 与 `almide_out-*` 合计超过 `ALMIDE_CACHE_MAX_BYTES`（默认 `4G`，`0`/`off` 禁用）时，在同一锁内按 mtime 从旧到新删除。保留最近 2 分钟内使用的文件。每分钟扫描一次（`.almide-size-stamp`）。依赖 crate 的 rlib 与 incremental 会话不在范围内。默认值为开发机实测每程序产物大小（2.0 GB）的两倍。
- **从 rustc ICE 恢复**：中断的构建（ENOSPC、kill）破坏 rustc incremental 会话后，同类构建以后会反复以 `the compiler unexpectedly panicked` 失败。失败输出含该横幅时，在同一锁内删除 `target/*/incremental` 并仅重建一次；成功则向 stderr 输出一行 `note: rustc crashed on a stale incremental session; cleared … and rebuilt successfully`。重建仍失败则原样报告原错误（不循环）。
- **`almide clean`** 清空此目录（见下文）。

测试：`tests/run_cache_recovery_test.rs`

**wasm 构建缓存**：`--target wasm`（及 `almide bench`）将渲染后的 wasm 模块放在同目录的 `target/wasm/almide-<key>`，程序未变时再次运行不重新渲染模块。key 包括结构化路径读取的一切：入口文件路径和源码、解析器从磁盘读出的所有模块（自有模块及依赖包）的路径和源码、依赖目录、工作目录、编译器自身（版本行以及可执行文件的路径、大小、mtime；内置 stdlib 包含其中）、环境中全部 `ALMIDE_*` 变量。解析、类型检查（含警告）、`[permissions]`、IR 验证、E081 每次都在命中前运行，因此命中与未命中产生相同诊断，拒绝相同内容。仅跳过渲染（结构化路径自己的 front、emit、验证）。被 wall 阻止的渲染不缓存。命中条件（SHA-256 记录及匹配）、7 天淘汰、`ALMIDE_CACHE_MAX_BYTES` 上限、`almide clean` 与 native 产物相同。`ALMIDE_TIME_PHASES=1` 的 `[phase] render (cache hit)` 行显示命中。测试：`tests/wasm_run_cache_test.rs`

**没有 `main` 的文件**：两个目标均以与 `almide test <file> --target <leg>` 相同的判定运行该文件的 `test` 块（向 stderr 输出一行 ``note: <file> declares no `main`; running its test blocks …``）。全部通过为 0，任一失败为 1，也没有 `test` 块则为表示“无内容可运行”的 5（`almide test` 的零数量处理，#2204）。过去 native 直接执行 libtest harness，而 wasm 拒绝。`--` 后的程序参数不使用。测试：`tests/wasm_run_cache_test.rs`

**stdout 缓冲**（#2245）：原生二进制的 `println` / `io.print` / `io.write(io.stdout()!, …)` 通过一个 64 KiB 缓冲区（按程序顺序）。stdout 为终端时每次写入 flush；即使是管道或文件，含换行的写入也 flush。其他 flush 点为退出、panic、`main` 返回 err、启动子进程前、读取 stdin 前，以及始终 flush 的 `io.print`（`io.print("")` 为显式 flush）。stderr（`eprintln`）无缓冲，因此非终端环境下不保留与 stdout 的相对顺序。详情见 [docs/stdlib/io.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/io.md)。

<a id="実行可能なスクリプト"></a>

### 可执行脚本

在文件开头写 shebang 并赋予执行权限，即可直接运行。环境需支持 `/usr/bin/env -S`，且 PATH 中须有 `almide`。

`-S` 不是装饰。Linux 内核把 shebang 的其余内容作为**单个参数**传给 `env`，所以没有 `-S` 的 `#!/usr/bin/env almide run` 会查找名为 `almide run` 的程序而失败。macOS 按空白拆分，因此能通过。`almide check` 对缺少 `-S` 且有两个以上词的 `env` shebang 发出 E062 警告。

```almd
#!/usr/bin/env -S almide run

fn main() -> Unit = {
  println("shebang ok")
}
```

```bash
chmod +x script.almd
./script.almd
```

仅在开头（包括可选 UTF-8 BOM 之后紧接的位置）识别 shebang。保留行号，`fmt` 将 shebang 保留在 imports 和 dialect stamp 之前。其他位置的 `#` 仍是错误。

测试：`spec/cli/shebang.almd`、`tests/hot_native_regressions_test.rs`

---

<a id="almide-build"></a>

### `almide build`

编译生成二进制。

```bash
almide build                            # src/main.almd → パッケージ名のバイナリ
almide build app.almd -o myapp          # 出力ファイル名指定
almide build app.almd --target wasm     # WASM バイナリ（直接 emit、rustc 不要）
almide build app.almd --target linux-musl -o app  # Linux の静的 musl バイナリ（#2772）
almide build app.almd --target x86_64-unknown-linux-musl  # 任意の rustc ターゲット三つ組
almide build app.almd --target wasm --host js -o dist/app.wasm  # + dist/app.js, dist/app.d.ts (JS ホスト、#2265)
almide build app.almd --target wasm --debug   # + DWARF 行テーブル（.debug_* カスタムセクション、#1315）
almide build --release                  # 最適化ビルド (opt-level=3, fat LTO, x86_64 は x86-64-v3 向け)
almide build --release --target-cpu portable -o app  # AVX2 の無い CPU にも配るバイナリ（rustc の汎用 CPU）
```

| 选项 | 说明 |
|---|---|
| `-o <path>` | 输出文件路径。在所有原生路径（二进制 / `--cdylib` / `--repr-c` / 其组合）中含义相同：写到指定路径本身，不存在的父目录会创建（#3349）。不追加扩展名或 `lib` 前缀 |
| `--target wasm` | 生成 WASM 二进制（直接 emit） |
| `--target <triple>` | 原生二进制目标（#2772）：`rust` / `native`（默认，本宿主）、`linux-musl`（宿主架构的 `<arch>-unknown-linux-musl`，x86_64 和 aarch64），或 rustc 目标三元组。向 cargo 传递 `--target`，从 `target/<triple>/<profile>/` 收集产物。通过 `rustup target add <triple>` 安装目标标准库。musl 目标按 Rust 默认值（`crt-static`）**静态链接**，almide 不添加链接标志（glibc 目标仍动态链接）。无依赖程序在 x86_64 宿主上可用 rustc 内置的 musl crt 链接，无需额外工具。跨架构或包含 C 的 `[native-deps]` 需要目标的链接器／C 编译器（如 `musl-tools`）。未知值（不具有三元组形式）和 `wasm32-*` 以退出码 2 拒绝，不默默为宿主构建 |
| `--host js` | 仅适用于 `--target wasm`：在模块旁写入 JS 宿主 `<mod>.js`（无依赖 ES module）及 `<mod>.d.ts`（#2265）。`init(source?, hooks?)` 实例化，`run()` 执行 `main`，每个 `pub fn` 有一个包装器。通过 `init({ js: { name } })` 连接 `@extern(wasm, "js", "name")`（声明为 `effect fn` 或 `Result[T, String]` 的 extern 将 hook 的 throw / reject 接为 err，可在 Almide 侧通过 `!` 传播；其他 extern 的 hook 抛出异常则丢弃实例，重新 `init()` 前不可调用——#3356）。返回 Promise 的 hook 须在 extern 上写 `@extern(wasm, "js", "name", returns: promise)`（#3353 / #3371）：glue 用 `WebAssembly.Suspending` 包装它，并从调用图推断仅可能到达它的 export（及 `run()`），通过 `WebAssembly.promising` 将其变为 async 函数（`.d.ts` 为 `Promise<T>`）。对实例的调用按调用顺序逐个串行；async 调用挂起期间，同步 export 会在进入前抛异常。没有 JSPI 的运行时，`init()` 说明理由并拒绝。语言的类型、效果、调用方不变，native 忽略该标记。未标记的 hook 返回 thenable 时，会抛出 `almide: hooks.js.<name> returned a Promise; mark its @extern with returns: promise` 并丢弃实例（即使可失败 extern 也不成为 err）。develop 上的临时标志 `--async-import` 已在 v0.67.0-rc1 前删除，未进入任何发布版。设计：docs/wasm/JS-HOST-ASYNC-IMPORTS.md。封送支持 Int（`number`，检查 ±2^53 范围）/ Float / Bool / String / Unit；export 的参数和返回值还支持 Bytes（`Uint8Array`）/ List[T]（数组，可嵌套）/ Option[T]（`T | undefined`）/ 记录（按声明顺序含字段的 plain object）（#3354；布局和构建侧所有权遵循发射器逐 export 的记录）。以其他类型（变体 / Map / Set / 元组 / 函数）跨边界的 `pub fn` 在构建时指明类型并拒绝。`effect fn`（以及返回 `Result[T, String]` 的 fn）的返回值在模块中为 Result 块；包装器按发射器记录的返回 ABI 生成：ok 为 `T` 值，err 抛出带 message 的 `AlmideError`（`Error` 子类）。记录与源类型不符的 export 也在构建时指明函数名并拒绝（#3352）。仅交付程序使用的部分（#2276）：边界含 `String` 时才有 `__alloc`/`__release` export 和 glue 的 String 辅助函数；WASI shim 只含交付模块（`--wasm-opt` 后）导入的名称。门禁：`scripts/check-js-host.sh`（在 node 运行 `spec/wasm_host_js/`，匹配预期及 native 输出，检查模块字节同一性、shim 集合、`glue-ceiling.txt` 上限）。规范：docs/wasm/WASM-OUTPUT.md 的“JS host”节 |
| `--debug` | 仅适用于 `--target wasm`：在交付模块（`to_wasi` 后）末尾添加 DWARF 4 的 `.debug_info` / `.debug_abbrev` / `.debug_line` / `.debug_str` / `.debug_ranges` 自定义节，使代码偏移（从 Code 节内容起始计的字节位置，与 LLVM/Emscripten 相同的“DWARF for WebAssembly”约定）可映射到 `.almd` 的 file:line:col（#1315）。每个源文件一个 compile unit，每个函数一个 `DW_TAG_subprogram`（名称、`low_pc`/`high_pc`）。Chrome DevTools 与 wasmtime + lldb 无需标志即可读取。发射器从 IR span 逐指令记录行（无 span 的合成节点继承最近祖先 span；祖先也没有的指令，如函数出口释放，记为 line 0）。代码本身不变：默认无 `--debug` 输出逐字节相同，不在 `size-baseline*.txt` 范围内。不可与 `--component` / `--wasm-opt` 合用（两者重写交付字节，改变表所指地址，故以退出码 2 拒绝）。原生目标也拒绝 |
| `--release` | 优化构建：opt-level=3、fat LTO、codegen-units=1。目标为 x86_64 时，**针对 `x86-64-v3`（AVX2、FMA；Intel 从 2013 年 Haswell 起，AMD 从 2017 年 Zen 起）调优**（#3604）。实测（spectralnorm，运行时 `n`）通用 → v3 在 AMD Zen3 上为 87.8 → 67.2 ms，在 Intel Xeon 8581C 上为 57.1 → 45.4 ms。`native` 在 Zen3 上反而为 109.8 ms，曾设为默认后撤销（#3727 → #3734）。没有 AVX2 的 CPU 会因 `SIGILL` 失败，分发到这些 CPU 应使用 `--target-cpu portable`。aarch64 等其他目标保持通用设置 |
| `--target-cpu <cpu>` | 调优 `--release` 原生构建的 CPU（#3604）。默认取决于目标系列：x86_64 为 `x86-64-v3`，其余为 rustc 通用 CPU（交叉构建也由目标三元组决定）。`portable` 为通用 CPU；其他值作为 rustc CPU 名原样传入（如 `native`、`x86-64-v4`、`neoverse-n1`）。交叉构建指定 `native` 为错误，配合 `--target wasm` 也为错误（wasm 不依赖 CPU）。运行时 rlib 也按同一 CPU 构建，构建缓存键包含 CPU |
| `--fast` | 与 `--release` 相同（为兼容保留） |
| `--unchecked-index` | 禁用数组边界检查（unsafe） |
| `--no-check` | 跳过类型检查 |
| `--locked` | 拒绝获取 `almide.lock` 中没有的依赖，以及会改变锁的解析，不写锁（#4093）。面向 CI |
| `--repr-c` | 给 struct/enum 添加 `#[repr(C)]`（C ABI 兼容） |
| `--cdylib` | 生成共享库（`.so` / `.dylib` / `.dll`）。省略 `-o` 时，为当前目录下的 `lib<名前>.<拡張子>`（名称／扩展名；Windows 为 `<名前>.dll`）。Cargo crate 名来自包名（`almide.toml` 中 `[package].name`，没有则入口文件名），而非 `-o`（#3349） |

**`CARGO_BUILD_TARGET`**（#2772）：没有 `--target` 时，将环境变量 `CARGO_BUILD_TARGET` 视为与 `--target <triple>` 相同的目标指定（`--target rust` 取消它并改为宿主）。两种情况 almide 都显式向 cargo 指定目标，并从子进程环境移除 `CARGO_BUILD_TARGET`。过去变量泄漏到 cargo，产物输出至 `target/<triple>/`，而 almide 收集 `target/<profile>/almide-out`，因此可能默默输出上次宿主二进制，或报 `expected binary not found`。构建前会删除该处旧产物，所以即使 cargo 设置 `build.target` 等改变输出位置，也不会输出旧二进制，而是以 `expected binary not found` 停止。构建缓存键含三元组。`almide run` / `almide test` 在本机执行二进制，因此始终为宿主构建（不把 `CARGO_BUILD_TARGET` 传给子 cargo）。

默认输出文件名：
- 存在 `almide.toml`：`[package] name`
- 否则：源文件名去掉 `.almd`

<a id="rustc-の下限4110"></a>

#### rustc 最低版本（#4110）

原生构建（`almide run` / `build` / `test` / `bench`，cdylib 与 install 路径）在启动 rustc 或 cargo 前读取 `rustc --version`（PATH 中 rustc，以及已配置的 `$RUSTC`）。低于 **rustc 1.97.0** 时不构建任何内容，以退出码 1 拒绝。错误包含发现版本、要求版本及修复方法（`rustup update`）。下限只在 `src/cli/rustc_floor.rs` 的 `MIN_RUSTC` 一处定义，为 max(1.77.2, CI 验证生成代码所用的 `RUST_TOOLCHAIN`)。1.77.2 修复 CVE-2024-24576（Windows `Command` 未在 cmd.exe 中净化 `.bat` / `.cmd` 参数）；旧 rustc 构建的二进制会重新暴露经 `process.exec` 的 shell 注入。`--target wasm`（直接 emit）不使用 rustc，不在此范围内。

---

<a id="almide-test"></a>

### `almide test`

检测并运行 `.almd` 文件内的 `test "name" { ... }` 块。

```bash
almide test                             # カレントディレクトリ以下を再帰スキャン
almide test spec/lang/                  # ディレクトリ指定
almide test spec/lang/expr_test.almd    # ファイル指定
almide test --run "pattern"             # テスト名でフィルタ
almide test --target wasm               # WASM ターゲットでテスト
almide test --json                      # 結果を JSONL (1行1ファイル) で出力
almide test --update-snapshots x_test.almd  # スナップショットの受理(期待値リテラルを書き換え)
```

| 选项 | 说明 |
|---|---|
| `-r, --run <pattern>` | 测试名模式过滤器（见下） |
| `--no-check` | 跳过类型检查 |
| `--locked` | 拒绝获取 `almide.lock` 中没有的依赖及会改变锁的解析，不写锁（#4093）。面向 CI |
| `--json` | 以 JSON 格式输出结果 |
| `--target wasm` | 仅运行 wasm 路径（无 native fallback） |
| `--update-snapshots` | 接受 `testing.assert_snapshot` 不匹配，在源码中改写调用方的期望值字面量（`ALMIDE_UPDATE_SNAPSHOTS=1` 相同） |
| `--ci` | CI 模式：完全不写快照（`CI=true` 相同）。新增和偏离均失败 |
| `--allow-no-tests` | 即使待运行测试为 0 个，也以 0 退出（默认 5） |
| `--show-output` | 显示所有测试写入 stdout / stderr 的内容，包括通过的测试 |

每次运行都报告**实际运行测试数**：`2 tests in 1 file`。若 `--run` 排除了测试，也附带排除数量，如 `0 tests in 1 file (2 filtered out)`。`--json` 每行含 `"tests"` 和 `"filtered_out"`。

**零数量处理**（#2084、#4264）：

| 情况 | 退出码 | 原因 |
|---|---|---|
| `--run` 没有匹配任何测试 | **5** | 什么都没运行。避免 CI 过滤器拼写错误看起来为绿。消息说明模式未匹配。可用 `--allow-no-tests` 退出该规则 |
| 没有任何应运行的测试 | **5** | 意外情况。可用 `--allow-no-tests` 退出该规则 |
| 有测试，但所有文件被跳过，没有运行任何测试 | **5** | 什么都没运行（#4325）。消息为 `no test ran: N file(s) were skipped`。可用 `--allow-no-tests` 退出该规则 |

“没有测试”**同时适用于未找到测试文件，以及指定文件没有 `test` 块**。5 是与 1（测试失败）区分的专用退出码，调用者无需读取输出即可识别。

本表**与目标无关**：相同文件、相同参数下，`--target wasm` 也返回相同退出码与计数行（`0 tests in 1 file`）。既没有 `main` 也没有 `test` 块的文件，不属于渲染器 wall（WALL，该路径拒绝），而是“没有可运行内容”，两个路径都为 5（#2204）。

**只有同次执行至少运行一个测试时，跳过才为绿**（#4325）。`--target wasm` 的跳过有三类，汇总行给出各类数量（`1 passed, 0 failed, 2 skipped: 1 walled,
1 declared `// wasm:skip` (of 3 files)`，计数行为 `1 test in 3 files, 2 files skipped`）：

| 跳过类型 | 含义 |
|---|---|
| declared（前 3 行中的 `// wasm:skip`） | 作者附理由声明“此文件无法在 wasm 运行” |
| walled（`WALL` 行） | wasm 路径无法 lower 该程序（与 E082 相同判定） |
| host unavailable | 环境无法启动该 lane（如 `ALMIDE_TEST_WASM_RUNNER=wasmtime` 时没有 CLI） |

它们都表示“存在测试，但此路径未运行”，不是通过。因此只有一条规则：**整次执行运行测试为 0，则无论是否有跳过都返回 5**。声明了 `// wasm:skip` 的文件单独用 `--target wasm` 运行也返回 5（未运行任何内容）。目录运行中若其他文件至少一个测试运行，声明跳过仍为绿。过去存在跳过时，0 测试也以 0 结束，导致无法在 wasm lower 的负对照以 `0 passed, 0 failed, 1 skipped` 通过。

**嵌入式宿主缺少的 `@extern(wasm)` import 不是误编译**（#4324）。导入 `@extern(wasm, "m", "f")` 的程序，会以与 `almide run --target wasm` 相同的措辞拒绝（`this program imports `m.f` … has no host for it` 及 `--host js` 提示）：

- 默认 `almide test`（wasm 优先 + native fallback）：该文件在 native 运行，输出 `note: <file>: the wasm leg cannot run this file; its tests run on the native fallback` 及理由。不会成为“wasm-only miscompile”的 WASM TRAP。
- `--target wasm`：与 `almide run --target wasm` 相同，为 **FAIL**（退出 1）。依赖该宿主的文件须用 `// wasm:skip <理由>` 声明。

测试：`tests/test_zero_outcomes_test.rs`；两目标一致性：`tests/test_zero_exit_parity_test.rs`；跳过与无宿主判定：`tests/wasm_test_skip_verdict_test.rs`

**失败测试的输出附在失败报告下**（#2538），避免测试用 `println` / `eprintln` 输出中间值后失败时丢失这些输出：

```
FAILED: e.almd
  test: eprintln inside a failing test
  at:   e.almd:4
  expected: 2
  found:    1
  stdout:
    STDOUT: value is 42
  stderr (whole file — stderr carries no per-test boundary):
    DEBUG: value is 42
```

- **stdout 按测试划分。** native（以 `--test-threads=1` 运行 libtest）和 wasm 运行器均在 stdout 打印测试前后边界，中间即该测试输出。
- **stderr 按文件划分。** 两个 harness 都不在 stderr 标记测试边界，因此无法拆分，也包含同文件通过测试的 stderr；标签会注明。
- 通过测试的输出默认静默。`--show-output` 显示全部测试输出。
- native、wasm、默认 lane（wasm 优先，失败后在 native 重跑）均相同。

测试：`tests/test_failure_shows_output_test.rs`

**运行一个文件的测试，只运行该文件自身的 `test`**（#2550）。导入模块的 `test` 不进入导入方构建，而在模块自身文件被测试时运行。因此传入目录的 `almide test` 中每个测试恰好运行一次，失败测试以编写它的文件路径和源码名称报告。native、wasm、默认 lane 均相同。

测试：`tests/imported_module_tests_not_rerun_test.rs`

`--run <pattern>` 是**对测试自身名称的区分大小写子串匹配**（`test "…"` 中的字符串，`where` 的 case 为 `名前 / case`，即名称 / case）（#4264）。把失败报告中的名称原样传入即可选择：`--run "beta fails"`（保留空格）以及 `--run "値が無ければ"` 等非 ASCII 名称都匹配。不匹配生成的函数名（`__test_almd_0001_beta_fails`、`almide_ir::test_fn_name`）。过去匹配生成名，导致含空白、符号或非 ASCII 的名称（在函数名中被折叠并附哈希）无法用自身名称选择。

本规则在 **native 和 wasm 路径完全相同**（#2085）。仅使用 `almide_base::names::test_name_matches_filter` 一个谓词；wasm 在合成运行器时用它过滤，native 用同一谓词选择测试完整路径，再以 `--exact` 传给 Rust 测试 harness。没有匹配的模式按上表以 5 结束。

**文件判定不只看退出码**（#4255）。测试中调用 `process.exit(0)` 会使进程以 0 结束，但剩余测试并未运行。native 在每个测试体到达末尾时，将名称追加到 `ALMIDE_TEST_LEDGER` 指向的文件（`test_module` 模板；是 harness 自身记录，不是程序 stdout）；仅当所有选中测试都有完成记录时通过。wasm 则由嵌入式宿主确认运行器 `main` 完整返回。记录不足则该文件失败，并指出首个未结束测试的名称。

测试：`tests/wasm_test_filter_parity_test.rs`

wasm 路径由嵌入 `almide` 二进制的 wasmtime 宿主运行（与 `almide run --target wasm` 相同），无需 PATH 中有 `wasmtime`（#3046）。仅在 `ALMIDE_TEST_WASM_RUNNER=wasmtime` 时，用 `wasmtime` CLI 运行 `almide build --target wasm` 输出的 stock 产物（已经 `to_wasi`），作为比较路径。任一路径如有文件的 wasm leg 无法启动，会按原因各输出一次 `note: the wasm leg could not start for N file(s) (<原因>); they run on the native fallback`。不会只留一个数量作为痕迹。

测试：`tests/test_wasm_leg_runner_test.rs`

临时产物（CLI 路径中 wasm leg 交给 wasmtime 的 `.wasm` 模块）放在每次执行独有的 `<cache base>/test-run-<pid>-<nonce>/` 下，以文件**绝对路径**哈希命名，退出时删除（`ALMIDE_KEEP_SCRATCH=1` 保留，并向 stderr 输出位置）。同名文件并行执行，或不同目录的同名文件，不会共享路径（#1877）。native fallback 的构建缓存持久存于 `<cache base>/test/native/`（同样以绝对路径为键）。

此 worker 目录**每个测试文件绝对路径对应一个**，各自拥有 `target/`。放任不管不会自动消失（重命名测试、已删 worktree、已消失分支的目录会保留；2026-09-22 实测 4,510 个目录 / 39 GB），因此 `almide test` 启动时**清空 7 天无人使用的 worker 目录**（#2504）。判断依据是“目录直接下方及 `target/<profile>/` 中文件的 mtime”：缓存命中更新所执行二进制的 mtime（#2500），故没构建但“使用过”的目录也保留。每天扫描一次（`native/.almide-evict-stamp`），**不等待**获取各目录的 `.almide-build.lock`；获取失败（其他进程正在构建）则跳过。保留 lockfile，所以扫描后成为仅含 lockfile 的空目录（理由见 [`almide clean`](#almide-clean)）。`ALMIDE_KEEP_SCRATCH=1` 时不扫描。`almide clean` 不论年龄清空所有 worker 目录。

年龄规则在**大量不同文件测试的突发负荷**中不会触发（2026-09-24 两小时达到 7,144 个目录 / 84.9 GB 并填满磁盘，#2608）。因此对整个 worker 缓存设**大小上限**：合计超过 `ALMIDE_CACHE_MAX_BYTES`（默认 `4G`，`0`/`off` 禁用）时，按最后使用时间从旧到新，以相同步骤清空 worker 目录（非阻塞获取各目录锁、保留 lockfile、在锁内再次确认最后使用时间未变）。最近 2 分钟内用过的目录即使超限也不清空（保护无锁缓存命中从存在性检查到 exec 的窗口）。每分钟扫描一次（`native/.almide-size-stamp`）。默认 4 GiB 是开发机在多个 worktree 和 5 个并行 agent 下实测一周工作集（65 个 worker 目录 / 2.0 GB）的两倍。

同样规则适用于**运行时 rlib 缓存** `<cache base>/almide-rtlib-<key>/`（#2504）。每个运行时源码 × rustc 版本 × 优化级别组合一个目录；每次重建编译器或升级工具链都会产生新键，旧目录不再被链接（2026-09-22 实测 31 个目录 / 102 MB）。每次链接刷新 rlib mtime，将 7 天未链接的目录清空（排除当前使用目录）。每天扫描一次，stamp 为 `<cache base>/.almide-evict-stamp`。

测试：`tests/test_scratch_race_test.rs`、`tests/run_cache_recovery_test.rs`

失败报告使用**结构化块**（`FAILED: <file>` 后接 `test:` / `at:` / `hint:` / `diff:`，或 `expected:` `found:`）。多行字符串、列表、记录按单位呈现实际差异。文件内失败按**源码行顺序**排列，因此两次运行可直接 diff。

`--json` 为每文件一行的 JSONL：

```json
{"file":"spec/lang/x_test.almd","status":"fail","exit_code":101,
 "failures":[{"name":"string mismatch","file":"spec/lang/x_test.almd","line":4,
              "op":"assert_eq","expected":"\"a\\nB\"","found":"\"a\\nb\"",
              "diff":"      a\n    - B\n    + b\n","message":"…"}]}
```

编写测试：

```almide
test "addition" {
  assert_eq(1 + 2, 3)
}

test "string concat" {
  assert_eq("a" + "b", "ab")
}
```

- 任意 `.almd` 文件都可写 `test` 块
- `*_test.almd` 后缀是惯例，不强制
- `test` 块内隐式为 effect context（可调用 I/O）

<a id="スナップショット-testingassert_snapshot"></a>

#### 快照（`testing.assert_snapshot`）

期望值是**源码内字面量**（第二参数），没有 sidecar 文件（expect-test 风格）。从 `""` 开始，执行 `almide test --update-snapshots <file>`，实测值即写回为字面量（多行则 heredoc）。仅改写调用行的字面量，文件其余部分逐字节不变。

```almide
import testing

test "render" {
  testing.assert_snapshot(render(x), "")   // → --update-snapshots で実測値に書き換わる
}
```

- 不匹配作为普通失败报告（`expected:` / `found:` 或 `diff:`，以及 `accept:` 行的接受命令）。两个目标的运行时停止块相同（`Error: snapshot mismatch` / `at: line N` / `expected:` / `found:`，exit 1，契约 C-336）。
- `--update-snapshots` 逐文件重复“运行 → 改写 → 重跑”直到收敛（因遇到首个不匹配即停止）。第二参数不是字面量（变量、插值字符串）时，不改写并失败。
- CI 模式（`--ci` / `CI=true`）下，`--update-snapshots` 不写任何内容，新增（`""`）与偏离均失败。快照像代码一样提交和评审。

---

<a id="almide-check"></a>

### `almide check`

仅执行类型检查，不生成二进制。面向 CI 与编辑器集成。

```bash
almide check                            # パッケージ内: src/ 配下の .almd を全部チェック (#2165)
almide check app.almd                   # 指定ファイルをチェック
almide check --deny-warnings            # 警告をエラーとして扱う
almide check --json                     # 診断を JSON で出力(パッケージ内なら src/ 全部、#2253)
almide check --explain E001             # エラーコードの説明
almide check --effects                  # 各関数のエフェクト分析を表示
almide check --timings                  # フロントエンドの phase 別内訳
almide check app.almd --profile critical --allow IO  # critical profile (#567)
```

| 选项 | 说明 |
|---|---|
| `--deny-warnings` | 将警告视为错误 |
| `--profile critical` | 对所有函数应用 bounded profile，能力从 deny-all 开始（见下） |
| `--allow <CAP>` | 在 `--profile critical` 中允许一项能力（可重复） |
| `--json` | 以 JSON 输出诊断，一行一条，用于编辑器／agent 集成 |
| `--explain <code>` | 解释错误代码（与 `almide explain <code>` 相同） |
| `--budget NAME=N` | 设置一项编译成本预算（见下，可重复；`--profile critical` 拒绝） |

`./almide.toml` 如有所有读取器都不读取的键（如 `brnach = "main"`），包括 `check` / `build` / `run` / `test` 的所有命令均只警告一次（#3382）：给出行（`almide.toml:LINE`）、键、该表接受的键，以及编辑距离为 1 的候选。构建按没有该键的情况继续，退出码不变（`--deny-warnings` 也不计入）。`--json` 下向 stdout 输出 `"level":"warning"` 行，否则向 stderr 输出。各表接受的键见 [package-system.md §6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/package-system.md#6-dependency-declaration)。

`almide explain --list [--json]`（#2149）逐行输出全部代码：`{code, mnemonic, severity, since, verdict}`。`mnemonic` 为 `docs/diagnostics/<CODE>.md` 标题，`verdict` 为其中的 `## Fix-it verdict`（mechanical / conditional / not-fixable），`severity`（error / warning / error|warning）和 `since`（首次输出该代码的发布版）来自 `docs/diagnostics/codes.toml`。行集合与编译器输出代码集合一致（`with_code` 的 77 个，加构建路径直接输出 `error[EXXX]` 的 E081–E083）；所有夹具的 severity 均与实际 level 对照（`tests/explain_list_test.rs`）。
| `--effects` | 各函数的效果／能力分析。调用收到的闭包的函数显示为 `{} + whatever f (arg 1) does`，不计为 pure（#3268）。闭包类别集合随值流动，并计入调用方调用点（ADR-0026 D1、`docs/specs/effect-system.md`）。`[permissions]` 违规通过 `path:` 行显示路径（D4） |
| `--timings` | lex / parse / check 分阶段墙钟时间（#1311） |

<a id="コンパイルコストの予算adr-0030-4136"></a>

#### 编译成本预算（ADR-0030、#4136）

编译器处理不可信输入（PR CI、文件刚打开时的 LSP、评分器、playground），因此工作量随输入增长的阶段各有具名**预算**。预算计数（节点数、实例数、字节数、层数），不按秒或空闲内存测量。所以相同输入在任何机器、native 或 wasm 上的编译器、CLI 或 LSP 中都获得相同判定。超限以已定义诊断结束，不崩溃、OOM 或挂起。

- **源码不能提高预算。** 属性、`almide.toml`、`almide.lock`、环境变量均不读取预算。
- **只有命令行能提高预算**：`--budget NAME=N`（可重复，所有子命令共用）。`--profile critical` 拒绝 `--budget`（按默认预算判定）。
- `almide budgets` 给出清单，`almide budgets --md` 输出下表本身（`tests/compile_budget_test.rs` 检查一致）。
- 隐藏标志 `--budget-report` 在退出时向 stderr 输出各预算观测最大值和计数工作量（由 `scripts/measure-compile-budgets.sh` 及上述门禁读取）。

> 原文生成标记（在本读者快照中不生效）：almide budgets --md: begin

| 预算 | 默认值 | 计数内容 | 超限结果 |
|---|---|---|---|
| `nesting` | 500 层 | 单个表达式或模式的语法嵌套，由解析器向下遍历时计数 | 解析错误 `expression nesting too deep (max N)` |
| `type_size` | 256 个节点 | 单个推断类型的节点，在检查器构建表达式类型及解析推断变量时计数；`let t1 = (t0, t0)` 加倍 22 次曾达到 3.7 GB（#4114） | E111 |
| `generic_instances` | 1000 个实例 | 单个程序单态化的协议约束泛型函数特化实例数，每个实例的类型实参也受 `type_size` 限制；多态递归会每轮无止境地增加更大实例 | E112 |
| `manifest_bytes` | 1048576 字节 | 读取的单个 `almide.toml` 或 `almide.lock` 大小，包含项目自身与依赖，在读取前和过程中检查（#4194） | 错误 `<file> is larger than N bytes` |
| `dependency_count` | 1000 个包 | 一次依赖解析访问的不同包，含传递闭包，在每个包获取前计数 | 错误 `too many dependencies (N, max M)` |
| `dependency_depth` | 64 层 | 从根包到当前解析包的依赖链长度，在获取前计数 | 错误 `dependency chain too deep (N, max M)` |
| `exhaustiveness_steps` | 400 步 | 单个 `match` 的穷尽性与可达性检查递归步数（模式矩阵 usefulness 最坏为指数复杂度） | E113 |

> 原文生成标记（在本读者快照中不生效）：almide budgets --md: end

<a id="--profile-critical"></a>

#### `--profile critical`

`--profile critical`（#567）对**全部函数**应用 bounded profile（ALS §B、E070–E078），无需 `@bounded` 属性。通过 critical 的代码也必定通过普通模式（子集而非方言）。高阶调用或创建闭包为 E074。

能力从 deny-all 开始。触及 host 的 stdlib 调用为 E076，仅 `--allow` 授权的部分通过。`--allow` 词汇及各项允许的模块由 `crates/almide-frontend/src/check/bounded.rs` 的 `CAPABILITY_GRANTS` 定义：

| `--allow` | 允许的模块 |
|---|---|
| `IO` | `io`, `fs` |
| `Net` | `http`, `net`, `unix` |
| `Env` | `env`, `args` |
| `Time` | `datetime`, `duration` |
| `Rand` | `random`, `uuid` |
| `Process` | `process` |

没有 `Fan`。`fan.*` 在 critical profile 范围之外（#1628）。此词汇与 `almide.toml` 中 `[permissions].allow` 的叶／父能力词汇不同。例如 critical 使用 `Process`，清单使用 `Proc`；清单也有 `Fan`。准确叶节点与父节点展开见 `docs/specs/effect-system.md` §8。

```
$ almide check rd.almd --profile critical
error[E076]: an effect outside the declared capability is not admissible under `--profile critical`
  hint: capabilities start deny-all — grant one with --allow IO|Net|Env|Time|Rand|Process
$ almide check rd.almd --profile critical --allow IO
No errors found
```

`--profile critical` 以 E100 拒绝 `ffi.*` 内存不安全调用及 `@extern` fn 声明，不能通过 `--allow` 授权。即便普通模式使用 `ffi` 也须显式 import。当前接口是不透明 `Handle` 及带长度、所有者的 `Region`，不是使用已删除 `RawPtr` 或 `bytes.from_raw_ptr` 的示例。详见 [ffi](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/ffi.md) 与 `stdlib/ffi.almd`（ADR-0031）。

不论 profile，`almide build` 都会在 `Built` 行列举 `@extern` 或 foreign pointer 的使用。这是保证边界的显示，不表示仅因没有该显示就获得完整内存安全证明。

测试：`tests/build_outside_guarantee_test.rs`

拒绝以下情况（均退出码 1）：

- 词汇外名称：``error: unknown capability `io` in --allow — grantable capabilities are IO, Net, Env, Time, Rand, Process`` 和 ``hint: did you mean `IO`?``。措辞与 `[permissions].allow` 拒绝相同（#3247）。
- 没有 `--profile` 的 `--allow`：`error: --allow requires --profile critical`
- 除 `critical` 以外的 profile 名：``error: unknown profile `strict` — the only profile is `critical` ``
- 与 `--effects` 合用：`error: --profile is not supported with --effects`

`--json` 和整个包的形式（省略 FILE）也按相同 profile 判定。

测试：`tests/critical_profile_test.rs`、`tests/manifest_permissions_test.rs`（`critical_allow_suggests_the_nearest_capability`）

<a id="--timings"></a>

#### `--timings`

将前端时间拆为 lex / parse / check，在 stderr 输出两行。首行为人类可读，第二行为机器可读（键名属于 API，由 `scripts/check-edit-loop-scale.sh` 的分阶段 ratchet 读取）：

```
$ almide check --timings spec/lang/expr_test.almd
timings: lex 1.8ms (16.9%, 2443k lines/s) parse 1.6ms (15.3%, 2692k lines/s) check 3.9ms (35.9%, 1149k lines/s) | other 3.4ms | total 10.7ms over 4426 lines in 43 sources
almide-timings {"lex_ns":1812250,"parse_ns":1644211,"check_ns":3851626,"total_ns":10717583,"lines":4426,"bytes":109057,"sources":43}
```

- 仅指定 `--timings` 时计量生效；未指定的运行完全不读取时钟（避免测量扰动被测对象；实测 A/B 在 `research/benchmark/editloop/scale.py` 开头）。
- `lines` / `sources` 是**实际 lex 的全部内容**。除入口、自有项目模块外，还包括每次检查必处理的自动导入 bundled stdlib。lines/sec 分母为此。
- `other` 是三阶段外的剩余部分（文件 I/O、import 解析、canonicalize、为 unused 警告执行的 lowering）。不给余项命名会吸收任意回归。
- 检查因错误退出时不输出时间，因为时间花在诊断渲染器上。

测试：`tests/check_timings_test.rs`（完整键集、各阶段非零、无重复计量、无 `--timings` 时无输出）。ratchet 在 `scripts/check-edit-loop-scale.sh`。

`--json` 每行为 `{level, code, message, hint, here, try, try_replace, applicability, suggestions, repair?, context, file, line, col, end_col, secondary}`。`try` 是可粘贴修复片段，`try_replace` 是其替换的 span，`suggestions[]` 是同一修复的结构化形式 `{line, col, end_col, replacement, applicability}`。

**`repair`**（#2149）：结构化修复字段，仅有修复的诊断输出（无修复诊断字节保持不变）。形式为 `{"primary": Edit|null, "alternatives": [Edit], "example": "…"|null}`；`Edit` 与 `suggestions[]` 元素相同，为 `{line, col, end_col, replacement, applicability}`。

- `primary`：诊断负责的一项 span 精确编辑。`almide fix` 只对 `applicability` 为 `"machine-applicable"` 的项无人值守应用。`docs/diagnostics/<CODE>.md` 中 `## Fix-it verdict` 为 **mechanical** 的代码，必须以 machine-applicable 输出此项（`tests/diagnostic_coverage_test.rs` 在二进制上判定，还确认 `almide fix` 后 `almide check` 通过）。反过来，非 mechanical 代码输出 machine-applicable 也失败（`diagnostic_harness_test.rs`）。
- `alternatives`：基于其他解释的编辑，始终为 `"maybe-incorrect"`，绝不自动应用。
- `example`：没有 span 精确编辑时的仅显示片段（可含占位符）。

`suggestions[]` 按序排列 `primary` 与 `alternatives`。`almide fix --json` 的 `suggestions` 和 LSP quickfix 也从同一 `repair` 生成（machine-applicable 的 `primary` 在 LSP 中为 preferred）。

**位置单位**（#2250）：`line` 为从 1 开始的行；`col` / `end_col` 为**从 1 开始的字符数**（Unicode 标量值数量），不是字节或显示宽度。含 CJK 的 66 字符（98 字节）行末插入为 `col = 67`。`end_col` 排他（到该列之前），`col == end_col` 为零宽插入点。按字节计数的 harness 会破坏 UTF-8。

**位置必须指向文件中真实存在的位置**：heredoc 第三行的 `${…}` 按该行该列报告（过去按字符串首行及整个解码模板的字符计数列报告）。当调用的 `)` 在后续行，因 `Span` 没有结束行，只携带 `(` 的 span；这种情况下 E041/E042 的 `!` 修复仅出现在 `try`，没有 `try_replace` / `suggestions`（宁可不提供位置，也不提供把 `!` 插入表达式中途的位置）。测试：`tests/fixit_span_on_line_test.rs`、`tests/interpolation_span_parity_test.rs`。

**语法错误也输出 JSON**：没有任何成功顶层声明的文件，`Parser::parse` 返回 `Err`，共享 `parse_file` 向 stderr 输出人类可读文本后退出。`--json` 直接从解析器获取诊断，在此路径也输出 JSON（LLM 最常生成的错误种类此前恰是唯一仍为文本的种类）。

**退出码与普通 `almide check` 一致**（#2350）：存在解析错误或任一 `level` 为 `error` 的诊断时为 `1`，仅警告则 `0`。过去“能解析的文件一律 `0`”，把类型错误输出到 JSON 行，却从退出码丢弃。读取 `--json` 的是编辑器、CI、harness 而非人，它们按退出码分支，仅失败时查看 payload，导致拒绝的程序全部作为“通过”放行。此规则与 `fmt --json` 从一开始就返回与 `--check` 相同退出码一致，只有 `check --json` 曾例外。

测试：`tests/mcp_test.rs`（`json_check_reports_a_total_parse_failure_as_json`）

**整个包的 JSON**（#2253）：省略 FILE 的 `almide check --json` 按与普通 `almide check` 相同顺序判定 `src/` 下所有入口，一行一诊断。各行 `file` 表明对应入口，因此没有其他多文件形式（也不输出逐入口 `: ok` 行，只有诊断）。退出码为单文件形式的汇总：任一入口不 clean 即为 `1`。不短路判定；在首个不合格入口停止会使其后诊断从报告完全消失，因此先判定全部入口再汇总。`--effects` 为逐函数报告，仍保持单入口解析。

测试：`tests/check_package_entries_test.rs`（`json_judges_every_entry_and_each_row_names_its_file`、`json_exit_code_is_one_when_an_entry_does_not_parse`）、`tests/check_json_exit_code_test.rs`（两模式退出码一致，以及“退出码为红 ⇔ 输出 error 行”等价）

错误代码：

| 代码 | 说明 |
|---|---|
| E001 | 类型不匹配 |
| E002 | 未定义函数 |
| E003 | 未定义变量 |
| E004 | 参数数量不符 |
| E005 | 参数类型不符 |
| E006 | 纯函数调用 effect 函数 |
| E007 | 纯函数内的 fan 块 |
| E008 | fan 中捕获 var |
| E009 | 对 let／参数赋值 |
| E010 | 非穷尽 match |

诊断代码说明见 [../diagnostics/](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/diagnostics)（上表为代表例）。

---

<a id="almide-fmt"></a>

### `almide fmt`

格式化源文件。

行长上限为 **100 字符**（#2161）。调用参数列表和记录字面量满足以下任一条件时，每成员一行（带尾逗号，闭括号回到原缩进）：

- 作者已经分行（成员在源码中位于不同行）
- 单行形式从起始位置计超过 100 字符

否则保持单行。仅一个成员的容器不纵向排列。两种排版都是不动点（再次格式化不变）。

```bash
almide fmt                              # src/**/*.almd を整形
almide fmt app.almd                     # 指定ファイルを整形
almide fmt --check                      # 差分があれば非ゼロで終了（CI 用）
almide fmt --check --json               # 同じゲートを JSON 1 オブジェクトで（機械可読）
almide fmt --dry-run                    # 書き込みせず差分表示
almide fmt --no-import-edit stdlib/     # import 行を一切触らず整形(splice-context ソース用)
```

| 选项 | 说明 |
|---|---|
| `--check` | 仅比较。向 stderr 列出未格式化文件，存在则退出码 1 |
| `--json` | `--check` 的机器可读形式（隐含 `--check`）。stdout 一个对象，退出码相同 |
| `--dry-run` | 仅向 stdout 输出格式化结果，不写入 |
| `--no-import-edit` | 完全不编辑 import 行 |

`--json` 输出：

```json
{"checked":2,"unformatted":["src/a.almd"],"unreadable":[],"verify_failed":false,"ok":false}
```

`--json` 和 `--check` 均不写入。应用格式化使用 `almide fmt <path>`。

测试：`tests/mcp_test.rs`（`fmt_check_json_reports_drift_and_keeps_the_gate_exit_code`）

<a id="コメントの付け先"></a>

#### 注释的附着位置

fmt 格式化后重新解析，验证 AST 同一性和注释数量（E054、#1309）；如有无处附着的注释，不修改文件并拒绝。附着位置有四种（`ExprComments`、#1404 / #1714 / #1326）：

| 书写位置 | 附着位置 | 重新输出 |
|---|---|---|
| `f(/* c */ a)`——节点前，同一行 | 后续节点的 leading | `/* c */ a`（随节点移动） |
| `f(a /* c */, b)` / `1 /* c */ + 2`——节点后，分隔符、闭括号、运算符前 | 前一节点的 trailing | `a /* c */`（不越过逗号） |
| `1 + // c` ↵ `2` / `xs // c` ↵ `\|> f` / `x // c` ↵ `.m()`——行末，表达式在下一行继续 | **结束该行的操作数**的 line_trailing | `1 // c` ↵ `  + 2`——操作数后立即换行，运算符、`\|>`、`.` 引领续行（只有 `...` 留在行末） |
| `xs` ↵ `  // c` ↵ `  \|> f`——续行之间的独立行 | 前一操作数的 line_between | 以续行缩进保持独立行 |

行末 `//` 无论写在运算符前或后都得到相同输出（规范形式为运算符引领续行）。以关键字或分隔符结尾的行上的 `//`（`then // c`、行末 `.`、`match { // c`）没有附着位置，仍被拒绝。

---

<a id="almide-compile"></a>

### `almide compile`

生成 Module Interface，供外部工具（如 binding generator）读取类型信息的 JSON / `.almdi` 产物。

```bash
almide compile                          # プロジェクト全体
almide compile parser                   # モジュール名指定
almide compile app.almd --json          # JSON 出力（stdout）
almide compile --dry-run                # 人間向け表示
almide compile -o target/compile        # 出力ディレクトリ指定
```

JSON 输出结构：

```json
{
  "module": "mathlib",
  "types": [{
    "name": "Point",
    "kind": { "kind": "record", "fields": [{"name": "x", "type": {"kind": "float"}}] },
    "abi": { "size": 16, "align": 8, "fields": [{"name": "x", "offset": 0, "size": 8}] }
  }],
  "functions": [{
    "name": "distance",
    "params": [{"name": "a", "type": {"kind": "named", "name": "Point"}}],
    "return": {"kind": "float"},
    "effect": false
  }],
  "constants": [],
  "dependencies": []
}
```

`abi` 字段仅添加到具体类型（非泛型），描述 C ABI 布局（size、align、field offset）。

---

<a id="almide-mcp"></a>

### `almide mcp`

通过 stdio 启动 Model Context Protocol 服务，使 agent（如 Claude Code）通过**带类型的工具调用**使用编译器。目标是避免让模型额外解析人类可读输出，而该步骤正是准确率流失之处。

```bash
almide mcp                              # stdio で JSON-RPC 2.0（改行区切り）を待つ
```

支持方法：`initialize` / `tools/list` / `tools/call` / `ping`。其他方法返回 JSON-RPC `-32601`（`capabilities` 只声明 `tools`）。

工具（六个，全部经由现有 CLI 机器可读输出）：

| 工具 | 实现 | 返回内容 |
|---|---|---|
| `almide_check` | `almide check --json` | 结构化诊断数组（包含 `try`/`try_replace`） |
| `almide_test` | `almide test --json` | 逐文件 pass/fail + 运行器原始输出 |
| `almide_api` | `almide ide outline --json` / `stdlib-snapshot --json` | 公开声明签名列表 |
| `almide_explain` | `almide explain <CODE>` | 诊断代码说明（markdown） |
| `almide_fmt_check` | `almide fmt --check --json` | 未格式化文件列表（不写入） |
| `almide_survive` | `almide survive --with - --json` | 写入编辑前的生存差值（下文 `almide survive`） |

设计约束：

- **只有一个编译器入口。** 各工具作为子进程启动 `current_exe()`（自身），读取 CLI 已有 JSON。不创建 MCP 专用输出路径（否则必然与 CLI 漂移，且无人目视阅读，漂移不可见）。
- **绝不解析人类可读文本。** 没有机器可读形式的部分（测试单项失败细节 = #1313），原样放入名为 `*_unstructured` 的字段返回。
- **没有写入工具。** `fmt` 只有 `--check` 形式；`almide_survive` 只判定编辑，不写入。应用经 CLI（`almide apply --if-survives` / `almide fmt` / `almide fix`）完成，让编辑保留在 agent 自身转录记录中。

Claude Code 插件定义（MCP + LSP）：`tools/claude-plugin/`；市场：`.claude-plugin/marketplace.json`；安装步骤：[../mcp.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/mcp.md)

测试：`tests/mcp_test.rs`

---

<a id="almide-survive"></a>

### `almide survive`

**写入前**判定提议的编辑（#2147）。在内存应用编辑，在编辑前后运行 check、能到达该编辑的测试和契约夹具，逐诊断／测试／契约返回 `{unchanged, newly_broken, newly_fixed, removed}`。绝不写文件。

```bash
almide survive src/calc.almd --with fix.patch --json   # unified diff
almide survive src/calc.almd --with new.txt  --json    # ファイル全文
git diff src/calc.almd | almide survive src/calc.almd --with - --json
```

- `--with <PATH|->`：编辑，可以是 unified diff（以 `--- ` / `@@ ` 开头），或新文件全文。可用 `--as auto|patch|text` 固定解释方式（默认 `auto`）。diff 的 hunk 必须上下文行和删除行完全一致才应用（模糊应用会判定另一项编辑，因此以 exit 2 拒绝）。
- **可达集合**：编辑文件本身，以及项目根（有 `./almide.toml` 则为 `.`，否则为文件目录）下 import 闭包会读取它的 `.almd` 文件。check 覆盖全部可达文件，测试覆盖含 `test` 块的可达文件，契约覆盖含 `// @contract: C-NNN` 的可达文件（native 与 `--target wasm` 的 stdout 和 exit code 逐字节一致）。
- **内存应用机制**：每个 leg 是子 `almide` 进程，仅编辑后侧传入隐藏参数 `--survive-overlay <PATH> --survive-overlay-text <FILE>`（#4129 从环境变量改为参数，因环境变量会让开发者不控制的环境迫使任意构建编译不同于磁盘的文本）。编译器全部 `.almd` 读取通过 `almide::source_overlay::read_to_string`，仅目标文件的读取返回提议文本。提议文本位于树外的系统临时目录。
- `--timeout <秒>`（默认 600）：每个子进程的上限。超限杀死整个进程组，测试报告 `timeout`，契约报告 `timeout`。
- exit：`0` = 存活，`1` = 不存活，`2` = 无法判定（不可读／hunk 不匹配）。`--json` 下 `2` 也向 stdout 输出 `{"schema_version":1,"error":"…"}`。

**存活** = 错误级诊断、测试、契约中均无 `newly_broken`，且测试、契约中均无 `removed`（#4267）。删除测试不会产生 `newly_broken`，但删除不等于存活。有意删除通过 `--allow-removed`（`survive` 和 `apply` 都有）放行，MCP 的 `almide_survive` 不传此标志。名称仍在，却失去全部读取值的断言（`assert` / `assert_eq` / `assert_ne` / `testing.assert_*` 中至少有一个非字面量参数）的测试（如 `assert(true)` 或空体）会变为 `newly_broken`（`after: "hollowed"`）。

warning 的 `newly_broken` 会报告，但不计入判定（`almide check` 本身不因 warning 失败；unused-variable lint 只在无 error 文件运行，修好最后一个 error 会显露原有 warning，若因此拒绝，就会越是修复越被拒绝）。

**诊断同一性**（编辑会移动行，这是关键）：通过编辑文件前后文本的行 diff（LCS），把未变的行从旧行号映射到新行号。编辑过的行不映射任何位置，其他文件采用恒等映射。

1. `position`：相同检查目标、相同 `file`、相同 level / code / message，且编辑前位置映射后与编辑后位置的行列都一致。
2. `position_renumbered`：与 1 相同，但 message 删除数字串后相同（“first defined at line 12”等引用行号的消息，会随同一插入移动编号）。
3. `content`：前后任一侧在被编辑行上时，只要目标、file、level / code / message 相同，即视为同一（改写行后 error 仍在，就不是修复）。

未配对的编辑前诊断为 `newly_fixed`，编辑后诊断为 `newly_broken`。诊断本身是不良状态，因此 check 的 `removed` 始终为空。

**测试和契约分类**（同一性：文件 + 测试名／fixture + 契约 ID）：

| 编辑前 | 编辑后 | 分类 |
|---|---|---|
| 存在 | 不存在 | `removed` |
| 不存在或不良 | 良好 | `newly_fixed` |
| 不存在或良好 | 不良 | `newly_broken` |
| 良好 | 良好 | `unchanged` |
| 不良 | 不良 | `unchanged` |

测试状态为 `pass` / `fail` / `ignored` / `not_run`（二进制未运行至此）/ `compile_error` / `timeout`（良好 = `pass` 和 `ignored`）。契约为 `holds` / `diverges` / `timeout`（良好 = `holds`）。

**JSON（`schema_version: 1`）**：

```json
{
  "schema_version": 1,
  "file": "calc.almd",
  "edit": { "kind": "patch", "lines_before": 3, "lines_after": 3 },
  "written": false,
  "survives": false,
  "reach": { "root": ".", "checked": ["calc.almd", "calc_test.almd"], "tests": ["calc_test.almd"], "contracts": [] },
  "summary": { "check": {"unchanged":0,"newly_broken":0,"newly_fixed":0,"removed":0}, "tests": {"…": 0}, "contracts": {"…": 0} },
  "check": {
    "unchanged":    [ { "checked": "…", "matched_by": "position", "before": {…}, "after": {…} } ],
    "newly_broken": [ { "checked": "…", "before": null, "after": {…} } ],
    "newly_fixed":  [ { "checked": "…", "before": {…}, "after": null } ],
    "removed": []
  },
  "tests":     { "newly_broken": [ { "file": "calc_test.almd", "name": "add adds", "before": "pass", "after": "fail", "failure": {…} } ], "…": [] },
  "contracts": { "unchanged": [ { "fixture": "…", "contract": "C-NNN", "before": "holds", "after": "holds", "detail": {…} } ], "…": [] }
}
```

check 的 `before` / `after` **原样**携带 `almide check --json` 的诊断对象（`almide check --json` 新增字段原样到达）。`failure` 与 `almide test --json` 失败记录同形。未生成 JSON 即失败的 check 执行，将 stderr 一起保留在 `check.unstructured`，不默默视为 clean。改变形式（名称、类型、分类规则）须提高 `schema_version`。

LSP 自定义请求 `almide/survive`（params 为 `{textDocument: {uri}, text?, patch?}`；两者都没有时把打开的缓冲区视为提议）和 MCP 的 `almide_survive` 返回相同 JSON。两者都以子进程运行 CLI。

测试：`tests/survive_test.rs`（`tests/survive/` 中四个 golden delta：破坏测试的 patch；跨行插入仍保留一个 error 并修复另一个的全文编辑；仅注解的中性 patch；把带 `repair`（#2149）的 E031 原样放入 `newly_broken` 的 patch；另验证不写入，以及 apply 的拒绝与接受）、`tests/mcp_test.rs`、`tests/lsp_test.rs`

---

<a id="almide-apply"></a>

### `almide apply`

将 `survive` 判定和写入合并为一个操作（verify-then-write）。

```bash
almide apply src/calc.almd --with fix.patch --if-survives        # 生き残るときだけ書く
almide apply src/calc.almd --with fix.patch --force               # 判定に関わらず書く（判定は報告する）
```

- `--if-survives[=true|false]`：值只接受 `true` / `false`。其他值（`maybe`、`1` 等）不求值、不写入，直接 exit 2（fail closed）。`--if-survives=false` 只有与 `--force` 合用才有意义。
- 同时没有 `--if-survives` 和 `--force` 的 `apply` 为 exit 2（写入须有显式门禁）。
- `--force` 是不取值标志（`--force=yes` 为 usage error）。
- 写入是原子的（写同目录临时文件、fsync、复制原权限、rename）。判定期间文件已改变则不写入，exit 2。
- exit：`0` = 已写入，`1` = 未存活而未写入，`2` = 判定前拒绝。
- `--json` 在 `survive` JSON 中增加 `written` 和 `forced`。

测试：`tests/survive_test.rs`

---

<a id="almide-init"></a>

### `almide init`

创建新项目。

```bash
almide init
```

生成内容：

```
almide.toml               [package] name, version, edition
src/
  main.almd               effect fn main テンプレート
tests/                    テスト用ディレクトリ
CLAUDE.md                 AI 向けプロジェクト説明
```

`name` 从当前目录名自动生成。

---

<a id="almide-update"></a>

### `almide update`

将已锁定 git 依赖推进到其 ref 的**当前远端 head**（#1131）。

```bash
almide update almai      # 指定の依存だけ
almide update            # tag 固定でない全依存
```

- `almide.lock` 有意保持 sticky（`fetch_all_deps` 复用 pin 保持可复现，对现有依赖执行 `almide add` 也写回同一 pin）。本命令是推进的唯一规范途径。
- **不移动 tag 固定的依赖**（否则改变清单要求本身；报告 skip）。
- 只改写改变的 entry，其他 pin 逐字节不变。
- 推进时丢弃该 ref 缓存目录，使下次获取重新下载。
- 用一次 `git ls-remote` 解析，不 clone。
- 接触网络前对照 `[trust] owners` 检查所有者（#4093）。
- `almide.toml` 中 `git` 与锁不同的依赖（来源变更），即使固定 tag 也丢弃旧 pin，从新来源重新锁定。构建拒绝来源变更并引导至本命令（[package-system.md §6.3](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/package-system.md)）。

输出为 `name <old12> -> <new12>`（首次锁定为 `name -> <new12>`）。

<a id="almide-add"></a>

### `almide add`

添加依赖包。顺序为“解析 → 所有者信任检查 → 存在检查（`git ls-remote`）→ 获取 → 写入 `almide.toml` 和 `almide.lock`”（#4093）。所有拒绝、失败均退出码 1，完全不改变两文件。

```bash
almide add bindgen                      # github.com/almide/almide-bindgen(パッケージ一覧のスナップショットで解決)
almide add almide/almide-bindgen        # 同上(パッケージ名 bindgen)
almide add user/repo@v0.1.0 --trust github.com/user   # github.com/almide 以外の所有者は --trust が要る
almide add --git https://example.com/team/repo.git --tag v1.0 --trust example.com/team mylib
almide add almide-graphics/ceangal2@v0.1.0 --subdir ceangal --trust almide-graphics  # サブディレクトリのパッケージ
almide add user/repo --dry-run          # 解決・信頼判定・存在確認だけを表示し、何も書かない
```

| 标志 | 含义 |
|---|---|
| `--trust <owner>` | 信任来源所有者（`host/owner`，github.com 可只写 `owner`），写入 `[trust] owners`。与解析所得所有者不同则报错 |
| `--dry-run` | 显示解析结果、信任判定、存在检查后退出，不获取、不写入（若添加会被拒绝则退出码 1） |

不存在 `--yes` / `--force` 或信任全部来源的环境变量。只有写明所有者的许可才能通过（[package-system.md §6.3](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/package-system.md)）。不存在的仓库报告 not found；如有近似 Almide 包，附上可执行的 `did you mean \`almide add <x>\``（仅建议，不自动替换）。

简写：
- `almide add name` → github.com/almide 的 `name` 包（包名与仓库名不同则查内嵌快照 `src/almide_packages.txt`，没有则 `https://github.com/almide/{name}`）
- `almide add user/repo` → `https://github.com/{user}/{repo}`
- `@v0.1.0` → `tag = "v0.1.0"`
- `--subdir <dir>` → `subdir = "<dir>"`（#3381、[package-system.md §6.1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/package-system.md)）。包名为 subdir 最后一段（指定 `--git` 时为第一参数）。

获取及检查（subdir 存在、`almide.toml` 的 package name 与依赖名一致、传递依赖的所有者）通过后，才写 `almide.toml` 和 `almide.lock`。失败时恢复两者。

---

<a id="almide-trust"></a>

### `almide trust`

将所有者加入 `almide.toml` 的 `[trust] owners`（#4093）。未经 `almide add` 添加的依赖（传递依赖、手写清单行）遭拒时，拒绝消息所示的操作即为此。

```bash
almide trust github.com/bob     # [trust] owners = ["github.com/bob"]
almide trust bob                # github.com/bob と同じ
```

---

<a id="almide-install"></a>

### `almide install`

```bash
almide install github.com/almide/<repo>          # 既定のブランチの最新コミット
almide install <spec> --tag v1.2.0               # タグ
almide install ./path/to/project                 # ローカルのディレクトリ
almide install <spec> --name tool --bin-dir ~/bin
```

获取包，以该包为根项目用 `--release` 构建，放入 bin 目录（优先级为 `--bin-dir`、`ALMIDE_INSTALL`、默认 `~/.local/bin`）。

- **信任**（#4184）：来源所有者（`host/owner`）仅限默认信任的 `github.com/almide` 或 `--trust <owner>` 明确指定者。其他来源**获取前**拒绝，与 `almide add` 同规则。本地目录无需信任许可。
- **原生代码**（#4184）：包自身原生代码（`native/` 下的 Rust、`[native-deps]` crate 及其 build script、`@extern(rust|c)`）在构建时以用户权限运行。`[permissions] native` 只检查依赖，所以 install 列举这些代码，没有 `--allow-native` 就拒绝。纯 Almide 包不需标志即可安装。
- **位置**（#4183）：名称（`[package] name` 或 `--name`）必须为单个普通标识符（英文字母、数字、`_`，不以数字开头）。先在 bin 目录临时名称下生成，再 rename 放置，绝不写入目标 symlink 指向位置。非 almide 安装的现有文件（bin 目录 `.almide-installed` 没有记录者）或 symlink，没有 `--force` 不替换。

测试：`tests/install_destination_test.rs`、`tests/install_native_gate_test.rs`

<a id="almide-deps"></a>

### `almide deps`

显示依赖包列表。获取依赖，对各包（先直接依赖，再经其引入的依赖）显示 version、能力集合（`[permissions].allow` 检查的类别；`@extern` fn 为全部类别）、带入的原生代码（`native/`、`[native-deps]`、`@extern(rust|c)` fn），以及是否由 `[permissions].native` 授权（#4093）。带未授权原生代码的依赖会被 `build` / `run` / `test` / `bench` 拒绝（`docs/specs/package-system.md` §6.4）。

```bash
almide deps
# bindgen = https://github.com/almide/almide-bindgen (v0.1.0)
#   version: 0.1.0
#   capabilities: none
#   native code: none
# nat = path ../nat
#   version: 0.1.0
#   capabilities: Abort, Env.read, Env.write, FS.read, FS.write, Fan, IO.stderr, IO.stdin, IO.stdout, Net.fetch, Net.listen, Proc, Rand, Time
#   native code: native/natval.rs; @extern fn `number` (src/mod.almd:2) (granted by [permissions].native)
# ceangal = https://github.com/almide-graphics/ceangal2 (v0.1.0) subdir ceangal
#   ...
```

---

<a id="almide-dep-path"></a>

### `almide dep-path`

输出依赖包的本地缓存目录。

```bash
almide dep-path bindgen
# ~/.almide/cache/.sources/<encoded-source>/@/<commit>/src
almide dep-path ceangal     # subdir 依存: 共有クローン内のパッケージディレクトリ
# ~/.almide/cache/.sources/<encoded-source>/@/<commit>/ceangal/src
```

用途：通过 `process.exec("almide", ["run", path])` 执行依赖包 `.almd` 文件时获取路径。

---

<a id="almide-clean"></a>

### `almide clean`

清理缓存，六类目标：

| 目标 | 位置 |
|---|---|
| 依赖缓存 | `~/.almide/cache/` |
| 增量缓存 | `./.almide/cache/` |
| 编译缓存 | `./target/compile/` |
| native 构建临时目录（#2500） | `ALMIDE_RUN_PROJECT_DIR`（默认 `<cache base>/run`）及 `<cache base>/build-cdylib` |
| `almide test` worker 目录（#2504） | 逐个 `<cache base>/test/native/<key>/` |
| 运行时 rlib（#2504） | 逐个 `<cache base>/almide-rtlib-<key>/` |

**构建临时目录的“清空”是完整行为，不是清理未完。** 获取各目录 `.almide-build.lock` 后才删除内容（进行中的构建完成后再删），因此目录保留为仅含 lockfile 的空目录。故意不删除 lockfile：删除后，“已经打开该 lockfile 等待的 builder”和“下一个创建新 lockfile 的 builder”会**锁定不同 inode，破坏互斥**，同目录两个构建同时运行，一方二进制变成另一方的内容（与 #1877 相同故障）。删除的是空间占用（每个 worker 数 MB），保留的是一个 0 字节文件。

每个清空目录向 stderr 输出一行 `Cleaned <path>`（worker 和 rlib 目录各自汇总为 `Cleaned <native> (N test worker dir(s))` / `Cleaned <temp>/almide-rtlib-* (N runtime rlib dir(s))`）。无内容则 `No cache to clean`。

```bash
almide clean
```

测试：`tests/run_cache_recovery_test.rs`

---

<a id="almide-verify"></a>

### `almide verify`

由**独立版本的单独二进制 `almide-verify`** 重新检查 `almide build --target wasm` 交付的 wasm 模块证书（ownership / call-modes / bounds / names / caps / caps-transitive witness）（#2152、#4011）。`almide verify` 自身不含检查器，只是 subprocess shim，按 **almide 可执行文件旁 → PATH** 的顺序查找并启动 `almide-verify`，原样转发标准输入输出和退出码。找不到时**没有链接进来的 fallback**，以具名错误 `error[verifier-missing]` 和退出码 127 结束。

```bash
almide build app.almd --target wasm               # app.wasm を書く
almide verify app.almd                            # 証明書 bundle を生成し、app.wasm の SHA-256 に結びつけて almide-verify bundle に渡す
almide verify app.almd --artifact out/app.wasm    # 結びつける成果物を明示する（無ければ・一致しなければエラー）
almide verify app.almd --emit app.bundle          # bundle をファイルに残す（almide-verify が無くても書く）
almide verify --mir app.almd --emit m.bundle      # MIR lowering の witness（coverage.sh 用、ビルドとは無関係）
almide verify ownership w.cert                    # .almd 以外の引数は almide-verify にそのまま渡る
almide-verify --version                           # 検査器自身の版数（コンパイラとは独立）
```

- `almide` 侧运行的只有**不可信 producer**。按与 `almide build --target wasm` 相同路径（structural leg）构建程序，将 emit 时记录的 witness 写入 bundle（`crates/almide-wasm/src/witness_bundle.rs`；与 CI 的 `proofs/structural-wall.sh` 所判定 bundle 使用同一 producer）。判定始终由 `almide-verify` 完成。
- **绑定产物**：若 `--artifact` 指定文件或 `almide build` 默认输出（`app.almd` 对应 `app.wasm`）存在，确认其字节与本次构建输出模块相同，并将 SHA-256 写入 bundle（version 2 的 `artifact` 行）。`almide-verify` 从文件重算 digest，不匹配则 REJECT。producer 拒绝不匹配的旧文件（退出码 1）。文件不存在则生成 version 1 bundle（针对该构建输出模块的判定），并用 note 说明。
- 以 component 交付的程序，如 `http.serve`，不在范围内（退出码 1，显示理由）。对象是普通 `almide build --target wasm` 写入的 core module。
- `almide-verify` 不链接任何编译器 crate（`crates/almide-verify`，零依赖）。各性质的判定转写自 `proofs/` 中的 Coq 检查器（`check_xc` / `check_names_cert` / `check_caps_cert` / `check_prog_cert` / `check_modes_cert` / `check_bounds_cert`），定理由 Rocq kernel 检查。
- 无法记录 witness 的 frame 在 bundle 中具名列为 `uncertified`，不默默跳过。
- 如 `./almide.toml` 有 `[permissions].allow`，将 `effect fn` 的声明能力限制于其中（使 caps witness 可拒绝）。普通 `fn` 上限为控制台输出（ADR-0022），清单不能扩展它。
- `almide verify --mir app.almd [--emit b]` 是 #4011 前行为：写 MIR lowering 的 witness（逐函数 ownership / names / caps 及 call-modes）。#2761 后不再由 MIR 产生交付 wasm 字节，因此判定的是 MIR 而非构建。保留是因为 `proofs/coverage.sh` 通过此路径驱动 MIR 证书 producer，其 coverage floor 是 ratchet（不可与 `--artifact` 合用）。判定 MIR witness 的 gate 为 `proofs/gate.sh` / `proofs/corpus-wall.sh`。

`almide-verify` 退出码：0 = 所有 witness ACCEPT（CERTIFIED）；1 = 有 REJECT，或 witness 为 0；3 = 全部 ACCEPT 但存在 uncertified 函数（INCOMPLETE）；2 = 用法错误、输入不可读、bundle 无效。

测试：`tests/verify_shim_test.rs`（shim 委派、不存在时具名错误、退出码转发、绑定产物与拒绝旧文件、超出清单能力的 REJECT）、`crates/almide-verify/tests/coq_examples.rs`（全部 Coq `Example` 及 `build-checker.sh` 固定行）、`crates/almide-verify/tests/cli.rs`

---

<a id="legacy-mode"></a>

## 旧式模式

首个参数若为以 `.almd` 结尾的文件名，则作为 `emit` 命令处理：

```bash
almide app.almd --target rust           # → almide emit app.almd --target rust
almide app.almd --target rust --repr-c  # #[repr(C)] 付き Rust 出力
almide app.almd --emit-ast              # AST を JSON で出力
almide app.almd --emit-ir               # 型付き IR を JSON で出力
```

---

<a id="exit-codes"></a>

## 退出码

| 代码 | 含义 |
|---|---|
| 0 | 成功 |
| 1 | 编译错误、测试失败、依赖解析失败、fmt --check 差异等 |
| 2 | 命令特有的用法、输入、无法判定错误（见各节） |
| 3 | almide verify：witness 为 ACCEPT，但仍有未认证函数，属于 INCOMPLETE |
| 5 | almide test：运行对象为 0（可用 --allow-no-tests 更改） |
| 127 | `almide verify`：找不到独立检查器 `almide-verify`（`error[verifier-missing]`） |

---

<a id="global-options"></a>

## 全局选项

| 选项 | 说明 |
|---|---|
| `-h, --help` | 显示帮助（也可用于各命令） |
| `-V, --version` | 显示版本（如 `almide 0.34.2`） |

---

<a id="環境変数"></a>

## 环境变量

`ALMIDE_*` 环境变量的唯一台账是 `almide_base::env::SWITCHES`（`crates/almide-base/src/env.rs`），
可用 `almide switches` 列举（#2205）。下表就是 `almide switches --md` 的输出。
`scripts/check-env-switches.sh`（CI `checks`）拒绝未登记名称和编译器端直接的
`std::env::var("ALMIDE_…")` 读取；`tools/almide-gates env-switches-doc` 拒绝本表与台账之间的差异。

- 布尔值只有一种解释：未设置、空值、`0`、`false`、`off`、`no` 为 OFF，其他值为 ON。
- `route`（强制执行某个分支）和 `gate`（绕过默认检查）类别的开关为 ON 时，
  在该进程第一次读取时，向 stderr 输出一行（`[almide] ALMIDE_X is set: …`）。
  强制路径得出的判定会表明这一点。
- **开关锁**（#4129）：`route` / `gate` / `ablation` / `trap` / `instrument` 类别会改变“编译什么、运行哪些检查”。
  发布构建（`almide --version` 自称 `(release…)` 的构建）会忽略从环境继承的这些开关，
  并在 stderr 输出一行 `[almide] ALMIDE_X is set but IGNORED: …`。若要启用，须在命令行通过
  `--allow-switch ALMIDE_X` 指名（可重复）。这是为防止 CI、`.envrc`、devcontainer、
  编辑器任务等不受开发者控制的环境，让编译器编译与已审查内容不同的源文件，或跳过检查。
  工具自行设置的开关（`almide test` 的 `ALMIDE_NO_RTLIB`、`--time-report` 的 `ALMIDE_FUEL_PROBE`）不受此锁限制。
  开发构建仍像以前一样读取环境，可用隐藏标志 `--lock-switches` 加上与发布构建相同的锁。
  锁抑制哪些开关取决于各开关声明的类别，因此会检查这些声明：对不受锁限制的类别
  （`debug` / `tool` / `runtime` / `harness` / `ci`），无论启用哪个开关，生成的
  Rust、wasm、原生程序的输出都不得改变（`tests/switch_scope_test.rs`）。
  发布构建默认启用锁，由 `almide_base::env::lock_engaged_by_default` 单元测试，
  以及发布工作流在产物本身上运行的 `scripts/check-release-switch-lock.sh` 验证。
- `--profile critical` 只要收到任意一个通过环境传入且为 ON 的此类开关，就会拒绝（exit 1），
  无论是否指定 `--allow-switch`。
- `instrument` 为产物加入测量设施（分配计数器、固定或施毒分配器）：程序的计算相同，
  但产物不同于出货版本，因此受锁限制。
- `harness` 是测试 harness（`tests/`、`crates/*/tests`、`tools/`）读取的钩子，`ci` 是仅由工作流和
  `scripts/` 使用的开关，`runtime` 是编译后的程序自身在运行时读取的开关。

> 固定原文快照表；此读者副本不参与生成或自动同步。上文描述正本及其检查，不表示对本翻译表进行了这些检查。

| 变量 | 类别 | 说明 |
|---|---|---|
| `ALMIDE_ABORT_SITE=value` | runtime | 原生测试子进程运行的 testing.assert_aborts 位置（ADR-0029）：由父进程在重新执行测试二进制时设置；切勿手动设置 |
| `ALMIDE_ALLOC_COUNT` | instrument | 使用计数分配器构建原生程序；`__almide_main` 返回时，在 stderr 输出 `__ALMD_ALLOC allocs=N deallocs=N reallocs=N peak=N`（原生借用 oracle 的分配测量通道，tests/native_borrow_oracle_test.rs） |
| `ALMIDE_BANG_RETURN` | ablation | 关闭 v1 lowering 中按位置进行的 `!` 脱糖，使每个 `!` 都进入绑定位置规则，或明确拒绝（decline-matrix 探针） |
| `ALMIDE_BASELINE_BIN=value` | harness | 引入套接字路径之前的编译器：tests/net_wasm_default_route_test.rs 将不使用套接字的程序的 preview 1 模块与其逐字节比较 |
| `ALMIDE_BENCH_DIR=value` | harness | 结构化分支性能探针的 fixture 目录（默认 `crates/almide-wasm/tests/perf`） |
| `ALMIDE_BENCH_MAIN_NS=value` | harness | `almide bench` 指给被测原生二进制的文件：拼接进去的 `main` 计时器将经过的纳秒数写入此处（#2980，仅 main 的测量边界） |
| `ALMIDE_BIN=value` | harness | 测试 harness、脚本和工作流驱动的 `almide` 二进制路径（默认先找 `target/release/almide`，再找 PATH） |
| `ALMIDE_BORROW_OWN_ALL` | ablation | 让 BorrowInsertion 像尚无推断时一样，拥有所有可借用参数；这是所有权认证器 C4 敏感性测试使用的消融开关，也是借用推断的性能调节开关 |
| `ALMIDE_BOUNDED_DEBUG` | debug | 输出 bounded-loop 绑定拒绝的原因（v1 lowering） |
| `ALMIDE_BUILD_PROVENANCE=value` | ci | 由 `build.rs` 在构建时读取：`release` 让 `almide --version` 显示 `(release)`，其他值（包括未设置）显示 `(dev)`。仅由唯一从标签构建的 `.github/workflows/release.yml` 设置，因此自称 release 的二进制理应来自那里（#2384） |
| `ALMIDE_BUILD_SHA=value` | ci | 由 `build.rs` 在构建时读取：`almide --version` 在构建类别旁显示的提交，截取前 9 个字符。由 `make install` 和发布工作流传入，而非构建脚本从 git 读取，以免每次提交后重新构建根 crate（#2384） |
| `ALMIDE_CACHE_MAX_BYTES=value` | tool | 每用户私有缓存基目录下各原生构建缓存的大小上限：包括 `almide test` 工作进程缓存（`<cache base>/test/native`，最久未使用的工作目录优先清空），以及构建临时目录中的每程序二进制与对象文件（`<cache base>/run`）。可为 `4G`（默认）、`512M`、字节数，或表示不限的 `0`/`off`（#2608） |
| `ALMIDE_CAPTURE_MOVE_OFF` | ablation | 让 CaptureClone 像 #2231 之前一样重新 clone 所有捕获值，而不是 move 仅由闭包使用的值；所有权认证器敏感性测试使用的消融开关 |
| `ALMIDE_CERTIFY_OWNERSHIP=value` | debug | 在 pass 流水线后运行原生所有权认证器（#2231）：`report` 输出每个违规，`fail` 遇到任一违规就中止构建，`off` 跳过；未设置时，debug 构建为 `fail`，release 构建为 `off` |
| `ALMIDE_CI_BASE=value` | harness | 分片 CI 为找出变更自身的文件而与 HEAD 比较的提交（PR 的 base sha、merge group 的 base_sha）：变更触及的每个 fixture 都加入 `ALMIDE_CI_SLICE` 分片；scripts/ci-slice-prepare.sh 将其解析为 `ALMIDE_CI_TOUCHED` |
| `ALMIDE_CI_SEED=value` | harness | 提交 sha，其前 7 位十六进制数字轮换 `ALMIDE_CI_SLICE` 的余数类，让连续提交判定不同分片（CI 传入 `github.sha`）；未设置时偏移量为 0 |
| `ALMIDE_CI_SLICE=value` | harness | `k` > 1 时，让每个遍历语料的测试和脚本判定按提交轮换的 fixture 的 1-in-k 分片，加上变更触及的 fixture，跳过整个语料的新鲜度／完整性检查，且从不重写台账（crates/almide-corpus/src/ci_slice.rs、scripts/lib/ci-slice.sh）；CI 在基底不是 main 的 PR 和 merge group 上设置；未设置或为 1 时，仍执行完整运行 |
| `ALMIDE_CI_TOUCHED=value` | harness | 按行列出变更触及的仓库相对路径的文件（由 scripts/ci-slice-prepare.sh 写入）；读取此文件，而不是针对 `ALMIDE_CI_BASE` 运行 `git diff` |
| `ALMIDE_COMPILER_STACK=value` | tool | 编译器驱动线程的栈大小，单位为字节（默认 256 MiB）；使其溢出的深层输入是回归测试的对象 |
| `ALMIDE_COMPONENT_ADAPTER` | route | 让 `--component` 经过 preview1 适配器，而不是直接发射 component |
| `ALMIDE_COMPONENT_P3` | route | 在 `--component` 下发射 WASI 0.3 component（通过 component-model streams 进行标准输入输出，异步 canonical ABI）。若程序的 op 集合包含 tcp / udp / name-lookup op，即使无此开关，也会构建为该 component，因为 preview 1 没有套接字；无需功能标志即可在标准 wasmtime 46+ 上运行（`wasmtime run app.wasm`，http 客户端加 `-S http` — ADR-0023 step 1） |
| `ALMIDE_CONTROL_ADMIT_CODEC_AT_INPUT` | ablation | 让 `@input_sink` 参数（`http.decode_json[T]`）接纳同时为 `Encode` 的类型（`Codec` 类型），而非报 E110：这是 row-22 输入类型 gate（tests/input_sink_test.rs）的负对照，其 over-post 单元必须从 E110 翻转为 `is_admin=true`；其他情况切勿设置 |
| `ALMIDE_CONTROL_ADMIT_HAND_DECODE` | ablation | 让请求 decode 流 gate（E110 的流检查部分，#4232）接纳手动解码到也用于输出的类型中的请求数据（通过 `User.decode(json.parse(http.body(req))!)` 解码为 `Codec` 类型）：这是 tests/input_sink_test.rs 的负对照，其手动解码程序必须从 E110 翻转为 `is_admin=true`；其他情况切勿设置 |
| `ALMIDE_CONTROL_BIN=value` | harness | 安全路径测试（`tests/effect_value_laundering_test.rs`、`proc_category_test.rs`、`permissions_routes_test.rs`）用来替代被测二进制的 `almide` 二进制；这是负对照：没有相应检查的构建会使其断言失败（#4151、#4155） |
| `ALMIDE_CONTROL_IGNORE_SKIP_DECODE` | ablation | 让 Codec derive 再次从输入读取 `@skip_decode` 字段：这是 #4108 字段跳过矩阵 gate（tests/codec_field_skip_test.rs）的负对照，必须使其 `admin=false` 单元翻转；其他情况切勿设置 |
| `ALMIDE_CONTROL_NO_TYPE_SHARING` | ablation | 让每次类型 clone 都复制整个树，而不是共享元素列表（ADR-0030 D5）：这是类型共享 gate（tests/compile_budget_test.rs）的负对照，解除类型大小预算后，其倍增链必须因此构造指数数量的类型列表；其他情况切勿设置 |
| `ALMIDE_CORPUS_FILTER=value` | harness | 对三路 oracle 测试所评估的 fixture 路径应用子串过滤 |
| `ALMIDE_CORPUS_SHARD=value` | harness | `k/N`（从 1 开始）在六个大型语料测试（#2381）中遍历排序后的 spec/wasm_cross 语料的第 k 个取模分片，在排序后读取；`merge/N` 从 `ALMIDE_CORPUS_SHARD_DIR` 读取 N 个分片的部分结果，判定整个语料的上限与以名称为键的 bridge 台账；未设置时为不分片的 gate |
| `ALMIDE_CORPUS_SHARD_DIR=value` | harness | 分片语料 gate 写入已遍历 fixture 列表、部分计数与 bridge 名称，且 `merge/N` 从中读取这些内容的目录；未设置时为不写入任何内容的本地分片 |
| `ALMIDE_CORPUS_WEIGHTS_DIR=value` | harness | 语料 gate 记录各 fixture 实测墙钟时间的目录（`weights/<column>.<gate>[.<k>-of-<N>].txt`、`stem<TAB>ms`），供 scripts/gen-corpus-weights.sh 整理为 proofs/corpus-weights.txt；均衡的 `k/N` 分片读取该表（#2457）。CI 将其指向 shard-partials 目录，使提交的表基于 runner 自己的比例生成（#2502）；未设置时不记录 |
| `ALMIDE_COVERAGE_CONDITION=value` | ci | coverage ratchet 的条件标签（用哪个基线行判定 push） |
| `ALMIDE_COVERAGE_NATIVE_LEG=value` | ci | `1` 让 proofs/coverage.sh 还运行原生 codegen 工作负载 `almide test spec/ --target rust`（#3195）；默认取 ALMIDE_COVERAGE_CONDITION |
| `ALMIDE_CWD=value` | runtime | 写入者的工作目录，由 `almide run` 为 wasm 主机设置，使相对 fs 路径像 native 上一样解析（C-137）；切勿手动设置 |
| `ALMIDE_DBG_ANF` | debug | 输出 lambda lift 或语句 inline 拒绝的原因（v1 lowering） |
| `ALMIDE_DBG_BANG` | debug | 输出 v1 bind lowering 对 `!` unwrap 的决策 |
| `ALMIDE_DBG_BORROW=value` | debug | 按不动点迭代逐轮转储包含指定值的每个原生借用推断签名键，以及各调用点查询的键 |
| `ALMIDE_DBG_CELLS` | debug | 输出被捕获／被修改／被 cell 化的变量集合（v1 lowering） |
| `ALMIDE_DBG_CONTLIFT` | debug | 输出 poison-oracle continuation lift 回滚的原因（v1 lowering） |
| `ALMIDE_DBG_DESUGAR_FN=value` | debug | 输出由值指定的 fn 完全脱糖后的函数体（v1 lowering；#2205 之前名为 `DBG_DESUGAR_FN`） |
| `ALMIDE_DBG_DESUGAR_RAW` | debug | 配合 `ALMIDE_DBG_DESUGAR_FN`，也输出脱糖前的原始函数体（旧名 `DBG_DESUGAR_RAW`） |
| `ALMIDE_DBG_ELEM` | debug | 输出列表字面量的 Block 元素拒绝的原因（v1 lowering） |
| `ALMIDE_DBG_FAN` | debug | 输出 fan lowering 的预取、模式与实例并行决策（结构化分支与嵌入式主机） |
| `ALMIDE_DBG_GINIT` | debug | 输出立即执行的 top-let 初始化 runner 的接纳集合，以及扩展 runner 拒绝的原因（v1 lowering，C-007） |
| `ALMIDE_DBG_LINK` | debug | 转储 wasm 链接需求集合和每个键的解析结果 |
| `ALMIDE_DBG_LOWER_FN=value` | debug | 对值指定的 fn，输出 v1 lowering 实际 lower 的完全脱糖后的函数体（旧名 `DBG_LOWER_FN`） |
| `ALMIDE_DBG_NESTED_MATCH` | debug | 输出嵌套 match 链被拒绝的原因（v1 lowering） |
| `ALMIDE_DBG_QQ` | debug | 输出每个 `??` 的 lowering 路径（match 优先或路径 fallback，v1 lowering） |
| `ALMIDE_DBG_ROUTER` | debug | 输出因注册签名而被拒绝的 stdlib 调用名称，以及不匹配信息和参数类型（v1 lowering） |
| `ALMIDE_DBG_TCO` | debug | 输出尾调用转循环的接纳决策（v1 lowering） |
| `ALMIDE_DBG_TRAP` | debug | 嵌入式主机捕获 wasm trap 时，输出其回溯和主机状态 |
| `ALMIDE_DBG_WHILE` | debug | 输出 while 循环 lowering 的决策（v1 lowering） |
| `ALMIDE_DEBUG_CALL_OPS` | harness | 输出每个已 lower 的 fn 的 call op（classify_corpus example） |
| `ALMIDE_DEBUG_EFFECTS` | debug | 输出 effect 推断 pass 的逐函数结果（原生 codegen） |
| `ALMIDE_DEBUG_MIR_OPS` | harness | 输出每个 MIR op 及其索引（classify_corpus example — 证书二分定位仪器） |
| `ALMIDE_DEFAULTS_DEBUG` | debug | 未找到 default 键时，输出记录 default 的解析结果（原生 codegen） |
| `ALMIDE_DEPS_CACHE_ONLY` | tool | 仅从磁盘现有内容解析依赖：依赖不在缓存中就报错，不 clone 任何内容，也从不写入 `almide.lock`。由 `almide mcp` 为每个工具子进程设置（#4179） |
| `ALMIDE_DISABLE_OPT` | ablation | 关闭所有可选 pass 后运行优化流水线（消融；始终启用的前置 pass 仍会运行） |
| `ALMIDE_DUMP_IR=value` | debug | 在原生流水线中转储指定 pass 后的 IR（逗号分隔，或 `all`）；在 v1 流水线中转储名称包含指定值的每个 fn 的链处理后函数体 |
| `ALMIDE_DUMP_MIR` | debug | 在原生 render 运行前输出每个已 lower 的 fn 的 op 流 |
| `ALMIDE_DUMP_VERIFY` | debug | 输出原生 render 的验证记录 |
| `ALMIDE_DUMP_WMIR=value` | debug | 输出名称包含指定值的每个 fn 已 lower 的 wasm 分支 op 流 |
| `ALMIDE_DWARF_FIXTURES=value` | harness | 用于 DWARF 行表 span 解析测量（`crates/almide-wasm/tests/dwarf_lines.rs`，ignored 测试）的待测 `.almd` 文件，以逗号分隔（#1315） |
| `ALMIDE_EXPECT_BASELINE` | harness | ALMIDE_BASELINE_BIN 未设置时，让 tests/net_wasm_default_route_test.rs 失败，而不是跳过字节比较 |
| `ALMIDE_EXPECT_TOOLS` | harness | 缺少外部工具（wasmtime、wasm-tools）时，让 harness 测试失败而非跳过；由 CI 设置 |
| `ALMIDE_F32_SWEEP_N=value` | harness | Float32 printer sweep 输出（`${x}` 和 `float32.to_string`）并在每个分支上与 Rust f32 Display 比较的 xorshift32 位模式数量（默认 100000；tests/float32_to_string_cross_target_test.rs） |
| `ALMIDE_FAKE_RUSTC_VERSION=value` | harness | tests/rustc_floor_test.rs 中伪 `rustc` 报告的版本，其他调用均转发给 `ALMIDE_REAL_RUSTC`（rustc 最低版本拒绝的探针） |
| `ALMIDE_FALLBACK_NAMES` | tool | 让 `almide test` 为每个未通过 wasm 分支的文件输出一行 `FALLBACK <file>`；wasm coverage ratchet 的数据来源 |
| `ALMIDE_FAN_COST_OFF` | ablation | 对成本模型（#3341）原本会保持串行的每个 `fan` 提案都采用并行；两个分支（原生线程和嵌入式 wasm 主机的实例）均恢复 #3341 之前的行为；交叉点测量和并行路径测试使用的消融开关 |
| `ALMIDE_FAN_PAR_OFF` | ablation | 不让 `fan` 分块 map 采用结构化分支的实例并行提案（#3003）：每个分块在本次运行自身的实例中串行运行 |
| `ALMIDE_FAN_SEQUENTIAL` | runtime | 在原生运行时串行运行 `fan.*`，并在异步钩子上构建 `--host js` fan，每个元素暂停一次，而非使用重叠协议（#3383）；用于测量的确定性调节开关，契约规定可观察结果相同 |
| `ALMIDE_FAN_THREADS=value` | runtime | 限制两个分支上 `fan` 并行 map 的 worker 数量，包括原生线程和嵌入式 wasm 主机的分块实例（#3003；用于线程扩展曲线测量的调节开关，契约规定可观察结果相同；默认是可用并行度） |
| `ALMIDE_FLAGS_BIN=value` | tool | 当被测编译器早于该标志时（#3883），research/benchmark/perf/bench.py 向此编译器查询原生 `--release` rustflags（`almide build --print-release-rustflags`）；它必须与被测构建使用相同的 release 标志；未设置时查询被测编译器 |
| `ALMIDE_FLOAT_SWEEP_N=value` | harness | float printer sweep 输出并在每个分支上与 Rust `format!` 比较的 xorshift64 位模式数量（默认 100000；tests/float_to_string_cross_target_test.rs） |
| `ALMIDE_FMOD_SWEEP_N=value` | harness | float `%` sweep 在每个分支上以 Float 和 Float32 与 Rust `%` 比较的 xorshift64 位模式对数量（默认 20000；tests/float_fmod_cross_target_test.rs） |
| `ALMIDE_FN_ESCAPE_OFF` | ablation | 让 BorrowInsertion 将每个 fn 类型参数都借用为 `&dyn Fn`，无论是否逃逸（#2288）；所有权认证器 C5 敏感性测试使用的消融开关 |
| `ALMIDE_FORCE_METER_CODEGEN` | route | 即使整个程序可由 v1 render，或其区域可拼接，也通过带独立确定性 meter 的标准 codegen 构建计量式（fan.bounded / fan.min / fan.timeout）原生程序；用于使 codegen meter 与 v1 render 对齐的差分检查（#3673） |
| `ALMIDE_FORCE_METER_SPLICE` | route | 即使整个程序可由 v1 render，也通过标准 codegen 构建计量式（fan.bounded / fan.min / fan.timeout）原生程序，并链接 v1 render 的区域；用于保持两条路径消耗相等的差分检查（#3669） |
| `ALMIDE_FUEL_PROBE` | route | 在原生 render 中插入 fuel 计费（charge 探针）；由 `almide run --time-report` 内部设置。仅限 native：wasm 分支没有计费轨迹（#2752） |
| `ALMIDE_FUZZ_BASE=value` | harness | 差分 fuzz 固定种子范围的首个种子（默认 0） |
| `ALMIDE_FUZZ_HOST_ORACLE` | harness | 以 host-oracle 模式运行差分 fuzz（对每个种子和模式，arm 选择是确定的） |
| `ALMIDE_FUZZ_ITERS=value` | harness | 差分 fuzz 固定范围覆盖的种子数量（默认 200） |
| `ALMIDE_GPU_REQUIRED` | harness | 找不到 wgpu adapter 时，让 GPU 分支 parity 测试失败，而不是输出 SKIP（#4236；`gpu-parity` CI 作业在 lavapipe 上设置它；crates/almide-gpu-run/tests/int_map_parity.rs） |
| `ALMIDE_HEAP_TRACE` | debug | 输出解释器的堆块分配与释放 |
| `ALMIDE_HTTP_MAX_RESPONSE_BYTES=value` | runtime | 由编译后的程序读取的 http 客户端缓冲响应字节数上限（默认 1 GiB；0 表示不限，#2825） |
| `ALMIDE_HTTP_TIMEOUT_SECS=value` | runtime | 由编译后的程序读取的 http 客户端连接与读取超时秒数（默认 30；0 表示无超时） |
| `ALMIDE_INLINE_OFF` | ablation | 关闭结构化分支的小型 scalar-fn inliner（#2980）：为 A/B 比较保留每次调用 |
| `ALMIDE_INSTALL=value` | tool | `almide install` 安装二进制的目录（覆盖默认的 `~/.local/bin`） |
| `ALMIDE_INTERP_SWEEP_THREADS=value` | harness | interp sweep 线程数；1 表示串行（#2381） |
| `ALMIDE_IR_FAULT=value` | route | 在指定优化 pass 后注入 IR 违规，以便观察 release 二进制中逐 pass verifier 变红（release 构建仅在指定 `--allow-switch ALMIDE_IR_FAULT` 时启用，#4129） |
| `ALMIDE_KEEP_SCRATCH` | tool | 保留而不删除 `almide test` 的临时构建目录 |
| `ALMIDE_LSP_TRACE` | debug | 输出语言服务器处理的每个 LSP 请求和响应 |
| `ALMIDE_MANIFEST_TREE_CHECK=value` | ci | parity manifest 生成器对陈旧工作树的拒绝（#2405，scripts/lib/oracle-header.sh）：`strict`（默认）拒绝不为 `<Cargo.toml version> (dev…)`、标记提交不是 HEAD，或无提交标记且早于源文件的 ORACLE；也拒绝落后于 upstream 的 worktree 和未跟踪的 spec/ fixture。`gate` 只保留未跟踪 fixture 检查（scripts/check-parity-goldens.sh 为 CI 产物作担保）。`off` 是有意覆盖 |
| `ALMIDE_MONO_DEBUG` | debug | 输出单态化的发现与实例化决策 |
| `ALMIDE_MP_PROBE` | debug | 输出 mut-param 分析决策（IR） |
| `ALMIDE_MUTATION_BASE=value` | ci | mutation gate 用于差分比较的基底 ref |
| `ALMIDE_MUTATION_BUDGET_SECS=value` | ci | mutation gate 的秒数时间预算：超过后不再启动新的 mutant，其余列为推迟（0／未设置表示无预算） |
| `ALMIDE_MUTATION_SCOPE=value` | ci | mutation gate 运行的 mutation 集合 |
| `ALMIDE_MUTATION_SHARD=value` | ci | 本作业在 mutation gate 中的分片索引 |
| `ALMIDE_MUTATION_SHARDS=value` | ci | mutation gate 拆分的分片数量 |
| `ALMIDE_NAMES_DEBUG` | debug | 输出原生名称验证映射和作用域内遮蔽决策 |
| `ALMIDE_NO_AVAIL_CHECK` | gate | 绕过 E081 stdlib 可用性检查（可用性探针构建时使用的测量逃生口） |
| `ALMIDE_NO_RTLIB` | route | 内联构建原生运行时，而不是链接预构建运行时 crate（自包含 cargo 路径；`almide test` 为 harness 构建设置） |
| `ALMIDE_NO_VERIFIED_OK` | gate | 重新启用已退役的 `--no-verified` 分支（v0 fallback），而不是拒绝该标志 |
| `ALMIDE_OMEGA=value` | route | 用于确定性墙钟 deadline 重放的固化 ω 序数：产物不读时钟，在第 n 次墙钟检查时截断（`-1`／未设置表示实时） |
| `ALMIDE_OMEGA_RECORD` | route | 让原生产物在每个 deadline 已触发的区域出口输出 `__ALMD_OMEGA <ord>`（在 native 记录，任意位置重放） |
| `ALMIDE_ONLY_PASS=value` | ablation | 只运行指定的可选 pass（加上始终启用的前置 pass）；未知名称是硬错误 |
| `ALMIDE_ORACLE_KEEP` | harness | 保留原生 borrow-mode oracle 生成的程序（tests/native_borrow_oracle_test.rs），而不是运行后删除 |
| `ALMIDE_ORG_DIR=value` | ci | 跨仓库验证脚本遍历的 org 仓库 checkout 目录 |
| `ALMIDE_P3_HTTP_STOP=value` | ablation | 让 p3 http shim 在请求构建第 N 阶段（1..=5）之后立即返回其固定错误，以定位挂起 |
| `ALMIDE_PASS_EDGES=value` | harness | shuffle 遵守的额外 `A<B` pass 顺序边（逗号分隔）；用于找出 shuffle 分歧所需声明的那对 pass 的二分定位仪器 |
| `ALMIDE_PROBE_DUMP=value` | harness | heap 探针写入所发射 wasm 的路径 |
| `ALMIDE_PROBE_IR=value` | harness | heap 探针写入 lower 后 IR 的路径 |
| `ALMIDE_PROBE_OUT=value` | harness | 带计费探针的原生程序写入 `__ALMD_PROBE` meter 行的文件；`almide run --time-report` 将其设为新的临时路径，以免把程序自身 stderr 当作测量读数（#3489）。未设置时输出到 stderr |
| `ALMIDE_PROBE_SRC=value` | harness | heap 探针编译的源文件（未设置时跳过探针） |
| `ALMIDE_PROFILE` | debug | 输出原生流水线逐 pass 和逐阶段的计时 |
| `ALMIDE_REAL_RUSTC=value` | harness | 伪 rustc 在 tests/rustc_floor_test.rs 中对除 `--version` 外的调用所运行的真实 `rustc` |
| `ALMIDE_REGION_DEBUG` | debug | 输出 region-window pass 的决策（原生和结构化分支） |
| `ALMIDE_REGION_OFF` | ablation | 关闭 region-window 分配 pass（原生和结构化分支） |
| `ALMIDE_REGION_TRAP_STALE` | trap | 启用原生 region prelude 的陈旧引用 trap（#2200） |
| `ALMIDE_REPO=value` | ci | 发布脚本的目标仓库 slug |
| `ALMIDE_RUN_PROJECT_DIR=value` | tool | `almide run` / `almide build` 编译原生二进制时取代 `<temp>/almide-run` 的临时目录（按内容键缓存的二进制、其 cargo target、其 rustc incremental session）；`almide clean` 将其清空 |
| `ALMIDE_SCOPED_CLOSURE_OFF` | ablation | 让 BorrowInsertion 将每个 `let` 绑定闭包都视为闭包值，而不是不逃逸的作用域（#3455）；像引入作用域规则之前一样，其捕获值再次被拥有 |
| `ALMIDE_SEMLAW_CASES=value` | harness | semantic-laws property test 生成的案例数量 |
| `ALMIDE_SHUFFLE_PASSES=value` | gate | 按已声明依赖边允许的、带种子的随机顺序运行原生 pass；这是 pass 依赖探针，发射的 Rust 不得改变（#2186） |
| `ALMIDE_SIZE_ALONE=value` | harness | size ratchet 子进程为隔离检查而单独测量的一个 fixture（#2309）；由 ratchet 在其派生的进程上设置 |
| `ALMIDE_SIZE_PRESSURE=value` | harness | size-ratchet 子进程为确定性检查而构建单个 fixture 时使用的分配压力（`plain` / `quarantine` / `ballast`，#3143）；由 ratchet 在其派生的进程上设置 |
| `ALMIDE_SKIP_PASS=value` | ablation | 跳过指定可选 pass（逗号分隔）；pass 依赖探针，输出不得改变 |
| `ALMIDE_SKIP_VERSION_CHECK` | gate | 跳过项目的 `almide` 版本要求检查 |
| `ALMIDE_STDLIB_COVERAGE_PROBE=value` | harness | 为单个 fixture 探测而由 tests/stdlib_test_coverage_test.rs 测量的一个程序（仓库相对路径） |
| `ALMIDE_STDLIB_COVERAGE_REPORT=value` | harness | tests/stdlib_test_coverage_test.rs 写入逐函数覆盖表的位置（scripts/check-stdlib-test-coverage.sh） |
| `ALMIDE_STREAM_FUSION_OFF` | ablation | 关闭 stream-fusion pass |
| `ALMIDE_SURVIVE_OVERLAY=value` | harness | 已退役（#4129）：不再读取。`almide survive` 将 overlay 作为 `--survive-overlay` / `--survive-overlay-text` 参数传给子执行，因此环境不能让编译器读取不同于磁盘文件的文本；保留注册，以便回归测试设置它并观察无变化 |
| `ALMIDE_SURVIVE_OVERLAY_TEXT=value` | harness | 随 `ALMIDE_SURVIVE_OVERLAY` 一同退役（#4129）：不再读取 |
| `ALMIDE_TCO_DEBUG` | debug | 输出原生尾调用循环改写决策 |
| `ALMIDE_TEST_ABORT_PROBE` | runtime | 标记原生 testing.assert_aborts 子进程（ADR-0029），让 `panic` 在其中采用 C-219 abort 路径；由父进程设置，切勿手动设置 |
| `ALMIDE_TEST_LAX_WASM` | gate | 允许默认 `almide test` 通道（优先 wasm、回退 native）让 wasm 分支发生分歧的文件通过，即 native 重跑通过但 wasm trap；若无此开关，分歧分支会让整个运行失败 |
| `ALMIDE_TEST_LEDGER=value` | runtime | 原生 `almide test` 二进制逐个追加已完成测试名称的完成台账文件（#4255）；由 runner 设置，切勿手动设置 |
| `ALMIDE_TEST_SIGNALLED_CALL_URL=value` | harness | 将 almide-rt-core http 测试的自身重跑变为 signalled-call 子进程的 loopback URL（#3857）：它在某个 call 等待该 URL 时派生一个不存在的命令；由父测试设置，切勿手动设置 |
| `ALMIDE_TEST_VERBOSE` | tool | 显示 `almide test` harness 构建的完整 cargo / rustc 输出 |
| `ALMIDE_TEST_WASM_RUNNER=value` | tool | `wasmtime` 让 `almide test` 的 wasm 分支运行在 `wasmtime` CLI 上（标准 `to_wasi` 产物），而不是每个 `almide` 二进制内含的嵌入式主机（#3046）；这是比较路径，未设置时使用嵌入式主机 |
| `ALMIDE_TIME_PHASES` | debug | 输出每个 `almide run` 阶段的墙钟时间 |
| `ALMIDE_TMPDBG` | debug | 输出 v1 lowering 的临时值 drop 决策 |
| `ALMIDE_TOPLET_DEBUG` | debug | 输出 checker 跨模块的 top-let 类型写入和读取 |
| `ALMIDE_TRACE_PASSES` | debug | 在每个优化 pass 运行之前输出其名称，使不返回的 pass 可被识别 |
| `ALMIDE_UPDATE_ALLOC` | harness | 重新生成结构化分支的分配基线 |
| `ALMIDE_UPDATE_DUMPS` | harness | 重新生成结构化分支的 section-dump golden |
| `ALMIDE_UPDATE_GAUNTLET` | harness | 重新生成 gauntlet manifest |
| `ALMIDE_UPDATE_INSPECTION` | harness | 向 stdlib inspection 台账添加缺失行，并重置陈旧行 |
| `ALMIDE_UPDATE_INTERP_LEDGER` | harness | 重新生成解释器的 abstain 和 bridge-fallback 台账 |
| `ALMIDE_UPDATE_NAME_SITES` | harness | 从以名称为键的 property 台账移除陈旧行并追加新位置（每个仍需要其 `why`） |
| `ALMIDE_UPDATE_NATIVE_OWN` | harness | 重新生成原生 result 所有权台账 |
| `ALMIDE_UPDATE_NATIVE_VERIFIED_FLOOR` | harness | 重新生成 proofs/native-verified-floor.txt，即验证式原生 renderer 覆盖的 spec/wasm_cross fixture（#4006，tests/native_verified_floor_test.rs） |
| `ALMIDE_UPDATE_SIZES` | harness | 重新生成结构化分支的大小基线 |
| `ALMIDE_UPDATE_SIZE_LADDER` | harness | 重新生成 stdlib 链接 size ladder 台账（#2141） |
| `ALMIDE_UPDATE_SNAPSHOTS` | tool | 与 `almide test --update-snapshots` 相同 |
| `ALMIDE_UPDATE_STDLIB_COVERAGE` | harness | 根据实测运行重写 stdlib 测试覆盖台账各行 |
| `ALMIDE_UPDATE_SURFACE` | harness | 重新生成已执行 surface 的 golden |
| `ALMIDE_UPDATE_WITNESS_FLOOR` | harness | 重新生成证书 witness floor |
| `ALMIDE_VERBOSE` | debug | 与 `almide -v` 相同：显示静默运行隐藏的原生拒绝及 fallback 注记 |
| `ALMIDE_VERIFIED_DEBUG` | debug | 指出完成 render 的 wasm 分支及另一分支拒绝的原因（route oracle） |
| `ALMIDE_VERSION_LINE=value` | ci | 不是运行时从环境读取：`build.rs` 将其作为 `cargo:rustc-env` 发射，`src/main.rs` 在编译时用 `env!` 读取。它是 `almide --version` 输出的字符串 — `<version> (<kind>[, <sha>])`，由 ALMIDE_BUILD_PROVENANCE 和 ALMIDE_BUILD_SHA 组合而成（#2384） |
| `ALMIDE_WALL_REASON` | debug | 让 `almide test` 指出 wasm 分支具体在哪个阶段拒绝了 fallback 文件，而不只显示 `v1 wall` |
| `ALMIDE_WASM_ALLOC_COUNT` | instrument | 发射结构化 wasm 分支的分配计数器（#2407：五个 i64 global，`$alloc`/`$free`/region restore bump，导出为 `__alloc_count` / `__alloc_reused` / `__alloc_bytes` / `__free_count` / `__region_reclaimed`；`main` 返回前还释放其 top-let global），并让 `almide run --target wasm` 在运行后向 stderr 输出 `__ALMD_WASM_ALLOC allocs=N reused=N bytes=N frees=N reclaimed=N live=N heap_end=N`，其中 `live` 表示从未释放的堆块数；关闭时，模块与不使用该开关的构建逐字节相同（`ALMIDE_ALLOC_COUNT` 的 wasm 对应项；台账为 crates/almide-wasm/tests/golden/alloc-count-baseline.txt 和 live-at-exit-baseline.txt） |
| `ALMIDE_WASM_CANONICAL_ALLOC` | instrument | 发射不带 free-list 路径的结构化 wasm 分支 `$alloc`，使每个请求都执行 bump（stdlib inspection meter 的固定分配器，owner 于 2026-10-15 作出的裁定）：实测 bytes-moved 序列于是只依赖程序的请求，不依赖堆中保留了哪些已释放块，并成为出货模块复制量的上界；关闭时，模块与不使用该开关的构建逐字节相同（crates/almide-wasm/src/canonical_alloc.rs） |
| `ALMIDE_WASM_CHECKED_ALLOC` | instrument | 发射结构化 wasm 分支的 `$free`，用 0xA5 对已释放的 size-class 块施毒；`$alloc` 在重用前检查毒值，若该块空闲期间发生写入则 trap（#4009：wasm 分支的 use-after-free 与杂散写入检测器；scripts/wasm-checked-alloc.sh 在此模式下运行语料）；关闭时，模块与不使用该开关的构建逐字节相同（crates/almide-wasm/src/checked_alloc.rs） |
| `ALMIDE_WASM_FREES` | ci | frees-churn gate 的开关；其编译器读取端随 v0 emitter 退役（#782），仍设置它的是 #2207 的 gate |
| `ALMIDE_WASM_SKIP_STOCK_AUDIT` | route | 跳过 `almide build --target wasm` 的标准 WASI host-op 审计（parity、diff-fuzz、availability gate 构建时使用的 emitter-frontier 探针）；#2934 之前名为 ALMIDE_WASM_STRUCTURAL |
| `ALMIDE_WASM_WATCHDOG_SECS=value` | harness | 进程内测试 runner 的 epoch watchdog（almide-wasm-run 中的 `run_wasm` / `run_wasm_with` / `run_wasm_capped`）允许模块运行、直到以 `interrupt` trap 的墙钟秒数（默认 30）；如同 native 没有 watchdog 一样，`almide run --target wasm` 和 `almide bench --target wasm` 在任何设置下都不启用 watchdog（#2615） |
| `ALMIDE_WITNESS_DUMP` | harness | 在 witness-floor 测试中输出每个 fixture 的证书 witness |
| `ALMIDE_WRITE_FUZZ_CORPUS` | harness | 将生成的 fuzz 程序写入磁盘 |

`ALMIDE_*` 以外：

| 变量 | 说明 |
|---|---|
| `CI` | `true` 等同于 `almide test --ci`（不写入快照） |
