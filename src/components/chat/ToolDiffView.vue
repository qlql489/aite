<script setup lang="ts">
import { computed } from 'vue';

interface Props {
  toolName: string;
  input: Record<string, unknown>;
  result?: string;
  isError?: boolean;
  cwd?: string;
  pendingAnswers?: Record<string, string>;
  interactive?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  isError: false,
  cwd: '',
  pendingAnswers: () => ({}),
  interactive: false,
});

const emit = defineEmits<{
  toggleAskUserQuestionOption: [payload: { question: string; optionLabel: string; multiSelect: boolean }];
  updateAskUserQuestionAnswer: [payload: { question: string; value: string }];
}>();

// AskUserQuestion 相关类型
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

// 判断是否为 AskUserQuestion 工具
const isAskUserQuestion = computed(() => {
  return props.toolName === 'AskUserQuestion';
});

// 解析 AskUserQuestion 的 input
const askUserQuestions = computed(() => {
  if (!isAskUserQuestion.value) return [];
  const questions = props.input.questions as AskUserQuestionItem[];
  return Array.isArray(questions) ? questions : [];
});

function getSelectedLabels(question: string): string[] {
  const rawValue = props.pendingAnswers?.[question];
  if (!rawValue) return [];

  return rawValue
    .split(',')
    .map((item) => item.trim())
    .filter(Boolean);
}

function isOptionSelected(question: string, optionLabel: string): boolean {
  return getSelectedLabels(question).includes(optionLabel);
}

function toggleAskUserQuestionOption(question: string, optionLabel: string, multiSelect: boolean) {
  if (!props.interactive) return;

  emit('toggleAskUserQuestionOption', {
    question,
    optionLabel,
    multiSelect,
  });
}

function updateAskUserQuestionAnswer(question: string, value: string) {
  if (!props.interactive) return;

  emit('updateAskUserQuestionAnswer', {
    question,
    value,
  });
}

// 解析持久化输出
const persistedOutputInfo = computed(() => {
  if (!props.result) return null;
  const match = props.result.match(/Full output saved to: (.+?\.txt)/);
  if (match) {
    return { filePath: match[1] };
  }
  return null;
});

// 判断是否为持久化输出
const isPersistedOutput = computed(() => {
  return props.result?.includes('<persisted-output>');
});

// 判断是否为 Edit 或 Write 工具
const isEditOrWrite = computed(() => {
  return props.toolName === 'Edit' || props.toolName === 'Write';
});

const isReadTool = computed(() => {
  return props.toolName === 'Read';
});

const shouldShowErrorResult = computed(() => {
  return !!props.isError && !!props.result;
});

interface DiffLine {
  type: 'unchanged' | 'removed' | 'added';
  content: string;
  lineNum?: number;
  oldLineNum?: number;
  newLineNum?: number;
}

// 计算 diff 信息
const diffLines = computed<DiffLine[]>(() => {
  if (props.toolName === 'Edit') {
    return computeEditDiff(props.input);
  } else if (props.toolName === 'Write') {
    return computeWriteDiff(props.input);
  }
  return [];
});

/**
 * 计算 Edit 工具的 diff
 * 通过比较 old_string 和 new_string 生成 diff 行
 */
function computeEditDiff(input: Record<string, unknown>): DiffLine[] {
  const oldString = (input.old_string as string) || '';
  const newString = (input.new_string as string) || '';

  const oldLines = oldString.split('\n');
  const newLines = newString.split('\n');

  // 使用更智能的 diff 算法：查找变更点
  const result: DiffLine[] = [];

  // 1. 查找共同前缀
  let prefixEnd = 0;
  while (
    prefixEnd < oldLines.length &&
    prefixEnd < newLines.length &&
    oldLines[prefixEnd] === newLines[prefixEnd]
  ) {
    prefixEnd++;
  }

  // 2. 查找共同后缀
  let suffixStart = 0;
  while (
    suffixStart < oldLines.length - prefixEnd &&
    suffixStart < newLines.length - prefixEnd &&
    oldLines[oldLines.length - 1 - suffixStart] === newLines[newLines.length - 1 - suffixStart]
  ) {
    suffixStart++;
  }

  // 3. 添加前缀上下文（unchanged）
  for (let i = 0; i < prefixEnd; i++) {
    result.push({
      type: 'unchanged',
      content: oldLines[i],
      lineNum: i + 1,
      oldLineNum: i + 1,
      newLineNum: i + 1
    });
  }

  // 4. 处理变更区域
  const changeStart = prefixEnd;
  const oldChangeEnd = oldLines.length - suffixStart;
  const newChangeEnd = newLines.length - suffixStart;

  // 删除的行（红色）
  for (let i = changeStart; i < oldChangeEnd; i++) {
    result.push({
      type: 'removed',
      content: oldLines[i],
      oldLineNum: i + 1
    });
  }

  // 新增的行（绿色）
  for (let i = changeStart; i < newChangeEnd; i++) {
    result.push({
      type: 'added',
      content: newLines[i],
      newLineNum: prefixEnd + (i - changeStart) + 1
    });
  }

  // 5. 添加后缀上下文（unchanged）
  for (let i = 0; i < suffixStart; i++) {
    const oldIdx = oldLines.length - suffixStart + i;
    const newIdx = newLines.length - suffixStart + i;
    result.push({
      type: 'unchanged',
      content: oldLines[oldIdx],
      lineNum: newIdx + 1,
      oldLineNum: oldIdx + 1,
      newLineNum: newIdx + 1
    });
  }

  return result;
}

/**
 * 计算 Write 工具的 diff
 * Write 工具创建新文件，所有行都是新增
 */
function computeWriteDiff(input: Record<string, unknown>): DiffLine[] {
  const content = (input.content as string) || '';
  const lines = content.split('\n');

  return lines.map((line, index) => ({
    type: 'added' as const,
    content: line,
    lineNum: index + 1,
    newLineNum: index + 1
  }));
}

// 获取 diff 显示的最大行数
const maxDisplayLines = 200;
const displayLines = computed(() => {
  return diffLines.value.slice(0, maxDisplayLines);
});

const hasMoreLines = computed(() => {
  return diffLines.value.length > maxDisplayLines;
});

interface ReadLine {
  lineNumber: string;
  content: string;
}

const readLines = computed<ReadLine[]>(() => {
  if (!isReadTool.value || !props.result) return [];

  return props.result.split('\n').map((line) => {
    const match = line.match(/^(\d+)([-:|])?(.*)$/);
    if (!match) {
      return { lineNumber: '', content: line };
    }

    const [, number, , rest] = match;
    return {
      lineNumber: number,
      content: rest ?? '',
    };
  });
});

// 获取状态样式
const statusStyle = computed(() => {
  if (props.isError) {
    return {
      bg: '#fef2f2',
      border: '#fecaca'
    };
  }
  return null;
});
</script>

<template>
  <div class="tool-diff-view">
    <!-- AskUserQuestion 工具显示 -->
    <template v-if="isAskUserQuestion">
      <div v-for="(qItem, idx) in askUserQuestions" :key="idx" class="ask-user-question">
        <!-- 问题标题 -->
        <div v-if="qItem.header" class="question-header">{{ qItem.header }}</div>

        <!-- 问题内容 -->
        <div class="question-text">{{ qItem.question }}</div>

        <!-- 选项列表 -->
        <div class="question-options">
          <div
            v-for="(option, optIdx) in qItem.options"
            :key="optIdx"
            :class="[
              'question-option',
              {
                selectable: interactive,
                selected: isOptionSelected(qItem.question, option.label),
                'single-select': !qItem.multiSelect,
                'multi-select': qItem.multiSelect,
              }
            ]"
            :role="qItem.multiSelect ? 'checkbox' : 'radio'"
            :aria-checked="isOptionSelected(qItem.question, option.label)"
            :tabindex="interactive ? 0 : -1"
            @click="toggleAskUserQuestionOption(qItem.question, option.label, qItem.multiSelect)"
            @keydown.enter.prevent="toggleAskUserQuestionOption(qItem.question, option.label, qItem.multiSelect)"
            @keydown.space.prevent="toggleAskUserQuestionOption(qItem.question, option.label, qItem.multiSelect)"
          >
            <span class="option-bullet">
              <template v-if="interactive">
                <svg
                  v-if="isOptionSelected(qItem.question, option.label)"
                  width="14"
                  height="14"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="3"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  aria-hidden="true"
                >
                  <polyline points="20 6 9 17 4 12"/>
                </svg>
              </template>
              <template v-else>
                {{ optIdx + 1 }}
              </template>
            </span>
            <div class="option-content">
              <div class="option-label-row">
                <div class="option-label">{{ option.label }}</div>
                <span
                  v-if="interactive && isOptionSelected(qItem.question, option.label)"
                  class="option-selected-tag"
                >
                  已选
                </span>
              </div>
              <div v-if="option.description" class="option-description">{{ option.description }}</div>
              <div v-if="option.preview" class="option-preview">{{ option.preview }}</div>
            </div>
          </div>
        </div>

        <div v-if="interactive" class="selection-mode">
          {{ qItem.multiSelect ? '多选：可选择多个选项' : '单选：仅可选择一个选项' }}
        </div>

        <div v-if="interactive" class="answer-input-panel">
          <label class="answer-input-label">或直接输入回答</label>
          <input
            :value="pendingAnswers?.[qItem.question] || ''"
            class="answer-input"
            type="text"
            placeholder="不选项也可以直接输入"
            @input="updateAskUserQuestionAnswer(qItem.question, ($event.target as HTMLInputElement).value)"
            @keydown.enter.stop
          />
        </div>

        <!-- 用户选择结果 -->
        <div v-if="result || pendingAnswers?.[qItem.question]" class="question-result">
          <div class="result-header">用户选择</div>
          <pre class="result-content">{{ result || pendingAnswers?.[qItem.question] }}</pre>
        </div>
      </div>
    </template>

    <!-- Diff 内容 -->
    <div v-else-if="shouldShowErrorResult" :class="['plain-result', { error: isError }]">
      <pre>{{ result }}</pre>
    </div>

    <div
      v-else-if="isEditOrWrite && diffLines.length > 0"
      class="diff-content"
      :style="statusStyle || undefined"
    >
      <div v-for="(line, index) in displayLines" :key="index" :class="['diff-line', line.type]">
        <!-- 行号：对于 removed 行显示旧行号，对于 added 行显示新行号 -->
        <span class="line-num">
          <template v-if="line.type === 'removed'">{{ line.oldLineNum || '-' }}</template>
          <template v-else-if="line.type === 'added'">{{ line.newLineNum || '-' }}</template>
          <template v-else>{{ line.lineNum || '-' }}</template>
        </span>
        <span class="line-marker">{{ line.type === 'removed' ? '-' : line.type === 'added' ? '+' : ' ' }}</span>
        <pre class="line-content">{{ line.content }}</pre>
      </div>
      <div v-if="hasMoreLines" class="more-lines">
        还有 {{ diffLines.length - maxDisplayLines }} 行未显示...
      </div>
    </div>

    <div v-else-if="isReadTool && readLines.length > 0" class="read-result">
      <div v-for="(line, index) in readLines" :key="index" class="read-line">
        <span class="read-line-num">{{ line.lineNumber || ' ' }}</span>
        <pre class="read-line-content">{{ line.content }}</pre>
      </div>
    </div>

    <!-- 非 Edit/Write 工具显示普通结果（包括 Read 以外） -->
    <div v-else-if="result" :class="['plain-result', { error: isError }]">
      <!-- 持久化输出提示 -->
      <div v-if="isPersistedOutput" class="persisted-output">
        <div class="persisted-info">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M13 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z"/>
            <polyline points="13 2 13 9 20 9"/>
          </svg>
          <span>输出已保存到外部文件</span>
        </div>
        <pre v-if="persistedOutputInfo" class="persisted-path">{{ persistedOutputInfo.filePath }}</pre>
        <details class="persisted-preview">
          <summary>预览前 2KB</summary>
          <pre>{{ result }}</pre>
        </details>
      </div>
      <!-- 普通输出 -->
      <pre v-else>{{ result }}</pre>
    </div>

    <!-- 无结果提示 -->
    <div v-else class="no-result">
      {{ isEditOrWrite ? '无变更内容' : '执行中...' }}
    </div>
  </div>
</template>

<style scoped>
.tool-diff-view {
  width: 100%;
}

/* AskUserQuestion 样式 */
.ask-user-question {
  padding: 12px 14px;
}

.question-header {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary, #1f2937);
  margin-bottom: 8px;
}

.question-text {
  font-size: 14px;
  color: var(--text-primary, #1f2937);
  margin-bottom: 12px;
}

.question-options {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-bottom: 12px;
}

.question-option {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding: 14px 14px;
  background: linear-gradient(180deg, rgba(255, 255, 255, 0.96), rgba(248, 250, 252, 0.96));
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 12px;
  box-shadow: 0 1px 2px rgba(15, 23, 42, 0.04);
  transition: background-color 0.15s, border-color 0.15s, box-shadow 0.15s, transform 0.15s;
}

.question-option:hover {
  background: linear-gradient(180deg, rgba(255, 255, 255, 1), rgba(241, 245, 249, 0.98));
  border-color: rgba(59, 130, 246, 0.22);
  box-shadow: 0 6px 14px rgba(15, 23, 42, 0.06);
}

.question-option.selectable {
  cursor: pointer;
}

.question-option.selectable:focus-visible {
  outline: 2px solid var(--primary-color, #3b82f6);
  outline-offset: 2px;
}

.question-option.selected {
  background: linear-gradient(180deg, rgba(239, 246, 255, 0.98), rgba(219, 234, 254, 0.94));
  border-color: rgba(59, 130, 246, 0.5);
  box-shadow: 0 10px 20px rgba(59, 130, 246, 0.12);
}

.option-bullet {
  flex-shrink: 0;
  width: 22px;
  height: 22px;
  display: flex;
  align-items: center;
  justify-content: center;
  margin-top: 1px;
  background: #ffffff;
  color: #ffffff;
  border: 1.5px solid rgba(148, 163, 184, 0.55);
  border-radius: 7px;
  font-size: 11px;
  font-weight: 700;
  box-sizing: border-box;
}

.question-option.selected .option-bullet {
  background: var(--primary-color, #3b82f6);
  border-color: var(--primary-color, #3b82f6);
  color: #ffffff;
}

.option-content {
  flex: 1;
  min-width: 0;
}

.option-label-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 4px;
}

.option-label {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary, #1f2937);
}

.option-selected-tag {
  flex-shrink: 0;
  padding: 2px 8px;
  border-radius: 999px;
  background: rgba(59, 130, 246, 0.12);
  color: #2563eb;
  font-size: 11px;
  font-weight: 600;
}

.option-description {
  font-size: 12.5px;
  line-height: 1.45;
  color: var(--text-secondary, #64748b);
}

.option-preview {
  margin-top: 4px;
  font-size: 12px;
  color: var(--text-muted, #94a3b8);
}

.selection-mode {
  margin-top: 2px;
  margin-bottom: 12px;
  font-size: 12px;
  color: var(--text-secondary, #64748b);
}

.answer-input-panel {
  margin-bottom: 12px;
}

.answer-input-label {
  display: block;
  margin-bottom: 6px;
  font-size: 12px;
  font-weight: 600;
  color: var(--text-secondary, #64748b);
}

.answer-input {
  width: 100%;
  height: 40px;
  padding: 0 12px;
  border: 1px solid var(--border-color, #d1d5db);
  border-radius: 10px;
  background: rgba(255, 255, 255, 0.96);
  color: var(--text-primary, #1f2937);
  font-size: 13px;
  box-sizing: border-box;
  transition: border-color 0.15s, box-shadow 0.15s, background-color 0.15s;
}

.answer-input:focus {
  outline: none;
  border-color: rgba(59, 130, 246, 0.75);
  box-shadow: 0 0 0 3px rgba(59, 130, 246, 0.12);
}

.answer-input::placeholder {
  color: var(--text-muted, #94a3b8);
}

.question-result {
  margin-top: 12px;
  padding-top: 12px;
  border-top: 1px solid var(--border-color, #e5e7eb);
}

.result-header {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-secondary, #6b7280);
  margin-bottom: 6px;
}

.result-content {
  margin: 0;
  padding: 10px 12px;
  background-color: var(--bg-secondary, #f9fafb);
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 6px;
  font-size: 13px;
  font-family: 'Monaco', 'Menlo', monospace;
  color: var(--text-primary, #1f2937);
  white-space: pre-wrap;
  word-break: break-word;
}

/* 深色模式 */
@media (prefers-color-scheme: dark) {
  .question-header,
  .question-text {
    color: var(--text-primary, #f9fafb);
  }

  .question-option {
    background: linear-gradient(180deg, rgba(51, 65, 85, 0.92), rgba(30, 41, 59, 0.94));
    border-color: rgba(100, 116, 139, 0.45);
    box-shadow: none;
  }

  .question-option:hover {
    background: linear-gradient(180deg, rgba(59, 130, 246, 0.12), rgba(30, 41, 59, 0.98));
    border-color: rgba(96, 165, 250, 0.4);
  }

  .question-option.selected {
    background: linear-gradient(180deg, rgba(37, 99, 235, 0.22), rgba(30, 41, 59, 0.98));
    border-color: rgba(96, 165, 250, 0.6);
    box-shadow: 0 10px 22px rgba(2, 6, 23, 0.28);
  }

  .option-bullet {
    background: rgba(15, 23, 42, 0.88);
    border-color: rgba(148, 163, 184, 0.45);
  }

  .question-option.selected .option-bullet {
    background: #60a5fa;
    border-color: #60a5fa;
  }

  .option-label {
    color: var(--text-primary, #f9fafb);
  }

  .option-description {
    color: var(--text-secondary, #9ca3af);
  }

  .option-selected-tag {
    background: rgba(96, 165, 250, 0.16);
    color: #bfdbfe;
  }

  .answer-input-label {
    color: var(--text-secondary, #9ca3af);
  }

  .answer-input {
    background: rgba(15, 23, 42, 0.72);
    border-color: rgba(100, 116, 139, 0.45);
    color: var(--text-primary, #f9fafb);
  }

  .answer-input:focus {
    border-color: rgba(96, 165, 250, 0.8);
    box-shadow: 0 0 0 3px rgba(96, 165, 250, 0.18);
  }

  .question-result {
    border-top-color: var(--border-color, #4b5563);
  }

  .result-content {
    background-color: var(--bg-tertiary, #374151);
    border-color: var(--border-color, #4b5563);
    color: var(--text-primary, #f9fafb);
  }
}

/* 文件路径头部 */
.file-header {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 12px;
  background-color: var(--bg-tertiary, #f3f4f6);
  border-bottom: 1px solid var(--border-color, #e5e7eb);
  font-size: 12px;
  color: var(--text-secondary, #6b7280);
}

.file-header svg {
  flex-shrink: 0;
}

.file-path {
  font-family: 'Monaco', 'Menlo', monospace;
  white-space: nowrap;
  flex: 1;
  min-width: 0;
}

.change-stats {
  margin-left: auto;
  font-size: 12px;
  font-family: 'Monaco', 'Menlo', monospace;
  flex-shrink: 0;
}

.change-stats .removed {
  color: #dc2626;
  margin-right: 4px;
}

.change-stats .added {
  color: #16a34a;
}

/* Diff 内容 */
.diff-content {
  font-family: 'Monaco', 'Menlo', monospace;
  font-size: 12px;
  line-height: 1.5;
  overflow-x: auto;
}

.diff-line {
  display: flex;
  min-height: 20px;
  min-width: max-content;
}

.diff-line.removed {
  background-color: #fee2e2;
}

.diff-line.added {
  background-color: #dcfce7;
}

.diff-line.unchanged {
  background-color: transparent;
}

/* 深色模式下的 diff 颜色 */
@media (prefers-color-scheme: dark) {
  .diff-line.removed {
    background-color: #450a0a;
  }

  .diff-line.added {
    background-color: #052e16;
  }

  .diff-line.unchanged {
    background-color: transparent;
  }

  .file-header {
    background-color: var(--bg-primary, #111827);
    border-bottom-color: var(--border-color, #374151);
  }

  .change-stats .added {
    color: #4ade80;
  }

  .change-stats .removed {
    color: #f87171;
  }
}

.line-num {
  flex-shrink: 0;
  width: 40px;
  text-align: right;
  padding-right: 8px;
  color: var(--text-muted, #9ca3af);
  user-select: none;
  border-right: 1px solid var(--border-color, #e5e7eb);
}

.line-marker {
  flex-shrink: 0;
  width: 20px;
  text-align: center;
  font-weight: bold;
  user-select: none;
}

.diff-line.removed .line-marker {
  color: #dc2626;
}

.diff-line.added .line-marker {
  color: #16a34a;
}

.line-content {
  flex: 1;
  margin: 0;
  padding: 0 8px;
  white-space: pre;
}

.more-lines {
  padding: 8px 12px;
  text-align: center;
  font-size: 12px;
  color: var(--text-muted, #9ca3af);
  background-color: var(--bg-secondary, #f9fafb);
  border-top: 1px solid var(--border-color, #e5e7eb);
}

/* 普通结果 */
.plain-result {
  padding: 12px 14px;
  font-family: 'Monaco', 'Menlo', monospace;
  font-size: 12px;
  white-space: pre-wrap;
  word-break: break-word;
  max-height: 400px;
  overflow-y: auto;
}

.read-result {
  max-height: 400px;
  overflow: auto;
  font-family: 'Monaco', 'Menlo', monospace;
  font-size: 12px;
  line-height: 1.5;
}

.read-line {
  display: flex;
  min-width: max-content;
}

.read-line-num {
  flex-shrink: 0;
  width: 48px;
  padding: 0 8px 0 12px;
  text-align: right;
  color: var(--text-muted, #9ca3af);
  user-select: none;
  border-right: 1px solid var(--border-color, #e5e7eb);
}

.read-line-content {
  flex: 1;
  margin: 0;
  padding: 0 12px;
  white-space: pre;
  color: var(--text-primary, #1f2937);
}

.plain-result.error {
  color: #dc2626;
}

/* 持久化输出 */
.persisted-output {
  padding: 12px 14px;
}

.persisted-info {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 12px;
  background-color: var(--bg-tertiary, #f3f4f6);
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 8px;
  color: var(--text-primary, #1f2937);
  font-size: 13px;
  font-weight: 500;
}

.persisted-info svg {
  flex-shrink: 0;
  color: var(--primary-color, #3b82f6);
}

.persisted-path {
  margin: 10px 0;
  padding: 10px 12px;
  background-color: var(--bg-secondary, #f9fafb);
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 6px;
  font-size: 12px;
  font-family: 'Monaco', 'Menlo', monospace;
  color: var(--text-primary, #1f2937);
  word-break: break-all;
}

.persisted-preview {
  margin-top: 10px;
}

.persisted-preview summary {
  cursor: pointer;
  padding: 8px 12px;
  background-color: var(--bg-tertiary, #f3f4f6);
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 6px;
  font-size: 12px;
  color: var(--text-secondary, #6b7280);
  user-select: none;
}

.persisted-preview summary:hover {
  background-color: var(--bg-secondary, #f9fafb);
}

.persisted-preview pre {
  margin-top: 10px;
  padding: 10px 12px;
  background-color: var(--bg-secondary, #f9fafb);
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 6px;
  font-size: 11px;
  font-family: 'Monaco', 'Menlo', monospace;
  color: var(--text-secondary, #6b7280);
  white-space: pre-wrap;
  word-break: break-word;
  max-height: 200px;
  overflow-y: auto;
}

/* 无结果 */
.no-result {
  padding: 16px;
  text-align: center;
  color: var(--text-muted, #9ca3af);
  font-size: 13px;
}

/* 深色模式 */
@media (prefers-color-scheme: dark) {
  .plain-result {
    color: var(--text-primary, #f3f4f6);
  }

  .read-line-num {
    border-right-color: var(--border-color, #374151);
    color: var(--text-muted, #6b7280);
  }

  .read-line-content {
    color: var(--text-primary, #f3f4f6);
  }

  .plain-result.error {
    color: #fca5a5;
  }

  .more-lines {
    background-color: var(--bg-primary, #111827);
    border-top-color: var(--border-color, #374151);
  }

  .no-result {
    color: var(--text-muted, #6b7280);
  }

  .persisted-info {
    background-color: var(--bg-tertiary, #374151);
    border-color: var(--border-color, #4b5563);
    color: var(--text-primary, #f9fafb);
  }

  .persisted-path {
    background-color: var(--bg-tertiary, #374151);
    border-color: var(--border-color, #4b5563);
    color: var(--text-primary, #f9fafb);
  }

  .persisted-preview summary {
    background-color: var(--bg-tertiary, #374151);
    border-color: var(--border-color, #4b5563);
    color: var(--text-secondary, #9ca3af);
  }

  .persisted-preview summary:hover {
    background-color: var(--bg-secondary, #1f2937);
  }

  .persisted-preview pre {
    background-color: var(--bg-tertiary, #374151);
    border-color: var(--border-color, #4b5563);
    color: var(--text-secondary, #9ca3af);
  }
}
</style>
