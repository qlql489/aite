<script setup lang="ts">
import { computed } from 'vue';

export interface ProjectTreeNode {
  name: string;
  path: string;
  is_dir: boolean;
  children: ProjectTreeNode[];
  has_unloaded_children: boolean;
}

const props = defineProps<{
  node: ProjectTreeNode;
  depth: number;
  expandedPaths: Set<string>;
  loadingPaths: Set<string>;
  query: string;
  selectedPath: string;
}>();

const emit = defineEmits<{
  toggle: [path: string];
  openFile: [path: string];
}>();

const isExpanded = computed(() => props.expandedPaths.has(props.node.path));
const isLoading = computed(() => props.loadingPaths.has(props.node.path));
const hasChildren = computed(
  () => props.node.is_dir && (props.node.children.length > 0 || props.node.has_unloaded_children)
);

const childVisible = (child: ProjectTreeNode): boolean => {
  if (!props.query) return true;
  const normalized = props.query;
  if (child.name.toLowerCase().includes(normalized)) return true;
  return child.children.some(childVisible);
};

const shouldShowChildren = computed(() => {
  if (!props.node.is_dir) return false;
  if (props.query) return props.node.children.some(childVisible);
  return isExpanded.value;
});

const onRowClick = () => {
  if (props.node.is_dir) {
    emit('toggle', props.node.path);
  } else {
    emit('openFile', props.node.path);
  }
};
</script>

<template>
  <div class="tree-node">
    <button
      type="button"
      class="tree-row"
      :class="{ 'is-selected': selectedPath === node.path }"
      :style="{ '--depth': depth }"
      @click="onRowClick"
    >
      <span class="tree-caret" :class="{ open: isExpanded, hidden: !hasChildren, loading: isLoading }">
        <span v-if="isLoading" class="tree-spinner" aria-hidden="true"></span>
        <template v-else>›</template>
      </span>
      <span class="tree-icon" :class="node.is_dir ? 'dir' : 'file'">
        <svg v-if="node.is_dir" width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
          <path d="M2.5 4.5H6L7.4 6H13.5V12.5H2.5V4.5Z" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" />
        </svg>
        <svg v-else width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
          <path d="M4 2.5H9.5L12 5V13.5H4V2.5Z" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" />
          <path d="M9.5 2.5V5H12" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" />
        </svg>
      </span>
      <span class="tree-label">{{ node.name }}</span>
    </button>

    <div v-if="shouldShowChildren && node.children.length" class="tree-children">
      <ProjectTreeRow
        v-for="child in node.children.filter(childVisible)"
        :key="child.path"
        :node="child"
        :depth="depth + 1"
        :expanded-paths="expandedPaths"
        :loading-paths="loadingPaths"
        :query="query"
        :selected-path="selectedPath"
        @toggle="$emit('toggle', $event)"
        @open-file="$emit('openFile', $event)"
      />
    </div>
  </div>
</template>

<style scoped>
.tree-node {
  display: flex;
  flex-direction: column;
}

.tree-row {
  display: flex;
  align-items: center;
  gap: 0.34rem;
  width: 100%;
  min-height: 28px;
  padding: 0.14rem 0.45rem;
  padding-left: calc(0.45rem + (var(--depth) * 0.9rem));
  border: none;
  border-radius: 8px;
  background: transparent;
  color: var(--text-secondary, #6b7280);
  cursor: pointer;
  text-align: left;
  transition: background-color 0.15s ease, color 0.15s ease;
}

.tree-row:hover,
.tree-row.is-selected {
  background: var(--bg-secondary, #f3f4f6);
  color: var(--text-primary, #111827);
}

.tree-row.is-selected {
  box-shadow: inset 0 0 0 1px rgba(var(--primary-color-rgb, 59, 130, 246), 0.18);
}

.tree-caret {
  width: 10px;
  flex-shrink: 0;
  color: var(--text-muted, #9ca3af);
  font-size: 0.86rem;
  transition: transform 0.15s ease;
}

.tree-caret.open {
  transform: rotate(90deg);
}

.tree-caret.loading {
  transform: none;
}

.tree-caret.hidden {
  opacity: 0;
}

.tree-spinner {
  display: inline-flex;
  width: 9px;
  height: 9px;
  border: 1.4px solid currentColor;
  border-right-color: transparent;
  border-radius: 999px;
  animation: tree-spin 0.7s linear infinite;
}

.tree-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}

.tree-icon.dir {
  color: #d97745;
}

.tree-icon.file {
  color: #c96a2d;
}

.tree-label {
  min-width: 0;
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 0.86rem;
}

.tree-children {
  display: flex;
  flex-direction: column;
}

@keyframes tree-spin {
  to {
    transform: rotate(360deg);
  }
}

@media (prefers-color-scheme: dark) {
  .tree-row:hover,
  .tree-row.is-selected {
    background: rgba(255, 255, 255, 0.06);
  }

  .tree-icon.dir,
  .tree-icon.file {
    color: #e5a67d;
  }
}
</style>
