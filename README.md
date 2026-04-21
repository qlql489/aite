<div align="center">

<img src="public/app-icon.svg" alt="Aite Logo" width="120" />

# Aite

### 面向 Claude Code 的桌面客户端

让项目管理、会话切换、消息查看、工具调用追踪和权限审批都回到一个清晰的桌面界面里。

[![Release](https://img.shields.io/badge/release-v0.1.0-1688d9?style=flat-square)](#)
[![License](https://img.shields.io/badge/license-Apache%202.0-97CA00?style=flat-square)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-macOS%2011.2%2B%20%7C%20Windows%2010%2B-blue?style=flat-square)](#)
[![Tauri](https://img.shields.io/badge/Tauri-2.0-FFC131?style=flat-square&logo=tauri&logoColor=white)](https://tauri.app)

**[中文](README.md)** | **[English](README_EN.md)**

</div>

## Aite 是什么

Aite 是一个围绕 Claude Code 日常使用体验打造的桌面客户端。

如果你已经在命令行里长期使用 Claude Code，你大概率已经遇到过这些问题：项目一多就得来回切窗口，历史消息不方便翻，工具到底做了什么也看得不够直观。Aite 想解决的就是这些真实、重复、影响效率的使用痛点。

它不是替代 Claude Code，而是给 Claude Code 提供一个更适合长期使用的桌面工作台。


## 主界面预览

![](img/main.png)

## 安装方式

### 前置要求
- [Claude Code](https://github.com/anthropics/claude-code) — Aite 依赖claude code cli, Aite会帮助检测与安装。

### 下载
macos系统
从 [Releases](https://github.com/qlql489/aite/releases) 下载`.dmg`文件，双击安装
同时支持 Apple Silicon (arm64) 和 Intel (x86_64)

window系统
从 [Releases](https://github.com/qlql489/aite/releases) 下载`.msi` 安装包运行

## 主要功能介绍

### 项目与会话管理

- 本地项目统一导入和管理
- Claude Code 会话创建、恢复、持久化
- 历史会话导入、搜索和快速切换

### 聊天与历史消息查看

- 流式消息展示
- 支持 thinking、工具调用、任务、子代理、图片、向用户提问等多种消息类型
- compact 后可恢复聊天历史与上下文链路
- 支持会话内搜索，以及输入 / 输出 / 缓存等 token usage 展示

### 工具调用与权限审批

- 工具调用卡片支持结构化展示和展开查看
- Edit / Write 支持 diff 视图，Read 支持行号显示
- AI 主动向用户提问时，支持在界面中展示问题、选项和填写结果
- 权限请求可在聊天流里直接审批
- 支持批准、拒绝、“始终允许”和多种权限模式切换
- TodoWrite 可展示待办列表、当前进度和完成状态
- 支持思考强度选择
- 支持模型切换

### 文件引用与附件输入

- 输入框里支持 `@` 文件引用
- 支持目录、图片和通用文件附件
- 文件搜索会和输入状态联动

### 工作区与代码协同

除了聊天本身，Aite 也补上了和本地项目协作时最常用的一些辅助能力。

- 支持工作区目录树查看和内联文件编辑
- 集成 Git 状态、分支菜单和工作区变更信息
- 支持vscode IDE插件的上下文注入

### 扩展、设置与统计

Aite 把 Claude Code 周边常见的配置项也放进了可视化界面里，减少到处切配置文件和命令的成本。

- MCP Server、skills、commands 可统一管理
- 支持 skill 导入、命令执行和相关状态查看
- 提供统计页和 Claude CLI 参数配置


