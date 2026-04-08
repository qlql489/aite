<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import type { FeedEntry, SubagentGroup, ToolMessageGroup } from '../../types';
import MessageGroup from './MessageGroup.vue';
import MessageItem from './MessageItem.vue';
import { formatElapsed, getToolLabel, getToolPreview } from '../../utils/messageGrouping';

interface Props {
  group: SubagentGroup;
}

const props = defineProps<Props>();

const isOpen = ref(false);
const now = ref(Date.now());
let timer: ReturnType<typeof setInterval> | null = null;

const childCount = computed(() => props.group.children.length);
const title = computed(() => props.group.agentType || 'Subagent');
const description = computed(() => props.group.description || '未提供子代理说明');

function visitEntries(entries: FeedEntry[], visitor: (entry: FeedEntry) => void): void {
  for (const entry of entries) {
    visitor(entry);
    if (entry.kind === 'subagent') {
      visitEntries(entry.children, visitor);
    }
  }
}

function findLastMeaningfulEntry(entries: FeedEntry[]): FeedEntry | null {
  for (let i = entries.length - 1; i >= 0; i -= 1) {
    const entry = entries[i];
    if (entry.kind === 'subagent') {
      const nested = findLastMeaningfulEntry(entry.children);
      if (nested) return nested;
      return entry;
    }
    if (entry.kind === 'tool_msg_group') {
      return entry;
    }
    if (entry.kind === 'message') {
      const text = (entry.msg.content || '').trim();
      if (text || entry.msg.contentBlocks?.length) {
        return entry;
      }
    }
  }

  return null;
}

function getEntryTimestamp(entry: FeedEntry): number | null {
  if (entry.kind === 'message') return entry.msg.timestamp ?? null;
  if (entry.kind === 'tool_msg_group') return entry.timestamp ?? null;
  return null;
}

const allToolGroups = computed<ToolMessageGroup[]>(() => {
  const groups: ToolMessageGroup[] = [];
  visitEntries(props.group.children, (entry) => {
    if (entry.kind === 'tool_msg_group') {
      groups.push(entry);
    }
  });
  return groups;
});

const toolCallCount = computed(() => {
  return allToolGroups.value.reduce((sum, group) => sum + group.items.length, 0);
});

const hasError = computed(() => {
  return allToolGroups.value.some((group) => group.items.some((item) => item.isError));
});

const hasRunningTool = computed(() => {
  return allToolGroups.value.some((group) => group.items.some((item) => item.result === undefined));
});

const startTimestamp = computed(() => {
  let min: number | null = null;
  visitEntries(props.group.children, (entry) => {
    const timestamp = getEntryTimestamp(entry);
    if (timestamp == null) return;
    min = min == null ? timestamp : Math.min(min, timestamp);
  });
  return min;
});

const endTimestamp = computed(() => {
  let max: number | null = null;
  visitEntries(props.group.children, (entry) => {
    const timestamp = getEntryTimestamp(entry);
    if (timestamp == null) return;
    max = max == null ? timestamp : Math.max(max, timestamp);
  });
  return max;
});

const elapsedMs = computed(() => {
  if (startTimestamp.value == null) return null;
  const end = hasRunningTool.value ? now.value : (endTimestamp.value ?? now.value);
  return Math.max(0, end - startTimestamp.value);
});

const elapsedLabel = computed(() => {
  if (elapsedMs.value == null) return '';
  return formatElapsed(elapsedMs.value);
});

const statusText = computed(() => {
  if (hasRunningTool.value) return '运行中';
  if (hasError.value) return '已出错';
  return '已完成';
});

const latestPreview = computed(() => {
  const lastEntry = findLastMeaningfulEntry(props.group.children);
  if (!lastEntry) return '';

  if (lastEntry.kind === 'tool_msg_group') {
    const item = lastEntry.items[lastEntry.items.length - 1];
    const preview = getToolPreview(lastEntry.toolName, item?.input || {});
    return preview
      ? `${getToolLabel(lastEntry.toolName)} ${preview}`
      : getToolLabel(lastEntry.toolName);
  }

  if (lastEntry.kind === 'message') {
    const text = (lastEntry.msg.content || '').trim();
    if (text) {
      return text.length > 80 ? `${text.slice(0, 80)}...` : text;
    }

    const toolBlock = lastEntry.msg.contentBlocks?.find((block) => block.type === 'tool_use');
    if (toolBlock) {
      const block = toolBlock as { name?: string };
      return getToolLabel(block.name || 'Tool');
    }
  }

  if (lastEntry.kind === 'subagent') {
    return lastEntry.description;
  }

  return '';
});

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

</script>

<template>
  <div class="subagent-view">
    <button
      class="subagent-header"
      :class="{ expanded: isOpen }"
      @click="isOpen = !isOpen"
    >
      <span class="status-dot" :class="{ running: hasRunningTool, error: hasError }"></span>

      <div class="subagent-main">
        <div class="subagent-title-row">
          <span class="subagent-title">{{ title }}</span>
          <span v-if="elapsedLabel" class="subagent-duration">{{ elapsedLabel }}</span>
          <span class="subagent-description">{{ description }}</span>
        </div>

        <div v-if="!isOpen" class="subagent-collapsed-meta">
          <span class="status-inline" :class="{ running: hasRunningTool, error: hasError }">
            {{ statusText }}
          </span>
          <span v-if="toolCallCount > 0" class="meta-inline">调用工具 {{ toolCallCount }} 次</span>
          <span v-if="latestPreview" class="subagent-preview">{{ latestPreview }}</span>
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

    <div v-if="isOpen" class="subagent-content">
      <div class="subagent-stats">
        <span class="status-pill" :class="{ running: hasRunningTool, error: hasError }">
          {{ statusText }}
        </span>
        <span v-if="elapsedLabel" class="stats-pill">
          {{ hasRunningTool ? '已运行' : '总耗时' }} {{ elapsedLabel }}
        </span>
        <span v-if="toolCallCount > 0" class="stats-pill">调用工具 {{ toolCallCount }} 次</span>
        <span class="stats-pill">记录 {{ childCount }} 条</span>
      </div>

      <div class="subagent-prompt">
        “{{ description }}”
      </div>

      <div class="subagent-records-title">
        调用记录<span v-if="toolCallCount > 0">（{{ toolCallCount }}）</span>
      </div>

      <div class="subagent-entries">
        <template v-for="(entry, index) in group.children" :key="index">
          <MessageGroup
            v-if="entry.kind === 'tool_msg_group'"
            :group="entry"
          />
          <div
            v-else-if="entry.kind === 'subagent'"
            class="nested-subagent"
          >
            <SubagentView :group="entry" />
          </div>
          <MessageItem
            v-else
            :message="entry.msg"
          />
        </template>
      </div>
    </div>
  </div>
</template>

<style scoped>
.subagent-view {
  animation: fadeIn 0.2s ease-out;
  margin: 0;
  border: 1px solid rgba(210, 198, 182, 0.9);
  border-radius: 18px;
  background: linear-gradient(180deg, rgba(252, 248, 243, 0.98) 0%, rgba(248, 243, 235, 0.98) 100%);
  overflow: hidden;
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
  gap: 0.75rem;
  width: 100%;
  padding: 0.95rem 1rem;
  background: transparent;
  border: none;
  cursor: pointer;
  text-align: left;
  transition: background-color 0.15s;
}

.subagent-header:hover {
  background-color: rgba(182, 155, 120, 0.08);
}

.status-dot {
  width: 10px;
  height: 10px;
  border-radius: 9999px;
  background: rgba(201, 138, 64, 0.9);
  flex-shrink: 0;
  box-shadow: 0 0 0 4px rgba(201, 138, 64, 0.12);
}

.status-dot.running {
  background: #d97706;
  box-shadow: 0 0 0 4px rgba(217, 119, 6, 0.14);
}

.status-dot.error {
  background: #dc2626;
  box-shadow: 0 0 0 4px rgba(220, 38, 38, 0.12);
}

.subagent-main {
  min-width: 0;
  flex: 1;
}

.subagent-title-row {
  display: flex;
  align-items: baseline;
  gap: 0.55rem;
  min-width: 0;
  color: #2f2a24;
}

.subagent-title {
  font-size: 15px;
  font-weight: 700;
  flex-shrink: 0;
}

.subagent-duration {
  font-size: 12px;
  color: #776a5b;
  flex-shrink: 0;
}

.subagent-description {
  min-width: 0;
  font-size: 13px;
  color: #675c50;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.subagent-collapsed-meta {
  display: flex;
  align-items: center;
  gap: 0.6rem;
  margin-top: 0.28rem;
  min-width: 0;
}

.status-inline,
.meta-inline {
  font-size: 12px;
  color: #8b7e6f;
  flex-shrink: 0;
}

.status-inline.running {
  color: #d97706;
}

.status-inline.error {
  color: #dc2626;
}

.subagent-preview {
  flex: 1;
  font-size: 12px;
  color: #8b7e6f;
  font-family: 'Monaco', 'Menlo', monospace;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.chevron {
  width: 13px;
  height: 13px;
  color: #8b7e6f;
  transition: transform 0.2s;
  flex-shrink: 0;
}

.chevron.rotated {
  transform: rotate(90deg);
}

.subagent-content {
  padding: 0 1rem 1rem;
}

.subagent-stats {
  display: flex;
  flex-wrap: wrap;
  gap: 0.6rem;
  padding: 0.8rem 0.9rem;
  border-radius: 14px;
  background: rgba(245, 236, 225, 0.72);
}

.status-pill,
.stats-pill {
  display: inline-flex;
  align-items: center;
  font-size: 12px;
  color: #6e6255;
}

.status-pill {
  font-weight: 700;
  color: #d97706;
}

.status-pill.error {
  color: #dc2626;
}

.status-pill.running {
  color: #d97706;
}

.subagent-prompt {
  margin-top: 0.85rem;
  padding: 1rem 1.05rem;
  border-radius: 14px;
  background: rgba(221, 220, 213, 0.55);
  color: #3a342f;
  font-size: 14px;
  line-height: 1.6;
  font-style: italic;
}

.subagent-records-title {
  margin-top: 0.95rem;
  margin-bottom: 0.7rem;
  font-size: 13px;
  font-weight: 600;
  color: #6d5f51;
}

.subagent-entries {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.nested-subagent {
  margin-left: 0;
}

@media (prefers-color-scheme: dark) {
  .subagent-view {
    border-color: rgba(93, 78, 65, 0.9);
    background: linear-gradient(180deg, rgba(39, 33, 28, 0.98) 0%, rgba(30, 26, 23, 0.98) 100%);
  }

  .subagent-header:hover {
    background-color: rgba(217, 119, 6, 0.08);
  }

  .subagent-title-row {
    color: #f4ede4;
  }

  .subagent-duration,
  .subagent-description,
  .status-inline,
  .meta-inline,
  .subagent-preview,
  .chevron,
  .stats-pill,
  .subagent-records-title {
    color: #c8b8a6;
  }

  .subagent-stats {
    background: rgba(82, 64, 49, 0.34);
  }

  .subagent-prompt {
    background: rgba(72, 68, 61, 0.5);
    color: #efe6dc;
  }
}
</style>
