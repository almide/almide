<a id="als--データ形式data-formats"></a>

# ALS — 数据格式（Data Formats）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](data-formats.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](data-formats.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](data-formats.zh-CN.md)

> 权威原文：[docs/specs/als/data-formats.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/data-formats.md)，固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466`。本页是非规范性的读者快照，不替代原 ALS 或检查器、台账、生成器输入，不自动同步。需求、历史状态、证据和安全主张均保留原文范围。本版未独立执行、测量、认证或证明。原文语言版为读者副本，其他语言版为翻译。

> 历史写法说明：`bytes.read_f16_le` 和 `json.stringify_pretty` 保留原需求写法，并非在此断言它们是当前 API 名称。请另查固定版／当前 API 参考；不静默改写规则或标识符。


> 最后更新：2026-10-26

JSON／正则表达式／二进制格式的可观察规范。引用方式与 [strings.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/strings.md) 开头相同。

<a id="als-d1-json-パス操作"></a>

## ALS-D1 JSON 路径操作

`value.get_path`、`set_path`、`remove_path` 的边界情况（缺失键、数组越界、类型不匹配节点、空路径）与不会失败的 native oracle（基于 serde_json 的参考实现）可观察等价：缺失退化为 none／空操作，不触发 trap。
Contracts: C-031。

<a id="als-d2-value-の-json-テキスト表現"></a>

## ALS-D2 Value 的 JSON 文本表示

动态 `Value` 的字符串化与其 **JSON 文本**逐字节一致，包括键顺序、数值表示和转义。单独使用或作为 Repr 记录的字段时，形式相同。
Contracts: C-060。

<a id="als-d3-異種ネスト文書の走査"></a>

## ALS-D3 遍历异质嵌套文档

异质嵌套 JSON（glTF 级别：数组的数组、混合类型、深层嵌套）的解析和逐元素遍历（`as_array`／下标／字段）在两个目标上逐字节一致。
Contracts: C-063。

<a id="als-d4-正規表現エンジン"></a>

## ALS-D4 正则表达式引擎

正则表达式引擎（match / find / replace / captures）在**经过模糊测试的整个文法范围内**与 native 引擎逐字节一致。方言差异（如 PCRE 与 RE2 的不一致）不符合规范；支持的语法具有单一规范文法。
Contracts: C-032。

<a id="als-d5-半精度浮動小数のデコード"></a>

## ALS-D5 半精度浮点解码

`bytes.read_f16_le` 将 IEEE-754 binary16 精确拓宽为 f64，包括 subnormal、±inf、NaN、±0。
Contracts: C-037。

<a id="als-d6-codec-と-json-デコード"></a>

## ALS-D6 Codec 与 JSON 解码

JSON 数值和 `\u` 转义的解码、整数形式数值到 f64 的拓宽、`json.stringify_pretty` 的缩进输出、派生 Codec 的跨模块分派、动态 Value 模型（merge／array 往返），以及 decode 错误消息的措辞，在目标之间逐字节一致。

`Codec` 是 `Encode` 与 `Decode` 的组合（C-435）。声明 `: Decode` 的类型 `T` 仅派生 `T.decode` 与 `T.decode_strict`；声明 `: Encode` 的类型仅派生 `T.encode`；`: Codec` 派生两者。`: Codec` 类型同时符合 `Encode` 和 `Decode`（满足 `[T: Decode]` 约束）。引用未声明方向的方法以 E002 拒绝。不是 `Encode` 的 `: Decode` 类型称为输入类型，其所有字段都是输入。在输入类型字段上添加 `@skip` / `@skip_decode` 会以 E053 拒绝。
声明 `: Codec` 的类型 `T` 具有静态方法 `T.encode` / `T.decode`。
记录 encode 按声明顺序执行，省略 `none` 字段（不输出显式 null；仅 `Value?` 的 `some(null)` 成为显式 null）。有 default 的字段原样输出实际值，`Value` 字段原样透传。
decode 按声明顺序遍历字段，返回第一个错误：非 object 为 `expected Object`，缺少必需键为 `missing field '<key>'`，类型不匹配使用 Value 分支名给出 `expected Int` / `expected Float` / `expected Str` / `expected Bool`（C-084 的词汇）。未知键被忽略。
派生的 `T.decode_strict`（record 和 variant）在 decode 前遍历值，对任意深度上第一个不属于类型的键 `k` 返回 `unknown key 'k'`。嵌套键附带该 record 的路径，形式为 `, at address.geo` / `, at tags[1]`。
record 先检查自身键（按文档顺序，找出不是被解码字段 wire 键的键），再按声明顺序遍历具有 `Decode` 类型的字段。variant 的第一个键是 tag，后面的键都是未知键。按 case 名路径遍历该 tag 的 case 载荷：record 按记录遍历，tuple 则在每个元素上附加 `[i]`。若没有发现未知键，则返回与 `T.decode` 相同的结果（C-429）。
`@skip` 字段既不 decode 也不 encode；`@skip_decode` 字段不 decode 但会 encode。两者都取 default，其键视为未知键。无 default 的 skip 字段以 E104 拒绝（C-428、`spec/wasm_cross/codec_field_skip_matrix.almd`）。
`Secret[A]` 字段从 `A` 的 wire 形式解码并包装。含 Secret 的类型不派生 `T.encode`，引用 `T.encode` 以 E023 拒绝（C-427、`spec/wasm_cross/secret_eq_and_decode.almd`）。Option 字段将缺失和显式 null 都折叠为 `none`，但 `Value?` 有三态：缺失 → `none`，显式 null → `some(null)`，值 → `some(v)`。有 default 的字段在缺失时取 default 值。`Float` 字段将整数形式 JSON 数值拓宽为 f64 并接受（C-085）；`List[U]` 逐元素解码数组，原样呈现首个 err，不加索引修饰。嵌套 Codec 记录递归处理；record 位置上的非 object 为 `expected Object`。variant Codec 使用外部标签（`{"Case": [payload…]}`，unit case 为 `{"Case": null}`），未知 tag 返回 `unknown variant for <T>`。
行为矩阵由 C-298 的语料固定：`spec/wasm_cross/codec_decode_errors.almd`、`spec/wasm_cross/codec_none_omission.almd`、`spec/wasm_cross/codec_variant_roundtrip.almd`、`spec/wasm_cross/codec_extra_keys.almd`、`spec/wasm_cross/codec_float_int.almd`。
Contracts: C-084, C-085, C-087, C-095, C-098, C-103, C-209, C-427, C-428, C-429, C-435。

<a id="als-d7-バイト列ブリッジ"></a>

## ALS-D7 字节序列桥接

RawPtr／线性内存的字节移动原样复制字节值。`bytes.from_list(List[Int])` 原样复制字节范围（0..=255）内的值，范围外则**按 mod-256 回绕**（`[65, 300, -1]` → 65, 44, 255；0.59.1 在两个目标上实测。不存在针对完整 Int 范围的不截断规范）。
Contracts: C-062, C-090。
