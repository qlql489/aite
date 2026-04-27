<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue';
import type { FeedEntry, PermissionRequest, SubagentGroup, ToolMessageGroup } from '../../types';
import MessageGroup from './MessageGroup.vue';
import MessageItem from './MessageItem.vue';
import { formatElapsed, getToolLabel, getToolPreview } from '../../utils/messageGrouping';
import { getLeadingSubagentPrompt } from '../../utils/subagentPrompt';

interface Props {
  group: SubagentGroup;
  depth?: number;
  permissions?: Map<string, PermissionRequest>;
  parentDescription?: string;
}

const props = withDefaults(defineProps<Props>(), {
  depth: 0,
  permissions: () => new Map(),
  parentDescription: '',
});

const emit = defineEmits<{
  approve: [requestId: string, updatedInput?: Record<string, unknown>];
  approveAlways: [requestId: string];
  reject: [requestId: string, reason?: string];
}>();

const isOpen = ref(false);
const recordsOpen = ref(false);
const now = ref(Date.now());
let timer: ReturnType<typeof setInterval> | null = null;

function toggleSubagentOpen() {
  const nextOpen = !isOpen.value;
  isOpen.value = nextOpen;

  if (!nextOpen) {
    recordsOpen.value = false;
  }
}

const title = computed(() => props.group.agentType || 'Subagent');
const description = computed(() => props.group.description || '未提供子代理说明');
const liveCalls = computed(() => props.group.liveCalls || []);
const leadingPrompt = computed(() => getLeadingSubagentPrompt(props.group.children));
const displayEntries = computed(() => {
  const hiddenMessageIds = leadingPrompt.value.hiddenMessageIds;
  return props.group.children.filter((entry) => (
    entry.kind !== 'message' || !hiddenMessageIds.has(entry.msg.id)
  ));
});
const hasRecords = computed(() => liveCalls.value.length > 0 || displayEntries.value.length > 0);

function visitEntries(entries: FeedEntry[], visitor: (entry: FeedEntry) => void): void {
  for (const entry of entries) {
    visitor(entry);
    if (entry.kind === 'subagent') {
      visitEntries(entry.children, visitor);
    }
  }
}

function getEntryTimestamp(entry: FeedEntry): number | null {
  if (entry.kind === 'message') return entry.msg.timestamp ?? null;
  if (entry.kind === 'tool_msg_group') return entry.timestamp ?? null;
  return null;
}

const allToolGroups = computed<ToolMessageGroup[]>(() => {
  const groups: ToolMessageGroup[] = [];
  visitEntries(displayEntries.value, (entry) => {
    if (entry.kind === 'tool_msg_group') {
      groups.push(entry);
    }
  });
  return groups;
});

const toolCallCount = computed(() => {
  return allToolGroups.value.reduce((sum, group) => sum + group.items.length, 0);
});

const totalToolCallCount = computed(() => {
  return Math.max(
    toolCallCount.value,
    props.group.toolCallCount || 0,
    liveCalls.value.length,
  );
});

const hasError = computed(() => {
  return props.group.status === 'error'
    || liveCalls.value.some((call) => call.status === 'error' || call.isError)
    || allToolGroups.value.some((group) => group.items.some((item) => item.isError));
});

const hasRunningTool = computed(() => {
  return props.group.status === 'running'
    || liveCalls.value.some((call) => call.status === 'running')
    || allToolGroups.value.some((group) => group.items.some((item) => item.result === undefined));
});

const startTimestamp = computed(() => {
  if (props.group.startedAt != null) return props.group.startedAt;
  let min: number | null = null;
  visitEntries(props.group.children, (entry) => {
    const timestamp = getEntryTimestamp(entry);
    if (timestamp == null) return;
    min = min == null ? timestamp : Math.min(min, timestamp);
  });
  return min;
});

const endTimestamp = computed(() => {
  if (props.group.completedAt != null) return props.group.completedAt;
  let max: number | null = null;
  visitEntries(props.group.children, (entry) => {
    const timestamp = getEntryTimestamp(entry);
    if (timestamp == null) return;
    max = max == null ? timestamp : Math.max(max, timestamp);
  });
  return max;
});

const agentName = computed(() => title.value);
const currentTaskName = computed(() => description.value);
const parentTaskName = computed(() => props.parentDescription.trim());
const currentContextDescription = computed(() => parentTaskName.value || currentTaskName.value);
const contextDescription = computed(() => leadingPrompt.value.promptText || currentContextDescription.value);

function getVisibleTextContent(message: { content?: string; contentBlocks?: Array<{ type: string; text?: string; content?: unknown }> }): string {
  const textFromBlocks = (message.contentBlocks || [])
    .filter((block) => block.type === 'text')
    .map((block) => {
      const blockContent = typeof block.content === 'string' ? block.content : '';
      return (block.text || blockContent || '').trim();
    })
    .filter(Boolean)
    .join('\n');

  return (textFromBlocks || message.content || '').trim();
}

function hasAssistantFinalResult(entry: FeedEntry): boolean {
  if (entry.kind === 'message') {
    if (entry.msg.role !== 'assistant') return false;
    return Boolean(getVisibleTextContent(entry.msg));
  }

  if (entry.kind === 'subagent') {
    return entry.children.some(hasAssistantFinalResult);
  }

  return false;
}

const hasFinalResult = computed(() => {
  return displayEntries.value.some(hasAssistantFinalResult);
});

const isAwaitingFinalResult = computed(() => {
  if (hasError.value) return false;
  return !hasFinalResult.value;
});

const elapsedMs = computed(() => {
  if (startTimestamp.value == null) return null;
  const end = isAwaitingFinalResult.value ? now.value : (endTimestamp.value ?? now.value);
  return Math.max(0, end - startTimestamp.value);
});

const elapsedLabel = computed(() => {
  if (elapsedMs.value == null) return '';
  return formatElapsed(elapsedMs.value);
});

const overallStatus = computed<'running' | 'success' | 'error'>(() => {
  if (hasFinalResult.value) return 'success';
  if (props.group.status === 'error') return 'error';
  if (hasError.value) return 'error';
  if (isAwaitingFinalResult.value) return 'running';
  return 'success';
});

const summaryStatusText = computed(() => {
  if (overallStatus.value === 'running') return '执行中';
  if (overallStatus.value === 'error') return '执行失败';
  return '执行成功';
});

function getRuntimeCallPreview(call: { name: string; input: Record<string, unknown> }): string {
  return getToolPreview(call.name, call.input || {});
}

function getRuntimeCallStatusLabel(call: { status: string; isError?: boolean }): string {
  if (call.status === 'running') return '运行中';
  if (call.status === 'error' || call.isError) return '失败';
  return '完成';
}

function getRuntimeCallResultPreview(call: { result?: string }): string {
  const text = (call.result || '').trim();
  if (!text) return '';
  return text.length > 100 ? `${text.slice(0, 100)}...` : text;
}

function getPermissionForGroup(entry: Extract<FeedEntry, { kind: 'tool_msg_group' }>): PermissionRequest | undefined {
  for (const item of entry.items) {
    const permission = props.permissions.get(item.id);
    if (permission) return permission;
  }
  return undefined;
}

function subagentEntryHasPendingPermission(entry: FeedEntry): boolean {
  if (entry.kind === 'tool_msg_group') {
    if (getPermissionForGroup(entry)) return true;
    return (entry.subagentGroups || []).some((group) => subagentGroupHasPendingPermission(group));
  }

  if (entry.kind === 'subagent') {
    return subagentGroupHasPendingPermission(entry);
  }

  return false;
}

function subagentGroupHasPendingPermission(group: SubagentGroup): boolean {
  return group.children.some((entry) => subagentEntryHasPendingPermission(entry));
}

const surfacedPermissionContext = computed<{
  permission: PermissionRequest;
  entryFirstId: string;
  preview: string;
} | null>(() => {
  for (const entry of displayEntries.value) {
    if (entry.kind !== 'tool_msg_group') continue;

    const permission = getPermissionForGroup(entry);
    if (!permission) continue;

    const firstItem = entry.items[0];
    const preview = firstItem
      ? `${getToolLabel(firstItem.name)} ${getToolPreview(firstItem.name, firstItem.input || {})}`.trim()
      : '';

    return {
      permission,
      entryFirstId: entry.firstId,
      preview,
    };
  }

  return null;
});

const surfacedPermission = computed(() => surfacedPermissionContext.value?.permission);
const hasPendingPermissionInTree = computed(() => displayEntries.value.some((entry) => subagentEntryHasPendingPermission(entry)));

onMounted(() => {
  timer = setInterval(() => {
    now.value = Date.now();
  }, 1000);
});

onUnmounted(() => {
  if (timer) {
    clearInterval(timer);
  }
});

watch(hasPendingPermissionInTree, (hasPendingPermission) => {
  if (!hasPendingPermission) return;
  isOpen.value = true;
  recordsOpen.value = true;
}, { immediate: true });

</script>

<template>
  <div class="subagent-view" :class="{ nested: depth > 0 }">
    <button
      class="subagent-header"
      :class="{ expanded: isOpen }"
      @click="toggleSubagentOpen"
    >
      <span
        v-if="overallStatus === 'running'"
        class="status-spinner"
        aria-hidden="true"
      ></span>
      <span
        v-else
        class="status-dot"
        :class="{ error: overallStatus === 'error', success: overallStatus === 'success' }"
      ></span>

      <div class="subagent-main">
        <div class="subagent-summary-line header-summary-line">
          <span
            class="summary-chip"
            :class="{
              running: overallStatus === 'running',
              error: overallStatus === 'error',
              success: overallStatus === 'success',
            }"
          >
            {{ summaryStatusText }}
          </span>
          <span class="summary-item summary-agent">
            <span class="summary-label">Agent</span>
            <span class="summary-value">{{ agentName }}</span>
          </span>
          <span v-if="elapsedLabel" class="summary-item">
            <span class="summary-label">耗时</span>
            <span class="summary-value">{{ elapsedLabel }}</span>
          </span>
          <span class="summary-item">
            <span class="summary-label">工具</span>
            <span class="summary-value">{{ totalToolCallCount }} 次</span>
          </span>
          <span class="summary-item summary-task">
            <span class="summary-label">任务</span>
            <span class="summary-value">{{ currentTaskName }}</span>
          </span>
        </div>
      </div>

      <svg
        viewBox="0 0 16 16"
        fill="currentColor"
        class="chevron"
        :class="{ rotated: isOpen }"
      >
        <path d="M6 4l4 4-4 4" />
      </svg>

    </button>

    <div
      v-if="surfacedPermission"
      class="subagent-permission-bar"
      @click.stop
    >
      <div class="subagent-permission-meta">
        <span class="subagent-permission-label">等待授权</span>
        <span
          v-if="surfacedPermissionContext?.preview"
          class="subagent-permission-preview"
        >
          {{ surfacedPermissionContext.preview }}
        </span>
        <span
          v-else-if="surfacedPermission.description"
          class="subagent-permission-preview"
        >
          {{ surfacedPermission.description }}
        </span>
      </div>
      <div class="subagent-permission-actions">
        <button
          class="subagent-permission-btn approve"
          @click.stop="emit('approve', surfacedPermission.request_id)"
        >
          允许
        </button>
        <button
          class="subagent-permission-btn approve-always"
          @click.stop="emit('approveAlways', surfacedPermission.request_id)"
        >
          始终允许
        </button>
        <button
          class="subagent-permission-btn reject"
          @click.stop="emit('reject', surfacedPermission.request_id)"
        >
          拒绝
        </button>
      </div>
    </div>

    <div v-if="isOpen" class="subagent-content">
      <div v-if="contextDescription" class="subagent-context-line" :title="contextDescription">
        <span class="context-text">{{ contextDescription }}</span>
      </div>

      <div class="subagent-tree">
        <button class="subagent-records-toggle" @click="recordsOpen = !recordsOpen">
          <span class="subagent-records-title">
            调用记录<span v-if="totalToolCallCount > 0">（{{ totalToolCallCount }}）</span>
          </span>
          <svg
            viewBox="0 0 16 16"
            fill="currentColor"
            class="records-chevron"
            :class="{ rotated: recordsOpen }"
          >
            <path d="M6 4l4 4-4 4" />
          </svg>
        </button>

        <div v-if="recordsOpen">
          <div v-if="liveCalls.length > 0" class="subagent-records-title subagent-records-subtitle">
            实时跟踪<span>（{{ liveCalls.length }}）</span>
          </div>

          <div v-if="liveCalls.length > 0" class="live-trace-list">
            <div
              v-for="call in liveCalls"
              :key="call.id"
              class="tree-entry"
            >
              <span class="tree-rail"></span>
              <span class="tree-joint"></span>
              <div
                class="live-trace-item"
                :class="[`status-${call.status}`]"
              >
                <div class="live-trace-head">
                  <span class="live-trace-tool">{{ getToolLabel(call.name) }}</span>
                  <span class="live-trace-status">{{ getRuntimeCallStatusLabel(call) }}</span>
                </div>
                <div v-if="getRuntimeCallPreview(call)" class="live-trace-preview">
                  {{ getRuntimeCallPreview(call) }}
                </div>
                <div v-if="getRuntimeCallResultPreview(call)" class="live-trace-result">
                  {{ getRuntimeCallResultPreview(call) }}
                </div>
              </div>
            </div>
          </div>

          <div class="subagent-entries">
            <template
              v-for="entry in displayEntries"
              :key="entry.kind === 'subagent'
                ? `subagent-${entry.taskToolUseId}`
                : entry.kind === 'tool_msg_group'
                  ? `tool-${entry.firstId}`
                  : `message-${entry.msg.id}`"
            >
              <div class="tree-entry">
                <span class="tree-rail"></span>
                <span class="tree-joint"></span>
                <div
                  v-if="entry.kind === 'tool_msg_group'"
                  class="tree-tool-stack"
                >
                  <MessageGroup
                    :group="entry"
                    :permission="getPermissionForGroup(entry)?.request_id === surfacedPermission?.request_id ? undefined : getPermissionForGroup(entry)"
                    embedded
                    @approve="(id: string, updatedInput?: Record<string, unknown>) => emit('approve', id, updatedInput)"
                    @approve-always="(id: string) => emit('approveAlways', id)"
                    @reject="(id: string, reason?: string) => emit('reject', id, reason)"
                  />
                  <div v-if="entry.subagentGroups?.length" class="tree-nested-subagents">
                    <div
                      v-for="subagent in entry.subagentGroups"
                      :key="subagent.taskToolUseId"
                      class="tree-nested-subagent-item"
                    >
                      <SubagentView
                        :group="subagent"
                        :depth="(depth || 0) + 1"
                        :permissions="props.permissions"
                        :parent-description="contextDescription"
                        @approve="(id: string, updatedInput?: Record<string, unknown>) => emit('approve', id, updatedInput)"
                        @approve-always="(id: string) => emit('approveAlways', id)"
                        @reject="(id: string, reason?: string) => emit('reject', id, reason)"
                      />
                    </div>
                  </div>
                </div>
                <div
                  v-else-if="entry.kind === 'subagent'"
                  class="nested-subagent"
                >
                  <SubagentView
                    :group="entry"
                    :depth="(depth || 0) + 1"
                    :permissions="props.permissions"
                    :parent-description="contextDescription"
                    @approve="(id: string, updatedInput?: Record<string, unknown>) => emit('approve', id, updatedInput)"
                    @approve-always="(id: string) => emit('approveAlways', id)"
                    @reject="(id: string, reason?: string) => emit('reject', id, reason)"
                  />
                </div>
                <MessageItem
                  v-else
                  :message="entry.msg"
                />
              </div>
            </template>
          </div>

          <div v-if="!hasRecords" class="subagent-awaiting">
            <span class="subagent-awaiting-dot"></span>
            <span>{{ hasRunningTool ? '子代理已启动，等待执行步骤...' : '暂无可展示的执行记录' }}</span>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.subagent-view {
  animation: fadeIn 0.2s ease-out;
  margin: 0;
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 18px;
  background: var(--bg-primary, #ffffff);
  overflow: hidden;
}

.subagent-view.nested {
  border-radius: 16px;
}

@keyframes fadeIn {
  from {
    opacity: 0;
    transform: translateY(4px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

.subagent-header {
  display: flex;
  align-items: center;
  gap: 0.6rem;
  width: 100%;
  padding: 0.62rem 0.88rem;
  background: transparent;
  border: none;
  cursor: pointer;
  text-align: left;
  transition: background-color 0.15s;
}

.subagent-header:hover {
  background-color: rgba(148, 163, 184, 0.08);
}

.status-dot {
  width: 8px;
  height: 8px;
  border-radius: 9999px;
  background: var(--text-muted, #9ca3af);
  flex-shrink: 0;
  box-shadow: 0 0 0 3px rgba(148, 163, 184, 0.12);
}

.status-dot.error {
  background: #dc2626;
  box-shadow: 0 0 0 3px rgba(220, 38, 38, 0.12);
}

.status-dot.success {
  background: #16a34a;
  box-shadow: 0 0 0 3px rgba(22, 163, 74, 0.12);
}

.status-spinner {
  width: 12px;
  height: 12px;
  border-radius: 9999px;
  border: 1.8px solid rgba(148, 163, 184, 0.24);
  border-top-color: var(--primary-color, #3b82f6);
  animation: subagentSpin 0.9s linear infinite;
  flex-shrink: 0;
}

@keyframes subagentSpin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}

.subagent-main {
  min-width: 0;
  flex: 1;
}

.subagent-summary-line {
  display: flex;
  align-items: center;
  gap: 0.45rem;
  min-width: 0;
  white-space: nowrap;
  overflow: hidden;
}

.header-summary-line {
  min-height: 1.1rem;
}

.summary-chip {
  display: inline-flex;
  align-items: center;
  padding: 0.1rem 0.42rem;
  border-radius: 9999px;
  font-size: 10px;
  font-weight: 700;
  color: var(--text-secondary, #6b7280);
  background: rgba(148, 163, 184, 0.12);
  flex-shrink: 0;
}

.summary-chip.running {
  color: var(--primary-color, #3b82f6);
  background: rgba(59, 130, 246, 0.1);
}

.summary-chip.error {
  color: #dc2626;
  background: rgba(220, 38, 38, 0.1);
}

.summary-chip.success {
  color: #15803d;
  background: rgba(22, 163, 74, 0.1);
}

.summary-item {
  display: inline-flex;
  align-items: center;
  gap: 0.2rem;
  min-width: 0;
  font-size: 11px;
  color: var(--text-secondary, #6b7280);
  flex-shrink: 0;
  line-height: 1.2;
}

.summary-agent .summary-value {
  font-size: 12px;
  font-weight: 700;
  color: var(--text-primary, #1f2937);
}

.summary-task {
  flex: 1;
  overflow: hidden;
}

.summary-label {
  color: var(--text-muted, #9ca3af);
  flex-shrink: 0;
}

.summary-value {
  min-width: 0;
  color: var(--text-primary, #1f2937);
}

.summary-task .summary-value {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.chevron {
  width: 11px;
  height: 11px;
  color: var(--text-muted, #9ca3af);
  transition: transform 0.2s;
  flex-shrink: 0;
}

.chevron.rotated {
  transform: rotate(90deg);
}

.subagent-permission-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.9rem;
  padding: 0.72rem 0.88rem 0.82rem 1.9rem;
  border-top: 1px solid var(--border-color, #e5e7eb);
  background: linear-gradient(180deg, rgba(59, 130, 246, 0.04), rgba(59, 130, 246, 0.02));
}

.subagent-permission-meta {
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 0.5rem;
  flex-wrap: wrap;
}

.subagent-permission-label {
  display: inline-flex;
  align-items: center;
  padding: 0.14rem 0.48rem;
  border-radius: 9999px;
  font-size: 10px;
  font-weight: 700;
  color: var(--primary-color, #2563eb);
  background: rgba(59, 130, 246, 0.1);
  flex-shrink: 0;
}

.subagent-permission-preview {
  min-width: 0;
  font-size: 12px;
  line-height: 1.45;
  color: var(--text-secondary, #6b7280);
  white-space: normal;
  overflow-wrap: anywhere;
  word-break: break-word;
}

.subagent-permission-actions {
  display: flex;
  align-items: center;
  gap: 0.45rem;
  flex-shrink: 0;
}

.subagent-permission-btn {
  padding: 0.42rem 0.72rem;
  border: 1px solid transparent;
  border-radius: 9999px;
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.15s ease;
  white-space: nowrap;
}

.subagent-permission-btn.approve {
  color: var(--primary-color, #2563eb);
  background: rgba(59, 130, 246, 0.1);
}

.subagent-permission-btn.approve:hover {
  background: rgba(59, 130, 246, 0.16);
}

.subagent-permission-btn.approve-always {
  color: #ffffff;
  background: var(--primary-color, #3b82f6);
}

.subagent-permission-btn.approve-always:hover {
  background: var(--primary-hover, #2563eb);
}

.subagent-permission-btn.reject {
  color: #dc2626;
  background: rgba(220, 38, 38, 0.1);
}

.subagent-permission-btn.reject:hover {
  background: rgba(220, 38, 38, 0.16);
}

.subagent-content {
  padding: 0 1rem 1rem;
}

.subagent-context-line {
  display: flex;
  align-items: flex-start;
  padding: 0.72rem 0.9rem;
  border-radius: 14px;
  background: var(--bg-secondary, #f9fafb);
  min-width: 0;
}

.context-text {
  min-width: 0;
  font-size: 12px;
  color: var(--text-primary, #1f2937);
  line-height: 1.6;
  white-space: normal;
  overflow-wrap: anywhere;
  word-break: break-word;
}

.subagent-tree {
  position: relative;
  margin-top: 0.75rem;
  padding-left: 0.5rem;
}

.subagent-records-title {
  margin-top: 0.95rem;
  margin-bottom: 0.7rem;
  font-size: 13px;
  font-weight: 600;
  color: var(--text-secondary, #6b7280);
}

.subagent-records-toggle {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
  width: 100%;
  margin-top: 0.95rem;
  padding: 0;
  background: transparent;
  border: none;
  cursor: pointer;
  text-align: left;
}

.subagent-records-toggle .subagent-records-title {
  margin: 0;
}

.subagent-records-subtitle {
  margin-top: 0.2rem;
}

.subagent-records-toggle + div {
  margin-top: 0.55rem;
}

.records-chevron {
  width: 13px;
  height: 13px;
  color: var(--text-muted, #9ca3af);
  transition: transform 0.2s;
  flex-shrink: 0;
}

.records-chevron.rotated {
  transform: rotate(90deg);
}

.tree-entry {
  position: relative;
  padding-left: 1.1rem;
}

.tree-entry + .tree-entry {
  margin-top: 0.75rem;
}

.tree-rail {
  position: absolute;
  left: 0.25rem;
  top: 0;
  bottom: -0.75rem;
  width: 1px;
  background: rgba(148, 163, 184, 0.45);
}

.tree-joint {
  position: absolute;
  left: 0;
  top: 0.78rem;
  width: 0.5rem;
  height: 0.5rem;
  border-radius: 9999px;
  border: 2px solid rgba(148, 163, 184, 0.7);
  background: var(--bg-primary, #ffffff);
  box-sizing: border-box;
}

.tree-entry::before {
  content: '';
  position: absolute;
  left: 0.25rem;
  top: 1rem;
  width: 0.7rem;
  height: 1px;
  background: rgba(148, 163, 184, 0.45);
}

.tree-entry:last-child > .tree-rail {
  bottom: calc(100% - 1rem);
}

.live-trace-list {
  display: flex;
  flex-direction: column;
  gap: 0.7rem;
}

.live-trace-item {
  padding: 0.75rem 0.85rem;
  border-radius: 12px;
  border: 1px solid var(--border-color, #e5e7eb);
  background: var(--bg-primary, #ffffff);
}

.live-trace-item.status-running {
  border-color: rgba(59, 130, 246, 0.3);
  background: rgba(59, 130, 246, 0.06);
}

.live-trace-item.status-error {
  border-color: rgba(220, 38, 38, 0.24);
  background: rgba(254, 242, 242, 0.92);
}

.live-trace-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
}

.live-trace-tool {
  font-size: 13px;
  font-weight: 700;
  color: var(--text-primary, #1f2937);
}

.live-trace-status {
  font-size: 11.5px;
  color: var(--text-muted, #9ca3af);
  flex-shrink: 0;
}

.live-trace-preview,
.live-trace-result {
  margin-top: 0.35rem;
  font-size: 12px;
  line-height: 1.45;
  color: var(--text-secondary, #6b7280);
  word-break: break-word;
}

.live-trace-result {
  color: var(--text-secondary, #6b7280);
}

.subagent-entries {
  display: flex;
  flex-direction: column;
  gap: 0;
}

.subagent-entries .tree-entry + .tree-entry {
  margin-top: 0;
}

.tree-tool-stack {
  min-width: 0;
}

.tree-nested-subagents {
  position: relative;
  margin-top: 0.45rem;
  margin-left: 1.2rem;
  padding-left: 0.95rem;
}

.tree-nested-subagents::before {
  content: '';
  position: absolute;
  left: 0.18rem;
  top: 0;
  bottom: 0.9rem;
  width: 1px;
  background: rgba(148, 163, 184, 0.45);
}

.tree-nested-subagent-item {
  position: relative;
}

.tree-nested-subagent-item + .tree-nested-subagent-item {
  margin-top: 0.65rem;
}

.tree-nested-subagent-item::before {
  content: '';
  position: absolute;
  left: -0.76rem;
  top: 1rem;
  width: 0.78rem;
  height: 1px;
  background: rgba(148, 163, 184, 0.45);
}

.tree-nested-subagent-item::after {
  content: '';
  position: absolute;
  left: -0.98rem;
  top: 0.76rem;
  width: 0.5rem;
  height: 0.5rem;
  border-radius: 9999px;
  border: 2px solid rgba(148, 163, 184, 0.7);
  background: var(--bg-primary, #ffffff);
  box-sizing: border-box;
}

.nested-subagent {
  margin-left: 0;
}

.subagent-awaiting {
  display: flex;
  align-items: center;
  gap: 0.55rem;
  margin-top: 0.75rem;
  margin-left: 1.1rem;
  padding: 0.85rem 0.95rem;
  border: 1px dashed rgba(148, 163, 184, 0.55);
  border-radius: 12px;
  color: var(--text-secondary, #6b7280);
  font-size: 12px;
  background: var(--bg-secondary, #f9fafb);
}

.subagent-awaiting-dot {
  width: 0.5rem;
  height: 0.5rem;
  border-radius: 9999px;
  background: var(--primary-color, #3b82f6);
  box-shadow: 0 0 0 4px rgba(59, 130, 246, 0.12);
  flex-shrink: 0;
}

@media (prefers-color-scheme: dark) {
  .subagent-view {
    border-color: var(--border-color, #374151);
    background: var(--bg-secondary, #1f2937);
  }

  .subagent-header:hover {
    background-color: rgba(75, 85, 99, 0.16);
  }

  .summary-item,
  .summary-label,
  .chevron,
  .records-chevron,
  .subagent-records-title,
  .live-trace-status {
    color: var(--text-secondary, #9ca3af);
  }

  .summary-chip {
    background: rgba(75, 85, 99, 0.28);
  }

  .summary-chip.running {
    background: rgba(59, 130, 246, 0.16);
  }

  .summary-chip.error {
    background: rgba(220, 38, 38, 0.16);
  }

  .summary-chip.success {
    color: #4ade80;
    background: rgba(22, 163, 74, 0.18);
  }

  .summary-value,
  .summary-agent .summary-value {
    color: var(--text-primary, #f9fafb);
  }

  .subagent-permission-bar {
    border-top-color: var(--border-color, #374151);
    background: linear-gradient(180deg, rgba(59, 130, 246, 0.08), rgba(59, 130, 246, 0.03));
  }

  .subagent-permission-label {
    color: #93c5fd;
    background: rgba(59, 130, 246, 0.18);
  }

  .subagent-permission-preview {
    color: var(--text-secondary, #9ca3af);
  }

  .subagent-permission-btn.approve {
    color: #93c5fd;
    background: rgba(59, 130, 246, 0.16);
  }

  .subagent-permission-btn.approve:hover {
    background: rgba(59, 130, 246, 0.24);
  }

  .subagent-permission-btn.approve-always {
    background: var(--primary-color, #3b82f6);
  }

  .subagent-permission-btn.approve-always:hover {
    background: #2563eb;
  }

  .subagent-permission-btn.reject {
    color: #fca5a5;
    background: rgba(220, 38, 38, 0.16);
  }

  .subagent-permission-btn.reject:hover {
    background: rgba(220, 38, 38, 0.24);
  }

  .subagent-context-line {
    background: var(--bg-tertiary, #374151);
  }

  .context-text {
    color: var(--text-primary, #f9fafb);
  }

  .live-trace-item {
    border-color: var(--border-color, #374151);
    background: var(--bg-secondary, #1f2937);
  }

  .tree-rail,
  .tree-entry::before,
  .tree-joint {
    background: rgba(107, 114, 128, 0.65);
  }

  .tree-joint {
    border-color: rgba(107, 114, 128, 0.85);
    background: var(--bg-secondary, #1f2937);
  }

  .tree-nested-subagents::before,
  .tree-nested-subagent-item::before {
    background: rgba(107, 114, 128, 0.65);
  }

  .tree-nested-subagent-item::after {
    border-color: rgba(107, 114, 128, 0.85);
    background: var(--bg-secondary, #1f2937);
  }

  .live-trace-item.status-running {
    border-color: rgba(59, 130, 246, 0.4);
    background: rgba(59, 130, 246, 0.12);
  }

  .live-trace-item.status-error {
    border-color: rgba(220, 38, 38, 0.4);
    background: rgba(91, 37, 37, 0.28);
  }

  .live-trace-tool {
    color: var(--text-primary, #f9fafb);
  }

  .live-trace-preview,
  .live-trace-result {
    color: var(--text-secondary, #9ca3af);
  }

  .subagent-awaiting {
    border-color: rgba(107, 114, 128, 0.55);
    color: var(--text-secondary, #9ca3af);
    background: var(--bg-tertiary, #374151);
  }
}

@media (max-width: 860px) {
  .subagent-permission-bar {
    align-items: flex-start;
    flex-direction: column;
    padding-left: 0.88rem;
  }

  .subagent-permission-actions {
    width: 100%;
    flex-wrap: wrap;
  }
}
</style>
