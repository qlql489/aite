import type { ContentBlock, Message } from '../types';

export interface TodoWriteItemState {
  content: string;
  status: string;
  activeForm?: string;
}

export interface TodoWritePanelState {
  todos: TodoWriteItemState[];
  completedCount: number;
  totalCount: number;
  activeLabel: string | null;
}

interface ToolUseBlockLike {
  id?: string;
  name?: string;
  input?: Record<string, unknown>;
  content?: string | Record<string, unknown>;
}

function isToolResultOnlyUserMessage(message: Message): boolean {
  if (message.role !== 'user') {
    return false;
  }

  const blocks = message.contentBlocks || [];
  return blocks.length > 0 && blocks.every((block) => block.type === 'tool_result');
}

function parseToolUseBlock(block: ContentBlock): ToolUseBlockLike | null {
  if (block.type !== 'tool_use') {
    return null;
  }

  const toolBlock = block as ContentBlock & ToolUseBlockLike;
  let parsedContent: Record<string, unknown> | null = null;

  if (typeof toolBlock.content === 'string') {
    try {
      parsedContent = JSON.parse(toolBlock.content) as Record<string, unknown>;
    } catch {
      parsedContent = null;
    }
  } else if (toolBlock.content && typeof toolBlock.content === 'object') {
    parsedContent = toolBlock.content;
  }

  return {
    id: toolBlock.id ?? (typeof parsedContent?.id === 'string' ? parsedContent.id : undefined),
    name: toolBlock.name ?? (typeof parsedContent?.name === 'string' ? parsedContent.name : undefined),
    input: toolBlock.input ?? (parsedContent?.input as Record<string, unknown> | undefined),
  };
}

function normalizeTodoItem(item: unknown): TodoWriteItemState | null {
  if (!item || typeof item !== 'object') {
    return null;
  }

  const raw = item as Record<string, unknown>;
  const content = typeof raw.content === 'string' ? raw.content.trim() : '';

  if (!content) {
    return null;
  }

  return {
    content,
    status: typeof raw.status === 'string' ? raw.status : 'pending',
    activeForm: typeof raw.activeForm === 'string' ? raw.activeForm.trim() : undefined,
  };
}

export function isTodoWriteToolUseBlock(block: ContentBlock): boolean {
  const toolBlock = parseToolUseBlock(block);
  return toolBlock?.name === 'TodoWrite';
}

export function extractTodoWritePanelState(messages: Message[]): TodoWritePanelState | null {
  let lastUserIndex = -1;
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index];
    if (message.role !== 'user') {
      continue;
    }
    if (isToolResultOnlyUserMessage(message)) {
      continue;
    }
    lastUserIndex = index;
    break;
  }
  const scopedMessages = lastUserIndex >= 0 ? messages.slice(lastUserIndex + 1) : messages;

  for (let messageIndex = scopedMessages.length - 1; messageIndex >= 0; messageIndex -= 1) {
    const message = scopedMessages[messageIndex];
    const blocks = message.contentBlocks || [];

    for (let blockIndex = blocks.length - 1; blockIndex >= 0; blockIndex -= 1) {
      const toolBlock = parseToolUseBlock(blocks[blockIndex]);
      if (!toolBlock || toolBlock.name !== 'TodoWrite') {
        continue;
      }

      const todosRaw = Array.isArray(toolBlock.input?.todos) ? toolBlock.input.todos : [];
      const todos = todosRaw
        .map((item) => normalizeTodoItem(item))
        .filter((item): item is TodoWriteItemState => item !== null);

      if (todos.length === 0) {
        continue;
      }

      const completedCount = todos.filter((todo) => todo.status === 'completed').length;
      const activeTodo = todos.find((todo) => todo.status === 'in_progress');
      const activeLabel =
        activeTodo?.activeForm
        || activeTodo?.content
        || (completedCount === todos.length ? '全部完成' : null);

      return {
        todos,
        completedCount,
        totalCount: todos.length,
        activeLabel,
      };
    }
  }

  return null;
}
