/**
 * 消息分组工具函数
 * 将消息列表转换为分组后的 FeedEntry 列表
 * 支持工具消息分组和子代理嵌套
 */

import type { Message, FeedEntry } from '../types';
import { isTodoWriteToolUseBlock } from './todoWrite';

/**
 * 工具项接口
 */
interface ToolItem {
  id: string;
  name: string;
  input: Record<string, unknown>;
  result?: string;
  isError?: boolean;
}

/**
 * 任务信息映射
 */
type TaskInfoMap = Map<string, { description: string; agentType: string }>;

/**
 * 子消息映射
 */
type ChildrenMap = Map<string, Message[]>;

/**
 * 判断消息是否为纯工具消息（仅包含相同类型的 tool_use）
 */
function getToolOnlyName(msg: Message): string | null {
  if (msg.role !== 'assistant') {
    return null;
  }

  const blocks = msg.contentBlocks;
  if (!blocks || blocks.length === 0) {
    return null;
  }

  let toolName: string | null = null;

  for (const block of blocks) {
    if (block.type === 'text') {
      // 后端 ContentBlock 使用 content 字段存储文本内容
      const textBlock = block as { type: 'text'; text?: string; content?: string };
      const text = textBlock.content ?? textBlock.text ?? '';
      if (text.trim()) {
        return null;  // 有非空文本，返回 null
      }
    }
    if (block.type === 'thinking') {
      return null;
    }
    if (block.type === 'tool_use') {
      if (isTodoWriteToolUseBlock(block)) {
        if (toolName === null) {
          toolName = 'TodoWrite';
        } else if (toolName !== 'TodoWrite') {
          return null;
        }
        continue;
      }

      // 尝试从 content 字段解析 tool_use 信息
      let toolBlockName: string | null = null;
      const blockWithContent = block as { type: 'tool_use'; content?: string; name?: string };

      if (blockWithContent.name) {
        toolBlockName = blockWithContent.name;
      } else if (blockWithContent.content && typeof blockWithContent.content === 'string') {
        try {
          const parsed = JSON.parse(blockWithContent.content);
          toolBlockName = parsed.name;
        } catch (e) {
          // 忽略解析错误
        }
      }

      if (toolBlockName) {
        if (toolName === null) {
          toolName = toolBlockName;
        } else if (toolName !== toolBlockName) {
          return null;
        }
      }
    }
  }

  return toolName;
}

/**
 * 从消息中提取工具项列表
 */
function extractToolItems(msg: Message): ToolItem[] {
  const blocks = msg.contentBlocks || [];
  const toolResults = msg.toolResults || {};
  const toolResultErrors = msg.toolResultErrors || {};

  return blocks
    .filter((block) => block.type === 'tool_use')
    .map((block) => {
      // 解析 tool_use 的 content（可能是 JSON 字符串或对象）
      let toolData: { id?: string; name?: string; input?: Record<string, unknown> } = {};

      // 如果 content 是字符串，尝试解析为 JSON
      const blockWithContent = block as unknown as { content?: string | Record<string, unknown> };
      if (typeof blockWithContent.content === 'string') {
        try {
          toolData = JSON.parse(blockWithContent.content);
        } catch (e) {
          // 忽略解析错误
        }
      } else if (blockWithContent.content) {
        // content 可能已经是对象
        toolData = blockWithContent.content as typeof toolData;
      }

      // 如果 block 本身有 id/name/input，优先使用
      const id = (block as { id?: string }).id || toolData.id || '';
      const name = (block as { name?: string }).name || toolData.name || '';
      const input = (block as { input?: Record<string, unknown> }).input || toolData.input || {};

      return {
        id,
        name,
        input,
        result: toolResults[id],
        isError: toolResultErrors[id],
      };
    });
}

/**
 * 获取 FeedEntry 中的 Task tool_use ID 列表
 */
function getTaskIdsFromEntry(entry: FeedEntry): string[] {
  if (entry.kind === 'message') {
    const blocks = entry.msg.contentBlocks || [];
    return blocks
      .filter((block) => block.type === 'tool_use' && (block as { name?: string }).name === 'Task')
      .map((block) => (block as { id: string }).id);
  }

  if (entry.kind === 'tool_msg_group' && entry.toolName === 'Task') {
    return entry.items.map((item) => item.id);
  }

  return [];
}

/**
 * 获取消息的 parent tool use ID（支持两种格式）
 */
function getParentToolUseId(msg: Message): string | null {
  return msg.parentToolUseId || msg.parent_tool_use_id || null;
}

/**
 * 分组连续的相同工具消息
 */
function groupToolMessages(messages: Message[]): FeedEntry[] {
  const entries: FeedEntry[] = [];

  for (const msg of messages) {
    const toolName = getToolOnlyName(msg);

    if (toolName) {
      if (toolName === 'TodoWrite') {
        continue;
      }
      // 每次工具调用都单独显示，不再合并连续同名工具
      entries.push({
        kind: 'tool_msg_group',
        toolName,
        items: extractToolItems(msg),
        firstId: msg.id,
        timestamp: msg.timestamp,
      });
    } else {
      // 普通消息
      entries.push({
        kind: 'message',
        msg,
      });
    }
  }

  return entries;
}

/**
 * 构建包含子代理的 FeedEntry 列表
 */
function buildEntries(
  messages: Message[],
  taskInfo: TaskInfoMap,
  childrenByParent: ChildrenMap
): FeedEntry[] {
  // 首先进行工具消息分组
  const grouped = groupToolMessages(messages);

  const result: FeedEntry[] = [];

  for (const entry of grouped) {
    // 在包含 Task tool_use 的条目后，插入子代理组
    const taskIds = getTaskIdsFromEntry(entry);
    const subagentEntries: FeedEntry[] = [];

    for (const taskId of taskIds) {
      const children = childrenByParent.get(taskId);

      if (children && children.length > 0) {
        const info = taskInfo.get(taskId) || { description: 'Subagent', agentType: '' };

        // 递归构建子代理的子条目
        const childEntries = buildEntries(children, taskInfo, childrenByParent);

        subagentEntries.push({
          kind: 'subagent',
          taskToolUseId: taskId,
          description: info.description,
          agentType: info.agentType,
          children: childEntries,
        });
      }
    }

    const isTaskToolGroupWithChildren =
      entry.kind === 'tool_msg_group'
      && entry.toolName === 'Task'
      && subagentEntries.length > 0;

    if (!isTaskToolGroupWithChildren) {
      result.push(entry);
    }

    result.push(...subagentEntries);
  }

  return result;
}

/**
 * 主函数：将消息列表转换为分组后的 FeedEntry 列表
 */
export function groupMessages(messages: Message[]): FeedEntry[] {
  // 阶段 1：找出所有 Task tool_use 的 ID
  const taskInfo: TaskInfoMap = new Map();

  for (const msg of messages) {
    if (!msg.contentBlocks) continue;

    for (const block of msg.contentBlocks) {
      if (
        block.type === 'tool_use' &&
        (block as { name?: string }).name === 'Task'
      ) {
        const toolBlock = block as {
          id: string;
          input: Record<string, unknown>;
        };
        const input = toolBlock.input;

        taskInfo.set(toolBlock.id, {
          description: String((input as { description?: unknown }).description || 'Subagent'),
          agentType: String((input as { subagent_type?: unknown }).subagent_type || ''),
        });
      }
    }
  }

  // 如果没有 Task tool_use，直接返回工具分组结果
  if (taskInfo.size === 0) {
    return groupToolMessages(messages);
  }

  // 阶段 2：将消息分区为顶级消息和子消息
  const childrenByParent: ChildrenMap = new Map();
  const topLevel: Message[] = [];

  for (const msg of messages) {
    if (getParentToolUseId(msg) && taskInfo.has(getParentToolUseId(msg)!)) {
      // 子消息
      const parentToolUseId = getParentToolUseId(msg)!;
      let arr = childrenByParent.get(parentToolUseId);
      if (!arr) {
        arr = [];
        childrenByParent.set(parentToolUseId, arr);
      }
      arr.push(msg);
    } else {
      // 顶级消息
      topLevel.push(msg);
    }
  }

  // 阶段 3：构建带有子代理嵌套的分组条目
  return buildEntries(topLevel, taskInfo, childrenByParent);
}

/**
 * 格式化时间差
 */
export function formatElapsed(ms: number): string {
  const secs = Math.floor(ms / 1000);
  if (secs < 60) return `${secs}s`;
  const mins = Math.floor(secs / 60);
  return `${mins}m ${secs % 60}s`;
}

/**
 * 格式化 token 数量
 */
export function formatTokens(n: number): string {
  if (n >= 1000) return `${(n / 1000).toFixed(1)}k`;
  return String(n);
}

/**
 * 获取工具显示名称
 */
export function getToolLabel(toolName: string): string {
  // 工具名称到显示名称的映射
  const labelMap: Record<string, string> = {
    'Read': '读取文件',
    'Write': '写入文件',
    'Edit': '编辑文件',
    'Search': '搜索',
    'Bash': '命令',
    'Task': '任务',
    'Glob': '文件匹配',
    'LSP': '代码分析',
    'AttemptCompletion': '完成任务',
  };

  return labelMap[toolName] || toolName;
}

/**
 * 获取工具图标类型
 */
export function getToolIconType(toolName: string): 'file' | 'code' | 'search' | 'terminal' | 'check' | 'default' {
  const iconMap: Record<string, 'file' | 'code' | 'search' | 'terminal' | 'check' | 'default'> = {
    'Read': 'file',
    'Write': 'file',
    'Edit': 'code',
    'Search': 'search',
    'Bash': 'terminal',
    'Glob': 'search',
    'LSP': 'code',
    'AttemptCompletion': 'check',
  };

  return iconMap[toolName] || 'default';
}

/**
 * 获取工具输入预览
 */
export function getToolPreview(toolName: string, input: Record<string, unknown>): string {
  switch (toolName) {
    case 'Read':
    case 'Edit':
      return String((input as { file_path?: string }).file_path || '');
    case 'Write':
      return String((input as { file_path?: string }).file_path || '');
    case 'Bash':
      return String((input as { command?: string }).command || '');
    case 'Search':
      return String((input as { query?: string }).query || '');
    case 'Glob':
      return String((input as { pattern?: string }).pattern || '');
    default:
      return JSON.stringify(input).slice(0, 60);
  }
}
