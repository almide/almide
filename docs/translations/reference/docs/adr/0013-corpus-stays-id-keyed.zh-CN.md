
> 本文是面向读者的参考译文。以上固定提交的原文为权威版本，本译文不修订规范。状态、数字及实现情况均为作出决定时的记录。Accepted 表示设计已采纳，不表示实现已经完成或当前验证已通过。本译文不会自动同步。原文中的外部调研 ../almide-references、QUALIFICATION.md 等均为原文引用，翻译时未另行获取或复核。

# ADR-0013：一致性测试语料继续以契约 ID 为键；不采用按语义重组

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0013-corpus-stays-id-keyed.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0013-corpus-stays-id-keyed.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](0013-corpus-stays-id-keyed.zh-CN.md)

> 本文是[固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0013-corpus-stays-id-keyed.md)的非规范读者译文；原文仍是权威版本，本译文不会自动同步。



- **状态**：Accepted（已采纳）
- **日期**：2026-08-20
- **背景**：2026-08 的参考调研（../almide-references）记录了 rustc 禁止扁平、机械式的测试放置方式（`deny_new_top_level_ui_tests`、compiler-team#902），并警告不要使用机械键式文件名（`issue-123456.rs`）。这看起来像是在反对本仓库保持路径稳定的语料布局。与此同时，每个使用方都按提交固定本仓库：黄金清单把测试样例路径嵌入由判定器计算哈希的输出中，契约台账的证据条目与 591 个 `// @contract:` 头部引用路径，现有实现也仍保留可做差异比较的副本。
- **决定**：语料保持当前布局和路径稳定的身份。测试样例以契约 ID（`C-NNN`）而不是位置为键。导航由生成的索引（ALS 章节索引、一致性报告）提供，绝不通过移动文件实现。新样例必须采用主题在前的 snake_case 名称（`map_upsert_str.almd`）；纯数字名称以及以 `issue` / `bug` 开头的名称由风格门禁拒绝。
- **理由**：rustc 自身允许的例外，是可以反向还原对应关系的方向——从稳定的不透明 ID 派生的路径（`tests/ui/error-codes/EXXXX.rs`）是合理的，而这正是本语料的形式。调研的警告针对仅以路径为键的语料；这里则是 `C-NNN ⇄ fixture`，并由门禁检查双向对应。重组不会带来生成索引尚未提供的收益，却会迫使所有固定版本的使用方重新生成黄金清单、重写全部引用，并破坏 Stage B 切换所依赖的副本差异比较能力。
- **替代方案**：(a) rustc 式的语义目录，加上 README⇄文件系统门禁——拒绝：生成索引已提供其收益，成本如上。(b) 什么也不做——拒绝：调研警告中的命名纪律问题确实存在，现在已由门禁检查。
- **后果**：语料路径实际上成为只追加的标识符。风格门禁负责样例的命名纪律。如果语料需要新的组织维度，应增加生成索引，而不是移动文件。
- **反证条件**：出现经证实的导航或评审失败，生成索引无法解决，而目录移动可以解决。截至该记录时，在 1,749 个语料文件中尚未观察到此类问题。
- **参考**：../almide-references 调研报告（D1）、rustc 的 `src/tools/tidy/src/ui_tests.rs`、`tests/best-practices.md`。
