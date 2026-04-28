# 手动安装

如果自动安装失败，或者你本来就希望自己掌控安装过程，可以按这一页手动准备 Claude Code 环境。

## 适用场景

- 自动安装失败
- 你已经有固定的 Node.js / npm 环境
- 你想自己控制 Claude Code 安装位置
- Aite 检测不到 CLI，但你确认它已经安装

## 手动安装 Claude Code

### 第一步：准备 Node.js / npm

先确认系统里已经有可用的 `node` 和 `npm`。

可以在终端中执行：

```bash
node --version
npm --version
```

如果这两个命令都能正常输出版本号，说明基础环境可用。

### 第二步：安装 Claude Code CLI

在终端中执行：

```bash
npm install -g @anthropic-ai/claude-code
```

安装完成后，检查：

```bash
claude --version
```

如果能正常输出版本号，说明 Claude Code CLI 已经可用。

## Windows 手动补依赖

如果你在 Windows 上使用 Aite，除了 Claude Code CLI，还建议确认 Git Bash 可用。

### 方式一：安装 Git for Windows

这是最常见也最稳妥的方案。安装完成后，系统里通常会带上 Git Bash。

### 方式二：使用 Aite 自动补 PortableGit

如果你不想单独安装 Git for Windows，也可以回到 Aite，让它帮你补 `PortableGit`。

## macOS 手动补依赖

macOS 一般不需要 Git Bash，但经常会遇到 GUI 与终端环境不一致的问题。

如果你使用 `nvm`、`fnm`、`volta` 等工具管理 Node.js，建议重点确认：

- `npm` 是否真的在当前登录环境可见
- `claude` 是否在 PATH 中
- Aite 是否需要通过 `CLAUDE_CLI_PATH` 指定路径

## 如果 `claude` 命令能用，但 Aite 检测不到

这通常不是 Claude Code 没装好，而是 Aite 没拿到正确路径。

优先按下面顺序处理：

1. 重启 Aite
2. 确认 `claude --version` 仍然可用
3. 显式配置 `CLAUDE_CLI_PATH`

例如：

```bash
export CLAUDE_CLI_PATH=/your/path/to/claude
```

Windows 可以在系统环境变量中配置对应值，macOS 可以在你的 shell 配置文件中设置。

## 常见排查命令

### macOS / Linux 风格终端

```bash
which node
which npm
which claude
```

### Windows PowerShell

```powershell
where.exe node
where.exe npm
where.exe claude
```

## 回到 Aite 后怎么验证

手动安装完成后，重新打开 Aite，确认：

- 能检测到 Claude Code
- 能看到 CLI 版本
- 不再提示缺少依赖

如果已经安装成功，但仍有模型或认证相关问题，继续看 [Claude Code 环境变量](/quick-start/claude-code-env)。
