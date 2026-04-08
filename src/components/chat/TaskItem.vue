<script setup lang="ts">
import { computed } from 'vue';
import { useTasksStore } from '../../stores/tasks';
import type { TaskItem as TaskItemType } from '../../types';

interface Props {
  task: TaskItemType;
  sessionId: string;
  canStart?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  canStart: false,
});

const emit = defineEmits<{
  click: [taskId: string];
  start: [taskId: string];
  complete: [taskId: string];
  reset: [taskId: string];
  delete: [taskId: string];
}>();

const tasksStore = useTasksStore();

// 阻塞此任务的任务列表
const blockingTasks = computed(() => {
  return tasksStore.getBlockingTasks(props.sessionId, props.task.id);
});

// 被此任务阻塞的任务列表
const blockedTasks = computed(() => {
  return tasksStore.getBlockedTasks(props.sessionId, props.task.id);
});

// 任务状态样式
const statusClass = computed(() => {
  switch (props.task.status) {
    case 'pending':
      return 'pending';
    case 'in_progress':
      return 'in-progress';
    case 'completed':
      return 'completed';
    default:
      return '';
  }
});

// 是否可交互
const isInteractive = computed(() => {
  return props.task.status !== 'completed' || props.canStart;
});

// 点击任务
function handleClick() {
  if (!isInteractive.value) return;
  emit('click', props.task.id);
}

// 开始任务
function handleStart(e: Event) {
  e.stopPropagation();
  if (!props.canStart) return;
  emit('start', props.task.id);
}

// 完成任务
function handleComplete(e: Event) {
  e.stopPropagation();
  emit('complete', props.task.id);
}

// 重置任务
function handleReset(e: Event) {
  e.stopPropagation();
  emit('reset', props.task.id);
}

// 删除任务
function handleDelete(e: Event) {
  e.stopPropagation();
  emit('delete', props.task.id);
}
</script>

<template>
  <div
    :class="['task-item', statusClass, { interactive: isInteractive, 'can-start': canStart }]"
    @click="handleClick"
  >
    <!-- 任务状态指示器 -->
    <div class="task-indicator">
      <span v-if="task.status === 'completed'" class="check-icon">✓</span>
      <span v-else-if="task.status === 'in_progress'" class="progress-dot"></span>
      <span v-else class="pending-dot"></span>
    </div>

    <!-- 任务内容 -->
    <div class="task-content">
      <div class="task-header">
        <span class="task-subject">{{ task.subject }}</span>
        <span v-if="task.owner" class="task-owner">{{ task.owner }}</span>
      </div>

      <div v-if="task.description" class="task-description">
        {{ task.description }}
      </div>

      <!-- 阻塞关系 -->
      <div v-if="blockingTasks.length > 0" class="task-blocks">
        <span class="blocks-label">等待:</span>
        <span v-for="(block, index) in blockingTasks" :key="block.id" class="block-item">
          {{ block.subject }}<template v-if="index < blockingTasks.length - 1">,</template>
        </span>
      </div>

      <div v-if="blockedTasks.length > 0" class="task-blocks">
        <span class="blocks-label">阻塞:</span>
        <span v-for="(block, index) in blockedTasks" :key="block.id" class="block-item">
          {{ block.subject }}<template v-if="index < blockedTasks.length - 1">,</template>
        </span>
      </div>
    </div>

    <!-- 任务操作 -->
    <div class="task-actions" @click.stop>
      <!-- 待处理状态：显示开始按钮 -->
      <button
        v-if="task.status === 'pending' && canStart"
        class="action-btn start"
        @click="handleStart"
        title="开始任务"
      >
        <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
          <path d="M4 4l8 4-8 4V4z"/>
        </svg>
      </button>

      <!-- 进行中状态：显示完成按钮 -->
      <button
        v-if="task.status === 'in_progress'"
        class="action-btn complete"
        @click="handleComplete"
        title="完成任务"
      >
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M3 8l3 3 7-7"/>
        </svg>
      </button>

      <!-- 进行中状态：显示重置按钮 -->
      <button
        v-if="task.status === 'in_progress'"
        class="action-btn reset"
        @click="handleReset"
        title="重置任务"
      >
        <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M8 3v5m0 0v5m0-5h5m-5 0H3"/>
        </svg>
      </button>

      <!-- 已完成状态：显示删除按钮 -->
      <button
        v-if="task.status === 'completed'"
        class="action-btn delete"
        @click="handleDelete"
        title="删除任务"
      >
        <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M3 4h10M8 4v8M5 4l1 10M11 4l-1 10"/>
        </svg>
      </button>
    </div>
  </div>
</template>

<style scoped>
.task-item {
  display: flex;
  align-items: flex-start;
  gap: 0.75rem;
  padding: 0.75rem;
  background-color: var(--bg-primary, #ffffff);
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 0.5rem;
  margin-bottom: 0.5rem;
  transition: all 0.2s;
}

.task-item.interactive {
  cursor: pointer;
}

.task-item.interactive:hover {
  border-color: var(--text-secondary, #6b7280);
  box-shadow: 0 2px 4px rgba(0, 0, 0, 0.05);
}

.task-item.can-start {
  border-color: #3b82f6;
}

/* 状态样式 */
.task-item.pending {
  border-left: 3px solid #f59e0b;
}

.task-item.in-progress {
  border-left: 3px solid #3b82f6;
}

.task-item.completed {
  border-left: 3px solid #10b981;
  opacity: 0.7;
}

/* 指示器 */
.task-indicator {
  flex-shrink: 0;
  width: 18px;
  height: 18px;
  display: flex;
  align-items: center;
  justify-content: center;
  margin-top: 0.125rem;
}

.pending-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background-color: #f59e0b;
}

.progress-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background-color: #3b82f6;
  animation: pulse 1.4s infinite ease-in-out;
}

@keyframes pulse {
  0%, 60%, 100% {
    opacity: 1;
  }
  30% {
    opacity: 0.6;
  }
}

.check-icon {
  width: 14px;
  height: 14px;
  border-radius: 50%;
  background-color: #10b981;
  color: #ffffff;
  font-size: 10px;
  font-weight: bold;
  display: flex;
  align-items: center;
  justify-content: center;
}

/* 内容 */
.task-content {
  flex: 1;
  min-width: 0;
}

.task-header {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin-bottom: 0.25rem;
}

.task-subject {
  font-size: 0.875rem;
  font-weight: 500;
  color: var(--text-primary, #1f2937);
}

.task-owner {
  font-size: 0.7rem;
  color: var(--text-muted, #9ca3af);
  background-color: var(--bg-tertiary, #f3f4f6);
  padding: 0.125rem 0.375rem;
  border-radius: 9999px;
}

.task-description {
  font-size: 0.75rem;
  color: var(--text-secondary, #6b7280);
  line-height: 1.4;
  margin-bottom: 0.25rem;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.task-blocks {
  display: flex;
  flex-wrap: wrap;
  gap: 0.25rem;
  margin-top: 0.375rem;
  font-size: 0.7rem;
}

.blocks-label {
  color: var(--text-muted, #9ca3af);
}

.block-item {
  color: var(--text-secondary, #6b7280);
  background-color: var(--bg-tertiary, #f3f4f6);
  padding: 0.125rem 0.375rem;
  border-radius: 0.25rem;
}

/* 操作按钮 */
.task-actions {
  display: flex;
  gap: 0.25rem;
  flex-shrink: 0;
}

.action-btn {
  width: 28px;
  height: 28px;
  border-radius: 0.375rem;
  border: none;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all 0.2s;
}

.action-btn.start {
  background-color: #3b82f6;
  color: #ffffff;
}

.action-btn.start:hover {
  background-color: #2563eb;
}

.action-btn.complete {
  background-color: #10b981;
  color: #ffffff;
}

.action-btn.complete:hover {
  background-color: #059669;
}

.action-btn.reset {
  background-color: var(--bg-tertiary, #f3f4f6);
  color: var(--text-secondary, #6b7280);
}

.action-btn.reset:hover {
  background-color: #e5e7eb;
}

.action-btn.delete {
  background-color: var(--bg-tertiary, #f3f4f6);
  color: var(--text-secondary, #6b7280);
}

.action-btn.delete:hover {
  background-color: #fee2e2;
  color: #dc2626;
}

/* 深色模式 */
@media (prefers-color-scheme: dark) {
  .task-item {
    background-color: var(--bg-card, #1f2937);
    border-color: var(--border-color, #374151);
  }

  .task-item.interactive:hover {
    border-color: var(--text-secondary, #9ca3af);
  }

  .task-subject {
    color: var(--text-primary, #f9fafb);
  }

  .task-owner {
    background-color: var(--bg-tertiary, #374151);
  }

  .block-item {
    background-color: var(--bg-tertiary, #374151);
  }

  .action-btn.reset,
  .action-btn.delete {
    background-color: var(--bg-tertiary, #374151);
    color: var(--text-secondary, #9ca3af);
  }

  .action-btn.reset:hover {
    background-color: #4b5563;
  }

  .action-btn.delete:hover {
    background-color: #7f1d1d;
    color: #fca5a5;
  }
}
</style>
