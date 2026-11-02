<a id="als--文字列strings"></a>

# ALS — 字符串（Strings）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](strings.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](strings.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](strings.zh-CN.md)

> [固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/strings.md)的非规范读者快照。下文的规范地位和要求属于原文，不属于本副本。不自动同步；本次翻译未独立复核历史实现、符合性及安全主张。代码与规则标识符保持不变。

> Last updated: 2026-09-10

Almide Language Specification 的字符串规范。独立于实现（v0 native / v1 wasm）定义可观测行为（stdout、stderr、退出码）。契约台账（docs/contracts/contracts.toml 的 `spec` 字段）引用各节，`scripts/check-contracts.sh` 强制要求所引章节存在。

<a id="als-s1-コードポイント意味論"></a>

## ALS-S1 码点语义

字符串是 UTF-8 字节序列；`string.len`、`slice`、`chars`、`string.get`、`index_of` 按**码点**计数（不是字节，也不是字素；String 的 `s[i]` 索引语法以 E026 拒绝，元素访问使用 `string.get(s, i) -> Option[String]`）。
对于多字节字符（包括 CJK、emoji、组合字符），这些操作必须一致地使用这一单位。越界时，`string.get` 返回 none，`slice` 返回空字符串。显式的字节级操作由 ALS-S4 规定。
Contracts: C-016。

<a id="als-s2-空パターンの検索規則"></a>

## ALS-S2 空模式的搜索规则

`string.count_matches("")` 返回**码点数 + 1**（Rust `str::matches` 的空模式语义）。`last_index_of` 返回最后一次匹配的**码点位置**；空模式时返回 `len`（末尾位置）。`index_of("")` 为 0。
Contracts: C-017。

<a id="als-s3-文字種述語"></a>

## ALS-S3 字符类别谓词

提升规则**因谓词而异**。**不存在**“空字符串的所有全称谓词都为 true”的统一规则（2026-08-21 裁定，参考求值器发现 F8）。规范如下表，各行由夹具固定：

| 谓词 | 字符集合 | 判定 | `""` |
|------|----------|------|------|
| `string.is_alpha` | Unicode `Alphabetic`（`char::is_alphabetic`） | 所有码点均符合，且**非空** | false |
| `string.is_alphanumeric` | Unicode 字母数字（`char::is_alphanumeric`：包含字母、数字、`²`、`½`、阿拉伯数字等） | 所有码点均符合，且**非空** | false |
| `string.is_digit` | **仅 ASCII `0-9`**（全角、阿拉伯、印度数字均不可） | 所有码点均符合，且**非空** | false |
| `string.is_upper` / `is_lower` | 有大小写属性（cased）的码点 | **至少有一个 cased 码点**，且 cased 码点**全部**为大写（小写）。忽略没有大小写属性的字符（`.`、空白、CJK、数字；`A.B` 为 upper，`日2` 两者均 false，`Ａa` 两者均 false） | false |
| `string.is_whitespace` | Unicode `White_Space`（ALS-T1 的集合） | 所有码点均符合。**空串按空真原则为 true** | **true** |

除 `is_digit` 之外，仅限 ASCII 的近似实现均不符合规范。`replace_first`、`strip_prefix`、`strip_suffix`、`cmp` 与 Rust str 的对应方法观测等价（`cmp` 使用字节字典序）。

```almide
test "each predicate lifts its own way over the empty string" {
  assert_eq(string.is_alpha(""), false)
  assert_eq(string.is_alphanumeric(""), false)
  assert_eq(string.is_digit(""), false)
  assert_eq(string.is_upper(""), false)
  assert_eq(string.is_whitespace(""), true)
  assert_eq(string.is_upper("A.B"), true)
  assert_eq(string.is_alphanumeric("\u{00BD}"), true)
  assert_eq(string.is_digit("\u{FF11}"), false)
}
```

夹具：`spec/wasm_cross/string_predicates.almd`、`spec/wasm_cross/string_ops_drain.almd`、
`spec/wasm_cross/string_whitespace.almd`、`spec/wasm_cross/string_is_alphanumeric.almd`。
Contracts: C-018, C-019。

<a id="als-s4-バイト列との相互変換"></a>

## ALS-S4 与字节序列的相互转换

`bytes.to_string_lossy` 进行 **UTF-8 有损解码**（无效序列替换为 U+FFFD，替换规则与 `String::from_utf8_lossy` 相同）。`bytes.from_string` 原样返回 UTF-8 字节序列；对于有效字符串，`to_string_lossy ∘ from_string` 是恒等映射。
`string.byte_slice(s, start, end)` 按 UTF-8 字节复制半开区间，仅当两端都是码点边界，且 `0 <= start <= end <= バイト長`（字节长度）时返回 `some(String)`。否则返回 `none`。有效边界上的空区间返回 `some("")`。端点检查不扫描区间之外，复制量等于指定区间的字节数。按码点计数的 `string.slice` 的截限规则不变。

夹具：`spec/wasm_cross/string_from_bytes.almd`、
`spec/wasm_cross/string_byte_slice.almd`。
Contracts: C-022, C-348。

<a id="als-s5-split-の区切り規範"></a>

## ALS-S5 split 的分隔符规范

`string.split` 将分隔字符串的转义和特殊字符按字面处理（不是正则表达式）。连续分隔符产生空元素，开头／结尾的分隔符也一样（与 Rust `str::split` 观测等价）。
Contracts: C-050。

<a id="als-s6-規模不変性"></a>

## ALS-S6 规模不变性

split / replace 等反复字符串操作，对于输入规模（MB 级）保持结果正确且与规模无关（内部缓冲区边界、重新分配不会改变结果）。
Contracts: C-074。

> 原文背景：ALS-S1 对越界 slice 结果作了概括说明，而 ALS-S4 同时保留 string.slice 的截限规则。这里保留两者，不推断统一的边界情形规则；具体区间应查阅固定规范与夹具。
