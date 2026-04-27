<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
import MarkdownContent from './MarkdownContent.vue';
import ThinkingBlock from './ThinkingBlock.vue';
import ToolCallBlock from './ToolCallBlock.vue';
import { useClaudeStore } from '../../stores/claude';
import { useStatsStore } from '../../stores/stats';
import type { ContentBlock, Message } from '../../types';
import { isTodoWriteToolUseBlock } from '../../utils/todoWrite';
import { formatAbsoluteDateTime, formatRelativeDateTime, type RewindAction, type RewindTurn } from '../../utils/rewind';
import { splitTextByQuery } from '../../utils/sessionSearch';

const claudeStore = useClaudeStore();
const statsStore = useStatsStore();

interface Props {
  message: Message;
  showTime?: boolean;
  isLastAssistantMessage?: boolean;
  blockCornerStyle?: 'none' | 'top' | 'bottom' | 'all';
  rewindTurn?: RewindTurn | null;
  rewindBusy?: boolean;
  searchQuery?: string;
  isActiveSearchMatch?: boolean;
}

interface Emits {
  (e: 'copy', content: string): void;
  (e: 'regenerate', messageId: string): void;
  (e: 'rewind', payload: { turn: RewindTurn; action: RewindAction }): void;
}

interface InlineUserFileReference {
  token: string;
  fullPath: string;
  displayName: string;
}

const emit = defineEmits<Emits>();

const props = withDefaults(defineProps<Props>(), {
  showTime: false,
  isLastAssistantMessage: false,
  blockCornerStyle: 'all',
  rewindTurn: null,
  rewindBusy: false,
  searchQuery: '',
  isActiveSearchMatch: false,
});

// 流式内容（用于实时更新）
const streamingContent = ref('');

// 格式化时间显示（精确到秒）
const formattedTime = computed(() => {
  if (!props.message.timestamp) return '';
  return formatAbsoluteDateTime(props.message.timestamp);
});

// 判断内容是否为 JSON（包含 content blocks）
const isStructuredContent = computed(() => {
  try {
    const parsed = JSON.parse(props.message.content);
    if (typeof parsed === 'string') {
      const reparsed = JSON.parse(parsed);
      return Array.isArray(reparsed) || typeof reparsed === 'object';
    }
    return Array.isArray(parsed) || typeof parsed === 'object';
  } catch {
    if (props.message.content.includes('\\"')) {
      try {
        const normalized = props.message.content
          .replace(/\\"/g, '"')
          .replace(/\\n/g, '\n')
          .replace(/\\t/g, '\t');
        const reparsed = JSON.parse(normalized);
        return Array.isArray(reparsed) || typeof reparsed === 'object';
      } catch {
        // ignore
      }
    }
    return false;
  }
});

// 解析结构化内容
const structuredContent = computed(() => {
  if (!isStructuredContent.value) return null;
  try {
    const parsed = JSON.parse(props.message.content);
    if (typeof parsed === 'string') {
      return JSON.parse(parsed);
    }
    return parsed;
  } catch {
    try {
      const normalized = props.message.content
        .replace(/\\"/g, '"')
        .replace(/\\n/g, '\n')
        .replace(/\\t/g, '\t');
      return JSON.parse(normalized);
    } catch {
      return null;
    }
  }
});

// 提取纯文本内容
const textContent = computed(() => {
  if (props.message.isStreaming && streamingContent.value) {
    return streamingContent.value;
  }

  if (!isStructuredContent.value) return props.message.content;

  const blocks = structuredContent.value;
  if (!Array.isArray(blocks)) return props.message.content;

  // 首先尝试提取 text 类型的块
  const textParts = blocks
    .filter((block: ContentBlock) => block.type === 'text')
    .map((block: ContentBlock) => {
      // 后端 ContentBlock 使用 content 字段存储文本内容
      // 兼容处理：优先使用 content 字段
      const b = block as { type: 'text'; text?: string; content?: string };
      return b.content ?? b.text ?? '';
    })
    .filter(Boolean); // 过滤掉空字符串

  if (textParts.length > 0) {
    return textParts.join('\n');
  }

  // 如果没有 text 块，返回空（不显示其他类型的块内容）
  return '';
});

// 提取内容块（用于结构化渲染）
const contentBlocks = computed(() => {
  if (!props.message.contentBlocks) {
    // 尝试从 content 解析
    if (isStructuredContent.value && structuredContent.value) {
      return structuredContent.value as ContentBlock[];
    }
    return [];
  }
  return props.message.contentBlocks;
});

// 按照 contentBlocks 顺序渲染的块列表
const renderBlocks = computed(() => {
  const blocks: Array<{
    type: 'thinking' | 'tool_use';
    data: any;
  }> = [];

  for (const block of contentBlocks.value) {
    if (block.type === 'thinking') {
      if (claudeStore.showThinking) {
        blocks.push({
          type: 'thinking',
          data: block,
        });
      }
    } else if (block.type === 'tool_use') {
      if (isTodoWriteToolUseBlock(block)) {
        continue;
      }
      blocks.push({
        type: 'tool_use',
        data: block,
      });
    }
  }

  return blocks;
});

const getRenderBlockCornerStyle = (index: number): 'none' | 'top' | 'bottom' | 'all' => {
  const hasPrevBlock = index > 0;
  const hasNextBlock = index < renderBlocks.value.length - 1;
  const connectsToPrevEntry =
    !textContent.value && (props.blockCornerStyle === 'bottom' || props.blockCornerStyle === 'none');
  const connectsToNextEntry =
    !textContent.value && (props.blockCornerStyle === 'top' || props.blockCornerStyle === 'none');

  const hasPrevConnection = hasPrevBlock || connectsToPrevEntry;
  const hasNextConnection = hasNextBlock || connectsToNextEntry;

  if (hasPrevConnection && hasNextConnection) return 'none';
  if (hasPrevConnection) return 'bottom';
  if (hasNextConnection) return 'top';
  return 'all';
};

// 获取思考块内容（用于兼容旧代码）
const thinkingBlockContent = (block: any): string => {
  if (block && block.type === 'thinking') {
    // 优先使用 thinking 字段（后端推送格式）
    const thinkingBlock = block as { type: 'thinking'; thinking?: string; content?: string };
    return thinkingBlock.thinking || thinkingBlock.content || '';
  }
  return '';
};

// 获取当前项目路径（优先使用会话的 cwd，其次使用选中的项目路径）
const currentCwd = computed(() => {
  return claudeStore.currentSession?.cwd || statsStore.selectedProjectPath || '';
});


// 获取工具调用状态
const getToolStatus = computed(() => {
  return (toolId: string) => {
    // 如果有结果，检查是否错误
    if (props.message.toolResults?.[toolId] !== undefined) {
      // 优先使用 toolResultErrors 字段
      if (props.message.toolResultErrors?.[toolId]) {
        return 'error' as const;
      }
      return 'success' as const;
    }
    // 否则认为是 running 状态
    return 'running' as const;
  };
});

// 获取工具结果
const getToolResult = computed(() => {
  return (toolId: string) => {
    return props.message.toolResults?.[toolId];
  };
});

// 获取工具结果是否错误
const getToolResultIsError = computed(() => {
  return (toolId: string) => {
    // 优先使用 toolResultErrors 字段
    if (props.message.toolResultErrors?.[toolId] !== undefined) {
      return props.message.toolResultErrors[toolId];
    }
    // 后备方案：通过内容判断
    const result = props.message.toolResults?.[toolId];
    if (result === undefined) return false;
    return result.includes('Error') || result.includes('错误') || result.includes('failed');
  };
});


// 监听消息变化，更新流式内容
watch(() => props.message.content, (newContent) => {
  if (props.message.isStreaming) {
    streamingContent.value = newContent;
  }
}, { immediate: true });

// 监听 isStreaming 变化，确保流式结束时内容同步
watch(() => props.message.isStreaming, (isStreaming) => {
  if (!isStreaming) {
    // 流式结束时，清空 streamingContent 确保使用最新的 message.content
    streamingContent.value = '';
  }
});

// ========== Token 使用量 ==========

// 获取 token 使用数据（兼容 tokenUsage 和 usage 两种字段名）
const tokenUsageData = computed(() => {
  return props.message.tokenUsage || props.message.usage;
});

// 是否显示 token 使用量（使用消息上的 showTokenUsage 标记）
const shouldShowTokenUsage = computed(() => {
  return props.message.role === 'assistant' &&
         props.message.showTokenUsage &&
         tokenUsageData.value !== undefined &&
         (tokenUsageData.value.inputTokens > 0 || tokenUsageData.value.outputTokens > 0);
});

// 格式化 token 数字
const formatTokenNumber = (num: number): string => {
  if (num >= 1000000) return (num / 1000000).toFixed(1) + 'M';
  if (num >= 1000) return (num / 1000).toFixed(1) + 'K';
  return num.toString();
};

// Cache token 总数
const cacheTokens = computed(() => {
  const data = tokenUsageData.value;
  if (!data) return 0;
  return (data.cacheReadInputTokens || 0) + (data.cacheCreationInputTokens || 0);
});

const canShowAssistantToolbar = computed(() => {
  return props.message.role === 'assistant' && !!textContent.value && !props.message.isStreaming;
});

const canShowLatestAssistantActions = computed(() => {
  return canShowAssistantToolbar.value && props.isLastAssistantMessage;
});

const visibleUserAttachments = computed(() => (
  (props.message.attachments || []).filter(att => att.isImage)
));

const inlineUserFileReferences = computed<InlineUserFileReference[]>(() => (
  (props.message.attachments || [])
    .filter((att) => !att.isImage && !!att.path)
    .map((att): InlineUserFileReference => ({
      token: `@${att.path}`,
      fullPath: att.originalPath || att.path,
      displayName: getDisplayFileName(att.path, att.name),
    }))
    .sort((a, b) => b.token.length - a.token.length)
));

const getDisplayFileName = (path: string, fallback?: string) => {
  const normalizedPath = (path || '').replace(/\\/g, '/').replace(/\/+$/, '');
  if (!normalizedPath) return fallback || path;

  const segments = normalizedPath.split('/').filter(Boolean);
  return segments[segments.length - 1] || fallback || path;
};

const looksLikeInlineFileReferenceToken = (token: string) => {
  const body = token.startsWith('@') ? token.slice(1) : token;
  if (!body) return false;

  const normalized = body.replace(/[),.;:!?]+$/, '');
  if (!normalized) return false;

  // 兜底识别要尽量保守，避免把 Java 注解/日志对象误判成文件路径。
  if (/["'`()\[\]{}<>]/.test(normalized)) {
    return false;
  }

  return /[\\/]/.test(normalized)
    || /^(\.{1,2}[\\/]|~[\\/])/.test(normalized)
    || /^[A-Za-z]:[\\/]/.test(normalized)
    || /(^|[\\/])[^\\/]+\.[A-Za-z0-9_-]{1,16}$/.test(normalized);
};

const userTextSegments = computed(() => {
  if (props.message.role !== 'user') {
    return [{ type: 'text' as const, content: textContent.value }];
  }

  const segments: Array<
    | { type: 'text'; content: string }
    | { type: 'file'; content: string; fullPath: string; displayName: string }
  > = [];
  const source = textContent.value || '';
  const knownReferences = inlineUserFileReferences.value;

  if (knownReferences.length === 0) {
    const fileReferencePattern = /(^|\s)(@[^\s]+)/g;
    let lastIndex = 0;
    let match: RegExpExecArray | null;

    while ((match = fileReferencePattern.exec(source)) !== null) {
      const matchStart = match.index;
      const leadingWhitespace = match[1];
      const reference = match[2];
      const referenceStart = matchStart + leadingWhitespace.length;

      if (!looksLikeInlineFileReferenceToken(reference)) {
        continue;
      }

      if (matchStart > lastIndex) {
        segments.push({
          type: 'text',
          content: source.slice(lastIndex, matchStart),
        });
      }

      if (leadingWhitespace) {
        segments.push({
          type: 'text',
          content: leadingWhitespace,
        });
      }

      const fullPath = reference.slice(1);
      segments.push({
        type: 'file',
        content: reference,
        fullPath,
        displayName: getDisplayFileName(fullPath),
      });

      lastIndex = referenceStart + reference.length;
      if (match[0].length === 0) {
        fileReferencePattern.lastIndex += 1;
      }
    }

    if (lastIndex < source.length) {
      segments.push({
        type: 'text',
        content: source.slice(lastIndex),
      });
    }

    return segments.length > 0 ? segments : [{ type: 'text' as const, content: source }];
  }

  let cursor = 0;

  while (cursor < source.length) {
    let nextMatch: { index: number; reference: InlineUserFileReference } | null = null;

    for (const reference of knownReferences) {
      const index = source.indexOf(reference.token, cursor);
      if (index === -1) continue;

      if (!nextMatch || index < nextMatch.index || (index === nextMatch.index && reference.token.length > nextMatch.reference.token.length)) {
        nextMatch = { index, reference };
      }
    }

    if (!nextMatch) {
      break;
    }

    const resolvedMatch = nextMatch;

    if (resolvedMatch.index > cursor) {
      segments.push({
        type: 'text',
        content: source.slice(cursor, resolvedMatch.index),
      });
    }

    segments.push({
      type: 'file',
      content: resolvedMatch.reference.token,
      fullPath: resolvedMatch.reference.fullPath,
      displayName: resolvedMatch.reference.displayName,
    });

    cursor = resolvedMatch.index + resolvedMatch.reference.token.length;
  }

  if (cursor < source.length) {
    segments.push({
      type: 'text',
      content: source.slice(cursor),
    });
  }

  return segments.length > 0 ? segments : [{ type: 'text' as const, content: source }];
});

const isBlockOnlyAssistantMessage = computed(() => {
  return props.message.role === 'assistant'
    && !textContent.value
    && renderBlocks.value.length > 0
    && !props.message.attachments?.length
    && !props.message.images?.length;
});

const messageWrapperClasses = computed(() => [
  'message-wrapper',
  `${props.message.role}-message`,
  {
    'search-match-active': props.isActiveSearchMatch,
    'block-stack-connected-top': isBlockOnlyAssistantMessage.value
      && (props.blockCornerStyle === 'bottom' || props.blockCornerStyle === 'none'),
    'block-stack-connected-bottom': isBlockOnlyAssistantMessage.value
      && (props.blockCornerStyle === 'top' || props.blockCornerStyle === 'none'),
  },
]);

const highlightText = (text: string) => splitTextByQuery(text, props.searchQuery);

const showUserRewindPanel = ref(false);
const rewindAnchorRef = ref<HTMLElement | null>(null);

const rewindButtonDisabled = computed(() => (
  !props.rewindTurn
  || props.rewindBusy
  || props.message.isStreaming
));

const rewindActions = computed(() => {
  const hasCheckpoint = !!props.rewindTurn?.checkpointUuid;
  return [
    { label: '恢复代码和对话', action: 'restore_all' as RewindAction, disabled: !hasCheckpoint },
    { label: '仅恢复对话', action: 'restore_conversation' as RewindAction, disabled: false },
    { label: '仅恢复代码', action: 'restore_code' as RewindAction, disabled: !hasCheckpoint },
  ];
});

const copied = ref(false);
let copiedTimer: ReturnType<typeof setTimeout> | null = null;

onUnmounted(() => {
  if (copiedTimer) {
    clearTimeout(copiedTimer);
  }

  document.removeEventListener('pointerdown', handleDocumentPointerDown);
});

onMounted(() => {
  document.addEventListener('pointerdown', handleDocumentPointerDown);
});

async function handleCopy() {
  if (!textContent.value) return;

  try {
    await navigator.clipboard.writeText(textContent.value);
    emit('copy', textContent.value);
  } catch (error) {
    console.error('[COPY] 复制 AI 回复失败:', error);
  }

  copied.value = true;
  if (copiedTimer) clearTimeout(copiedTimer);
  copiedTimer = setTimeout(() => {
    copied.value = false;
  }, 1500);
}

function handleRegenerate() {
  emit('regenerate', props.message.id);
}

function toggleUserRewindPanel() {
  if (rewindButtonDisabled.value) return;
  showUserRewindPanel.value = !showUserRewindPanel.value;
}

function handleDocumentPointerDown(event: PointerEvent) {
  if (!showUserRewindPanel.value) return;
  const target = event.target as Node | null;
  if (!target) return;
  if (rewindAnchorRef.value?.contains(target)) return;
  showUserRewindPanel.value = false;
}

function applyRewind(action: RewindAction) {
  if (!props.rewindTurn) return;
  emit('rewind', { turn: props.rewindTurn, action });
  showUserRewindPanel.value = false;
}

watch(() => props.rewindTurn?.messageId, () => {
  showUserRewindPanel.value = false;
});
</script>

<template>
  <div :class="messageWrapperClasses" :data-message-id="message.id">
    <div v-if="showTime && formattedTime" class="message-timestamp">{{ formattedTime }}</div>

    <div :class="['message', message.role]">
      <!-- 用户消息 -->
      <div v-if="message.role === 'user'" class="message-content user-message-shell">
        <div class="user-content">
          <div class="message-text">
            <template v-for="(segment, index) in userTextSegments" :key="`${message.id}-segment-${index}`">
              <template v-if="segment.type === 'text'">
                <template v-for="(piece, pieceIndex) in highlightText(segment.content)" :key="`${message.id}-segment-${index}-piece-${pieceIndex}`">
                  <mark
                    v-if="piece.matched"
                    class="session-search-hit"
                    :class="{ active: isActiveSearchMatch }"
                  >
                    {{ piece.text }}
                  </mark>
                  <span v-else>{{ piece.text }}</span>
                </template>
              </template>
              <span
                v-else
                class="inline-file-chip"
                :title="segment.fullPath"
              >
                {{ segment.displayName }}
              </span>
            </template>
          </div>
          <!-- 文件附件 -->
          <div v-if="visibleUserAttachments.length > 0" class="message-attachments">
            <div
              v-for="(att, idx) in visibleUserAttachments"
              :key="idx"
              class="attachment-chip"
              :title="att.originalPath || att.path"
            >
              <!-- 图片附件显示缩略图 -->
              <div v-if="att.isImage && att.preview" class="attachment-thumb">
                <img :src="att.preview" alt="" />
              </div>
              <!-- 非图片附件显示文件图标 -->
              <div v-else class="attachment-icon">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
                  <polyline points="14 2 14 8 20 8"/>
                </svg>
              </div>
              <span class="attachment-name">{{ getDisplayFileName(att.path, att.name) }}</span>
            </div>
          </div>
        </div>

        <div v-if="rewindTurn" ref="rewindAnchorRef" class="user-rewind-anchor">
          <div class="user-message-footer">
            <button
              type="button"
              class="user-message-action"
              :class="{ active: showUserRewindPanel }"
              :disabled="rewindButtonDisabled"
              @click="toggleUserRewindPanel"
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M9 14 4 9l5-5" />
                <path d="M4 9h11a5 5 0 1 1 0 10h-1" />
              </svg>
              <span>回退</span>
            </button>
          </div>

          <div v-if="showUserRewindPanel" class="user-rewind-panel">
            <div class="user-rewind-header">
              <div class="user-rewind-title">回退到第 {{ rewindTurn.index }} 轮</div>
              <div class="user-rewind-subtitle">{{ formatRelativeDateTime(rewindTurn.timestamp) }}</div>
            </div>

            <div v-if="rewindTurn.codeChanges.length > 0" class="user-rewind-changes">
              <span
                v-for="change in rewindTurn.codeChanges.slice(0, 4)"
                :key="`${rewindTurn.messageId}-${change.label}`"
                class="user-rewind-chip"
              >
                {{ change.label }}
              </span>
            </div>

            <div class="user-rewind-actions">
              <button
                v-for="item in rewindActions"
                :key="item.action"
                type="button"
                class="user-rewind-action-btn"
                :class="{ primary: item.action === 'restore_conversation' }"
                :disabled="item.disabled || rewindBusy"
                @click="applyRewind(item.action)"
              >
                {{ item.label }}
              </button>
            </div>
          </div>
        </div>
      </div>

      <!-- 助手消息 -->
      <div v-else-if="message.role === 'assistant'" class="message-content assistant-content">
        <div class="assistant-content-inner">
          <div class="assistant-main-content">
            <!-- 按照 contentBlocks 顺序渲染思考块和工具调用 -->
            <template v-for="(block, index) in renderBlocks" :key="`block-${index}`">
              <!-- 思考块 -->
              <ThinkingBlock
                v-if="block.type === 'thinking'"
                :content="thinkingBlockContent(block.data)"
                :collapsed="true"
                :corner-style="getRenderBlockCornerStyle(index)"
              />

              <!-- 工具调用 -->
              <div v-if="block.type === 'tool_use'" class="tool-calls-container">
                <ToolCallBlock
                  :name="block.data.name"
                  :input="block.data.input"
                  :description="block.data.input?.description"
                  :result="getToolResult(block.data.id)"
                  :status="getToolStatus(block.data.id)"
                  :is-error="getToolResultIsError(block.data.id)"
                  :requestId="block.data.id"
                  :cwd="currentCwd"
                  :corner-style="getRenderBlockCornerStyle(index)"
                />
              </div>
            </template>

            <!-- 文本内容 -->
            <div v-if="textContent" class="text-content-wrapper">
              <!-- Markdown 渲染 -->
              <MarkdownContent
                :content="textContent"
                :is-streaming="message.isStreaming"
                :search-query="searchQuery"
                :active-search-match="isActiveSearchMatch"
              />

              <div v-if="canShowAssistantToolbar || shouldShowTokenUsage" class="assistant-toolbar">
                <button v-if="canShowLatestAssistantActions" class="assistant-toolbar-btn" type="button" @click="handleRegenerate">
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <path d="M3 12a9 9 0 1 0 3-6.7" />
                    <path d="M3 3v6h6" />
                  </svg>
                  <span>重新生成</span>
                </button>

                <button v-if="canShowAssistantToolbar" class="assistant-toolbar-btn assistant-toolbar-btn-icon" type="button" @click="handleCopy" :title="copied ? '已复制' : '复制'" :aria-label="copied ? '已复制' : '复制'">
                  <svg v-if="copied" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <path d="M20 6 9 17l-5-5" />
                  </svg>
                  <svg v-else width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
                    <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
                  </svg>
                </button>

                <div v-if="shouldShowTokenUsage" class="token-usage-badge">
                  <span class="token-value">
                    ↑ {{ formatTokenNumber(tokenUsageData!.inputTokens) }}
                    <span v-if="cacheTokens > 0" class="token-cache">cache {{ formatTokenNumber(cacheTokens) }}</span>
                  </span>
                  <span class="token-value">↓ {{ formatTokenNumber(tokenUsageData!.outputTokens) }}</span>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- 系统消息 -->
      <div v-else-if="message.role === 'system'" class="message-content system-content">
        <div class="system-message-card">
          <span class="system-message-text">
            <template v-for="(piece, pieceIndex) in highlightText(textContent)" :key="`${message.id}-system-${pieceIndex}`">
              <mark
                v-if="piece.matched"
                class="session-search-hit"
                :class="{ active: isActiveSearchMatch }"
              >
                {{ piece.text }}
              </mark>
              <span v-else>{{ piece.text }}</span>
            </template>
          </span>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.message-wrapper {
  display: flex;
  flex-direction: column;
  margin-bottom: 0;
  --chat-font-size: var(--chat-font-size-px, 14px);
}

.message-wrapper.block-stack-connected-top {
  margin-top: -1px;
}

.message-wrapper.block-stack-connected-bottom {
  margin-bottom: -1px;
}

.message-wrapper.user-message + .message-wrapper.assistant-message {
  margin-top: 0.5rem;
}

.message-wrapper.search-match-active {
  position: relative;
}

.message-wrapper.search-match-active::after {
  content: '';
  position: absolute;
  inset: 0.15rem -0.25rem;
  border: 1px solid rgba(249, 115, 22, 0.28);
  border-radius: 1rem;
  pointer-events: none;
}

.message-timestamp {
  font-size: 0.75rem;
  color: var(--text-muted, #9ca3af);
  text-align: center;
  margin-bottom: 0;
}

.message {
  display: flex;
  animation: fadeIn 0.2s ease-in;
}

.message.user {
  justify-content: flex-end;
}

.message.assistant {
  justify-content: flex-start;
  flex-wrap: wrap;
  align-items: flex-start;
  width: 100%;
}

.message.system {
  justify-content: flex-start;
  width: 100%;
}

.message-content {
  max-width: 80%;
}

.user-message-shell {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  width: fit-content;
  max-width: min(100%, 80%);
  margin-left: auto;
  gap: 0.5rem;
}

.user-content {
  padding: 0.75rem 1rem;
  border-radius: 0.75rem;
  background-color: var(--primary-color, #3b82f6);
  color: #ffffff;
  max-width: 100%;
}

.user-content .message-text {
  font-size: var(--chat-font-size);
  line-height: 1.65;
  color: #ffffff;
  white-space: pre-wrap;
  word-break: break-word;
}

.inline-file-chip {
  display: inline-flex;
  align-items: center;
  max-width: min(100%, 220px);
  margin: 0 0.12rem;
  padding: 0.18rem 0.52rem;
  border-radius: 0.6rem;
  background: linear-gradient(180deg, rgba(250, 244, 232, 0.98), rgba(241, 232, 214, 0.94));
  border: 1px solid rgba(159, 128, 92, 0.26);
  color: #5c4630;
  font-family: 'Monaco', 'Menlo', monospace;
  font-size: 0.92em;
  font-weight: 600;
  line-height: 1.25;
  vertical-align: baseline;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.55), 0 1px 2px rgba(29, 19, 8, 0.12);
}

.user-message-footer {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  width: 100%;
  color: var(--text-muted, #9ca3af);
  font-size: 0.75rem;
}

.user-rewind-anchor {
  position: relative;
  display: flex;
  justify-content: flex-end;
  width: 100%;
}

.user-message-time {
  white-space: nowrap;
}
.user-message-action {
  display: inline-flex;
  align-items: center;
  gap: 0.3rem;
  padding: 0.2rem 0.55rem;
  border: 1px solid var(--border-color, #dbe3ef);
  border-radius: 999px;
  background: var(--bg-primary, #ffffff);
  color: var(--text-secondary, #6b7280);
  font-size: 0.75rem;
  line-height: 1;
  cursor: pointer;
  transition: all 0.2s ease;
}

.user-message-action:hover:not(:disabled),
.user-message-action.active {
  color: var(--primary-color, #2563eb);
  border-color: rgba(37, 99, 235, 0.26);
  background: var(--bg-secondary, #f8fafc);
}

.user-message-action:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

.user-rewind-panel {
  position: absolute;
  top: calc(100% + 0.45rem);
  right: 0;
  z-index: 12;
  width: min(360px, calc(100vw - 2rem));
  max-width: calc(100vw - 2rem);
  padding: 0.85rem;
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 0.9rem;
  background: var(--bg-primary, #ffffff);
  box-shadow: 0 14px 40px rgba(15, 23, 42, 0.08);
  box-sizing: border-box;
}

.user-rewind-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 0.75rem;
}

.user-rewind-title {
  color: var(--text-primary, #111827);
  font-size: 0.875rem;
  font-weight: 600;
}

.user-rewind-subtitle {
  color: var(--text-muted, #9ca3af);
  font-size: 0.75rem;
  white-space: nowrap;
}

.user-rewind-changes {
  display: flex;
  flex-wrap: wrap;
  gap: 0.4rem;
  margin-top: 0.7rem;
}

.user-rewind-chip {
  padding: 0.24rem 0.5rem;
  border-radius: 999px;
  background: var(--bg-secondary, #f3f4f6);
  color: var(--text-secondary, #6b7280);
  font-size: 0.72rem;
}

.user-rewind-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
  margin-top: 0.8rem;
}

.user-rewind-action-btn {
  flex: 1 1 calc(50% - 0.25rem);
  min-width: 132px;
  padding: 0.55rem 0.7rem;
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 0.7rem;
  background: var(--bg-secondary, #f9fafb);
  color: var(--text-primary, #1f2937);
  font-size: 0.76rem;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s ease;
}

.user-rewind-action-btn.primary {
  background: rgba(37, 99, 235, 0.08);
  border-color: rgba(37, 99, 235, 0.2);
  color: #1d4ed8;
}

.user-rewind-action-btn:hover:not(:disabled) {
  transform: translateY(-1px);
  border-color: rgba(37, 99, 235, 0.22);
}

.user-rewind-action-btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

/* 文件附件 */
.message-attachments {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
  margin-top: 0.75rem;
}

.attachment-chip {
  display: inline-flex;
  align-items: center;
  gap: 0.375rem;
  padding: 0.375rem 0.625rem;
  background-color: rgba(255, 255, 255, 0.14);
  border: 1px solid rgba(255, 255, 255, 0.2);
  border-radius: 999px;
  font-size: 0.75rem;
  color: #ffffff;
  transition: all 0.15s;
}

.attachment-chip:hover {
  background-color: rgba(255, 255, 255, 0.22);
}

.attachment-thumb {
  width: 24px;
  height: 24px;
  border-radius: 0.25rem;
  overflow: hidden;
  flex-shrink: 0;
}

.attachment-thumb img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.attachment-icon {
  width: 24px;
  height: 24px;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  color: rgba(255, 255, 255, 0.82);
}

.attachment-name {
  max-width: 150px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-weight: 600;
}

.assistant-content {
  background-color: transparent;
  padding: 0;
  max-width: 100%;
  width: 100%;
  flex: 1 0 100%;
}

.assistant-content-inner {
  display: flex;
  gap: 0.5rem;
  align-items: flex-start;
  justify-content: space-between;
}

.assistant-main-content {
  flex: 1;
  min-width: 0;
}


.tool-calls-container > * + * {
  margin-top: -1px;
}

.assistant-main-content > .tool-calls-container + .tool-calls-container {
  margin-top: -1px;
}


.assistant-main-content > .thinking-block + .tool-calls-container,
.assistant-main-content > .tool-calls-container + .thinking-block {
  margin-top: -1px;
}

.system-content {
  width: 100%;
  display: flex;
  justify-content: flex-start;
  max-width: 100%;
  padding: 0.2rem 0 0.35rem;
  box-sizing: border-box;
}

.system-message-card {
  display: inline-flex;
  align-items: center;
  justify-content: flex-start;
  max-width: min(100%, 720px);
  padding: 0;
  background: transparent;
  color: var(--text-muted, #94a3b8);
  font-size: var(--chat-font-size);
  line-height: 1.5;
  text-align: left;
}

.system-message-text {
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  word-break: break-word;
  text-wrap: pretty;
}

.message-text {
  color: #ffffff;
  white-space: pre-wrap;
  word-break: break-word;
}

.session-search-hit {
  padding: 0.06em 0.16em;
  border-radius: 0.25rem;
  background: rgba(254, 240, 138, 0.48);
  color: inherit;
}

.session-search-hit.active {
  background: rgba(251, 146, 60, 0.38);
}

/* 工具调用 */
.tool-use {
  background-color: var(--bg-secondary, #f9fafb);
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 0.5rem;
  margin-bottom: 0.75rem;
  overflow: hidden;
}

.tool-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 0.5rem 0.75rem;
  background-color: var(--bg-tertiary, #f3f4f6);
  border-bottom: 1px solid var(--border-color, #e5e7eb);
}

.tool-icon {
  font-size: 0.875rem;
  margin-right: 0.5rem;
}

.tool-name {
  font-size: 0.75rem;
  font-weight: 600;
  color: var(--text-primary, #1f2937);
  font-family: 'Monaco', 'Menlo', monospace;
  flex: 1;
}

.copy-btn {
  background: none;
  border: none;
  cursor: pointer;
  color: var(--text-secondary, #6b7280);
  padding: 0.125rem;
  border-radius: 0.25rem;
  display: flex;
  align-items: center;
  transition: all 0.2s;
}

.copy-btn:hover {
  background-color: var(--bg-primary, #ffffff);
  color: var(--text-primary, #1f2937);
}

.tool-input {
  margin: 0;
  padding: 0.75rem;
  font-size: 0.75rem;
  color: var(--text-primary, #1f2937);
  font-family: 'Monaco', 'Menlo', monospace;
  white-space: pre-wrap;
  word-break: break-all;
}

/* 工具结果 */
.tool-result {
  background-color: var(--bg-secondary, #f9fafb);
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 0.5rem;
  margin-bottom: 0.75rem;
  overflow: hidden;
}

.tool-result.error {
  border-color: #ef4444;
  background-color: #fef2f2;
}

.tool-result-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 0.5rem 0.75rem;
  background-color: var(--bg-tertiary, #f3f4f6);
  border-bottom: 1px solid var(--border-color, #e5e7eb);
}

.tool-result.error .tool-result-header {
  background-color: #fecaca;
  border-bottom-color: #fca5a5;
}

.tool-result-label {
  font-size: 0.75rem;
  font-weight: 600;
  color: var(--text-primary, #1f2937);
}

.tool-result.error .tool-result-label {
  color: #dc2626;
}

.tool-result-content {
  margin: 0;
  padding: 0.75rem;
  font-size: 0.75rem;
  color: var(--text-primary, #1f2937);
  font-family: 'Monaco', 'Menlo', monospace;
  white-space: pre-wrap;
  word-break: break-all;
  max-height: 200px;
  overflow-y: auto;
}

/* 文本内容包装器 */
.text-content-wrapper {
  position: relative;
  margin-top: 0.5rem;
  font-size: var(--chat-font-size);
}

.assistant-toolbar {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 0.375rem;
  margin-top: 0.125rem;
  margin-bottom: 0.625rem;
  margin-left: -0.25rem;
}

.assistant-toolbar-btn {
  display: inline-flex;
  align-items: center;
  gap: 0.375rem;
  padding: 0.375rem 0.75rem;
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 999px;
  background: var(--bg-primary, #ffffff);
  color: var(--text-secondary, #6b7280);
  font-size: 0.75rem;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s ease;
}

.assistant-toolbar-btn:hover {
  color: var(--text-primary, #1f2937);
  border-color: rgba(59, 130, 246, 0.25);
  background: var(--bg-secondary, #f9fafb);
}

.assistant-toolbar-btn:active {
  transform: translateY(1px);
}

.assistant-toolbar-btn-icon {
  padding: 0.25rem;
  min-width: auto;
  justify-content: center;
  border: none;
  background: transparent;
  border-radius: 0.5rem;
}

/* Token 使用量 Badge */
.token-usage-badge {
  flex-shrink: 0;
  display: flex;
  align-items: flex-end;
  gap: 0.75rem;
  margin-left: auto;
  font-size: 0.7rem;
  font-family: 'Monaco', 'Menlo', monospace;
}

.token-value {
  color: var(--text-primary, #1f2937);
  font-weight: 500;
  white-space: nowrap;
}

.token-value:first-child {
  color: #3b82f6;
}

.token-value:nth-child(2) {
  color: #10b981;
}

.token-cache {
  color: #3b82f6;
}

/* 深色模式 */
@media (prefers-color-scheme: dark) {
  .message-wrapper.search-match-active::after {
    border-color: rgba(251, 146, 60, 0.35);
  }

  .session-search-hit {
    background: rgba(250, 204, 21, 0.24);
  }

  .session-search-hit.active {
    background: rgba(249, 115, 22, 0.28);
  }

  /* 深色模式附件样式 */
  .attachment-chip {
    background-color: rgba(255, 255, 255, 0.14);
    color: #ffffff;
  }

  .attachment-chip:hover {
    background-color: rgba(255, 255, 255, 0.22);
  }

  .attachment-icon {
    color: rgba(255, 255, 255, 0.82);
  }

  .user-message-footer {
    color: var(--text-muted, #94a3b8);
  }

  .user-message-action {
    background: var(--bg-secondary, #1f2937);
    border-color: var(--border-color, #374151);
    color: var(--text-muted, #9ca3af);
  }

  .user-message-action:hover:not(:disabled),
  .user-message-action.active {
    background: var(--bg-tertiary, #374151);
    color: #93c5fd;
    border-color: rgba(147, 197, 253, 0.3);
  }

  .user-rewind-panel {
    top: calc(100% + 0.4rem);
    right: 0;
    left: auto;
    background: var(--bg-secondary, #111827);
    border-color: var(--border-color, #374151);
    box-shadow: 0 14px 40px rgba(2, 6, 23, 0.35);
  }

  .user-rewind-title,
  .user-rewind-action-btn {
    color: var(--text-primary, #f9fafb);
  }

  .user-rewind-chip {
    background: var(--bg-tertiary, #374151);
    color: var(--text-muted, #cbd5e1);
  }

  .user-rewind-action-btn {
    background: var(--bg-tertiary, #1f2937);
    border-color: var(--border-color, #374151);
  }

  .user-rewind-action-btn.primary {
    background: rgba(59, 130, 246, 0.18);
    color: #bfdbfe;
    border-color: rgba(96, 165, 250, 0.3);
  }

  .system-message-card {
    color: rgba(148, 163, 184, 0.88);
  }

  .tool-use,
  .tool-result {
    background-color: var(--bg-secondary, #1f2937);
    border-color: var(--border-color, #374151);
  }

  .tool-header,
  .tool-result-header {
    background-color: var(--bg-tertiary, #374151);
    border-bottom-color: var(--border-color, #374151);
  }

  .tool-name,
  .tool-result-label {
    color: var(--text-primary, #f9fafb);
  }

  .tool-result.error {
    border-color: #991b1b;
    background-color: #450a0a;
  }

  .tool-result.error .tool-result-header {
    background-color: #7f1d1d;
    border-bottom-color: #991b1b;
  }

  .tool-result.error .tool-result-label {
    color: #fca5a5;
  }

  .copy-btn:hover {
    background-color: var(--bg-primary, #0f172a);
    color: var(--text-primary, #f9fafb);
  }

  .tool-input,
  .tool-result-content {
    color: var(--text-primary, #f9fafb);
  }

  .assistant-toolbar-btn {
    background: var(--bg-secondary, #1f2937);
    border-color: var(--border-color, #374151);
    color: var(--text-muted, #9ca3af);
  }

  .assistant-toolbar-btn:hover {
    background: var(--bg-tertiary, #374151);
    color: var(--text-primary, #f9fafb);
    border-color: rgba(96, 165, 250, 0.35);
  }

  .assistant-toolbar-btn-icon {
    border: none;
    background: transparent;
  }

  .token-value {
    color: var(--text-primary, #f9fafb);
  }

  .token-value:first-child {
    color: #60a5fa;
  }

  .token-value:nth-child(2) {
    color: #34d399;
  }

  .token-cache {
    color: #60a5fa;
  }
}
</style>
