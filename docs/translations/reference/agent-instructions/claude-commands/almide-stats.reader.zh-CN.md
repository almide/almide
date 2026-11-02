[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](almide-stats.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](almide-stats.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](almide-stats.reader.zh-CN.md)

> 历史仓库命令的非活动、非规范读者快照。权威原文不变。此参考路径和 .reader 文件名不注册代理命令。引用指示只是文档，不授权执行、花费、发布、推送或删除。不自动同步，也未独立验证当前 workflow 或测量主张。代码与注释保持原样。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/.claude/commands/almide-stats.md)

<a id="update-published-measurement-stats"></a>

# 更新公开测量统计

首页 README 链接到 `docs/benchmarks/README.md`，后者包含生成块和带日期的测量（`scripts/lib/readme-targets.sh`）。每个公开数字要么由脚本从 tree 内来源渲染，要么带测量日期；出现无此依据的裸数字时，`scripts/check-readme-numbers.sh` 会使 CI 失败。因此“更新统计”意为重新生成，只有以带记录的 baseline 为来源时才测量。

<a id="regenerate-no-measurement--derives-from-the-tree"></a>

## 重新生成（不测量，从 tree 导出）

```bash
bash scripts/gen-readme-stats.sh        # stdlib functions/modules (from docs/stdlib signature indexes),
                                        # spec/ test files, contract count; renders the Hello, world
                                        # size table from docs/benchmarks/wasm-size.txt
bash scripts/gen-claims.sh              # contract-ledger claims block + proofs/STAGE-STATUS.md
almide run tools/almide-gates/src/main.almd -- bench docs/benchmarks --readme
                                        # build-speed block from docs/benchmarks/build-speed.txt
```

原文注释说明：首个命令处理 stdlib 函数／模块数量（来自 docs/stdlib 签名索引）、spec/ 测试文件数及契约数，并从 docs/benchmarks/wasm-size.txt 渲染 Hello, world 大小表。第二个生成契约台账主张块和 proofs/STAGE-STATUS.md。最后一个从 docs/benchmarks/build-speed.txt 生成构建速度块。

输入更改时，lefthook 的 pre-commit hook 会运行前两个命令。手动运行适用于新 checkout 或 stdlib 变更后（先用 `make stdlib-docs` 重新生成签名索引）。

<a id="re-measure-only-when-the-compiler-changed-the-thing-being-measured"></a>

## 重新测量（仅当编译器改变了测量对象）

```bash
bash scripts/gen-readme-stats.sh --measure     # Hello, world on the structural wasm leg → docs/benchmarks/wasm-size.txt
almide run tools/almide-gates/src/main.almd -- bench docs/benchmarks
                                               # build-speed baseline → docs/benchmarks/build-speed.txt
```

原文注释说明：首个将 structural wasm 路径的 Hello, world 记录到 docs/benchmarks/wasm-size.txt；第二个将构建速度 baseline 记录到 docs/benchmarks/build-speed.txt。

两个 baseline 均包含二进制版本与日期。CI 重建 Hello, world 并要求字节数与记录一致；因此 emitter 改变时在此重新记录，绝不手动编辑生成的测量块。

<a id="msr"></a>

## MSR

由 [almide-dojo](https://github.com/almide/almide-dojo) 测量，不在此处。新运行结果落地后，更新 `docs/benchmarks/README.md` 的 scorecard 表及日期（该行必须保留 `YYYY-MM-DD`，否则数值 gate 拒绝）。

<a id="check"></a>

## 检查

```bash
bash scripts/gen-readme-stats.sh --check && bash scripts/check-readme-numbers.sh && bash scripts/gen-claims.sh --check
```

提交信息用一行，例如 `Regenerate README stats after the stdlib change`。
