<div align="center">

<img src="public/app-icon.svg" alt="Aite Logo" width="120" />

# Aite

### A Desktop Client for Claude Code

Manage projects, sessions, permissions, file references, extensions, and usage stats in one desktop workspace.

[![Release](https://img.shields.io/badge/release-v0.1.0-1688d9?style=flat-square)](#)
[![License](https://img.shields.io/badge/license-Apache%202.0-97CA00?style=flat-square)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-macOS%2011.2%2B%20%7C%20Windows%2010%2B-blue?style=flat-square)](#)
[![Tauri](https://img.shields.io/badge/Tauri-2.0-FFC131?style=flat-square&logo=tauri&logoColor=white)](https://tauri.app)

**[中文](README.md)** | **[English](README_EN.md)**

</div>

## What is Aite

Aite is a desktop workspace built around everyday Claude Code usage.

It brings the most common workflows into one interface:

- manage local projects
- create, resume, switch, and search sessions
- review thinking, tool calls, tasks, and sub-agent activity
- approve edits, commands, and plan execution
- attach files, folders, images, and references directly in chat
- configure providers, MCP, skills, commands, and updates

## Highlights

### Multi-session workflow

- independent session management
- fast switching, refresh, and resume support
- automatic session titles
- draft session reuse
- session and project search

### Permission control

- plan, edit, read-only, and bypass modes
- inline approval inside chat
- approval buttons beside tool calls
- session-level always-allow flow

### Better chat experience

- stable streaming output
- grouped messages
- thinking, tool use, tool result, task, sub-agent, and image support
- diff view for edit/write actions
- dedicated AskUserQuestion cards

### File and attachment workflow

- `@` file references in the input box
- folder references and folder attachments
- linked file search while typing
- image upload and generic attachments

### Workspace and extension support

- project tree and inline file editor
- Git status and branch actions
- worktree mode
- MCP server management
- skills and slash commands

### Settings and updates

- Claude CLI argument management
- usage and token statistics
- auto-update flow

## Who is it for

- people who run multiple Claude Code sessions
- people who want better session, context, and permission management
- people who frequently reference files, folders, and images while chatting

## Availability

This project is now public. You can get the source code from the repository and use GitHub Actions artifacts or future releases for builds and updates.

## Screenshots

Product screenshots will be added in a future update.

## Installation Notes

- Available on macOS 11.2+ and Windows 10+
- On first launch, Aite checks Claude Code CLI and required runtime dependencies
- If Node.js or npm is missing, Aite will guide the installation flow when needed
- After updating, restarting the app once is recommended

## FAQ

### Do I need to install Claude Code CLI first

Not always.

If Claude Code CLI is not available on the machine, Aite will detect it and guide the setup flow when required.

### Why does Aite check Node.js or npm on first launch

Because Claude Code CLI depends on that runtime. Aite verifies the environment first so the install flow can continue correctly.

### Can I manage multiple projects and sessions

Yes. Multi-project and multi-session workflow is one of the main goals of Aite.

### Do I need to reconfigure everything after an update

Usually no. Most sessions, projects, and settings remain available, but checking provider and extension status after an update is still recommended.

## More Docs

- [Changelog](CHANGELOG.md)
- [Contributing](CONTRIBUTING.md)
- [Security Policy](SECURITY.md)
- [Auto Update Guide](docs/自动更新接入说明.md)
- [Build Guide](docs/安装包构建说明.md)

## Quick Start

1. Open Aite
2. Import a local project
3. Create a new session or resume an existing one
4. Start chatting and reference files or images when needed
5. Approve permissions, switch modes, and review tool activity in the same window
