<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useClaudeStore } from '../../stores/claude';
import type { PermissionMode, PermissionRequest, GitInfo, OutgoingMessagePayload } from '../../types';
import MessageList from './MessageList.vue';
import MessageInput from './MessageInput.vue';
import GitStatus from './GitStatus.vue';
import ThinkingAnimation from './ThinkingAnimation.vue';
import { extractTodoWritePanelState } from '../../utils/todoWrite';

interface Props {
  sessionId?: string;
  disabled?: boolean;
}

const props = defineProps<Props>();

const emit = defineEmits<{
  send: [payload: OutgoingMessagePayload];
  stop: [];
  executeCommand: [commandName: string];
}>();

const claudeStore = useClaudeStore();

// 当前会话状态
const currentSession = computed(() => {
  if (!props.sessionId) return null;
  return claudeStore.sessions.get(props.sessionId) || null;
});

// 连接状态
const connectionStatus = computed(() => {
  if (!props.sessionId) return 'disconnected';
  return claudeStore.connectionStatus.get(props.sessionId) || 'disconnected';
});

// CLI 连接状态
const cliConnected = computed(() => {
  if (!props.sessionId) return false;
  return claudeStore.cliConnected.get(props.sessionId) || false;
});

// 会话状态
const sessionStatus = computed(() => {
  if (!props.sessionId) return null;
  return claudeStore.sessionStatus.get(props.sessionId) || null;
});

const isBusy = computed(() => sessionStatus.value === 'running' || sessionStatus.value === 'compacting');

// 待处理的权限请求
const pendingPermissions = computed(() => {
  if (!props.sessionId) return [];
  // 触发响应式追踪：访问整个 pendingPermissions.value
  const allPerms = claudeStore.pendingPermissions;
  const perms = allPerms.get(props.sessionId);
  const result: PermissionRequest[] = perms ? Array.from(perms.values()) as PermissionRequest[] : [];
  console.log('🔍 [ChatView] pendingPermissions computed:', {
    propsSessionId: props.sessionId,
    currentSessionId: claudeStore.currentSessionId,
    permsCount: result.length,
    perms: result
  });
  return result;
});

const permissionRequestsInFlight = ref<Set<string>>(new Set());

function resolvePermissionSessionId(requestId: string): string | null {
  if (props.sessionId) {
    const sessionPermissions = claudeStore.pendingPermissions.get(props.sessionId);
    if (sessionPermissions?.has(requestId)) {
      return props.sessionId;
    }
  }
  return claudeStore.findPermissionSessionId(requestId);
}

function markPermissionRequestInFlight(requestId: string): boolean {
  if (permissionRequestsInFlight.value.has(requestId)) {
    return false;
  }

  const next = new Set(permissionRequestsInFlight.value);
  next.add(requestId);
  permissionRequestsInFlight.value = next;
  return true;
}

function clearPermissionRequestInFlight(requestId: string): void {
  if (!permissionRequestsInFlight.value.has(requestId)) return;
  const next = new Set(permissionRequestsInFlight.value);
  next.delete(requestId);
  permissionRequestsInFlight.value = next;
}

// 权限模式
const permissionMode = computed<PermissionMode>(() => {
  return currentSession.value?.permissionMode || 'default';
});

// 之前的权限模式
const previousPermissionMode = computed(() => {
  if (!props.sessionId) return 'default';
  return claudeStore.previousPermissionMode.get(props.sessionId) || 'default';
});

// 是否为计划模式
const isPlanMode = computed(() => permissionMode.value === 'plan');

// 流式文本（暂未使用，保留供后续扩展）
// const streamingText = computed(() => {
//   if (!props.sessionId) return null;
//   return claudeStore.streaming.get(props.sessionId) || null;
// });

// 流式输出 token 数
const streamingOutputTokens = computed(() => {
  if (!props.sessionId) return null;
  return claudeStore.streamingOutputTokens.get(props.sessionId) || null;
});

const streamingStartedAt = computed(() => {
  if (!props.sessionId) return null;
  return claudeStore.streamingStartedAt.get(props.sessionId) || null;
});

// 当前消息
const currentMessages = computed(() => {
  if (!props.sessionId) return [];
  return claudeStore.messages.get(props.sessionId) || [];
});

const todoPanelState = computed(() => extractTodoWritePanelState(currentMessages.value));
const sessionSearchQuery = ref('');
const sessionSearchMatchCount = ref(0);
const sessionSearchActiveIndex = ref(0);
const searchInputRef = ref<HTMLInputElement | null>(null);

const hasSessionSearchQuery = computed(() => sessionSearchQuery.value.trim().length > 0);
const hasSessionSearchResults = computed(() => sessionSearchMatchCount.value > 0);
const sessionSearchStatusText = computed(() => {
  if (!hasSessionSearchQuery.value) return '搜索当前会话';
  if (!hasSessionSearchResults.value) return '无结果';
  return `${sessionSearchActiveIndex.value + 1} / ${sessionSearchMatchCount.value}`;
});

// Git 信息
const gitInfo = computed<GitInfo | null>(() => {
  if (!props.sessionId) return null;
  return claudeStore.sessionGitInfo.get(props.sessionId) || null;
});

// 切换权限模式
async function togglePermissionMode() {
  if (!props.sessionId || !currentSession.value) return;

  const currentMode = permissionMode.value;
  const storedPreviousMode = previousPermissionMode.value;
  const nextMode = isPlanMode.value ? storedPreviousMode : 'plan';

  if (!isPlanMode.value) {
    claudeStore.setPreviousPermissionMode(props.sessionId, currentMode);
  }

  claudeStore.updateSession(props.sessionId, { permissionMode: nextMode });

  try {
    await invoke('set_permission_mode', {
      sessionId: props.sessionId,
      mode: nextMode,
    });
  } catch (error) {
    console.error('[PermissionMode] Failed to toggle mode:', error);
    claudeStore.updateSession(props.sessionId, { permissionMode: currentMode });
    claudeStore.setPreviousPermissionMode(props.sessionId, storedPreviousMode);
  }
}

// 批准权限请求（仅批准当前）
async function approvePermission(requestId: string, updatedInput?: Record<string, unknown>) {
  const targetSessionId = resolvePermissionSessionId(requestId);
  if (!targetSessionId || !markPermissionRequestInFlight(requestId)) return;

  // 发送批准决定到后端
  try {
    await invoke('respond_to_permission', {
      sessionId: targetSessionId,
      requestId,
      action: 'approve',
      updatedInput: updatedInput || undefined,
    });
    claudeStore.removePermission(targetSessionId, requestId);
    console.log('Permission approved:', requestId);
  } catch (error) {
    console.error('Failed to approve permission:', error);
  } finally {
    clearPermissionRequestInFlight(requestId);
  }
}

// 批准权限请求（不再询问此工具）
async function approvePermissionAlways(requestId: string) {
  const targetSessionId = resolvePermissionSessionId(requestId);
  if (!targetSessionId || !markPermissionRequestInFlight(requestId)) return;

  // 发送批准决定到后端
  try {
    await invoke('respond_to_permission', {
      sessionId: targetSessionId,
      requestId,
      action: 'approve_always',
    });
    claudeStore.removePermission(targetSessionId, requestId);
    console.log('Permission approved always:', requestId);
  } catch (error) {
    console.error('Failed to approve permission always:', error);
  } finally {
    clearPermissionRequestInFlight(requestId);
  }
}

// 拒绝权限请求
async function rejectPermission(requestId: string, reason?: string) {
  const targetSessionId = resolvePermissionSessionId(requestId);
  if (!targetSessionId || !markPermissionRequestInFlight(requestId)) return;

  // 发送拒绝决定到后端
  try {
    await invoke('respond_to_permission', {
      sessionId: targetSessionId,
      requestId,
      action: 'reject',
      reason: reason?.trim() || 'Rejected by user',
    });
    claudeStore.removePermission(targetSessionId, requestId);
    console.log('Permission rejected:', requestId);
  } catch (error) {
    console.error('Failed to reject permission:', error);
  } finally {
    clearPermissionRequestInFlight(requestId);
  }
}

// 监听后端的权限请求事件
onMounted(async () => {
  // 监听 permission-request 事件
  const unlistenPermission = await listen<{
    sessionId: string;
    requestId: string;
    permission: any; // 使用 any 因为后端发送的格式可能与 PermissionRequest 类型不完全匹配
  }>('permission-request', (event) => {
    const data = event.payload;
    console.log('Received permission request:', data);

    // 将权限请求添加到 store，确保 tool_use_id 正确提取
    const permissionWithToolUseId: PermissionRequest = {
      request_id: data.permission.request_id,
      type: data.permission.type,
      description: data.permission.description,
      session_id: data.sessionId,
      params: data.permission.params,
      tool_use_id: data.permission.tool_use_id || data.permission.params?.tool_use_id,
    };
    claudeStore.addPermission(data.sessionId, permissionWithToolUseId);
  });

  // 监听 git-info 事件
  const unlistenGit = await listen<{
    sessionId: string;
    info: GitInfo;
  }>('git-info', (event) => {
    const data = event.payload;
    console.log('Received git info:', data);

    // 更新 session 中的 Git 信息
    if (data.sessionId === props.sessionId) {
      claudeStore.setSessionGitInfo(data.sessionId, data.info);
    }
  });

  // 清理监听器
  onUnmounted(() => {
    unlistenPermission();
    unlistenGit();
  });
});

// 发送消息
function sendMessage(payload: OutgoingMessagePayload) {
  if (!props.sessionId) return;

  // 发送到父组件，由父组件处理
  emit('send', payload);
}

// 停止流式响应
function stopStreaming() {
  // 发送到父组件，由父组件处理
  emit('stop');
}

function focusSearchInput(select = true) {
  if (!searchInputRef.value) return;

  searchInputRef.value.focus();
  if (select) {
    searchInputRef.value.select();
  }
}

function handleSearchResultsChange(payload: { count: number }) {
  sessionSearchMatchCount.value = payload.count;

  if (payload.count === 0) {
    sessionSearchActiveIndex.value = 0;
    return;
  }

  if (sessionSearchActiveIndex.value >= payload.count) {
    sessionSearchActiveIndex.value = 0;
  }
}

function goToSearchMatch(direction: 1 | -1) {
  if (!hasSessionSearchResults.value) return;

  const count = sessionSearchMatchCount.value;
  const nextIndex = sessionSearchActiveIndex.value + direction;
  sessionSearchActiveIndex.value = nextIndex < 0 ? count - 1 : nextIndex % count;
}

function clearSessionSearch() {
  sessionSearchQuery.value = '';
  sessionSearchMatchCount.value = 0;
  sessionSearchActiveIndex.value = 0;
}

function handleWindowKeydown(event: KeyboardEvent) {
  const isSearchShortcut = (event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'f';
  if (isSearchShortcut) {
    event.preventDefault();
    focusSearchInput(true);
    return;
  }

  if (document.activeElement !== searchInputRef.value) return;

  if (event.key === 'Enter') {
    event.preventDefault();
    goToSearchMatch(event.shiftKey ? -1 : 1);
    return;
  }

  if (event.key === 'Escape') {
    event.preventDefault();
    if (sessionSearchQuery.value) {
      clearSessionSearch();
    } else {
      searchInputRef.value?.blur();
    }
  }
}

watch(() => sessionSearchQuery.value, (value, previousValue) => {
  if (value.trim() !== previousValue.trim()) {
    sessionSearchActiveIndex.value = 0;
  }
});

watch(() => props.sessionId, () => {
  clearSessionSearch();
});

onMounted(() => {
  window.addEventListener('keydown', handleWindowKeydown);
});

onUnmounted(() => {
  window.removeEventListener('keydown', handleWindowKeydown);
});
</script>

<template>
  <div class="chat-view">
    <!-- 连接中横幅 -->
    <div
      v-if="connectionStatus === 'connecting'"
      class="status-banner info"
    >
      <span class="status-text">正在建立连接...</span>
    </div>

    <!-- CLI 断开连接横幅 -->
    <div
      v-if="connectionStatus === 'connected' && !cliConnected"
      class="status-banner warning"
    >
      <span class="status-text">CLI 已断开连接</span>
      <button class="btn-reconnect" @click="togglePermissionMode">
        重新连接
      </button>
    </div>

    <!-- WebSocket 断开连接横幅 -->
    <div
      v-if="connectionStatus === 'disconnected'"
      class="status-banner warning"
    >
      <span class="status-text">正在重新连接...</span>
    </div>

    <div
      v-if="sessionStatus === 'compacting'"
      class="status-banner info"
    >
      <span class="status-text info-text">正在压缩上下文...</span>
    </div>

    <!-- Git 状态 -->
    <div v-if="gitInfo" class="git-status-container">
      <GitStatus :git-info="gitInfo" />
    </div>

    <div class="session-search-bar">
      <label class="session-search-input-wrap">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <circle cx="11" cy="11" r="7" />
          <path d="m20 20-3.5-3.5" />
        </svg>
        <input
          ref="searchInputRef"
          v-model="sessionSearchQuery"
          type="text"
          class="session-search-input"
          placeholder="搜索当前 session 的消息..."
        >
      </label>

      <div class="session-search-actions">
        <span class="session-search-status">{{ sessionSearchStatusText }}</span>
        <button
          type="button"
          class="session-search-btn"
          :disabled="!hasSessionSearchResults"
          title="上一条匹配"
          @click="goToSearchMatch(-1)"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="m18 15-6-6-6 6" />
          </svg>
        </button>
        <button
          type="button"
          class="session-search-btn"
          :disabled="!hasSessionSearchResults"
          title="下一条匹配"
          @click="goToSearchMatch(1)"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="m6 9 6 6 6-6" />
          </svg>
        </button>
        <button
          v-if="hasSessionSearchQuery"
          type="button"
          class="session-search-btn clear"
          title="清空搜索"
          @click="clearSessionSearch"
        >
          清空
        </button>
      </div>
    </div>

    <!-- 消息列表 -->
    <MessageList
      :messages="currentMessages"
      :is-streaming="isBusy"
      :pending-permissions="pendingPermissions"
      :session-id="sessionId || ''"
      :search-query="sessionSearchQuery"
      :active-search-result-index="sessionSearchActiveIndex"
      @approve="approvePermission"
      @approve-always="approvePermissionAlways"
      @reject="rejectPermission"
      @search-results-change="handleSearchResultsChange"
    />

    <!-- 炫酷的执行中动画 -->
    <div class="thinking-animation-wrapper">
      <ThinkingAnimation
        v-if="sessionStatus === 'running'"
        :started-at="streamingStartedAt ?? undefined"
        :output-tokens="streamingOutputTokens ?? undefined"
      />
    </div>

    <!-- 输入区域 -->
    <div class="input-section">
      <!-- 权限模式切换 -->
      <button
        class="mode-toggle"
        :class="{ active: isPlanMode }"
        @click="togglePermissionMode"
      >
        <svg v-if="isPlanMode" width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
          <rect x="3" y="3" width="3.5" height="10" rx="0.75" />
          <rect x="9.5" y="3" width="3.5" height="10" rx="0.75" />
        </svg>
        <svg v-else width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
          <path d="M2.5 4l4 4-4 4" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" fill="none" />
          <path d="M8.5 4l4 4-4 4" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" fill="none" />
        </svg>
        <span>{{ isPlanMode ? '计划模式' : '接受编辑' }}</span>
      </button>

      <!-- 消息输入 -->
      <MessageInput
        :session-id="sessionId"
        :streaming="isBusy"
        :disabled="disabled || connectionStatus !== 'connected'"
        :todo-state="todoPanelState"
        @send="sendMessage"
        @stop="stopStreaming"
        @execute-command="(commandName) => emit('executeCommand', commandName)"
      />
    </div>
  </div>
</template>

<style scoped>
.chat-view {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}

/* 状态横幅 */
.status-banner {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.75rem;
  padding: 0.5rem 1rem;
  border-bottom: 1px solid;
}

/* Git 状态容器 */
.git-status-container {
  padding: 0.5rem 1rem;
  border-bottom: 1px solid var(--border-color, #e5e7eb);
  background-color: var(--bg-card, #ffffff);
}

.session-search-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
  padding: 0.75rem 1.5rem;
  border-bottom: 1px solid var(--border-color, #e5e7eb);
  background: linear-gradient(180deg, rgba(248, 250, 252, 0.96), rgba(255, 255, 255, 0.96));
  backdrop-filter: blur(10px);
}

.session-search-input-wrap {
  display: flex;
  align-items: center;
  gap: 0.65rem;
  flex: 1;
  min-width: 0;
  padding: 0.65rem 0.85rem;
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 0.85rem;
  background: var(--bg-primary, #ffffff);
  color: var(--text-muted, #9ca3af);
  transition: border-color 0.2s ease, box-shadow 0.2s ease;
}

.session-search-input-wrap:focus-within {
  border-color: rgba(59, 130, 246, 0.35);
  box-shadow: 0 0 0 3px rgba(59, 130, 246, 0.08);
}

.session-search-input {
  width: 100%;
  min-width: 0;
  border: none;
  outline: none;
  background: transparent;
  font-size: 0.875rem;
  color: var(--text-primary, #111827);
}

.session-search-input::placeholder {
  color: var(--text-muted, #9ca3af);
}

.session-search-actions {
  display: flex;
  align-items: center;
  gap: 0.45rem;
}

.session-search-status {
  min-width: 5.5rem;
  text-align: right;
  font-size: 0.75rem;
  font-weight: 600;
  color: var(--text-secondary, #6b7280);
}

.session-search-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 2rem;
  height: 2rem;
  padding: 0 0.65rem;
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 0.65rem;
  background: var(--bg-primary, #ffffff);
  color: var(--text-secondary, #6b7280);
  cursor: pointer;
  transition: all 0.2s ease;
}

.session-search-btn:hover:not(:disabled) {
  border-color: rgba(59, 130, 246, 0.3);
  color: var(--primary-color, #3b82f6);
}

.session-search-btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.session-search-btn.clear {
  font-size: 0.75rem;
  font-weight: 600;
}

.status-banner.warning {
  background-color: rgba(245, 158, 11, 0.1);
  border-bottom-color: rgba(245, 158, 11, 0.2);
}

.status-text {
  font-size: 12px;
  font-weight: 500;
  color: #f59e0b;
}

.status-banner.info {
  background-color: rgba(59, 130, 246, 0.08);
  border-bottom-color: rgba(59, 130, 246, 0.16);
}

.status-text.info-text {
  color: var(--primary-color, #3b82f6);
}

.btn-reconnect {
  padding: 0.25rem 0.75rem;
  background-color: rgba(245, 158, 11, 0.2);
  border: none;
  border-radius: 6px;
  font-size: 12px;
  font-weight: 500;
  color: #f59e0b;
  cursor: pointer;
  transition: all 0.15s;
}

.btn-reconnect:hover {
  background-color: rgba(245, 158, 11, 0.3);
}

/* 输入区域 */
.input-section {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  padding: 1rem 1.5rem;
  border-top: 1px solid var(--border-color, #e5e7eb);
  background-color: var(--bg-secondary, #f9fafb);
}

/* 模式切换按钮 */
.mode-toggle {
  display: flex;
  align-items: center;
  gap: 0.375rem;
  padding: 0.5rem 0.75rem;
  background: transparent;
  border: none;
  border-radius: 6px;
  font-size: 12px;
  font-weight: 500;
  color: var(--text-muted, #9ca3af);
  cursor: pointer;
  transition: all 0.2s;
  align-self: flex-start;
}

.mode-toggle:hover {
  background-color: var(--bg-hover, #f3f4f6);
  color: var(--text-primary, #1f2937);
}

.mode-toggle.active {
  color: var(--primary-color, #3b82f6);
  background-color: rgba(59, 130, 246, 0.1);
}

/* 执行中动画包装器 */
.thinking-animation-wrapper {
  padding-left: 2.5rem;
  padding-top: 0.75rem;
  padding-bottom: 0.5rem;
}

/* 深色模式 */
@media (prefers-color-scheme: dark) {
  .status-banner.warning {
    background-color: rgba(245, 158, 11, 0.15);
    border-bottom-color: rgba(245, 158, 11, 0.3);
  }

  .btn-reconnect {
    background-color: rgba(245, 158, 11, 0.25);
  }

  .btn-reconnect:hover {
    background-color: rgba(245, 158, 11, 0.35);
  }

  .input-section {
    background-color: var(--bg-secondary, #1f2937);
    border-top-color: var(--border-color, #374151);
  }

  .session-search-bar {
    background: linear-gradient(180deg, rgba(17, 24, 39, 0.96), rgba(31, 41, 55, 0.96));
    border-bottom-color: var(--border-color, #374151);
  }

  .session-search-input-wrap,
  .session-search-btn {
    background: var(--bg-secondary, #111827);
    border-color: var(--border-color, #374151);
    color: var(--text-secondary, #d1d5db);
  }

  .session-search-input {
    color: var(--text-primary, #f9fafb);
  }

  .mode-toggle:hover {
    background-color: var(--bg-hover, #374151);
  }

  .mode-toggle.active {
    background-color: rgba(59, 130, 246, 0.2);
  }

  .git-status-container {
    background-color: var(--bg-card, #1f2937);
    border-bottom-color: var(--border-color, #374151);
  }
}

@media (max-width: 960px) {
  .session-search-bar {
    flex-direction: column;
    align-items: stretch;
  }

  .session-search-actions {
    justify-content: space-between;
  }

  .session-search-status {
    min-width: 0;
    text-align: left;
  }
}
</style>
