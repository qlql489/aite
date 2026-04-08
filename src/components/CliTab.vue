<script setup lang="ts">
import { ref, computed, onMounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

interface CliStatus {
  installed: boolean;
  path: string | null;
  version: string | null;
  git_bash_missing: boolean;
}

interface NodeEnvStatus {
  node_available: boolean;
  node_version: string | null;
  node_source: string | null;
  npm_available: boolean;
}

type CliCheckStatus = 'idle' | 'checking' | 'found' | 'not_found' | 'error';
type FeedbackType = 'success' | 'error' | 'info';

const CACHE_KEY = 'cli_status_cache';
const CACHE_TTL = 24 * 60 * 60 * 1000;
const CLI_REFERENCE_URL = 'https://code.claude.com/docs/en/cli-reference';

const status = ref<CliCheckStatus>('idle');
const cliStatus = ref<CliStatus>({
  installed: false,
  path: null,
  version: null,
  git_bash_missing: false,
});
const errorMsg = ref('');
const loading = ref(false);
const installingGitBash = ref(false);
const gitBashFeedback = ref('');
const gitBashFeedbackType = ref<FeedbackType>('info');
const installingClaudeCli = ref(false);
const claudeInstallFeedback = ref('');
const claudeInstallFeedbackType = ref<FeedbackType>('info');
const installPercent = ref(0);
const installPhase = ref('idle');
const customArgsInput = ref('');
const customArgsLoading = ref(false);
const customArgsSaving = ref(false);
const customArgsFeedback = ref('');
const customArgsFeedbackType = ref<FeedbackType>('info');

function getCachedData(): { data: CliStatus | null; timestamp: number } | null {
  try {
    const cached = localStorage.getItem(CACHE_KEY);
    if (cached) {
      const parsed = JSON.parse(cached);
      if (Date.now() - parsed.timestamp < CACHE_TTL) {
        return parsed;
      }
    }
  } catch (e) {
    console.warn('读取缓存失败:', e);
  }
  return null;
}

function saveCachedData(data: CliStatus) {
  try {
    localStorage.setItem(CACHE_KEY, JSON.stringify({
      data,
      timestamp: Date.now(),
    }));
  } catch (e) {
    console.warn('保存缓存失败:', e);
  }
}

function clearCachedData() {
  try {
    localStorage.removeItem(CACHE_KEY);
  } catch (e) {
    console.warn('清除缓存失败:', e);
  }
}

function toErrorMessage(error: unknown): string {
  if (typeof error === 'string') return error;
  if (error instanceof Error) return error.message;
  return String(error);
}

function parseCliArgs(input: string): string[] {
  const tokens: string[] = [];
  let current = '';
  let quote: 'single' | 'double' | null = null;
  let escaping = false;
  let tokenStarted = false;

  for (const char of input) {
    if (escaping) {
      current += char;
      escaping = false;
      tokenStarted = true;
      continue;
    }

    if (char === '\\' && quote !== 'single') {
      escaping = true;
      tokenStarted = true;
      continue;
    }

    if (quote === 'single') {
      if (char === '\'') {
        quote = null;
      } else {
        current += char;
      }
      tokenStarted = true;
      continue;
    }

    if (quote === 'double') {
      if (char === '"') {
        quote = null;
      } else {
        current += char;
      }
      tokenStarted = true;
      continue;
    }

    if (char === '\'') {
      quote = 'single';
      tokenStarted = true;
      continue;
    }

    if (char === '"') {
      quote = 'double';
      tokenStarted = true;
      continue;
    }

    if (/\s/.test(char)) {
      if (tokenStarted) {
        tokens.push(current);
        current = '';
        tokenStarted = false;
      }
      continue;
    }

    current += char;
    tokenStarted = true;
  }

  if (escaping) {
    throw new Error('结尾存在未完成的转义符，请检查反斜杠');
  }

  if (quote) {
    throw new Error('存在未闭合的引号，请检查输入');
  }

  if (tokenStarted) {
    tokens.push(current);
  }

  return tokens;
}

function quoteCliArg(arg: string): string {
  if (!arg) return '""';
  if (!/[\s"'\\]/.test(arg)) return arg;
  return `"${arg.replace(/\\/g, '\\\\').replace(/"/g, '\\"')}"`;
}

function formatCliArgs(args: string[]): string {
  return args.map(quoteCliArg).join(' ');
}

async function checkCli(forceRefresh = false) {
  if (!forceRefresh) {
    const cached = getCachedData();
    if (cached?.data) {
      cliStatus.value = cached.data;
      status.value = cached.data.installed ? 'found' : 'not_found';
      console.log('使用缓存数据:', cached.data);
      return;
    }
  }

  status.value = 'checking';
  errorMsg.value = '';
  loading.value = true;

  try {
    const result = await invoke<CliStatus>('check_claude_cli');
    cliStatus.value = result;
    saveCachedData(result);

    if (result.installed) {
      status.value = 'found';
      console.log('Claude CLI 已安装:', {
        version: result.version,
        path: result.path,
      });
    } else {
      status.value = 'not_found';
      console.log('Claude CLI 未安装');
    }
  } catch (error) {
    status.value = 'error';
    errorMsg.value = toErrorMessage(error);
    console.error('检查 Claude CLI 失败:', error);
  } finally {
    loading.value = false;
  }
}

async function loadCustomArgs() {
  customArgsLoading.value = true;
  customArgsFeedback.value = '';

  try {
    const args = await invoke<string[]>('get_claude_cli_extra_args');
    customArgsInput.value = formatCliArgs(args || []);
  } catch (error) {
    customArgsFeedbackType.value = 'error';
    customArgsFeedback.value = `读取自定义参数失败：${toErrorMessage(error)}`;
  } finally {
    customArgsLoading.value = false;
  }
}

async function saveCustomArgs() {
  if (parseError.value) {
    customArgsFeedbackType.value = 'error';
    customArgsFeedback.value = parseError.value;
    return;
  }

  customArgsSaving.value = true;
  customArgsFeedback.value = '';

  try {
    const savedArgs = await invoke<string[]>('set_claude_cli_extra_args', {
      args: parsedArgs.value,
    });
    customArgsInput.value = formatCliArgs(savedArgs || []);
    customArgsFeedbackType.value = 'success';
    customArgsFeedback.value = savedArgs.length > 0
      ? `已保存 ${savedArgs.length} 个启动参数 token，新会话会自动带上这些参数。`
      : '已清空自定义启动参数，新会话将只使用应用默认参数。';
  } catch (error) {
    customArgsFeedbackType.value = 'error';
    customArgsFeedback.value = toErrorMessage(error);
  } finally {
    customArgsSaving.value = false;
  }
}

async function clearCustomArgs() {
  customArgsInput.value = '';
  await saveCustomArgs();
}

function handleRefresh() {
  clearCachedData();
  checkCli(true);
}

async function handleInstallClaudeCli() {
  const decision = await resolveNodeInstallDecision();
  if (!decision.proceed) {
    return;
  }

  installingClaudeCli.value = true;
  claudeInstallFeedback.value = '';
  installPercent.value = 0;
  installPhase.value = 'idle';

  let unlistenProgress: UnlistenFn | null = null;

  try {
    unlistenProgress = await listen<{ percent: number; phase: string }>('setup:download:progress', (event) => {
      installPercent.value = event.payload.percent ?? 0;
      installPhase.value = event.payload.phase ?? 'idle';
    });

    await invoke(
      decision.allowNodeInstall ? 'install_claude_cli_with_node_runtime' : 'install_claude_cli',
    );
    claudeInstallFeedbackType.value = 'success';
    claudeInstallFeedback.value = 'Claude Code CLI 已安装完成，新的会话可以直接使用。';
    clearCachedData();
    await checkCli(true);
  } catch (error) {
    claudeInstallFeedbackType.value = 'error';
    claudeInstallFeedback.value = toErrorMessage(error);
  } finally {
    unlistenProgress?.();
    installingClaudeCli.value = false;
  }
}

async function handleInstallGitBash() {
  installingGitBash.value = true;
  gitBashFeedback.value = '';

  try {
    const bashPath = await invoke<string>('install_portable_git');
    gitBashFeedbackType.value = 'success';
    gitBashFeedback.value = `PortableGit 安装完成，Git Bash 路径：${bashPath}`;
    clearCachedData();
    await checkCli(true);
  } catch (error) {
    gitBashFeedbackType.value = 'error';
    gitBashFeedback.value = toErrorMessage(error);
  } finally {
    installingGitBash.value = false;
  }
}

async function resolveNodeInstallDecision(): Promise<{ proceed: boolean; allowNodeInstall: boolean }> {
  try {
    const nodeEnv = await invoke<NodeEnvStatus>('check_node_env');
    if (nodeEnv.node_available && nodeEnv.npm_available) {
      return { proceed: true, allowNodeInstall: false };
    }
  } catch (error) {
    console.warn('预检查 Node.js 运行时失败，将转为用户确认:', error);
  }

  const confirmed = window.confirm(
    '当前环境未检测到可用的 npm。继续后，应用会下载并安装本地 Node.js 运行时到应用数据目录，不会覆盖你系统里已有的 Node 配置。是否继续？',
  );

  return { proceed: confirmed, allowNodeInstall: confirmed };
}

onMounted(async () => {
  await Promise.all([checkCli(false), loadCustomArgs()]);
});

const cliVersion = computed(() => cliStatus.value.version || '未知');
const cliPath = computed(() => cliStatus.value.path || '未知路径');
const hasGitBashMissing = computed(() => cliStatus.value.git_bash_missing);
const hasCustomArgs = computed(() => customArgsInput.value.trim().length > 0);

const parsedArgs = computed(() => {
  try {
    return parseCliArgs(customArgsInput.value);
  } catch {
    return [];
  }
});

const parsedOptionCount = computed(() =>
  parsedArgs.value.filter((arg) => arg.startsWith('-')).length
);

const parseError = computed(() => {
  try {
    parseCliArgs(customArgsInput.value);
    return '';
  } catch (error) {
    return toErrorMessage(error);
  }
});

const parsedArgSummary = computed(() => {
  if (customArgsLoading.value) return '读取配置中...';
  if (!hasCustomArgs.value) return '未设置自定义参数';
  if (parseError.value) return parseError.value;
  if (parsedOptionCount.value > 0) {
    return `已识别 ${parsedOptionCount.value} 个参数项，会在新会话启动时生效`;
  }
  return `已识别 ${parsedArgs.value.length} 段命令片段，会在新会话启动时生效`;
});

const canSaveCustomArgs = computed(() =>
  !customArgsLoading.value && !customArgsSaving.value && !parseError.value
);

const statusText = computed(() => {
  switch (status.value) {
    case 'idle':
      return '等待检查';
    case 'checking':
      return '检查中...';
    case 'found':
      return '已安装';
    case 'not_found':
      return '未找到 Claude CLI';
    case 'error':
      return '检查出错';
    default:
      return '未知状态';
  }
});

const statusColor = computed(() => {
  switch (status.value) {
    case 'found':
      return 'text-green-500';
    case 'not_found':
      return 'text-amber-500';
    case 'error':
      return 'text-red-500';
    default:
      return 'text-gray-500';
  }
});

const customArgsFeedbackClass = computed(() => {
  switch (customArgsFeedbackType.value) {
    case 'success':
      return 'feedback-success';
    case 'error':
      return 'feedback-error';
    default:
      return 'feedback-info';
  }
});

const gitBashFeedbackClass = computed(() => {
  switch (gitBashFeedbackType.value) {
    case 'success':
      return 'feedback-success';
    case 'error':
      return 'feedback-error';
    default:
      return 'feedback-info';
  }
});

const claudeInstallFeedbackClass = computed(() => {
  switch (claudeInstallFeedbackType.value) {
    case 'success':
      return 'feedback-success';
    case 'error':
      return 'feedback-error';
    default:
      return 'feedback-info';
  }
});

const installPhaseText = computed(() => {
  switch (installPhase.value) {
    case 'node_downloading':
      return '正在下载 Node.js 运行时';
    case 'node_extracting':
      return '正在安装 Node.js 运行时';
    case 'node_complete':
      return 'Node.js 已就位，继续安装 Claude Code CLI';
    case 'git_downloading':
      return '正在下载 Git Bash 环境';
    case 'git_extracting':
      return '正在安装 PortableGit';
    case 'npm_fallback':
      return '正在通过 npm 安装 Claude Code CLI';
    case 'installing':
      return '正在验证安装结果';
    case 'complete':
      return '安装完成';
    default:
      return '准备安装';
  }
});
</script>

<template>
  <div class="cli-tab">
    <div class="cli-summary setting-row-card">
      <div class="cli-summary-main">
        <span class="cli-summary-title">Claude Code CLI</span>
        <span class="cli-status" :class="statusColor">{{ statusText }}</span>
      </div>
      <button
        class="refresh-btn"
        :disabled="loading"
        @click="handleRefresh"
        title="刷新"
      >
        <svg
          v-if="!loading"
          class="refresh-icon"
          width="16"
          height="16"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
        >
          <path d="M23 4v6h-6M1 20v-6h6M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15" />
        </svg>
        <svg
          v-else
          class="loading-icon"
          width="16"
          height="16"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
        >
          <circle cx="12" cy="12" r="10" stroke-opacity="0.3" />
          <path d="M12 2a10 10 0 0 1 10 10">
            <animateTransform
              attributeName="transform"
              type="rotate"
              from="0 12 12"
              to="360 12 12"
              dur="1s"
              repeatCount="indefinite"
            />
          </path>
        </svg>
      </button>
    </div>

    <template v-if="status === 'found'">
      <div class="info-item">
        <span class="info-label">版本号</span>
        <span class="info-value">{{ cliVersion }}</span>
      </div>
      <div class="info-item">
        <span class="info-label">安装路径</span>
        <span class="info-value path-value" :title="cliPath">{{ cliPath }}</span>
      </div>

      <div v-if="hasGitBashMissing" class="warning-banner">
        <svg class="warning-icon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0zM12 9v4M12 17h.01" />
        </svg>
        <div class="warning-content">
          <span>未检测到 Git Bash，Claude Code 需要 Git Bash 才能正常工作</span>
          <div class="warning-actions">
            <button
              class="primary-btn warning-primary-btn"
              :disabled="installingClaudeCli || installingGitBash"
              @click="handleInstallClaudeCli"
            >
              {{ installingClaudeCli ? '处理中...' : '补齐 Claude 环境' }}
            </button>
            <button
              class="secondary-btn warning-btn"
              :disabled="installingGitBash || installingClaudeCli"
              @click="handleInstallGitBash"
            >
              {{ installingGitBash ? '安装中...' : '安装 PortableGit' }}
            </button>
          </div>
          <div v-if="installingClaudeCli" class="install-progress-card">
            <div class="install-progress-header">
              <span>{{ installPhaseText }}</span>
              <span>{{ installPercent }}%</span>
            </div>
            <div class="install-progress-track">
              <div class="install-progress-bar" :style="{ width: `${Math.max(installPercent, 10)}%` }" />
            </div>
          </div>
          <div
            v-if="gitBashFeedback"
            class="cli-feedback warning-feedback"
            :class="gitBashFeedbackClass"
          >
            {{ gitBashFeedback }}
          </div>
          <div
            v-if="claudeInstallFeedback"
            class="cli-feedback warning-feedback"
            :class="claudeInstallFeedbackClass"
          >
            {{ claudeInstallFeedback }}
          </div>
        </div>
      </div>
    </template>

    <div v-else-if="status === 'not_found'" class="not-found">
      <p class="not-found-text">
        未在系统中找到 Claude Code CLI。
      </p>
      <p class="install-hint">
        请使用 <code>npm install -g @anthropic-ai/claude-code</code> 安装，
        或者从 <a href="https://claude.ai/download" target="_blank">Claude 官网</a> 下载桌面应用。
      </p>
      <button
        class="primary-btn install-btn"
        :disabled="installingClaudeCli"
        @click="handleInstallClaudeCli"
      >
        {{ installingClaudeCli ? '安装中...' : '在应用内安装 Claude Code' }}
      </button>
      <div v-if="installingClaudeCli" class="install-progress-card">
        <div class="install-progress-header">
          <span>{{ installPhaseText }}</span>
          <span>{{ installPercent }}%</span>
        </div>
        <div class="install-progress-track">
          <div class="install-progress-bar" :style="{ width: `${Math.max(installPercent, 10)}%` }" />
        </div>
      </div>
      <div
        v-if="claudeInstallFeedback"
        class="cli-feedback"
        :class="claudeInstallFeedbackClass"
      >
        {{ claudeInstallFeedback }}
      </div>
    </div>

    <div v-else-if="status === 'error'" class="error-state">
      <svg class="error-icon" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <circle cx="12" cy="12" r="10" />
        <path d="M15 9l-6 6M9 9l6 6" />
      </svg>
      <p class="error-text">检查 Claude CLI 时出错</p>
      <p class="error-message">{{ errorMsg }}</p>
    </div>

    <div class="cli-config-card">
      <div class="cli-config-header">
        <div class="cli-config-copy">
          <span class="info-label">启动参数</span>
          <h3 class="cli-config-title">自定义 Claude CLI 启动参数</h3>
          <p class="cli-config-description">
            直接填写官方 CLI 参数片段。应用会自动拦截已由自身接管的参数，避免与当前启动流程冲突。
          </p>
        </div>
        <a
          class="docs-link"
          :href="CLI_REFERENCE_URL"
          target="_blank"
          rel="noreferrer"
        >
          查看 CLI 文档
        </a>
      </div>

      <textarea
        v-model="customArgsInput"
        class="cli-args-textarea"
        spellcheck="false"
      />

      <div class="cli-config-meta">
        <span class="cli-meta-text" :class="{ error: !!parseError }">{{ parsedArgSummary }}</span>
        <span class="cli-meta-text muted">修改后仅对新启动的 Claude 会话生效</span>
      </div>

      <div
        v-if="customArgsFeedback"
        class="cli-feedback"
        :class="customArgsFeedbackClass"
      >
        {{ customArgsFeedback }}
      </div>

      <div class="cli-config-actions">
        <button
          class="secondary-btn"
          :disabled="customArgsSaving || customArgsLoading || !hasCustomArgs"
          @click="clearCustomArgs"
        >
          清空
        </button>
        <button
          class="primary-btn"
          :disabled="!canSaveCustomArgs"
          @click="saveCustomArgs"
        >
          {{ customArgsSaving ? '保存中...' : '保存参数' }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.cli-tab {
  display: flex;
  flex-direction: column;
  gap: 0;
  background-color: #ffffff;
  border: 1px solid #e2e8f0;
  border-radius: 0.875rem;
  overflow: hidden;
}

.setting-row-card,
.cli-config-card {
  background-color: transparent;
  border: none;
  border-radius: 0;
}

.setting-row-card {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.9rem 1.25rem;
}

.cli-summary-main {
  display: flex;
  align-items: center;
  gap: 0.625rem;
}

.cli-summary-title {
  margin: 0;
  font-size: 0.9375rem;
  font-weight: 600;
  color: #1e293b;
}

.cli-status {
  font-size: 0.8125rem;
  font-weight: 600;
  padding: 0.375rem 0.75rem;
  border-radius: 9999px;
  transition: background-color 0.2s ease, color 0.2s ease;
}

.cli-status.text-green-500 {
  background-color: #dcfce7;
  color: #16a34a;
}

.cli-status.text-amber-500 {
  background-color: #fef3c7;
  color: #d97706;
}

.cli-status.text-red-500 {
  background-color: #fee2e2;
  color: #dc2626;
}

.cli-status.text-gray-500 {
  background-color: #f1f5f9;
  color: #64748b;
}

.refresh-btn,
.primary-btn,
.secondary-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 0.625rem;
  cursor: pointer;
  transition: all 0.2s ease;
  font-size: 0.875rem;
  font-weight: 600;
}

.refresh-btn {
  width: 38px;
  height: 38px;
  padding: 0;
  background-color: #f8fafc;
  border: 1px solid #cbd5e1;
  color: #64748b;
}

.refresh-btn:hover:not(:disabled) {
  background-color: #3b82f6;
  border-color: #3b82f6;
  color: #ffffff;
  box-shadow: 0 2px 4px rgba(59, 130, 246, 0.2);
}

.primary-btn {
  min-width: 110px;
  height: 40px;
  padding: 0 1rem;
  border: 1px solid #2563eb;
  background: linear-gradient(135deg, #3b82f6 0%, #2563eb 100%);
  color: #ffffff;
  box-shadow: 0 10px 20px rgba(37, 99, 235, 0.16);
}

.primary-btn:hover:not(:disabled) {
  transform: translateY(-1px);
  box-shadow: 0 14px 28px rgba(37, 99, 235, 0.2);
}

.secondary-btn {
  min-width: 88px;
  height: 40px;
  padding: 0 1rem;
  border: 1px solid #cbd5e1;
  background-color: #f8fafc;
  color: #475569;
}

.secondary-btn:hover:not(:disabled) {
  border-color: #94a3b8;
  background-color: #f1f5f9;
}

.refresh-btn:disabled,
.primary-btn:disabled,
.secondary-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
  transform: none;
  box-shadow: none;
}

.refresh-icon,
.loading-icon {
  transition: transform 0.2s ease;
}

.refresh-btn:hover:not(:disabled) .refresh-icon {
  transform: rotate(180deg);
}

.loading-icon {
  animation: spin 1s linear infinite;
}

@keyframes spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}

.cli-config-card {
  padding: 1.1rem 1.25rem 1.2rem;
}

.cli-config-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 1rem;
  margin-bottom: 0.9rem;
}

.cli-config-copy {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
}

.cli-config-title {
  margin: 0;
  font-size: 1rem;
  font-weight: 700;
  color: #0f172a;
}

.cli-config-description {
  margin: 0;
  font-size: 0.9rem;
  line-height: 1.65;
  color: #475569;
  max-width: 760px;
}

.cli-config-description code {
  padding: 0.15rem 0.45rem;
  background-color: #eff6ff;
  border: 1px solid #bfdbfe;
  border-radius: 0.4rem;
  color: #1d4ed8;
  font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Consolas, 'Liberation Mono', monospace;
}

.docs-link {
  flex-shrink: 0;
  color: #2563eb;
  text-decoration: none;
  font-size: 0.875rem;
  font-weight: 600;
}

.docs-link:hover {
  text-decoration: underline;
}

.cli-args-textarea {
  width: 100%;
  min-height: 118px;
  resize: vertical;
  border: 1px solid #cbd5e1;
  border-radius: 0.8rem;
  background: linear-gradient(180deg, #f8fafc 0%, #ffffff 100%);
  padding: 0.95rem 1rem;
  font-size: 0.875rem;
  line-height: 1.7;
  color: #0f172a;
  font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Consolas, 'Liberation Mono', monospace;
}

.cli-args-textarea:focus {
  outline: none;
  border-color: #60a5fa;
  box-shadow: 0 0 0 4px rgba(96, 165, 250, 0.16);
}

.cli-config-meta {
  display: flex;
  justify-content: space-between;
  gap: 1rem;
  margin-top: 0.7rem;
}

.cli-meta-text {
  font-size: 0.8125rem;
  color: #334155;
}

.cli-meta-text.muted {
  color: #64748b;
}

.cli-meta-text.error {
  color: #dc2626;
}

.cli-feedback {
  margin-top: 0.9rem;
  padding: 0.8rem 0.95rem;
  border-radius: 0.75rem;
  font-size: 0.875rem;
  line-height: 1.6;
}

.feedback-success {
  background-color: #ecfdf3;
  border: 1px solid #86efac;
  color: #166534;
}

.feedback-error {
  background-color: #fef2f2;
  border: 1px solid #fca5a5;
  color: #b91c1c;
}

.feedback-info {
  background-color: #eff6ff;
  border: 1px solid #93c5fd;
  color: #1d4ed8;
}

.cli-config-actions {
  display: flex;
  justify-content: flex-end;
  gap: 0.75rem;
  margin-top: 1rem;
}

.info-item {
  display: flex;
  flex-direction: column;
  gap: 0.375rem;
  padding: 0.9rem 1.25rem;
  background-color: transparent;
  border: none;
  border-radius: 0;
}

.info-label {
  font-size: 0.75rem;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.075em;
  color: #64748b;
}

.info-value {
  font-size: 0.9375rem;
  color: #1e293b;
  font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Consolas, 'Liberation Mono', monospace;
  font-weight: 500;
}

.path-value {
  overflow: hidden;
  display: block;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 100%;
  background-color: #f8fafc;
  padding: 0.5rem 0.75rem;
  border-radius: 0.5rem;
  font-size: 0.875rem;
}

.warning-banner {
  display: flex;
  align-items: flex-start;
  gap: 0.75rem;
  padding: 0.9rem 1.25rem;
  background: linear-gradient(135deg, #fffbeb 0%, #fef3c7 100%);
  border: 1px solid #fcd34d;
  border-left: 4px solid #f59e0b;
  border-radius: 0.75rem;
  font-size: 0.875rem;
  color: #92400e;
  line-height: 1.625;
}

.warning-content {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: 0.75rem;
}

.warning-actions {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 0.75rem;
}

.warning-btn {
  min-width: 148px;
}

.warning-primary-btn {
  min-width: 164px;
}

.warning-feedback {
  margin-top: 0;
}

.install-btn {
  align-self: flex-start;
}

.install-progress-card {
  display: flex;
  flex-direction: column;
  gap: 0.65rem;
  padding: 0.85rem 0.95rem;
  border-radius: 0.75rem;
  background-color: rgba(59, 130, 246, 0.08);
  border: 1px solid rgba(59, 130, 246, 0.18);
}

.install-progress-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
  font-size: 0.8125rem;
  color: #1e3a8a;
}

.install-progress-track {
  width: 100%;
  height: 0.5rem;
  border-radius: 999px;
  background-color: rgba(59, 130, 246, 0.14);
  overflow: hidden;
}

.install-progress-bar {
  height: 100%;
  border-radius: 999px;
  background: linear-gradient(90deg, #3b82f6 0%, #0ea5e9 100%);
  transition: width 0.25s ease;
}

.warning-icon {
  flex-shrink: 0;
  margin-top: 0.125rem;
}

.not-found {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  padding: 1.25rem;
  background-color: transparent;
  border: none;
  border-radius: 0;
  text-align: left;
}

.not-found-text {
  font-size: 1rem;
  font-weight: 500;
  color: #475569;
  line-height: 1.625;
}

.install-hint {
  font-size: 0.9375rem;
  color: #334155;
  line-height: 1.625;
  background-color: #f8fafc;
  padding: 1rem 1.5rem;
  border-radius: 0.75rem;
}

.install-hint code {
  padding: 0.125rem 0.5rem;
  background-color: #e2e8f0;
  border-radius: 0.375rem;
  font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Consolas, 'Liberation Mono', monospace;
  font-size: 0.875rem;
  font-weight: 500;
  color: #1e293b;
}

.install-hint a {
  color: #3b82f6;
  text-decoration: none;
  font-weight: 500;
}

.install-hint a:hover {
  text-decoration: underline;
}

.error-state {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  justify-content: center;
  gap: 0.75rem;
  padding: 1.25rem;
  background-color: transparent;
  border: none;
  border-radius: 0;
  text-align: left;
}

.error-icon {
  color: #ef4444;
}

.error-text {
  font-size: 0.95rem;
  font-weight: 600;
  color: #1e293b;
  margin-bottom: 0.25rem;
}

.error-message {
  font-size: 0.875rem;
  color: #64748b;
  line-height: 1.5;
  max-width: 400px;
}

.cli-tab > * + * {
  border-top: 1px solid #e2e8f0;
}

@media (prefers-color-scheme: dark) {
  .cli-tab {
    background-color: #1e293b;
    border-color: #334155;
    box-shadow: 0 1px 3px 0 rgba(0, 0, 0, 0.3), 0 1px 2px -1px rgba(0, 0, 0, 0.2);
  }

  .setting-row-card,
  .cli-config-card,
  .info-item,
  .not-found,
  .error-state {
    background-color: transparent;
    border-color: transparent;
    box-shadow: none;
  }

  .cli-summary-title,
  .cli-config-title,
  .info-value,
  .error-text {
    color: #e2e8f0;
  }

  .cli-tab > * + * {
    border-top-color: #334155;
  }

  .cli-config-description,
  .not-found-text,
  .install-hint,
  .error-message,
  .cli-meta-text,
  .reserved-tag {
    color: #cbd5e1;
  }

  .cli-config-description code {
    background-color: rgba(37, 99, 235, 0.16);
    border-color: rgba(96, 165, 250, 0.4);
    color: #93c5fd;
  }

  .cli-status.text-green-500 {
    background-color: rgba(22, 163, 74, 0.2);
    color: #4ade80;
  }

  .cli-status.text-amber-500 {
    background-color: rgba(217, 119, 6, 0.2);
    color: #fbbf24;
  }

  .cli-status.text-red-500 {
    background-color: rgba(220, 38, 38, 0.2);
    color: #f87171;
  }

  .cli-status.text-gray-500 {
    background-color: rgba(100, 116, 139, 0.2);
    color: #94a3b8;
  }

  .refresh-btn,
  .secondary-btn {
    background-color: #0f172a;
    border-color: #334155;
    color: #94a3b8;
  }

  .refresh-btn:hover:not(:disabled) {
    background-color: #2563eb;
    border-color: #2563eb;
    color: #ffffff;
    box-shadow: 0 2px 4px rgba(37, 99, 235, 0.3);
  }

  .secondary-btn:hover:not(:disabled) {
    border-color: #475569;
    background-color: #111827;
  }

  .cli-args-textarea,
  .path-value,
  .install-hint {
    background-color: #0f172a;
    border-color: #334155;
    color: #e2e8f0;
  }

  .cli-args-textarea:focus {
    border-color: #3b82f6;
    box-shadow: 0 0 0 4px rgba(59, 130, 246, 0.2);
  }

  .cli-meta-text.muted,
  .info-label {
    color: #94a3b8;
  }

  .cli-meta-text.error {
    color: #fca5a5;
  }

  .warning-banner {
    background: linear-gradient(135deg, rgba(245, 158, 11, 0.15) 0%, rgba(251, 191, 36, 0.1) 100%);
    border-color: #d97706;
    border-left-color: #f59e0b;
    color: #fbbf24;
  }

  .install-hint code {
    background-color: #334155;
    color: #e2e8f0;
  }

  .feedback-success {
    background-color: rgba(22, 101, 52, 0.18);
    border-color: rgba(74, 222, 128, 0.35);
    color: #86efac;
  }

  .feedback-error {
    background-color: rgba(127, 29, 29, 0.18);
    border-color: rgba(248, 113, 113, 0.35);
    color: #fca5a5;
  }

  .feedback-info {
    background-color: rgba(29, 78, 216, 0.16);
    border-color: rgba(96, 165, 250, 0.35);
    color: #93c5fd;
  }

  .error-icon {
    color: #f87171;
  }
}

@media (max-width: 640px) {
  .cli-config-header,
  .cli-config-meta,
  .cli-config-actions {
    flex-direction: column;
  }

  .docs-link {
    align-self: flex-start;
  }

  .cli-config-actions {
    align-items: stretch;
  }

  .primary-btn,
  .secondary-btn {
    width: 100%;
  }

  .refresh-btn {
    width: 36px;
    height: 36px;
    align-self: flex-end;
  }

  .info-item,
  .cli-config-card {
    padding: 0.95rem 1rem;
  }

  .path-value,
  .cli-args-textarea {
    font-size: 0.8125rem;
  }

  .warning-banner,
  .not-found,
  .error-state {
    padding: 0.95rem 1rem;
  }
}
</style>
