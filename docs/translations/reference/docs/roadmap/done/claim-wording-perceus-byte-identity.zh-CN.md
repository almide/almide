<a id="claim-wording-perceus-phrasing-and-the-byte-identity-guarantee-scope"></a>

# 主张措辞：Perceus 表述与字节一致性保证的范围

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](claim-wording-perceus-byte-identity.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](claim-wording-perceus-byte-identity.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](claim-wording-perceus-byte-identity.zh-CN.md)

> 标记为 2026-07-15 完成的历史路线图的非规范读者副本。测量、对外文案中的保证和 API 删除陈述均为当时原文的主张，不是独立验证的当前保证。后续文档记载了 fan 时间 API 的变化，不能将本记录视为当前 API 指南。原文仍是该历史记录的权威版本，本副本不会自动同步。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/claim-wording-perceus-byte-identity.md)

调整两处措辞，使公开主张（推文／README claims 区块）完全符合测量结果。GitHub issue 为 [#773](https://github.com/almide/almide/issues/773)。两处措辞均**已确定**，README 也已更新。

<a id="1-perceus-rc-で自動管理perceus-方式の所有権推論で自動管理確定"></a>

## 1. “由 Perceus RC 自动管理”→“由 Perceus 式所有权推断自动管理”【确定】

严格而言：
- **wasm 路径**：通过 RC（rc_inc/rc_dec + free-list）执行 Perceus MIR 的判定。
- **Rust 路径**：通过 Rust 的 clone/drop 实现相同的 Perceus MIR 判定（Alloc/Dup/Drop/Consume，已由 `verify_ownership` 认证）。RC 这一**执行机制**是 wasm 侧的实现方式。

确定措辞：“内存管理由 Perceus 式所有权推断自动决定（wasm 执行 RC，Rust 输出通过 Rust 的 drop 实现相同判定）。”这样可以同时用于两个目标。

README 更新：将 Memory Safety 节改写为“decided by Perceus-style ownership inference / what differs per target is only the *execution mechanism*”的结构。对 trust-spine ladder（#764）的引用也更新为“shared scalar and list ops already render on both targets from the same MIR”，反映 rung 4 交付后的情况。

<a id="2-出力バイト一致を保証の範囲明示確定"></a>

## 2. 明确“保证输出字节一致”的范围【确定】

确定措辞：“通过契约台账（C-001..C-133，flagged 0）、跨目标差分门禁（spec/wasm_cross 246 个 fixture）以及组织内真实项目的双目标验证，**持续保证**可观察输出（stdout/stderr/exit）的字节一致。”

显式排除项（由台账管理）：
- 对本质上的非确定性，通过“确定性不变量”契约认证，例如 C-112 `random.int` 的范围不变量。唯一的墙钟接口（fan.timeout）已由 C-006 删除。
- wasm 尚未实现的 API（fs.glob 等）会**拒绝编译或执行**，不会输出错误字节（honest wall）。

README 更新：在 Equivalence Claim 节的“continuous, with an explicit scope”下加入三项：in scope / nondeterministic sources / not-yet-implemented APIs。

<a id="確定版発信文ツイート等"></a>

## 确定版对外文案（推文等）

> 我正在为这样的需求开发一门语言，程序员不需要写所有权或生命周期。内存管理由 Perceus 式所有权推断自动决定（wasm 执行 RC，Rust 输出通过 drop 实现相同判定）。在保留无异常、穷尽 match 的 Rust 系类型纪律下，编译到 Rust/WASM，并通过契约台账和差分门禁持续保证可观察输出（stdout/stderr/exit）的字节一致。有兴趣欢迎看看。

简短版（字数受限时）：

> 我在开发一门不用写所有权或生命周期的语言。内存管理由 Perceus 式所有权推断自动决定（wasm 用 RC，Rust 用 drop 实现相同判定）。无异常、穷尽 match，通过契约台账与 CI 门禁持续保证 Rust/WASM 两种输出的可观察输出字节一致。有兴趣欢迎看看。

<a id="背景言い切りを支える事実"></a>

## 背景：支撑措辞的事实

- 最后一个已知差异（可变 module var 的 snapshot alias——`--no-verified` 的 v0-wasm 为 alias，native／默认 v1 为 COW）在 e088c25d 时已**在默认路径解决**，并固定为 C-033 的 fixture `spec/wasm_cross/module_var_alias_cow.almd`。差异只在显式 opt-out（`--no-verified`）下保留。
- checker 通过 E010 强制穷尽 match（已实测确认）。
