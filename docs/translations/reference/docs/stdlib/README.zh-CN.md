<a id="standard-library"></a>

# 标准库

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](README.zh-CN.md)



> 本文是[固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/README.md)的非规范读者索引，不是原生成器的输出，不自动生成或同步。保留固定版本的签名、数量和顺序，并按签名匹配各模块读者译文中的 API 说明，因此可能比原索引的首句摘要更详细。原文仍为权威版本。本次翻译没有独立验证示例、实现或安全性；请务必同时阅读各模块有关原文不一致、目标差异和安全上下文的注记。


列出 51 个 stdlib 模块的全部 1324 个公开函数及其签名和说明。每个模块页面提供完整说明、示例及 `@since` 版本。

`mut` 参数被原地修改：调用方传入 `var` 绑定，并能看到修改（`let`、非 `mut` 参数或临时值触发 E032）。其他参数均为只读；不使用 `mut` 修改值的函数返回新值。

| 模块 | 函数 | 类型 |
|---|---:|---:|
| [args](#args) | 3 | 0 |
| [base64](#base64) | 4 | 0 |
| [bigint](#bigint) | 24 | 1 |
| [bool](#bool) | 3 | 0 |
| [bytes](#bytes) | 178 | 0 |
| [compute](#compute) | 7 | 0 |
| [datetime](#datetime) | 18 | 0 |
| [deque](#deque) | 60 | 0 |
| [duration](#duration) | 7 | 0 |
| [env](#env) | 9 | 0 |
| [ffi](#ffi) | 8 | 0 |
| [float](#float) | 27 | 0 |
| [float32](#float32) | 24 | 0 |
| [float64](#float64) | 24 | 0 |
| [fs](#fs) | 37 | 1 |
| [hash](#hash) | 11 | 0 |
| [heap](#heap) | 16 | 0 |
| [hex](#hex) | 3 | 0 |
| [html](#html) | 8 | 0 |
| [http](#http) | 36 | 4 |
| [int](#int) | 47 | 0 |
| [int16](#int16) | 37 | 0 |
| [int32](#int32) | 37 | 0 |
| [int64](#int64) | 37 | 0 |
| [int8](#int8) | 37 | 0 |
| [io](#io) | 12 | 0 |
| [json](#json) | 3 | 0 |
| [list](#list) | 74 | 0 |
| [map](#map) | 37 | 0 |
| [math](#math) | 27 | 0 |
| [matrix](#matrix) | 27 | 0 |
| [net](#net) | 16 | 2 |
| [option](#option) | 14 | 0 |
| [path](#path) | 11 | 0 |
| [process](#process) | 13 | 2 |
| [random](#random) | 10 | 0 |
| [regex](#regex) | 16 | 1 |
| [result](#result) | 15 | 0 |
| [secret](#secret) | 2 | 0 |
| [set](#set) | 42 | 0 |
| [string](#string) | 87 | 0 |
| [testing](#testing) | 10 | 0 |
| [uint16](#uint16) | 35 | 0 |
| [uint32](#uint32) | 35 | 0 |
| [uint64](#uint64) | 35 | 0 |
| [uint8](#uint8) | 35 | 0 |
| [unix](#unix) | 10 | 0 |
| [url](#url) | 9 | 1 |
| [uuid](#uuid) | 7 | 0 |
| [value](#value) | 29 | 0 |
| [zlib](#zlib) | 11 | 0 |

## args

[args.md](args.zh-CN.md)

- `effect args.has_flag(name: String) -> Bool` — `--` 前出现 --name 则为 true；单字母名称也匹配 -name。不推导短形式。
- `effect args.option(name: String) -> Option[String]` — `--` 前 --name=v 或 --name v 的值；不存在则为 none。
- `effect args.positional(options: List[String]) -> List[String]` — `--` 前既非标志也非指定选项值的参数，随后为 `--` 后的所有参数。

## base64

[base64.md](base64.zh-CN.md)

- `base64.encode(b: Bytes) -> String` — 标准 base64，使用 + / 和 = 填充。
- `base64.decode(s: String) -> Result[Bytes, String]` — 从带填充的规范 base64（+ /）得到字节；否则 err。
- `base64.encode_url(b: Bytes) -> String` — URL-safe base64（- _），仍保留 = 填充。
- `base64.decode_url(s: String) -> Result[Bytes, String]` — 从带填充的规范 URL-safe base64（- _）得到字节；否则 err。

## bigint

[bigint.md](bigint.zh-CN.md)

- `bigint.parse(s: String) -> Result[BigInt, String]` — 从去掉首尾空白的十进制 s 得到 BigInt，可带 + 或 -；非法内容或超过 4300 位为 err。
- `bigint.parse_radix(s: String, radix: Int) -> Result[BigInt, String]` — 从去掉首尾空白的 radix 进制 s 得到 BigInt；可带 +/-，大小写均可，16/8/2 允许 0x/0o/0b；非法内容，以及非二的幂基数超过 4300 位时为 err；radix 超出 2..36 则 abort。
- `bigint.to_string(x: BigInt) -> String` — 十进制数字，负数带 -；超过 4300 位则 abort。
- `bigint.to_string_radix(x: BigInt, radix: Int) -> String` — x 的 radix 进制小写数字，负数带 -；radix 超出 2..36 则 abort，非二的幂基数文本会超过 4300 位时也 abort。
- `bigint.compare(a: BigInt, b: BigInt) -> Int` — a 小于、等于、大于 b 时分别为 -1、0、1。
- `bigint.min(a: BigInt, b: BigInt) -> BigInt` — a 与 b 中较小者。
- `bigint.max(a: BigInt, b: BigInt) -> BigInt` — a 与 b 中较大者。
- `bigint.clamp(x: BigInt, lo: BigInt, hi: BigInt) -> BigInt` — x 限制在 lo..hi；lo > hi 则 abort。
- `bigint.abs(x: BigInt) -> BigInt` — x 的大小（绝对值）。
- `bigint.sign(x: BigInt) -> Int` — 根据 x 符号返回 -1、0、1；0 返回 0。
- `bigint.is_even(x: BigInt) -> Bool` — x 是 2 的倍数时为 true。
- `bigint.is_odd(x: BigInt) -> Bool` — x 不是 2 的倍数时为 true。
- `bigint.floor_div(a: BigInt, b: BigInt) -> BigInt` — 向 -inf 舍入的商；b == 0 则 abort。
- `bigint.modulo(a: BigInt, b: BigInt) -> BigInt` — 与 b 同号的余数，使 a == floor_div(a, b) * b + modulo(a, b)；b == 0 则 abort。
- `bigint.pow(base: BigInt, exp: Int) -> BigInt` — base 的 exp 次方；exp < 0 或结果必定超过 30 * 2^28 位时 abort。
- `bigint.bit_length(x: BigInt) -> Int` — 表示 x 绝对值所需位数；0 返回 0。
- `bigint.band(a: BigInt, b: BigInt) -> BigInt` — 按位 AND。
- `bigint.bor(a: BigInt, b: BigInt) -> BigInt` — 按位 OR。
- `bigint.bxor(a: BigInt, b: BigInt) -> BigInt` — 按位 XOR。
- `bigint.bnot(x: BigInt) -> BigInt` — 按位取反，等于 -x - 1。
- `bigint.bshl(a: BigInt, n: Int) -> BigInt` — a 左移 n 位（a * 2^n）；n < 0 则 abort。
- `bigint.bshr(a: BigInt, n: Int) -> BigInt` — a 右移 n 位，向 -inf 舍入（符号填充）；n < 0 则 abort。
- `bigint.from(x: S) -> BigInt` — x 向零转换为 BigInt；NaN 或无穷大则 abort。
- `bigint.from_checked(x: S) -> Option[BigInt]` — x 是整数时为 some(x as BigInt)，否则（小数、NaN、无穷大）为 none。

## bool

[bool.md](bool.zh-CN.md)

- `bool.to_string(b: Bool) -> String` — "true" 或 "false"；bool.parse 可读回。
- `bool.parse(s: String) -> Result[Bool, String]` — "true" 得到 ok(true)，"false" 得到 ok(false)，其他内容得到 err。
- `bool.compare(a: Bool, b: Bool) -> Int` — a < b 时为负，相等时为0，a > b 时为正；false < true。

## bytes

[bytes.md](bytes.zh-CN.md)

- `bytes.append_f32_be(mut b: Bytes, x: Float) -> Unit` — 将 x 舍入为 f32，以 BE 追加。
- `bytes.append_f32_le(mut b: Bytes, x: Float) -> Unit` — 将 x 舍入为 f32，以 LE 追加。
- `bytes.append_f64_be(mut b: Bytes, x: Float) -> Unit` — 将 x 作为 8-byte f64，以 BE 追加。
- `bytes.append_f64_le(mut b: Bytes, x: Float) -> Unit` — 将 x 作为 8-byte f64，以 LE 追加。
- `bytes.append_i16_be(mut b: Bytes, x: Int) -> Unit` — 以 BE 追加 x 的低16位。
- `bytes.append_i16_le(mut b: Bytes, x: Int) -> Unit` — 以 LE 追加 x 的低16位。
- `bytes.append_i32_be(mut b: Bytes, x: Int) -> Unit` — 以 BE 追加 x 的低32位。
- `bytes.append_i32_le(mut b: Bytes, x: Int) -> Unit` — 以 LE 追加 x 的低32位。
- `bytes.append_i64_be(mut b: Bytes, x: Int) -> Unit` — 将 x 作为 8 bytes，以 BE 追加。
- `bytes.append_i64_le(mut b: Bytes, x: Int) -> Unit` — 将 x 作为 8 bytes，以 LE 追加。
- `bytes.append_u16_be(mut b: Bytes, x: Int) -> Unit` — 以 BE 追加 x 的低16位。
- `bytes.append_u16_le(mut b: Bytes, x: Int) -> Unit` — 以 LE 追加 x 的低16位。
- `bytes.append_u32_be(mut b: Bytes, x: Int) -> Unit` — 以 BE 追加 x 的低32位。
- `bytes.append_u32_le(mut b: Bytes, x: Int) -> Unit` — 以 LE 追加 x 的低32位。
- `bytes.append_u8(mut b: Bytes, x: Int) -> Unit` — 追加 x 的低8位；256变为0。
- `bytes.clear(mut b: Bytes) -> Unit` — 将 b 原地截断到长度0。
- `bytes.compare(a: Bytes, b: Bytes) -> Int` — 按字典序返回 -1、0或1；前缀排在前面。
- `bytes.contains(b: Bytes, needle: Bytes) -> Bool` — needle 出现在 b 中或 needle 为空时为 true。
- `bytes.copy_from(mut b: Bytes, src: Bytes, dst_off: Int, src_off: Int, len: Int) -> Unit` — 将 src 的 len 字节复制到 b，限幅以适应空间。
- `bytes.ends_with(b: Bytes, suffix: Bytes) -> Bool` — b 以 suffix 结尾或 suffix 为空时为 true。
- `bytes.is_eof(b: Bytes, pos: Int) -> Bool` — pos >= len 或 pos 为负时为 true。
- `bytes.fill(mut b: Bytes, x: Int) -> Unit` — 原地将每个字节设为 x 的低8位。
- `bytes.from_list(xs: List[Int]) -> Bytes` — 将 xs 转为字节，每项取低8位；-1变为255。
- `bytes.from_string(s: String) -> Bytes` — s 的 UTF-8 编码。
- `bytes.get(b: Bytes, i: Int) -> Option[Int]` — 索引 i 处的字节，越界为 none。
- `bytes.index_of(b: Bytes, needle: Bytes) -> Option[Int]` — needle 的首次偏移，或 none；为空时为 some(0)。
- `bytes.insert(b: Bytes, i: Int, x: Int) -> Bytes` — 在限制至0..len 的 i 处插入 x 低字节的副本。
- `bytes.is_empty(b: Bytes) -> Bool` — b 不含字节时为 true。
- `bytes.is_valid_utf8(b: Bytes) -> Bool` — b 是否为有效 UTF-8；为空时为 true。
- `bytes.len(b: Bytes) -> Int` — 字节数；空缓冲区为0。
- `bytes.map(b: Bytes, f: (Int) -> Int) -> Bytes` — 逐字节应用 f，结果截断为低8位。
- `bytes.zeros(n: Int) -> Bytes` — n 个零字节；n <= 0 时为空。
- `bytes.pad_start(b: Bytes, n: Int, fill: Int) -> Bytes` — 左侧用 fill 填充到长度 n；已更长则保持原样。
- `bytes.pad_end(b: Bytes, n: Int, fill: Int) -> Bytes` — 右侧用 fill 填充到长度 n；已更长则保持原样。
- `bytes.push(mut b: Bytes, x: Int) -> Unit` — 原地追加 x 的低8位。
- `bytes.copy_within(mut b: Bytes, src_start: Int, src_end: Int, dst: Int) -> Unit` — 将 src_start..src_end 复制到 dst；容纳不下则不操作。
- `bytes.remove_at(b: Bytes, i: Int) -> Bytes` — 去掉 pos 处字节的副本；越界则保持原样。
- `bytes.repeat(b: Bytes, n: Int) -> Bytes` — 将 b 拼接 n 次；n <= 0 时为空。
- `bytes.reverse(b: Bytes) -> Bytes` — 字节顺序反转的副本。
- `bytes.set(b: Bytes, i: Int, x: Int) -> Bytes` — 将字节 i 设为 x 的副本；越界则保持原样。
- `bytes.skip(b: Bytes, pos: Int, n: Int) -> Int` — pos + n，上限为 len。
- `bytes.slice(b: Bytes, start: Int, end: Int) -> Bytes` — start..end 的副本，各边界限制在0..len；start >= end 时为空。
- `bytes.split(b: Bytes, sep: Bytes) -> List[Bytes]` — sep 匹配之间的片段；sep 为空时为 [b]。
- `bytes.starts_with(b: Bytes, prefix: Bytes) -> Bool` — b 以 prefix 开始或 prefix 为空时为 true。
- `bytes.is_equal_constant_time(a: Bytes, b: Bytes) -> Bool` — a 与 b 包含相同字节时为 true，耗时仅依赖长度，不会在首个不同字节处提前退出。用于比较 MAC、令牌或密码哈希；`==` 会在首个差异处返回。长度不同时立即为 false（长度是公开的）。
- `bytes.to_list(b: Bytes) -> List[Int]` — 每个字节作为0..255的 Int。
- `bytes.to_string(b: Bytes) -> Result[String, String]` — UTF-8 解码；无效时返回 err "invalid UTF-8: ..."。
- `bytes.to_string_lossy(b: Bytes) -> String` — UTF-8 解码；无效序列变为 U+FFFD。
- `bytes.append_bool(mut b: Bytes, x: Bool) -> Unit` — true 追加1，false 追加0。
- `bytes.xor(a: Bytes, b: Bytes) -> Bytes` — 逐字节 XOR，长度取较短输入。
- `bytes.get_u8(b: Bytes, pos: Int) -> Option[Int]` — 以0..255返回 pos 处字节；超出0..len 为 none，与 get 的值相同。
- `bytes.next_u8(b: Bytes, pos: Int) -> (Option[Int], Int)` — (get_u8(b, pos), pos + 1)；未读取任何内容时位置保持 pos。
- `bytes.put_u8(mut b: Bytes, pos: Int, x: Int) -> Unit` — 在 pos 原地写入 x 的低8位；pos 超出0..len 时中止。
- `bytes.get_i8(b: Bytes, pos: Int) -> Option[Int]` — 将 pos 处字节作为有符号 -128..127 返回；超出0..len 为 none。
- `bytes.next_i8(b: Bytes, pos: Int) -> (Option[Int], Int)` — (get_i8(b, pos), pos + 1)；未读取任何内容时位置保持 pos。
- `bytes.put_i8(mut b: Bytes, pos: Int, x: Int) -> Unit` — 在 pos 原地写入 x 的低8位；pos 超出0..len 时中止。
- `bytes.append_i8(mut b: Bytes, x: Int) -> Unit` — 原地追加 x 的低8位；-1变为255。
- `bytes.get_bool(b: Bytes, pos: Int) -> Option[Bool]` — pos 处字节 != 0；超出0..len 为 none。
- `bytes.next_bool(b: Bytes, pos: Int) -> (Option[Bool], Int)` — (get_bool(b, pos), pos + 1)；未读取任何内容时位置保持 pos。
- `bytes.put_bool(mut b: Bytes, pos: Int, x: Bool) -> Unit` — 在 pos 原地写入 true 对应的1或 false 对应的0；pos 超出0..len 时中止。
- `bytes.get_u16_le(b: Bytes, pos: Int) -> Option[Int]` — pos 处的 u16 LE；容纳不下2字节时为 none。
- `bytes.next_u16_le(b: Bytes, pos: Int) -> (Option[Int], Int)` — (get_u16_le(b, pos), pos + 2)；未读取任何内容时位置保持 pos。
- `bytes.get_u16_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]` — 从 pos 起 count 个 u16 LE；无法全部容纳或 count < 0 时为 none。
- `bytes.put_u16_le(mut b: Bytes, pos: Int, x: Int) -> Unit` — 将 x 作为 u16 LE 在 pos 原地写入；写入窗口超出0..len 时中止。
- `bytes.get_u16_be(b: Bytes, pos: Int) -> Option[Int]` — pos 处的 u16 BE；容纳不下2字节时为 none。
- `bytes.next_u16_be(b: Bytes, pos: Int) -> (Option[Int], Int)` — (get_u16_be(b, pos), pos + 2)；未读取任何内容时位置保持 pos。
- `bytes.get_u16_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]` — 从 pos 起 count 个 u16 BE；无法全部容纳或 count < 0 时为 none。
- `bytes.put_u16_be(mut b: Bytes, pos: Int, x: Int) -> Unit` — 将 x 作为 u16 BE 在 pos 原地写入；写入窗口超出0..len 时中止。
- `bytes.get_i16_le(b: Bytes, pos: Int) -> Option[Int]` — pos 处的 i16 LE；容纳不下2字节时为 none。
- `bytes.next_i16_le(b: Bytes, pos: Int) -> (Option[Int], Int)` — (get_i16_le(b, pos), pos + 2)；未读取任何内容时位置保持 pos。
- `bytes.get_i16_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]` — 从 pos 起 count 个 i16 LE；无法全部容纳或 count < 0 时为 none。
- `bytes.put_i16_le(mut b: Bytes, pos: Int, x: Int) -> Unit` — 将 x 作为 i16 LE 在 pos 原地写入；写入窗口超出0..len 时中止。
- `bytes.get_i16_be(b: Bytes, pos: Int) -> Option[Int]` — pos 处的 i16 BE；容纳不下2字节时为 none。
- `bytes.next_i16_be(b: Bytes, pos: Int) -> (Option[Int], Int)` — (get_i16_be(b, pos), pos + 2)；未读取任何内容时位置保持 pos。
- `bytes.get_i16_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]` — 从 pos 起 count 个 i16 BE；无法全部容纳或 count < 0 时为 none。
- `bytes.put_i16_be(mut b: Bytes, pos: Int, x: Int) -> Unit` — 将 x 作为 i16 BE 在 pos 原地写入；写入窗口超出0..len 时中止。
- `bytes.get_u32_le(b: Bytes, pos: Int) -> Option[Int]` — pos 处的 u32 LE；容纳不下4字节时为 none。
- `bytes.next_u32_le(b: Bytes, pos: Int) -> (Option[Int], Int)` — (get_u32_le(b, pos), pos + 4)；未读取任何内容时位置保持 pos。
- `bytes.get_u32_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]` — 从 pos 起 count 个 u32 LE；无法全部容纳或 count < 0 时为 none。
- `bytes.put_u32_le(mut b: Bytes, pos: Int, x: Int) -> Unit` — 将 x 作为 u32 LE 在 pos 原地写入；写入窗口超出0..len 时中止。
- `bytes.get_u32_be(b: Bytes, pos: Int) -> Option[Int]` — pos 处的 u32 BE；容纳不下4字节时为 none。
- `bytes.next_u32_be(b: Bytes, pos: Int) -> (Option[Int], Int)` — (get_u32_be(b, pos), pos + 4)；未读取任何内容时位置保持 pos。
- `bytes.get_u32_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]` — 从 pos 起 count 个 u32 BE；无法全部容纳或 count < 0 时为 none。
- `bytes.put_u32_be(mut b: Bytes, pos: Int, x: Int) -> Unit` — 将 x 作为 u32 BE 在 pos 原地写入；写入窗口超出0..len 时中止。
- `bytes.get_i32_le(b: Bytes, pos: Int) -> Option[Int]` — pos 处的 i32 LE；容纳不下4字节时为 none。
- `bytes.next_i32_le(b: Bytes, pos: Int) -> (Option[Int], Int)` — (get_i32_le(b, pos), pos + 4)；未读取任何内容时位置保持 pos。
- `bytes.get_i32_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]` — 从 pos 起 count 个 i32 LE；无法全部容纳或 count < 0 时为 none。
- `bytes.put_i32_le(mut b: Bytes, pos: Int, x: Int) -> Unit` — 将 x 作为 i32 LE 在 pos 原地写入；写入窗口超出0..len 时中止。
- `bytes.get_i32_be(b: Bytes, pos: Int) -> Option[Int]` — pos 处的 i32 BE；容纳不下4字节时为 none。
- `bytes.next_i32_be(b: Bytes, pos: Int) -> (Option[Int], Int)` — (get_i32_be(b, pos), pos + 4)；未读取任何内容时位置保持 pos。
- `bytes.get_i32_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]` — 从 pos 起 count 个 i32 BE；无法全部容纳或 count < 0 时为 none。
- `bytes.put_i32_be(mut b: Bytes, pos: Int, x: Int) -> Unit` — 将 x 作为 i32 BE 在 pos 原地写入；写入窗口超出0..len 时中止。
- `bytes.get_i64_le(b: Bytes, pos: Int) -> Option[Int]` — pos 处的 i64 LE；容纳不下8字节时为 none。
- `bytes.next_i64_le(b: Bytes, pos: Int) -> (Option[Int], Int)` — (get_i64_le(b, pos), pos + 8)；未读取任何内容时位置保持 pos。
- `bytes.get_i64_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]` — 从 pos 起 count 个 i64 LE；无法全部容纳或 count < 0 时为 none。
- `bytes.put_i64_le(mut b: Bytes, pos: Int, x: Int) -> Unit` — 将 x 作为 i64 LE 在 pos 原地写入；写入窗口超出0..len 时中止。
- `bytes.get_i64_be(b: Bytes, pos: Int) -> Option[Int]` — pos 处的 i64 BE；容纳不下8字节时为 none。
- `bytes.next_i64_be(b: Bytes, pos: Int) -> (Option[Int], Int)` — (get_i64_be(b, pos), pos + 8)；未读取任何内容时位置保持 pos。
- `bytes.get_i64_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]` — 从 pos 起 count 个 i64 BE；无法全部容纳或 count < 0 时为 none。
- `bytes.put_i64_be(mut b: Bytes, pos: Int, x: Int) -> Unit` — 将 x 作为 i64 BE 在 pos 原地写入；写入窗口超出0..len 时中止。
- `bytes.get_f32_le(b: Bytes, pos: Int) -> Option[Float]` — pos 处的 f32 LE；容纳不下4字节时为 none。
- `bytes.next_f32_le(b: Bytes, pos: Int) -> (Option[Float], Int)` — (get_f32_le(b, pos), pos + 4)；未读取任何内容时位置保持 pos。
- `bytes.get_f32_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Float]]` — 从 pos 起 count 个 f32 LE；无法全部容纳或 count < 0 时为 none。
- `bytes.put_f32_le(mut b: Bytes, pos: Int, x: Float) -> Unit` — 将 x 作为 f32 LE 在 pos 原地写入；写入窗口超出0..len 时中止。
- `bytes.get_f32_be(b: Bytes, pos: Int) -> Option[Float]` — pos 处的 f32 BE；容纳不下4字节时为 none。
- `bytes.next_f32_be(b: Bytes, pos: Int) -> (Option[Float], Int)` — (get_f32_be(b, pos), pos + 4)；未读取任何内容时位置保持 pos。
- `bytes.get_f32_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Float]]` — 从 pos 起 count 个 f32 BE；无法全部容纳或 count < 0 时为 none。
- `bytes.put_f32_be(mut b: Bytes, pos: Int, x: Float) -> Unit` — 将 x 作为 f32 BE 在 pos 原地写入；写入窗口超出0..len 时中止。
- `bytes.get_f64_le(b: Bytes, pos: Int) -> Option[Float]` — pos 处的 f64 LE；容纳不下8字节时为 none。
- `bytes.next_f64_le(b: Bytes, pos: Int) -> (Option[Float], Int)` — (get_f64_le(b, pos), pos + 8)；未读取任何内容时位置保持 pos。
- `bytes.get_f64_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Float]]` — 从 pos 起 count 个 f64 LE；无法全部容纳或 count < 0 时为 none。
- `bytes.put_f64_le(mut b: Bytes, pos: Int, x: Float) -> Unit` — 将 x 作为 f64 LE 在 pos 原地写入；写入窗口超出0..len 时中止。
- `bytes.get_f64_be(b: Bytes, pos: Int) -> Option[Float]` — pos 处的 f64 BE；容纳不下8字节时为 none。
- `bytes.next_f64_be(b: Bytes, pos: Int) -> (Option[Float], Int)` — (get_f64_be(b, pos), pos + 8)；未读取任何内容时位置保持 pos。
- `bytes.get_f64_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Float]]` — 从 pos 起 count 个 f64 BE；无法全部容纳或 count < 0 时为 none。
- `bytes.put_f64_be(mut b: Bytes, pos: Int, x: Float) -> Unit` — 将 x 作为 f64 BE 在 pos 原地写入；写入窗口超出0..len 时中止。
- `bytes.get_f16_le(b: Bytes, pos: Int) -> Option[Float]` — pos 处的 f16 LE，作为 Float 返回；容纳不下2字节时为 none。
- `bytes.next_f16_le(b: Bytes, pos: Int) -> (Option[Float], Int)` — (get_f16_le(b, pos), pos + 2)；未读取任何内容时位置保持 pos。
- `bytes.get_f16_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Float]]` — 从 pos 起 count 个 f16 LE；无法全部容纳或 count < 0 时为 none。
- `bytes.get_f16_be(b: Bytes, pos: Int) -> Option[Float]` — pos 处的 f16 BE，作为 Float 返回；容纳不下2字节时为 none。
- `bytes.next_f16_be(b: Bytes, pos: Int) -> (Option[Float], Int)` — (get_f16_be(b, pos), pos + 2)；未读取任何内容时位置保持 pos。
- `bytes.get_f16_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Float]]` — 从 pos 起 count 个 f16 BE；无法全部容纳或 count < 0 时为 none。
- `bytes.put_f16_le(mut b: Bytes, pos: Int, x: Float) -> Unit` — 将 x 舍入为 f16 LE 在 pos 原地写入；写入窗口超出0..len 时中止。
- `bytes.put_f16_be(mut b: Bytes, pos: Int, x: Float) -> Unit` — 将 x 舍入为 f16 BE 在 pos 原地写入；写入窗口超出0..len 时中止。
- `bytes.append_f16_le(mut b: Bytes, x: Float) -> Unit` — 追加舍入为 f16 的 x（最近偶数；溢出为 inf），LE。
- `bytes.append_f16_be(mut b: Bytes, x: Float) -> Unit` — 追加舍入为 f16 的 x（最近偶数；溢出为 inf），BE。
- `bytes.next_bytes(b: Bytes, pos: Int, n: Int) -> (Option[Bytes], Int)` — (pos 处的 n 字节, pos + n)；容纳不下或 n < 0 时为 (none, pos)。
- `bytes.append_bytes(mut b: Bytes, src: Bytes) -> Unit` — 将 src 的字节原地追加到 b。
- `bytes.new() -> Bytes` — 空字节缓冲区。
- `bytes.chunk(b: Bytes, n: Int) -> List[Bytes]` — 按顺序分成 n 字节块，最后一块可能较短；n <= 0 时中止。
- `bytes.pop(mut b: Bytes) -> Option[Int]` — 移除并返回末尾字节；b 为空则为 none。
- `bytes.take(b: Bytes, n: Int) -> Bytes` — 前 n 个字节；超过长度取整个 b，n < 0 时为空。
- `bytes.drop(b: Bytes, n: Int) -> Bytes` — 去掉 b 的前 n 个字节；超过长度则为空，n < 0 时为整个 b。
- `bytes.take_end(b: Bytes, n: Int) -> Bytes` — 最后 n 个字节；超过长度取整个 b，n < 0 时为空。
- `bytes.drop_end(b: Bytes, n: Int) -> Bytes` — 去掉 b 的最后 n 个字节；超过长度则为空，n < 0 时为整个 b。
- `bytes.split_at(b: Bytes, n: Int) -> (Bytes, Bytes)` — (前 n 字节, 其余)；n 超过末尾为 (b, 空)，n < 0 为 (空, b)。
- `bytes.first(b: Bytes) -> Option[Int]` — 首字节，b 为空则为 none。
- `bytes.last(b: Bytes) -> Option[Int]` — 末字节，b 为空则为 none。
- `bytes.filter(b: Bytes, f: (Int) -> Bool) -> Bytes` — f 为 true 的字节。
- `bytes.fold(b: Bytes, init: B, f: (B, Int) -> B) -> B` — 对字节从左到右折叠 f。
- `bytes.any(b: Bytes, f: (Int) -> Bool) -> Bool` — 某个字节满足 f 时为 true；b 为空为 false。
- `bytes.all(b: Bytes, f: (Int) -> Bool) -> Bool` — 每个字节都满足 f 时为 true；b 为空为 true。
- `bytes.count(b: Bytes, f: (Int) -> Bool) -> Int` — 满足 f 的字节数量。
- `bytes.find(b: Bytes, f: (Int) -> Bool) -> Option[Int]` — 满足 f 的首个字节，或 none。
- `bytes.find_index(b: Bytes, f: (Int) -> Bool) -> Option[Int]` — 满足 f 的首个字节的索引，或 none。
- `bytes.enumerate(b: Bytes) -> List[(Int, Int)]` — 从0开始的(索引, 字节)对。
- `bytes.zip(b: Bytes, ys: List[B]) -> List[(Int, B)]` — 将字节与 ys 配对，在较短者末尾停止。
- `bytes.zip_with(b: Bytes, ys: List[B], f: (Int, B) -> C) -> List[C]` — 对与 ys 配对的字节应用 f，在较短者末尾停止。
- `bytes.filter_map(b: Bytes, f: (Int) -> Option[B]) -> List[B]` — 按顺序收集对字节应用 f 所得的 some 结果。
- `bytes.find_map(b: Bytes, f: (Int) -> Option[B]) -> Option[B]` — 对字节应用 f 所得的首个 some 结果，或 none。
- `bytes.reduce(b: Bytes, f: (Int, Int) -> Int) -> Option[Int]` — 从首字节开始用 f 折叠；b 为空则为 none。
- `bytes.scan(b: Bytes, init: B, f: (B, Int) -> B) -> List[B]` — 字节的逐步折叠值，不含 init。
- `bytes.partition(b: Bytes, f: (Int) -> Bool) -> (List[Int], List[Int])` — (f 为 true 的字节, 其余)，各自保持顺序。
- `bytes.group_by(b: Bytes, f: (Int) -> B) -> Map[B, List[Int]]` — 按 f 的键对字节分组，各组保持顺序。
- `bytes.sort(b: Bytes) -> List[Int]` — 升序排列的字节。
- `bytes.sort_by(b: Bytes, f: (Int) -> B) -> List[Int]` — 按 f 的键排序的字节，相同键保持顺序。
- `bytes.sort_with(b: Bytes, cmp: (Int, Int) -> Int) -> List[Int]` — 按 cmp 排序的字节（负数表示第一个参数在前），稳定。
- `bytes.min(b: Bytes) -> Option[Int]` — 最小字节；空时为 none。
- `bytes.max(b: Bytes) -> Option[Int]` — 最大字节；空时为 none。
- `bytes.min_by(b: Bytes, f: (Int) -> K) -> Option[Int]` — 具有最小键的首个字节；空时为 none。
- `bytes.max_by(b: Bytes, f: (Int) -> K) -> Option[Int]` — 具有最大键的最后一个字节；空时为 none。
- `bytes.min_with(b: Bytes, cmp: (Int, Int) -> Int) -> Option[Int]` — 按 cmp 最小的首个字节；空时为 none。
- `bytes.max_with(b: Bytes, cmp: (Int, Int) -> Int) -> Option[Int]` — 按 cmp 最大的最后一个字节；空时为 none。
- `bytes.sum(b: Bytes) -> Int` — 字节之和；b 为空为0。
- `bytes.product(b: Bytes) -> Int` — 字节之积，回绕；b 为空为1。
- `bytes.take_while(b: Bytes, f: (Int) -> Bool) -> Bytes` — 所有字节均满足 f 的最长前缀。
- `bytes.drop_while(b: Bytes, f: (Int) -> Bool) -> Bytes` — 去掉所有字节均满足 f 的最长前缀后的 b。
- `bytes.window(b: Bytes, n: Int) -> List[Bytes]` — 每段 n 个连续字节；n > len 为 []，n <= 0 时中止。
- `bytes.dedup(b: Bytes) -> Bytes` — 将 b 中每段相邻相等字节合并为一个。
- `bytes.intersperse(b: Bytes, sep: Int) -> Bytes` — 在 b 的每对相邻字节间插入 sep。
- `bytes.unique(b: Bytes) -> Bytes` — 按顺序保留每个字节值的首次出现。
- `bytes.unique_by(b: Bytes, f: (Int) -> K) -> Bytes` — 按顺序保留 f 给出的每个键的首个字节。
- `bytes.last_index_of(b: Bytes, needle: Bytes) -> Option[Int]` — 最后一个 needle 的字节索引，或 none；空 needle 在 len 处找到。
- `bytes.replace(b: Bytes, from: Bytes, to: Bytes) -> Bytes` — 从左到右将 b 中所有不重叠的 from 替换为 to。
- `bytes.replace_first(b: Bytes, from: Bytes, to: Bytes) -> Bytes` — 将 b 中第一个 from 替换为 to。
- `bytes.strip_prefix(b: Bytes, prefix: Bytes) -> Option[Bytes]` — 去掉 b 的 prefix；不以它开头则为 none。
- `bytes.strip_suffix(b: Bytes, suffix: Bytes) -> Option[Bytes]` — 去掉 b 的 suffix；不以它结尾则为 none。
- `bytes.count_matches(b: Bytes, needle: Bytes) -> Int` — needle 不重叠的出现次数；空 needle 为 len + 1。

## compute

[compute.md](compute.zh-CN.md)

- `compute.ns(n: Int) -> Compute` — n 纳秒确定性计算时间；负 n 则 abort。
- `compute.us(n: Int) -> Compute` — n 微秒确定性计算时间；负 n 则 abort。
- `compute.ms(n: Int) -> Compute` — n 毫秒确定性计算时间；负 n 则 abort。
- `compute.s(n: Int) -> Compute` — n 秒确定性计算时间；负 n 则 abort。
- `compute.min(n: Int) -> Compute` — n 分钟确定性计算时间；负 n 则 abort。
- `compute.h(n: Int) -> Compute` — n 小时确定性计算时间；负 n 则 abort。
- `compute.compare(a: Compute, b: Compute) -> Int` — a < b 为负，相等为 0，a > b 为正。

## datetime

[datetime.md](datetime.zh-CN.md)

- `datetime.add_days(ts: Int, n: Int) -> Int` — ts + n * 86400；没有夏令时或日历逻辑。
- `datetime.add_hours(ts: Int, n: Int) -> Int` — ts + n * 3600；n 可以为负。
- `datetime.add_minutes(ts: Int, n: Int) -> Int` — ts + n * 60；n 可以为负。
- `datetime.add_seconds(ts: Int, n: Int) -> Int` — ts + n；n 可以为负。
- `datetime.day(ts: Int) -> Int` — 日历日1..31（UTC）。
- `datetime.format(ts: Int, pattern: String) -> String` — 替换 %Y %m %d %H %M %S（UTC）；没有 %% 转义。
- `datetime.from_parts(y: Int, m: Int, d: Int, h: Int, min: Int, s: Int) -> Result[Int, String]` — UTC 纪元秒；与 parse_iso 一样，字段越界为 err。
- `datetime.hour(ts: Int) -> Int` — UTC 小时0..23。
- `datetime.minute(ts: Int) -> Int` — UTC 分钟0..59。
- `datetime.month(ts: Int) -> Int` — UTC 月份1..12。
- `effect datetime.now() -> Int` — 墙上时钟的 Unix 秒（非单调）。
- `effect datetime.monotonic_ns() -> Int` — 自首次调用起的单调纳秒；比较差值。
- `effect datetime.now_ms() -> Int` — 墙上时钟自 Unix 纪元起的毫秒（非单调）。
- `datetime.parse_iso(s: String) -> Result[Int, String]` — YYYY-MM-DDTHH:MM:SSZ 的纪元秒，年0..=9999；小数部分为 err。
- `datetime.second(ts: Int) -> Int` — UTC 秒0..59。
- `datetime.to_iso(ts: Int) -> String` — UTC 的 YYYY-MM-DDTHH:MM:SSZ。
- `datetime.weekday(ts: Int) -> String` — UTC 的英语星期名；0为 Thursday。
- `datetime.year(ts: Int) -> Int` — UTC 日历年。

## deque

[deque.md](deque.zh-CN.md)

- `deque.new() -> Deque[A]` — 空双端队列。
- `deque.from_list(xs: List[A]) -> Deque[A]` — 由 xs 构成的队列，从头到尾。
- `deque.to_list(d: Deque[A]) -> List[A]` — 从头到尾的元素。
- `deque.len(d: Deque[A]) -> Int` — 元素数量；空时为0。
- `deque.is_empty(d: Deque[A]) -> Bool` — 当且仅当 d 无元素时为 true。
- `deque.push(mut d: Deque[A], x: A) -> Unit` — 在尾部原地追加 x。
- `deque.push_front(mut d: Deque[A], x: A) -> Unit` — 在头部原地添加 x。
- `deque.pop(mut d: Deque[A]) -> Option[A]` — 移除并返回末尾元素；空时为 none。
- `deque.pop_front(mut d: Deque[A]) -> Option[A]` — 移除并返回头部元素；空时为 none。
- `deque.first(d: Deque[A]) -> Option[A]` — 首个元素，空时为 none。
- `deque.last(d: Deque[A]) -> Option[A]` — 末尾元素，空时为 none。
- `deque.get(d: Deque[A], i: Int) -> Option[A]` — 从头部起索引 i 处的元素；越界为 none。
- `deque.contains(d: Deque[A], x: A) -> Bool` — 某个元素 == x 时为 true。
- `deque.clear(mut d: Deque[A]) -> Unit` — 原地移除 d 的全部元素。
- `deque.map(d: Deque[A], f: (A) -> B) -> Deque[B]` — 由 f(x) 构成的队列，从头到尾；f 也按从头到尾的顺序运行。
- `deque.filter(d: Deque[A], f: (A) -> Bool) -> Deque[A]` — 满足 f 的元素，保持顺序。
- `deque.fold(d: Deque[A], init: B, f: (B, A) -> B) -> B` — 从头到尾折叠 f(acc, x)。
- `deque.any(d: Deque[A], f: (A) -> Bool) -> Bool` — 某个元素满足 f 时为 true；空时为 false。
- `deque.all(d: Deque[A], f: (A) -> Bool) -> Bool` — 所有元素满足 f 时为 true；空时为 true。
- `deque.count(d: Deque[A], f: (A) -> Bool) -> Int` — 满足 f 的元素数量。
- `deque.find(d: Deque[A], f: (A) -> Bool) -> Option[A]` — 最靠近头部且满足 f 的元素，或 none。
- `deque.reverse(d: Deque[A]) -> Deque[A]` — 从尾到头的元素。
- `deque.take(d: Deque[A], n: Int) -> Deque[A]` — 前 n 个元素，同 list.take。
- `deque.drop(d: Deque[A], n: Int) -> Deque[A]` — 除前 n 个以外的全部元素，同 list.drop。
- `deque.take_end(d: Deque[A], n: Int) -> Deque[A]` — 末尾 n 个元素，同 list.take_end。
- `deque.drop_end(d: Deque[A], n: Int) -> Deque[A]` — 除末尾 n 个以外的全部元素，同 list.drop_end。
- `deque.slice(d: Deque[A], start: Int, end: Int) -> Deque[A]` — 索引 start..<end，同 list.slice。
- `deque.set(d: Deque[A], i: Int, x: A) -> Deque[A]` — 将位置 i 设为 x 的副本，同 list.set。
- `deque.insert(d: Deque[A], i: Int, x: A) -> Deque[A]` — 在 i 插入 x 的副本，同 list.insert。
- `deque.remove_at(d: Deque[A], i: Int) -> Deque[A]` — 去掉位置 i 的副本，同 list.remove_at。
- `deque.enumerate(d: Deque[A]) -> List[(Int, A)]` — (索引, 元素) 对，索引从头部的0开始。
- `deque.zip(d: Deque[A], ys: List[B]) -> List[(A, B)]` — 按位置与 ys 配对，在较短者末尾停止。
- `deque.zip_with(d: Deque[A], ys: List[B], f: (A, B) -> C) -> List[C]` — 对(元素, ys 元素)对应用 f，在较短者末尾停止。
- `deque.filter_map(d: Deque[A], f: (A) -> Option[B]) -> List[B]` — f 给出的 some，从头到尾排列。
- `deque.find_map(d: Deque[A], f: (A) -> Option[B]) -> Option[B]` — f 给出的第一个 some，或 none；命中后不再调用 f。
- `deque.reduce(d: Deque[A], f: (A, A) -> A) -> Option[A]` — 以首元素为初值的左折叠；空时为 none。
- `deque.scan(d: Deque[A], init: B, f: (B, A) -> B) -> List[B]` — 从头到尾的逐步折叠值，不包含 init。
- `deque.partition(d: Deque[A], f: (A) -> Bool) -> (List[A], List[A])` — (满足 f 的元素, 其余元素)，保持顺序。
- `deque.group_by(d: Deque[A], f: (A) -> B) -> Map[B, List[A]]` — 键 f(x) → 元素；键按首次出现顺序排列。
- `deque.sort(d: Deque[A]) -> List[A]` — 元素按升序排列；Float 按 IEEE totalOrder。
- `deque.sort_by(d: Deque[A], f: (A) -> B) -> List[A]` — 按键 f(x) 稳定排序；每个元素运行 f 一次。
- `deque.sort_with(d: Deque[A], cmp: (A, A) -> Int) -> List[A]` — 按 cmp 稳定排序；负数将 a 放在前面。
- `deque.min(d: Deque[A]) -> Option[A]` — 最小元素，空时为 none。
- `deque.max(d: Deque[A]) -> Option[A]` — 最大元素，空时为 none。
- `deque.min_by(d: Deque[A], f: (A) -> K) -> Option[A]` — 键 f(x) 最小的元素；并列时取最靠前者。
- `deque.max_by(d: Deque[A], f: (A) -> K) -> Option[A]` — 键 f(x) 最大的元素；并列时取最靠前者。
- `deque.min_with(d: Deque[A], cmp: (A, A) -> Int) -> Option[A]` — 按 cmp 的最小元素；并列时取最靠前者。
- `deque.max_with(d: Deque[A], cmp: (A, A) -> Int) -> Option[A]` — 按 cmp 的最大元素；并列时取最靠前者。
- `deque.find_index(d: Deque[A], f: (A) -> Bool) -> Option[Int]` — 从头部起第一个满足 f 的 x 的索引，或 none。
- `deque.take_while(d: Deque[A], f: (A) -> Bool) -> Deque[A]` — 从头部起满足 f 的最长连续段。
- `deque.drop_while(d: Deque[A], f: (A) -> Bool) -> Deque[A]` — 去掉头部满足 f 的最长连续段后剩余的部分。
- `deque.chunk(d: Deque[A], n: Int) -> List[Deque[A]]` — 从头部起每 n 个一段，同 list.chunk。
- `deque.window(d: Deque[A], n: Int) -> List[Deque[A]]` — 所有长为 n 的连续段，同 list.window。
- `deque.split_at(d: Deque[A], n: Int) -> (Deque[A], Deque[A])` — (前 n 个, 其余)，同 list.split_at。
- `deque.dedup(d: Deque[A]) -> Deque[A]` — 合并相邻相等的连续元素，同 list.dedup。
- `deque.intersperse(d: Deque[A], sep: A) -> Deque[A]` — 相邻元素间插入 sep。
- `deque.unique(d: Deque[A]) -> Deque[A]` — 去重，保留最靠近头部的出现。
- `deque.unique_by(d: Deque[A], f: (A) -> K) -> Deque[A]` — 每个不同键 f(x) 的第一个元素。
- `deque.sum(d: Deque[A]) -> A` — 元素之和；空时为0；整数溢出时回绕。
- `deque.product(d: Deque[A]) -> A` — 元素之积；空时为1；整数溢出时回绕。

## duration

[duration.md](duration.zh-CN.md)

- `duration.ns(n: Int) -> Duration` — n 纳秒墙钟时间；n 为负则中止。
- `duration.us(n: Int) -> Duration` — n 微秒墙钟时间；n 为负则中止。
- `duration.ms(n: Int) -> Duration` — n 毫秒墙钟时间；n 为负则中止。
- `duration.s(n: Int) -> Duration` — n 秒墙钟时间；n 为负则中止。
- `duration.min(n: Int) -> Duration` — n 分钟墙钟时间；n 为负则中止。
- `duration.h(n: Int) -> Duration` — n 小时墙钟时间；n 为负则中止。
- `duration.compare(a: Duration, b: Duration) -> Int` — a < b 时为负，相等时为0，a > b 时为正。

## env

[env.md](env.zh-CN.md)

- `effect env.args() -> List[String]` — 程序参数，不含 argv[0]。
- `effect env.get(name: String) -> Option[String]` — name 的值，未设置为 none。
- `effect env.secret(name: String) -> Option[Secret[String]]` — 以 Secret 返回 name 的值（打印为 `Secret(***)`，常量时间 ==）；未设置为 none。用于令牌或密钥。
- `effect env.set(name: String, value: String) -> Unit` — 为此进程及之后启动的子进程设置 name；名称为空或含 `=`、NUL，或值含 NUL 时为 err。
- `effect env.remove(name: String) -> Unit` — 为此进程及之后启动的子进程移除 name；未设置也成功；名称为空或含 `=`、NUL 时为 err。
- `effect env.vars() -> Map[String, String]` — 此进程的全部环境变量，以名称为键并排序。秘密也作为普通值包含在内：凭据请改用 `env.secret` 按名称读取。
- `effect env.cwd() -> String` — 当前目录的绝对路径；无法读取时为 err。
- `effect env.sleep_ms(ms: Int) -> Unit` — 阻塞 ms 毫秒；零或负值不等待即返回。
- `env.os() -> String` — 宿主 OS：macos、linux、windows 或 unknown；wasm 上为 "wasi"（C-189）。

## ffi

[ffi.md](ffi.zh-CN.md)

- `ffi.region(b: Bytes) -> Region` — b 的只读视图；至少在视图存在期间 b 保持存活。
- `effect ffi.alloc(n: Int) -> Region` — 含 n 个零字节的新可写区域；n 为负则 err。
- `ffi.len(r: Region) -> Int` — 区域的字节长度。
- `ffi.is_writable(r: Region) -> Bool` — 若 write 可改变区域则 true（ffi.alloc 和可写外部区域）。
- `effect ffi.read(r: Region, off: Int, n: Int) -> Bytes` — off 处 n 字节的副本；除非 0 <= off 且 off + n <= len(r)，否则 err。
- `effect ffi.write(r: Region, off: Int, b: Bytes) -> Unit` — 在 off 写入 b；区域只读或放不下 b 则 err。
- `effect ffi.to_bytes(r: Region) -> Bytes` — 整个区域的 Bytes 副本。
- `ffi.slice(r: Region, off: Int, n: Int) -> Result[Region, String]` — r 内 off 处 n 字节的只读视图；除非 0 <= off 且 off + n <= len(r)，否则 err。

## float

[float.md](float.zh-CN.md)

- `float.to_string(x: Float) -> String` — 可往返转换的最短数字串，无指数；2.0保留 .0。
- `float.parse(s: String) -> Result[Float, String]` — 去除 s 两侧 ASCII 空白后解析为有限 Float；无效内容、inf、NaN、溢出返回 err。
- `float.to_fixed(x: Float, decimals: Int) -> String` — 定点格式，中点舍入到偶数；decimals 不在0..4096时中止。
- `float.to_bits(f: Float) -> Int` — 以 Int 返回 IEEE-754 位；任何 NaN 都得到规范 NaN。
- `float.abs(x: Float) -> Float` — 绝对值；-0.0得到0.0，NaN 保持 NaN。
- `float.floor(x: Float) -> Float` — 向 -inf 舍入；-0.5得到 -1.0。
- `float.ceil(x: Float) -> Float` — 向 +inf 舍入；-0.5得到 -0.0。
- `float.round(x: Float) -> Float` — 最近的整数值；中点向远离零的方向舍入。
- `float.min(a: Float, b: Float) -> Float` — 较小者；忽略 NaN 操作数；-0.0 < 0.0。
- `float.max(a: Float, b: Float) -> Float` — 较大者；忽略 NaN 操作数；0.0 > -0.0。
- `float.clamp(x: Float, lo: Float, hi: Float) -> Float` — 将 x 限制在 lo..hi；lo > hi 或边界为 NaN 时中止。
- `float.sign(n: Float) -> Float   (deprecated — use float.signum)` — 按符号位返回1.0或-1.0（0.0得到1.0）；NaN 得到 NaN。
- `float.signum(x: Float) -> Float` — 按符号位返回1.0或-1.0（0.0得到1.0，-0.0得到-1.0）；NaN 得到 NaN。
- `float.is_nan(x: Float) -> Bool` — 仅对 NaN（不等于自身）为 true。
- `float.is_infinite(x: Float) -> Bool` — 对 inf 和 -inf 为 true，对 NaN 为 false。
- `float.trunc(x: Float) -> Float` — 向0舍入；-0.5得到 -0.0。
- `float.is_finite(x: Float) -> Bool` — 除非 x 为 inf、-inf 或 NaN，否则为 true。
- `float.pow(base: Float, exp: Float) -> Float` — base 的 exp 次幂；负 base 与小数 exp 得到 NaN。
- `float.compare(a: Float, b: Float) -> Int` — 按 IEEE-754 全序返回 -1、0或1：-0.0 < 0.0，NaN 高于 inf 且等于自身。
- `float.from_bits(bits: Int) -> Float` — IEEE-754 binary64 位为 bits 的 Float；任何 NaN 都得到同一个 NaN。
- `float.infinity() -> Float` — 正无穷。
- `float.nan() -> Float` — 唯一的 quiet NaN（位为0x7ff8000000000000）。
- `float.epsilon() -> Float` — 1.0与下一个 Float 的间隔：2^-52。
- `float.max_value() -> Float` — 最大的有限 Float，约1.7976931348623157e308。
- `float.min_value() -> Float` — 最负的有限 Float（= -max_value()），不是最小正数。
- `float.from(x: S) -> Float` — 将 x 转为最近的 Float（中点到偶数）；NaN 保持 NaN。
- `float.from_checked(x: S) -> Option[Float]` — 若 Float 能精确容纳 x，则 some(x as Float)，否则（被舍入的值、NaN）为 none。

## float32

[float32.md](float32.zh-CN.md)

- `float32.to_string(x: Float32) -> String` — f32 可往返转换的最短数字串，无指数；0.1保持0.1，2.0保留 .0。
- `float32.compare(a: Float32, b: Float32) -> Int` — 按 IEEE 754 全序返回 -1、0或1：-0.0 < 0.0，NaN 高于 inf，NaN 等于 NaN。
- `float32.min(a: Float32, b: Float32) -> Float32` — 较小者；忽略 NaN 操作数；-0.0 < 0.0。
- `float32.max(a: Float32, b: Float32) -> Float32` — 较大者；忽略 NaN 操作数；0.0 > -0.0。
- `float32.clamp(x: Float32, lo: Float32, hi: Float32) -> Float32` — 将 x 限制在 lo..hi；lo > hi 或边界为 NaN 时中止。
- `float32.is_nan(x: Float32) -> Bool` — 仅对 NaN 为 true。
- `float32.is_infinite(x: Float32) -> Bool` — 对 inf 和 -inf 为 true，对 NaN 为 false。
- `float32.is_finite(x: Float32) -> Bool` — 除非 x 为 inf、-inf 或 NaN，否则为 true。
- `float32.abs(x: Float32) -> Float32` — 绝对值；-0.0得到0.0，NaN 保持 NaN。
- `float32.floor(x: Float32) -> Float32` — 向 -inf 舍入；-0.5得到 -1.0。
- `float32.ceil(x: Float32) -> Float32` — 向 +inf 舍入；-0.5得到 -0.0。
- `float32.round(x: Float32) -> Float32` — 最近的整数值；中点向远离零的方向舍入。
- `float32.trunc(x: Float32) -> Float32` — 向0舍入；-0.5得到 -0.0。
- `float32.signum(x: Float32) -> Float32` — 按符号位返回1.0或-1.0（0.0得到1.0，-0.0得到-1.0）；NaN 得到 NaN。
- `float32.to_bits(x: Float32) -> Int` — 将 IEEE-754 binary32 的32位作为非负 Int 返回；任何 NaN 都得到0x7fc00000。
- `float32.from_bits(bits: Int) -> Float32` — binary32 位为 bits 的低32位的 Float32。
- `float32.infinity() -> Float32` — 正无穷。
- `float32.nan() -> Float32` — 唯一的 quiet NaN（位为0x7fc00000）。
- `float32.epsilon() -> Float32` — 1.0与下一个 Float32 的间隔：2^-23。
- `float32.max_value() -> Float32` — 最大的有限 Float32，约3.4028235e38。
- `float32.min_value() -> Float32` — 最负的有限 Float32（= -max_value()），不是最小正数。
- `float32.parse(s: String) -> Result[Float32, String]` — 将去除两侧空白的 s 解析为 Float32，语法同 float.parse；无效内容返回 err，溢出得到 inf。
- `float32.from(x: S) -> Float32` — 将 x 舍入一次至最近的 Float32（中点到偶数）；NaN 保持 NaN。
- `float32.from_checked(x: S) -> Option[Float32]` — 若 Float32 能精确容纳 x，则 some(x as Float32)，否则（被舍入的值、NaN）为 none。

## float64

[float64.md](float64.zh-CN.md)

- `float64.to_string(x: Float64) -> String` — 普通十进制，绝不使用指数；2.0保留 .0。
- `float64.compare(a: Float64, b: Float64) -> Int` — 按 IEEE 754 全序返回 -1、0或1：-0.0 < 0.0，NaN 高于 inf，NaN 等于 NaN。
- `float64.min(a: Float64, b: Float64) -> Float64` — 较小者；忽略 NaN 操作数；-0.0 < 0.0。
- `float64.max(a: Float64, b: Float64) -> Float64` — 较大者；忽略 NaN 操作数；0.0 > -0.0。
- `float64.clamp(x: Float64, lo: Float64, hi: Float64) -> Float64` — 将 x 限制在 lo..hi；lo > hi 或边界为 NaN 时中止。
- `float64.is_nan(x: Float64) -> Bool` — 仅对 NaN 为 true。
- `float64.is_infinite(x: Float64) -> Bool` — 对 inf 和 -inf 为 true，对 NaN 为 false。
- `float64.is_finite(x: Float64) -> Bool` — 除非 x 为 inf、-inf 或 NaN，否则为 true。
- `float64.abs(x: Float64) -> Float64` — 绝对值；-0.0得到0.0，NaN 保持 NaN。
- `float64.floor(x: Float64) -> Float64` — 向 -inf 舍入；-0.5得到 -1.0。
- `float64.ceil(x: Float64) -> Float64` — 向 +inf 舍入；-0.5得到 -0.0。
- `float64.round(x: Float64) -> Float64` — 最近的整数值；中点向远离零的方向舍入。
- `float64.trunc(x: Float64) -> Float64` — 向0舍入；-0.5得到 -0.0。
- `float64.signum(x: Float64) -> Float64` — 按符号位返回1.0或-1.0（0.0得到1.0，-0.0得到-1.0）；NaN 得到 NaN。
- `float64.to_bits(x: Float64) -> Int` — 将 IEEE-754 binary64 的64位作为 Int 返回；任何 NaN 都得到规范 NaN。
- `float64.from_bits(bits: Int) -> Float64` — IEEE-754 binary64 位为 bits 的 Float64。
- `float64.infinity() -> Float64` — 正无穷。
- `float64.nan() -> Float64` — 唯一的 quiet NaN（位为0x7ff8000000000000）。
- `float64.epsilon() -> Float64` — 1.0与下一个 Float64 的间隔：2^-52。
- `float64.max_value() -> Float64` — 最大的有限 Float64，约1.7976931348623157e308。
- `float64.min_value() -> Float64` — 最负的有限 Float64（= -max_value()），不是最小正数。
- `float64.parse(s: String) -> Result[Float64, String]` — 去除 s 两侧 ASCII 空白后解析为 Float64，语法同 float.parse；无效内容、inf、NaN 返回 err。
- `float64.from(x: S) -> Float64` — 将 x 转为最近的 Float64（中点到偶数）；NaN 保持 NaN。
- `float64.from_checked(x: S) -> Option[Float64]` — 若 Float64 能精确容纳 x，则 some(x as Float64)，否则（被舍入的值、NaN）为 none。

## fs

[fs.md](fs.zh-CN.md)

- `effect fs.read_text(path: String) -> String` — 将整个普通文件（<= 1 GiB）读为 UTF-8；不存在或非 UTF-8 则 err。
- `effect fs.read_bytes(path: String) -> Bytes` — 将整个普通文件（<= 1 GiB）读为 Bytes；不存在则 err。
- `effect fs.write_text(path: String, content: String) -> Unit` — 覆盖或创建；无父目录则 err。
- `effect fs.write_bytes(path: String, data: Bytes) -> Unit` — 原样写入 data，替换文件。
- `effect fs.append_text(path: String, content: String) -> Unit` — 追加 content；不存在则创建。
- `effect fs.append_bytes(path: String, data: Bytes) -> Unit` — 原样追加 data；不存在则创建。
- `effect fs.mkdir_p(path: String) -> Unit` — 创建目录及父目录；已存在则 ok。
- `effect fs.exists(path: String) -> Bool` — 不存在或为悬空符号链接时为 false。
- `effect fs.read_lines(path: String) -> List[String]` — 普通文件（<= 1 GiB）的行，识别 CRLF；末尾换行不增加一行。
- `effect fs.fold_lines(path: String, init: A, f: (A, String) -> A) -> A` — 对行进行流式 fold；不存在则 err。
- `effect fs.for_each_line(path: String, f: (String) -> Unit) -> Unit` — 流式逐行调用 f；可能在文件中途 err。
- `effect fs.fold_lines_range(path: String, start: Int, end: Int, init: A, f: (A, String) -> A) -> A` — 对在字节 [start, end) 中开始的行进行 fold。
- `effect fs.fold_lines_chunked(path: String, workers: Int, init: A, f: (A, String) -> A) -> List[A]` — 按顺序返回并行分块的部分结果；空则 []。
- `effect fs.read_text_if_exists(path: String) -> Option[String]` — 文本，不存在则 none；非 UTF-8 则 err。
- `effect fs.read_bytes_if_exists(path: String) -> Option[Bytes]` — 文件 Bytes，不存在则 none。
- `effect fs.read_lines_if_exists(path: String) -> Option[List[String]]` — read_lines 结果，不存在则 none。
- `effect fs.remove(path: String) -> Unit` — 删除文件或空目录；不存在则 err。
- `effect fs.list_dir(path: String) -> List[String]` — 排序后的名称（不是路径）；不是目录则 err。
- `effect fs.is_dir(path: String) -> Bool` — 目录则 true；跟随符号链接。
- `effect fs.is_file(path: String) -> Bool` — 普通文件则 true；跟随符号链接。
- `effect fs.copy(src: String, dst: String) -> Unit` — 将 src 复制到 dst；覆盖 dst。
- `effect fs.rename(src: String, dst: String) -> Unit` — 将 src 移到 dst；替换已有 dst。
- `effect fs.walk(dir: String) -> List[String]` — dir 下排序后的路径；列出链接但不跟随。
- `effect fs.remove_all(path: String) -> Unit` — 删除目录树或文件；不存在则 err。
- `effect fs.temp_dir() -> String` — 宿主临时目录；这是位置，不是安全的名称。
- `effect fs.stat(path: String) -> FileStat` — 大小、类型和 mtime；不存在则 err。
- `effect fs.glob(pattern: String) -> List[String]` — 排序后的 * 与 ** 匹配；不跟随链接。
- `effect fs.create_temp_file(prefix: String) -> String` — 随机名称的新私有文件，返回其路径。
- `effect fs.create_temp_dir(prefix: String) -> String` — 随机名称的新私有目录，返回其路径。
- `effect fs.is_symlink(path: String) -> Bool` — 符号链接则 true，即使悬空。
- `effect fs.open_root(dir: String) -> FsRoot` — 将 dir 作为 FsRoot；若不是目录则 err。
- `effect fs.root_read_text(root: FsRoot, path: String) -> String` — 对 root 内的 path 执行 read_text；逃出范围则 err。
- `effect fs.root_read_bytes(root: FsRoot, path: String) -> Bytes` — 对 root 内的 path 执行 read_bytes；逃出范围则 err。
- `effect fs.root_write_text(root: FsRoot, path: String, content: String) -> Unit` — 向 root 内的 path 写入 content；逃出范围则 err。
- `effect fs.root_list_dir(root: FsRoot, path: String) -> List[String]` — 对 root 内的 path 执行 list_dir；逃出范围则 err。
- `effect fs.root_exists(root: FsRoot, path: String) -> Bool` — 对 root 内的 path 执行 exists；逃出范围则 err。
- `effect fs.root_remove(root: FsRoot, path: String) -> Unit` — 对 root 内的 path 执行 remove；逃出范围则 err。

## hash

[hash.md](hash.zh-CN.md)

- `hash.fnv1a32(b: Bytes) -> Int` — b 的 32-bit FNV-1a，范围 0..4294967295。
- `hash.sha256(b: Bytes) -> Bytes` — b 的 32-byte SHA-256 摘要。不是 MAC：sha256(key ++ msg) 可通过 length extension 伪造；使用 hash.hmac_sha256 / hash.is_valid_hmac_sha256。
- `hash.sha256_hex(b: Bytes) -> String` — b 的 SHA-256，64 个小写十六进制字符。
- `hash.crc32(b: Bytes) -> Int` — CRC-32（IEEE，反射型），作为 unsigned Int；空 b 为 0。
- `hash.adler32(b: Bytes) -> Int` — Adler-32，作为 unsigned Int；空 b 为 1。
- `hash.sha512(b: Bytes) -> Bytes` — b 的 64-byte SHA-512 摘要。不是 MAC：sha512(key ++ msg) 可通过 length extension 伪造；使用 hash.hmac_sha512 / hash.is_valid_hmac_sha512。
- `hash.sha512_hex(b: Bytes) -> String` — b 的 SHA-512，128 个小写十六进制字符。
- `hash.hmac_sha256(key: Secret[Bytes], b: Bytes) -> Secret[Bytes]` — key 下 b 的 32-byte HMAC-SHA256（RFC 2104），key 在前；tag 是 Secret，因此 == 为恒定时间。十六进制：hex.encode(secret.reveal(tag))。
- `hash.hmac_sha512(key: Secret[Bytes], b: Bytes) -> Secret[Bytes]` — key 下 b 的 64-byte HMAC-SHA512（RFC 2104），key 在前；tag 是 Secret，因此 == 为恒定时间。十六进制：hex.encode(secret.reveal(tag))。
- `hash.is_valid_hmac_sha256(key: Secret[Bytes], b: Bytes, tag: Secret[Bytes]) -> Bool` — tag 是 key 下 b 的 HMAC-SHA256 时为 true，恒定时间比较。十六进制 tag：hash.is_valid_hmac_sha256(key, b, secret.new(hex.decode(sig) ?? bytes.from_list([])))。
- `hash.is_valid_hmac_sha512(key: Secret[Bytes], b: Bytes, tag: Secret[Bytes]) -> Bool` — tag 是 key 下 b 的 HMAC-SHA512 时为 true，恒定时间比较。十六进制 tag：hash.is_valid_hmac_sha512(key, b, secret.new(hex.decode(sig) ?? bytes.from_list([])))。

## heap

[heap.md](heap.zh-CN.md)

- `heap.new() -> Heap[A]` — 空堆；最小元素先出。
- `heap.new_by(f: (A) -> K) -> Heap[A]` — 按键 f(x) 排序的空堆；最小键先出。
- `heap.new_with(cmp: (A, A) -> Int) -> Heap[A]` — 按 cmp 排序的空堆；负值让 a 在前。
- `heap.from_list(xs: List[A]) -> Heap[A]` — 由 xs 构成的自然顺序堆；相等元素按列表顺序取出。
- `heap.push(mut h: Heap[A], x: A) -> Unit` — 原地添加 x。
- `heap.pop(mut h: Heap[A]) -> Option[A]` — 移除并返回下一个出堆元素；空时为 none。
- `heap.clear(mut h: Heap[A]) -> Unit` — 原地移除 h 的所有元素；保留排序规则。
- `heap.peek(h: Heap[A]) -> Option[A]` — pop 将返回的下一个元素；空时为 none。
- `heap.len(h: Heap[A]) -> Int` — 元素数量；空堆为 0。
- `heap.is_empty(h: Heap[A]) -> Bool` — 当且仅当 h 没有元素时为 true。
- `heap.to_list(h: Heap[A]) -> List[A]` — 按 pop 顺序排列的元素。
- `heap.contains(h: Heap[A], x: A) -> Bool` — 有元素 == x 时为 true。
- `heap.any(h: Heap[A], f: (A) -> Bool) -> Bool` — 某个元素的 f 成立时为 true；空时为 false。
- `heap.all(h: Heap[A], f: (A) -> Bool) -> Bool` — 每个元素的 f 成立时为 true；空时为 true。
- `heap.count(h: Heap[A], f: (A) -> Bool) -> Int` — f 成立的元素数量。
- `heap.filter(h: Heap[A], f: (A) -> Bool) -> Heap[A]` — f 成立的元素组成排序规则相同的堆；相等元素保持进入顺序。

## hex

[hex.md](hex.zh-CN.md)

- `hex.encode(b: Bytes) -> String` — 小写十六进制，每字节两个字符。
- `hex.encode_upper(b: Bytes) -> String` — 大写十六进制，每字节两个字符。
- `hex.decode(s: String) -> Result[Bytes, String]` — 从十六进制（任意大小写）解码字节；长度为奇数或字符无效则 err。

## html

[html.md](html.zh-CN.md)

- `html.escape(s: String) -> SafeHtml` — 把 s 中的 & < > " ' 转为 HTML 实体；仅适用于文本上下文。
- `html.raw(s: String) -> SafeHtml` — 将 s 直接作为 SafeHtml，不转义；仅限可信输入。
- `html.to_string(h: SafeHtml) -> String` — 原样返回 h 内的 HTML 文本。
- `html.concat(a: SafeHtml, b: SafeHtml) -> SafeHtml` — 将 a 接 b 组成一个 SafeHtml；不重新转义。
- `html.empty() -> SafeHtml` — 空字符串的 SafeHtml。
- `html.attr(name: String, value: String) -> SafeHtml` — 输出 name="value"，value 按双引号属性转义；URL、事件处理器或格式错误的名称会中止。
- `html.url_attr(name: String, url: String) -> SafeHtml` — 转义 name="url"；仅保留相对 URL 或 http/https/mailto，否则为 about:invalid#almide-blocked。
- `html.script_json(v: Value) -> SafeHtml` — 将 json.stringify(v) 中的 < > & U+2028 U+2029 写成 \u 转义，可用于 `<script>` 内。

## http

[http.md](http.zh-CN.md)

- `effect http.serve(port: Int, handler: HttpHandler) -> Unit` — 持续在 0.0.0.0:port 提供服务；处理器 err 为 500。
- `http.response(status: Int, body: B) -> HttpResponse` — 携带正文响应；其类型决定 Content-Type（String: text/plain; charset=utf-8）。
- `http.json(status: Int, body: String) -> HttpResponse` — 以 Content-Type application/json 响应；正文是 JSON 文本。
- `http.with_headers(status: Int, body: B, headers: List[(String, String)]) -> HttpResponse` — 携带正文响应，在其 Content-Type 之后按给定顺序放入头部；同名重复时保留首次拼写和最后一个值。
- `http.with_body(resp: HttpResponse, body: B) -> HttpResponse` — 将 resp 的正文替换为 body，Content-Type 替换为 body 的类型。
- `http.typed(media_type: String, body: Bytes) -> TypedBody` — 媒体类型为 media_type 的正文；类型必须是字面量，不能是可运行脚本或标准库拥有的类型。
- `http.redirect(url: String) -> HttpResponse` — 302，Location 为 url，空正文，Content-Type 为 text/plain; charset=utf-8。
- `http.set_status(resp: HttpResponse, code: Int) -> HttpResponse` — 为 resp 设置 code；不检查范围。
- `http.status(resp: HttpResponse) -> Int` — HTTP 状态码；状态行不合法时为 0。
- `http.header(m: M, name: String) -> Option[String]` — 头 name 的第一个值，不区分大小写；缺失时为 none。
- `http.header_values(m: M, name: String) -> List[String]` — 按线上顺序返回头 name 的每个值，不区分大小写；缺失时为 []。
- `http.headers(m: M) -> List[(String, String)]` — 每个头作为 (name, value) 返回，保持线上拼写和顺序，保留重复项。
- `http.body(m: M) -> String` — 正文文本；获取的正文以有损 UTF-8 解码。
- `http.body_bytes(m: M) -> Bytes` — 正文的字节：获取的正文与发送的完全相同，构建的正文则为 body(m) 的 UTF-8 字节。
- `http.set_header(m: M, name: String, value: String) -> M` — 为 m 设置头 name，不区分大小写：原位替换第一个匹配项，否则追加；响应拒绝 Content-Type。
- `http.set_body(m: M, body: String) -> M` — 替换 m 的正文；响应的 Content-Type 变为 text/plain; charset=utf-8。
- `http.set_body_bytes(m: M, body: Bytes) -> M` — 将 m 的正文替换为按有损 UTF-8 解码的 body；响应的 Content-Type 变为 application/octet-stream。
- `http.method(req: HttpRequest) -> String` — 发送时的请求方法，例如 GET。
- `http.path(req: HttpRequest) -> String` — 请求目标，包含查询字符串（为发送构建的请求中是完整 URL）。
- `http.query_params(req: HttpRequest) -> Result[List[(String, String)], String]` — 对目标中第一个 ? 之后的部分调用 url.parse_query；各对按顺序排列；错误转义返回 err。
- `effect http.get(url: String) -> String` — 即使 404 也返回正文；仅传输失败为 err。
- `effect http.request(method: String, url: String, body: String, headers: List[(String, String)]) -> String` — 任意方法的响应正文；任何状态都是 ok。
- `effect http.get_response(url: String) -> HttpResponse` — 完整响应，包含 404；不跟随重定向。
- `effect http.send(req: HttpRequest) -> HttpResponse` — 将 req 发送到其目标指定的 URL；返回任何状态的完整响应；不跟随重定向。
- `effect http.start(req: HttpRequest, limits: HttpLimits) -> HttpCall` — 开始发送 req；立即返回句柄。
- `effect http.poll(c: HttpCall) -> Option[Result[HttpResponse, String]]` — 从不阻塞：运行时为 none，之后为结果。
- `effect http.read_new(c: HttpCall) -> String` — 自上次 read_new 起的正文文本；从不阻塞。
- `effect http.wait(c: HttpCall) -> HttpResponse` — 阻塞到调用结束；任何状态都是 ok。
- `effect http.cancel(c: HttpCall) -> Unit` — 关闭连接；调用以 err 结束。
- `http.new_request(method: String, target: String, body: String, headers: List[(String, String)]) -> HttpRequest` — 在代码中构建的请求；目标保留 ?query（send / start 使用完整 URL）。
- `http.param(req: HttpRequest, name: String) -> Option[String]` — 路由器绑定并解码的路径参数；未绑定时为 none。
- `http.route(pattern: String, handler: HttpHandler) -> HttpRoute` — 将 "GET /users/{id}" 转为路由；无方法表示任意方法。
- `http.mount(prefix: String, sub: HttpHandler) -> HttpRoute` — 前缀下的子应用：/api/x 以 /x 到达它。
- `http.router(xs: List[HttpRoute]) -> Result[HttpHandler, String]` — 将路由表作为处理器；路由冲突时为 err。
- `http.wrap(handler: HttpHandler, middleware: List[HttpMiddleware]) -> HttpHandler` — 应用中间件；第一个位于最外层。
- `http.decode_json(req: HttpRequest) -> Result[T, HttpResponse]` — 严格将 JSON 正文解码为输入类型 T；err 是 400 响应。

## int

[int.md](int.zh-CN.md)

- `int.to_string(x: Int) -> String` — 十进制数字，负数带 -。
- `int.to_hex(x: Int) -> String` — 小写十六进制，不带0x；-1得到16个 f。
- `int.parse(s: String) -> Result[Int, String]` — 去除 s 两侧的 ASCII 空格/制表符/LF/FF/CR 后解析为十进制 Int；无效内容或溢出返回 err。
- `int.abs(x: Int) -> Int` — x 的大小；min_value 回绕为自身。
- `int.min(a: Int, b: Int) -> Int` — a 与 b 中较小者。
- `int.max(a: Int, b: Int) -> Int` — a 与 b 中较大者。
- `int.clamp(x: Int, lo: Int, hi: Int) -> Int` — 将 x 限制在 lo..hi；lo > hi 时中止。
- `int.is_even(x: Int) -> Bool` — x % 2 == 0 时为 true；对负数也精确。
- `int.is_odd(x: Int) -> Bool` — x % 2 != 0 时为 true；-3算作奇数。
- `int.band(a: Int, b: Int) -> Int` — 全部64位的按位 AND。
- `int.bor(a: Int, b: Int) -> Int` — 全部64位的按位 OR。
- `int.bxor(a: Int, b: Int) -> Int` — 全部64位的按位 XOR。
- `int.bnot(x: Int) -> Int` — 按位取反，等于 -x - 1。
- `int.bshl(a: Int, n: Int) -> Int` — 将 a 左移 n 位；n 对64取模。
- `int.bshr(a: Int, n: Int) -> Int` — 算术（填充符号位）右移；n 对64取模。
- `int.count_leading_zeros(x: Int) -> Int` — 64位字的前导零位数；0时为64。
- `int.count_trailing_zeros(x: Int) -> Int` — 尾随零位数；0时为64。
- `int.pop_count(n: Int) -> Int   (deprecated — use int.count_ones)` — 置位的位数；-1时为64。
- `int.count_ones(x: Int) -> Int` — 置位的位数；-1时为64。
- `int.bit_reverse(x: Int) -> Int` — 反转64位的顺序；1得到 min_value。
- `int.byte_swap(x: Int) -> Int` — 反转8字节的顺序（翻转字节序）。
- `int.bit_width(n: Int) -> Int   (deprecated — use int.bit_length)` — 容纳 n 所需的位数；0为0，负数为64。
- `int.bit_length(x: Int) -> Int` — 容纳 x 所需的位数；0为0，负数为64。
- `int.log2_floor(x: Int) -> Int` — log2(x) 向下取整；x <= 0 时为 -1。
- `int.log2_ceil(x: Int) -> Int` — log2(x) 向上取整；x <= 1 时为0。
- `int.next_power_of_two(x: Int) -> Int` — 大于等于 x 的最小2的幂；x <= 1 时为1，超过2^62时为 min_value。
- `int.prev_power_of_two(x: Int) -> Int` — 小于等于 x 的最大2的幂；x <= 0 时为0。
- `int.wrap_add(a: Int, b: Int) -> Int` — a + b 回绕到64位。
- `int.wrap_mul(a: Int, b: Int) -> Int` — a * b 回绕到64位。
- `int.rotate_right(a: Int, n: Int) -> Int` — 将 a 的64位向右旋转 n 位；n 对64取模。
- `int.rotate_left(a: Int, n: Int) -> Int` — 将 a 的64位向左旋转 n 位；n 对64取模。
- `int.min_value() -> Int` — 最小 Int：-2^63。
- `int.max_value() -> Int` — 最大 Int：2^63-1。
- `int.compare(a: Int, b: Int) -> Int` — a 小于、等于、大于 b 时分别为 -1、0、1。
- `int.sign(x: Int) -> Int` — 按 x 的符号返回 -1、0、1；0返回0。
- `int.pow(base: Int, exp: Int) -> Int` — base 的 exp 次幂，像 * 一样回绕到64位；exp < 0 时中止。
- `int.floor_div(a: Int, b: Int) -> Int` — 商向 -inf 舍入；与 / 一样，b == 0 和 min_value / -1 时中止。
- `int.modulo(a: Int, b: Int) -> Int` — 余数与 b 同号，因此 a == floor_div(a, b) * b + modulo(a, b)；b == 0 时中止。
- `int.wrap(x: Int, bits: Int) -> Int` — x mod 2^bits，bits < 64 时非负；bits <= 0 时中止，bits >= 64 时保持 x。
- `int.wrap_sub(a: Int, b: Int) -> Int` — a - b 回绕到64位。
- `int.bshr_logical(a: Int, n: Int) -> Int` — 对64位进行零填充右移；n 对64取模。
- `int.to_string_radix(x: Int, radix: Int) -> String` — 用 radix 进制的小写带符号数字表示 x；-255的16进制是 "-ff"；radix 不在2..36时中止。
- `int.parse_radix(s: String, radix: Int) -> Result[Int, String]` — 将去除两侧空白的 s 按 radix 解析为 Int：允许 + 或 - 符号、任意大小写，以及16、8、2进制的0x、0o、0b；无效内容或溢出返回 err，radix 不在2..36时中止。
- `int.from(x: S) -> Int` — 将 x 向零舍入为 Int；NaN、无穷大或超出 Int 范围的值会中止。
- `int.from_checked(x: S) -> Option[Int]` — 若 Int 能精确容纳 x，则 some(x as Int)，否则（小数、NaN、越界）为 none。
- `int.from_saturating(x: S) -> Int` — 将 x 向零舍入为 Int，并限制在 Int 范围内；NaN 为0。
- `int.from_wrapping(x: S) -> Int` — 将整数 x 的低64位作为 Int（二进制补码）。

## int16

[int16.md](int16.zh-CN.md)

- `int16.to_string(x: Int16) -> String` — 十进制数字，负数带 -。
- `int16.min_value() -> Int16` — 最小 Int16：-32768。
- `int16.max_value() -> Int16` — 最大 Int16：32767。
- `int16.compare(a: Int16, b: Int16) -> Int` — a 小于、等于、大于 b 时分别为 -1、0、1。
- `int16.min(a: Int16, b: Int16) -> Int16` — a 与 b 中较小者。
- `int16.max(a: Int16, b: Int16) -> Int16` — a 与 b 中较大者。
- `int16.clamp(x: Int16, lo: Int16, hi: Int16) -> Int16` — 将 x 限制在 lo..hi；lo > hi 时中止。
- `int16.floor_div(a: Int16, b: Int16) -> Int16` — 商向 -inf 舍入；与 / 一样，b == 0 和 min_value / -1 时中止。
- `int16.modulo(a: Int16, b: Int16) -> Int16` — 余数与 b 同号，因此 a == floor_div(a, b) * b + modulo(a, b)；b == 0 时中止。
- `int16.pow(base: Int16, exp: Int) -> Int16` — base 的 exp 次幂，像 * 一样回绕到16位；exp < 0 时中止。
- `int16.is_even(x: Int16) -> Bool` — x 是2的倍数时为 true。
- `int16.is_odd(x: Int16) -> Bool` — x 不是2的倍数时为 true。
- `int16.abs(x: Int16) -> Int16` — x 的大小；min_value 回绕为自身。
- `int16.sign(x: Int16) -> Int` — 按 x 的符号返回 -1、0或1。
- `int16.band(a: Int16, b: Int16) -> Int16` — 按位 AND。
- `int16.bor(a: Int16, b: Int16) -> Int16` — 按位 OR。
- `int16.bxor(a: Int16, b: Int16) -> Int16` — 按位 XOR。
- `int16.bnot(x: Int16) -> Int16` — 对全部16位按位取反。
- `int16.bshl(a: Int16, n: Int) -> Int16` — 将 a 左移 n 位；n 对16取模。
- `int16.bshr(a: Int16, n: Int) -> Int16` — 填充符号位的右移；n 对16取模。
- `int16.count_leading_zeros(x: Int16) -> Int` — 在16位内计数的最高置位之上的零位数；0时为16。
- `int16.count_trailing_zeros(x: Int16) -> Int` — 最低置位之下的零位数；0时为16。
- `int16.count_ones(x: Int16) -> Int` — 16位中置位的位数。
- `int16.byte_swap(x: Int16) -> Int16` — 反转2字节的顺序（翻转字节序）。
- `int16.wrap_add(a: Int16, b: Int16) -> Int16` — a + b 回绕到16位。
- `int16.wrap_sub(a: Int16, b: Int16) -> Int16` — a - b 回绕到16位。
- `int16.wrap_mul(a: Int16, b: Int16) -> Int16` — a * b 回绕到16位。
- `int16.rotate_left(a: Int16, n: Int) -> Int16` — 将 a 的16位向左旋转 n 位；n 对16取模。
- `int16.rotate_right(a: Int16, n: Int) -> Int16` — 将 a 的16位向右旋转 n 位；n 对16取模。
- `int16.bit_length(x: Int16) -> Int` — 容纳 x 所需的位数；0为0，负数为16。
- `int16.parse(s: String) -> Result[Int16, String]` — 去除 s 两侧 ASCII 空白后解析为十进制 Int16；无效内容或超出 -32768..32767 返回 err。
- `int16.to_string_radix(x: Int16, radix: Int) -> String` — 以 radix 进制的小写数字表示 x，负数带 -；radix 不在2..36时中止。
- `int16.parse_radix(s: String, radix: Int) -> Result[Int16, String]` — 将去除两侧空白的 s 按 radix 解析为 Int16，语法同 int.parse_radix；无效内容或超出 -32768..32767 返回 err。
- `int16.from(x: S) -> Int16` — 将 x 向零舍入为 Int16；NaN、无穷大或超出 Int16 范围的值会中止。
- `int16.from_checked(x: S) -> Option[Int16]` — 若 Int16 能精确容纳 x，则 some(x as Int16)，否则（小数、NaN、越界）为 none。
- `int16.from_saturating(x: S) -> Int16` — 将 x 向零舍入为 Int16，并限制在 Int16 范围内；NaN 为0。
- `int16.from_wrapping(x: S) -> Int16` — 将整数 x 的低16位作为 Int16（二进制补码）。

## int32

[int32.md](int32.zh-CN.md)

- `int32.to_string(x: Int32) -> String` — 十进制数字，负数带 -。
- `int32.min_value() -> Int32` — 最小 Int32：-2147483648。
- `int32.max_value() -> Int32` — 最大 Int32：2147483647。
- `int32.compare(a: Int32, b: Int32) -> Int` — a 小于、等于、大于 b 时分别为 -1、0、1。
- `int32.min(a: Int32, b: Int32) -> Int32` — a 与 b 中较小者。
- `int32.max(a: Int32, b: Int32) -> Int32` — a 与 b 中较大者。
- `int32.clamp(x: Int32, lo: Int32, hi: Int32) -> Int32` — 将 x 限制在 lo..hi；lo > hi 时中止。
- `int32.floor_div(a: Int32, b: Int32) -> Int32` — 商向 -inf 舍入；与 / 一样，b == 0 和 min_value / -1 时中止。
- `int32.modulo(a: Int32, b: Int32) -> Int32` — 余数与 b 同号，因此 a == floor_div(a, b) * b + modulo(a, b)；b == 0 时中止。
- `int32.pow(base: Int32, exp: Int) -> Int32` — base 的 exp 次幂，像 * 一样回绕到32位；exp < 0 时中止。
- `int32.is_even(x: Int32) -> Bool` — x 是2的倍数时为 true。
- `int32.is_odd(x: Int32) -> Bool` — x 不是2的倍数时为 true。
- `int32.abs(x: Int32) -> Int32` — x 的大小；min_value 回绕为自身。
- `int32.sign(x: Int32) -> Int` — 按 x 的符号返回 -1、0或1。
- `int32.band(a: Int32, b: Int32) -> Int32` — 按位 AND。
- `int32.bor(a: Int32, b: Int32) -> Int32` — 按位 OR。
- `int32.bxor(a: Int32, b: Int32) -> Int32` — 按位 XOR。
- `int32.bnot(x: Int32) -> Int32` — 对全部32位按位取反。
- `int32.bshl(a: Int32, n: Int) -> Int32` — 将 a 左移 n 位；n 对32取模。
- `int32.bshr(a: Int32, n: Int) -> Int32` — 填充符号位的右移；n 对32取模。
- `int32.count_leading_zeros(x: Int32) -> Int` — 在32位内计数的最高置位之上的零位数；0时为32。
- `int32.count_trailing_zeros(x: Int32) -> Int` — 最低置位之下的零位数；0时为32。
- `int32.count_ones(x: Int32) -> Int` — 32位中置位的位数。
- `int32.byte_swap(x: Int32) -> Int32` — 反转4字节的顺序（翻转字节序）。
- `int32.wrap_add(a: Int32, b: Int32) -> Int32` — a + b 回绕到32位。
- `int32.wrap_sub(a: Int32, b: Int32) -> Int32` — a - b 回绕到32位。
- `int32.wrap_mul(a: Int32, b: Int32) -> Int32` — a * b 回绕到32位。
- `int32.rotate_left(a: Int32, n: Int) -> Int32` — 将 a 的32位向左旋转 n 位；n 对32取模。
- `int32.rotate_right(a: Int32, n: Int) -> Int32` — 将 a 的32位向右旋转 n 位；n 对32取模。
- `int32.bit_length(x: Int32) -> Int` — 容纳 x 所需的位数；0为0，负数为32。
- `int32.parse(s: String) -> Result[Int32, String]` — 去除 s 两侧 ASCII 空白后解析为十进制 Int32；无效内容或超出 -2147483648..2147483647 返回 err。
- `int32.to_string_radix(x: Int32, radix: Int) -> String` — 以 radix 进制的小写数字表示 x，负数带 -；radix 不在2..36时中止。
- `int32.parse_radix(s: String, radix: Int) -> Result[Int32, String]` — 将去除两侧空白的 s 按 radix 解析为 Int32，语法同 int.parse_radix；无效内容或超出 -2147483648..2147483647 返回 err。
- `int32.from(x: S) -> Int32` — 将 x 向零舍入为 Int32；NaN、无穷大或超出 Int32 范围的值会中止。
- `int32.from_checked(x: S) -> Option[Int32]` — 若 Int32 能精确容纳 x，则 some(x as Int32)，否则（小数、NaN、越界）为 none。
- `int32.from_saturating(x: S) -> Int32` — 将 x 向零舍入为 Int32，并限制在 Int32 范围内；NaN 为0。
- `int32.from_wrapping(x: S) -> Int32` — 将整数 x 的低32位作为 Int32（二进制补码）。

## int64

[int64.md](int64.zh-CN.md)

- `int64.to_string(x: Int64) -> String` — 十进制数字，负数带 -。
- `int64.min_value() -> Int64` — 最小 Int64：-2^63。
- `int64.max_value() -> Int64` — 最大 Int64：2^63-1。
- `int64.compare(a: Int64, b: Int64) -> Int` — a 小于、等于、大于 b 时分别为 -1、0、1。
- `int64.min(a: Int64, b: Int64) -> Int64` — a 和 b 的较小者。
- `int64.max(a: Int64, b: Int64) -> Int64` — a 和 b 的较大者。
- `int64.clamp(x: Int64, lo: Int64, hi: Int64) -> Int64` — 将 x 限制在 lo..hi；lo > hi 时 abort。
- `int64.floor_div(a: Int64, b: Int64) -> Int64` — 商向 -inf 舍入；与 / 一样，b == 0 和 min_value / -1 时 abort。
- `int64.modulo(a: Int64, b: Int64) -> Int64` — 余数与 b 同号，满足 a == floor_div(a, b) * b + modulo(a, b)；b == 0 时 abort。
- `int64.pow(base: Int64, exp: Int) -> Int64` — base 的 exp 次幂，像 * 一样回绕到 64 位；exp < 0 时 abort。
- `int64.is_even(x: Int64) -> Bool` — x 是 2 的倍数时为 true。
- `int64.is_odd(x: Int64) -> Bool` — x 不是 2 的倍数时为 true。
- `int64.abs(x: Int64) -> Int64` — x 的大小；min_value 回绕为自身。
- `int64.sign(x: Int64) -> Int` — 按 x 的符号返回 -1、0、1。
- `int64.band(a: Int64, b: Int64) -> Int64` — 按位 AND。
- `int64.bor(a: Int64, b: Int64) -> Int64` — 按位 OR。
- `int64.bxor(a: Int64, b: Int64) -> Int64` — 按位 XOR。
- `int64.bnot(x: Int64) -> Int64` — 全部 64 位的按位取反。
- `int64.bshl(a: Int64, n: Int) -> Int64` — a 左移 n 位；n 对 64 取模。
- `int64.bshr(a: Int64, n: Int) -> Int64` — 填充符号位右移；n 对 64 取模。
- `int64.count_leading_zeros(x: Int64) -> Int` — 在 64 位中最高置位位之上的零位数；0 时为 64。
- `int64.count_trailing_zeros(x: Int64) -> Int` — 最低置位位之下的零位数；0 时为 64。
- `int64.count_ones(x: Int64) -> Int` — 64 位中置位的数量。
- `int64.byte_swap(x: Int64) -> Int64` — 反转 8 字节顺序（翻转字节序）。
- `int64.wrap_add(a: Int64, b: Int64) -> Int64` — a + b 回绕到 64 位。
- `int64.wrap_sub(a: Int64, b: Int64) -> Int64` — a - b 回绕到 64 位。
- `int64.wrap_mul(a: Int64, b: Int64) -> Int64` — a * b 回绕到 64 位。
- `int64.rotate_left(a: Int64, n: Int) -> Int64` — 将 a 的 64 位向左旋转 n；n 对 64 取模。
- `int64.rotate_right(a: Int64, n: Int) -> Int64` — 将 a 的 64 位向右旋转 n；n 对 64 取模。
- `int64.bit_length(x: Int64) -> Int` — 容纳 x 所需的位数；0 时为 0，负数为 64。
- `int64.parse(s: String) -> Result[Int64, String]` — 去除 s 两侧 ASCII 空白后解析十进制 Int64；无效内容或溢出为 err。
- `int64.to_string_radix(x: Int64, radix: Int) -> String` — 用 radix 进制的小写数字表示 x；负数带 -；radix 不在 2..36 时 abort。
- `int64.parse_radix(s: String, radix: Int) -> Result[Int64, String]` — 去除 s 两侧空白后按 radix 解析 Int64，语法同 int.parse_radix；无效内容或溢出为 err。
- `int64.from(x: S) -> Int64` — x 向零舍入为 Int64；NaN、无穷大或超出 Int64 范围时 abort。
- `int64.from_checked(x: S) -> Option[Int64]` — Int64 能精确容纳 x 时为 some(x as Int64)，否则（小数部分、NaN、越界）为 none。
- `int64.from_saturating(x: S) -> Int64` — x 向零舍入为 Int64，限制在 Int64 范围；NaN 为 0。
- `int64.from_wrapping(x: S) -> Int64` — 将整数 x 的低 64 位作为 Int64（二进制补码）。

## int8

[int8.md](int8.zh-CN.md)

- `int8.to_string(x: Int8) -> String` — 十进制数字，负数带 -。
- `int8.min_value() -> Int8` — 最小 Int8：-128。
- `int8.max_value() -> Int8` — 最大 Int8：127。
- `int8.compare(a: Int8, b: Int8) -> Int` — a 小于、等于、大于 b 时分别为 -1、0、1。
- `int8.min(a: Int8, b: Int8) -> Int8` — a 与 b 中较小者。
- `int8.max(a: Int8, b: Int8) -> Int8` — a 与 b 中较大者。
- `int8.clamp(x: Int8, lo: Int8, hi: Int8) -> Int8` — 将 x 限制在 lo..hi；lo > hi 时中止。
- `int8.floor_div(a: Int8, b: Int8) -> Int8` — 商向 -inf 舍入；与 / 一样，b == 0 和 min_value / -1 时中止。
- `int8.modulo(a: Int8, b: Int8) -> Int8` — 余数与 b 同号，因此 a == floor_div(a, b) * b + modulo(a, b)；b == 0 时中止。
- `int8.pow(base: Int8, exp: Int) -> Int8` — base 的 exp 次幂，像 * 一样回绕到8位；exp < 0 时中止。
- `int8.is_even(x: Int8) -> Bool` — x 是2的倍数时为 true。
- `int8.is_odd(x: Int8) -> Bool` — x 不是2的倍数时为 true。
- `int8.abs(x: Int8) -> Int8` — x 的大小；min_value 回绕为自身。
- `int8.sign(x: Int8) -> Int` — 按 x 的符号返回 -1、0或1。
- `int8.band(a: Int8, b: Int8) -> Int8` — 按位 AND。
- `int8.bor(a: Int8, b: Int8) -> Int8` — 按位 OR。
- `int8.bxor(a: Int8, b: Int8) -> Int8` — 按位 XOR。
- `int8.bnot(x: Int8) -> Int8` — 对全部8位按位取反。
- `int8.bshl(a: Int8, n: Int) -> Int8` — 将 a 左移 n 位；n 对8取模。
- `int8.bshr(a: Int8, n: Int) -> Int8` — 填充符号位的右移；n 对8取模。
- `int8.count_leading_zeros(x: Int8) -> Int` — 在8位内计数的最高置位之上的零位数；0时为8。
- `int8.count_trailing_zeros(x: Int8) -> Int` — 最低置位之下的零位数；0时为8。
- `int8.count_ones(x: Int8) -> Int` — 8位中置位的位数。
- `int8.byte_swap(x: Int8) -> Int8` — 反转字节顺序；只有1字节，因此为 x 本身。
- `int8.wrap_add(a: Int8, b: Int8) -> Int8` — a + b 回绕到8位。
- `int8.wrap_sub(a: Int8, b: Int8) -> Int8` — a - b 回绕到8位。
- `int8.wrap_mul(a: Int8, b: Int8) -> Int8` — a * b 回绕到8位。
- `int8.rotate_left(a: Int8, n: Int) -> Int8` — 将 a 的8位向左旋转 n 位；n 对8取模。
- `int8.rotate_right(a: Int8, n: Int) -> Int8` — 将 a 的8位向右旋转 n 位；n 对8取模。
- `int8.bit_length(x: Int8) -> Int` — 容纳 x 所需的位数；0为0，负数为8。
- `int8.parse(s: String) -> Result[Int8, String]` — 去除 s 两侧 ASCII 空白后解析为十进制 Int8；无效内容或超出 -128..127 返回 err。
- `int8.to_string_radix(x: Int8, radix: Int) -> String` — 以 radix 进制的小写数字表示 x，负数带 -；radix 不在2..36时中止。
- `int8.parse_radix(s: String, radix: Int) -> Result[Int8, String]` — 将去除两侧空白的 s 按 radix 解析为 Int8，语法同 int.parse_radix；无效内容或超出 -128..127 返回 err。
- `int8.from(x: S) -> Int8` — 将 x 向零舍入为 Int8；NaN、无穷大或超出 Int8 范围的值会中止。
- `int8.from_checked(x: S) -> Option[Int8]` — 若 Int8 能精确容纳 x，则 some(x as Int8)，否则（小数、NaN、越界）为 none。
- `int8.from_saturating(x: S) -> Int8` — 将 x 向零舍入为 Int8，并限制在 Int8 范围内；NaN 为0。
- `int8.from_wrapping(x: S) -> Int8` — 将整数 x 的低8位作为 Int8（二进制补码）。

## io

[io.md](io.zh-CN.md)

- `effect io.read_line() -> Option[String]` — stdin 的下一行，去掉换行，无效 UTF-8 为 U+FFFD；EOF 为 none，空行为 some("")。
- `effect io.read_all() -> String` — stdin 的剩余文本，无效 UTF-8 为 U+FFFD；EOF 为空。
- `effect io.print(s: String) -> Unit` — 将 s 写到 stdout，不加换行，然后刷新。
- `effect io.eprint(s: String) -> Unit` — 将 s 写到 stderr，不加换行。
- `effect io.stdin() -> Stdin` — 标准输入，供 io.read / read_exact / read_to_end 使用。
- `effect io.stdout() -> Stdout` — 标准输出，供 io.write 使用。
- `effect io.stderr() -> Stderr` — 标准错误，供 io.write 使用。
- `effect io.read(h: H, max: Int) -> Bytes` — 从 h 读取最多 max 字节；h 结束后为空；max < 0 为 err。
- `effect io.read_exact(h: H, n: Int) -> Bytes` — 从 h 恰好读取 n 字节；h 先结束则为 err，并注明已到达字节数。
- `effect io.read_to_end(h: H) -> Bytes` — 读取全部字节直到 h 结束。
- `effect io.write(h: H, data: Bytes) -> Unit` — 将 data 全部写给 h。
- `effect io.close(h: H) -> Unit` — 关闭 h；关闭两次也成功。

## json

[json.md](json.zh-CN.md)

- `json.parse(text: String) -> Result[Value, String]` — 严格 JSON Value 或 err；尾部非空白文本及超过1000层的嵌套都是错误。
- `json.stringify(v: Value) -> String` — 紧凑 JSON；Float 1.0 打印为1，NaN/inf 为 null。
- `json.stringify_pretty(v: Value) -> String` — 2空格缩进的 JSON；[] 和 {} 行内显示，NaN/inf 为 null。

## list

[list.md](list.zh-CN.md)

- `list.len(xs: List[A]) -> Int` — 元素数量；空列表为 0。
- `list.length(xs: List[A]) -> Int   (deprecated — use list.len)` — list.len 的别名。
- `list.get(xs: List[A], i: Int) -> Option[A]` — 索引 i 处的元素；越界时为 none。
- `list.set(xs: List[A], i: Int, x: A) -> List[A]` — 将槽位 i = x 的副本；i 越界时不做任何操作。
- `list.swap(xs: List[A], i: Int, j: Int) -> List[A]` — 交换 i 和 j 的副本；任一索引越界时不做任何操作。
- `list.sort(xs: List[A]) -> List[A]` — 升序副本；Float 遵循 IEEE totalOrder。
- `list.reverse(xs: List[A]) -> List[A]` — 按逆序排列的元素。
- `list.contains(xs: List[A], x: A) -> Bool` — 若有元素 == x 则为 true；NaN 永不匹配。
- `list.enumerate(xs: List[A]) -> List[(Int, A)]` — （索引、元素）对，索引从 0 开始。
- `list.zip(xs: List[A], ys: List[B]) -> List[(A, B)]` — 按位置配对；到较短列表的末尾停止。
- `list.flatten(xs: List[List[A]]) -> List[A]` — 按顺序连接内部列表。
- `list.take(xs: List[A], n: Int) -> List[A]` — 前 n 个元素；n >= len 时取全部，n < 0 时为 []。
- `list.drop(xs: List[A], n: Int) -> List[A]` — 除前 n 个元素以外的全部；n >= len 时为 []，n < 0 时取全部。
- `list.unique(xs: List[A]) -> List[A]` — 移除重复项；保留首次出现的元素。
- `list.index_of(xs: List[A], x: A) -> Option[Int]` — 第一个 x 的索引；没有则为 none。
- `list.last(xs: List[A]) -> Option[A]` — 最后一个元素；为空时为 none。
- `list.chunk(xs: List[A], n: Int) -> List[List[A]]` — 每段 n 个元素，最后一段可能较短；n <= 0 时中止执行。
- `list.sum(xs: List[A]) -> A` — 任意数值的和（R-L0-42）；为空时为 0；整数溢出时回绕。
- `list.product(xs: List[A]) -> A` — 任意数值的积（R-L0-42）；为空时为 1；整数溢出时回绕。
- `list.first(xs: List[A]) -> Option[A]` — 头部元素；为空时为 none。
- `list.is_empty(xs: List[A]) -> Bool` — 当且仅当 xs 没有元素时为 true。
- `list.min(xs: List[A]) -> Option[A]` — 最小元素；为空时为 none。
- `list.max(xs: List[A]) -> Option[A]` — 最大元素；为空时为 none。
- `list.join(xs: List[T], sep: T) -> T` — 以 sep 连接各部分（R-L0-42）：String 或 Bytes；[] 时为空。
- `list.map(xs: List[A], f: (A) -> B) -> List[B]` — 按顺序对每个元素应用 f。
- `list.filter(xs: List[A], f: (A) -> Bool) -> List[A]` — f 为 true 的元素，保持顺序。
- `list.find(xs: List[A], f: (A) -> Bool) -> Option[A]` — f 成立的第一个元素；没有则为 none。
- `list.any(xs: List[A], f: (A) -> Bool) -> Bool` — 若 f 对某个 x 成立则为 true；为空时为 false。
- `list.all(xs: List[A], f: (A) -> Bool) -> Bool` — 若 f 对每个 x 成立则为 true；为空时为 true。
- `list.count(xs: List[A], f: (A) -> Bool) -> Int` — f 成立的元素个数。
- `list.flat_map(xs: List[A], f: (A) -> List[B]) -> List[B]` — 按顺序连接 f(x) 列表。
- `list.filter_map(xs: List[A], f: (A) -> Option[B]) -> List[B]` — 按顺序取 f(x) 的 some 值。
- `list.fold(xs: List[A], init: B, f: (B, A) -> B) -> B` — 从 init 开始左折叠；为空时为 init。
- `list.sort_by(xs: List[A], f: (A) -> B) -> List[A]` — 按键 f(x) 稳定排序；每个 x 执行一次 f。
- `list.take_while(xs: List[A], f: (A) -> Bool) -> List[A]` — f 成立的最长前缀。
- `list.drop_while(xs: List[A], f: (A) -> Bool) -> List[A]` — 去掉 f 成立的最长前缀后的剩余部分。
- `list.partition(xs: List[A], f: (A) -> Bool) -> (List[A], List[A])` — （f 成立的元素，其余元素），保持顺序。
- `list.reduce(xs: List[A], f: (A, A) -> A) -> Option[A]` — 以 xs[0] 为初值左折叠；为空时为 none。
- `list.group_by(xs: List[A], f: (A) -> B) -> Map[B, List[A]]` — 键 f(x) → 元素；键按首次出现顺序排列。
- `list.find_index(xs: List[A], f: (A) -> Bool) -> Option[Int]` — f 成立的第一个 x 的索引；没有则为 none。
- `list.update(xs: List[A], i: Int, f: (A) -> A) -> List[A]` — 将槽位 i = f(old) 的副本；i 越界时不做任何操作。
- `list.scan(xs: List[A], init: B, f: (B, A) -> B) -> List[B]` — 折叠过程中的值，不含 init。
- `list.zip_with(xs: List[A], ys: List[B], f: (A, B) -> C) -> List[C]` — 对二元组应用 f；到较短列表的末尾停止。
- `list.unique_by(xs: List[A], f: (A) -> K) -> List[A]` — 每个不同的键 f(x) 对应的第一个元素。
- `list.range(start: Int, end: Int) -> List[Int]` — 从 start 到 end 之前，不含 end；end <= start 时为 []。
- `list.slice(xs: List[A], start: Int, end: Int) -> List[A]` — 索引 start..<end，分别限制在 0..len 内；start >= end 时为 []。
- `list.insert(xs: List[A], i: Int, x: A) -> List[A]` — 在 i 处放入 x 的副本；i >= len 或 i < 0 时追加到末尾。
- `list.remove_at(xs: List[A], i: Int) -> List[A]` — 移除槽位 i 的副本；i 越界时不做任何操作。
- `list.repeat(x: A, n: Int) -> List[A]` — n 个 x；n <= 0 时为 []。
- `list.intersperse(xs: List[A], sep: A) -> List[A]` — 在相邻元素之间插入 sep。
- `list.dedup(xs: List[A]) -> List[A]` — 合并相邻相等的连续段；并非全局去重。
- `list.take_end(xs: List[A], n: Int) -> List[A]` — 最后 n 个元素；n >= len 时取全部，n < 0 时为 []。
- `list.drop_end(xs: List[A], n: Int) -> List[A]` — 除最后 n 个元素以外的全部；n >= len 时为 []，n < 0 时取全部。
- `list.window(xs: List[A], n: Int) -> List[List[A]]` — 所有长度为 n 的连续段；n > len 时为 []；n <= 0 时中止执行。
- `list.binary_search(xs: List[A], target: A) -> Option[Int]` — target 在升序排列的 xs 中的索引（采用 list.sort 的顺序）；没有则为 none。
- `list.push(mut xs: List[A], x: A) -> Unit` — 原地将 x 追加到 xs。
- `list.with_capacity(cap: Int) -> List[A]` — 空列表；cap 只是预分配提示。
- `list.pop(mut xs: List[A]) -> Option[A]` — 移除并返回最后一个元素；为空时为 none。
- `list.clear(mut xs: List[A]) -> Unit` — 原地移除 xs 的每个元素。
- `list.split_at(xs: List[A], n: Int) -> (List[A], List[A])` — （取前 n 个，丢弃前 n 个）；n 超过末尾时为 (xs, [])，n < 0 时为 ([], xs)。
- `list.iterate(seed: A, f: (A) -> A, n: Int) -> List[A]` — 长度为 n 的 [seed, f(seed), ...]；n <= 0 时为 []。
- `list.find_map(xs: List[A], f: (A) -> Option[B]) -> Option[B]` — f 给出的第一个 some；没有则为 none；命中后不再调用 f。
- `list.new() -> List[A]` — 空列表；与 [] 是相同的值。
- `list.from_list(xs: List[A]) -> List[A]` — xs 本身；从列表构造 Container 的构造器。
- `list.to_list(xs: List[A]) -> List[A]` — xs 本身；Container 的列表视图。
- `list.sort_with(xs: List[A], cmp: (A, A) -> Int) -> List[A]` — 按 cmp 稳定排序：负数让 a 在前，0 保持输入顺序。
- `list.min_by(xs: List[A], f: (A) -> K) -> Option[A]` — 键 f(x) 最小的元素；相等时取第一个；每个 x 执行一次 f。
- `list.max_by(xs: List[A], f: (A) -> K) -> Option[A]` — 键 f(x) 最大的元素；相等时取第一个；每个 x 执行一次 f。
- `list.min_with(xs: List[A], cmp: (A, A) -> Int) -> Option[A]` — 按 cmp 比较的最小元素；相等时取第一个；为空时为 none。
- `list.max_with(xs: List[A], cmp: (A, A) -> Int) -> Option[A]` — 按 cmp 比较的最大元素；相等时取第一个；为空时为 none。
- `list.binary_search_by(xs: List[A], target: K, f: (A) -> K) -> Option[Int]` — 在按键排序的 xs 的键 f(x) 中查找 target 的索引；没有则为 none。
- `list.binary_search_with(xs: List[A], target: A, cmp: (A, A) -> Int) -> Option[Int]` — target 在按 cmp 排序的 xs 中的索引；没有则为 none；会调用 cmp(x, target)。
- `list.unzip(xs: List[(A, B)]) -> (List[A], List[B])` — 将二元组列表转为（第一项列表，第二项列表）；是 zip 的逆操作。
- `list.range_step(start: Int, end: Int, step: Int) -> List[Int]` — start, start + step, ...，止于 end 之前；step < 0 时递减；step = 0 时中止执行。

## map

[map.md](map.zh-CN.md)

- `map.new() -> Map[K, V]` — 空 Map[K, V]。
- `map.get(m: Map[K, V], key: K) -> Option[V]` — key 对应的值，不存在时为 none。
- `map.set(m: Map[K, V], key: K, value: V) -> Map[K, V]` — 设置 key → value 的副本；已有键被覆盖，但保留位置。
- `map.contains(m: Map[K, V], key: K) -> Bool` — key 存在时为 true。
- `map.remove(m: Map[K, V], key: K) -> Map[K, V]` — 移除 key 的副本；不存在时保持不变。
- `map.keys(m: Map[K, V]) -> List[K]` — 按插入顺序返回所有键。
- `map.values(m: Map[K, V]) -> List[V]` — 按键插入顺序返回所有值。
- `map.len(m: Map[K, V]) -> Int` — entry 数量，空 map 为 0。
- `map.to_list(m: Map[K, V]) -> List[(K, V)]` — 按插入顺序返回 (key, value) 对。
- `map.merge(a: Map[K, V], b: Map[K, V]) -> Map[K, V]` — a 加上 b 的 entry；共有键以 b 为准。
- `map.is_empty(m: Map[K, V]) -> Bool` — 当且仅当 m 没有 entry 时为 true。
- `map.from_list(xs: List[(K, V)]) -> Map[K, V]` — 由 pairs xs 构造 map；重复键采用最后一个值。
- `map.map(m: Map[K, V], f: (V) -> B) -> Map[K, B]` — 键和顺序相同，值替换为 f(v)。
- `map.filter(m: Map[K, V], f: ((K, V)) -> Bool) -> Map[K, V]` — f((k, v)) 成立的 entry，保持顺序。
- `map.fold(m: Map[K, V], init: A, f: (A, (K, V)) -> A) -> A` — 按插入顺序折叠 f(acc, (k, v))。
- `map.any(m: Map[K, V], f: ((K, V)) -> Bool) -> Bool` — 存在 f((k, v)) 成立时为 true；空时为 false。
- `map.all(m: Map[K, V], f: ((K, V)) -> Bool) -> Bool` — 所有 f((k, v)) 成立时为 true；空时为 true。
- `map.count(m: Map[K, V], f: ((K, V)) -> Bool) -> Int` — f((k, v)) 成立的 entry 数量。
- `map.find(m: Map[K, V], f: ((K, V)) -> Bool) -> Option[(K, V)]` — 首个 f 成立的 (k, v)，不存在则为 none。
- `map.update(m: Map[K, V], key: K, f: (V) -> V) -> Map[K, V]` — 将 key 的值设为 f(v) 的副本；key 不存在则不操作。
- `map.upsert(m: Map[K, V], key: K, init: V, f: (V) -> V) -> Map[K, V]` — 存在则 key → f(old)，否则在末尾追加 init。
- `map.clear(mut m: Map[K, V]) -> Unit` — 原地移除 m 的每个 entry。
- `map.enumerate(m: Map[K, V]) -> List[(Int, (K, V))]` — 按插入顺序返回 (index, (key, value)) 对，index 从 0 开始。
- `map.zip(m: Map[K, V], ys: List[B]) -> List[((K, V), B)]` — 按位置将 entry 与 ys 配对，在较短者处停止。
- `map.zip_with(m: Map[K, V], ys: List[B], f: ((K, V), B) -> C) -> List[C]` — 对 (entry, ys 元素) 对应用 f，在较短者处停止。
- `map.filter_map(m: Map[K, V], f: ((K, V)) -> Option[B]) -> List[B]` — 对 entry 应用 f 所得的 some，按插入顺序返回。
- `map.find_map(m: Map[K, V], f: ((K, V)) -> Option[B]) -> Option[B]` — 对 entry 应用 f 所得的首个 some，或 none；命中后不再调用 f。
- `map.reduce(m: Map[K, V], f: ((K, V), (K, V)) -> (K, V)) -> Option[(K, V)]` — 以首个 entry 为初始值对 entry 左折叠；空时为 none。
- `map.scan(m: Map[K, V], init: B, f: (B, (K, V)) -> B) -> List[B]` — 对 entry 折叠的中间值，不包括 init。
- `map.partition(m: Map[K, V], f: ((K, V)) -> Bool) -> (List[(K, V)], List[(K, V)])` — (f 成立的 entry，其余 entry)，保持插入顺序。
- `map.group_by(m: Map[K, V], f: ((K, V)) -> B) -> Map[B, List[(K, V)]]` — 键 f(entry) → entry 组；键按首次出现顺序。
- `map.sort_by(m: Map[K, V], f: ((K, V)) -> B) -> List[(K, V)]` — 按键 f(entry) 稳定排序 entry；每个 entry 调用一次 f。
- `map.sort_with(m: Map[K, V], cmp: ((K, V), (K, V)) -> Int) -> List[(K, V)]` — 按 cmp 稳定排序 entry；负值将 a 放在前面。
- `map.min_by(m: Map[K, V], f: ((K, V)) -> U) -> Option[(K, V)]` — f(entry) 最小的 entry；并列时取首个。
- `map.max_by(m: Map[K, V], f: ((K, V)) -> U) -> Option[(K, V)]` — f(entry) 最大的 entry；并列时取首个。
- `map.min_with(m: Map[K, V], cmp: ((K, V), (K, V)) -> Int) -> Option[(K, V)]` — 按 cmp 最小的 entry；并列时取首个。
- `map.max_with(m: Map[K, V], cmp: ((K, V), (K, V)) -> Int) -> Option[(K, V)]` — 按 cmp 最大的 entry；并列时取首个。

## math

[math.md](math.zh-CN.md)

- `math.atan(x: Float) -> Float` — 反正切，单位弧度，位于 -pi/2..pi/2。
- `math.choose(n: Int, k: Int) -> Int` — 二项式系数 n over k；k < 0 或 k > n 时为 0；溢出回绕。
- `math.cos(x: Float) -> Float` — x 弧度的余弦；无限 x 为 NaN。
- `math.e() -> Float` — 欧拉数 2.718281828459045。
- `math.exp(x: Float) -> Float` — e 的 x 次方；约 709.78 以上溢出为 inf。
- `math.factorial(n: Int) -> Int` — 1..n 的乘积；n <= 0 为 1；超过 20 回绕，从 66 起为 0。
- `math.log(x: Float) -> Float` — 自然对数；0 为 -inf，负 x 为 NaN。
- `math.log10(x: Float) -> Float` — 底 10 对数；0 为 -inf，负 x 为 NaN。
- `math.log2(x: Float) -> Float` — 底 2 对数；0 为 -inf，负 x 为 NaN。
- `math.log_gamma(x: Float) -> Float` — 整个实数轴上的 ln|Gamma(x)|；极点为 inf。
- `math.pi() -> Float` — 圆周率 3.141592653589793。
- `math.sin(x: Float) -> Float` — x 弧度的正弦；无限 x 为 NaN。
- `math.sqrt(x: Float) -> Float` — 平方根；低于 0 为 NaN，-0.0 保持 -0.0。
- `math.tan(x: Float) -> Float` — x 弧度的正切；无限 x 为 NaN。
- `math.tanh(x: Float) -> Float` — -1..1 内的双曲正切；inf 返回 1.0。
- `math.asin(x: Float) -> Float` — 反正弦，单位弧度，位于 -pi/2..pi/2；-1..1 外为 NaN。
- `math.acos(x: Float) -> Float` — 反余弦，单位弧度，位于 0..pi；-1..1 外为 NaN。
- `math.atan2(y: Float, x: Float) -> Float` — 点 (x, y) 的角度，单位弧度，位于 -pi..pi。
- `math.expm1(x: Float) -> Float` — e 的 x 次方减 1，在 0 附近保持精度；约 -38 以下为 -1.0。
- `math.log1p(x: Float) -> Float` — 1 + x 的自然对数，在 0 附近保持精度；-1 为 -inf，更低为 NaN。
- `math.sinh(x: Float) -> Float` — 双曲正弦；约 710 以外溢出为 +-inf。
- `math.cosh(x: Float) -> Float` — 双曲余弦，至少 1.0；约 710 以外溢出为 inf。
- `math.hypot(a: Float, b: Float) -> Float` — sqrt(a*a + b*b)，中间过程不溢出或下溢。
- `math.cbrt(x: Float) -> Float` — 实立方根；负 x 得到负根。
- `math.asinh(x: Float) -> Float` — 反双曲正弦。
- `math.acosh(x: Float) -> Float` — 反双曲余弦，至少 0.0；1.0 以下为 NaN。
- `math.atanh(x: Float) -> Float` — 反双曲正切；+-1 为 +-inf，-1..1 外为 NaN。

## matrix

[matrix.md](matrix.zh-CN.md)

- `matrix.zeros(rows: Int, cols: Int) -> Matrix` — rows × cols 个0.0；负值限为0。
- `matrix.ones(rows: Int, cols: Int) -> Matrix` — rows × cols 个1.0；负值限为0。
- `matrix.shape(m: Matrix) -> (Int, Int)` — (rows, cols)；无行时为 (0, 0)。
- `matrix.transpose(m: Matrix) -> Matrix` — cols × rows 的副本；r × 0 变成0 × 0。
- `matrix.from_lists(xs: List[List[Float]]) -> Matrix` — 由等长行 xs 构成的矩阵；[] 为0 × 0。
- `matrix.to_lists(m: Matrix) -> List[List[Float]]` — 行作为列表；r × 0 给出 r 个空行。
- `matrix.get(m: Matrix, row: Int, col: Int) -> Option[Float]` — (row, col) 处元素；越界为 none。
- `matrix.rows(m: Matrix) -> Int` — 行数；空矩阵为0。
- `matrix.cols(m: Matrix) -> Int` — 行0的宽度；无行时为0。
- `matrix.scale(m: Matrix, s: Float) -> Matrix` — 每个元素乘 s。
- `matrix.map(m: Matrix, f: (Float) -> Float) -> Matrix` — 对每个元素应用 f。
- `matrix.from_bytes_f32_le(b: Bytes, offset: Int, rows: Int, cols: Int) -> Option[Matrix]` — 从 b 的 offset 起读取 f32 LE 的 rows × cols；b 不足时为 none。
- `matrix.to_bytes_f64_le(m: Matrix) -> Bytes` — 行优先 f64 LE，每元素8字节。
- `matrix.to_bytes_f32_le(m: Matrix) -> Bytes` — 行优先 f32 LE，每元素4字节（舍入）。
- `matrix.to_bytes_f16_le(m: Matrix) -> Bytes` — 行优先 f16 LE，每元素2字节（最近偶数舍入；溢出为 inf）。
- `matrix.from_bytes_f16_le(b: Bytes, offset: Int, rows: Int, cols: Int) -> Option[Matrix]` — 从 b 的 offset 起读取 f16 LE 的 rows × cols；b 不足时为 none。
- `matrix.from_bytes_f64_le(b: Bytes, offset: Int, rows: Int, cols: Int) -> Option[Matrix]` — 从 b 的 offset 起读取 f64 LE 的 rows × cols；b 不足时为 none。
- `matrix.broadcast_add_row(m: Matrix, bias: List[Float]) -> Matrix` — 对每行加 bias，列截断到 bias 的长度。
- `matrix.split_cols_even(m: Matrix, n: Int) -> List[Matrix]` — n 个宽为 cols/n 的切片，丢弃余数；n = 0 时为 []。
- `matrix.concat_cols(xs: List[Matrix]) -> Matrix` — 并排连接；[] 得到0 × 0。
- `matrix.concat_cols_many(matrices: List[Matrix]) -> Matrix   (deprecated — use matrix.concat_cols)` — matrix.concat_cols 的别名。
- `matrix.slice_rows(m: Matrix, start: Int, end: Int) -> Matrix` — 行 [start, end)，每个边界限制在0..行数内。
- `matrix.gather_rows(m: Matrix, indices: List[Int]) -> Matrix` — indices 处的行；越界为全零行。
- `matrix.dot_row(m: Matrix, r: Int, vec: List[Float]) -> Float` — 行 r 与 vec 的点积；r 越界为0.0。
- `matrix.row_dot(m: Matrix, r: Int, vec: List[Float]) -> Float   (deprecated — use matrix.dot_row)` — matrix.dot_row 的别名。
- `matrix.zip_with(a: Matrix, b: Matrix, f: (Float, Float) -> Float) -> Matrix` — 对相同位置的各元素对应用 f；形状不匹配时中止。
- `matrix.concat_rows(xs: List[Matrix]) -> Matrix` — 各矩阵的行从上到下排列；跳过空成员，宽度不匹配时中止。

## net

[net.md](net.zh-CN.md)

- `effect net.tcp_connect(host: String, port: Int) -> TcpStream` — 连接到 host:port 的流；被拒绝则 err。
- `effect net.tcp_is_open(stream: TcpStream) -> Bool` — io.close 后为 false；不反映对端关闭。
- `effect net.tcp_read_timeout(stream: TcpStream, len: Int, timeout_ms: Int) -> Bytes` — 在 timeout_ms（> 0）内最多 len 字节；随后恢复流原来的超时。
- `effect net.tcp_set_timeout(stream: TcpStream, timeout_ms: Int) -> Unit` — 设置读写超时；<= 0 清除。
- `effect net.tcp_available(stream: TcpStream) -> Int` — 当前可读字节数，上限 65536；没有则 0。
- `effect net.tcp_listen(host: String, port: Int) -> TcpListener` — 绑定到 host:port 的监听器；已使用则 err，端口 0 由 OS 选择。
- `effect net.tcp_local_port(listener: TcpListener) -> Int` — 监听器绑定的端口，即在端口 0 监听时 OS 选择的值。
- `effect net.tcp_accept(listener: TcpListener) -> TcpStream` — 阻塞等待下一个客户端，返回其流。
- `effect net.udp_bind(host: String, port: Int) -> UdpSocket` — 绑定到 host:port 的 UDP 套接字，端口 0 由 OS 选择。
- `effect net.udp_local_port(sock: UdpSocket) -> Int` — sock 绑定的端口。
- `effect net.udp_send_to(sock: UdpSocket, data: Bytes, host: String, port: Int) -> Unit` — 将 data 作为一个数据报发往 host:port。
- `effect net.udp_recv_from(sock: UdpSocket, max: Int) -> UdpPacket` — 阻塞等待下一个数据报，返回最多 max 字节及发送方。
- `effect net.lookup_host(host: String) -> List[IpAddr]` — host 解析出的地址，每个一次；没有则 err。
- `net.parse_ip(s: String) -> Result[IpAddr, String]` — s 表示的 IP 地址：点分 IPv4 或 IPv6（允许 `::` 和 IPv4 尾部）。
- `net.ip_to_string(ip: IpAddr) -> String` — ip 的文本：点分 IPv4 或 RFC 5952 短 IPv6 形式。
- `net.is_loopback(ip: IpAddr) -> Bool` — 127.0.0.0/8 和 ::1 时为 true。

## option

[option.md](option.zh-CN.md)

- `option.map(o: Option[A], f: (A) -> B) -> Option[B]` — some(v) 得到 some(f(v))；none 保持 none。
- `option.flat_map(o: Option[A], f: (A) -> Option[B]) -> Option[B]` — some(v) 得到 f(v)；none 保持 none。
- `option.flatten(o: Option[Option[A]]) -> Option[A]` — some(inner) 的内部 option；none 得到 none。
- `option.unwrap_or(o: Option[A], default: A) -> A   (deprecated — use ??)` — some(v) 得到 v，否则为 default（总是求值）。
- `option.unwrap_or_else(o: Option[A], f: () -> A) -> A` — some(v) 得到 v，否则为 f()；仅在 none 时运行 f。
- `option.is_some(o: Option[A]) -> Bool` — some(_) 为 true，none 为 false。
- `option.is_none(o: Option[A]) -> Bool` — none 为 true，some(_) 为 false。
- `option.to_result(o: Option[A], e: E) -> Result[A, E]` — some(v) 得到 ok(v)，none 得到 err(e)。
- `option.filter(o: Option[A], f: (A) -> Bool) -> Option[A]` — 为 some(v) 且 f(v) 成立时返回 o，否则为 none。
- `option.zip(a: Option[A], b: Option[B]) -> Option[(A, B)]` — 两者均为 some 时得到 some((x, y))，否则为 none。
- `option.or_else(o: Option[A], f: () -> Option[A]) -> Option[A]` — 为 some 时返回 o，否则为 f()；仅在 none 时运行 f。
- `option.to_list(o: Option[A]) -> List[A]` — some(v) 得到 [v]，none 得到 []。
- `option.collect(xs: List[Option[A]]) -> Option[List[A]]` — 没有元素为 none 时得到 some(values)，否则为 none。
- `option.collect_map(xs: List[A], f: (A) -> Option[B]) -> Option[List[B]]` — 包含所有 f(x) 的 some，或 none；对每个 x 都运行 f。

## path

[path.md](path.zh-CN.md)

- `path.join(base: String, child: String) -> String` — base/child；绝对 child 替换 base。
- `path.join_within(base: String, child: String) -> Result[String, String]` — base/child；child 可能离开 base 时为 err。
- `path.dirname(path: String) -> String` — 去掉最后片段的 path；没有斜杠时为 ""。
- `path.basename(path: String) -> String` — 最后一个非空片段；/ 或空路径为 ""。
- `path.extension(path: String) -> Option[String]` — 基本文件名最后一个点之后的内容；没有点或为点文件时为 none。
- `path.is_absolute(path: String) -> Bool` — path 以 / 开头时为 true。
- `path.stem(path: String) -> String` — 基本文件名去掉最后扩展名；点文件不变。
- `path.normalize(path: String) -> String` — 词法解析 . 和 ..；"" 变为 "."。
- `path.components(path: String) -> List[String]` — normalize(path) 的片段，绝对路径以 "/" 开头；"" 或 "." 得到 ["."]。
- `path.relative_to(path: String, base: String) -> Option[String]` — path 相对 base 的词法路径；相等为 "."，不在 base 下则为 none。
- `path.with_extension(path: String, ext: String) -> String` — 将 path 的最后扩展名替换为 ext；"" 表示移除；没有文件名则不变。

## process

[process.md](process.zh-CN.md)

- `effect process.exec(cmd: String, args: List[String]) -> String` — 退出码 0 时为 stdout，否则为含 stderr 文本的 err。
- `effect process.output(c: ProcessCommand) -> ProcessStatus` — 运行 c 直至结束；任何退出码都是 ok，无法启动或超过截止时间则 err。
- `effect process.run(c: ProcessCommand) -> Int` — 在本终端运行 c（未设置 c.stdin 时包括 stdin）；返回退出码，因信号结束时为 -1。
- `effect process.start(c: ProcessCommand) -> ProcessChild` — 在独立进程组启动 c 并立即返回；捕获输出。
- `effect process.poll(c: ProcessChild) -> Option[Result[ProcessStatus, String]]` — 永不阻塞：运行时为 none，之后为结果（超时则 err）。
- `effect process.wait(c: ProcessChild) -> ProcessStatus` — 阻塞直到 c 结束，返回其结果。
- `effect process.stop(c: ProcessChild, grace_ms: Int) -> ProcessStatus` — 向 c 的组发送 SIGTERM，给予 grace_ms 后 SIGKILL；结束后再调用结果相同。
- `effect process.read_new_stdout(c: ProcessChild) -> String` — c 自上次读取后写入 stdout 的内容；不阻塞。
- `effect process.read_new_stderr(c: ProcessChild) -> String` — c 自上次读取后写入 stderr 的内容；不阻塞。
- `effect process.child_pid(c: ProcessChild) -> Int` — c 的 OS 进程 ID，用于 process.kill。
- `effect process.kill(pid: Int, signal: Int) -> Unit` — 通过 kill(2) 向 pid（> 0）发送 signal（0..=31）；err 指明原因。
- `effect process.exit(code: Int) -> Never` — 以 0..=255 的 code 结束；其他值中止并以 1 退出。
- `effect process.pid() -> Int` — 本程序的 OS 进程 ID。

## random

[random.md](random.zh-CN.md)

- `effect random.int(min: Int, max: Int) -> Int` — [min, max] 内的随机 Int；max <= min 时为 min。不可用于秘密，请使用 random.bytes。
- `effect random.float() -> Float` — [0, 1) 内的随机 Float。不可用于秘密，请使用 random.bytes。
- `effect random.choice(xs: List[A]) -> Option[A]` — xs 的随机元素；空时为 none。不可用于秘密，请使用 random.bytes。
- `effect random.shuffle(xs: List[A]) -> List[A]` — xs 的随机排列，作为新列表返回。不可用于秘密，请使用 random.bytes。
- `effect random.bytes(n: Int) -> Bytes` — 来自 OS 密码学随机源（CSPRNG）的 n 字节；n < 0 则 abort。
- `random.seeded(seed: Int) -> Rng` — 随机流由 seed 固定的 generator。
- `random.next_int(rng: Rng, min: Int, max: Int) -> (Int, Rng)` — [min, max]（含两端）内的 Int 与下一状态；max <= min 时返回 min，状态不变。
- `random.next_float(rng: Rng) -> (Float, Rng)` — 由高 53 位生成的 [0, 1) 内 Float 与下一状态。
- `random.next_choice(rng: Rng, xs: List[A]) -> (Option[A], Rng)` — 均匀选择的元素及下一状态（xs 为空时为 none，状态不变）。
- `random.next_shuffle(rng: Rng, xs: List[A]) -> (List[A], Rng)` — xs 的均匀排列（Fisher-Yates）与下一状态。

## regex

[regex.md](regex.zh-CN.md)

- `regex.new(pattern: String) -> Regex` — `pattern` 的 Regex；拒绝时以 `Error: invalid regex: <reason>` 中止。
- `regex.compile(pattern: String) -> Result[Regex, String]` — `pattern` 的 Regex；拒绝时为 err(reason)。
- `regex.is_match(re: Regex, s: String) -> Bool` — re 匹配 s 的某处。
- `regex.is_full_match(re: Regex, s: String) -> Bool` — re 匹配整个 s。
- `regex.full_match(re: Regex, s: String) -> Bool   (deprecated — use regex.is_full_match)` — re 匹配整个 s（已更名为 regex.is_full_match）。
- `regex.find(re: Regex, s: String) -> Option[String]` — re 在 s 中最左侧的匹配，没有则 none。
- `regex.find_all(re: Regex, s: String) -> List[String]` — 按顺序返回所有不重叠匹配。
- `regex.index_of(re: Regex, s: String) -> Option[Int]` — 最左侧匹配起点的码点索引，没有则 none。
- `regex.replace(re: Regex, s: String, rep: String) -> String` — 用字面文本 rep 替换所有匹配（不解释 `$1`）。
- `regex.replace_first(re: Regex, s: String, rep: String) -> String` — 用字面文本 rep 替换第一个匹配。
- `regex.replace_with(re: Regex, s: String, f: (List[Option[String]]) -> String) -> String` — 用 f([whole, group1, ...]) 替换所有匹配。
- `regex.replace_first_with(re: Regex, s: String, f: (List[Option[String]]) -> String) -> String` — 用 f([whole, group1, ...]) 替换第一个匹配。
- `regex.split(re: Regex, s: String) -> List[String]` — 匹配之间的文本；无匹配则 [s]。
- `regex.captures(re: Regex, s: String) -> Option[List[Option[String]]]` — 第一个匹配的 [whole, group1, ...]，没有则 none；未参与的组为 none。
- `regex.captures_all(re: Regex, s: String) -> List[List[Option[String]]]` — 按顺序返回每个匹配的 [whole, group1, ...]。
- `regex.escape(s: String) -> String` — 转义 s 的所有正则元字符，使其匹配自身。

## result

[result.md](result.zh-CN.md)

- `result.map(r: Result[A, E], f: (A) -> B) -> Result[B, E]` — ok(v) 得到 ok(f(v))；err 原样传递。
- `result.map_err(r: Result[A, E], f: (E) -> F) -> Result[A, F]` — err(e) 得到 err(f(e))；ok 原样传递。
- `result.flat_map(r: Result[A, E], f: (A) -> Result[B, E]) -> Result[B, E]` — ok(v) 得到 f(v)；err 原样传递。
- `result.unwrap_or(r: Result[A, E], default: A) -> A   (deprecated — use ??)` — ok(v) 得到 v，否则为 default（总是求值）。
- `result.unwrap_or_else(r: Result[A, E], f: (E) -> A) -> A` — ok(v) 得到 v，否则为 f(e)；仅在 err 时运行 f。
- `result.is_ok(r: Result[A, E]) -> Bool` — ok(_) 为 true，err(_) 为 false。
- `result.is_err(r: Result[A, E]) -> Bool` — err(_) 为 true，ok(_) 为 false。
- `result.to_option(r: Result[A, E]) -> Option[A]` — ok(v) 得到 some(v)，err 得到 none，丢弃 e。
- `result.to_err_option(r: Result[A, E]) -> Option[E]` — err(e) 得到 some(e)，ok 得到 none。
- `result.partition(xs: List[Result[A, E]]) -> (List[A], List[E])` — (ok 值，err 值)，各自保持列表顺序。
- `result.flatten(r: Result[Result[A, E], E]) -> Result[A, E]` — ok(inner) 的内部 result；保留外层 err。
- `result.to_list(r: Result[A, E]) -> List[A]` — ok(v) 得到 [v]，err 得到 []。
- `result.zip(a: Result[A, E], b: Result[B, E]) -> Result[(A, B), E]` — 两者均为 ok 时得到 ok((x, y))，否则为第一个 err。
- `result.or_else(r: Result[A, E], f: (E) -> Result[A, F]) -> Result[A, F]` — 为 ok 时返回 r，否则为 f(e)；仅在 err 时运行 f。
- `result.filter(r: Result[A, E], f: (A) -> Bool, err_val: E) -> Result[A, E]` — 若对 ok 的 f 判定失败，则 err(err_val)；保留 err。

## secret

[secret.md](secret.zh-CN.md)

- `secret.new(x: A) -> Secret[A]` — 包装 x，使每个渲染器都将其打印为 `Secret(***)`。
- `secret.reveal(s: Secret[A]) -> A` — s 包装的值。

## set

[set.md](set.zh-CN.md)

- `set.new() -> Set[A]` — 空 Set[A]。
- `set.from_list(xs: List[A]) -> Set[A]` — xs 中不同的元素，按首次出现顺序。
- `set.insert(s: Set[A], x: A) -> Set[A]` — 追加 x 的副本；已存在则不变。
- `set.remove(s: Set[A], x: A) -> Set[A]` — 移除 x 的副本；不存在则不变。
- `set.contains(s: Set[A], x: A) -> Bool` — x 是成员时为 true。
- `set.len(s: Set[A]) -> Int` — 成员数量；空 set 为 0。
- `set.is_empty(s: Set[A]) -> Bool` — 当且仅当 s 没有成员时为 true。
- `set.to_list(s: Set[A]) -> List[A]` — 按插入顺序返回成员。
- `set.union(a: Set[A], b: Set[A]) -> Set[A]` — a 的成员，随后是 b 的新增成员。
- `set.intersection(a: Set[A], b: Set[A]) -> Set[A]` — a 中也在 b 中的成员，按 a 的顺序。
- `set.difference(a: Set[A], b: Set[A]) -> Set[A]` — a 中不在 b 中的成员，按 a 的顺序。
- `set.symmetric_difference(a: Set[A], b: Set[A]) -> Set[A]` — 恰好属于一个 set：先仅属于 a 的，再仅属于 b 的。
- `set.is_subset(a: Set[A], b: Set[A]) -> Bool` — a 的全部成员都在 b 中则为 true；空 a 也为 true。
- `set.is_disjoint(a: Set[A], b: Set[A]) -> Bool` — a 与 b 没有共同成员时为 true。
- `set.filter(s: Set[A], f: (A) -> Bool) -> Set[A]` — f 成立的成员，保持顺序。
- `set.map(s: Set[A], f: (A) -> B) -> Set[B]` — f(x) 的 set；f 的结果碰撞时会缩小。
- `set.fold(s: Set[A], init: B, f: (B, A) -> B) -> B` — 按插入顺序折叠 f(acc, x)。
- `set.any(s: Set[A], f: (A) -> Bool) -> Bool` — 存在 f 成立的成员时为 true；空时为 false。
- `set.all(s: Set[A], f: (A) -> Bool) -> Bool` — 每个成员的 f 都成立时为 true；空时为 true。
- `set.count(s: Set[A], f: (A) -> Bool) -> Int` — f 成立的成员数量。
- `set.find(s: Set[A], f: (A) -> Bool) -> Option[A]` — 按插入顺序首个 f 成立的成员，或 none。
- `set.clear(mut s: Set[A]) -> Unit` — 原地移除 s 的每个成员。
- `set.enumerate(s: Set[A]) -> List[(Int, A)]` — 按插入顺序返回 (index, member) 对，index 从 0 开始。
- `set.zip(s: Set[A], ys: List[B]) -> List[(A, B)]` — 按位置将成员与 ys 配对；在较短者处停止。
- `set.zip_with(s: Set[A], ys: List[B], f: (A, B) -> C) -> List[C]` — 对 (member, ys 元素) 对应用 f；在较短者处停止。
- `set.filter_map(s: Set[A], f: (A) -> Option[B]) -> List[B]` — f 返回的 some，按插入顺序。
- `set.find_map(s: Set[A], f: (A) -> Option[B]) -> Option[B]` — f 返回的首个 some，或 none；命中后不再调用 f。
- `set.reduce(s: Set[A], f: (A, A) -> A) -> Option[A]` — 以首个成员为初始值左折叠；空时为 none。
- `set.scan(s: Set[A], init: B, f: (B, A) -> B) -> List[B]` — 按插入顺序折叠的中间值，不包括 init。
- `set.partition(s: Set[A], f: (A) -> Bool) -> (List[A], List[A])` — (f 成立的成员，其余成员)，保持插入顺序。
- `set.group_by(s: Set[A], f: (A) -> B) -> Map[B, List[A]]` — 键 f(x) → 成员组；键按首次出现顺序。
- `set.sort(s: Set[A]) -> List[A]` — 升序成员；Float 使用 IEEE totalOrder。
- `set.sort_by(s: Set[A], f: (A) -> B) -> List[A]` — 按键 f(x) 稳定排序成员；每个成员调用一次 f。
- `set.sort_with(s: Set[A], cmp: (A, A) -> Int) -> List[A]` — 按 cmp 稳定排序成员；负值将 a 放在前面。
- `set.min(s: Set[A]) -> Option[A]` — 最小成员，空时为 none。
- `set.max(s: Set[A]) -> Option[A]` — 最大成员，空时为 none。
- `set.min_by(s: Set[A], f: (A) -> K) -> Option[A]` — 键 f(x) 最小的成员；并列时取首个。
- `set.max_by(s: Set[A], f: (A) -> K) -> Option[A]` — 键 f(x) 最大的成员；并列时取首个。
- `set.min_with(s: Set[A], cmp: (A, A) -> Int) -> Option[A]` — 按 cmp 最小的成员；并列时取首个。
- `set.max_with(s: Set[A], cmp: (A, A) -> Int) -> Option[A]` — 按 cmp 最大的成员；并列时取首个。
- `set.sum(s: Set[A]) -> A` — 元素之和；空时为 0；整数溢出时回绕。
- `set.product(s: Set[A]) -> A` — 元素之积；空时为 1；整数溢出时回绕。

## string

[string.md](string.zh-CN.md)

- `string.trim(s: String) -> String` — 移除 s 开头/结尾的空白。
- `string.split(s: String, sep: String) -> List[String]` — sep 周围的片段；保留空片段。
- `string.split_once(s: String, sep: String) -> Option[(String, String)]` — 在 `sep` 首次出现处拆分为 `(before, after)`；当且仅当不存在 `sep` 时为 none。
- `string.len(s: String) -> Int` — 码点数（不是字节数）。
- `string.length(s: String) -> Int   (deprecated — use string.len)` — string.len 的别名。
- `string.contains(s: String, needle: String) -> Bool` — needle 是否出现在 s 中；空 needle 为 true。
- `string.is_equal_constant_time(a: String, b: String) -> Bool` — a 与 b 含相同 UTF-8 字节时为 true，时间仅取决于长度：不在首个不同字节处提前退出。用于比较令牌或签名头；`==` 在首处差异即返回。长度不同时立即为 false（长度是公开信息）。
- `string.starts_with(s: String, prefix: String) -> Bool` — s 以 prefix 开头；"" 始终匹配。
- `string.ends_with(s: String, suffix: String) -> Bool` — s 以 suffix 结尾；"" 始终匹配。
- `string.slice(s: String, start: Int, end: Int) -> String` — 码点区间 [start, end)，限制在 s 范围内。
- `string.byte_slice(s: String, start: Int, end: Int) -> Option[String]` — 字节区间 [start, end)；无效或落在字符中间则为 none。
- `string.pad_start(s: String, n: Int, fill: String) -> String` — 用 fill 左填充至 n 字符；不截断。
- `string.capitalize(s: String) -> String` — 按 to_upper 的规则大写化首字符，其余不变。
- `string.to_upper(s: String) -> String` — Unicode 大写；不将非 ASCII 折叠为 ASCII；不用于身份比较。
- `string.to_lower(s: String) -> String` — Unicode 小写；不将非 ASCII 折叠为 ASCII；不用于身份比较。
- `string.replace(s: String, from: String, to: String) -> String` — 将所有 from 替换为 to。
- `string.get(s: String, i: Int) -> Option[String]` — 索引 i 处的码点，越界为 none。
- `string.lines(s: String) -> List[String]` — 按 \n 或 \r\n 拆分；去除行尾。
- `string.to_list(s: String) -> List[String]` — 将每个码点表示为单字符字符串。
- `string.index_of(s: String, needle: String) -> Option[Int]` — 首次 needle 的码点索引，或 none。
- `string.repeat(s: String, n: Int) -> String` — s 重复 n 次；n <= 0 时为空。
- `string.is_digit(s: String) -> Bool` — 全部为 ASCII 0-9；空 s 为 false。
- `string.is_alpha(s: String) -> Bool` — 全部为 Unicode 字母；空 s 为 false。
- `string.is_alphanumeric(s: String) -> Bool` — 全部为字母/数字；空 s 为 false。
- `string.is_whitespace(s: String) -> Bool` — 全部为空白；空 s 为 true。
- `string.is_upper(s: String) -> Bool` — 存在字母且全部大写；忽略其他字符。
- `string.is_lower(s: String) -> Bool` — 存在字母且全部小写；忽略其他字符。
- `string.to_codepoint(s: String) -> Option[Int]` — s 恰有一个码点时的标量值；否则为 none。
- `string.from_codepoint(n: Int) -> Option[String]` — n 对应的单字符字符串；n 不是 Unicode 标量值时为 none。
- `string.pad_end(s: String, n: Int, fill: String) -> String` — 用 fill 右填充至 n 字符；不截断。
- `string.trim_start(s: String) -> String` — 移除 s 开头的空白。
- `string.trim_end(s: String) -> String` — 移除 s 结尾的空白。
- `string.count_matches(s: String, needle: String) -> Int` — 不重叠的 needle 数量；空 needle 为 len+1。
- `string.count(s: String, f: (String) -> Bool) -> Int` — f 为 true 的字符数。
- `string.is_empty(s: String) -> Bool` — 仅 "" 为 true；" " 不为空。
- `string.reverse(s: String) -> String` — 反转码点；不考虑字素。
- `string.strip_prefix(s: String, prefix: String) -> Option[String]` — 移除 s 的 prefix，不存在则为 none。
- `string.strip_suffix(s: String, suffix: String) -> Option[String]` — 移除 s 的 suffix，不存在则为 none。
- `string.replace_first(s: String, from: String, to: String) -> String` — 若有 from，将首次出现替换为 to。
- `string.last_index_of(s: String, needle: String) -> Option[Int]` — 最后一次 needle 的码点索引，或 none。
- `string.first(s: String) -> Option[String]` — 首码点；空则为 none。
- `string.last(s: String) -> Option[String]` — 末码点；空则为 none。
- `string.take(s: String, n: Int) -> String` — 前 n 个码点；n < 0 时为 ""。
- `string.take_end(s: String, n: Int) -> String` — 后 n 个码点；n < 0 时为 ""。
- `string.drop(s: String, n: Int) -> String` — 移除 s 前 n 个码点；n < 0 时为 s。
- `string.drop_end(s: String, n: Int) -> String` — 移除 s 后 n 个码点；n < 0 时为 s。
- `string.push(mut s: String, x: String) -> Unit` — 原地将 x 追加到 s。
- `string.pop(mut s: String) -> Option[String]` — 移除并返回末字符；s 为空则为 none。
- `string.clear(mut s: String) -> Unit` — 原地清空 s。
- `string.compare(a: String, b: String) -> Int` — 按码点顺序，a 在 b 前为负，相等为 0，在后为正。
- `string.byte_len(s: String) -> Int` — UTF-8 字节数（string.len 计算码点）。
- `string.to_utf16(s: String) -> List[Int]` — UTF-16 代码单元；超过 U+FFFF 的码点为代理项对。
- `string.from_utf16(xs: List[Int]) -> Result[String, String]` — 从 UTF-16 代码单元生成字符串；未配对代理项或超出 0..65535 的单元导致 err。
- `string.filter(s: String, f: (String) -> Bool) -> String` — s 中 f 为 true 的字符。
- `string.fold(s: String, init: B, f: (B, String) -> B) -> B` — 用 f 从左到右折叠 s 的字符。
- `string.insert(s: String, i: Int, x: String) -> String` — 在字符 i 前插入 x 后的 s；i 超出 0..=len 则 abort。
- `string.remove_at(s: String, i: Int) -> String` — 移除字符 i 后的 s；i 超出 0..len 则 abort。
- `string.new() -> String` — 空字符串。
- `string.from_list(xs: List[String]) -> String` — 将 xs 的字符串直接连接，不插入内容。
- `string.enumerate(s: String) -> List[(Int, String)]` — 从 0 开始的 (index, character) 对。
- `string.zip(s: String, ys: List[B]) -> List[(String, B)]` — 字符与 ys 配对；以较短者为限。
- `string.zip_with(s: String, ys: List[B], f: (String, B) -> C) -> List[C]` — 对字符与 ys 的配对应用 f；以较短者为限。
- `string.filter_map(s: String, f: (String) -> Option[B]) -> List[B]` — 按顺序返回对各字符应用 f 得到的 some 结果。
- `string.find_map(s: String, f: (String) -> Option[B]) -> Option[B]` — 对各字符应用 f 的首个 some 结果，或 none。
- `string.reduce(s: String, f: (String, String) -> String) -> Option[String]` — 从首字符开始用 f 折叠；s 为空则为 none。
- `string.scan(s: String, init: B, f: (B, String) -> B) -> List[B]` — 对字符的逐步 fold 结果，不含 init。
- `string.partition(s: String, f: (String) -> Bool) -> (List[String], List[String])` — (f 为 true 的字符，其余字符)，各自保持顺序。
- `string.group_by(s: String, f: (String) -> B) -> Map[B, List[String]]` — 按 f 的键分组字符，各组保持顺序。
- `string.sort(s: String) -> List[String]` — 按码点顺序排列的字符。
- `string.sort_by(s: String, f: (String) -> B) -> List[String]` — 按 f 的键排序的字符；相同键保持顺序。
- `string.sort_with(s: String, cmp: (String, String) -> Int) -> List[String]` — 按 cmp 排序的字符（负值表示第一参数在前）；稳定。
- `string.min(s: String) -> Option[String]` — 码点顺序最小的字符；空则为 none。
- `string.max(s: String) -> Option[String]` — 码点顺序最大的字符；空则为 none。
- `string.min_by(s: String, f: (String) -> K) -> Option[String]` — 具有最小键的首个字符；空则为 none。
- `string.max_by(s: String, f: (String) -> K) -> Option[String]` — 具有最大键的末个字符；空则为 none。
- `string.min_with(s: String, cmp: (String, String) -> Int) -> Option[String]` — 按 cmp 最小的首个字符；空则为 none。
- `string.max_with(s: String, cmp: (String, String) -> Int) -> Option[String]` — 按 cmp 最大的末个字符；空则为 none。
- `string.find_index(s: String, f: (String) -> Bool) -> Option[Int]` — f 为 true 的首字符索引，或 none。
- `string.take_while(s: String, f: (String) -> Bool) -> String` — 所有字符均满足 f 的最长前缀。
- `string.drop_while(s: String, f: (String) -> Bool) -> String` — 从 s 移除所有字符均满足 f 的最长前缀。
- `string.chunk(s: String, n: Int) -> List[String]` — 每段 n 个字符，末段可更短；n <= 0 则 abort。
- `string.window(s: String, n: Int) -> List[String]` — 所有连续 n 字符的窗口；n > len 时为 []；n <= 0 则 abort。
- `string.split_at(s: String, n: Int) -> (String, String)` — (前 n 个字符，其余部分)；n 超出末尾为 (s, "")，n < 0 为 ("", s)。
- `string.dedup(s: String) -> String` — 将 s 中每段相同相邻字符合并为一个。
- `string.intersperse(s: String, sep: String) -> String` — 在 s 的每对相邻字符间插入 sep。
- `string.unique(s: String) -> String` — 按顺序保留每个字符的首次出现。
- `string.unique_by(s: String, f: (String) -> K) -> String` — 按顺序保留 f 给出的每个键的首个字符。

## testing

[testing.md](testing.zh-CN.md)

- `testing.assert_aborts(expected: String, f: () -> A) -> Unit` — 若单独在子执行环境中运行的 f 未以包含 expected 的文本中止，则失败。
- `testing.assert_contains(haystack: String, needle: String) -> Unit` — 若 haystack 不包含 needle，则失败；空字符串 "" 通过。
- `testing.assert_approx(a: Float, b: Float, tolerance: Float) -> Unit` — 若不满足 abs(a - b) < tolerance（严格小于），则失败。
- `testing.assert_gt(a: A, b: A) -> Unit` — 若不满足 a > b，则失败；相等也失败。
- `testing.assert_lt(a: A, b: A) -> Unit` — 若不满足 a < b，则失败；相等也失败。
- `testing.assert_some(o: Option[A]) -> Unit` — o 为 none 时失败。
- `testing.assert_ok(r: Result[A, B]) -> Unit` — r 为 err 时失败。
- `testing.assert_none(o: Option[A]) -> Unit` — o 为 some 时失败。
- `testing.assert_err(r: Result[A, B]) -> Unit` — r 为 ok 时失败。
- `testing.assert_snapshot(actual: String, expected: String) -> Unit` — 若 actual != expected 则失败；--update-snapshots 会重写。

## uint16

[uint16.md](uint16.zh-CN.md)

- `uint16.to_string(x: UInt16) -> String` — 十进制数字，绝不带符号。
- `uint16.min_value() -> UInt16` — 最小 UInt16：0。
- `uint16.max_value() -> UInt16` — 最大 UInt16：65535。
- `uint16.compare(a: UInt16, b: UInt16) -> Int` — a 小于、等于、大于 b 时分别为 -1、0、1。
- `uint16.min(a: UInt16, b: UInt16) -> UInt16` — a 和 b 的较小者。
- `uint16.max(a: UInt16, b: UInt16) -> UInt16` — a 和 b 的较大者。
- `uint16.clamp(x: UInt16, lo: UInt16, hi: UInt16) -> UInt16` — 将 x 限制在 lo..hi；lo > hi 时 abort。
- `uint16.floor_div(a: UInt16, b: UInt16) -> UInt16` — 商；对无符号类型与 a / b 相同；b == 0 时 abort。
- `uint16.modulo(a: UInt16, b: UInt16) -> UInt16` — 余数；对无符号类型与 a % b 相同；b == 0 时 abort。
- `uint16.pow(base: UInt16, exp: Int) -> UInt16` — base 的 exp 次幂，像 * 一样回绕到 16 位；exp < 0 时 abort。
- `uint16.is_even(x: UInt16) -> Bool` — x 是 2 的倍数时为 true。
- `uint16.is_odd(x: UInt16) -> Bool` — x 不是 2 的倍数时为 true。
- `uint16.band(a: UInt16, b: UInt16) -> UInt16` — 按位 AND。
- `uint16.bor(a: UInt16, b: UInt16) -> UInt16` — 按位 OR。
- `uint16.bxor(a: UInt16, b: UInt16) -> UInt16` — 按位 XOR。
- `uint16.bnot(x: UInt16) -> UInt16` — 全部 16 位的按位取反。
- `uint16.bshl(a: UInt16, n: Int) -> UInt16` — a 左移 n 位；n 对 16 取模。
- `uint16.bshr(a: UInt16, n: Int) -> UInt16` — 零填充右移；n 对 16 取模。
- `uint16.count_leading_zeros(x: UInt16) -> Int` — 在 16 位中最高置位位之上的零位数；0 时为 16。
- `uint16.count_trailing_zeros(x: UInt16) -> Int` — 最低置位位之下的零位数；0 时为 16。
- `uint16.count_ones(x: UInt16) -> Int` — 16 位中置位的数量。
- `uint16.byte_swap(x: UInt16) -> UInt16` — 反转 2 字节顺序（翻转字节序）。
- `uint16.wrap_add(a: UInt16, b: UInt16) -> UInt16` — a + b 回绕到 16 位。
- `uint16.wrap_sub(a: UInt16, b: UInt16) -> UInt16` — a - b 回绕到 16 位。
- `uint16.wrap_mul(a: UInt16, b: UInt16) -> UInt16` — a * b 回绕到 16 位。
- `uint16.rotate_left(a: UInt16, n: Int) -> UInt16` — 将 a 的 16 位向左旋转 n；n 对 16 取模。
- `uint16.rotate_right(a: UInt16, n: Int) -> UInt16` — 将 a 的 16 位向右旋转 n；n 对 16 取模。
- `uint16.bit_length(x: UInt16) -> Int` — 容纳 x 所需的位数；0 时为 0。
- `uint16.parse(s: String) -> Result[UInt16, String]` — 去除 s 两侧 ASCII 空白后解析十进制 UInt16；无效内容、- 符号或超过 65535 为 err。
- `uint16.to_string_radix(x: UInt16, radix: Int) -> String` — 用 radix 进制的小写数字表示 x；无符号；radix 不在 2..36 时 abort。
- `uint16.parse_radix(s: String, radix: Int) -> Result[UInt16, String]` — 去除 s 两侧空白后按 radix 解析 UInt16，语法同 int.parse_radix；无效内容、- 符号或超过 65535 为 err。
- `uint16.from(x: S) -> UInt16` — x 向零舍入为 UInt16；NaN、无穷大或超出 UInt16 范围时 abort。
- `uint16.from_checked(x: S) -> Option[UInt16]` — UInt16 能精确容纳 x 时为 some(x as UInt16)，否则（小数部分、NaN、越界）为 none。
- `uint16.from_saturating(x: S) -> UInt16` — x 向零舍入为 UInt16，限制在 UInt16 范围；NaN 为 0。
- `uint16.from_wrapping(x: S) -> UInt16` — 将整数 x 的低 16 位作为 UInt16（二进制补码）。

## uint32

[uint32.md](uint32.zh-CN.md)

- `uint32.to_string(x: UInt32) -> String` — 十进制数字，绝不带符号。
- `uint32.min_value() -> UInt32` — 最小 UInt32：0。
- `uint32.max_value() -> UInt32` — 最大 UInt32：4294967295。
- `uint32.compare(a: UInt32, b: UInt32) -> Int` — a 小于、等于、大于 b 时分别为 -1、0、1。
- `uint32.min(a: UInt32, b: UInt32) -> UInt32` — a 和 b 的较小者。
- `uint32.max(a: UInt32, b: UInt32) -> UInt32` — a 和 b 的较大者。
- `uint32.clamp(x: UInt32, lo: UInt32, hi: UInt32) -> UInt32` — 将 x 限制在 lo..hi；lo > hi 时 abort。
- `uint32.floor_div(a: UInt32, b: UInt32) -> UInt32` — 商；对无符号类型与 a / b 相同；b == 0 时 abort。
- `uint32.modulo(a: UInt32, b: UInt32) -> UInt32` — 余数；对无符号类型与 a % b 相同；b == 0 时 abort。
- `uint32.pow(base: UInt32, exp: Int) -> UInt32` — base 的 exp 次幂，像 * 一样回绕到 32 位；exp < 0 时 abort。
- `uint32.is_even(x: UInt32) -> Bool` — x 是 2 的倍数时为 true。
- `uint32.is_odd(x: UInt32) -> Bool` — x 不是 2 的倍数时为 true。
- `uint32.band(a: UInt32, b: UInt32) -> UInt32` — 按位 AND。
- `uint32.bor(a: UInt32, b: UInt32) -> UInt32` — 按位 OR。
- `uint32.bxor(a: UInt32, b: UInt32) -> UInt32` — 按位 XOR。
- `uint32.bnot(x: UInt32) -> UInt32` — 全部 32 位的按位取反。
- `uint32.bshl(a: UInt32, n: Int) -> UInt32` — a 左移 n 位；n 对 32 取模。
- `uint32.bshr(a: UInt32, n: Int) -> UInt32` — 零填充右移；n 对 32 取模。
- `uint32.count_leading_zeros(x: UInt32) -> Int` — 在 32 位中最高置位位之上的零位数；0 时为 32。
- `uint32.count_trailing_zeros(x: UInt32) -> Int` — 最低置位位之下的零位数；0 时为 32。
- `uint32.count_ones(x: UInt32) -> Int` — 32 位中置位的数量。
- `uint32.byte_swap(x: UInt32) -> UInt32` — 反转 4 字节顺序（翻转字节序）。
- `uint32.wrap_add(a: UInt32, b: UInt32) -> UInt32` — a + b 回绕到 32 位。
- `uint32.wrap_sub(a: UInt32, b: UInt32) -> UInt32` — a - b 回绕到 32 位。
- `uint32.wrap_mul(a: UInt32, b: UInt32) -> UInt32` — a * b 回绕到 32 位。
- `uint32.rotate_left(a: UInt32, n: Int) -> UInt32` — 将 a 的 32 位向左旋转 n；n 对 32 取模。
- `uint32.rotate_right(a: UInt32, n: Int) -> UInt32` — 将 a 的 32 位向右旋转 n；n 对 32 取模。
- `uint32.bit_length(x: UInt32) -> Int` — 容纳 x 所需的位数；0 时为 0。
- `uint32.parse(s: String) -> Result[UInt32, String]` — 去除 s 两侧 ASCII 空白后解析十进制 UInt32；无效内容、- 符号或超过 4294967295 为 err。
- `uint32.to_string_radix(x: UInt32, radix: Int) -> String` — 用 radix 进制的小写数字表示 x；无符号；radix 不在 2..36 时 abort。
- `uint32.parse_radix(s: String, radix: Int) -> Result[UInt32, String]` — 去除 s 两侧空白后按 radix 解析 UInt32，语法同 int.parse_radix；无效内容、- 符号或超过 4294967295 为 err。
- `uint32.from(x: S) -> UInt32` — x 向零舍入为 UInt32；NaN、无穷大或超出 UInt32 范围时 abort。
- `uint32.from_checked(x: S) -> Option[UInt32]` — UInt32 能精确容纳 x 时为 some(x as UInt32)，否则（小数部分、NaN、越界）为 none。
- `uint32.from_saturating(x: S) -> UInt32` — x 向零舍入为 UInt32，限制在 UInt32 范围；NaN 为 0。
- `uint32.from_wrapping(x: S) -> UInt32` — 将整数 x 的低 32 位作为 UInt32（二进制补码）。

## uint64

[uint64.md](uint64.zh-CN.md)

- `uint64.to_string(x: UInt64) -> String` — 十进制数字，无符号；精确至 2^64-1。
- `uint64.min_value() -> UInt64` — 最小 UInt64：0。
- `uint64.max_value() -> UInt64` — 最大 UInt64：2^64-1。
- `uint64.compare(a: UInt64, b: UInt64) -> Int` — a 小于、等于、大于 b 时分别为 -1、0、1。
- `uint64.min(a: UInt64, b: UInt64) -> UInt64` — a 和 b 的较小者。
- `uint64.max(a: UInt64, b: UInt64) -> UInt64` — a 和 b 的较大者。
- `uint64.clamp(x: UInt64, lo: UInt64, hi: UInt64) -> UInt64` — 将 x 限制在 lo..hi；lo > hi 时 abort。
- `uint64.floor_div(a: UInt64, b: UInt64) -> UInt64` — 商；对无符号类型与 a / b 相同；b == 0 时 abort。
- `uint64.modulo(a: UInt64, b: UInt64) -> UInt64` — 余数；对无符号类型与 a % b 相同；b == 0 时 abort。
- `uint64.pow(base: UInt64, exp: Int) -> UInt64` — base 的 exp 次幂，像 * 一样回绕到 64 位；exp < 0 时 abort。
- `uint64.is_even(x: UInt64) -> Bool` — x 是 2 的倍数时为 true。
- `uint64.is_odd(x: UInt64) -> Bool` — x 不是 2 的倍数时为 true。
- `uint64.band(a: UInt64, b: UInt64) -> UInt64` — 按位 AND。
- `uint64.bor(a: UInt64, b: UInt64) -> UInt64` — 按位 OR。
- `uint64.bxor(a: UInt64, b: UInt64) -> UInt64` — 按位 XOR。
- `uint64.bnot(x: UInt64) -> UInt64` — 全部 64 位的按位取反。
- `uint64.bshl(a: UInt64, n: Int) -> UInt64` — a 左移 n 位；n 对 64 取模。
- `uint64.bshr(a: UInt64, n: Int) -> UInt64` — 零填充右移；n 对 64 取模。
- `uint64.count_leading_zeros(x: UInt64) -> Int` — 在 64 位中最高置位位之上的零位数；0 时为 64。
- `uint64.count_trailing_zeros(x: UInt64) -> Int` — 最低置位位之下的零位数；0 时为 64。
- `uint64.count_ones(x: UInt64) -> Int` — 64 位中置位的数量。
- `uint64.byte_swap(x: UInt64) -> UInt64` — 反转 8 字节顺序（翻转字节序）。
- `uint64.wrap_add(a: UInt64, b: UInt64) -> UInt64` — a + b 回绕到 64 位。
- `uint64.wrap_sub(a: UInt64, b: UInt64) -> UInt64` — a - b 回绕到 64 位。
- `uint64.wrap_mul(a: UInt64, b: UInt64) -> UInt64` — a * b 回绕到 64 位。
- `uint64.rotate_left(a: UInt64, n: Int) -> UInt64` — 将 a 的 64 位向左旋转 n；n 对 64 取模。
- `uint64.rotate_right(a: UInt64, n: Int) -> UInt64` — 将 a 的 64 位向右旋转 n；n 对 64 取模。
- `uint64.bit_length(x: UInt64) -> Int` — 容纳 x 所需的位数；0 时为 0。
- `uint64.parse(s: String) -> Result[UInt64, String]` — 去除 s 两侧 ASCII 空白后解析十进制 UInt64；无效内容、- 符号或超过 2^64-1 为 err。
- `uint64.to_string_radix(x: UInt64, radix: Int) -> String` — 用 radix 进制的小写数字表示 x；无符号；精确至 2^64-1；radix 不在 2..36 时 abort。
- `uint64.parse_radix(s: String, radix: Int) -> Result[UInt64, String]` — 去除 s 两侧空白后按 radix 解析 UInt64，语法同 int.parse_radix；无效内容、- 符号或超过 2^64-1 为 err。
- `uint64.from(x: S) -> UInt64` — x 向零舍入为 UInt64；NaN、无穷大或超出 UInt64 范围时 abort。
- `uint64.from_checked(x: S) -> Option[UInt64]` — UInt64 能精确容纳 x 时为 some(x as UInt64)，否则（小数部分、NaN、越界）为 none。
- `uint64.from_saturating(x: S) -> UInt64` — x 向零舍入为 UInt64，限制在 UInt64 范围；NaN 为 0。
- `uint64.from_wrapping(x: S) -> UInt64` — 将整数 x 的低 64 位作为 UInt64（二进制补码）。

## uint8

[uint8.md](uint8.zh-CN.md)

- `uint8.to_string(x: UInt8) -> String` — 十进制数字，绝不带符号。
- `uint8.min_value() -> UInt8` — 最小 UInt8：0。
- `uint8.max_value() -> UInt8` — 最大 UInt8：255。
- `uint8.compare(a: UInt8, b: UInt8) -> Int` — a 小于、等于、大于 b 时分别为 -1、0、1。
- `uint8.min(a: UInt8, b: UInt8) -> UInt8` — a 和 b 的较小者。
- `uint8.max(a: UInt8, b: UInt8) -> UInt8` — a 和 b 的较大者。
- `uint8.clamp(x: UInt8, lo: UInt8, hi: UInt8) -> UInt8` — 将 x 限制在 lo..hi；lo > hi 时 abort。
- `uint8.floor_div(a: UInt8, b: UInt8) -> UInt8` — 商；对无符号类型与 a / b 相同；b == 0 时 abort。
- `uint8.modulo(a: UInt8, b: UInt8) -> UInt8` — 余数；对无符号类型与 a % b 相同；b == 0 时 abort。
- `uint8.pow(base: UInt8, exp: Int) -> UInt8` — base 的 exp 次幂，像 * 一样回绕到 8 位；exp < 0 时 abort。
- `uint8.is_even(x: UInt8) -> Bool` — x 是 2 的倍数时为 true。
- `uint8.is_odd(x: UInt8) -> Bool` — x 不是 2 的倍数时为 true。
- `uint8.band(a: UInt8, b: UInt8) -> UInt8` — 按位 AND。
- `uint8.bor(a: UInt8, b: UInt8) -> UInt8` — 按位 OR。
- `uint8.bxor(a: UInt8, b: UInt8) -> UInt8` — 按位 XOR。
- `uint8.bnot(x: UInt8) -> UInt8` — 全部 8 位的按位取反。
- `uint8.bshl(a: UInt8, n: Int) -> UInt8` — a 左移 n 位；n 对 8 取模。
- `uint8.bshr(a: UInt8, n: Int) -> UInt8` — 零填充右移；n 对 8 取模。
- `uint8.count_leading_zeros(x: UInt8) -> Int` — 在 8 位中最高置位位之上的零位数；0 时为 8。
- `uint8.count_trailing_zeros(x: UInt8) -> Int` — 最低置位位之下的零位数；0 时为 8。
- `uint8.count_ones(x: UInt8) -> Int` — 8 位中置位的数量。
- `uint8.byte_swap(x: UInt8) -> UInt8` — 反转字节顺序；仅一个字节，因此为 x 自身。
- `uint8.wrap_add(a: UInt8, b: UInt8) -> UInt8` — a + b 回绕到 8 位。
- `uint8.wrap_sub(a: UInt8, b: UInt8) -> UInt8` — a - b 回绕到 8 位。
- `uint8.wrap_mul(a: UInt8, b: UInt8) -> UInt8` — a * b 回绕到 8 位。
- `uint8.rotate_left(a: UInt8, n: Int) -> UInt8` — 将 a 的 8 位向左旋转 n；n 对 8 取模。
- `uint8.rotate_right(a: UInt8, n: Int) -> UInt8` — 将 a 的 8 位向右旋转 n；n 对 8 取模。
- `uint8.bit_length(x: UInt8) -> Int` — 容纳 x 所需的位数；0 时为 0。
- `uint8.parse(s: String) -> Result[UInt8, String]` — 去除 s 两侧 ASCII 空白后解析十进制 UInt8；无效内容、- 符号或超过 255 为 err。
- `uint8.to_string_radix(x: UInt8, radix: Int) -> String` — 用 radix 进制的小写数字表示 x；无符号；radix 不在 2..36 时 abort。
- `uint8.parse_radix(s: String, radix: Int) -> Result[UInt8, String]` — 去除 s 两侧空白后按 radix 解析 UInt8，语法同 int.parse_radix；无效内容、- 符号或超过 255 为 err。
- `uint8.from(x: S) -> UInt8` — x 向零舍入为 UInt8；NaN、无穷大或超出 UInt8 范围时 abort。
- `uint8.from_checked(x: S) -> Option[UInt8]` — UInt8 能精确容纳 x 时为 some(x as UInt8)，否则（小数部分、NaN、越界）为 none。
- `uint8.from_saturating(x: S) -> UInt8` — x 向零舍入为 UInt8，限制在 UInt8 范围；NaN 为 0。
- `uint8.from_wrapping(x: S) -> UInt8` — 将整数 x 的低 8 位作为 UInt8（二进制补码）。

## unix

[unix.md](unix.zh-CN.md)

- `effect unix.connect(path: String) -> Fd` — 连接到 path 处监听器的流式套接字。
- `effect unix.listen(path: String) -> Fd` — 绑定在 path 的监听器，path 必须尚不存在。
- `effect unix.accept(listener: Fd) -> Fd` — 阻塞等待 listener 上的下一个连接。
- `effect unix.write_fds(sock: Fd, data: Bytes, fds: List[Fd]) -> Unit` — 发送全部 data，描述符 fds 随第一个字节传递。
- `effect unix.take_fds(sock: Fd) -> List[Fd]` — sock 上收到但尚未取走的描述符，最早的在前。
- `effect unix.select(xs: List[Fd], timeout_ms: Int) -> List[Fd]` — 按顺序返回 xs 中在 timeout_ms 内可读的描述符；< 0 等待一个可读，0仅检查。
- `effect unix.shm_create(size: Int) -> Fd` — 新建 size 字节的填零无名文件。
- `effect unix.shm_resize(fd: Fd, size: Int) -> Unit` — 将文件 fd 扩大或缩小到 size 字节。
- `effect unix.shm_write(fd: Fd, offset: Int, data: Bytes) -> Unit` — 在 fd 的 offset 处写入 data；负偏移或越过末尾的写入会中止（先 shm_resize）。
- `effect unix.shm_read(fd: Fd, offset: Int, len: Int) -> Bytes` — 从 fd 的 offset 读取最多 len 字节，末尾可能更少。

## url

[url.md](url.zh-CN.md)

- `url.encode_component(s: String) -> String` — 除 A-Za-z0-9-._~ 外，均编码为 UTF-8 %XX 转义。
- `url.decode_component(s: String) -> Result[String, String]` — 百分号解码 s；错误转义/UTF-8 为 err；保留 +。
- `url.parse(s: String) -> Result[Url, String]` — 从 s 得到 Url，按 WHATWG 方式清理；错误 scheme、主机、端口，反斜杠或控制字符、userinfo、IPv6 均为 err。
- `url.to_string(u: Url) -> String` — u 的 URL 文本；省略空 query/fragment。
- `url.build_query(xs: List[(String, String)]) -> String` — 用 & 连接 k=v，两侧均百分号编码。
- `url.encode_form(s: String) -> String` — 类似 encode_component，但空格变为 +。
- `url.decode_form(s: String) -> Result[String, String]` — 先将 + 变为空格，再百分号解码；错误转义/UTF-8 为 err。
- `url.parse_query(query: String) -> Result[List[(String, String)], String]` — 解码后的 (k, v) 对；无值键得到 ""；错误转义为 err。
- `url.resolve(base: Url, ref: String) -> Result[Url, String]` — 像 parse 一样清理 ref，相对于 base 解析（RFC 3986 5.2）；结果不是 URL 则为 err。

## uuid

[uuid.md](uuid.zh-CN.md)

- `effect uuid.v4() -> Uuid` — 随机（version 4）Uuid：来自 OS CSPRNG 的 122 位。
- `effect uuid.v7() -> Uuid` — 按时间排序的（version 7）Uuid：Unix ms、计数器、随机位；每个都大于上一个。
- `uuid.parse(s: String) -> Result[Uuid, String]` — s 表示的 Uuid：8-4-4-4-12 十六进制数字，大小写均可，可选置于 {…} 内或 urn:uuid: 后。
- `uuid.to_string(u: Uuid) -> String` — u 的 8-4-4-4-12 小写十六进制表示。
- `uuid.from_bytes(b: Bytes) -> Result[Uuid, String]` — 恰好 16 字节 b 的 Uuid；其他长度为 err。
- `uuid.to_bytes(u: Uuid) -> Bytes` — u 的 16 个字节。
- `uuid.compare(a: Uuid, b: Uuid) -> Int` — 逐字节比较，a 在 b 之前、相同、之后时分别为负、零、正。

## value

[value.md](value.zh-CN.md)

- `value.array(xs: List[Value]) -> Value` — 将 xs 包装为 Array Value。
- `value.as_array(v: Value) -> Result[List[Value], String]` — Array 元素；v 为其他种类则 err。
- `value.as_bool(v: Value) -> Result[Bool, String]` — Bool payload；否则 err（没有 truthiness）。
- `value.as_float(v: Value) -> Result[Float, String]` — Float payload（Int 拓宽）；否则 err。
- `value.as_int(v: Value) -> Result[Int, String]` — Int payload；Float 为 err，从不截断。
- `value.as_string(v: Value) -> Result[String, String]` — Str payload；否则 err（不强制转换）。
- `value.bool(b: Bool) -> Value` — 持有 Bool b 的 Value。
- `value.float(f: Float) -> Value` — 持有 Float f 的 Value。
- `value.int(n: Int) -> Value` — 持有 Int n 的 Value。
- `value.merge(a: Value, b: Value) -> Value` — 浅合并，b 的键优先；若不都是 object，则返回 b。
- `value.null() -> Value` — JSON null Value。
- `value.object(xs: List[(String, Value)]) -> Value` — 将 pairs xs 包装为 Object；重复键保留首次位置、最后值。
- `value.omit(v: Value, keys: List[String]) -> Value` — v 去掉列出的键；非 object 保持原样。
- `value.pick(v: Value, keys: List[String]) -> Value` — 仅 v 中列出的键；非 object 选取为 {}。
- `value.string(s: String) -> Value` — 持有 String s 的 Value。
- `value.as_object(v: Value) -> Result[List[(String, Value)], String]` — 按顺序排列的 Object 对；v 为其他种类则 err。
- `value.get(v: Value, key: String) -> Option[Value]` — key 处的 Value；不存在或 v 不是 Object 则 none。
- `value.get_string(v: Value, key: String) -> Option[String]` — key 处的 String；不存在或为其他种类则 none。
- `value.get_int(v: Value, key: String) -> Option[Int]` — key 处的 Int；不存在或非 Int 则 none（Float 从不截断）。
- `value.get_float(v: Value, key: String) -> Option[Float]` — key 处的 Float（Int 拓宽）；不存在或为其他种类则 none。
- `value.get_bool(v: Value, key: String) -> Option[Bool]` — key 处的 Bool；不存在或为其他种类则 none。
- `value.get_array(v: Value, key: String) -> Option[List[Value]]` — key 处的 Array 元素；不存在或为其他种类则 none。
- `value.get_object(v: Value, key: String) -> Option[List[(String, Value)]]` — key 处的 Object 对；不存在或为其他种类则 none。
- `value.root() -> Path` — 空 path，即整个值。
- `value.key(p: Path, name: String) -> Path` — 用 object 键 name 扩展 p。
- `value.index(p: Path, i: Int) -> Path` — 用 array 索引 i 扩展 p；负 i 为越界。
- `value.get_path(v: Value, p: Path) -> Option[Value]` — p 处的 Value；任一步未命中则 none。
- `value.set_path(v: Value, p: Path, x: Value) -> Result[Value, String]` — 在 p 处设为 x 的 v；创建缺失键，索引未命中则保持 v；结果嵌套会超过 1000 时 err。
- `value.remove_path(v: Value, p: Path) -> Value` — v 去掉 p 处的 entry；不存在则不变；root 得到 null。

## zlib

[zlib.md](zlib.zh-CN.md)

- `effect zlib.compress(b: Bytes) -> Bytes` — b 的 zlib 封装 DEFLATE；字节因目标而异。
- `effect zlib.compress_level(b: Bytes, level: Int) -> Bytes` — 类似 compress；level 限制到 0..9，wasm 不使用。
- `zlib.decompress(b: Bytes) -> Result[Bytes, String]` — 解压的 zlib stream；输入损坏、其后有额外字节、输出超过 268435456 字节均为 err。
- `zlib.decompress_max(b: Bytes, max_len: Int) -> Result[Bytes, String]` — 类似 decompress，输出上限 max_len 字节；max_len <= 0 则 abort。
- `effect zlib.deflate(b: Bytes) -> Bytes` — 无 header 的 raw DEFLATE；字节因目标而异。
- `effect zlib.deflate_level(b: Bytes, level: Int) -> Bytes` — 类似 deflate；level 限制到 0..9，wasm 不使用。
- `zlib.inflate(b: Bytes) -> Result[Bytes, String]` — decode raw DEFLATE；stream 损坏、其后有额外字节、输出超过 268435456 字节均为 err。
- `zlib.inflate_max(b: Bytes, max_len: Int) -> Result[Bytes, String]` — 类似 inflate，输出上限 max_len 字节；max_len <= 0 则 abort。
- `effect zlib.gzip(b: Bytes) -> Bytes` — b 的 RFC 1952 gzip；字节因目标而异。
- `zlib.gunzip(b: Bytes) -> Result[Bytes, String]` — decode gzip member；header 错误、CRC 错误、空输入或输出超过 268435456 字节均为 err。
- `zlib.gunzip_max(b: Bytes, max_len: Int) -> Result[Bytes, String]` — 类似 gunzip，输出上限 max_len 字节（所有 member 合计）；max_len <= 0 则 abort。
