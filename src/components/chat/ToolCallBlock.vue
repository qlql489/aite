<script setup lang="ts">
import { computed, ref, watch, nextTick, onUnmounted } from 'vue';
import ToolDiffView from './ToolDiffView.vue';
import type { PermissionRequest } from '../../types';

interface Props {
  name: string;
  input: Record<string, unknown>;
  description?: string;
  result?: string;
  isError?: boolean;
  status?: 'running' | 'success' | 'error' | 'approval-requested';
  duration?: number;
  requestId?: string;
  cwd?: string;
  cornerStyle?: 'none' | 'top' | 'bottom' | 'all';
  onApprove?: (requestId: string) => void;
  onReject?: (requestId: string, reason?: string) => void;
  permission?: PermissionRequest;  // 关联的权限请求
}

const props = withDefaults(defineProps<Props>(), {
  status: 'running',
  isError: false,
  cwd: '',
  cornerStyle: 'all',
});

// 跟踪"已批准但等待结果"的状态
const wasApproved = ref(false);

// 监听权限从有到无的变化（表示被批准了）
watch(() => props.permission, (newPerm, oldPerm) => {
  if (oldPerm && !newPerm) {
    // 权限从有到无，说明被批准了
    wasApproved.value = true;
  }
  // 当结果返回后，重置状态
  if (props.result) {
    wasApproved.value = false;
  }
});

// 判断是否显示"执行中"状态
const showExecutingState = computed(() => {
  return wasApproved.value && !props.result && props.status === 'running';
});

// 判断是否为文件工具（根据是否有 file_path）
const isFileTool = computed(() => {
  const hasFilePath = !!(props.input.file_path || props.input.path);
  return hasFilePath;
});

// 提取文件路径
const filePath = computed(() => {
  return (props.input.file_path || props.input.path || '') as string;
});

// 相对路径（去掉项目根目录前缀）
const relativePath = computed(() => {
  if (!filePath.value) return filePath.value;

  const normalizedPath = filePath.value.replace(/\\/g, '/');

  // 如果有 cwd，优先使用
  if (props.cwd) {
    const normalizedCwd = props.cwd.replace(/\\/g, '/');
    if (normalizedPath === normalizedCwd) {
      return '.';
    }
    if (normalizedPath.startsWith(normalizedCwd + '/')) {
      return normalizedPath.slice(normalizedCwd.length + 1);
    }
  }

  // 后备方案：从文件路径中推断项目根目录
  // 查找常见的项目目录标记（src, lib, app, components 等）
  const parts = normalizedPath.split('/');
  const commonProjectDirs = ['src', 'lib', 'app', 'components', 'pages', 'views', 'controllers', 'services', 'models', 'utils', 'hooks', 'store', 'router', 'config', 'doc', 'docs', 'test', 'tests', 'spec', 'specs'];

  // 从后往前找，找到第一个常见项目目录
  for (let i = parts.length - 1; i >= 0; i--) {
    if (commonProjectDirs.includes(parts[i])) {
      // 项目根目录是这个目录的父目录
      return parts.slice(i).join('/');
    }
  }

  // 如果没找到常见项目目录，尝试查找项目标记文件
  // 例如：package.json, Cargo.toml, go.mod, etc.
  // 这里简化处理：返回最后两个部分（目录/文件名）
  if (parts.length >= 2) {
    return parts.slice(-2).join('/');
  }

  // 实在找不到，返回文件名
  return parts[parts.length - 1] || normalizedPath;
});

const emit = defineEmits<{
  approve: [requestId: string, updatedInput?: Record<string, unknown>];
  approveAlways: [requestId: string];
  reject: [requestId: string, reason?: string];
}>();

// 默认不展开，用户手动点击展开
const expanded = ref(false);
const copied = ref(false);
let copiedTimer: ReturnType<typeof setTimeout> | null = null;

// 权限请求相关
const permissionButtonsRef = ref<HTMLElement>();

// 选中的按钮索引
const selectedButtonIndex = ref(0);
const askUserQuestionAnswers = ref<Record<string, string>>({});

// 按钮列表
const buttons = computed(() => [
  { id: 'approve', label: '允许', action: () => emit('approve', props.permission!.request_id) },
  { id: 'approveAlways', label: '始终允许', action: () => emit('approveAlways', props.permission!.request_id) },
  { id: 'reject', label: '拒绝', action: () => handleReject() },
]);

interface AskUserQuestionOption {
  label: string;
  description: string;
  preview?: string;
}

interface AskUserQuestionItem {
  question: string;
  header: string;
  options: AskUserQuestionOption[];
  multiSelect: boolean;
}

const isAskUserQuestionTool = computed(() => props.name === 'AskUserQuestion');

const askUserQuestions = computed<AskUserQuestionItem[]>(() => {
  if (!isAskUserQuestionTool.value) return [];
  const questions = props.input.questions as AskUserQuestionItem[] | undefined;
  return Array.isArray(questions) ? questions : [];
});

const askUserQuestionConfirmDisabled = computed(() => {
  if (!props.permission) return true;
  if (!askUserQuestions.value.length) return true;

  return askUserQuestions.value.some((item) => !askUserQuestionAnswers.value[item.question]?.trim());
});

// 有权限请求时自动展开并滚动到权限按钮
watch(() => props.permission, (newPerm) => {
  if (newPerm) {
    expanded.value = true;
    selectedButtonIndex.value = 0;
    askUserQuestionAnswers.value = isAskUserQuestionTool.value
      ? (((props.input.answers as Record<string, string> | undefined) || {}))
      : {};

    // 等待 DOM 更新后滚动到权限按钮
    nextTick(() => {
      if (permissionButtonsRef.value) {
        permissionButtonsRef.value.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
      }
      // 添加键盘事件监听
      document.addEventListener('keydown', handleKeydown);
    });
  } else {
    // 移除键盘事件监听
    document.removeEventListener('keydown', handleKeydown);
  }
}, { immediate: true });

// 组件卸载时移除事件监听
onUnmounted(() => {
  document.removeEventListener('keydown', handleKeydown);
  if (copiedTimer) {
    clearTimeout(copiedTimer);
  }
});

// 处理键盘事件
function handleKeydown(event: KeyboardEvent) {
  if (!props.permission) return;

  if (isAskUserQuestionTool.value) {
    if (event.key === 'Escape') {
      event.preventDefault();
      handleReject();
      return;
    }

    if (event.key === 'Enter' && !askUserQuestionConfirmDisabled.value) {
      event.preventDefault();
      handleAskUserQuestionConfirm();
    }
    return;
  }

  switch (event.key) {
    case 'ArrowDown':
      event.preventDefault();
      selectedButtonIndex.value = (selectedButtonIndex.value + 1) % buttons.value.length;
      break;
    case 'ArrowUp':
      event.preventDefault();
      selectedButtonIndex.value = (selectedButtonIndex.value - 1 + buttons.value.length) % buttons.value.length;
      break;
    case 'Enter':
      event.preventDefault();
      buttons.value[selectedButtonIndex.value].action();
      break;
    case 'Escape':
      event.preventDefault();
      handleReject();
      break;
  }
}

function handleReject() {
  if (!props.permission) return;
  emit('reject', props.permission.request_id);
}

function toggleAskUserQuestionOption(payload: {
  question: string;
  optionLabel: string;
  multiSelect: boolean;
}) {
  const currentSelection = askUserQuestionAnswers.value[payload.question]
    ?.split(',')
    .map((item) => item.trim())
    .filter(Boolean) || [];

  let nextSelection: string[];
  if (payload.multiSelect) {
    nextSelection = currentSelection.includes(payload.optionLabel)
      ? currentSelection.filter((item) => item !== payload.optionLabel)
      : currentSelection.concat(payload.optionLabel);
  } else {
    nextSelection = [payload.optionLabel];
  }

  askUserQuestionAnswers.value = {
    ...askUserQuestionAnswers.value,
    [payload.question]: nextSelection.join(', '),
  };
}

function updateAskUserQuestionAnswer(payload: {
  question: string;
  value: string;
}) {
  askUserQuestionAnswers.value = {
    ...askUserQuestionAnswers.value,
    [payload.question]: payload.value,
  };
}

function handleAskUserQuestionConfirm() {
  if (!props.permission || askUserQuestionConfirmDisabled.value) return;

  emit('approve', props.permission.request_id, {
    answers: { ...askUserQuestionAnswers.value },
  });
}


// 自动展开功能已禁用
// watch(() => ({ result: props.result, status: props.status }), () => {
//   if (props.result && (props.status === 'success' || props.status === 'error')) {
//     expanded.value = true;
//   }
// }, { immediate: true });

// Bash 命令
const bashCommand = computed(() => {
  return (props.input.command || props.input.cmd || '') as string;
});

// Glob pattern
const globPattern = computed(() => {
  return (props.input.pattern || '') as string;
});

const grepSummary = computed(() => {
  if (props.name !== 'Grep') return '';

  const pattern = String(props.input.pattern || '').trim();
  return pattern;
});

const isSkillTool = computed(() => props.name === 'Skill');

const skillSummary = computed(() => {
  if (props.result?.trim()) {
    return props.result.trim();
  }

  const skill = (props.input.skill || props.input.name || '') as string;
  return skill ? `Skill: ${skill}` : 'Skill';
});

const canExpand = computed(() => !isSkillTool.value || !!props.permission);

const toolLabel = computed(() => {
  switch (props.name) {
    case 'Read': return '读取文件';
    case 'Write': return '写入文件';
    case 'Edit': return '编辑文件';
    case 'Bash': return '终端';
    case 'Glob': return 'Glob';
    case 'Grep': return 'Grep';
    default: return props.name;
  }
});

const toolDescription = computed(() => {
  if (isAskUserQuestionTool.value) return '';
  if (isSkillTool.value) return skillSummary.value;
  if (grepSummary.value) return grepSummary.value;
  if (isFileTool.value) return relativePath.value || filePath.value;
  if (props.name === 'Glob') return globPattern.value;
  if (bashCommand.value) return bashCommand.value;
  if (props.description) return props.description;

  return Object.entries(props.input)
    .filter(([, value]) => value !== undefined && value !== null && String(value).trim() !== '')
    .slice(0, 3)
    .map(([key, value]) => `${key}: ${typeof value === 'string' ? value : JSON.stringify(value)}`)
    .join(' · ');
});

const iconKind = computed(() => {
  switch (props.name) {
    case 'Read': return 'file';
    case 'Write': return 'write';
    case 'Edit': return 'edit';
    case 'Bash': return 'terminal';
    case 'Glob':
    case 'Grep':
    case 'Search': return 'search';
    case 'Skill': return 'spark';
    case 'Task': return 'task';
    default: return 'tool';
  }
});

async function handleCopyDescription() {
  if (!toolDescription.value) return;

  try {
    await navigator.clipboard.writeText(toolDescription.value);
    copied.value = true;
    if (copiedTimer) clearTimeout(copiedTimer);
    copiedTimer = setTimeout(() => {
      copied.value = false;
    }, 1500);
  } catch (error) {
    console.error('[COPY] 复制工具描述失败:', error);
  }
}

</script>

<template>
  <div class="message-block tool-call-block" :class="`corner-${cornerStyle}`">
    <!-- 头部 -->
    <div class="message-block-header tool-header" :class="{ clickable: canExpand }" @click="canExpand ? expanded = !expanded : null">
      <span class="tool-icon" :class="[`icon-${iconKind}`]">
        <svg v-if="iconKind === 'file'" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
          <polyline points="14 2 14 8 20 8"/>
        </svg>
        <svg v-else-if="iconKind === 'write' || iconKind === 'edit'" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M12 20h9"/>
          <path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4Z"/>
        </svg>
        <svg v-else-if="iconKind === 'terminal'" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <polyline points="4 17 10 11 4 5"/>
          <line x1="12" y1="19" x2="20" y2="19"/>
        </svg>
        <svg v-else-if="iconKind === 'search'" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <circle cx="11" cy="11" r="7"/>
          <line x1="21" y1="21" x2="16.65" y2="16.65"/>
        </svg>
        <svg v-else-if="iconKind === 'spark'" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="m12 3-1.9 5.1L5 10l5.1 1.9L12 17l1.9-5.1L19 10l-5.1-1.9Z"/>
        </svg>
        <svg v-else-if="iconKind === 'task'" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M9 11l3 3L22 4"/>
          <path d="M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11"/>
        </svg>
        <svg v-else width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <circle cx="12" cy="12" r="3"/>
          <path d="M12 1v6m0 10v6m11-11h-6M7 12H1"/>
        </svg>
      </span>

      <div class="message-block-main command-text unified-row">
        <span class="message-block-title tool-title" :class="{ error: isError }">{{ toolLabel }}</span>
        <span
          v-if="toolDescription"
          class="message-block-description tool-description"
          :class="{ error: isError }"
          :title="toolDescription"
        >{{ toolDescription }}</span>
      </div>

      <div class="tool-header-actions">
        <button
          v-if="toolDescription"
          :class="['tool-copy-button', { copied }]"
          type="button"
          :title="copied ? '已复制描述' : '复制描述'"
          :aria-label="copied ? '已复制描述' : '复制描述'"
          @click.stop="handleCopyDescription"
        >
          <svg v-if="copied" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M20 6 9 17l-5-5" />
          </svg>
          <svg v-else width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
            <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
          </svg>
        </button>

        <!-- 右侧状态 -->
        <div class="message-block-status status-right">
          <span v-if="duration !== undefined" class="duration">{{ duration < 1000 ? `${duration}ms` : `${(duration / 1000).toFixed(1)}s` }}</span>
          <!-- 成功/失败图标 -->
          <span v-if="status === 'success' && !isError" class="status-check" title="成功">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
              <polyline points="20 6 9 17 4 12"/>
            </svg>
          </span>
          <span v-else-if="status === 'error' || isError" class="status-error" title="失败">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
              <line x1="18" y1="6" x2="6" y2="18"/>
              <line x1="6" y1="6" x2="18" y2="18"/>
            </svg>
          </span>
        </div>

        <!-- 展开箭头 -->
        <span v-if="canExpand" :class="['expand-arrow', expanded ? 'expanded' : '']">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <polyline points="6 9 12 15 18 9"/>
          </svg>
        </span>
      </div>
    </div>

    <!-- 展开内容：执行结果 -->
    <div v-if="expanded && canExpand" class="message-block-content tool-result">
      <!-- 工具执行结果 -->
      <ToolDiffView
        :tool-name="name"
        :input="input"
        :result="result"
        :is-error="isError"
        :cwd="cwd"
        :pending-answers="askUserQuestionAnswers"
        :interactive="!!permission && isAskUserQuestionTool"
        @toggle-ask-user-question-option="toggleAskUserQuestionOption"
        @update-ask-user-question-answer="updateAskUserQuestionAnswer"
      />

      <!-- 执行中状态（已批准但等待结果） -->
      <div v-if="showExecutingState" class="executing-state">
        <div class="executing-content">
          <div class="executing-spinner"></div>
          <span class="executing-text">执行中...</span>
        </div>
      </div>

      <!-- 权限请求按钮（显示在最下面） -->
      <div v-if="permission" class="permission-section" ref="permissionButtonsRef">
        <div class="permission-divider"></div>

        <div v-if="isAskUserQuestionTool" class="permission-actions-inline permission-actions-compact">
          <button
            class="btn-action btn-confirm"
            :disabled="askUserQuestionConfirmDisabled"
            @click.stop="handleAskUserQuestionConfirm"
            title="确认当前选择并继续"
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <polyline points="20 6 9 17 4 12"/>
            </svg>
            确认
          </button>
          <button
            class="btn-action btn-reject"
            @click.stop="handleReject"
            title="拒绝当前操作"
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <line x1="18" y1="6" x2="6" y2="18"/>
              <line x1="6" y1="6" x2="18" y2="18"/>
            </svg>
            拒绝
          </button>
        </div>

        <!-- 操作按钮 -->
        <div v-else class="permission-actions-inline permission-actions-compact">
          <button
            :class="['btn-action', 'btn-approve-simple', { selected: selectedButtonIndex === 0 }]"
            @click.stop="emit('approve', permission.request_id)"
            @mouseenter="selectedButtonIndex = 0"
            title="仅批准当前请求"
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <polyline points="20 6 9 17 4 12"/>
            </svg>
            允许
          </button>
          <button
            :class="['btn-action', 'btn-approve-always', { selected: selectedButtonIndex === 1 }]"
            @click.stop="emit('approveAlways', permission.request_id)"
            @mouseenter="selectedButtonIndex = 1"
            title="批准并记住，下次不再询问"
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <polyline points="20 6 9 17 4 12"/>
              <circle cx="12" cy="12" r="3" fill="currentColor" opacity="0.3"/>
            </svg>
            始终允许
          </button>
          <button
            :class="['btn-action', 'btn-reject', { selected: selectedButtonIndex === 2 }]"
            @click.stop="handleReject"
            @mouseenter="selectedButtonIndex = 2"
            title="拒绝当前操作"
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <line x1="18" y1="6" x2="6" y2="18"/>
              <line x1="6" y1="6" x2="18" y2="18"/>
            </svg>
            拒绝
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.message-block {
  margin: 0;
  border: 1px solid var(--border-color, #e5e7eb);
  overflow: hidden;
  background: var(--bg-secondary, #f9fafb);
  box-shadow: none;
}

.message-block.corner-none {
  border-radius: 0;
}

.message-block.corner-top {
  border-radius: 8px 8px 0 0;
}

.message-block.corner-bottom {
  border-radius: 0 0 8px 8px;
}

.message-block.corner-all {
  border-radius: 8px;
}

.message-block-header,
.tool-header {
  display: flex;
  align-items: center;
  gap: 0.65rem;
  width: 100%;
  padding: 0.42rem 0.95rem;
  text-align: left;
  font-size: 13px;
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
  background: transparent;
  border: none;
  cursor: default;
  transition: background-color 0.15s;
}

.tool-header.clickable {
  cursor: pointer;
}

.tool-header.clickable:hover {
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

.message-block-main,
.command-text {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 0.5rem;
  min-height: 1.25rem;
}

.command-text.unified-row {
  flex-direction: row;
}

.tool-header-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
}

.message-block-title,
.tool-title {
  color: var(--text-primary, #1f2937);
  font-size: 13px;
  font-weight: 500;
  flex-shrink: 0;
}

.tool-title.error {
  color: #dc2626;
}

.message-block-description,
.tool-description {
  min-width: 0;
  color: var(--text-secondary, #6b7280);
  font-family: 'Monaco', 'Menlo', monospace;
  font-size: 12.5px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.tool-description.error {
  color: #dc2626;
}

.tool-result {
  border-top: 1px solid var(--border-color, #e5e7eb);
  background: var(--bg-tertiary, #f3f4f6);
}

@media (prefers-color-scheme: dark) {
  .message-block {
    background: var(--bg-secondary, #1f2937);
    border-color: var(--border-color, #374151);
  }

  .tool-header {
    background: transparent;
  }

  .tool-header.clickable:hover {
    background-color: rgba(75, 85, 99, 0.16);
  }

  .tool-icon {
    color: var(--text-secondary, #9ca3af);
  }

  .tool-title {
    color: var(--text-primary, #f9fafb);
  }

  .tool-description {
    color: var(--text-secondary, #9ca3af);
  }

  .tool-result {
    background: var(--bg-tertiary, #374151);
    border-top-color: var(--border-color, #374151);
  }
}

/* 右侧状态 */
.message-block-status,
.status-right {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
}

.tool-copy-button {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 1.6rem;
  height: 1.6rem;
  padding: 0;
  border: none;
  border-radius: 0.4rem;
  background: transparent;
  color: var(--text-muted, #9ca3af);
  opacity: 0;
  pointer-events: none;
  transition: opacity 0.15s ease, color 0.15s ease, background-color 0.15s ease;
}

.tool-header:hover .tool-copy-button,
.tool-copy-button:focus-visible,
.tool-copy-button.copied {
  opacity: 1;
  pointer-events: auto;
}

.tool-copy-button:hover,
.tool-copy-button:focus-visible {
  background: rgba(148, 163, 184, 0.14);
  color: var(--text-primary, #1f2937);
  outline: none;
}

.tool-copy-button.copied {
  color: var(--primary-color, #3b82f6);
}

.duration {
  font-size: 12px;
  color: var(--text-muted, #9ca3af);
}

.status-check {
  display: flex;
  align-items: center;
  color: #22c55e;
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

/* 权限请求内容区域 */
.tool-result.approval-content {
  padding: 0.625rem 0.75rem;
}

/* 内联操作按钮 - 竖向排列 */
.permission-actions-inline {
  display: flex;
  flex-direction: column;
  gap: 8px;
  width: 100%;
  max-width: 100%;
}

.permission-actions-compact {
  flex-direction: row;
}

.permission-actions-inline button,
.permission-actions-inline .btn-action {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  width: 100%;
  padding: 12px 16px;
  border: none;
  border-radius: 8px;
  font-size: 14px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s;
  white-space: nowrap;
}

.permission-actions-compact button,
.permission-actions-compact .btn-action {
  flex: 1;
}

.permission-actions-inline button:disabled,
.permission-actions-inline .btn-action:disabled {
  cursor: not-allowed;
  opacity: 0.6;
}

/* 选中状态 */
.btn-action.selected {
  outline: 2px solid var(--primary-color, #3b82f6);
  outline-offset: -2px;
}

/* 拒绝按钮 - 默认红色 */
.btn-reject {
  background-color: #fee2e2;
  color: #dc2626;
}

.btn-reject:hover {
  background-color: #fecaca;
  color: #b91c1c;
}

/* 简单批准按钮 */
.btn-approve-simple {
  background-color: #dbeafe;
  color: #1d4ed8;
}

.btn-approve-simple:hover {
  background-color: #bfdbfe;
}

.btn-confirm {
  background-color: #dbeafe;
  color: #1d4ed8;
}

.btn-confirm:hover:not(:disabled) {
  background-color: #bfdbfe;
}

.btn-confirm:disabled {
  background-color: #e5e7eb;
  color: #94a3b8;
  cursor: not-allowed;
}

/* 总是批准按钮 */
.btn-approve-always {
  background-color: var(--primary-color, #3b82f6);
  color: #ffffff;
}

.btn-approve-always:hover {
  background-color: var(--primary-hover, #2563eb);
}

/* 深色模式 - 权限按钮 */
@media (prefers-color-scheme: dark) {
  .btn-reject {
    background-color: rgba(239, 68, 68, 0.2);
    color: #fca5a5;
  }

  .btn-reject:hover {
    background-color: rgba(239, 68, 68, 0.4);
    color: #fecaca;
  }

  .btn-approve-simple {
    background-color: rgba(59, 130, 246, 0.2);
    color: #93c5fd;
  }

  .btn-approve-simple:hover {
    background-color: rgba(59, 130, 246, 0.3);
  }

  .btn-confirm {
    background-color: rgba(59, 130, 246, 0.2);
    color: #93c5fd;
  }

  .btn-confirm:hover:not(:disabled) {
    background-color: rgba(59, 130, 246, 0.3);
  }

  .btn-confirm:disabled {
    background-color: rgba(148, 163, 184, 0.18);
    color: #64748b;
  }

  .btn-approve-always {
    background-color: var(--primary-color, #3b82f6);
  }

  .btn-approve-always:hover {
    background-color: #2563eb;
  }
}

/* 执行中状态 */
.executing-state {
  padding: 16px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.executing-content {
  display: flex;
  align-items: center;
  gap: 12px;
}

.executing-spinner {
  width: 20px;
  height: 20px;
  border: 2px solid var(--border-color, #e5e7eb);
  border-top-color: var(--primary-color, #3b82f6);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

.executing-text {
  font-size: 14px;
  color: var(--text-secondary, #6b7280);
}

/* 深色模式 - 执行中状态 */
@media (prefers-color-scheme: dark) {
  .executing-spinner {
    border-color: var(--border-color, #4b5563);
    border-top-color: var(--primary-color, #3b82f6);
  }

  .executing-text {
    color: var(--text-secondary, #9ca3af);
  }
}

/* 权限部分样式 */
.permission-section {
  padding: 12px 14px;
  border-top: 1px solid var(--border-color, #e5e7eb);
  background: var(--bg-primary, #ffffff);
}

.permission-divider {
  height: 1px;
  background: var(--border-color, #e5e7eb);
  margin-bottom: 12px;
}

/* 深色模式 - 权限部分 */
@media (prefers-color-scheme: dark) {
  .permission-section {
    background: var(--bg-secondary, #1f2937);
    border-top-color: var(--border-color, #374151);
  }

  .permission-divider {
    background: var(--border-color, #374151);
  }
}

@media (max-width: 640px) {
  .permission-actions-compact {
    flex-direction: column;
  }
}
</style>
