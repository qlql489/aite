/**
 * 消息分组工具函数
 * 将消息列表转换为分组后的 FeedEntry 列表
 * 支持工具消息分组和子代理嵌套
 */

import type { Message, FeedEntry, SubagentRuntimeState } from '../types';
import { isTodoWriteToolUseBlock } from './todoWrite';

const SUBAGENT_PARENT_TOOL_NAMES = new Set(['Task', 'Agent']);

function isGenericSubagentDescription(value: string | undefined): boolean {
  const normalized = (value || '').trim();
  return !normalized || normalized === 'Subagent';
}

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

interface ParsedToolUseBlock {
  id: string;
  name: string;
  input: Record<string, unknown>;
}

/**
 * 任务信息映射
 */
type TaskInfoMap = Map<string, { description: string; agentType: string }>;

/**
 * 子消息映射
 */
type ChildrenMap = Map<string, Message[]>;
type RuntimeMap = Map<string, SubagentRuntimeState>;

function getMessageVisibleText(msg: Message): string {
  const textFromBlocks = (msg.contentBlocks || [])
    .filter((block) => block.type === 'text')
    .map((block) => {
      const typedBlock = block as { text?: string; content?: unknown };
      const blockContent = typeof typedBlock.content === 'string' ? typedBlock.content : '';
      return (typedBlock.text || blockContent || '').trim();
    })
    .filter(Boolean)
    .join('\n');

  return (textFromBlocks || msg.content || '').trim();
}

function entryHasAssistantFinalResult(entry: FeedEntry): boolean {
  if (entry.kind === 'message') {
    return entry.msg.role === 'assistant' && Boolean(getMessageVisibleText(entry.msg));
  }

  if (entry.kind === 'subagent') {
    return entry.children.some(entryHasAssistantFinalResult);
  }

  return false;
}

function entriesHaveAssistantFinalResult(entries: FeedEntry[]): boolean {
  return entries.some(entryHasAssistantFinalResult);
}

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
      const parsed = parseToolUseBlock(block);
      const id = parsed?.id || '';
      const name = parsed?.name || '';
      const input = parsed?.input || {};

      return {
        id,
        name,
        input,
        result: toolResults[id],
        isError: toolResultErrors[id],
      };
    });
}

function parseToolUseBlock(block: unknown): ParsedToolUseBlock | null {
  if (!block || typeof block !== 'object') return null;

  const typedBlock = block as {
    id?: string;
    name?: string;
    input?: Record<string, unknown>;
    content?: string | Record<string, unknown>;
  };

  let contentData: { id?: string; name?: string; input?: Record<string, unknown> } = {};

  if (typeof typedBlock.content === 'string') {
    try {
      contentData = JSON.parse(typedBlock.content);
    } catch {
      contentData = {};
    }
  } else if (typedBlock.content && typeof typedBlock.content === 'object') {
    contentData = typedBlock.content as typeof contentData;
  }

  return {
    id: typedBlock.id || contentData.id || '',
    name: typedBlock.name || contentData.name || '',
    input: typedBlock.input || contentData.input || {},
  };
}

/**
 * 获取 FeedEntry 中的 Task tool_use ID 列表
 */
function getTaskIdsFromEntry(entry: FeedEntry): string[] {
  if (entry.kind === 'message') {
    const blocks = entry.msg.contentBlocks || [];
    return blocks
      .filter((block) => {
        if (block.type !== 'tool_use') return false;
        const parsed = parseToolUseBlock(block);
        return !!parsed && SUBAGENT_PARENT_TOOL_NAMES.has(parsed.name);
      })
      .map((block) => parseToolUseBlock(block)?.id || '')
      .filter(Boolean);
  }

  if (entry.kind === 'tool_msg_group' && SUBAGENT_PARENT_TOOL_NAMES.has(entry.toolName)) {
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
function groupToolMessages(messages: Message[], runtimeByTask: RuntimeMap): FeedEntry[] {
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
        taskRuntimeSummary: SUBAGENT_PARENT_TOOL_NAMES.has(toolName)
          ? (() => {
            const taskId = extractToolItems(msg)[0]?.id;
            const runtime = taskId ? runtimeByTask.get(taskId) : undefined;
            if (!runtime) return undefined;
            return {
              status: runtime.status,
              elapsedMs: runtime.startedAt
                ? Math.max(0, (runtime.completedAt ?? Date.now()) - runtime.startedAt)
                : undefined,
              toolCallCount: runtime.toolCallCount,
              latestPreview: runtime.latestPreview,
            };
          })()
          : undefined,
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
function dedupeLiveCallEntries(entries: FeedEntry[], runtime: SubagentRuntimeState | undefined): FeedEntry[] {
  if (!runtime || runtime.calls.length === 0) {
    return entries;
  }

  const liveCallIds = new Set(runtime.calls.map((call) => call.id));

  return entries.filter((entry) => {
    if (entry.kind !== 'tool_msg_group') return true;
    if (entry.items.length === 0) return true;
    return !entry.items.every((item) => liveCallIds.has(item.id));
  });
}

function buildEntries(
  messages: Message[],
  taskInfo: TaskInfoMap,
  childrenByParent: ChildrenMap,
  runtimeByTask: RuntimeMap,
): FeedEntry[] {
  // 首先进行工具消息分组
  const grouped = groupToolMessages(messages, runtimeByTask);

  const result: FeedEntry[] = [];

  for (const entry of grouped) {
    // 在包含 Task tool_use 的条目后，插入子代理组
    const taskIds = getTaskIdsFromEntry(entry);
    const subagentEntries: FeedEntry[] = [];

    for (const taskId of taskIds) {
      const children = childrenByParent.get(taskId);
      const runtime = runtimeByTask.get(taskId);

      if ((children && children.length > 0) || runtime) {
        const info = taskInfo.get(taskId) || { description: 'Subagent', agentType: '' };

        // 递归构建子代理的子条目
        const childEntries = children
          ? dedupeLiveCallEntries(
            buildEntries(children, taskInfo, childrenByParent, runtimeByTask),
            runtime,
          )
          : [];
        const hasFinalResult = entriesHaveAssistantFinalResult(childEntries);

        subagentEntries.push({
          kind: 'subagent',
          taskToolUseId: taskId,
          description: isGenericSubagentDescription(runtime?.description)
            ? info.description
            : (runtime?.description || info.description),
          agentType: runtime?.agentType || info.agentType,
          children: childEntries,
          liveCalls: runtime?.calls,
          status: hasFinalResult ? 'completed' : runtime?.status,
          startedAt: runtime?.startedAt,
          completedAt: runtime?.completedAt,
          latestPreview: runtime?.latestPreview,
          toolCallCount: runtime?.toolCallCount,
        });
      }
    }

    if (entry.kind === 'tool_msg_group' && subagentEntries.length > 0) {
      const firstSubagentWithFinalResult = subagentEntries.find((item) => (
        item.kind === 'subagent' && entriesHaveAssistantFinalResult(item.children)
      ));
      const taskRuntimeSummary = firstSubagentWithFinalResult && entry.taskRuntimeSummary
        ? { ...entry.taskRuntimeSummary, status: 'completed' as const }
        : entry.taskRuntimeSummary;

      result.push({
        ...entry,
        taskRuntimeSummary,
        subagentGroups: subagentEntries.filter((item): item is Extract<FeedEntry, { kind: 'subagent' }> => item.kind === 'subagent'),
      });
      continue;
    }

    result.push(entry);
    result.push(...subagentEntries);
  }

  return result;
}

/**
 * 主函数：将消息列表转换为分组后的 FeedEntry 列表
 */
export function groupMessages(
  messages: Message[],
  runtimeByTask: RuntimeMap = new Map(),
): FeedEntry[] {
  // 阶段 1：找出所有 Task tool_use 的 ID
  const taskInfo: TaskInfoMap = new Map();

  for (const msg of messages) {
    if (!msg.contentBlocks) continue;

    for (const block of msg.contentBlocks) {
      if (block.type === 'tool_use') {
        const parsed = parseToolUseBlock(block);
        if (!parsed || !SUBAGENT_PARENT_TOOL_NAMES.has(parsed.name) || !parsed.id) {
          continue;
        }
        const input = parsed.input;

        taskInfo.set(parsed.id, {
          description: String((input as { description?: unknown }).description || 'Subagent'),
          agentType: String((input as { subagent_type?: unknown }).subagent_type || ''),
        });
      }
    }
  }

  // 如果没有 Task tool_use，直接返回工具分组结果
  if (taskInfo.size === 0) {
    return groupToolMessages(messages, runtimeByTask);
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
  return buildEntries(topLevel, taskInfo, childrenByParent, runtimeByTask);
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
