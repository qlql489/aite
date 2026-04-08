<script setup lang="ts">
import { ref, computed, withDefaults } from 'vue';
import type { ToolMessageGroup, PermissionRequest } from '../../types';
import { getToolLabel } from '../../utils/messageGrouping';
import ToolCallBlock from './ToolCallBlock.vue';
import ToolDiffView from './ToolDiffView.vue';
import { useStatsStore } from '../../stores/stats';

interface Props {
  group: ToolMessageGroup;
  cornerStyle?: 'none' | 'top' | 'bottom' | 'all';
  permission?: PermissionRequest;  // 关联的权限请求
}

const props = withDefaults(defineProps<Props>(), {
  cornerStyle: 'all',
});
const statsStore = useStatsStore();

interface Emits {
  (e: 'approve', requestId: string, updatedInput?: Record<string, unknown>): void;
  (e: 'approveAlways', requestId: string): void;
  (e: 'reject', requestId: string, reason?: string): void;
}

const emit = defineEmits<Emits>();

const isOpen = ref(false);

// 计算整体状态（基于所有项）
const overallStatus = computed(() => {
  const hasError = props.group.items.some(item => item.isError);
  const hasResult = props.group.items.some(item => item.result !== undefined);
  if (hasError) return 'error';
  if (hasResult) return 'success';
  return 'running';
});

// 工具名称
const toolName = computed(() => getToolLabel(props.group.toolName));

// 获取当前项目路径
const currentCwd = computed(() => statsStore.selectedProjectPath || '');

// 数量
const count = computed(() => props.group.items.length);

// 自动展开逻辑已禁用
</script>

<template>
  <div class="message-group" :class="[`corner-${cornerStyle}`]">
    <!-- 单个工具调用直接使用 ToolCallBlock -->
    <ToolCallBlock
      v-if="count === 1"
      :name="group.toolName"
      :input="group.items[0].input"
      :result="group.items[0].result"
      :status="overallStatus"
      :is-error="group.items[0].isError"
      :cwd="currentCwd"
      :corner-style="cornerStyle"
      :permission="permission"
      @approve="(id: string, updatedInput?: Record<string, unknown>) => emit('approve', id, updatedInput)"
      @approve-always="(id: string) => emit('approveAlways', id)"
      @reject="(id: string, reason?: string) => emit('reject', id, reason)"
    />

    <!-- 多个工具调用使用分组显示 -->
    <template v-else>
      <!-- 头部 -->
      <div class="tool-header" @click="isOpen = !isOpen">
        <span class="tool-icon">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <circle cx="11" cy="11" r="7"/>
            <line x1="21" y1="21" x2="16.65" y2="16.65"/>
          </svg>
        </span>

        <span class="tool-label">{{ toolName }}</span>

        <!-- 右侧状态 -->
        <div class="status-right">
          <!-- 数量徽章 -->
          <span class="tool-count">{{ count }}</span>
          <!-- 成功/失败图标 -->
          <span v-if="overallStatus === 'success'" class="status-check" title="成功">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
              <polyline points="20 6 9 17 4 12"/>
            </svg>
          </span>
          <span v-else-if="overallStatus === 'error'" class="status-error" title="失败">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
              <line x1="18" y1="6" x2="6" y2="18"/>
              <line x1="6" y1="6" x2="18" y2="18"/>
            </svg>
          </span>
          <!-- 展开箭头 -->
          <span :class="['expand-arrow', isOpen ? 'expanded' : '']">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <polyline points="6 9 12 15 18 9"/>
            </svg>
          </span>
        </div>
      </div>

      <!-- 展开内容：执行结果 -->
      <div v-if="isOpen" class="tool-results">
        <div
          v-for="(item, index) in group.items"
          :key="item.id || index"
          :class="['tool-result-item', { error: item.isError }]"
        >
          <div class="item-number">{{ index + 1 }}</div>
          <ToolDiffView
            :tool-name="item.name"
            :input="item.input"
            :result="item.result"
            :is-error="item.isError"
            :cwd="currentCwd"
          />
        </div>
      </div>
    </template>
  </div>
</template>

<style scoped>
.message-group {
  margin: 0;
}

.message-group.corner-none,
.message-group.corner-bottom {
  margin-top: -1px;
}

/* 多项工具调用的头部 */
.tool-header {
  display: flex;
  align-items: center;
  gap: 0.65rem;
  width: 100%;
  padding: 0.42rem 0.95rem;
  text-align: left;
  font-size: 13px;
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
  background: var(--bg-secondary, #f9fafb);
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 0;
  cursor: pointer;
  transition: background-color 0.15s;
}

.tool-header:hover {
  background-color: rgba(148, 163, 184, 0.08);
}

.tool-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 1rem;
  height: 1rem;
  color: var(--text-secondary, #6b7280);
  flex-shrink: 0;
}

/* 工具标签 */
.tool-label {
  flex: 1;
  color: var(--text-primary, #1f2937);
  font-weight: 500;
}

/* 右侧状态 */
.status-right {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
}

.tool-count {
  font-size: 10px;
  color: var(--text-muted, #9ca3af);
  background-color: var(--bg-hover, #f3f4f6);
  border-radius: 9999px;
  padding: 0.125rem 0.375rem;
  font-weight: 500;
}

.status-check {
  display: flex;
  align-items: center;
  color: var(--primary-color, #3b82f6);
}

.status-error {
  display: flex;
  align-items: center;
  color: #ef4444;
}

.expand-arrow {
  display: flex;
  align-items: center;
  color: var(--text-muted, #9ca3af);
  transition: transform 0.2s;
}

.expand-arrow.expanded {
  transform: rotate(180deg);
}

/* 执行结果 */
.tool-results {
  margin-top: 0.2rem;
}

.tool-result-item {
  position: relative;
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 0;
  overflow: hidden;
  background: var(--bg-secondary, #f9fafb);
  margin-bottom: 0;
}

.tool-result-item:last-child {
  margin-bottom: 0;
}

.tool-result-item.error {
  background: #fef2f2;
  border-color: #fca5a5;
}

.tool-result-item > .item-number {
  position: absolute;
  top: 8px;
  right: 8px;
  font-size: 10px;
  color: var(--text-muted, #9ca3af);
  background-color: var(--bg-primary, #ffffff);
  border-radius: 4px;
  padding: 2px 6px;
  z-index: 1;
}

/* 深色模式 */
@media (prefers-color-scheme: dark) {
  .tool-header {
    background: var(--bg-secondary, #1f2937);
    border-color: var(--border-color, #374151);
  }

  .tool-header:hover {
    background-color: rgba(75, 85, 99, 0.16);
  }

  .tool-icon {
    color: var(--text-secondary, #9ca3af);
  }

  .tool-label {
    color: var(--text-primary, #f9fafb);
  }

  .tool-results {
    background: transparent;
  }

  .tool-result-item {
    background: var(--bg-secondary, #1f2937);
    border-color: var(--border-color, #374151);
  }

  .tool-result-item.error {
    background: #450a0a;
    border-color: #7f1d1d;
  }

  .tool-result-item > .item-number {
    background-color: var(--bg-secondary, #1f2937);
  }

  .tool-count {
    background-color: var(--bg-hover, #4b5563);
  }
}
</style>
