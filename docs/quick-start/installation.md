# 安装与环境准备

本页说明如何安装 Aite，以及在 Windows 和 macOS 上让 Claude Code 运行起来分别需要准备什么。

## 安装 Aite

### macOS

从项目的 [Releases](https://github.com/qlql489/aite/releases) 页面下载 `.dmg` 安装包，双击后按系统提示安装。

如果系统提示“无法验证开发者”或“Apple 无法检查此 App 是否包含恶意软件”，可以这样处理：

1. 先双击打开一次 Aite，让 macOS 先弹出拦截提示
2. 打开“系统设置” -> “隐私与安全性”
3. 滚动到“安全性”区域，找到刚刚被拦截的 Aite
4. 点击“仍要打开”
5. 输入登录密码确认后，再次打开应用

注意：

- `仍要打开` 按钮通常只会在首次被拦截后的约 1 小时内出现
- 如果没有看到这个按钮，重新双击应用一次，再回到“隐私与安全性”页面查看
- 这是未经过 Apple notarization 的常见提示，不影响应用本体是否完整下载

当前项目面向：

- Apple Silicon（arm64）
- Intel（x86_64）

### Windows

从项目的 [Releases](https://github.com/qlql489/aite/releases) 页面下载 `.msi` 安装包，双击运行安装程序即可。

## Aite 依赖什么

Aite 是桌面端壳层，实际聊天能力依赖本机 Claude Code CLI。

安装链路里涉及的核心组件有：

- `Aite`：桌面应用本体
- `Claude Code CLI`：核心执行环境
- `Node.js / npm`：用于安装或补齐 Claude Code CLI 运行环境
- `Git Bash`：Windows 下 Claude Code 运行所需依赖之一

## Windows 环境准备

Windows 需要关注两类依赖：

### 1. Claude Code CLI

如果系统里还没有 Claude Code，Aite 可以在首次启动时自动尝试安装。

### 2. Git Bash

根据当前实现，Aite 会在 Windows 上检查 Git Bash 是否可用。

如果系统里没有现成的 Git Bash，Aite 可以自动补齐 `PortableGit` 运行环境；也可以自行安装 Git for Windows。

### 3. Node.js / npm

Aite 会先检查系统里是否已有可用的 `node` 和 `npm`。

如果缺失，Aite 会在征得确认后，把本地 Node.js 运行时安装到应用数据目录，不会覆盖你系统已有的 Node 配置。

## macOS 环境准备

macOS 相对简单，主要关注：

### 1. Claude Code CLI

如果没有安装 Claude Code，Aite 可以自动安装。

### 2. Node.js / npm

如果系统里缺少 `npm`，Aite 同样会提示补齐本地 Node.js 运行时，再继续安装 Claude Code CLI。

### 3. PATH 与终端环境

如果你是通过 `nvm`、`fnm`、`volta` 等方式安装 Node.js，偶尔会遇到“终端里能用，但 GUI 应用里检测不到”的情况。

这时建议：

- 先尝试 Aite 内置的自动安装
- 或者手动配置 `CLAUDE_CLI_PATH`
- 或者按手动安装页重新确认 `npm` 与 `claude` 路径

## 首次启动时会检查什么

Aite 首次启动会做这些事情：

1. 检查 Claude Code CLI 是否已安装
2. 检查 CLI 是否真的可运行，而不只是文件存在
3. Windows 下额外检查 Git Bash
4. 检查 Node.js / npm 运行环境
5. 在必要时引导自动安装

## 安装完成后的检查

完成安装后，建议确认下面几项：

- Aite 可以正常进入主界面
- 应用没有反复弹出环境缺失提示
- Claude Code 版本能被识别
- 能成功新建一个项目会话

如果任意一项不正常，继续看：

- [自动安装](/quick-start/auto-installation)
- [手动安装](/quick-start/manual-installation)
- [Claude Code 环境变量](/quick-start/claude-code-env)
