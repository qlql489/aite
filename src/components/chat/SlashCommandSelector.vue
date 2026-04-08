<script setup lang="ts">
import { ref, computed, watch, nextTick } from 'vue';
import { useSlashesStore, type CommandItem, filterAndSortCommands } from '../../stores/slashes';

interface Props {
  modelValue: boolean;
  query: string;
}

interface Emits {
  (e: 'update:modelValue', value: boolean): void;
  (e: 'select', command: CommandItem): void;
}

const props = defineProps<Props>();

const emit = defineEmits<Emits>();

const slashesStore = useSlashesStore();
const menuRef = ref<HTMLElement>();
const selectedIndex = ref(0);

// 将 argumentHint 转换为字符串显示
function formatArgumentHint(hint?: string | string[]): string | undefined {
  if (!hint) return undefined;
  if (Array.isArray(hint)) {
    return hint.join(' ');
  }
  return hint;
}

function getExecutionLabel(command: CommandItem): string | null {
  if (command.execution === 'ui' && command.immediate) return '即时';
  if (command.execution === 'session') return '会话';
  return null;
}

const groupedCommands = computed(() => {
  const sections: Array<{ key: string; label: string; items: CommandItem[] }> = [];
  const builtins = filteredCommands.value.filter((command) => command.category === 'builtin');
  const commands = filteredCommands.value.filter((command) => command.category !== 'builtin');

  if (builtins.length > 0) {
    sections.push({ key: 'builtin', label: '系统命令', items: builtins });
  }

  if (commands.length > 0) {
    sections.push({ key: 'command', label: '全局命令', items: commands });
  }

  return sections;
});

// 过滤后的命令
const filteredCommands = computed(() => {
  const allCommands = slashesStore.allCommands;
  return filterAndSortCommands(allCommands, props.query);
});

function resetMenuPosition(): void {
  selectedIndex.value = 0;
  if (menuRef.value) {
    menuRef.value.scrollTop = 0;
  }
}

// 监听查询变化，重置选择索引和滚动位置
watch(() => props.query, async () => {
  selectedIndex.value = 0;

  if (!props.modelValue) return;

  await nextTick();
  if (menuRef.value) {
    menuRef.value.scrollTop = 0;
  }
});

// 监听打开状态，每次从顶部开始展示
watch(() => props.modelValue, async (isOpen) => {
  if (isOpen) {
    await nextTick();
    resetMenuPosition();
    return;
  }

  selectedIndex.value = 0;
});

// 监听过滤后的命令数量，确保索引在范围内
watch(filteredCommands, (commands) => {
  if (selectedIndex.value >= commands.length) {
    selectedIndex.value = Math.max(0, commands.length - 1);
  }
});

// 滚动选中项到视图
function scrollSelectedIntoView(): void {
  const selected = menuRef.value?.querySelector(`[data-index="${selectedIndex.value}"]`) as HTMLElement;
  selected?.scrollIntoView({ block: 'nearest' });
}

// 选择命令
function selectCommand(command: CommandItem): void {
  emit('select', command);
  emit('update:modelValue', false);
  selectedIndex.value = 0;
}

// 键盘导航
// 复刻 companion: 支持上下箭头、Tab、Enter、Escape
function handleKeydown(event: KeyboardEvent): void {
  const count = filteredCommands.value.length;

  switch (event.key) {
    case 'ArrowDown':
      event.preventDefault();
      selectedIndex.value = (selectedIndex.value + 1) % count;
      scrollSelectedIntoView();
      break;
    case 'ArrowUp':
      event.preventDefault();
      selectedIndex.value = (selectedIndex.value - 1 + count) % count;
      scrollSelectedIntoView();
      break;
    case 'Enter':
    case 'Tab':
      event.preventDefault();
      if (filteredCommands.value[selectedIndex.value]) {
        selectCommand(filteredCommands.value[selectedIndex.value]);
      }
      break;
    case 'Escape':
      event.preventDefault();
      emit('update:modelValue', false);
      break;
  }
}

// 暴露键盘处理方法
defineExpose({
  handleKeydown,
});
</script>

<template>
  <Transition name="dropdown">
    <div
      v-if="modelValue && filteredCommands.length > 0"
      ref="menuRef"
      class="slash-command-selector"
    >
      <template v-for="(group, groupIndex) in groupedCommands" :key="group.key">
        <div v-if="groupIndex > 0" class="command-group-divider"></div>
        <div class="command-group-title">{{ group.label }}</div>
        <button
          v-for="command in group.items"
          :key="`${group.key}-${command.name}`"
          :data-index="filteredCommands.indexOf(command)"
          :class="[
            'command-item',
            { selected: filteredCommands.indexOf(command) === selectedIndex }
          ]"
          @click="selectCommand(command)"
        >
          <div class="command-content">
            <span class="command-name">/{{ command.name }}</span>
            <div class="command-meta-column">
              <div v-if="formatArgumentHint(command.argumentHint)" class="command-header">
                <span
                  v-if="getExecutionLabel(command)"
                  class="command-execution-badge"
                  :class="command.execution"
                >
                  {{ getExecutionLabel(command) }}
                </span>
                <span v-if="formatArgumentHint(command.argumentHint)" class="command-argument-hint">{{ formatArgumentHint(command.argumentHint) }}</span>
              </div>
              <div v-if="command.description" class="command-description-row">
                <span
                  v-if="!formatArgumentHint(command.argumentHint) && getExecutionLabel(command)"
                  class="command-execution-badge inline"
                  :class="command.execution"
                >
                  {{ getExecutionLabel(command) }}
                </span>
                <span class="command-description" :class="{ compact: !formatArgumentHint(command.argumentHint) }">
                  {{ command.description }}
                </span>
              </div>
            </div>
          </div>
        </button>
      </template>
    </div>
  </Transition>
</template>

<style scoped>
.slash-command-selector {
  position: absolute;
  bottom: 100%;
  left: 0;
  right: 0;
  max-height: 300px;
  overflow-y: auto;
  background-color: var(--bg-primary, #ffffff);
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 0.5rem;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.1);
  margin-bottom: 0.5rem;
  z-index: 100;
  padding: 0.25rem;
}

.command-group-title {
  padding: 0.45rem 0.75rem 0.3rem;
  font-size: 0.72rem;
  font-weight: 600;
  color: var(--text-secondary, #6b7280);
}

.command-group-divider {
  height: 1px;
  margin: 0.3rem 0.5rem;
  background: var(--border-color, #e5e7eb);
}

.command-item {
  display: flex;
  align-items: flex-start;
  gap: 0.5rem;
  width: 100%;
  padding: 0.5rem 0.75rem;
  border: none;
  background: transparent;
  cursor: pointer;
  transition: background-color 0.1s ease;
  text-align: left;
  border-radius: 0.375rem;
}

.command-item:hover {
  background-color: var(--bg-tertiary, #f3f4f6);
}

.command-item + .command-item {
  border-top: 1px solid var(--border-color, #e5e7eb);
}

.command-item.selected {
  background-color: var(--primary-color, #3b82f6);
}

.command-item.selected .command-name,
.command-item.selected .command-argument-hint,
.command-item.selected .command-description {
  color: #ffffff;
}

.command-item.selected .command-name {
  background-color: rgba(255, 255, 255, 0.2);
}

.command-item.selected .command-execution-badge {
  background-color: rgba(255, 255, 255, 0.2);
  color: #ffffff;
}

.command-content {
  display: flex;
  align-items: center;
  gap: 0.95rem;
  width: 100%;
}

.command-meta-column {
  display: flex;
  flex-direction: column;
  justify-content: center;
  gap: 0.18rem;
  min-width: 0;
  flex: 1;
}

.command-execution-badge {
  font-size: 0.65rem;
  font-weight: 600;
  padding: 0.125rem 0.4rem;
  border-radius: 999px;
  letter-spacing: 0.02em;
}

.command-execution-badge.ui {
  background-color: #e5e7eb;
  color: #6b7280;
}

.command-execution-badge.session {
  background-color: #fef3c7;
  color: #b45309;
}

.command-header {
  display: flex;
  align-items: baseline;
  gap: 0.5rem;
  flex-wrap: wrap;
}

.command-name {
  display: inline-flex;
  align-items: center;
  align-self: center;
  flex-shrink: 0;
  font-size: 0.78rem;
  font-weight: 500;
  color: #1e40af;
  font-family: 'Monaco', 'Menlo', 'Courier New', monospace;
  background-color: #dbeafe;
  border-radius: 999px;
  padding: 0.15rem 0.55rem;
}

.command-argument-hint {
  font-size: 0.75rem;
  color: var(--text-secondary, #6b7280);
  font-family: 'Monaco', 'Menlo', 'Courier New', monospace;
}

.command-description-row {
  display: flex;
  align-items: center;
  gap: 0.45rem;
  min-width: 0;
}

.command-description {
  font-size: 0.75rem;
  color: var(--text-secondary, #6b7280);
  line-height: 1.4;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}

.command-description.compact {
  margin-top: 0;
}

@media (max-width: 820px) {
  .command-content {
    align-items: flex-start;
  }
}

/* 滚动条样式 */
.slash-command-selector::-webkit-scrollbar {
  width: 4px;
}

.slash-command-selector::-webkit-scrollbar-track {
  background: transparent;
}

.slash-command-selector::-webkit-scrollbar-thumb {
  background: var(--border-color, #e5e7eb);
  border-radius: 2px;
}

.slash-command-selector::-webkit-scrollbar-thumb:hover {
  background: var(--text-muted, #9ca3af);
}

/* 下拉动画 */
.dropdown-enter-active,
.dropdown-leave-active {
  transition: all 0.12s ease;
}

.dropdown-enter-from,
.dropdown-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}

/* 深色模式 */
@media (prefers-color-scheme: dark) {
  .slash-command-selector {
    background-color: var(--bg-secondary, #1f2937);
    border-color: var(--border-color, #374151);
  }

  .command-item:hover {
    background-color: var(--bg-tertiary, #374151);
  }

  .command-item + .command-item {
    border-top-color: var(--border-color, #374151);
  }

  .command-item.selected {
    background-color: var(--primary-color, #3b82f6);
  }

  .command-group-title {
    color: var(--text-secondary, #9ca3af);
  }

  .command-group-divider {
    background: var(--border-color, #374151);
  }

  .command-name {
    color: #93c5fd;
    background-color: #1e3a5f;
  }

  .command-execution-badge.ui {
    background-color: #374151;
    color: #d1d5db;
  }

  .command-execution-badge.session {
    background-color: #78350f;
    color: #fcd34d;
  }
}
</style>
