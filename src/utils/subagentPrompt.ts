import type { FeedEntry, Message } from '../types';

export interface LeadingSubagentPrompt {
  promptText: string;
  hiddenMessageIds: Set<string>;
}

function extractTextFromBlocks(blocks: Array<{ type?: string; text?: string; content?: string }>): string {
  return blocks
    .filter((block) => block.type === 'text')
    .map((block) => (block.content ?? block.text ?? '').trim())
    .filter(Boolean)
    .join('\n');
}

export function extractMessagePlainText(message: Message): string {
  if (message.contentBlocks?.length) {
    const blockText = extractTextFromBlocks(
      message.contentBlocks as Array<{ type?: string; text?: string; content?: string }>,
    );
    if (blockText) return blockText;
  }

  const rawContent = (message.content || '').trim();
  if (!rawContent) return '';

  try {
    const parsed = JSON.parse(rawContent);
    if (Array.isArray(parsed)) {
      const blockText = extractTextFromBlocks(parsed as Array<{ type?: string; text?: string; content?: string }>);
      if (blockText) return blockText;
    }
  } catch {
    // 忽略 JSON 解析失败，回退到原始文本
  }

  return rawContent;
}

export function getLeadingSubagentPrompt(entries: FeedEntry[]): LeadingSubagentPrompt {
  const hiddenMessageIds = new Set<string>();
  let promptText = '';

  for (const entry of entries) {
    if (entry.kind !== 'message') break;

    const text = extractMessagePlainText(entry.msg).trim();
    if (entry.msg.role === 'user') {
      hiddenMessageIds.add(entry.msg.id);
      if (!promptText && text) {
        promptText = text;
      }
      continue;
    }

    if (!text) {
      continue;
    }

    break;
  }

  return {
    promptText,
    hiddenMessageIds,
  };
}
