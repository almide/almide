<a id="adr-0013-native-string-as-a-shared-buffer--implementation"></a>

# ADR-0013：native String 作为共享buffer — 实现

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](adr-0013-string-sharing.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](adr-0013-string-sharing.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](adr-0013-string-sharing.zh-CN.md)

> 非规范性固定原文历史实现读者版。原本不变，不自动同步，未独立重跑benchmark、corpus检查、Miri或检查stamp。stage 1已落地、stage 2仅分支完成、早期下一步/待办与10月17日最终flip分层保留，不断言当前merge/支持状态。测量保留机器、版本、workload、排除file、harness边界；变更harness后的数字明确不可跨表比较。取整零时间仍只是测量报告，不保证零执行成本。thread安全依赖所述marking纪律，此处无独立证明；不因后来完成措辞暗中补齐早期Miri/覆盖缺口。命令、GCE运行、owner裁定是历史记录，不是新执行或付费授权。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/adr-0013-string-sharing.md)

**状态：** stage 1已落地（#3715，2026-10-15）。stage 2在 `a13-flip-final` 分支完成（2026-10-17）：
- 已落地：step 1（alias，#3716）、类型（#3717）、literal（#3719）；
- 分支上：steps 3–6、thread边界、step 7——共享buffer是唯一native Almide String，`ALMIDE_SHARED_STR` 开关已删除（见末尾“Step 7”）。
**决策：** [ADR-0013](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0013-string-shared-immutable-buffer.md)（2026-09-04接受）。
**owner裁定2026-10-15：** stdlib检查的各项修复之后立即实施。不要以改变条件性消费参数的borrow推断替代。

<a id="why-now"></a>

## 为何现在

stdlib检查测得 `result.filter` 在native每call O(n)，wasm O(1)。
调用把活 `String` 传给仅失败分支消费的参数，native在调用点render `x.clone()`。

同一形状在六种语言运行（macOS arm64，n=20,000→320,000时每call的µs）：

| 模型 | 每call | 随n增长 |
|---|---|---|
| Rust，owned参数+caller clone（当时Almide native） | 0.22 → 4.25 | 是（19倍） |
| Rust，borrowed+消费分支clone | 0.000 → 0.000 | 仅该分支 |
| Rust `Rc<str>`；Swift；Go；Python；Node；Ruby | 平坦 | 否 |

仅owned-copy模型为参数大小付费。wasm侧已共享（rc block）。这是从另一方向再次遇到ADR-0013发现。

<a id="stage-1-the-ratchet-landed"></a>

## Stage 1：ratchet（已落地）

反证条件：若strchurn或fasta慢**5%或更多**，thread-shared变体也无法恢复，则回退stage 2。
只有噪声明显低于5%的测量才能判定。

CI的 `check-perf-ratio.sh` 报告strchurn和decode，但不锚定，因为其ratio依赖架构。
所以反证条件改在一台固定机器判断：

- `research/benchmark/perf/gce_ab.sh <ref-a> <ref-b> <out>` 在单台一次性GCE VM构建两个ref，
  在相同bench source交替A、B、A、B运行 `bench.py`（完整workload，native+rust侧）。
- `research/benchmark/perf/ab_summary.py <out>/res` 汇总每bench：native时间中位数、各侧almide/rust比中位数、
  B对A、所有round的ratio波动。

**噪声底：** A/B都用develop 0d93660ea。GCE c4-standard-4
（Intel Xeon Platinum 8581C，4 vCPU）、Ubuntu 24.04、Linux 7.0.0-1014-gcp、rustc 1.99.0。
3 round ×5 run，中位数，2026-10-15。

| Bench | 对手写Rust比值 | B对A（同build） | 6 round波动 |
|---|---:|---:|---:|
| fasta | 1.164 | +0.07% | 1.18% |
| strchurn（同形参考） | 1.126 | −0.67% | 1.38% |
| decode | 0.783 | +2.03% | 3.11% |

- 反证行fasta、strchurn在约1.4%内重复，所以5%回归清晰可见。
- decode噪声约3%，只报告，不作反证，符合ADR。
- 这些是stage 2 baseline。

<a id="stage-2-the-representation-next"></a>

## Stage 2：表示（下一步）

develop 0d93660ea的清单：

| 表面 | 数量 |
|---|---:|
| `runtime/rs/src` 的 `pub fn almide_rt_*` | 809 |
| … 返回 `String`（或 `Result<String, …>`） | 61 |
| … 按值接收 `String` | 17 |
| runtime中 `Vec<String>` 出现 | 32 |
| 拼写 `String` 类型的codegen / MIR位置 | 17 |

编码前须定案的开放设计点：

1. **类型。** 首测：Rust microbenchmark，macOS arm64，5次最好，2026-10-15。
   prototype在session scratchpad，选择前在GCE重测。

   | 操作 | `String`（当时） | `Rc<str>` | `Rc<String>` | 单block `[rc, len, cap \| bytes]` |
   |---|---:|---:|---:|---:|
   | 构造2M短字符串 | 28.1 ms | 34.5（+21%） | 56.7（+100%） | 31.0（+10%） |
   | 共享100 KB字符串20k次 | 25.3 ms | 0.00 | 0.00 | 0.00 |
   | 向unique字符串push 2M字符 | 0.72 ms | 二次（无capacity） | 1.47（2倍） | 3.18（4.4倍） |
   | 2M次相等扫描 | 0.57 ms | 0.51 | 1.05 | 0.73 |

   - **`Rc<str>`** 没capacity，`string.push` 会每次重建字符串，排除。
   - **`Rc<String>`** 付两次分配和第二次间接访问。
   - **单block** 是wasm侧布局，构造和共享获胜。未优化prototype的unique push慢4.4倍，
     每push查count且无内联fast path。stage 2落地前必须降到strbuild ratchet内。
   - **push差距来自aliasing，不是count检查。** 字节存储通过compiler不能与header分开的pointer，
     每push重载 `rc`、`len`、`cap`。内联fast path无助（3.21 ms）。
   - **unique view** 解决它：count只查一次，len/cap留在local，view结束再写回。
     达到 **0.76 ms**，String为0.72。
   - 所以只要codegen对同一 `var` 连续push取一个unique view，单block可处处匹配 `String`。
     `var s` 的in-place append window已是这个连续区间。
2. **`mut` 表面。** `string.push` / `string.clear` 用RcCow（`Rc::make_mut` 语义）：
   unique原地，共享则复制。与List / Map已有规则相同。
3. **fan边界。** 用Koka thread-shared flip，仅捕获进scoped thread的值变atomic。
   stage 2 ratchet裁判；flip使strchurn回归则停。
4. **ABI。** runtime函数仍收 `&str`，通过 `Deref` 工作。返回 `String` 的函数返回共享类型，
   在边界转换一次。

Stage 3（`split` / `slice` / `trim` / `lines` 的无缝slice）在stage 2后。
Stage 4（内联短字符串）不在本文范围。

<a id="stage-2-the-migration-plan"></a>

## Stage 2：迁移计划

在develop 0d93660ea映射表面，file:line引用来自该commit。

**两个native renderer，按program选择。**
- `src/cli/mod.rs:240`（`render_v1_native_or_fallback`）尝试整program MIR v1，遇任何墙fallback到v0 walker。
  - v1对multi-module、top-level let、heap tail recursion、Codec / `Value` program设墙，所以多数实际program由walker render。
  - `almide test`、`install` 始终v0。
- metered program可将v1 region **splice**到v0 code（`mod.rs:323-328`），此处两renderer String表示须同类型。
- v1输出不含 `runtime/rs` 或codegen prelude，有自己header（`render_native.rs:229`）和shim（`render_native_shims.rs`）。

**步骤。** 每步独立落地，维持所有gate green。

1. **alias，不改行为。**
   - `type AlmideString = String;` 放入 `rust_runtime_prelude`（`crates/almide-codegen/src/lib.rs:343`）
     及v1 header各一次；metered splice最终须恰好一定义。
   - 所有拼写Almide String类型位置emit alias：
     - walker：`walker/types.rs:138`、`templates/rust.toml:242`、error类型default（`types.rs:172`、`expressions_calls_binops.rs:549`）；
     - v1：`render_native*.rs` 的 `NTy::Str` render。
   - alias透明，runtime不改即可编译。
   - gate：grep spec corpus生成Rust的类型位置，找不到bare `String`。
2. **flag后的类型。**
   - `AlmideStr`：单block `[rc, len, cap | bytes]`，带 `Deref<Target = str>`、`From<String>`、`From<&str>`、
     `Display`、按bytes的 `PartialEq` / `Ord` / `Hash`、`Clone` = count+1，`unique()` 返回前述已测view。
   - 类型单独unit test及Miri run。
   - flag `ALMIDE_SHARED_STR=1` 翻转alias，默认off。
3. **构造位置。**
   - literal变 `AlmideStr::from("..")`，更好是Swift literal那样static immortal block。
   - concat经 `AlmideStr` 的 `AlmideConcat`，左操作数unique时用unique view。
   - `format!` 结果在边界以 `From<String>` 转换。
   - error payload（`Result<T, AlmideStr>`）。
4. **runtime ABI。**
   - `&str` 参数（140）经 `Deref` 继续工作；返回（`-> String` 51、`Vec<String>` 16、
     `Option<String>` 13、`Result<_, String>` 124）在函数边界转换。
   - 四个 `&mut String`（`string.push` / `clear`、`value.rs:411`）收 `&mut AlmideStr` 并用 `unique()`。
   - `buildscript/runtime_registry.rs:314` 从runtime source text解析mode，所以pattern须识别新拼写。
5. **in-place window。这是正确性步骤，不是速度步骤。**
   - MIR v1 append window（`render_native_append.rs:28-45`）把 `s = __str_concat(s, x)` render成 `s.push_str(x)`。
     所述健全性依据是“每个 `Dup` 都是deep `.clone()`”（注释15–20行）；共享buffer下不再成立，须经 `unique()`。
   - walker的 `AlmideConcat<String> for String`（`lib.rs:348-353`）同义务。
   - gate：两侧alias fixture中 `let t = s; s = s + "x"` 必须不改 `t`。
6. **`AlmideValue::Str`**（`value.rs:18`）改为 `Str(AlmideStr)`，这是ADR-0018 / #1679阶段。
7. **翻默认。** 对stage 1 baseline运行 `gce_ab.sh`，strchurn/fasta不能慢5%或更多，然后删alias flag。

**保持可观察行为不变的措施：**
- 每步两侧运行全部spec corpus及cross-target契约。
- 2–6步直到第7步都在flag后，回归只需flip flag即可回旧表示。

**step 3–4 ratchet（测于2026-10-15）。**
- **全量检查：** alias指向 `AlmideStr`（probe flag），emit所有 `spec/lang`、`spec/stdlib` file（463），
  每个以 `rustc --test` 编译并计error。
  - flag off同harness 0 error，除standalone rustc无法link rustls的10个http file。
    属环境问题，两侧计数都排除。
- **迁移规则：** 每构造位置改为alias为 `String` 时恒等、为 `AlmideStr` 时转换的拼写，
  即 `AlmideString::from(..)` / `.into()`。flag off行为不能变，全量计数只能降。

| 状态 | flag on error |
|---|---:|
| 仅alias（#3716） | 8,924 |
| + literal为 `AlmideString::from("..")` | 7,326 |
| + `int.to_string` / `float.to_string` 返回 `AlmideString`（probe） | 3,657 |

此后还数**distinct site**（runtime内error会在全部463file重复）：

| 状态（probe，flag on） | error | distinct site |
|---|---:|---:|
| literal + `AlmideStr` 的 `AlmideConcat` impl | 6,664 | 1,421 |
| + 46个runtime `-> String` 函数返回 `AlmideString` | 2,631 | 1,193 |
| + 137个runtime `Result<_, String>` 签名写成 `AlmideString` | 9,501 | 1,137 |

最后raw count上升，因为一些error移到runtime自身，每个file重复；distinct site下降。
flag off每步均为0。

**flag从此真实存在（2026-10-17）。** `ALMIDE_SHARED_STR=1` 成注册switch：
prelude（`crates/almide-codegen/src/prelude_string.rs`）把 `almide-str` source拼入 `mod almide_str_block`，
alias指向 `AlmideStr`。全量检查不再需probe修改，harness也相应改：
- `almide <file> --target rust` emit，`rustc --test --emit=metadata` 类型检查，
  覆盖463减需rustls / flate2的10个，即453file。
- distinct site定义为（error code、message、问题source行）。
- flag-on prelude带 `AlmideStr` 的 `AlmideConcat` impl
  （经仅unique才原地append的 `AlmideStr::concat`），以及结构化 `into_almide`
  （String转换；`Vec` / `Option` / `Result` / tuple / `HashMap` / `AlmideMap` map元素；
  scalar和 `AlmideValue` 恒等）。
- 新harness重测起点，所以下列不可与上表比较。

| 状态（flag on、此harness） | error | distinct site | 有error的file |
|---|---:|---:|---:|
| 起点：已落地alias、literal、runtime返回、`Result` error | 15,643 | 724 | 453 |
| + runtime内部直到边界（`fan`、`fs`、`value`、`json`、`map`、`list`、`error`、`process_core`） | 959 | 681 | 187 |
| + 构造位置：String copy用 `to_almide()`，插值用 `AlmideString::from(format!(..))`，error payload、`ok_or`、decode frame key | 115 | 96 | 18 |
| + 无前缀runtime返回（`almide_regex_*`、`almide_json_*`、`ProcessStatus` 字段） | 61 | 44 | 14 |

剩余61error全为thread边界：`AlmideStr` 非 `Send`/`Sync`，到达 `fan` block、`fan.map_par`、
`fs.fold_lines_chunked` 或top-level `let`（`static LazyLock`）。这是设计点3（Koka thread-shared flip），不是构造位置。

| + 设计点3：每thread边界thread-shared mark | 0 | 0 | 0 |

**设计点3（thread边界）。** 这是Koka thread-shared flip
（`kklib.h`：负count为atomic，task开始前 `kk_block_mark_shared`）。
- `AlmideStr` count带 `ALMIDE_STR_SHARED` bit。marked block以atomic read-modify-write计数，
  最后drop配对 `Release` 与 `Acquire` fence。unmarked用 `Relaxed` load/store，即plain move。
  unique检查以 `Acquire` load。
- `almide_str_mark_reachable(&v)` 标记 `v` 可达全部内容。
  在marking window内clone `v`；窗口是令每clone置bit的global counter。
  无需逐类型walker：每Almide值都 `Clone`，如Koka遍历每block。
- compiler仅在所emit的每边界标记：
  - threaded `fan` block各arm自由变量；
  - 传给签名bound `Send` 的runtime helper的参数自由变量
    （从签名生成的 `runtime_fn_modes::crosses_threads`）；
  - helper发给worker的数据（`fan.map_par` item、`list.par_*`、`fold_lines_chunked` init）；
  - `static LazyLock` 交给每thread的top-level `let` 值。
- `unsafe impl Send / Sync for AlmideStr` 依赖此marking；unmarked block绝不会从两个thread可达。
- 只有越边界值转atomic。marking window中另一thread做clone也会标block，只损速度。
- flag off时 `almide_share()` 是空default method，runtime call编译成无，compiler不emit mark。

**flag on，corpus运行。** 同463file以 `rustc --test`、opt-level 0构建并native运行：
on通过438，off通过452，差异是14个thread边界file。
设计点3后（加 `spec/stdlib/thread_shared_string_test.almd`，共464file），两侧都453通过，
非通过同11file：10个rustls / flate2 file不能standalone rustc link，
一个deep-recursion test在此harness opt-level 0溢出默认test-thread stack（两侧都如此）。

**Step 5（`let t = s; s = s + "x"`）。** `spec/lang/alias_cow_test.almd` 固定直线和loop形状，
wasm与native flag on/off均通过。共享buffer下walker `+` 经 `AlmideStr::concat`，
v1 append window的 `s.push_str(x)` 经 `AlmideStr::push_str`。
二者取unique view，写前复制共享block。

**v1 renderer与http随后。**
- v1 header在flag下splice同block（`crates/almide-mir/src/render_native_str.rs`），string shim/literal构建 `AlmideString`。
- `http.rs` 与SSE client返回 `AlmideString`，并接收此类型的header map、chunk callback。
- 全量检查看不到这两者：v1不是 `--target rust` 路径，http需rustls。
  - http：排除的10file用flag on的 `almide build`（含rustls/flate2的cargo路径）构建，10个全编译。
  - v1：尚无corpus测量。一个覆盖append window及每个string shim的program用flag on走v1，输出byte与off相同。
  - **v1全量检查。** `probe_native`（作为example binary的v1 renderer）跑543program：
    `spec/`、`examples/`、`research/benchmark/perf`、`exercises`。
    - 只有7个经v1 render；其余536例如因 `env.args` wall到walker。
    - flag on全部7个类型检查及运行通过，stdout与exit code均与off相同。

**尚未覆盖：**
- 新atomic路径（`crates/almide-str/tests/almide_str_threads_test.rs`）Miri只在CI（`miri-almide-str`）跑，
  此机器无Miri的rustup toolchain；
- 默认flip（step 7），等GCE A/B；
- flag下速度：`&str + AlmideStr` 和v1 shim结果多复制一次，`to_almide()` 在count增加足够处也复制。
  step 7的 `gce_ab.sh` 负责判定。

**Step 6（`AlmideValue::Str`）。** payload现为 `Str(AlmideString)`，所以flag下Value共享构造来源string block；
ADR-0018对 `AlmideStr` 判○。
- Object key仍 `AlmideKey`（intern的 `Cow<'static, str>`）。
- 改9行runtime拼写，alias为 `String` 时均恒等。
- flag off生成用户代码在全部463file byte相同（比较 `RT_BOUNDARY_MARKER` 后部分），on/off全量检查均0error。
- `spec/stdlib/value_string_payload_test.almd` 固定payload路径：`value.str`、push来源、parse/stringify、
  derived Codec round-trip、多Value共享一string。wasm与native on/off通过。
- native corpus（465file）：on/off都454通过，同11个非通过。

**Step 7（flip，2026-10-17）。** 反证条件先失败：GCE
（c4-standard-4、Xeon 8581C、`gce-perf-ab.sh`、3交替round×5run、中位数）strchurn
`String` 0.478s，共享buffer0.675s，慢41%；fasta不变。耗时原因及修复：
- **每string结果构建两次。** runtime先render `String` 再转换（`.into()` / `into_almide()`），
  每string再分配、copy、free。`int.to_string`、`string.join`（30 MB copy）、
  `string.split`（4M片段和第二个 `Vec`）现构建一个exact-size block
  （`AlmideStr::from_i64`、`AlmideStr::join`、每片段 `AlmideString::from`）。
  本地指令数由 `String` 的1.72倍降至1.04倍。
- **24byte header让每短string上升一个allocator size class。** glibc中7byte string是48byte chunk，
  而 `String` bytes为32byte。header现为 `[rc: u32, spare: u32, len: usize]`、16byte；
  capacity是 `len + spare`，只限制spare（到 `u32::MAX`；分配/增长clamp，unique view结束超出时缩block）。
  GCE三路profile：`String` 0.4545s、24byte header0.4855s（+6.8%）、16byte0.3925s（−13.6%）。
- release runtime rlib曾静默build失败（`pub #[inline] fn`），每个共享buffer release构建fallback到cargo；
  现单测固定attribute顺序。
- unique block在drop fast path先于其他检查释放。

最终反证判定（GCE A=`adr0013-stage2` 带 `String`，B=`a13-perf`）：
strchurn0.4565s→0.3925s（**−14.0%**，almide/rust ratio1.117→0.953）；
fasta3.177s→3.186s（+0.27%，波动0.77%）。二者都低于5%线。

随后删除switch（不剩 `String` 路径）：
- `prelude_string.rs` 只render共享prelude；`almide_into_str_impls!` gate、env-switch行及其 `docs/specs/cli.md` 行删除。
  thread边界mark、lazy-static mark无条件；v1 header始终splice block。
- **`@extern(rust)` host保留已文档化 `String` 签名**（docs/specs/module-system.md §11.1）。
  wrapper在边界转换：List / Set / Map / Option / Result / tuple内含String的值，入口copy为 `String` 形
  （`mut` 则call后copy back）；返回值copy为 `AlmideString` 形，无论host返回 `String` 还是已为 `AlmideString`。
  plain `String` param仍 `&str`。`ret_string`、`ret_list_str`、`ret_opt_str`、`mut_str`、`mut_list_str` 行固定它。
- **以 `String` 为key的Map / set hash。** `almide_rt_map_key_hash` 按 `TypeId` 识别 `String`
  （及 `(String, _)`、`Vec<String>`）key，所以 `AlmideString` key落到线性路径：
  500,000string的 `list.unique` 花27s，`map.update` rebind每step复制。downcast现写 `AlmideString`。
- v1 renderer曾用 `.to_string()`（`String`）从borrowed string生成owned copy，
  现为 `AlmideString::from(..)`，其 `int.to_string` / concat shim构建一个block。
- `UniqueStr` 改为 `AlmideUniqueStr`（bare top-level item名），`FromIterator` 写全路径
  （v1 render也以edition 2015编译）。
- 插值render `AlmideString::from_fmt(format_args!(..))`（stack buffer、一个exact block），
  `+` 左侧literal向owned右操作数prepend（`AlmideStr::prepend`），因此native分配台账只降：
  str389→349、strs822→708、map569→524、rec473→378、recs766→686；list/set/bytes不变。

未完：许多runtime函数仍返回 `let __almide_out: String = ..; __almide_out.into()`，每结果二次分配；
各是与 `int.to_string` 同类的局部修复。

**runtime ABI步骤使检查stamp失效。** runtime函数文本变化会改实现hash，
`tests/stdlib_inspection_test.rs` 随后要求重检。仅 `-> String` 步骤就使38个已stamp函数失效。
所以ABI步骤（return、`Result`、`Vec`、`Option`）集中一branch，在提议前于GCE重检一次。
