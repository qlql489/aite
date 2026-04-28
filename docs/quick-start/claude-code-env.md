# Claude Code 环境变量

大多数情况下，Aite 推荐你直接通过图形界面管理供应商、模型和 CLI 参数，而不是自己维护一大堆环境变量。

但在下面这些场景里，环境变量仍然很有用：

- Claude Code 已装好，但 Aite 找不到 CLI
- 你希望显式指定 Claude Code 可执行文件路径
- 你习惯通过环境变量管理 API Key 与 Base URL
- 你要接入非默认供应商或兼容协议服务

## 最常见的变量

### `CLAUDE_CLI_PATH`

这是 Aite 当前最值得优先关注的变量。

用途：

- 显式指定 Claude Code CLI 路径
- 解决“终端能用，Aite 检测不到”的问题

示例：

```bash
export CLAUDE_CLI_PATH=/absolute/path/to/claude
```

如果你安装的是 Windows 下的命令包装文件，也可能指向 `.cmd` 路径。

## Claude / 供应商相关变量

Aite 当前运行时会继承或注入一些 Claude / 供应商相关环境变量，常见包括：

- `ANTHROPIC_API_KEY`
- `ANTHROPIC_AUTH_TOKEN`
- `ANTHROPIC_BASE_URL`
- `ANTHROPIC_MODEL`
- `ANTHROPIC_SMALL_FAST_MODEL`

如果你使用 OpenAI 兼容供应商，Aite 也会通过内部桥接把供应商配置转换成 Claude Code 可用的运行环境。

这意味着对大多数用户来说：

- 不必手写复杂的兼容层变量
- 直接在 Aite 的供应商设置中配置更稳妥

## 推荐做法

### 普通用户

推荐只在这两种情况下自己配环境变量：

1. 配 `CLAUDE_CLI_PATH`
2. 临时用系统环境变量提供 API Key

其余供应商、模型、Base URL、附加环境变量，优先放到 Aite 的供应商管理界面里。

### 高级用户

如果你已经有成熟的命令行工作流，也可以继续沿用系统环境变量管理方式，再让 Aite 继承它们。

## Windows 配置方式

可以在“系统环境变量”或“用户环境变量”中新增变量。

常见做法：

- 新增 `CLAUDE_CLI_PATH`
- 新增模型供应商所需的 Key
- 保存后重新启动 Aite

## macOS 配置方式

可以在你使用的 shell 配置文件里设置，例如：

- `~/.zshrc`
- `~/.bashrc`
- `~/.bash_profile`

示例：

```bash
export CLAUDE_CLI_PATH=/absolute/path/to/claude
export ANTHROPIC_API_KEY=your_key_here
```

修改后建议：

1. 重开一个终端确认变量生效
2. 重启 Aite 让桌面应用重新读取环境

## 关于供应商配置

Aite 的供应商配置页已经支持：

- Base URL
- API Key
- 协议类型
- 模型列表
- 额外环境变量

因此，如果你在 Aite 里配置了供应商，通常不需要再手动设置：

- `ANTHROPIC_BASE_URL`
- `ANTHROPIC_API_KEY`
- 兼容供应商的额外桥接参数

## 变量不生效时怎么排查

### 1. 先区分问题类型

先判断到底是哪一类问题：

- CLI 路径问题
- API Key / 鉴权问题
- Base URL 问题
- GUI 与终端环境不一致

### 2. 优先检查 `CLAUDE_CLI_PATH`

如果 Aite 连 Claude Code 都找不到，优先处理这个变量。

### 3. 检查终端里是否真的可用

```bash
echo $CLAUDE_CLI_PATH
claude --version
```

### 4. 重启 Aite

桌面应用通常不会实时感知你刚改过的 shell 配置，重启应用很重要。

## 一条实用建议

如果你不是强依赖命令行配置的人，建议把“路径问题”交给 `CLAUDE_CLI_PATH`，把“模型供应商问题”交给 Aite 设置页。

这样后续排查会简单很多。
