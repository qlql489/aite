import type { Message, ToolUseBlock } from '../types';

export type RewindAction =
  | 'restore_all'
  | 'restore_conversation'
  | 'restore_code'
  | 'summarize';

export interface RewindCodeChange {
  label: string;
  action: 'edited' | 'created' | 'terminal' | 'tool';
}

export interface RewindTurn {
  index: number;
  messageId: string;
  userContent: string;
  timestamp: number;
  startMsgIdx: number;
  checkpointUuid?: string;
  codeChanges: RewindCodeChange[];
}

export function parseRewindTurns(messages: Message[]): RewindTurn[] {
  const turns: RewindTurn[] = [];
  let turnIndex = 0;

  for (let i = 0; i < messages.length; i += 1) {
    const message = messages[i];
    if (message.role !== 'user') continue;

    turnIndex += 1;
    const nextUserIdx = findNextUserIndex(messages, i + 1);
    const userContent = (message.content || '').trim();

    turns.push({
      index: turnIndex,
      messageId: message.id,
      userContent: userContent.length > 80 ? `${userContent.slice(0, 80)}...` : userContent,
      timestamp: message.timestamp || Date.now(),
      startMsgIdx: i,
      checkpointUuid: message.checkpointUuid,
      codeChanges: extractCodeChanges(messages, i + 1, nextUserIdx),
    });
  }

  return turns;
}

export function buildRewindSummary(messages: Message[], startMsgIdx: number): string {
  const summaryParts: string[] = [];

  for (const message of messages.slice(startMsgIdx)) {
    if (message.role === 'user' && message.content) {
      summaryParts.push(`用户：${truncate(message.content, 200)}`);
      continue;
    }

    if (message.role === 'assistant' && message.content) {
      summaryParts.push(`Claude：${truncate(message.content, 300)}`);
      continue;
    }

    const toolBlocks = (message.contentBlocks || []).filter(
      (block): block is ToolUseBlock => block.type === 'tool_use',
    );

    for (const block of toolBlocks) {
      const toolData = getToolUseData(block);
      const toolName = toolData.name || block.name || '未知工具';
      const filePath = typeof toolData.input?.file_path === 'string'
        ? toolData.input.file_path
        : typeof toolData.input?.command === 'string'
          ? toolData.input.command
          : toolName;
      summaryParts.push(`${toolName}：${truncate(String(filePath), 120)}`);
    }
  }

  return summaryParts.join('\n');
}

export function formatAbsoluteDateTime(timestamp?: number): string {
  if (!timestamp) return '';

  const date = new Date(timestamp);
  const formatter = new Intl.DateTimeFormat('zh-CN', {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  });

  return formatter.format(date).replace(/\//g, '-');
}

export function formatRelativeDateTime(timestamp: number): string {
  const diff = Date.now() - timestamp;
  const secs = Math.floor(diff / 1000);
  if (secs < 60) return `${secs}s 前`;
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}m 前`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h 前`;
  const days = Math.floor(hours / 24);
  return `${days}d 前`;
}

function findNextUserIndex(messages: Message[], startIndex: number): number {
  for (let i = startIndex; i < messages.length; i += 1) {
    if (messages[i].role === 'user') return i;
  }
  return messages.length;
}

function extractCodeChanges(messages: Message[], startIndex: number, endIndex: number): RewindCodeChange[] {
  const changes: RewindCodeChange[] = [];
  const seen = new Set<string>();

  for (let i = startIndex; i < endIndex; i += 1) {
    const message = messages[i];
    const toolBlocks = (message.contentBlocks || []).filter(
      (block): block is ToolUseBlock => block.type === 'tool_use',
    );

    for (const block of toolBlocks) {
      const toolData = getToolUseData(block);
      const input = toolData.input || {};
      const toolName = toolData.name || block.name || '未知工具';

      if (toolName === 'Edit' && typeof input.file_path === 'string') {
        pushChange(changes, seen, input.file_path, 'edited');
      } else if (toolName === 'Write' && typeof input.file_path === 'string') {
        pushChange(changes, seen, input.file_path, 'created');
      } else if (toolName === 'Bash' && typeof input.command === 'string') {
        pushChange(changes, seen, input.command.slice(0, 48), 'terminal');
      } else {
        const genericLabel = typeof input.file_path === 'string'
          ? input.file_path
          : toolName;
        pushChange(changes, seen, genericLabel, 'tool');
      }
    }
  }

  return changes;
}

function pushChange(
  changes: RewindCodeChange[],
  seen: Set<string>,
  label: string | null | undefined,
  action: RewindCodeChange['action'],
) {
  const safeLabel = normalizeLabel(label);
  if (seen.has(safeLabel)) return;
  seen.add(safeLabel);
  changes.push({
    label: shortenPath(safeLabel),
    action,
  });
}

function getToolUseData(block: ToolUseBlock): { name?: string; input?: Record<string, unknown> } {
  const blockWithContent = block as ToolUseBlock & {
    content?: string | { name?: string; input?: Record<string, unknown> };
  };

  if (blockWithContent.content && typeof blockWithContent.content === 'object') {
    return {
      name: block.name || blockWithContent.content.name,
      input: block.input || blockWithContent.content.input,
    };
  }

  if (typeof blockWithContent.content === 'string') {
    try {
      const parsed = JSON.parse(blockWithContent.content) as { name?: string; input?: Record<string, unknown> };
      return {
        name: block.name || parsed.name,
        input: block.input || parsed.input,
      };
    } catch {
      return {
        name: block.name,
        input: block.input,
      };
    }
  }

  return {
    name: block.name,
    input: block.input,
  };
}

function normalizeLabel(label: string | null | undefined): string {
  if (typeof label !== 'string') return '未知工具';
  const trimmed = label.trim();
  return trimmed || '未知工具';
}

function shortenPath(filePath: string): string {
  if (!filePath) return '未知工具';
  const parts = filePath.split(/[\\/]/).filter(Boolean);
  if (parts.length <= 2) return filePath;
  return parts.slice(-2).join('/');
}

function truncate(text: string, maxLength: number): string {
  return text.length > maxLength ? `${text.slice(0, maxLength)}...` : text;
}
