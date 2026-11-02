> 说明（原本索引元数据的非活动副本）：标准库重设计第 1 层——对照九种语言的模块清单：新增、保持分离、移动的类别，以及环境、时间、进程的职责划分
<a id="l1-カテゴリモジュールの一覧"></a>

# L1：类别（模块）清单

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L1-categories.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L1-categories.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L1-categories.zh-CN.md)

> 非规范性历史重设计参照版，固定原本权威且不变。不自动同步，未核实现行API/他家规范或独立测量。[F]是原作者当时核查，不是此次翻译重新获取。保留采纳/拒绝/计划、原计数，以及初始3条件后2026-10-24所有者例外。跨target、fusion、Secret显示/常数时间和SafeHtml为原主张，不构成独立或普遍安全/性能保证。CRC32初始归属摘要与后段Python例外不擅自调和。保留的伪代码中型意为类型。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)

> 最后更新：2026-10-21
> 前提：[L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)
> 依据表：[facts/categories.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/facts/categories.md)（约 100 类别 × 9 种语言；● = 标准提供，◐ = 有条件标准提供，pkg = 仅官方包，[F] = 本轮通过官方文档核实的单元）

<a id="判定の基準"></a>

## 判定标准

仅在以下 3 条全部满足时**添加类别**：
1. 9 种语言中至少 5 种标准提供（● 与 ◐ 总数）。
2. 可写成纯计算，或通过 WASI 标准接口运行于 wasm（R-L0-15）。
3. 当前函数或既有裁定无法覆盖。

仅在违反 L0 规则（R-L0-01 同一角色一种拼写，R-L0-02 放在主体类型模块）时**拆分、合并、移动类别**。不会仅因划分方式与多数不同而移动。

---

<a id="1-いまの-44-モジュールと-9-言語"></a>

## 1. 当前 44 个模块与 9 种语言

| 分类 | Almide | 9 种语言的处理（摘要） | 判定 |
|---|---|---|---|
| 数值 | `int` `float` `math` | 9/9 提供 | 保持（内容在 L2） |
| 定宽数值 | `int8`〜`uint64`、`float32` `float64` | 按类型设模块的语言为 0。Rust・Kotlin・Swift 采用类型方法，Go 按用途设包 | R-L1-20 |
| 字符串 | `string` `regex` | 9/9 | 保持 |
| 集合 | `list` `map` `set` | 9/9 | 保持 |
| Option・Result | `option` `result` `error` | Rust・Gleam・OCaml・Swift・Kotlin 提供 | `option` 与 `result` 保持。`error` 的 3 个函数重复或转发，故移除，模块变空（[L2-error](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-error.md)） |
| 字节序列 | `bytes` | 9/9 | 内容在 L2，裸指针见 R-L1-26 |
| 编码 | `base64` `hex` `url` `html` | 5 种语言合并 base64 与 hex | R-L1-23 |
| 数据格式 | `json` `value` | JSON 为 8/9；标准提供与格式无关动态值的只有 Gleam dynamic | R-L1-27 |
| 哈希／压缩 | `hash` `zlib` | 非密码学哈希为 6+2，Go・Python・Erlang・JVM 将 CRC32 放在哈希一侧 | R-L1-05 |
| 时间 | `datetime` `duration` `compute` | 9 种语言中无 compute 对应物 | R-L1-21 |
| OS／进程 | `env` `process` `args` `fs` `path` `io` | 多数语言将 argv 与环境变量放在同一模块 | R-L1-25 |
| 网络 | `net` `http` | TCP 为 9/9，UDP・DNS・IP 地址类型为 7 | R-L1-06、R-L1-26 |
| 随机数 | `random` | 带种子随机数 6，密码学随机数（CSPRNG）6 | R-L1-04 |
| 矩阵 | `matrix` | 标准提供的语言为 0 | R-L1-22 |
| 测试 | `testing` | 5+1 | 保持 |

---

<a id="2-足すカテゴリ"></a>

## 2. 新增类别

<a id="r-l1-01-両端キュー-deque---採用"></a>

### R-L1-01 双端队列 `deque` — ○ 采纳

- (a) 7 种语言标准提供（如 Rust `VecDeque`、Python `collections.deque`、Kotlin `ArrayDeque`、OCaml `Queue`）。当前 Almide 从前端取出时，`list.drop(xs, 1)` 每次消耗 O(n)。
- (b) 纯计算，不需 host 功能。
- (c) 广度优先搜索等算法题中，LLM 按其他语言 deque 的前提编写。没有则每次自写队列，增加笔误。
- (d) 新增（新模块）。
- 函数（遵循 L0 动词）：`deque.new`、`from_list`、`to_list`、`len`、`is_empty`、`push`、`push_front`、`pop`、`pop_front`、`first`、`last`。`push_*`・`pop_*` 与 `list.push`・`list.pop` 一样接受 `mut` 参数并原地修改（R-L0-04 的 3：同一动词在全部模块采用同一方式）。详见 [L2-deque](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-deque.md)。
- 门禁：采用与 list 相同的边界输入矩阵测试，放置验证 native 与 wasm 结果一致的 fixture。
- issue: #3932

<a id="r-l1-02-優先度キュー-heap---採用"></a>

### R-L1-02 优先队列 `heap` — ○ 采纳

- (a) 6 种语言提供（如 Rust `BinaryHeap`、Go `container/heap`、Python `heapq`、OCaml `Pqueue`）。
- (b) 纯计算。
- (c) Dijkstra 算法、前 k 项等常见问题的标准工具。
- (d) 新增。默认最小堆（同 Python・Go・OCaml）。指定键的版本为 `heap.new_by(f)`（R-L0-10）。
- issue: #3933

<a id="r-l1-03-任意精度整数-bigint---採用"></a>

### R-L1-03 任意精度整数 `bigint` — ○ 采纳

- (a) 6 种语言提供（Go `math/big`、Python `int`、JS `BigInt`、Elixir 整数、JVM `BigInteger`）。
- (b) 纯计算。将 native 与 wasm 结果连 1 字节都不差纳入 ADR-0016（数值确定性）的承诺。
- (c) 阶乘、大斐波那契数等问题中，当前 `Int` 回绕，静默产生错误答案。
- (d) 新增。运算符采用 ADR-0005 的 desugar（`+` 到 `bigint.add`）。
- issue: #3931

<a id="r-l1-04-random-にシード付き乱数と暗号用の乱数を足す---採用"></a>

### R-L1-04 为 `random` 添加带种子的随机数和密码学随机数 — ○ 采纳

- (a) 带种子随机数有 6 种语言提供，CSPRNG（密码学随机数）也为 6 种。当前 `random` 有 4 个函数，不能传入种子。
- (b) 带种子随机数是纯计算（用 Almide 编写 PCG 等算法，native 与 wasm 产生同一序列）。密码学随机数通过 `wasi:random` 的 `get-random-bytes` 运行于 wasm。
- (c) 测试需要可复现随机数时，LLM 会写 `Random(seed)` 形态的 API。
- (d) 新增。添加 `random.seeded(seed) -> Rng`、`random.next_int(rng, lo, hi) -> (Int, Rng)` 等复制版及 `random.bytes(n)`（effect）。
- issue: #3939

<a id="r-l1-05-hash-を鍵を使わない要約の置き場にするcrc32adler-32-を-zlib-から移しmd5sha-1sha-512-を足す---採用"></a>

### R-L1-05 让 `hash` 容纳“不使用密钥的摘要”：从 zlib 移入 CRC32・Adler-32，添加 MD5・SHA-1・SHA-512 — ○ 采纳

- (a) 7 种语言提供 MD5・SHA-1・SHA-512・SHA-3。Go（`hash/crc32`）、Python（`zlib.crc32`）、Erlang、JVM 提供 CRC32，但位置不统一。Almide 将返回摘要的函数集中于 `hash`（R-L0-02：结果种类相同则归同一模块）。
- (b) 纯计算。
- (c) “CRC32 在 zlib”除 Python 外猜不准；放在 `hash` 可凭名称寻找。
- (d) 新增 `hash.crc32` 与 `hash.adler32`，移除 `zlib.crc32` 与 `zlib.adler32`（采用 R-L0-14 窗口，详见 [L2-hash](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-hash.md) R-hash-02 和 [L2-zlib](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-zlib.md) R-zlib-01）。新增 `hash.md5`、`sha1`、`sha512` 及各自 `_hex` 版。使用密钥的内容（HMAC、AEAD、签名）放入 #2596 的 `crypto`。
- issue: #3914

<a id="r-l1-06-net-に-udpdnsip-アドレスを足す---採用"></a>

### R-L1-06 为 `net` 添加 UDP・DNS・IP 地址 — ○ 采纳

- (a) 7 种语言提供。
- (b) 通过 `wasi:sockets` 的 `udp` 与 `ip-name-lookup` 运行于 wasm。与 TCP 一样采用 ADR-0023 路径（p3 shim）。不新增 host op。
- (d) 新增。
- issue: #3930

<a id="r-l1-07-uuid---採用"></a>

### R-L1-07 `uuid` — ○ 采纳

- (a) 5 种语言提供（Go 1.27 的 `uuid`、Python、JS `crypto.randomUUID`、Kotlin、Swift）。
- (b) v4 使用随机数（`wasi:random`），v7 使用时钟与随机数（`wasi:clocks` 和 `wasi:random`），两者均为 effect。`uuid.parse` 与 `uuid.to_string` 是纯计算。
- (d) 新增（`uuid.v4()`、`uuid.v7()`、`uuid.parse`、`uuid.to_string`）。
- issue: #3934

<a id="r-l1-08-utf-16latin-1-との変換---採用新しいモジュールにはしない"></a>

### R-L1-08 与 UTF-16・Latin-1 互转 — ○ 采纳（不新增模块）

- (a) 8 种语言提供。
- (b) 纯计算。
- (d) 将 `string.to_utf16`、`string.from_utf16` 等加入 string（L2-string 判定），不新增模块。遵循 R-L0-07，从容器构建属于目标类型的模块。
- issue：在 L2-string 提案中立项。

---
- issue: #3948

<a id="r-l1-30-bool-モジュールを置きboolparse-を足す---採用"></a>

### R-L1-30 设置 `bool` 模块并添加 `bool.parse` — ○ 采纳

- 现状：有 `int.parse`・`float.parse`・各定宽类型的 `parse`，却没有从字符串读取 `Bool` 的函数。用 `s == "true"` 读取配置时，`"ture"` 之类笔误会静默变为 `false`。
- 提案：设置 `bool` 模块（与 `int`・`float` 一样无须 import），新增 `fn bool.parse(s: String) -> Result[Bool, String]`。仅接收 `"true"` 和 `"false"`（前后空白、大写均 err）。内容见 [L2-bool](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-bool.md)。
- 依据：L1 新增标准（至少 5 种语言）、R-L0-25（从字符串转换采用目标类型的 `parse`）。
- (a) Rust `"true".parse::<bool>()`、Go `strconv.ParseBool`、OCaml `bool_of_string_opt`、Kotlin `toBooleanStrictOrNull`、Swift `Bool("true")`，都拒绝无效输入。
- issue: #3980

<a id="r-l1-31-秘密の値を包む-secret---採用"></a>

### R-L1-31 包装秘密值的 `secret` — ○ 采纳

- 现状：包含密码、API 密钥的记录在插值、`assert_eq` 失败显示、`almide test` 报告中原样输出全部字段（#4107）。字段上的 `@redact` 等属性不被读取，静默地不提供任何保护。
- 提案：设置 `secret` 模块（不自动 import，需 `import secret`），新增类型 `Secret[A]` 与 `secret.new`・`secret.reveal`。所有显示路径输出 `Secret(***)`；`==` 仅对 String / Bytes 进行常数时间比较，Codec 仅派生 decode。内容见 [L2-secret](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-secret.md)。
- 依据：仅 2/8 具有专用类型（Rust secrecy・pydantic），未达 5 种语言标准。仍采纳，是因为只有类型能够从 1 处声明推导所有显示路径（字段属性只能保护 1 条记录的 1 条路径）。所有者裁定 2026-10-24。
- issue: #4107

<a id="3-足さないカテゴリ-と理由"></a>

## 3. 不新增的类别（× 及理由）

| 类别 | 提供语言数 | 判定 | 理由 |
|---|---:|---|---|
| 惰性迭代器 | 8 | × | `|>` 的链通过 fusion（融合）变为 1 次循环（裁定“隐式 `|>` 即 fusion”）。另添惰性类型会使同一处理有 2 种写法（R-L0-01）。 |
| 字符串构建器 | 8 | × | `string.push(mut s, x)` 已是原地追加函数。 |
| printf 类格式化 | 8 | × | 字符串插值 `"${x}"` 与 `float.to_fixed`、`string.pad_start` 可以表达。再加 1 个格式化小语言会成为 LLM 笔误来源。 |
| 锁／原子操作／线程／通道／async／取消 | 5〜8 | × | 既定方针把并发限定为 `fan` 的确定性数据并行（concurrency-stance）。取消和截止由 `fan.timeout` 与 `fan.min(duration)`（#3788）承担。 |
| 二进制序列化 | 7 | × | `bytes` 的读写函数族（`append_<型>`、`read_<型>`）已存在。 |
| 时区（IANA） | 6 | × | 排除在标准库外，通过 package 提供。tz 数据库达数百 KB，wasm 需从 host 读取或嵌入。标准 `datetime` 保持确定性，仅处理 UTC 与固定偏移。推荐附带 tz 数据的 package（如 `almide/tz`）作为归属。 |
| 已排序映射 | 5 | × | `map` 按插入顺序确定。需要排序时用 `map.entries(m) |> list.sort_by(...)`。添加第 2 个 map 类型会增加使用哪一个的选择。 |
| XML | 6 | × | 作为 package，标准数据格式仅 JSON。 |
| 密码学（AES、签名、TLS） | 6 | ○（已有） | 在 #2596 中作为纯 Almide 的 `crypto` 推进（AEAD、Ed25519、HMAC）。TLS 在 `http` 内。 |
| 日志 | 5 | × | `eprintln` 可写标准错误。按重要程度分流属于应用职责。 |
| 终端／ANSI | 5 | × | 转义序列可用字符串编写，作为 package。 |
| tar・zip、bzip2・zstd | 5 | × | 作为 package，标准仅有 `zlib`（deflate・gzip）。 |
| 完整标志解析 | 5 | × | `args` 保持最小读取器。帮助文本生成等属于应用职责。 |
| 计时器 | 5 | × | 可用 `fan.timeout` 和 `env.sleep_ms` 表达。 |

---

<a id="4-分け方置き場所"></a>

## 4. 划分及归属

<a id="r-l1-20-固定幅の数のモジュールint8uint64float32float64---統合しない"></a>

### R-L1-20 定宽数值模块（`int8`～`uint64`、`float32`、`float64`）— × 不合并

- (a) 按类型设模块的语言为 0，但多数在类型上附加方法。Almide 没有方法（只有 UFCS），因此应用 R-L0-02（放在主体类型模块）便得到每种类型各一模块。
- (c) 在 `x |> int8.to_int16` 形式中，由管道起始类型决定模块。
- 内部不一致（`int.to_u32` 与 `int.to_uint32`、sized int 到 Int 缺少 `to_int`）在 L2 判定（R-L0-07）。

<a id="r-l1-21-compute-と-duration---統合しないadr-0001"></a>

### R-L1-21 `compute` 与 `duration` — × 不合并（ADR-0001）

ADR-0001 将确定性计算时间（`Compute`）和真实时间（`Duration`）定为相互不可转换的不同类型。9 种语言中没有与 `compute` 对应的内容。即便如此，Almide 有承诺确定性的独有理由，故不随多数。

<a id="r-l1-22-matrix-を標準に残し3852-の切り出しを終える---採用"></a>

### R-L1-22 将 `matrix` 留在标准库并完成 #3852 拆分 — ○ 采纳

- (a) 标准提供矩阵的语言为 0。
- 保留理由：编译器重写规则（`@rewrite`、通过 egg fusion）以 `matrix` 运算为对象（`crates/almide-codegen/src/target.rs`）。目前 package 不能带入重写规则。
- 在此前提下继续 #3852，将推理 kernel 和 fusion 对象函数从表中移除，仅保留通用稠密矩阵运算。
- issue: #3852（已有）

<a id="r-l1-23-hex-と-base64-を別のモジュールのまま残す---まとめない"></a>

### R-L1-23 `hex` 与 `base64` 保持分离模块 — × 不合并

5 种语言集中在一处，但合并模块名称不统一（Elixir `Base`、Deno `@std/encoding`、Python `base64` 内的 `b16encode`）。Go 分为 `encoding/hex` 和 `encoding/base64`。依据 R-L0-00，无多数拼写则不移动。

<a id="r-l1-25-os-とプロセスまわりの持ち分を-1-つに決める---採用"></a>

### R-L1-25 为 OS 和进程相关内容确定唯一归属 — ○ 采纳

| 职责 | 模块 | 移除的重复 |
|---|---|---|
| argv、环境变量、工作目录、OS 名称 | `env` | `args.raw`（= `env.args`）、`process.args`（→ `env.args`）、`process.env`（纯 fn 读取环境的漏洞） |
| 时钟（真实时间／单调时钟） | `datetime` | `env.unix_timestamp`（= `datetime.now`）、将 `env.millis` 移至 `datetime.now_ms` |
| 等待 | `env.sleep_ms` | `process.sleep`（仅负值处理不同） |
| 临时目录／文件 | `fs` | `env.temp_dir`（= `fs.temp_dir`） |
| 标准输入 | `io` | `process.stdin_lines`（→ `string.lines(io.read_all()!)`） |
| 子进程、自身退出、pid | `process` | — |
| 读取标志 | `args`（建立在 `env.args` 之上） | — |

- (a) argv 与环境变量放在同一模块的有 Rust `std::env`、Go `os`、Kotlin `System` 等。
- (c) 若 2 个函数返回同一值，边界行为却不同，LLM 无论选哪一个都有一半猜错。
- (d) 移除，遵循 R-L0-14。
- issue: #3851 B（已有）。新增 `env.millis` 移动事项。

<a id="r-l1-26-生ポインタを-ffi-にunix-ソケットと共有メモリを-unix-に移す---採用"></a>

### R-L1-26 将裸指针移至 `ffi`，Unix socket 和共享内存移至 `unix` — ○ 采纳

遵循 #3853 提案。`bytes` 在所有文件中无须 import 即可使用，因此不放内存不安全函数。`net` 只容纳可通过 wasm `wasi:sockets` 运行的内容。`unix.*` 保持已裁定的暂定 host op（104～116）原样移动。
- issue: #3853（已有）

<a id="r-l1-27-json-はテキストとの変換value-は動的な値の木---採用"></a>

### R-L1-27 `json` 负责文本转换，`value` 负责动态值树 — ○ 采纳

确定职责：与文本互转（`parse`、`stringify`）放在 `json`；树的构造和提取放在 `value`。`value.stringify` 与 `json.stringify` 重复，故移除（R-L0-01）。细节在 L2 判定。
- issue: #3953

<a id="r-l1-28-url-の符号化は-url-に置く---採用"></a>

### R-L1-28 URL 编码放在 `url` — ○ 采纳

`http.url_decode`（将 `+` 还原为空格的表单解码）属于 URL 编码角色，因此移至 `url` 中新设的 `url.decode_form` 并移除旧函数（R-L0-01、R-L0-02）。改写为 `url.decode_form(s) ?? s`。细节在 [L2-url](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-url.md) R-url-01 和 [L2-http](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-http.md) R-http-04 判定。
- issue: #3950

<a id="r-l1-29-html-と-testing---動かさない"></a>

### R-L1-29 `html` 与 `testing` — × 不移动

`html` 有 5 个函数。“将已转义 HTML 设为独立类型”（SafeHtml）的设计与 Go `html/template` 思路相同，通过类型防止混用。`testing` 为 5 种语言标准提供。

---

<a id="5-予定されているもの参考"></a>

## 5. 计划中的内容（参考）

- `kv`（键值存储，#2699）正在按 ADR-0020 流程推进，此处不处理。
- `crypto`（#2596）与 R-L1-05 配对。不使用密钥的摘要归 `hash`，使用密钥的归 `crypto`。
