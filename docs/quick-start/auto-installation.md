# 自动安装

Aite 内置了 Claude Code 的自动检测与自动安装流程，默认推荐优先走这条路径。

它适合两类情况：

- 你还没有安装 Claude Code
- 你装过，但当前系统环境不完整，Aite 无法正常调用

## 自动检测会做什么

首次启动时，Aite 会尝试完成下面这些检查：

1. 查找 Claude Code CLI
2. 验证 CLI 是否真的能执行 `--version`
3. 检查当前运行环境是否缺少 Node.js / npm
4. Windows 下检查 Git Bash 是否存在

如果 Claude Code 已安装且运行环境完整，Aite 会直接通过检测并进入应用。

## 自动安装流程

### 场景一：系统已经有 Claude Code

如果系统里已经有 Claude Code，Aite 会先尝试直接复用。

这时可能出现两种结果：

- 运行正常：直接通过
- CLI 文件存在，但依赖不完整：继续补齐依赖

### 场景二：系统里没有 Claude Code

如果没有检测到 Claude Code，Aite 会进入安装流程。

当前实现下，Aite 的主要安装方式是：

1. 先准备运行依赖
2. 再通过 `npm` 安装 `@anthropic-ai/claude-code`

## Node.js / npm 的处理方式

Aite 会先检查系统环境里是否已经有可用的 `node` 和 `npm`。

### 如果系统里已有 Node.js / npm

会直接继续安装 Claude Code CLI。

### 如果没有可用的 npm

Aite 会先弹出确认，再安装本地 Node.js 运行时。

这个本地运行时：

- 安装在应用自己的数据目录
- 用于让 Aite 能完成 Claude Code 安装和运行
- 不会覆盖你系统中已有的 Node 配置

## Windows 下的额外处理

Windows 比 macOS 多一步：检查 Git Bash。

如果缺少 Git Bash，Aite 会尝试自动安装 `PortableGit`，然后继续后续流程。

所以在 Windows 下，一次自动安装可能包含三段：

1. 补 Git Bash
2. 补 Node.js 运行时
3. 安装 Claude Code CLI

## 安装过程中的状态提示

当前实现里，安装界面会展示进度和阶段信息，常见阶段包括：

- 正在拉取 Node.js 运行时
- 正在部署 Node.js 运行时
- 正在拉取 Git Bash 运行环境
- 正在部署 PortableGit
- 正在通过 npm 安装 Claude Code CLI
- 正在验证安装结果

这些状态一般用来判断当前卡在了哪一步。

## 自动安装成功后

安装成功后，建议立刻确认：

- Claude Code 版本已显示
- 不再提示缺少 Git Bash 或 Node.js
- 可以创建会话并发消息

## 常见失败原因

### 网络问题

自动安装依赖下载资源与 npm 仓库，网络不稳定时容易失败。

### npm 不可用

系统里没有 `npm`，同时你没有允许 Aite 安装本地 Node.js 运行时。

### Windows 运行依赖缺失

Windows 上常见问题是 Git Bash 不可用，或者 PortableGit 没有完整安装成功。

### PATH 不一致

命令行里可用，但桌面应用检测不到，通常是 GUI 环境拿到的 PATH 与终端不同。

## 自动安装失败后怎么办

建议按下面顺序排查：

1. 重新点击检测或重试安装
2. 如果是 Windows，先确认 Git Bash 是否可用
3. 手动确认 `node`、`npm`、`claude` 是否可执行
4. 必要时改走 [手动安装](/quick-start/manual-installation)
5. 如果 CLI 已装好但 Aite 识别不到，补充 [Claude Code 环境变量](/quick-start/claude-code-env)
