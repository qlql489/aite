<script setup lang="ts">
import { computed, ref } from 'vue';
import type { TodoWritePanelState } from '../../utils/todoWrite';

interface Props {
  todoState: TodoWritePanelState;
}

const props = defineProps<Props>();

const collapsed = ref(false);

const headerLabel = computed(() => `${props.todoState.completedCount}/${props.todoState.totalCount}`);

const summaryLabel = computed(() => {
  if (props.todoState.completedCount === props.todoState.totalCount) {
    return '全部完成';
  }
  return props.todoState.activeLabel || '待执行';
});
</script>

<template>
  <div class="todo-dock">
    <button type="button" class="todo-dock-header" @click="collapsed = !collapsed">
      <span class="todo-dock-arrow" :class="{ collapsed }" aria-hidden="true">
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2">
          <polyline points="6 9 12 15 18 9" />
        </svg>
      </span>
      <span class="todo-dock-title">待办</span>
      <span class="todo-dock-progress">{{ headerLabel }}</span>
      <span class="todo-dock-summary">{{ summaryLabel }}</span>
    </button>

    <div v-if="!collapsed" class="todo-dock-body">
      <div class="todo-list">
        <div
          v-for="(todo, index) in todoState.todos"
          :key="`${todo.content}-${index}`"
          class="todo-item"
          :class="`status-${todo.status}`"
        >
          <span class="todo-marker" aria-hidden="true">
            <svg
              v-if="todo.status === 'completed'"
              width="11"
              height="11"
              viewBox="0 0 16 16"
              fill="none"
              stroke="currentColor"
              stroke-width="2.2"
            >
              <polyline points="3.2 8.3 6.5 11.4 12.8 4.9" />
            </svg>
            <span v-else-if="todo.status === 'in_progress'" class="todo-spinner"></span>
          </span>
          <span class="todo-content">{{ todo.content }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.todo-dock {
  display: flex;
  flex-direction: column;
  align-self: center;
  gap: 0.35rem;
  width: min(100%, 840px);
  max-width: 100%;
  margin: 0 auto;
  padding: 0.5rem 0.7rem 0.2rem;
  border: 1px solid var(--border-color, rgba(148, 163, 184, 0.28));
  border-radius: 12px;
  background:
    linear-gradient(180deg, rgba(255, 255, 255, 0.98), rgba(248, 250, 252, 0.94));
  box-shadow:
    0 8px 18px rgba(15, 23, 42, 0.06),
    inset 0 1px 0 rgba(255, 255, 255, 0.82);
}

.todo-dock-header {
  display: flex;
  align-items: center;
  gap: 0.45rem;
  width: 100%;
  padding: 0;
  border: none;
  background: transparent;
  color: var(--text-primary, #0f172a);
  cursor: pointer;
  text-align: left;
}

.todo-dock-arrow {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: var(--text-secondary, #64748b);
  transition: transform 0.18s ease;
}

.todo-dock-arrow.collapsed {
  transform: rotate(-90deg);
}

.todo-dock-title {
  font-size: 0.88rem;
  font-weight: 700;
  letter-spacing: 0.01em;
}

.todo-dock-progress {
  font-size: 0.84rem;
  font-weight: 500;
  color: var(--text-secondary, #64748b);
}

.todo-dock-summary {
  min-width: 0;
  margin-left: 0.15rem;
  overflow: hidden;
  color: #2563eb;
  font-size: 0.82rem;
  font-style: italic;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.todo-dock-body {
  max-height: min(180px, 28vh);
  padding: 0.1rem 0 0.15rem;
  overflow-y: auto;
}

.todo-list {
  display: flex;
  flex-direction: column;
  gap: 0.08rem;
}

.todo-item {
  display: flex;
  align-items: center;
  gap: 0.55rem;
  min-height: 1.78rem;
  padding: 0.02rem 0;
  color: var(--text-secondary, #475569);
}

.todo-marker {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 1.05rem;
  height: 1.05rem;
  border: 1.6px solid #cbd5e1;
  border-radius: 999px;
  background: #ffffff;
  color: transparent;
  flex-shrink: 0;
}

.todo-content {
  font-size: 0.84rem;
  line-height: 1.34;
}

.status-completed .todo-marker {
  border-color: #10b981;
  color: #10b981;
}

.status-completed .todo-content {
  color: #9ca3af;
  text-decoration: line-through;
  text-decoration-thickness: 1.25px;
}

.status-in_progress .todo-marker {
  border-color: #4f7cff;
  color: #4f7cff;
}

.status-in_progress .todo-content {
  color: var(--text-primary, #111827);
  font-weight: 600;
}

.todo-spinner {
  width: 0.58rem;
  height: 0.58rem;
  border: 1.6px solid rgba(79, 124, 255, 0.28);
  border-top-color: #4f7cff;
  border-radius: 999px;
  animation: todo-spin 0.78s linear infinite;
}

@keyframes todo-spin {
  to {
    transform: rotate(360deg);
  }
}

@media (max-width: 720px) {
  .todo-dock {
    width: 100%;
    padding: 0.48rem 0.62rem 0.16rem;
  }

  .todo-dock-header {
    flex-wrap: wrap;
    row-gap: 0.16rem;
  }

  .todo-dock-summary {
    width: 100%;
    margin-left: 1.2rem;
    white-space: normal;
  }
}
</style>
