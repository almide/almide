[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](almide-release.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](almide-release.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](almide-release.reader.zh-CN.md)

> 历史仓库命令的非活动、非规范读者快照。权威原文不变。此参考路径和 .reader 文件名不注册代理命令。引用指示只是文档，不授权执行、花费、发布、推送或删除。不自动同步，也未独立验证当前 workflow 或测量主张。代码与注释保持原样。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/.claude/commands/almide-release.md)

<a id="release-tag--release-notes"></a>

# 发布：标签与发布说明

创建带标签的 GitHub Release。推送标签会触发 `.github/workflows/release.yml`，自动构建所有平台的二进制文件并附到发布中。

<a id="prerequisites"></a>

## 前提条件

- develop 到 main 的 PR 已合并。
- CI 必须成功。
- Cargo.toml 中的版本已更新。

<a id="steps"></a>

## 步骤

1. **确认状态**
   ```bash
   git fetch origin main
   git log v{previous_version}..origin/main --oneline
   ```
   审查上一个标签以来的全部提交。

2. **编写发布说明**——`docs/releases/v{version}.md`，在打标签之前提交并合并到 `main`（格式与规则见 `docs/releases/README.md`）。第1行 `# v{version} — <headline>` 成为发布标题，其余为按主题组织的正文。绝不使用自动生成的“What's Changed” PR 列表。
   ```bash
   bash scripts/check-release-notes.sh v{version}
   ```

3. **创建并推送标签**
   ```bash
   git tag v{version} origin/main
   git push origin v{version}
   ```
   这会触发 release workflow，执行：
   - 构建 Linux（x86_64、aarch64）、macOS（x86_64、aarch64）、Windows（x86_64）二进制文件。
   - 创建 `almide-{os}-{arch}.tar.gz`（Windows 为 `.zip`）归档。
   - 生成 `almide-checksums.sha256`。
   - 创建包含全部资产的 GitHub Release，标题和说明取自 `docs/releases/v{version}.md`。
   （若该文件缺失或格式错误，workflow 会在构建前失败。）

4. **验证发布**——检查全部5个平台归档及校验和均已附上，每个归档都含 `almide`、`almide-verify` 和 LICENSE：
   ```bash
   gh release view v{version}
   ```

5. **稍后更正说明**——先编辑文件，再从文件重新发布：
   ```bash
   bash scripts/check-release-notes.sh v{version} out
   gh release edit v{version} --title "$(cat out/title)" --notes-file out/body.md
   ```

<a id="release-assets"></a>

## 发布资产

每次发布自动包含：

| 文件 | 内容 |
|------|----------|
| `almide-linux-x86_64.tar.gz` | Linux x86_64 `almide` + `almide-verify` + LICENSE |
| `almide-linux-aarch64.tar.gz` | Linux ARM64 `almide` + `almide-verify` + LICENSE |
| `almide-macos-x86_64.tar.gz` | macOS Intel `almide` + `almide-verify` + LICENSE |
| `almide-macos-aarch64.tar.gz` | macOS Apple Silicon `almide` + `almide-verify` + LICENSE |
| `almide-windows-x86_64.zip` | Windows x86_64 `almide.exe` + `almide-verify.exe` + LICENSE |
| `almide-checksums.sha256` | 全部归档的 SHA-256 校验和 |

<a id="notes"></a>

## 注记

- 版本格式：`v{major}.{minor}.{patch}`。
- 发布说明必须为英语。
- release workflow 完成所有构建约需10分钟。
- 仅修改发布说明的 PR 不属于 CI 文档专用路径：`scripts/check-release-notes.sh` 与 release workflow 读取 `docs/releases/`，因此会重建编译器并运行全部 Emit & Format 检查。
- 构建失败时，先确定是否已发布任何内容。发布前失败与已发布版本的恢复是不同情况。不要把构建失败视为删除或重新标记已发布版本的许可：已提交的 release seal 要求原标签保持可获取。遵循 `CLAUDE.md` 的恢复边界，在任何破坏性操作之前取得所有者明确批准的证据保全计划。
