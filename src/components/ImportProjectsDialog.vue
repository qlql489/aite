<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { HugeiconsIcon } from '@hugeicons/vue';
import {
  Search01Icon,
  FolderOpenIcon,
  ClockIcon,
  FileImportIcon,
  MessageAddIcon,
  Loading02Icon,
} from '@hugeicons/core-free-icons';

interface CliSessionInfo {
  session_id: string;
  title: string;
  created_at: string;
  updated_at: string;
  message_count: number;
  file_size: number;
}

interface ProjectWithSessions {
  project_name: string;
  cwd: string;
  project_path: string;
  sessions: CliSessionInfo[];
  session_count: number;
  last_activity: string;
  total_file_size: number;
  cli_version: string;
  preview: string;
}

const props = defineProps<{
  open: boolean;
}>();

const emit = defineEmits<{
  close: [];
  imported: [project: ProjectWithSessions];
}>();

const projects = ref<ProjectWithSessions[]>([]);
const loading = ref(false);
const importing = ref<string | null>(null);
const error = ref<string | null>(null);
const searchQuery = ref('');
const cliConfigured = ref(false);

// 过滤后的项目列表
const filteredProjects = computed(() => {
  if (!searchQuery.value) return projects.value;
  const query = searchQuery.value.toLowerCase();
  return projects.value.filter(p =>
    p.project_name.toLowerCase().includes(query) ||
    p.cwd.toLowerCase().includes(query)
  );
});

// 格式化文件大小
function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

// 格式化相对时间（类似 CodePilot 的 formatRelativeTime）
function formatTimestamp(timestamp: string): string {
  const secs = parseInt(timestamp);
  if (secs === 0) return 'Unknown';
  const date = new Date(secs * 1000);
  const now = new Date();
  const diff = now.getTime() - date.getTime();
  const seconds = Math.floor(diff / 1000);
  const minutes = Math.floor(seconds / 60);
  const hours = Math.floor(minutes / 60);
  const days = Math.floor(hours / 24);

  if (seconds < 60) return 'now';
  if (minutes < 60) return `${minutes}m`;
  if (hours < 24) return `${hours}h`;
  if (days < 7) return `${days}d`;
  return date.toLocaleDateString();
}

// 加载项目列表
async function loadProjects() {
  loading.value = true;
  error.value = null;

  try {
    projects.value = await invoke<ProjectWithSessions[]>('get_projects_with_sessions');
  } catch (e) {
    error.value = e as string;
    console.error('Failed to load projects:', e);
  } finally {
    loading.value = false;
  }
}

// 导入项目
async function importProject(project: ProjectWithSessions) {
  importing.value = project.project_path;

  try {
    await invoke('import_project', { projectPath: project.project_path });
    emit('imported', project);
    emit('close');
  } catch (e) {
    error.value = e as string;
    console.error('Failed to import project:', e);
  } finally {
    importing.value = null;
  }
}

// 检查 CLI 是否配置
async function checkCliConfigured() {
  try {
    cliConfigured.value = await invoke('check_cli_configured');
  } catch (e) {
    console.error('Failed to check CLI status:', e);
  }
}

onMounted(() => {
  checkCliConfigured();
  if (props.open) {
    loadProjects();
  }
});

watch(() => props.open, (isOpen) => {
  if (isOpen) {
    loadProjects();
  }
});
</script>

<template>
  <div v-if="open" class="dialog-overlay" @click.self="emit('close')">
    <div class="dialog">
      <div class="dialog-header">
        <h2>导入项目</h2>
        <button class="btn-close" @click="emit('close')">&times;</button>
      </div>

      <div class="dialog-body">
        <!-- CLI 未配置提示 -->
        <div v-if="!cliConfigured" class="empty-state">
          <HugeiconsIcon :icon="FolderOpenIcon" class="empty-icon" />
          <h3>Claude CLI 未配置</h3>
          <p>请先安装并配置 Claude Code CLI，然后使用 CLI 进行对话后，再尝试导入。</p>
          <a href="https://docs.anthropic.com/en/docs/build-with-claude/claude-for-developers" target="_blank">
            了解如何安装 Claude Code CLI
          </a>
        </div>

        <!-- 加载状态 -->
        <div v-else-if="loading" class="loading-state">
          <HugeiconsIcon :icon="Loading02Icon" class="loading-icon spinner-icon" />
          <p>扫描项目...</p>
        </div>

        <!-- 错误状态 -->
        <div v-else-if="error" class="error-state">
          <HugeiconsIcon :icon="FolderOpenIcon" class="error-icon" />
          <h3>加载失败</h3>
          <p>{{ error }}</p>
          <button class="btn-primary" @click="loadProjects">重试</button>
        </div>

        <!-- 空状态 -->
        <div v-else-if="projects.length === 0" class="empty-state">
          <HugeiconsIcon :icon="FolderOpenIcon" class="empty-icon" />
          <h3>没有找到项目</h3>
          <p>请先使用 Claude CLI 在项目中进行对话，然后可以在这里导入。</p>
        </div>

        <!-- 项目列表 -->
        <div v-else class="projects-content">
          <!-- 搜索框 -->
          <div class="search-box">
            <HugeiconsIcon :icon="Search01Icon" class="search-icon" />
            <input
              v-model="searchQuery"
              type="text"
              placeholder="搜索项目名称或路径..."
              class="search-input"
            />
          </div>

          <!-- 项目卡片列表 -->
          <div class="projects-list">
            <div
              v-for="project in filteredProjects"
              :key="project.cwd"
              class="project-card"
            >
              <div class="project-content">
                <!-- 第一行：项目名称 + 导入按钮 -->
                <div class="project-header">
                  <div class="project-title">{{ project.project_name }}</div>
                  <button
                    class="btn-import"
                    @click="importProject(project)"
                    :disabled="importing === project.project_path"
                  >
                    <HugeiconsIcon
                      v-if="importing === project.project_path"
                      :icon="Loading02Icon"
                      class="btn-icon spinner-icon"
                    />
                    <template v-else>
                      <HugeiconsIcon :icon="FileImportIcon" class="btn-icon" />
                      导入
                    </template>
                  </button>
                </div>

                <!-- 第二行：文件路径 -->
                <div class="project-path-row" :title="project.cwd">
                  <HugeiconsIcon :icon="FolderOpenIcon" class="path-icon" />
                  <span class="path-text">{{ project.cwd }}</span>
                </div>

                <!-- 第三行：元数据（左对齐） -->
                <div class="project-meta">
                  <span class="meta-item">
                    <HugeiconsIcon :icon="MessageAddIcon" class="meta-icon" />
                    <span>{{ project.session_count }}</span>
                  </span>
                  <span class="meta-item">
                    <HugeiconsIcon :icon="ClockIcon" class="meta-icon" />
                    <span>{{ formatTimestamp(project.last_activity) }}</span>
                  </span>
                  <span class="meta-item">
                    {{ formatFileSize(project.total_file_size) }}
                  </span>
                  <span v-if="project.cli_version && project.cli_version !== 'Unknown'" class="meta-item">
                    v{{ project.cli_version }}
                  </span>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.dialog-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background-color: rgba(0, 0, 0, 0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
}

.dialog {
  background-color: var(--bg-primary, #ffffff);
  border-radius: 12px;
  box-shadow: 0 20px 25px -5px rgba(0, 0, 0, 0.1);
  width: 90%;
  max-width: 700px;
  max-height: 80vh;
  display: flex;
  flex-direction: column;
}

.dialog-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 1.5rem;
  border-bottom: 1px solid var(--border-color, #e5e7eb);
}

.dialog-header h2 {
  margin: 0;
  font-size: 1.25rem;
  font-weight: 600;
  color: var(--text-primary, #1f2937);
}

.btn-close {
  width: 32px;
  height: 32px;
  padding: 0;
  background: transparent;
  border: none;
  border-radius: 4px;
  font-size: 1.5rem;
  cursor: pointer;
  color: var(--text-secondary, #6b7280);
  transition: all 0.2s;
}

.btn-close:hover {
  background-color: var(--bg-secondary, #f9fafb);
}

.dialog-body {
  padding: 1.5rem;
  overflow-y: auto;
  flex: 1;
}

/* 空状态 */
.empty-state,
.error-state,
.loading-state {
  text-align: center;
  padding: 3rem 1rem;
}

.empty-icon,
.error-icon,
.loading-icon {
  width: 2rem;
  height: 2rem;
  margin: 0 auto 1rem;
  opacity: 0.4;
}

.empty-state h3,
.error-state h3 {
  margin: 0 0 0.5rem 0;
  font-size: 1.125rem;
  font-weight: 600;
  color: var(--text-primary, #1f2937);
}

.empty-state p,
.error-state p {
  margin: 0 0 1rem 0;
  color: var(--text-secondary, #6b7280);
}

.empty-state a {
  color: var(--primary-color, #3b82f6);
  text-decoration: none;
}

.btn-primary {
  padding: 0.5rem 1rem;
  background-color: var(--primary-color, #3b82f6);
  color: white;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  transition: background-color 0.2s;
}

.btn-primary:hover {
  background-color: var(--primary-hover, #2563eb);
}

/* 加载动画 */
.spinner-icon {
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

/* 项目内容 */
.projects-content {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

/* 搜索框 */
.search-box {
  position: relative;
  display: flex;
  align-items: center;
}

.search-icon {
  position: absolute;
  left: 0.625rem;
  top: 50%;
  transform: translateY(-50%);
  width: 0.875rem;
  height: 0.875rem;
  color: var(--text-muted, #9ca3af);
}

.search-input {
  width: 100%;
  padding: 0.75rem 1rem 0.75rem 2.25rem;
  background-color: var(--bg-secondary, #f9fafb);
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 8px;
  font-size: 0.875rem;
  color: var(--text-primary, #1f2937);
  outline: none;
  transition: all 0.2s;
}

.search-input:focus {
  border-color: var(--primary-color, #3b82f6);
  background-color: var(--bg-primary, #ffffff);
}

.search-input::placeholder {
  color: var(--text-muted, #9ca3af);
}

/* 项目列表 */
.projects-list {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.project-card {
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 8px;
  padding: 0.75rem;
  background-color: var(--bg-primary, #ffffff);
  transition: all 0.2s;
}

.project-card:hover {
  background-color: var(--accent-hover, rgba(0, 0, 0, 0.03));
}

.project-content {
  display: flex;
  flex-direction: column;
  gap: 0.375rem;
}

/* 第一行：项目名称 + 导入按钮 */
.project-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 0.5rem;
}

.project-title {
  flex: 1;
  min-width: 0;
  font-size: 0.875rem;
  font-weight: 500;
  color: var(--text-primary, #1f2937);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* 第二行：文件路径 */
.project-path-row {
  display: flex;
  align-items: center;
  gap: 0.375rem;
  overflow: hidden;
}

.path-icon {
  width: 12px;
  height: 12px;
  flex-shrink: 0;
  color: var(--text-muted, rgba(107, 114, 128, 0.8));
}

.path-text {
  font-size: 0.75rem;
  color: var(--text-muted, rgba(107, 114, 128, 0.8));
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: 'Monaco', 'Menlo', 'Consolas', monospace;
}

/* 第三行：预览文本 */
.preview-text {
  font-size: 0.75rem;
  color: var(--text-secondary, #6b7280);
  overflow: hidden;
  text-overflow: ellipsis;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  line-clamp: 2;
  word-break: break-word;
}

/* 第四行：元数据 */
.project-meta {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  font-size: 10px;
  color: var(--text-muted, rgba(107, 114, 128, 0.6));
}

.meta-item {
  display: flex;
  align-items: center;
  gap: 0.125rem;
  white-space: nowrap;
}

.meta-icon {
  width: 10px;
  height: 10px;
  line-height: 1;
  flex-shrink: 0;
}

/* 导入按钮 */
.btn-import {
  padding: 0.25rem 0.5rem;
  background-color: transparent;
  color: var(--text-primary, #1f2937);
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 6px;
  cursor: pointer;
  font-size: 0.75rem;
  font-weight: 500;
  transition: all 0.2s;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 0.25rem;
  white-space: nowrap;
  flex-shrink: 0;
  height: 28px;
}

.btn-icon {
  width: 12px;
  height: 12px;
}

.btn-import:hover:not(:disabled) {
  background-color: var(--accent-bg, rgba(0, 0, 0, 0.05));
}

.btn-import:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

/* 深色模式 */
@media (prefers-color-scheme: dark) {
  .dialog {
    background-color: var(--bg-primary, #1f2937);
  }

  .dialog-header {
    border-bottom-color: var(--border-color, #374151);
  }

  .dialog-header h2 {
    color: var(--text-primary, #f9fafb);
  }

  .btn-close:hover {
    background-color: var(--bg-tertiary, #374151);
  }

  .empty-state h3,
  .error-state h3 {
    color: var(--text-primary, #f9fafb);
  }

  .project-card {
    background-color: var(--bg-primary, #1f2937);
    border-color: var(--border-color, #374151);
  }

  .project-title {
    color: var(--text-primary, #f9fafb);
  }

  .project-card:hover {
    background-color: var(--accent-hover, rgba(255, 255, 255, 0.05));
  }

  .btn-import {
    color: var(--text-primary, #f9fafb);
    border-color: var(--border-color, #374151);
  }

  .btn-import:hover:not(:disabled) {
    background-color: var(--accent-bg, rgba(255, 255, 255, 0.1));
  }

  .btn-loading {
    border-color: var(--border-color, #374151);
    border-top-color: var(--text-primary, #f9fafb);
  }

  .project-meta {
    color: var(--text-muted, rgba(156, 163, 175, 0.6));
  }
}
</style>
