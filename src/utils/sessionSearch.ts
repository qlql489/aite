import type { ContentBlock, Message, ToolResultBlock } from '../types';

export interface SearchTextSegment {
  text: string;
  matched: boolean;
}

function normalizeSearchValue(value: string): string {
  return value.trim();
}

export function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function pushText(chunks: string[], value: unknown): void {
  if (typeof value !== 'string') return;
  const trimmed = value.trim();
  if (!trimmed) return;
  chunks.push(trimmed);
}

function serializeValue(value: unknown): string {
  try {
    return JSON.stringify(value);
  } catch {
    return '';
  }
}

function parseStructuredBlocks(content: string): ContentBlock[] | null {
  try {
    const parsed = JSON.parse(content);
    return Array.isArray(parsed) ? parsed as ContentBlock[] : null;
  } catch {
    return null;
  }
}

function collectToolResultText(result: ToolResultBlock['content'], chunks: string[]): void {
  if (typeof result === 'string') {
    pushText(chunks, result);
    return;
  }

  result.forEach((item) => pushText(chunks, item.text));
}

function collectBlockText(blocks: ContentBlock[] | undefined, chunks: string[]): void {
  if (!blocks?.length) return;

  blocks.forEach((block) => {
    switch (block.type) {
      case 'text': {
        const textBlock = block as { type: 'text'; text?: string; content?: string };
        pushText(chunks, textBlock.content ?? textBlock.text);
        break;
      }
      case 'thinking': {
        pushText(chunks, block.thinking ?? block.content);
        break;
      }
      case 'tool_use': {
        pushText(chunks, block.name);
        pushText(chunks, typeof block.input?.description === 'string' ? block.input.description : '');
        pushText(chunks, serializeValue(block.input));
        break;
      }
      case 'tool_result': {
        collectToolResultText(block.content, chunks);
        break;
      }
      default:
        break;
    }
  });
}

export function extractSearchableText(message: Message): string {
  const chunks: string[] = [];
  const structuredBlocks = message.contentBlocks?.length
    ? message.contentBlocks
    : parseStructuredBlocks(message.content);

  if (structuredBlocks) {
    collectBlockText(structuredBlocks, chunks);
  } else {
    pushText(chunks, message.content);
  }

  Object.values(message.toolResults || {}).forEach((value) => pushText(chunks, value));

  message.attachments?.forEach((attachment) => {
    pushText(chunks, attachment.name);
    pushText(chunks, attachment.originalPath || attachment.path);
  });

  return chunks.join('\n');
}

export function messageMatchesQuery(message: Message, query: string): boolean {
  const normalizedQuery = normalizeSearchValue(query).toLocaleLowerCase();
  if (!normalizedQuery) return false;

  return extractSearchableText(message).toLocaleLowerCase().includes(normalizedQuery);
}

export function splitTextByQuery(text: string, query: string): SearchTextSegment[] {
  if (!text) return [];

  const normalizedQuery = normalizeSearchValue(query);
  if (!normalizedQuery) {
    return [{ text, matched: false }];
  }

  const matcher = new RegExp(escapeRegExp(normalizedQuery), 'gi');
  const segments: SearchTextSegment[] = [];
  let lastIndex = 0;
  let match: RegExpExecArray | null;

  while ((match = matcher.exec(text)) !== null) {
    if (match.index > lastIndex) {
      segments.push({
        text: text.slice(lastIndex, match.index),
        matched: false,
      });
    }

    segments.push({
      text: match[0],
      matched: true,
    });

    lastIndex = match.index + match[0].length;
    if (match[0].length === 0) {
      matcher.lastIndex += 1;
    }
  }

  if (lastIndex < text.length) {
    segments.push({
      text: text.slice(lastIndex),
      matched: false,
    });
  }

  return segments.length > 0 ? segments : [{ text, matched: false }];
}
