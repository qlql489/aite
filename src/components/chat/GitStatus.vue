<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { HugeiconsIcon } from '@hugeicons/vue';
import { GitPullRequestIcon } from '@hugeicons/core-free-icons';
import { invoke } from '@tauri-apps/api/core';
import type { GitInfo } from '../../types';

interface Props {
  gitInfo: GitInfo | null;
  projectPath?: string;
}

const props = defineProps<Props>();

const emit = defineEmits<{
  updated: [gitInfo: GitInfo | null];
}>();

const branchMenuOpen = ref(false);
const createDialogOpen = ref(false);
const branches = ref<string[]>([]);
const branchQuery = ref('');
const loadingBranches = ref(false);
const busy = ref(false);
const createBranchName = ref('');
const createBranchError = ref('');
const containerRef = ref<HTMLElement | null>(null);
const branchInputRef = ref<HTMLInputElement | null>(null);


const hasGitInfo = computed(() => Boolean(props.gitInfo?.branch));
const branchName = computed(() => props.gitInfo?.branch || '未知');
const filteredBranches = computed(() => {
  const keyword = branchQuery.value.trim().toLowerCase();
  if (!keyword) return branches.value;
  return branches.value.filter((branch) => branch.toLowerCase().includes(keyword));
});

const validateBranchName = (name: string) => {
  const trimmed = name.trim();
  if (!trimmed) return '请输入分支名称。';
  if (trimmed.endsWith('/')) return '分支名不能以“/”结尾。';
  return '';
};


const loadBranches = async () => {
  if (!props.projectPath || !hasGitInfo.value) return;
  loadingBranches.value = true;
  try {
    branches.value = await invoke<string[]>('git_list_branches', { projectPath: props.projectPath });
  } catch (error) {
    console.error('加载分支列表失败:', error);
    branches.value = props.gitInfo?.branch ? [props.gitInfo.branch] : [];
  } finally {
    loadingBranches.value = false;
  }
};

const toggleBranchMenu = async () => {
  if (!hasGitInfo.value) return;
  branchMenuOpen.value = !branchMenuOpen.value;
  if (branchMenuOpen.value) {
    branchQuery.value = '';
    await loadBranches();
  }
};

const selectBranch = async (branch: string) => {
  if (!props.projectPath || branch === props.gitInfo?.branch) {
    branchMenuOpen.value = false;
    return;
  }

  busy.value = true;
  try {
    const gitInfo = await invoke<GitInfo>('git_checkout_branch', {
      projectPath: props.projectPath,
      branch,
      createBranch: false,
    });
    emit('updated', gitInfo);
    branchMenuOpen.value = false;
  } catch (error) {
    console.error('切换分支失败:', error);
    window.alert(error instanceof Error ? error.message : '切换分支失败');
  } finally {
    busy.value = false;
  }
};

const createRepository = async () => {
  if (!props.projectPath) return;
  busy.value = true;
  try {
    const gitInfo = await invoke<GitInfo>('git_init_repository', { projectPath: props.projectPath });
    emit('updated', gitInfo?.branch ? gitInfo : null);
  } catch (error) {
    console.error('创建 Git 仓库失败:', error);
    window.alert(error instanceof Error ? error.message : '创建 Git 仓库失败');
  } finally {
    busy.value = false;
  }
};

const openCreateDialog = async () => {
  branchMenuOpen.value = false;
  createBranchName.value = '';
  createBranchError.value = validateBranchName(createBranchName.value);
  createDialogOpen.value = true;
  await nextTick();
  branchInputRef.value?.focus();
  branchInputRef.value?.setSelectionRange(createBranchName.value.length, createBranchName.value.length);
};

const closeCreateDialog = () => {
  createDialogOpen.value = false;
  createBranchError.value = '';
};

const submitCreateBranch = async () => {
  if (!props.projectPath) return;
  const error = validateBranchName(createBranchName.value);
  createBranchError.value = error;
  if (error) return;

  busy.value = true;
  try {
    const gitInfo = await invoke<GitInfo>('git_checkout_branch', {
      projectPath: props.projectPath,
      branch: createBranchName.value.trim(),
      createBranch: true,
    });
    emit('updated', gitInfo);
    closeCreateDialog();
  } catch (error) {
    console.error('创建分支失败:', error);
    createBranchError.value = error instanceof Error ? error.message : '创建分支失败';
  } finally {
    busy.value = false;
  }
};

const onDocumentClick = (event: MouseEvent) => {
  const target = event.target as Node | null;
  if (!target || !containerRef.value?.contains(target)) {
    branchMenuOpen.value = false;
  }
};

watch(createBranchName, (value) => {
  createBranchError.value = validateBranchName(value);
});

watch(
  () => props.projectPath,
  () => {
    branchMenuOpen.value = false;
    createDialogOpen.value = false;
    branchQuery.value = '';
    branches.value = [];
  }
);

onMounted(() => {
  document.addEventListener('mousedown', onDocumentClick);
});

onBeforeUnmount(() => {
  document.removeEventListener('mousedown', onDocumentClick);
});
</script>

<template>
  <div ref="containerRef" class="git-status-shell">
    <button
      v-if="!hasGitInfo"
      class="git-create-btn"
      type="button"
      :disabled="busy || !projectPath"
      @click="createRepository"
    >
      <HugeiconsIcon :icon="GitPullRequestIcon" class="git-branch-icon" />
      <span>{{ busy ? '创建中…' : '创建 Git 仓库' }}</span>
    </button>

    <template v-else>
      <div class="git-status-bar">
        <button class="branch-trigger" type="button" :disabled="busy" @click="toggleBranchMenu">
          <span class="branch-trigger-main">
            <HugeiconsIcon :icon="GitPullRequestIcon" class="git-branch-icon" />
            <span>{{ branchName }}</span>
          </span>
          <svg class="chevron" width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
            <path d="M3.5 5.25L7 8.75L10.5 5.25" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" />
          </svg>
        </button>
      </div>

      <div v-if="branchMenuOpen" class="branch-menu">
        <label class="branch-search">
          <svg width="18" height="18" viewBox="0 0 18 18" fill="none" aria-hidden="true">
            <circle cx="8" cy="8" r="5.5" stroke="currentColor" stroke-width="1.8" />
            <path d="M12 12L15.5 15.5" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" />
          </svg>
          <input v-model="branchQuery" type="text" placeholder="搜索分支" />
        </label>

        <div class="branch-section-title">分支</div>

        <div class="branch-list">
          <div v-if="loadingBranches" class="branch-empty">加载中…</div>
          <button
            v-for="branch in filteredBranches"
            :key="branch"
            class="branch-option"
            :class="{ active: branch === gitInfo?.branch }"
            type="button"
            @click="selectBranch(branch)"
          >
            <span class="branch-option-main">
              <HugeiconsIcon :icon="GitPullRequestIcon" class="git-branch-icon" />
              <span>{{ branch }}</span>
            </span>
            <span v-if="branch === gitInfo?.branch" class="branch-check">✓</span>
          </button>
          <div v-if="!loadingBranches && filteredBranches.length === 0" class="branch-empty">没有匹配的分支</div>
        </div>

        <button class="branch-create-btn" type="button" :disabled="busy" @click="openCreateDialog">
          <span class="branch-create-plus">＋</span>
          <span>创建并检出新分支...</span>
        </button>
      </div>
    </template>

    <div v-if="createDialogOpen" class="dialog-overlay" @click.self="closeCreateDialog">
      <div class="create-dialog">
        <div class="dialog-header">
          <h3>创建并检出分支</h3>
          <button class="dialog-close" type="button" @click="closeCreateDialog">×</button>
        </div>

        <div class="dialog-body">
          <div class="dialog-row">
            <label class="dialog-label">分支名称</label>
          </div>

          <input
            ref="branchInputRef"
            v-model="createBranchName"
            class="dialog-input"
            type="text"
            placeholder="输入分支名称"
            @keydown.enter.prevent="submitCreateBranch"
          />

          <div class="dialog-error">{{ createBranchError || ' ' }}</div>
        </div>

        <div class="dialog-actions">
          <button class="dialog-btn secondary" type="button" @click="closeCreateDialog">关闭</button>
          <button class="dialog-btn primary" type="button" :disabled="busy || !!createBranchError" @click="submitCreateBranch">
            {{ busy ? '创建中...' : '创建并检出' }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.git-status-shell {
  position: relative;
  display: inline-flex;
  flex-direction: column;
  align-items: flex-start;
}

.git-create-btn,
.branch-trigger {
  border: none;
  background: #f3f3f3;
  color: #6d6d6d;
  border-radius: 999px;
  display: inline-flex;
  align-items: center;
  cursor: pointer;
  transition: background-color 0.18s ease, color 0.18s ease;
}

.git-create-btn,
.branch-trigger {
  padding: 0.34rem 0.68rem;
  font-size: 11px;
  font-weight: 500;
}

.git-create-btn {
  gap: 0.45rem;
}

.git-status-bar {
  display: flex;
  align-items: center;
  gap: 0;
}

.branch-trigger {
  min-width: 92px;
  justify-content: space-between;
  gap: 0.5rem;
}

.branch-trigger-main {
  display: inline-flex;
  align-items: center;
  gap: 0.45rem;
}

.git-branch-icon {
  width: 14px;
  height: 14px;
  flex-shrink: 0;
}


.git-create-btn:hover,
.branch-trigger:hover {
  background: #ebebeb;
  color: #202020;
}

.branch-menu {
  position: absolute;
  left: 0;
  bottom: calc(100% + 0.55rem);
  z-index: 30;
  width: 300px;
  max-width: min(560px, calc(100vw - 32px));
  background: #ffffff;
  border-radius: 12px;
  box-shadow: 0 20px 40px rgba(15, 23, 42, 0.16);
  padding: 8px 8px 6px;
}

.branch-search {
  display: flex;
  align-items: center;
  gap: 6px;
  background: #f5f5f5;
  border-radius: 12px;
  padding: 8px 10px;
  color: #777;
}

.branch-search input {
  width: 100%;
  border: none;
  background: transparent;
  outline: none;
  font-size: 11px;
  color: #1a1a1a;
}

.branch-search input::placeholder {
  color: #8b8b8b;
}

.branch-section-title {
  padding: 10px 6px 6px;
  font-size: 14px;
  font-weight: 500;
  color: #747474;
}

.branch-list {
  max-height: 220px;
  overflow-y: auto;
  padding-right: 4px;
}

.branch-option {
  width: 100%;
  display: flex;
  align-items: center;
  justify-content: space-between;
  border: none;
  background: transparent;
  padding: 5px 4px;
  cursor: pointer;
  color: #111;
  font-size: 14px;
  line-height: 1.3;
  text-align: left;
}

.branch-option-main {
  display: inline-flex;
  align-items: center;
  gap: 10px;
}

.branch-option.active {
  color: #111;
}

.branch-option:disabled {
  color: #9b9b9b;
}

.branch-check {
  font-size: 18px;
  line-height: 1;
}

.branch-empty {
  padding: 8px 6px 10px;
  color: #9b9b9b;
  font-size: 11px;
}

.branch-create-btn {
  margin-top: 6px;
  width: 100%;
  border: none;
  border-top: 1px solid #e7e7e7;
  background: transparent;
  padding: 10px 4px 6px;
  display: flex;
  align-items: center;
  gap: 10px;
  color: #111;
  font-size: 14px;
  cursor: pointer;
  text-align: left;
}

.branch-create-plus {
  font-size: 20px;
  line-height: 0.8;
  font-weight: 300;
}

.dialog-overlay {
  position: fixed;
  inset: 0;
  z-index: 80;
  background: rgba(15, 23, 42, 0.18);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 20px;
}

.create-dialog {
  width: min(460px, calc(100vw - 32px));
  background: #ffffff;
  border-radius: 16px;
  box-shadow: 0 28px 56px rgba(15, 23, 42, 0.18);
  padding: 18px 18px 16px;
}

.dialog-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 10px;
}

.dialog-header h3 {
  margin: 0;
  font-size: 20px;
  line-height: 1.15;
  color: #111;
}

.dialog-close {
  border: none;
  background: transparent;
  color: #666;
  font-size: 24px;
  line-height: 1;
  cursor: pointer;
}

.dialog-body {
  margin-top: 14px;
}

.dialog-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}

.dialog-label {
  font-size: 14px;
  font-weight: 600;
  color: #111;
}

.prefix-button {
  border: none;
  background: transparent;
  color: #8b8b8b;
  font-size: 14px;
  cursor: pointer;
}

.dialog-input {
  width: 100%;
  margin-top: 6px;
  border: 1px solid #ebebeb;
  border-radius: 12px;
  padding: 12px 14px;
  font-size: 14px;
  color: #111;
  outline: none;
}

.dialog-input:focus {
  border-color: #d8d8d8;
}

.dialog-error {
  min-height: 20px;
  margin-top: 6px;
  font-size: 11px;
  color: #222;
}

.dialog-actions {
  margin-top: 6px;
  display: flex;
  justify-content: flex-end;
  gap: 10px;
}

.dialog-btn {
  border: none;
  border-radius: 12px;
  padding: 8px 14px;
  font-size: 14px;
  font-weight: 600;
  cursor: pointer;
}

.dialog-btn.secondary {
  background: #f0f0f0;
  color: #111;
}

.dialog-btn.primary {
  background: #a8a8a8;
  color: #fff;
}

.dialog-btn.primary:disabled {
  cursor: default;
  opacity: 0.75;
}

button:disabled {
  opacity: 0.72;
  cursor: default;
}

@media (prefers-color-scheme: dark) {
  .git-create-btn,
  .branch-trigger,
  .git-refresh-btn {
    background: #2b313b;
    color: #d1d5db;
  }

  .git-create-btn:hover,
  .branch-trigger:hover,
  .git-refresh-btn:hover {
    background: #374151;
    color: #f9fafb;
  }

  .branch-menu,
  .create-dialog {
    background: #111827;
    color: #f9fafb;
  }

  .branch-search {
    background: #1f2937;
    color: #9ca3af;
  }

  .branch-search input,
  .branch-option,
  .branch-create-btn,
  .dialog-header h3,
  .dialog-label,
  .dialog-error {
    color: #f9fafb;
  }

  .branch-section-title,
  .branch-empty,
  .dialog-close {
    color: #9ca3af;
  }

  .branch-create-btn {
    border-top-color: #374151;
  }

  .dialog-input {
    background: #1f2937;
    border-color: #374151;
    color: #f9fafb;
  }

  .dialog-btn.secondary {
    background: #374151;
    color: #f9fafb;
  }

  .dialog-btn.primary {
    background: #6b7280;
  }
}
</style>
