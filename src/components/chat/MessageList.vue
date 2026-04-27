<script setup lang="ts">
import { ref, watch, nextTick, onMounted, onUnmounted, computed } from 'vue';
import type { FeedEntry, Message, PermissionRequest, SubagentRuntimeState } from '../../types';
import MessageItem from './MessageItem.vue';
import MessageGroup from './MessageGroup.vue';
import SubagentView from './SubagentView.vue';
import { groupMessages } from '../../utils/messageGrouping';
import { isTodoWriteToolUseBlock } from '../../utils/todoWrite';
import type { RewindAction, RewindTurn } from '../../utils/rewind';
import { messageMatchesQuery } from '../../utils/sessionSearch';

interface Props {
  messages: Message[];
  isStreaming?: boolean;
  pendingPermissions?: PermissionRequest[];
  sessionId?: string;
  rewindTurns?: RewindTurn[];
  rewindBusy?: boolean;
  searchQuery?: string;
  activeSearchResultIndex?: number;
  subagentRuntime?: Map<string, SubagentRuntimeState>;
}

const props = withDefaults(defineProps<Props>(), {
  isStreaming: false,
  pendingPermissions: () => [],
  sessionId: '',
  rewindTurns: () => [],
  rewindBusy: false,
  searchQuery: '',
  activeSearchResultIndex: 0,
  subagentRuntime: () => new Map(),
});

// 调试：立即输出 props 的值
console.log('🔍 [MessageList] Component initialized with props:', {
  messagesCount: props.messages.length,
  pendingPermissionsCount: props.pendingPermissions?.length || 0,
  pendingPermissions: props.pendingPermissions,
  sessionId: props.sessionId
});

// 调试：监听 pendingPermissions 的变化
watch(() => props.pendingPermissions, (newPerms) => {
  console.log('🔍 [MessageList] pendingPermissions changed:', {
    count: newPerms?.length || 0,
    permissions: newPerms,
    sessionId: props.sessionId
  });
}, { deep: true, immediate: true });

// 调试：监听 sessionId 的变化
watch(() => props.sessionId, (newSessionId) => {
  console.log('🔍 [MessageList] sessionId changed:', newSessionId);
}, { immediate: true });

// 调试：监听 messages 的变化
watch(() => props.messages, (newMessages) => {
  console.log('🔍 [MessageList] messages changed:', {
    count: newMessages.length,
    messageIds: newMessages.map(m => ({ id: m.id, role: m.role, hasContentBlocks: !!m.contentBlocks })),
    sessionId: props.sessionId
  });
}, { deep: true, immediate: true });

interface Emits {
  (e: 'approve', requestId: string, updatedInput?: Record<string, unknown>): void;
  (e: 'approveAlways', requestId: string): void;
  (e: 'reject', requestId: string, reason?: string): void;
  (e: 'copy', content: string): void;
  (e: 'regenerate', messageId: string): void;
  (e: 'rewind', payload: { turn: RewindTurn; action: RewindAction }): void;
  (e: 'searchResultsChange', payload: { count: number }): void;
}

const emit = defineEmits<Emits>();

const messagesContainer = ref<HTMLElement>();
const showScrollButton = ref(false);
let isAutoScrolling = true;
const pendingInstantScrollCount = ref(0);

const consumePendingInstantScroll = () => {
  if (pendingInstantScrollCount.value <= 0) return false;
  pendingInstantScrollCount.value -= 1;
  return true;
};

// 分组后的消息条目
const feedEntries = computed<FeedEntry[]>(() => {
  return groupMessages(props.messages, props.subagentRuntime);
});

const rewindTurnMap = computed(() => {
  return new Map(props.rewindTurns.map((turn) => [turn.messageId, turn]));
});

const matchedMessageIds = computed(() => {
  const query = props.searchQuery.trim();
  if (!query) return [];

  return props.messages
    .filter((message) => messageMatchesQuery(message, query))
    .map((message) => message.id);
});

const activeSearchResultIndexResolved = computed(() => {
  const count = matchedMessageIds.value.length;
  if (count === 0) return -1;
  return Math.max(0, Math.min(props.activeSearchResultIndex, count - 1));
});

const activeSearchMessageId = computed(() => {
  if (activeSearchResultIndexResolved.value < 0) return null;
  return matchedMessageIds.value[activeSearchResultIndexResolved.value] || null;
});

type GroupCornerStyle = 'none' | 'top' | 'bottom' | 'all';
const parentToolExpanded = ref<Record<string, boolean>>({});

function getParentToolUseId(message: Message): string | null {
  return message.parentToolUseId || message.parent_tool_use_id || null;
}

function summarizeFeedEntry(entry: FeedEntry): Record<string, unknown> {
  if (entry.kind === 'message') {
    return {
      kind: entry.kind,
      id: entry.msg.id,
      role: entry.msg.role,
      parentToolUseId: getParentToolUseId(entry.msg),
      toolNames: (entry.msg.contentBlocks || [])
        .filter((block) => block.type === 'tool_use')
        .map((block) => (block as { name?: string }).name || ''),
    };
  }

  if (entry.kind === 'tool_msg_group') {
    return {
      kind: entry.kind,
      toolName: entry.toolName,
      firstId: entry.firstId,
      itemIds: entry.items.map((item) => item.id),
      subagentGroups: entry.subagentGroups?.map((group) => summarizeFeedEntry(group)) || [],
    };
  }

  return {
    kind: entry.kind,
    taskToolUseId: entry.taskToolUseId,
    description: entry.description,
    agentType: entry.agentType,
    liveCalls: entry.liveCalls?.map((call) => ({ id: call.id, name: call.name, status: call.status })) || [],
    children: entry.children.map((child) => summarizeFeedEntry(child)),
  };
}

function getVisibleTextContent(msg: Message): string {
  if (!msg.content) return '';

  try {
    const parsed = JSON.parse(msg.content);
    if (!Array.isArray(parsed)) return msg.content.trim();

    return parsed
      .filter((block: any) => block?.type === 'text')
      .map((block: any) => (block.content ?? block.text ?? '').trim())
      .filter(Boolean)
      .join('\n');
  } catch {
    return msg.content.trim();
  }
}

function isParentToolExpanded(entryId: string, defaultValue = false): boolean {
  if (!(entryId in parentToolExpanded.value)) {
    parentToolExpanded.value = {
      ...parentToolExpanded.value,
      [entryId]: defaultValue,
    };
  }

  return parentToolExpanded.value[entryId];
}

function setParentToolExpanded(entryId: string, expanded: boolean): void {
  parentToolExpanded.value = {
    ...parentToolExpanded.value,
    [entryId]: expanded,
  };
}

function hasVisibleBlockContent(msg: Message): boolean {
  const blocks = msg.contentBlocks || [];
  return blocks.some((block) => {
    if (block.type === 'thinking') return true;
    if (block.type === 'tool_use') return !isTodoWriteToolUseBlock(block);
    return false;
  });
}

function hasVisibleMessageContent(entry: FeedEntry): boolean {
  if (entry.kind !== 'message') return true;

  const { msg } = entry;
  if (msg.attachments?.length || msg.images?.length || msg.showTokenUsage) {
    return true;
  }

  const blocks = msg.contentBlocks || [];
  if (blocks.length > 0) {
    return blocks.some((block) => {
      if (block.type === 'thinking') return true;
      if (block.type === 'tool_use') return !isTodoWriteToolUseBlock(block);
      if (block.type === 'text') {
        const textBlock = block as { content?: string; text?: string };
        return Boolean((textBlock.content ?? textBlock.text ?? '').trim());
      }
      return true;
    });
  }

  return Boolean(getVisibleTextContent(msg));
}

function isSkippableAssistantEntry(entry: FeedEntry): boolean {
  return entry.kind === 'message' && entry.msg.role === 'assistant' && !hasVisibleMessageContent(entry);
}

function isStandaloneAssistantBlockEntry(entry: FeedEntry): boolean {
  return entry.kind === 'message'
    && entry.msg.role === 'assistant'
    && !getVisibleTextContent(entry.msg)
    && hasVisibleBlockContent(entry.msg)
    && !entry.msg.attachments?.length
    && !entry.msg.images?.length;
}

function isVisualBlockEntry(entry: FeedEntry): boolean {
  return entry.kind === 'tool_msg_group' || isStandaloneAssistantBlockEntry(entry);
}

function hasAdjacentToolGroup(startIndex: number, direction: -1 | 1): boolean {
  let cursor = startIndex + direction;

  while (cursor >= 0 && cursor < feedEntries.value.length) {
    const entry = feedEntries.value[cursor];

    if (isVisualBlockEntry(entry)) return true;
    if (isSkippableAssistantEntry(entry)) {
      cursor += direction;
      continue;
    }

    return false;
  }

  return false;
}

function getToolGroupCornerStyle(index: number): GroupCornerStyle {
  const hasPrevToolGroup = hasAdjacentToolGroup(index, -1);
  const hasNextToolGroup = hasAdjacentToolGroup(index, 1);

  if (hasPrevToolGroup && hasNextToolGroup) return 'none';
  if (hasPrevToolGroup) return 'bottom';
  if (hasNextToolGroup) return 'top';
  return 'all';
}

function getAssistantBlockCornerStyle(index: number): GroupCornerStyle {
  return getToolGroupCornerStyle(index);
}

function permissionMatchesToolUseId(permission: PermissionRequest, toolUseId: string): boolean {
  return permission.tool_use_id === toolUseId || permission.params?.tool_use_id === toolUseId;
}

function collectEntryPermissions(
  entries: FeedEntry[],
  permissions: PermissionRequest[],
  permMap: Map<string, PermissionRequest>,
  matchedRequestIds: Set<string>,
): void {
  for (const entry of entries) {
    if (entry.kind === 'tool_msg_group') {
      let hasMatch = false;

      for (const item of entry.items) {
        const perm = permissions.find((candidate) => permissionMatchesToolUseId(candidate, item.id));
        if (!perm) continue;

        permMap.set(item.id, perm);
        matchedRequestIds.add(perm.request_id);
        hasMatch = true;

        console.log('✅ [MessageList] Matched permission:', {
          entryFirstId: entry.firstId,
          itemId: item.id,
          permRequestId: perm.request_id,
          permToolUseId: perm.tool_use_id,
          permParamsToolUseId: perm.params?.tool_use_id,
        });
        break;
      }

      if (!hasMatch && permissions.length) {
        console.log('⚠️ [MessageList] No permission match for entry:', {
          entryFirstId: entry.firstId,
          itemCount: entry.items.length,
          itemIds: entry.items.map((item) => item.id),
          pendingPermissions: permissions.map((perm) => ({
            requestId: perm.request_id,
            toolUseId: perm.tool_use_id,
            paramsToolUseId: perm.params?.tool_use_id,
          })),
        });
      }

      for (const subagent of entry.subagentGroups || []) {
        collectEntryPermissions(subagent.children, permissions, permMap, matchedRequestIds);
      }
      continue;
    }

    if (entry.kind === 'subagent') {
      collectEntryPermissions(entry.children, permissions, permMap, matchedRequestIds);
    }
  }
}

// 为每个 entry 计算对应的权限请求（响应式）
const entryPermissions = computed(() => {
  const permMap = new Map<string, PermissionRequest>();
  const permissions = props.pendingPermissions || [];
  const matchedRequestIds = new Set<string>();
  collectEntryPermissions(feedEntries.value, permissions, permMap, matchedRequestIds);
  return permMap;
});

// 获取条目对应的权限请求
function getPermissionForGroup(entry: FeedEntry): PermissionRequest | undefined {
  if (entry.kind !== 'tool_msg_group') return undefined;
  // 遍历 items，使用 item.id 查找权限（因为现在 permMap 的键是 item.id）
  for (const item of entry.items) {
    const perm = entryPermissions.value.get(item.id);
    if (perm) return perm;
  }
  return undefined;
}

function subagentGroupHasPendingPermission(group: Extract<FeedEntry, { kind: 'subagent' }>): boolean {
  return group.children.some((entry) => entryHasPendingPermission(entry));
}

function entryHasPendingPermission(entry: FeedEntry): boolean {
  if (entry.kind === 'tool_msg_group') {
    if (getPermissionForGroup(entry)) return true;
    return (entry.subagentGroups || []).some((group) => subagentGroupHasPendingPermission(group));
  }

  if (entry.kind === 'subagent') {
    return subagentGroupHasPendingPermission(entry);
  }

  return false;
}

function getSubagentParentDescription(entry: FeedEntry): string {
  if (entry.kind !== 'tool_msg_group') return '';

  const input = entry.items[0]?.input || {};
  const description = String(input.description || '').trim();
  const name = String(input.name || '').trim();
  const prompt = String(input.prompt || '').trim();
  const parts: string[] = [];

  if (description) parts.push(description);
  if (name) parts.push(`name: ${name}`);
  if (prompt) parts.push(`prompt: ${prompt}`);

  return parts.join(' · ');
}

// 获取条目的唯一 key
function getEntryKey(entry: FeedEntry, _index: number): string {
  if (entry.kind === 'message') {
    return entry.msg.id;
  } else if (entry.kind === 'tool_msg_group') {
    return entry.firstId;
  } else {
    return entry.taskToolUseId;
  }
}

// 检查是否在底部
const checkIsAtBottom = () => {
  if (!messagesContainer.value) return true;
  const container = messagesContainer.value;
  const threshold = 50;
  return container.scrollHeight - container.scrollTop - container.clientHeight < threshold;
};

// 滚动到底部
const scrollToBottom = (smooth = true) => {
  if (!messagesContainer.value) return;
  isAutoScrolling = true;
  const container = messagesContainer.value;

  if (!smooth) {
    container.classList.add('instant-scroll');
    container.scrollTop = container.scrollHeight;
    requestAnimationFrame(() => {
      container.classList.remove('instant-scroll');
    });
    return;
  }

  container.scrollTo({
    top: container.scrollHeight,
    behavior: 'smooth'
  });
};

// 处理滚动事件
const handleScroll = () => {
  isAutoScrolling = checkIsAtBottom();
  showScrollButton.value = !isAutoScrolling;
};

// 点击滚动到底部按钮
const handleScrollToBottom = () => {
  scrollToBottom(true);
};

const escapeAttributeValue = (value: string) => value.replace(/\\/g, '\\\\').replace(/"/g, '\\"');

const scrollToMessage = async (messageId: string | null, smooth = true) => {
  if (!messageId || !messagesContainer.value) return;

  await nextTick();

  const container = messagesContainer.value;
  const target = container.querySelector(
    `[data-message-id="${escapeAttributeValue(messageId)}"]`,
  ) as HTMLElement | null;
  if (!target) return;

  isAutoScrolling = false;

  const containerRect = container.getBoundingClientRect();
  const targetRect = target.getBoundingClientRect();
  const top = targetRect.top - containerRect.top + container.scrollTop - 24;

  container.scrollTo({
    top: Math.max(top, 0),
    behavior: smooth ? 'smooth' : 'auto',
  });
};

// 监听消息变化，自动滚动
watch(() => feedEntries.value, async () => {
  if (isAutoScrolling) {
    await nextTick();
    scrollToBottom(!consumePendingInstantScroll());
  }
}, { deep: true });

watch(
  () => feedEntries.value,
  (entries) => {
    const nextExpanded = { ...parentToolExpanded.value };
    let hasChange = false;

    for (const entry of entries) {
      if (entry.kind !== 'tool_msg_group') continue;
      if (!entryHasPendingPermission(entry)) continue;
      if (nextExpanded[entry.firstId] === true) continue;

      nextExpanded[entry.firstId] = true;
      hasChange = true;
    }

    if (hasChange) {
      parentToolExpanded.value = nextExpanded;
    }
  },
  { deep: true, immediate: true },
);

watch(
  () => ({
    messageIds: props.messages.map((message) => message.id).join(','),
    runtimeKeys: Array.from(props.subagentRuntime.keys()).join(','),
    feedSummary: feedEntries.value.map((entry) => JSON.stringify(summarizeFeedEntry(entry))).join('||'),
  }),
  () => {
    if (!import.meta.env.DEV) return;

    const hasSubagentSignal = props.messages.some((message) => {
      if (getParentToolUseId(message)) return true;
      return (message.contentBlocks || []).some((block) => block.type === 'tool_use' && (block as { name?: string }).name === 'Task');
    }) || props.subagentRuntime.size > 0;

    if (!hasSubagentSignal) return;

    console.groupCollapsed('[SubagentDebug] MessageList feed structure');
    console.log('messages', props.messages.map((message) => ({
      id: message.id,
      role: message.role,
      parentToolUseId: getParentToolUseId(message),
      toolNames: (message.contentBlocks || [])
        .filter((block) => block.type === 'tool_use')
        .map((block) => (block as { name?: string }).name || ''),
    })));
    console.log('runtime', Array.from(props.subagentRuntime.entries()).map(([taskId, runtime]) => ({
      taskId,
      status: runtime.status,
      callIds: runtime.calls.map((call) => call.id),
    })));
    console.log('feedEntries', feedEntries.value.map((entry) => summarizeFeedEntry(entry)));
    console.groupEnd();
  },
  { deep: true, immediate: true },
);

// 监听流式状态
watch(() => props.isStreaming, async (isStreaming) => {
  if (isStreaming) {
    isAutoScrolling = true;
    await nextTick();
    scrollToBottom(!consumePendingInstantScroll());
  }
});

watch(() => props.sessionId, () => {
  pendingInstantScrollCount.value = 4;
  isAutoScrolling = true;
});

watch(
  matchedMessageIds,
  (ids) => {
    emit('searchResultsChange', { count: ids.length });
  },
  { immediate: true },
);

watch(
  () => [props.searchQuery, activeSearchMessageId.value],
  async ([query, messageId], previousValue) => {
    if (!query || !query.trim() || !messageId) return;
    const previousMessageId = previousValue?.[1] ?? null;
    await scrollToMessage(messageId, messageId !== previousMessageId);
  },
);

onMounted(() => {
  if (messagesContainer.value) {
    messagesContainer.value.addEventListener('scroll', handleScroll);
  }
  // 初始滚动到底部
  nextTick(() => scrollToBottom(false));
});

onUnmounted(() => {
  if (messagesContainer.value) {
    messagesContainer.value.removeEventListener('scroll', handleScroll);
  }
});

// 暴露滚动方法
defineExpose({
  scrollToBottom,
  checkIsAtBottom: () => checkIsAtBottom()
});

// ========== 最后一条消息判断 ==========

// 计算最后一条 assistant 消息的 ID
const lastAssistantMessageId = computed(() => {
  // 从后往前找最后一条 assistant 消息
  for (let i = props.messages.length - 1; i >= 0; i--) {
    const msg = props.messages[i];
    if (msg.role === 'assistant') {
      return msg.id;
    }
  }
  return null;
});

// ========== 权限请求处理 ==========

// 获取没有匹配到工具组的权限请求（用于后备显示）
const unmatchedPermissions = computed(() => {
  const matchedRequestIds = new Set<string>();
  collectEntryPermissions(feedEntries.value, props.pendingPermissions || [], new Map(), matchedRequestIds);
  return props.pendingPermissions.filter((permission) => !matchedRequestIds.has(permission.request_id));
});
</script>

<template>
  <div class="message-list-container">
    <div
      ref="messagesContainer"
      class="messages-list"
    >
      <!-- 空状态 -->
      <div v-if="messages.length === 0" class="empty-state">
        <div class="empty-icon">💬</div>
        <div class="empty-title">开始新对话</div>
        <div class="empty-description">输入消息开始与 Claude 对话</div>
      </div>

      <!-- 分组后的消息列表 -->
      <template v-for="(entry, index) in feedEntries" :key="getEntryKey(entry, index)">
        <!-- 工具消息组 -->
        <div
          v-if="entry.kind === 'tool_msg_group'"
          class="tool-entry-stack"
          :class="{ 'has-subagents': !!entry.subagentGroups?.length }"
        >
          <MessageGroup
            :group="entry"
            :corner-style="getToolGroupCornerStyle(index)"
            :permission="getPermissionForGroup(entry)"
            :initial-expanded="isParentToolExpanded(entry.firstId, !!entry.subagentGroups?.length)"
            @approve="(id: string, updatedInput?: Record<string, unknown>) => emit('approve', id, updatedInput)"
            @approve-always="(id: string) => emit('approveAlways', id)"
            @reject="(id: string, reason?: string) => emit('reject', id, reason)"
            @expanded-change="(expanded: boolean) => setParentToolExpanded(entry.firstId, expanded)"
          >
            <template #before-result>
              <div
                v-if="entry.subagentGroups?.length"
                class="nested-subagent-stack"
              >
                <div
                  v-for="subagent in entry.subagentGroups"
                  :key="subagent.taskToolUseId"
                  class="nested-subagent-item"
                >
                  <SubagentView
                    :group="subagent"
                    :depth="1"
                    :permissions="entryPermissions"
                    :parent-description="getSubagentParentDescription(entry)"
                    @approve="(id: string, updatedInput?: Record<string, unknown>) => emit('approve', id, updatedInput)"
                    @approve-always="(id: string) => emit('approveAlways', id)"
                    @reject="(id: string, reason?: string) => emit('reject', id, reason)"
                  />
                </div>
              </div>
            </template>
          </MessageGroup>
        </div>

        <!-- 子代理 -->
        <SubagentView
          v-else-if="entry.kind === 'subagent'"
          :group="entry"
          :permissions="entryPermissions"
          @approve="(id: string, updatedInput?: Record<string, unknown>) => emit('approve', id, updatedInput)"
          @approve-always="(id: string) => emit('approveAlways', id)"
          @reject="(id: string, reason?: string) => emit('reject', id, reason)"
        />

        <!-- 普通消息 -->
        <MessageItem
          v-else
          :message="entry.msg"
          :show-time="entry.msg.role === 'user'"
          :is-last-assistant-message="entry.msg.id === lastAssistantMessageId"
          :block-corner-style="getAssistantBlockCornerStyle(index)"
          :rewind-turn="rewindTurnMap.get(entry.msg.id) || null"
          :rewind-busy="rewindBusy"
          :search-query="searchQuery"
          :is-active-search-match="entry.msg.id === activeSearchMessageId"
          @copy="(content: string) => emit('copy', content)"
          @regenerate="(messageId: string) => emit('regenerate', messageId)"
          @rewind="(payload) => emit('rewind', payload)"
        />
      </template>

      <!-- 未匹配到工具组的权限请求（后备显示） -->
      <template v-for="permission in unmatchedPermissions" :key="permission.request_id">
        <div class="permission-fallback">
          <div class="permission-header">
            <span class="status-dot">●</span>
            <span class="permission-desc">{{ permission.description || '权限请求' }}</span>
          </div>
          <div class="permission-actions">
            <button class="btn-approve" @click="emit('approve', permission.request_id)">允许</button>
            <button class="btn-approve-all" @click="emit('approveAlways', permission.request_id)">始终允许</button>
            <button class="btn-reject" @click="emit('reject', permission.request_id)">拒绝</button>
          </div>
        </div>
      </template>
    </div>

    <!-- 滚动到底部按钮 -->
    <Transition name="fade">
      <button
        v-if="showScrollButton"
        class="scroll-to-bottom"
        @click="handleScrollToBottom"
        title="滚动到底部"
      >
        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M12 5v14M19 12l-7 7-7-7"/>
        </svg>
      </button>
    </Transition>
  </div>
</template>

<style scoped>
.message-list-container {
  position: relative;
  height: 100%;
  min-height: 200px;
  display: flex;
  flex-direction: column;
  --chat-font-size: var(--chat-font-size-px, 14px);
}

.messages-list {
  flex: 1;
  overflow-x: auto;
  overflow-y: auto;
  padding: 1rem 1.5rem 1.25rem;
  scroll-behavior: smooth;
}

.messages-list.instant-scroll {
  scroll-behavior: auto;
}

.messages-list > * {
  max-width: 960px;
  margin-left: auto;
  margin-right: auto;
}

.tool-entry-stack {
  max-width: 960px;
  margin-left: auto;
  margin-right: auto;
}

.nested-subagent-stack {
  position: relative;
  margin-top: 0.35rem;
  margin-left: 1.35rem;
  padding-left: 1rem;
}

.nested-subagent-stack::before {
  content: '';
  position: absolute;
  left: 0.22rem;
  top: 0;
  bottom: 0.9rem;
  width: 1px;
  background: rgba(148, 163, 184, 0.45);
}

.nested-subagent-item {
  position: relative;
}

.nested-subagent-item + .nested-subagent-item {
  margin-top: 0.65rem;
}

.nested-subagent-item::before {
  content: '';
  position: absolute;
  left: -0.78rem;
  top: 1rem;
  width: 0.8rem;
  height: 1px;
  background: rgba(148, 163, 184, 0.45);
}

.nested-subagent-item::after {
  content: '';
  position: absolute;
  left: -1rem;
  top: 0.76rem;
  width: 0.5rem;
  height: 0.5rem;
  border-radius: 9999px;
  border: 2px solid rgba(148, 163, 184, 0.75);
  background: var(--bg-primary, #ffffff);
  box-sizing: border-box;
}

/* 空状态 */
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 100%;
  color: var(--text-muted, #9ca3af);
  text-align: center;
}

.empty-icon {
  font-size: 3rem;
  margin-bottom: 1rem;
  opacity: 0.5;
}

.empty-title {
  font-size: 1rem;
  font-weight: 500;
  color: var(--text-primary, #1f2937);
  margin-bottom: 0.25rem;
}

.empty-description {
  font-size: 0.875rem;
  color: var(--text-muted, #9ca3af);
}

/* 滚动到底部按钮 */
.scroll-to-bottom {
  position: absolute;
  bottom: 2rem;
  right: 2rem;
  width: 2.5rem;
  height: 2.5rem;
  border-radius: 50%;
  background-color: var(--bg-primary, #ffffff);
  border: 1px solid var(--border-color, #e5e7eb);
  box-shadow: 0 4px 6px -1px rgba(0, 0, 0, 0.1);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-secondary, #6b7280);
  transition: all 0.2s;
}

.scroll-to-bottom:hover {
  background-color: var(--bg-secondary, #f9fafb);
  color: var(--primary-color, #3b82f6);
  transform: scale(1.05);
}

.scroll-to-bottom:active {
  transform: scale(0.95);
}

/* 淡入淡出动画 */
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s, transform 0.2s;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
  transform: translateY(10px);
}

/* 深色模式 */
@media (prefers-color-scheme: dark) {
  .nested-subagent-stack::before,
  .nested-subagent-item::before {
    background: rgba(107, 114, 128, 0.65);
  }

  .nested-subagent-item::after {
    border-color: rgba(107, 114, 128, 0.85);
    background: var(--bg-secondary, #1f2937);
  }

  .empty-title {
    color: var(--text-primary, #f9fafb);
  }

  .scroll-to-bottom {
    background-color: var(--bg-secondary, #1f2937);
    border-color: var(--border-color, #374151);
  }

  .scroll-to-bottom:hover {
    background-color: var(--bg-tertiary, #374151);
  }
}

/* 权限请求后备显示 - 和工具调用块保持一致的样式 */
.permission-fallback {
  margin: 0.125rem 0;
  padding: 0.4rem 0.65rem;
  border: 1px solid color-mix(in srgb, var(--border-color, #e5e7eb) 76%, transparent);
  border-left: 2px solid color-mix(in srgb, var(--primary-color, #3b82f6) 20%, transparent);
  border-radius: 10px;
  background: var(--bg-primary, #ffffff);
}

.permission-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 0;
  cursor: pointer;
}

.permission-header .status-dot {
  color: #3b82f6;
  font-size: 8px;
}

.permission-desc {
  font-size: var(--chat-font-size);
  color: var(--text-primary, #1f2937);
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
  flex: 1;
  line-height: 1.5;
}

.permission-actions {
  display: flex;
  flex-direction: column;
  gap: 8px;
  max-width: 280px;
  margin-top: 8px;
  padding-left: 18px;
}

.permission-actions button {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  width: 100%;
  padding: 10px 16px;
  border: none;
  border-radius: 8px;
  font-size: calc(var(--chat-font-size) * 0.95);
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s;
  white-space: nowrap;
}

.btn-reject {
  background-color: var(--bg-tertiary, #f3f4f6);
  color: var(--text-primary, #1f2937);
}

.btn-reject:hover {
  background-color: #fee2e2;
  color: #dc2626;
}

.btn-approve {
  background-color: #dbeafe;
  color: #1d4ed8;
}

.btn-approve:hover {
  background-color: #bfdbfe;
}

.btn-approve-all {
  background-color: var(--primary-color, #3b82f6);
  color: #ffffff;
}

.btn-approve-all:hover {
  background-color: var(--primary-hover, #2563eb);
}

@media (prefers-color-scheme: dark) {
  .permission-fallback {
    background: var(--bg-secondary, #1f2937);
    border-color: var(--border-color, #374151);
  }

  .permission-desc {
    color: var(--text-primary, #f9fafb);
  }

  .permission-header {
    color: var(--text-primary, #f9fafb);
  }

  .btn-reject {
    background-color: var(--bg-tertiary, #374151);
    color: var(--text-primary, #f9fafb);
  }

  .btn-reject:hover {
    background-color: #7f1d1d;
    color: #fca5a5;
  }

  .btn-approve {
    background-color: rgba(59, 130, 246, 0.2);
    color: #93c5fd;
  }

  .btn-approve:hover {
    background-color: rgba(59, 130, 246, 0.3);
  }
}
</style>
