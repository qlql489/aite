# 快速开始

本章面向第一次接触 Aite 的用户，目标很简单：把 Aite 安装好，让 Claude Code 能跑起来，并完成第一次可用配置。

如果你只想尽快开始，建议按下面顺序阅读：

1. [安装与环境准备](/quick-start/installation)
2. [自动安装](/quick-start/auto-installation)
3. [手动安装](/quick-start/manual-installation)
4. [Claude Code 环境变量](/quick-start/claude-code-env)

## 你需要先知道的事

Aite 是 Claude Code 的桌面客户端，本身不替代 Claude Code CLI。

也就是说，Aite 的聊天、项目、工具调用和会话管理能力，底层仍然依赖本机可用的 Claude Code 运行环境。为了降低门槛，Aite 已经内置了环境检测与自动安装流程：

- 启动时自动检查 Claude Code 是否可用
- Windows 下额外检查 Git Bash 运行依赖
- 检查系统里是否有可用的 Node.js / npm
- 缺失时引导自动安装

## 推荐路径

### 新用户

推荐直接走 Aite 的自动检测与自动安装流程：

- 安装 Aite
- 首次启动
- 按界面提示完成环境检测
- 需要时允许 Aite 自动补齐依赖

### 已经有 Claude Code 环境的用户

如果你本机已经装好了 Claude Code CLI：

- Aite 会优先复用现有环境
- 如果路径或运行时异常，再按需手动修复
- 如果 CLI 不在常规 PATH 中，也可以通过环境变量显式指定

## 本章涵盖内容

### 安装

- 从哪里下载 Aite
- Windows 安装方式
- macOS 安装方式

### 环境准备

- Claude Code 基础运行依赖
- Windows 的 Git Bash 依赖
- Node.js / npm 检测逻辑

### 安装方式

- Aite 自动安装
- 手动安装 Claude Code
- 手动修复 PATH 与环境变量

### 首次可用配置

- Claude Code 环境变量
- 供应商与模型相关变量
- 首次启动后的检查项

## 完成标志

当下面几件事都成立时，就可以认为快速开始完成了：

- Aite 能正常启动
- Aite 能检测到 Claude Code CLI
- 可以打开一个项目并创建会话
- 可以正常发送一条消息并收到响应
