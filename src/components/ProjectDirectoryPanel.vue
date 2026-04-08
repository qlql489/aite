<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import ProjectTreeRow, { type ProjectTreeNode } from './ProjectTreeRow.vue';
import Extensions from './Extensions.vue';

interface ProjectTreeResponse {
  root_path: string;
  root_name: string;
  summary: {
    total_files: number;
    total_dirs: number;
    displayed_entries: number;
  };
  tree: ProjectTreeNode[];
  truncated: boolean;
}

interface ProjectFileResponse {
  path: string;
  content: string;
  size: number;
}

const props = defineProps<{
  projectPath: string;
  projectName?: string;
  panelWidth?: number;
}>();

const loading = ref(false);
const error = ref('');
const query = ref('');
const treeData = ref<ProjectTreeResponse | null>(null);
const expandedPaths = ref<Set<string>>(new Set());
const selectedFile = ref('');
const selectedFileSize = ref(0);
const fileContent = ref('');
const editedContent = ref('');
const editorLoading = ref(false);
const editorError = ref('');
const saveState = ref<'idle' | 'dirty' | 'saving' | 'saved' | 'error'>('idle');
const saveMessage = ref('');
const treeWidth = ref(336);
const isResizing = ref(false);
const showWorkspaceSettings = ref(false);

let saveTimer: number | null = null;
let resizeStartX = 0;
let resizeStartWidth = 336;

const clearSaveTimer = () => {
  if (saveTimer !== null) {
    window.clearTimeout(saveTimer);
    saveTimer = null;
  }
};

const matchesNode = (node: ProjectTreeNode): boolean => {
  const normalized = query.value.trim().toLowerCase();
  if (!normalized) return true;
  if (node.name.toLowerCase().includes(normalized)) return true;
  return node.children.some(matchesNode);
};

const loadTree = async () => {
  if (!props.projectPath) return;
  loading.value = true;
  error.value = '';

  try {
    const response = await invoke<ProjectTreeResponse>('read_project_tree', {
      path: props.projectPath,
      depth: 5,
      max_entries: 4000,
    });

    treeData.value = response;
    expandedPaths.value = new Set(
      response.tree.filter((node) => node.is_dir).slice(0, 6).map((node) => node.path)
    );
  } catch (err) {
    treeData.value = null;
    error.value = err instanceof Error ? err.message : '读取项目目录失败';
  } finally {
    loading.value = false;
  }
};

const loadFile = async (filePath: string) => {
  if (!props.projectPath || !filePath) return;
  editorLoading.value = true;
  editorError.value = '';
  saveState.value = 'idle';
  saveMessage.value = '';
  clearSaveTimer();

  try {
    const response = await invoke<ProjectFileResponse>('read_project_file', {
      rootPath: props.projectPath,
      filePath,
    });
    selectedFile.value = response.path;
    selectedFileSize.value = response.size;
    fileContent.value = response.content;
    editedContent.value = response.content;
  } catch (err) {
    selectedFile.value = filePath;
    fileContent.value = '';
    editedContent.value = '';
    editorError.value = err instanceof Error ? err.message : '读取文件失败';
    saveState.value = 'error';
  } finally {
    editorLoading.value = false;
  }
};

const saveFile = async () => {
  if (!selectedFile.value || editedContent.value === fileContent.value) {
    saveState.value = 'idle';
    return;
  }

  saveState.value = 'saving';
  saveMessage.value = '正在保存…';

  try {
    const response = await invoke<ProjectFileResponse>('write_project_file', {
      rootPath: props.projectPath,
      filePath: selectedFile.value,
      content: editedContent.value,
    });
    fileContent.value = response.content;
    selectedFileSize.value = response.size;
    saveState.value = 'saved';
    saveMessage.value = '已自动保存';
    window.setTimeout(() => {
      if (saveState.value === 'saved') {
        saveState.value = 'idle';
        saveMessage.value = '';
      }
    }, 1200);
  } catch (err) {
    saveState.value = 'error';
    saveMessage.value = err instanceof Error ? err.message : '保存失败';
  }
};

watch(
  () => props.projectPath,
  () => {
    query.value = '';
    selectedFile.value = '';
    selectedFileSize.value = 0;
    fileContent.value = '';
    editedContent.value = '';
    editorError.value = '';
    clearSaveTimer();
    void loadTree();
  },
  { immediate: true }
);

watch(editedContent, (value) => {
  if (!selectedFile.value || editorLoading.value) return;
  if (value === fileContent.value) {
    clearSaveTimer();
    saveState.value = 'idle';
    saveMessage.value = '';
    return;
  }

  saveState.value = 'dirty';
  saveMessage.value = '编辑后将自动保存';
  clearSaveTimer();
  saveTimer = window.setTimeout(() => {
    void saveFile();
  }, 650);
});

onMounted(() => {
  if (!treeData.value && props.projectPath) {
    void loadTree();
  }
});

const handleResizeMove = (event: MouseEvent) => {
  if (!isResizing.value) return;
  const nextWidth = resizeStartWidth + (event.clientX - resizeStartX);
  treeWidth.value = Math.max(260, Math.min(460, nextWidth));
};

const stopResize = () => {
  isResizing.value = false;
  document.body.style.cursor = '';
  document.body.style.userSelect = '';
  window.removeEventListener('mousemove', handleResizeMove);
  window.removeEventListener('mouseup', stopResize);
};

const startResize = (event: MouseEvent) => {
  isResizing.value = true;
  resizeStartX = event.clientX;
  resizeStartWidth = treeWidth.value;
  document.body.style.cursor = 'col-resize';
  document.body.style.userSelect = 'none';
  window.addEventListener('mousemove', handleResizeMove);
  window.addEventListener('mouseup', stopResize);
};

onBeforeUnmount(() => {
  clearSaveTimer();
  stopResize();
});

const visibleNodes = computed(() => {
  if (!treeData.value) return [] as ProjectTreeNode[];
  return treeData.value.tree.filter(matchesNode);
});

const normalizedQuery = computed(() => query.value.trim().toLowerCase());
const rootLabel = computed(() => props.projectName || treeData.value?.root_name || '当前项目');
const subtitle = computed(() => treeData.value?.root_path || props.projectPath);
const fileSummary = computed(() => {
  if (!treeData.value) return '';
  return `${treeData.value.summary.total_files} 文件`;
});
const selectedFileName = computed(() => selectedFile.value.split('/').pop() || selectedFile.value);
const hasEditor = computed(() => Boolean(selectedFile.value));
const defaultPanelWidth = computed(() => (hasEditor.value ? 760 : 336));
const resolvedPanelWidth = computed(() => props.panelWidth ?? defaultPanelWidth.value);
const sizeLabel = computed(() => {
  if (!selectedFileSize.value) return '0 B';
  if (selectedFileSize.value < 1024) return `${selectedFileSize.value} B`;
  if (selectedFileSize.value < 1024 * 1024) return `${(selectedFileSize.value / 1024).toFixed(1)} KB`;
  return `${(selectedFileSize.value / (1024 * 1024)).toFixed(1)} MB`;
});

const togglePath = (path: string) => {
  const next = new Set(expandedPaths.value);
  if (next.has(path)) {
    next.delete(path);
  } else {
    next.add(path);
  }
  expandedPaths.value = next;
};

const collapseAll = () => {
  expandedPaths.value = new Set();
};

const copySelectedPath = async () => {
  if (!selectedFile.value) return;
  try {
    await navigator.clipboard.writeText(selectedFile.value);
    saveMessage.value = '相对路径已复制';
    saveState.value = 'saved';
  } catch (err) {
    console.error('复制路径失败:', err);
  }
};

const closeEditor = async () => {
  if (!selectedFile.value) return;

  if (editedContent.value !== fileContent.value && !editorLoading.value && !editorError.value) {
    clearSaveTimer();
    await saveFile();
  }

  selectedFile.value = '';
  selectedFileSize.value = 0;
  fileContent.value = '';
  editedContent.value = '';
  editorError.value = '';
  saveState.value = 'idle';
  saveMessage.value = '';
};
</script>

<template>
  <aside
    class="project-directory-panel"
    :class="{ 'has-editor': hasEditor }"
    :style="{ width: `${resolvedPanelWidth}px`, maxWidth: `${resolvedPanelWidth}px` }"
  >
    <div class="panel-shell">
      <div class="workspace-header">
        <div class="workspace-title-wrap">
          <span class="workspace-title-icon">
            <svg width="18" height="18" viewBox="0 0 16 16" fill="none" aria-hidden="true">
              <path d="M3.5 2.5H12.5V13.5H3.5V2.5Z" stroke="currentColor" stroke-width="1.4" rx="1.6" />
              <path d="M7.2 5.2L5.1 7.3L7.2 9.4" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" />
            </svg>
          </span>
          <span class="workspace-title">项目工作区</span>
        </div>

        <div class="workspace-actions">
          <button
            class="workspace-action"
            type="button"
            @click="showWorkspaceSettings = true"
            title="工作区扩展设置"
          >
            <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
              <path
                d="M8 2.2L8.7 3.7C8.82 3.94 9.05 4.12 9.31 4.18L10.96 4.56C11.46 4.67 11.66 5.28 11.33 5.65L10.22 6.88C10.04 7.08 9.95 7.36 9.98 7.63L10.15 9.31C10.2 9.82 9.68 10.19 9.22 9.97L7.7 9.24C7.45 9.12 7.15 9.12 6.9 9.24L5.38 9.97C4.92 10.19 4.4 9.82 4.45 9.31L4.62 7.63C4.65 7.36 4.56 7.08 4.38 6.88L3.27 5.65C2.94 5.28 3.14 4.67 3.64 4.56L5.29 4.18C5.55 4.12 5.78 3.94 5.9 3.7L6.6 2.2C6.82 1.74 7.48 1.74 7.7 2.2H8Z"
                stroke="currentColor"
                stroke-width="1.2"
                stroke-linejoin="round"
              />
              <circle cx="7.65" cy="6.9" r="1.4" stroke="currentColor" stroke-width="1.2" />
            </svg>
            设置
          </button>
          <button class="workspace-action" type="button" @click="loadTree" :disabled="loading" title="刷新工作区">
            <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
              <path d="M13 8a5 5 0 10-1.2 3.25" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
              <path d="M10.6 11.2H13V8.8" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" />
            </svg>
            {{ loading ? '刷新中' : '刷新' }}
          </button>
        </div>
      </div>

      <div class="workspace-divider"></div>

      <div class="workspace-meta">
        <span class="workspace-boxmark">
          <svg width="28" height="28" viewBox="0 0 24 24" fill="none" aria-hidden="true">
            <path d="M12 3L19 7V17L12 21L5 17V7L12 3Z" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round" />
            <path d="M5 7L12 11L19 7" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round" />
            <path d="M12 11V21" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round" />
          </svg>
        </span>

        <div class="workspace-meta-copy">
          <div class="workspace-name-row">
            <span class="workspace-name">{{ rootLabel }}</span>
            <span v-if="treeData" class="workspace-counts">{{ fileSummary }}</span>
          </div>
          <div class="workspace-path">{{ subtitle }}</div>
        </div>
      </div>

      <div class="panel-body">
        <div class="tree-column" :style="{ width: `${treeWidth}px` }">
          <div class="toolbar-row">
            <label class="search-box">
              <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
                <circle cx="7" cy="7" r="4.75" stroke="currentColor" stroke-width="1.4" />
                <path d="M10.5 10.5L13.5 13.5" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
              </svg>
              <input v-model="query" type="text" placeholder="搜索文件..." />
            </label>
            <button class="toolbar-action" type="button" @click="collapseAll">收起</button>
          </div>

          <div v-if="error" class="feedback-card error">{{ error }}</div>
          <div v-else-if="loading && !treeData" class="feedback-card">正在加载项目目录…</div>
          <div v-else-if="treeData" class="tree-scroll">
            <div v-if="!visibleNodes.length" class="feedback-card empty">没有匹配的目录项</div>
            <div v-else class="tree-list">
              <ProjectTreeRow
                v-for="node in visibleNodes"
                :key="node.path"
                :node="node"
                :depth="0"
                :expanded-paths="expandedPaths"
                :query="normalizedQuery"
                :selected-path="selectedFile"
                @toggle="togglePath"
                @open-file="loadFile"
              />
            </div>
          </div>
        </div>

        <div
          v-if="hasEditor"
          class="column-resizer"
          :class="{ active: isResizing }"
          @mousedown.prevent="startResize"
        ></div>

        <div v-if="hasEditor" class="editor-column">
          <div class="editor-header">
            <div class="editor-copy">
              <div class="editor-name">{{ selectedFileName }}</div>
            </div>
            <div class="editor-actions">
              <span class="editor-size">{{ sizeLabel }}</span>
              <button class="toolbar-action toolbar-icon-button" type="button" @click="copySelectedPath" title="复制路径" aria-label="复制路径">
                <svg width="15" height="15" viewBox="0 0 16 16" fill="none" aria-hidden="true">
                  <rect x="5" y="3" width="8" height="10" rx="1.5" stroke="currentColor" stroke-width="1.3" />
                  <path d="M3.5 10.5H3A1.5 1.5 0 0 1 1.5 9V4A1.5 1.5 0 0 1 3 2.5H8" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" />
                </svg>
              </button>
              <button class="toolbar-action toolbar-icon-button" type="button" @click="loadFile(selectedFile)" title="重载" aria-label="重载">
                <svg width="15" height="15" viewBox="0 0 16 16" fill="none" aria-hidden="true">
                  <path d="M13 8a5 5 0 1 1-1.2-3.25" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
                  <path d="M10.6 11.2H13V8.8" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" />
                </svg>
              </button>
              <button class="toolbar-action toolbar-action-danger toolbar-icon-button" type="button" @click="closeEditor" title="关闭编辑器" aria-label="关闭编辑器">
                <svg width="15" height="15" viewBox="0 0 16 16" fill="none" aria-hidden="true">
                  <path d="M4 4L12 12M12 4L4 12" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
                </svg>
              </button>
            </div>
          </div>

          <div class="editor-status" :data-state="saveState">
            <span v-if="editorLoading">正在读取文件…</span>
            <span v-else-if="editorError">{{ editorError }}</span>
            <span v-else>{{ saveMessage || '支持实时编辑，停顿后自动保存' }}</span>
          </div>

          <textarea
            v-model="editedContent"
            class="editor-textarea"
            spellcheck="false"
            :disabled="editorLoading || !!editorError"
            placeholder="选择一个文本文件开始编辑"
          ></textarea>
        </div>
      </div>

      <div class="panel-footer">
        <span>{{ hasEditor ? '修改会自动写回项目文件' : '点击文件可在右侧打开编辑' }}</span>
        <span v-if="treeData?.truncated" class="footer-warning">已裁剪显示</span>
      </div>
    </div>

    <div v-if="showWorkspaceSettings" class="workspace-settings-overlay" @click.self="showWorkspaceSettings = false">
      <div class="workspace-settings-dialog">
        <div class="workspace-settings-header">
          <div class="workspace-settings-copy">
            <div class="workspace-settings-title">工作区扩展设置</div>
            <div class="workspace-settings-subtitle">{{ subtitle }}</div>
          </div>
          <button class="workspace-settings-close" type="button" @click="showWorkspaceSettings = false" aria-label="关闭">
            ×
          </button>
        </div>

        <div class="workspace-settings-body">
          <Extensions mode="workspace" :workspace-path="projectPath" />
        </div>
      </div>
    </div>
  </aside>
</template>

<style scoped>
.project-directory-panel {
  width: 336px;
  min-width: 300px;
  max-width: 360px;
  height: 100%;
  padding: 0;
  border-left: 1px solid var(--border-color, #e5e7eb);
  background: var(--bg-primary, #fff);
  transition: width 0.18s ease, max-width 0.18s ease;
}

.project-directory-panel.has-editor {
  width: 760px;
  max-width: 760px;
}

.panel-shell {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--bg-primary, #fff);
}

.workspace-settings-overlay {
  position: fixed;
  inset: 0;
  z-index: 40;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 1.5rem;
  background: rgba(15, 23, 42, 0.34);
}

.workspace-settings-dialog {
  display: flex;
  flex-direction: column;
  width: min(1120px, calc(100vw - 3rem));
  height: min(820px, calc(100vh - 3rem));
  overflow: hidden;
  border: 1px solid rgba(148, 163, 184, 0.2);
  border-radius: 20px;
  background: var(--bg-primary, #fff);
  box-shadow: 0 28px 70px rgba(15, 23, 42, 0.18);
}

.workspace-settings-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 1rem;
  padding: 1.15rem 1.25rem;
  border-bottom: 1px solid var(--border-color, #e5e7eb);
  background: linear-gradient(180deg, #ffffff 0%, #f8fafc 100%);
}

.workspace-settings-copy {
  min-width: 0;
}

.workspace-settings-title {
  color: var(--text-primary, #111827);
  font-size: 1rem;
  font-weight: 700;
}

.workspace-settings-subtitle {
  margin-top: 0.3rem;
  color: var(--text-secondary, #6b7280);
  font-size: 0.82rem;
  word-break: break-all;
}

.workspace-settings-close {
  width: 2rem;
  height: 2rem;
  border: none;
  border-radius: 999px;
  background: transparent;
  color: var(--text-secondary, #6b7280);
  font-size: 1.25rem;
  line-height: 1;
  cursor: pointer;
}

.workspace-settings-close:hover {
  background: var(--bg-secondary, #f3f4f6);
  color: var(--text-primary, #111827);
}

.workspace-settings-body {
  flex: 1;
  min-height: 0;
}

.workspace-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
  padding: 1.05rem 1.25rem 0.95rem;
}

.workspace-title-wrap {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  min-width: 0;
}

.workspace-title-icon {
  display: inline-flex;
  color: var(--text-secondary, #6b7280);
}

.workspace-title {
  color: var(--text-primary, #111827);
  font-size: 1rem;
  font-weight: 700;
}

.workspace-action,
.toolbar-action {
  display: inline-flex;
  align-items: center;
  gap: 0.32rem;
  border: none;
  background: transparent;
  color: var(--text-secondary, #6b7280);
  font-size: 0.82rem;
  font-weight: 500;
  cursor: pointer;
  padding: 0.35rem 0.5rem;
  border-radius: 8px;
}

.workspace-actions,
.editor-actions {
  display: inline-flex;
  align-items: center;
  gap: 0.45rem;
}

.workspace-action:hover,
.toolbar-action:hover {
  background: var(--bg-secondary, #f3f4f6);
  color: var(--text-primary, #111827);
}

.toolbar-action-danger {
  color: #ef4444;
}

.toolbar-action-danger:hover {
  background: #fef2f2;
  color: #dc2626;
}

.toolbar-icon-button {
  justify-content: center;
  min-width: 2rem;
  min-height: 2rem;
  padding: 0.35rem;
}

.workspace-divider {
  margin: 0 1.25rem;
  border-bottom: 1px solid var(--border-color, #e5e7eb);
}

.workspace-meta {
  display: flex;
  align-items: flex-start;
  gap: 0.85rem;
  padding: 1rem 1.25rem 0.85rem;
}

.workspace-boxmark {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: #7c6a5b;
  flex-shrink: 0;
}

.workspace-meta-copy {
  min-width: 0;
  flex: 1;
}

.workspace-name-row {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  min-width: 0;
}

.workspace-name {
  max-width: 160px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-primary, #111827);
  font-size: 0.98rem;
  font-weight: 700;
}

.workspace-counts {
  margin-left: auto;
  flex-shrink: 0;
  color: var(--text-secondary, #6b7280);
  font-size: 0.8rem;
}

.workspace-path {
  margin-top: 0.28rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-secondary, #6b7280);
  font-size: 0.78rem;
}

.panel-body {
  display: flex;
  flex: 1;
  min-height: 0;
}

.tree-column {
  min-width: 260px;
  display: flex;
  flex-direction: column;
}

.column-resizer {
  width: 8px;
  cursor: col-resize;
  position: relative;
  flex-shrink: 0;
  background: transparent;
}

.column-resizer::before {
  content: '';
  position: absolute;
  top: 10px;
  bottom: 10px;
  left: 50%;
  width: 1px;
  transform: translateX(-50%);
  background: var(--border-color, #d1d5db);
}

.column-resizer:hover::before,
.column-resizer.active::before {
  width: 2px;
  background: rgba(var(--primary-color-rgb, 59, 130, 246), 0.55);
}

.toolbar-row {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  padding: 0.35rem 1rem 0.35rem;
}

.search-box {
  display: flex;
  align-items: center;
  gap: 0.45rem;
  flex: 1;
  min-width: 0;
  padding: 0 0.8rem;
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 14px;
  background: var(--bg-secondary, #f8fafc);
  color: var(--text-muted, #9ca3af);
}

.search-box input {
  width: 100%;
  border: none;
  outline: none;
  background: transparent;
  padding: 0.68rem 0;
  color: var(--text-primary, #111827);
  font-size: 0.84rem;
}

.tree-scroll,
.editor-column {
  flex: 1;
  min-height: 0;
}

.tree-scroll {
  overflow: auto;
  padding: 0 0.5rem 0.75rem;
}

.tree-list {
  display: flex;
  flex-direction: column;
  gap: 0;
}

.editor-column {
  display: flex;
  flex-direction: column;
  min-width: 0;
  background: var(--bg-secondary, #fbfbfc);
}

.editor-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
  padding: 0.85rem 1rem;
  border-bottom: 1px solid var(--border-color, #e5e7eb);
}

.editor-copy {
  min-width: 0;
}

.editor-name {
  color: var(--text-primary, #111827);
  font-size: 0.92rem;
  font-weight: 700;
}

.editor-actions {
  display: flex;
  align-items: center;
  gap: 0.25rem;
}

.editor-size {
  color: var(--text-muted, #9ca3af);
  font-size: 0.72rem;
  margin-right: 0.25rem;
}

.editor-status {
  padding: 0.5rem 1rem;
  border-bottom: 1px solid var(--border-color, #e5e7eb);
  color: var(--text-secondary, #6b7280);
  font-size: 0.76rem;
}

.editor-status[data-state='dirty'] {
  color: #b45309;
}

.editor-status[data-state='saving'] {
  color: #2563eb;
}

.editor-status[data-state='saved'] {
  color: #047857;
}

.editor-status[data-state='error'] {
  color: #b91c1c;
}

.editor-textarea {
  flex: 1;
  min-height: 0;
  width: 100%;
  border: none;
  outline: none;
  resize: none;
  padding: 1rem;
  background: transparent;
  color: var(--text-primary, #111827);
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, 'Liberation Mono', monospace;
  font-size: 0.84rem;
  line-height: 1.6;
}

.feedback-card {
  margin: 0 1rem 0.9rem;
  padding: 0.75rem 0.9rem;
  border-radius: 10px;
  background: var(--bg-secondary, #f3f4f6);
  color: var(--text-secondary, #6b7280);
  font-size: 0.82rem;
}

.feedback-card.error {
  background: #fef2f2;
  color: #b91c1c;
}

.feedback-card.empty {
  margin-right: 0.5rem;
}

.panel-footer {
  display: flex;
  justify-content: space-between;
  gap: 0.75rem;
  padding: 0.75rem 1rem 0.9rem;
  border-top: 1px solid var(--border-color, #e5e7eb);
  color: var(--text-muted, #9ca3af);
  font-size: 0.74rem;
}

.footer-warning {
  color: #b45309;
}

@media (max-width: 1480px) {
  .project-directory-panel.has-editor {
    width: 680px;
    max-width: 680px;
  }
}

@media (max-width: 1180px) {
  .project-directory-panel {
    display: none;
  }
}

@media (prefers-color-scheme: dark) {
  .project-directory-panel,
  .panel-shell,
  .editor-column {
    background: var(--bg-primary, #111827);
    border-left-color: var(--border-color, #374151);
  }

  .workspace-divider,
  .panel-footer,
  .tree-column,
  .editor-header,
  .editor-status {
    border-color: var(--border-color, #374151);
  }

  .workspace-action:hover,
  .toolbar-action:hover,
  .search-box {
    background: var(--bg-secondary, #1f2937);
  }

  .workspace-boxmark {
    color: #c9b29b;
  }

  .editor-textarea {
    color: var(--text-primary, #f9fafb);
  }
}
</style>
