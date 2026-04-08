<script setup lang="ts">
import { computed } from 'vue';
import { useTasksStore } from '../../stores/tasks';
import type { TaskStatus } from '../../types';
import TaskItem from './TaskItem.vue';

interface Props {
  sessionId: string;
  filter?: TaskStatus | 'all';
}

const props = withDefaults(defineProps<Props>(), {
  filter: 'all',
});

const emit = defineEmits<{
  taskClick: [taskId: string];
}>();

const tasksStore = useTasksStore();

// 过滤后的任务列表（暂未使用，保留供后续扩展）
// const filteredTasks = computed(() => {
//   if (props.filter === 'all') {
//     return tasksStore.getTasksForSession(props.sessionId);
//   }
//   return tasksStore.getTasksByStatus(props.sessionId, props.filter);
// });

// 可执行的任务（暂未使用，保留供后续扩展）
// const availableTasks = computed(() => {
//   return tasksStore.getAvailableTasks(props.sessionId);
// });

// 统计信息
const stats = computed(() => {
  return tasksStore.getTaskStats(props.sessionId);
});

// 按状态分组的任务
const pendingTasks = computed(() => tasksStore.getTasksByStatus(props.sessionId, 'pending'));
const inProgressTasks = computed(() => tasksStore.getTasksByStatus(props.sessionId, 'in_progress'));
const completedTasks = computed(() => tasksStore.getTasksByStatus(props.sessionId, 'completed'));

// 点击任务
function handleTaskClick(taskId: string) {
  emit('taskClick', taskId);
  tasksStore.setSelectedTask(taskId);
}

// 开始任务
function handleStartTask(taskId: string) {
  tasksStore.startTask(props.sessionId, taskId);
}

// 完成任务
function handleCompleteTask(taskId: string) {
  tasksStore.completeTask(props.sessionId, taskId);
}

// 重置任务
function handleResetTask(taskId: string) {
  tasksStore.resetTask(props.sessionId, taskId);
}

// 删除任务
function handleDeleteTask(taskId: string) {
  tasksStore.removeTask(props.sessionId, taskId);
}
</script>

<template>
  <div class="task-list">
    <!-- 统计信息 -->
    <div class="task-stats">
      <div class="stat-item">
        <span class="stat-label">总计</span>
        <span class="stat-value">{{ stats.total }}</span>
      </div>
      <div class="stat-item">
        <span class="stat-label pending">待处理</span>
        <span class="stat-value">{{ stats.pending }}</span>
      </div>
      <div class="stat-item">
        <span class="stat-label in-progress">进行中</span>
        <span class="stat-value">{{ stats.inProgress }}</span>
      </div>
      <div class="stat-item">
        <span class="stat-label completed">已完成</span>
        <span class="stat-value">{{ stats.completed }}</span>
      </div>
    </div>

    <!-- 任务列表 -->
    <div class="tasks-container">
      <!-- 待处理任务 -->
      <div v-if="pendingTasks.length > 0" class="task-group">
        <div class="task-group-header">
          <span class="group-dot pending"></span>
          <span class="group-label">待处理</span>
          <span class="group-count">{{ pendingTasks.length }}</span>
        </div>
        <TaskItem
          v-for="task in pendingTasks"
          :key="task.id"
          :task="task"
          :session-id="sessionId"
          :can-start="tasksStore.canStartTask(sessionId, task.id)"
          @click="handleTaskClick"
          @start="handleStartTask"
        />
      </div>

      <!-- 进行中任务 -->
      <div v-if="inProgressTasks.length > 0" class="task-group">
        <div class="task-group-header">
          <span class="group-dot in-progress"></span>
          <span class="group-label">进行中</span>
          <span class="group-count">{{ inProgressTasks.length }}</span>
        </div>
        <TaskItem
          v-for="task in inProgressTasks"
          :key="task.id"
          :task="task"
          :session-id="sessionId"
          @click="handleTaskClick"
          @complete="handleCompleteTask"
          @reset="handleResetTask"
        />
      </div>

      <!-- 已完成任务 -->
      <div v-if="completedTasks.length > 0" class="task-group">
        <div class="task-group-header">
          <span class="group-dot completed"></span>
          <span class="group-label">已完成</span>
          <span class="group-count">{{ completedTasks.length }}</span>
        </div>
        <TaskItem
          v-for="task in completedTasks"
          :key="task.id"
          :task="task"
          :session-id="sessionId"
          @click="handleTaskClick"
          @delete="handleDeleteTask"
        />
      </div>

      <!-- 空状态 -->
      <div v-if="stats.total === 0" class="empty-state">
        <div class="empty-icon">📋</div>
        <div class="empty-text">暂无任务</div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.task-list {
  display: flex;
  flex-direction: column;
  height: 100%;
}

/* 统计信息 */
.task-stats {
  display: flex;
  gap: 1rem;
  padding: 0.75rem 1rem;
  background-color: var(--bg-secondary, #f9fafb);
  border-bottom: 1px solid var(--border-color, #e5e7eb);
}

.stat-item {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.25rem;
}

.stat-label {
  font-size: 0.7rem;
  font-weight: 500;
  color: var(--text-muted, #9ca3af);
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

.stat-label.pending {
  color: #f59e0b;
}

.stat-label.in-progress {
  color: #3b82f6;
}

.stat-label.completed {
  color: #10b981;
}

.stat-value {
  font-size: 1rem;
  font-weight: 600;
  color: var(--text-primary, #1f2937);
}

/* 任务容器 */
.tasks-container {
  flex: 1;
  overflow-y: auto;
  padding: 0.75rem;
}

/* 任务组 */
.task-group {
  margin-bottom: 1rem;
}

.task-group:last-child {
  margin-bottom: 0;
}

.task-group-header {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.5rem 0.75rem;
  margin-bottom: 0.5rem;
}

.group-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
}

.group-dot.pending {
  background-color: #f59e0b;
}

.group-dot.in-progress {
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

.group-dot.completed {
  background-color: #10b981;
}

.group-label {
  font-size: 0.75rem;
  font-weight: 600;
  color: var(--text-secondary, #6b7280);
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

.group-count {
  font-size: 0.7rem;
  color: var(--text-muted, #9ca3af);
  background-color: var(--bg-tertiary, #f3f4f6);
  padding: 0.125rem 0.5rem;
  border-radius: 9999px;
  font-weight: 500;
}

/* 空状态 */
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 2rem 1rem;
  text-align: center;
}

.empty-icon {
  font-size: 2rem;
  margin-bottom: 0.5rem;
  opacity: 0.5;
}

.empty-text {
  font-size: 0.875rem;
  color: var(--text-muted, #9ca3af);
}

/* 深色模式 */
@media (prefers-color-scheme: dark) {
  .task-stats {
    background-color: var(--bg-secondary, #1f2937);
    border-bottom-color: var(--border-color, #374151);
  }

  .stat-value {
    color: var(--text-primary, #f9fafb);
  }

  .group-count {
    background-color: var(--bg-tertiary, #374151);
  }
}
</style>
