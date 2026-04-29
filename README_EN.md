<div align="center">

<img src="public/app-icon.svg" alt="Aite Logo" width="120" />

# Aite

### A Desktop Client for Claude Code

Bring project management, session switching, message browsing, tool call tracking, and permission approval back into one clear desktop interface.

[![Release](https://img.shields.io/badge/release-v0.1.0-1688d9?style=flat-square)](#)
[![License](https://img.shields.io/badge/license-Apache%202.0-97CA00?style=flat-square)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-macOS%2011.2%2B%20%7C%20Windows%2010%2B-blue?style=flat-square)](#)
[![Tauri](https://img.shields.io/badge/Tauri-2.0-FFC131?style=flat-square&logo=tauri&logoColor=white)](https://tauri.app)

**[中文](README.md)** | **[English](README_EN.md)**

<p align="center">
  <a href="#what-is-aite">What is Aite</a> ·
  <a href="#desktop-preview">Screenshots</a> ·
  <a href="#architecture-overview">Architecture</a> ·
  <a href="#quick-start">Quick Start</a> ·
  <a href="#more-docs">More Docs</a>
</p>

</div>

## What Is Aite

Aite is a desktop client built around the daily Claude Code workflow.

With a desktop-native experience for multi-project management, fluid history browsing, and clear tool call visualization, Aite makes using Claude Code more efficient and enjoyable.

<a id="desktop-preview"></a>
## Desktop Preview

<table>
  <tr>
    <td align="center" width="50%">
      <img src="img/main.png" alt="Main interface" />
      <br />
      <b style="font-size: 16px;">Main interface</b>
    </td>
    <td align="center" width="50%">
      <img src="img/codediff.png" alt="Tool calls and diff view" />
      <br />
      <b style="font-size: 16px;">Tool calls and diff view</b>
    </td>
  </tr>
  <tr>
    <td align="center" width="50%">
      <img src="img/permission.png" alt="Permission review" />
      <br />
      <b style="font-size: 16px;">Permission review</b>
    </td>
    <td align="center" width="50%">
      <img src="img/skill.png" alt="Skill management" />
      <br />
      <b style="font-size: 16px;">Skill management</b>
    </td>
  </tr>
  <tr>
    <td align="center" width="50%">
      <img src="img/mcp.png" alt="MCP management" />
      <br />
      <b style="font-size: 16px;">MCP management</b>
    </td>
    <td align="center" width="50%">
      <img src="img/provider.png" alt="Anthropic and OpenAI provider management" />
      <br />
      <b style="font-size: 16px;">Anthropic and OpenAI provider management</b>
    </td>
  </tr>
</table>

<a id="quick-start"></a>
## Quick Start

### Requirements

- [Claude Code](https://github.com/anthropics/claude-code) — Aite depends on the Claude Code CLI and can help detect and install it.

### Download

For macOS:

Download the `.dmg` file from [Releases](https://github.com/qlql489/aite/releases) and install it by double-clicking.
Both Apple Silicon (`arm64`) and Intel (`x86_64`) are supported.

If macOS shows “developer cannot be verified” or says Apple cannot check the app for malicious software:

1. Try opening the app once so macOS shows the warning.
2. Open `System Settings` -> `Privacy & Security`.
3. Scroll to the `Security` section and click `Open Anyway`.
4. Enter your login password, then open Aite again.

The `Open Anyway` button is usually available for about one hour after the first blocked launch.

For Windows:

Download the `.msi` installer from [Releases](https://github.com/qlql489/aite/releases) and run it.

<a id="features"></a>
## Key Features

### Multi-project and session management

Stop juggling terminal windows and manage local projects together with Claude Code sessions in one place.

- Quickly import and switch local projects
- Create, resume, and persist sessions
- Search historical sessions and jump back fast

### Message and tool visualization

Make every Claude Code interaction visible and easy to inspect.

- Stream all message types including thinking, tool calls, subagents, and images
- Present tool calls in a structured way, with diff views for `Edit` and `Write`
- Review permission requests directly inside the chat stream, with approve, reject, and always-allow actions
- Visualize todo progress and completion state

### File and code collaboration

Integrate smoothly with local project workflows.

- `@` file references, with support for directories, images, and file attachments
- Workspace tree browsing and inline file editing
- Integrated Git status, branch, and change information
- VSCode IDE context injection support

### Configuration and extension management

Replace command-line-heavy configuration with a visual interface.

- Unified management for MCP servers, skills, and commands
- Model switching and thinking-level selection
- Support for custom LLMs (Anthropic/OpenAI protocol compatible)
- Token usage statistics and CLI argument configuration

<a id="architecture-overview"></a>
## Architecture Overview

- **Desktop container**: Built on Tauri 2.0 for a compact install size, fast startup, and cross-platform support
- **Frontend UI**: Vue 3 + TypeScript + Vite for the desktop interaction layer
- **Local capability bridge**: Rust and a Node backend cooperate to handle CLI, files, Git, and provider bridge logic
- **State and rendering**: Pinia drives state management, while Markdown rendering, code highlighting, and tool-call views power message visualization

## Tech Stack

Aite uses a modern desktop application stack to stay lightweight, efficient, and cross-platform.

| Layer | Technology |
|------|----------|
| **Desktop framework** | [Tauri 2.0](https://tauri.app) — lightweight desktop framework powered by Rust |
| **Frontend** | Vue 3 + TypeScript + Vite |
| **State management** | Pinia |
| **UI icons** | HugeIcons |
| **Backend** | Rust (Tokio async runtime) |
| **Markdown rendering** | marked + DOMPurify |
| **Code highlighting** | highlight.js |

## Roadmap

- **Computer Use** — support desktop automation workflows for Claude
- **Scheduled tasks** — support cron-based command execution
- **Lark integration** — support Lark APIs for messaging, calendars, documents, and more

<a id="environment-variables"></a>
## Environment Variables

The current version prefers managing Claude Code, model providers, MCP servers, and CLI arguments through the Aite GUI.

If you are just using the desktop client normally, you usually do not need to maintain complex environment variables manually. This section can be expanded later if standalone documentation is added.

<a id="faq"></a>
## FAQ

### What runtime does Aite depend on?

It depends on a locally installed [Claude Code](https://github.com/anthropics/claude-code). On first launch, Aite helps detect and install it if needed.

### What is Aite best suited for?

It is a strong fit if you frequently switch between projects, revisit historical messages, inspect tool-call details, or want Claude Code to feel like part of your desktop workflow instead of a terminal-only tool.

<a id="global-usage"></a>
## Using It Alongside CLI Workflows

Aite itself is a desktop client, and the recommended path is to manage sessions and projects through the GUI. If you prefer terminal-heavy workflows, you can still use it alongside your local Claude Code CLI setup.

<a id="more-docs"></a>
## More Docs

### Author

This project is fully developed independently by [qlql489](https://github.com/qlql489/aite).

## Contact

This project is developed independently by [qlql489](https://github.com/qlql489/aite) in spare time. Sponsorship from individuals or companies is welcome to support continued iteration. If you have custom development, system integration, or business collaboration needs, feel free to get in touch.

## Community Group

Scan the QR code to join the group and share usage feedback, report issues, and discuss future features together.

<img src="img/group.jpg" width="200" alt="Aite WeChat community QR code" />

## Support the Project

If this project helps you, you can support it here:

<img src="img/wechat.jpg" width="200" alt="WeChat donation QR code" />
