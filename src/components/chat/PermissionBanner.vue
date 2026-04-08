<script setup lang="ts">
import { ref } from 'vue';
import type { PermissionRequest } from '../../types';

interface Props {
  permission: PermissionRequest;
  sessionId: string;
}

interface Emits {
  (e: 'approve', requestId: string): void;
  (e: 'approveAlways', requestId: string): void;
  (e: 'reject', requestId: string, reason?: string): void;
}

const props = defineProps<Props>();
const emit = defineEmits<Emits>();

// 格式化权限参数显示
const formatParams = (params: Record<string, unknown> | undefined): string => {
  if (!params) return '';
  return JSON.stringify(params, null, 2);
};

// 获取权限类型图标
const getPermissionIcon = (type: string): string => {
  switch (type) {
    case 'file_read':
      return '📄';
    case 'file_write':
      return '✏️';
    case 'command':
      return '⚡';
    case 'network':
      return '🌐';
    case 'clipboard':
      return '📋';
    default:
      return '🔐';
  }
};

// 获取权限类型标签
const getPermissionLabel = (type: string): string => {
  const labels: Record<string, string> = {
    'file_read': '读取文件',
    'file_write': '写入文件',
    'command': '执行命令',
    'network': '网络请求',
    'clipboard': '剪贴板访问',
  };
  return labels[type] || '权限请求';
};

// 显示建议输入框
const showSuggestionInput = ref(false);
const rejectReasonText = ref('');

// 处理拒绝按钮点击
function handleReject() {
  if (showSuggestionInput.value) {
    emit('reject', props.permission.request_id, rejectReasonText.value.trim() || undefined);
    showSuggestionInput.value = false;
    rejectReasonText.value = '';
  } else {
    showSuggestionInput.value = true;
  }
}

// 取消建议输入
function cancelSuggestion() {
  showSuggestionInput.value = false;
  rejectReasonText.value = '';
}
</script>

<template>
  <div class="permission-banner">
    <div class="permission-content">
      <!-- 权限图标 -->
      <div class="permission-icon">
        {{ getPermissionIcon(permission.type) }}
      </div>

      <!-- 权限详情 -->
      <div class="permission-details">
        <div class="permission-header">
          <span class="permission-type">{{ getPermissionLabel(permission.type) }}</span>
          <span class="permission-description">{{ permission.description }}</span>
        </div>

        <!-- 权限参数（如果有） -->
        <pre v-if="permission.params" class="permission-params">{{ formatParams(permission.params) }}</pre>

        <!-- 建议输入框 -->
        <div v-if="showSuggestionInput" class="suggestion-input-wrapper">
          <textarea
            v-model="rejectReasonText"
            class="suggestion-input"
            placeholder="请输入拒绝原因..."
            rows="2"
          ></textarea>
        </div>
      </div>

      <!-- 操作按钮 -->
      <div class="permission-actions">
        <button
          class="btn-approve-simple"
          @click="emit('approve', permission.request_id)"
          title="仅批准当前请求"
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <polyline points="20 6 9 17 4 12"/>
          </svg>
          允许
        </button>
        <button
          class="btn-approve"
          @click="emit('approveAlways', permission.request_id)"
          title="批准并记住，下次不再询问"
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <polyline points="20 6 9 17 4 12"/>
            <circle cx="12" cy="12" r="3" fill="currentColor" opacity="0.3"/>
          </svg>
          始终允许
        </button>
        <button
          class="btn-reject"
          @click="handleReject"
          :title="showSuggestionInput ? '发送拒绝原因' : '拒绝，并填写拒绝原因'"
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <line x1="18" y1="6" x2="6" y2="18"/>
            <line x1="6" y1="6" x2="18" y2="18"/>
          </svg>
          {{ showSuggestionInput ? '发送拒绝' : '拒绝' }}
        </button>
        <button
          v-if="showSuggestionInput"
          class="btn-cancel"
          @click="cancelSuggestion"
          title="取消"
        >
          取消
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.permission-banner {
  border-top: 1px solid var(--border-color, #e5e7eb);
  background-color: var(--bg-card, #ffffff);
}

.permission-content {
  display: flex;
  align-items: flex-start;
  gap: 0.75rem;
  padding: 0.75rem 1rem;
}

.permission-icon {
  font-size: 1.25rem;
  line-height: 1;
  flex-shrink: 0;
  margin-top: 0.125rem;
}

.permission-details {
  flex: 1;
  min-width: 0;
}

.permission-header {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.permission-type {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-primary, #1f2937);
}

.permission-description {
  font-size: 13px;
  color: var(--text-secondary, #6b7280);
}

.permission-params {
  margin: 0.5rem 0 0;
  padding: 0.5rem;
  background-color: var(--bg-secondary, #f9fafb);
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 0.375rem;
  font-size: 11px;
  color: var(--text-secondary, #6b7280);
  font-family: 'Monaco', 'Menlo', monospace;
  white-space: pre-wrap;
  word-break: break-all;
  max-height: 120px;
  overflow-y: auto;
}

.suggestion-input-wrapper {
  margin: 0.5rem 0 0;
}

.suggestion-input {
  width: 100%;
  padding: 0.5rem;
  background-color: var(--bg-secondary, #f9fafb);
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 0.375rem;
  font-size: 13px;
  color: var(--text-primary, #1f2937);
  font-family: inherit;
  resize: vertical;
  transition: border-color 0.15s;
}

.suggestion-input:focus {
  outline: none;
  border-color: var(--primary-color, #3b82f6);
}

.suggestion-input::placeholder {
  color: var(--text-muted, #9ca3af);
}

.permission-actions {
  display: flex;
  flex-direction: column;
  gap: 0.625rem;
  flex-shrink: 0;
  max-width: 280px;
}

.permission-actions button {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.5rem;
  width: 100%;
  padding: 0.625rem 1rem;
  border: none;
  border-radius: 0.5rem;
  font-size: 14px;
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

.btn-cancel {
  background-color: transparent;
  color: var(--text-muted, #9ca3af);
  border: 1px solid var(--border-color, #e5e7eb);
}

.btn-cancel:hover {
  background-color: var(--bg-tertiary, #f3f4f6);
  color: var(--text-primary, #1f2937);
}

.btn-approve {
  background-color: var(--primary-color, #3b82f6);
  color: #ffffff;
}

.btn-approve:hover {
  background-color: var(--primary-hover, #2563eb);
}

.btn-approve-simple {
  background-color: #dbeafe;
  color: #1d4ed8;
}

.btn-approve-simple:hover {
  background-color: #bfdbfe;
}

/* 深色模式 */
@media (prefers-color-scheme: dark) {
  .permission-banner {
    background-color: var(--bg-card, #1f2937);
    border-top-color: var(--border-color, #374151);
  }

  .permission-type {
    color: var(--text-primary, #f9fafb);
  }

  .permission-params {
    background-color: var(--bg-tertiary, #374151);
    border-color: var(--border-color, #4b5563);
  }

  .suggestion-input {
    background-color: var(--bg-tertiary, #374151);
    border-color: var(--border-color, #4b5563);
    color: var(--text-primary, #f9fafb);
  }

  .suggestion-input:focus {
    border-color: var(--primary-color, #3b82f6);
  }

  .btn-reject {
    background-color: var(--bg-tertiary, #374151);
    color: var(--text-primary, #f9fafb);
  }

  .btn-reject:hover {
    background-color: #7f1d1d;
    color: #fca5a5;
  }

  .btn-cancel {
    border-color: var(--border-color, #4b5563);
    color: var(--text-muted, #9ca3af);
  }

  .btn-cancel:hover {
    background-color: var(--bg-hover, #374151);
    color: var(--text-primary, #f9fafb);
  }

  .btn-approve {
    background-color: var(--primary-color, #3b82f6);
  }

  .btn-approve:hover {
    background-color: #2563eb;
  }

  .btn-approve-simple {
    background-color: rgba(59, 130, 246, 0.2);
    color: #93c5fd;
  }

  .btn-approve-simple:hover {
    background-color: rgba(59, 130, 246, 0.3);
  }
}
</style>
