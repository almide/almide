# 安全政策

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](./SECURITY.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](./SECURITY.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](./SECURITY.zh-CN.md)

## 安全负责人

Almide 开发公司 Aid-On Inc.（株式会社Aid-On）的代表董事 Hiromi Motodera（本寺 広海）是安全负责人。报告应通过下方的私密渠道提交，而非发送至个人地址。

## 报告漏洞

通过 **GitHub Security Advisories** 私下报告：
https://github.com/almide/almide/security/advisories/new — 请勿为尚未修复的漏洞创建公开 Issue。预计将在 7 天内收到初次回复。协调披露：我们请求留出 90 天的窗口期（修复发布后可经协商缩短）。

范围包括：编译器及其生成的代码（破坏内存安全性声明的错误编译属于安全漏洞；参见 `docs/project/ISSUE-TAXONOMY.md` 中的严重性分类：根据 CI 政策，`I-unsound` / `I-miscompile` 会阻止发布）、运行时（`runtime/rs`）、wasm 处理路径及其宿主 shim，以及发布流水线。

## 支持的版本

最新次版本系列会获得修复。旧系列不回移修复，请升级。发布阻断政策：只要存在未关闭的 `I-unsound` / `I-miscompile` / `I-divergence` / `regression` Issue，就不能发布正式标签（由发布工作流执行，#1482）。

## 依赖政策

CI 会根据 RustSec 安全公告数据库审计依赖树（`cargo audit`，每周运行并在每次 lockfile 变更时运行，参见 `.github/workflows/dependency-audit.yml`）。没有修复版本的公告通过 Issue 分诊处理，而不是静默屏蔽；永久忽略必须在工作流文件中写明理由。

## 发布完整性

每个版本都附带 `almide-checksums.sha256`，发布产物**附有 Sigstore 证明**（GitHub artifact attestation）。验证任意下载的产物：

```
gh attestation verify <asset> -R almide/almide
```

工具自身也会执行此项检查（#4127）：

- `almide self-update` 使用编译器自身的 HTTPS 客户端下载（rustls；不使用 PATH 中的 `curl`，只允许重定向到 https，#4181），在进程内计算压缩包哈希，精确匹配文件名对应的校验和行，在新建的私有目录中暂存，并用 `gh attestation verify` 验证证明。没有 `gh` 时会拒绝安装；`--skip-attestation` 仅凭校验和安装，并明确提示这一点。
- `tools/install.sh` / `tools/install.ps1` 会拒绝缺失或不匹配的校验和，并使用 `gh attestation verify` 验证证明。与 `self-update` 一样，没有 `gh` 时会拒绝安装；`--skip-attestation`（`curl … | sh -s -- --skip-attestation`）仅凭校验和安装，并明确提示这一点。`scripts/check-installer-attestation.sh` 在 CI 中针对模拟的发布运行两个安装程序。

每个版本所附的资格材料（`almide-dossier-*.md`）包含该标签的信任链验证记录，并以相同方式附加证明。
