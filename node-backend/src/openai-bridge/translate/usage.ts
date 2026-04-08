// Normalized token usage: intermediate representation between OpenAI and Anthropic formats

import type { OpenAIUsage } from '../types/openai.js';
import type { AnthropicUsage } from '../types/anthropic.js';
import type { ResponsesUsage } from '../types/openai-responses.js';

export interface UsageSnapshot {
  inputTokens: number;
  outputTokens: number;
  cacheReadInputTokens: number;
  cacheCreationInputTokens: number;
  reasoningTokens: number;
}

export function emptyUsage(): UsageSnapshot {
  return {
    inputTokens: 0,
    outputTokens: 0,
    cacheReadInputTokens: 0,
    cacheCreationInputTokens: 0,
    reasoningTokens: 0,
  };
}

function normalizeUsage(
  usage: {
    prompt_tokens?: number;
    completion_tokens?: number;
    input_tokens?: number;
    output_tokens?: number;
    cache_read_input_tokens?: number;
    cache_creation_input_tokens?: number;
    prompt_tokens_details?: { cached_tokens?: number };
    input_tokens_details?: { cached_tokens?: number };
    completion_tokens_details?: { reasoning_tokens?: number };
    output_tokens_details?: { reasoning_tokens?: number };
  },
  existing?: UsageSnapshot,
): UsageSnapshot {
  return {
    inputTokens: usage.prompt_tokens ?? usage.input_tokens ?? existing?.inputTokens ?? 0,
    outputTokens: usage.completion_tokens ?? usage.output_tokens ?? existing?.outputTokens ?? 0,
    cacheReadInputTokens:
      usage.prompt_tokens_details?.cached_tokens
      ?? usage.input_tokens_details?.cached_tokens
      ?? usage.cache_read_input_tokens
      ?? existing?.cacheReadInputTokens
      ?? 0,
    cacheCreationInputTokens:
      usage.cache_creation_input_tokens
      ?? existing?.cacheCreationInputTokens
      ?? 0,
    reasoningTokens:
      usage.completion_tokens_details?.reasoning_tokens
      ?? usage.output_tokens_details?.reasoning_tokens
      ?? existing?.reasoningTokens
      ?? 0,
  };
}

/** OpenAI usage → normalized UsageSnapshot */
export function fromOpenAIUsage(usage: OpenAIUsage | null | undefined): UsageSnapshot {
  if (!usage) return emptyUsage();
  return normalizeUsage(usage);
}

/** Responses usage → normalized UsageSnapshot */
export function fromResponsesUsage(usage: ResponsesUsage | null | undefined): UsageSnapshot {
  if (!usage) return emptyUsage();
  return normalizeUsage(usage);
}

/** UsageSnapshot → Anthropic usage format */
export function toAnthropicUsage(snap: UsageSnapshot): AnthropicUsage {
  return {
    input_tokens: snap.inputTokens,
    output_tokens: snap.outputTokens,
    ...(snap.cacheReadInputTokens > 0 ? { cache_read_input_tokens: snap.cacheReadInputTokens } : {}),
    ...(snap.cacheCreationInputTokens > 0 ? { cache_creation_input_tokens: snap.cacheCreationInputTokens } : {}),
  };
}

/** Merge a partial OpenAI usage update into an existing snapshot (for streaming accumulation) */
export function mergeUsage(existing: UsageSnapshot, usage: OpenAIUsage | null | undefined): UsageSnapshot {
  if (!usage) return existing;
  return normalizeUsage(usage, existing);
}
