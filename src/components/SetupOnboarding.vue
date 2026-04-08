<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

interface CliStatus {
  installed: boolean;
  path: string | null;
  version: string | null;
  git_bash_missing: boolean;
}

interface SetupProgressEvent {
  percent: number;
  phase: string;
}

interface NodeEnvStatus {
  node_available: boolean;
  node_version: string | null;
  node_source: string | null;
  npm_available: boolean;
}

type SetupStep = 'checking' | 'not_installed' | 'installing' | 'install_failed' | 'installed';

const emit = defineEmits<{
  (e: 'complete'): void;
}>();

const step = ref<SetupStep>('checking');
const errorMsg = ref('');
const cliStatus = ref<CliStatus>({
  installed: false,
  path: null,
  version: null,
  git_bash_missing: false,
});
const downloadPercent = ref(0);
const downloadPhase = ref('idle');
const actionBusy = ref(false);
const userAgent = navigator.userAgent.toLowerCase();
const isWindows = userAgent.includes('windows');

let unlistenProgress: UnlistenFn | null = null;

function toErrorMessage(error: unknown): string {
  if (typeof error === 'string') return error;
  if (error instanceof Error) return error.message;
  return String(error);
}

async function markSetupCompleted() {
  await invoke('set_setup_completed', { completed: true });
  emit('complete');
}

async function handleDetectionResult(result: CliStatus) {
  cliStatus.value = result;

  if (result.installed && !result.git_bash_missing) {
    step.value = 'installed';
    await markSetupCompleted();
    return;
  }

  step.value = 'not_installed';
}

async function checkCli() {
  step.value = 'checking';
  errorMsg.value = '';

  try {
    const result = await invoke<CliStatus>('check_claude_cli');
    await handleDetectionResult(result);
  } catch (error) {
    errorMsg.value = toErrorMessage(error);
    step.value = 'not_installed';
  }
}

async function ensureProgressListener() {
  if (unlistenProgress) return;
  unlistenProgress = await listen<SetupProgressEvent>('setup:download:progress', (event) => {
    downloadPercent.value = event.payload.percent ?? 0;
    downloadPhase.value = event.payload.phase ?? 'idle';
  });
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

async function handleInstall() {
  const decision = await resolveNodeInstallDecision();
  if (!decision.proceed) {
    return;
  }

  actionBusy.value = true;
  errorMsg.value = '';
  downloadPercent.value = 0;
  downloadPhase.value = 'idle';
  step.value = 'installing';

  try {
    await ensureProgressListener();
    await invoke(
      decision.allowNodeInstall ? 'install_claude_cli_with_node_runtime' : 'install_claude_cli',
    );
    const result = await invoke<CliStatus>('check_claude_cli');
    await handleDetectionResult(result);
  } catch (error) {
    errorMsg.value = toErrorMessage(error);
    step.value = 'install_failed';
  } finally {
    actionBusy.value = false;
  }
}

async function handleSkip() {
  actionBusy.value = true;
  try {
    await markSetupCompleted();
  } finally {
    actionBusy.value = false;
  }
}

const statusHeadline = computed(() => {
  if (step.value === 'checking') return '正在检查环境';
  if (step.value === 'installing') return '正在准备 Claude Code';
  if (step.value === 'install_failed') return '安装没有完成';
  if (cliStatus.value.installed && cliStatus.value.git_bash_missing) {
    return '检测到 Claude Code，但还缺运行依赖';
  }
  return '需要先准备 Claude Code';
});

const statusDescription = computed(() => {
  if (step.value === 'checking') {
    return isWindows
      ? '我们会检查 Claude Code CLI，并补齐 Windows 所需的运行依赖。涉及本地 Node.js 运行时安装时会先征求你的确认。'
      : '我们会检查 Claude Code CLI，并补齐缺失项。';
  }
  if (step.value === 'installing') {
    return isWindows
      ? '安装过程中会先补齐运行依赖，再尝试安装 Claude Code CLI。若需要安装本地 Node.js 运行时，会先征求你的确认。'
      : '安装过程中会尝试安装 Claude Code CLI，并自动完成验证。若需要安装本地 Node.js 运行时，会先征求你的确认。';
  }
  if (step.value === 'install_failed') {
    return '可以重试安装，也可以先跳过进入应用，稍后再到设置里处理。';
  }
  if (cliStatus.value.installed && cliStatus.value.git_bash_missing) {
    return '你的系统已经有 Claude Code CLI，但还缺少运行依赖。继续后会自动补齐。';
  }
  return '首次使用前需要先准备 Claude Code CLI。我们会尽量自动完成。';
});

const phaseLabel = computed(() => {
  switch (downloadPhase.value) {
    case 'node_downloading':
      return '正在拉取 Node.js 运行时';
    case 'node_extracting':
      return '正在部署 Node.js 运行时';
    case 'node_complete':
      return 'Node.js 已就位，继续准备 CLI';
    case 'git_downloading':
      return '正在拉取 Git Bash 运行环境';
    case 'git_extracting':
      return '正在部署 PortableGit';
    case 'git_complete':
      return 'Git Bash 已就位，继续准备 CLI';
    case 'npm_fallback':
      return '正在通过 npm 安装 Claude Code CLI';
    case 'installing':
      return '正在收尾并验证安装结果';
    case 'complete':
      return '安装完成，正在进入应用';
    default:
      return '准备中';
  }
});

const progressWidth = computed(() => `${Math.max(downloadPercent.value, step.value === 'installing' ? 8 : 0)}%`);

onMounted(async () => {
  await checkCli();
});

onBeforeUnmount(() => {
  unlistenProgress?.();
  unlistenProgress = null;
});
</script>

<template>
  <section class="setup-shell">
    <div class="setup-grid">
      <div class="setup-panel">
        <div class="panel-topline">Environment Check</div>
        <div class="panel-title">{{ statusHeadline }}</div>
        <div class="panel-description">{{ statusDescription }}</div>

        <div v-if="step === 'checking'" class="activity-card">
          <div class="activity-inline">
            <span class="activity-sigil">/</span>
            <span>正在检测本机环境与启动依赖</span>
          </div>
          <div class="activity-track" aria-hidden="true">
            <div class="activity-indicator" />
          </div>
        </div>

        <div class="status-stack">
          <div class="status-card">
            <span class="status-label">Claude CLI</span>
            <strong class="status-value">
              {{ cliStatus.installed ? (cliStatus.version || '已安装') : '未安装' }}
            </strong>
            <span v-if="cliStatus.path" class="status-footnote" :title="cliStatus.path">
              {{ cliStatus.path }}
            </span>
          </div>

          <div v-if="isWindows" class="status-card">
            <span class="status-label">Windows 依赖</span>
            <strong class="status-value">
              {{ cliStatus.git_bash_missing ? 'Git Bash 缺失' : '已就绪' }}
            </strong>
            <span class="status-footnote">
              {{ cliStatus.git_bash_missing ? '继续后会自动尝试补齐 PortableGit。' : '当前运行环境满足启动要求。' }}
            </span>
          </div>
        </div>

        <div v-if="step === 'installing'" class="progress-card">
          <div class="progress-header">
            <span>{{ phaseLabel }}</span>
            <span>{{ downloadPercent }}%</span>
          </div>
          <div class="progress-track">
            <div class="progress-bar" :style="{ width: progressWidth }" />
          </div>
        </div>

        <div v-if="errorMsg && step === 'install_failed'" class="error-card">
          {{ errorMsg }}
        </div>

        <div v-if="step === 'installed'" class="success-card">
          <div class="success-inline">
            <span class="success-badge">✓</span>
            <span>环境验证完成，正在进入主界面</span>
          </div>
        </div>

        <div class="panel-actions">
          <button
            class="primary-action"
            :disabled="actionBusy || step === 'checking'"
            @click="handleInstall"
          >
            {{ step === 'install_failed' ? '重试安装' : cliStatus.installed ? '补齐并继续' : '安装并继续' }}
          </button>
          <button
            class="secondary-action"
            :disabled="actionBusy || step === 'checking' || step === 'installing'"
            @click="handleSkip"
          >
            先跳过
          </button>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.setup-shell {
  position: relative;
  display: flex;
  min-height: 100%;
  padding: 32px;
  background:
    radial-gradient(circle at top left, rgba(var(--primary-color-rgb), 0.16), transparent 34%),
    linear-gradient(135deg, #fcfaf4 0%, #f6f2e8 48%, #f7f8fb 100%);
  overflow: hidden;
}

:global(:root[data-theme-resolved='dark']) .setup-shell {
  background:
    radial-gradient(circle at top left, rgba(var(--primary-color-rgb), 0.22), transparent 34%),
    linear-gradient(135deg, #17181d 0%, #111827 46%, #10141e 100%);
}

.setup-shell::before {
  content: '';
  position: absolute;
  inset: 18px;
  border: 1px solid rgba(15, 23, 42, 0.08);
  border-radius: 28px;
  pointer-events: none;
}

:global(:root[data-theme-resolved='dark']) .setup-shell::before {
  border-color: rgba(255, 255, 255, 0.08);
}

.setup-grid {
  position: relative;
  z-index: 1;
  width: 100%;
}

.setup-panel {
  max-width: 460px;
  margin: 0 auto;
}

.setup-hero,
.setup-panel {
  border-radius: 28px;
  backdrop-filter: blur(12px);
}

.setup-hero {
  display: none;
}

.setup-grid {
  width: 100%;
}

.setup-panel {
  display: flex;
  flex-direction: column;
  padding: 32px;
  background: rgba(14, 20, 33, 0.94);
  color: #e5e7eb;
  box-shadow: 0 30px 60px rgba(15, 23, 42, 0.18);
}

:global(:root[data-theme-resolved='dark']) .setup-panel {
  background: rgba(6, 10, 18, 0.92);
}

.panel-topline {
  font-size: 12px;
  letter-spacing: 0.18em;
  text-transform: uppercase;
  color: rgba(229, 231, 235, 0.64);
}

.panel-title {
  margin-top: 18px;
  font-size: 30px;
  line-height: 1.15;
  font-weight: 700;
  color: #f8fafc;
}

.panel-description {
  margin-top: 14px;
  color: rgba(229, 231, 235, 0.74);
  font-size: 14px;
  line-height: 1.8;
}

.status-stack {
  display: grid;
  gap: 12px;
  margin-top: 28px;
}

.status-card,
.activity-card,
.progress-card,
.success-card,
.error-card {
  padding: 16px 18px;
  border-radius: 18px;
  background: rgba(255, 255, 255, 0.05);
  border: 1px solid rgba(255, 255, 255, 0.08);
}

.activity-card {
  margin-top: 22px;
}

.activity-inline,
.success-inline {
  display: flex;
  align-items: center;
  gap: 10px;
  color: #f8fafc;
  font-size: 14px;
  font-weight: 600;
}

.activity-sigil,
.success-badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border-radius: 10px;
  flex: 0 0 auto;
}

.activity-sigil {
  background: rgba(var(--primary-color-rgb), 0.16);
  color: #f8fafc;
  animation: activity-pulse 1.45s ease-in-out infinite;
}

.activity-track {
  position: relative;
  height: 8px;
  margin-top: 14px;
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.08);
  overflow: hidden;
}

.activity-indicator {
  position: absolute;
  inset: 0 auto 0 0;
  width: 30%;
  border-radius: 999px;
  background: linear-gradient(90deg, #f59e0b 0%, #f97316 52%, #fb7185 100%);
  animation: activity-progress 1.4s ease-in-out infinite;
}

.status-label {
  display: block;
  font-size: 11px;
  letter-spacing: 0.14em;
  text-transform: uppercase;
  color: rgba(229, 231, 235, 0.56);
}

.status-value {
  display: block;
  margin-top: 10px;
  font-size: 20px;
  color: #f8fafc;
}

.status-footnote {
  display: block;
  margin-top: 10px;
  color: rgba(229, 231, 235, 0.68);
  font-size: 12px;
  line-height: 1.6;
  word-break: break-all;
}

.progress-card {
  margin-top: 20px;
}

.progress-header {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  color: rgba(248, 250, 252, 0.82);
  font-size: 13px;
}

.progress-track {
  height: 10px;
  margin-top: 14px;
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.08);
  overflow: hidden;
}

.progress-bar {
  height: 100%;
  border-radius: 999px;
  background: linear-gradient(90deg, #f59e0b 0%, #f97316 52%, #fb7185 100%);
  transition: width 0.28s ease;
}

.error-card {
  margin-top: 20px;
  color: #fecaca;
  line-height: 1.7;
  background: rgba(239, 68, 68, 0.1);
  border-color: rgba(248, 113, 113, 0.34);
}

.success-card {
  margin-top: 20px;
  background: rgba(16, 185, 129, 0.12);
  border-color: rgba(52, 211, 153, 0.28);
}

.success-badge {
  background: rgba(16, 185, 129, 0.18);
  color: #a7f3d0;
}

.panel-actions {
  display: flex;
  gap: 12px;
  margin-top: auto;
  padding-top: 28px;
}

.primary-action,
.secondary-action {
  appearance: none;
  border: none;
  cursor: pointer;
  transition: transform 0.2s ease, box-shadow 0.2s ease, opacity 0.2s ease;
}

.primary-action {
  flex: 1;
  min-height: 52px;
  padding: 0 18px;
  border-radius: 16px;
  background: linear-gradient(135deg, #f59e0b 0%, #f97316 100%);
  color: #111827;
  font-size: 15px;
  font-weight: 800;
  box-shadow: 0 18px 32px rgba(249, 115, 22, 0.26);
}

.secondary-action {
  min-width: 118px;
  min-height: 52px;
  padding: 0 18px;
  border-radius: 16px;
  background: transparent;
  color: rgba(248, 250, 252, 0.8);
  border: 1px solid rgba(255, 255, 255, 0.12);
  font-size: 14px;
  font-weight: 700;
}

.primary-action:hover:not(:disabled),
.secondary-action:hover:not(:disabled) {
  transform: translateY(-1px);
}

.primary-action:disabled,
.secondary-action:disabled {
  opacity: 0.5;
  cursor: not-allowed;
  transform: none;
}

@keyframes activity-pulse {
  0%,
  100% {
    opacity: 1;
    transform: scale(1);
  }
  50% {
    opacity: 0.55;
    transform: scale(0.95);
  }
}

@keyframes activity-progress {
  0% {
    transform: translateX(-115%);
  }
  100% {
    transform: translateX(330%);
  }
}

@media (max-width: 1080px) {
  .setup-shell {
    padding: 18px;
  }

  .setup-panel {
    padding: 24px;
  }
}

@media (max-width: 720px) {
  .setup-shell::before {
    inset: 10px;
    border-radius: 20px;
  }

  .setup-shell {
    padding: 12px;
  }

  .panel-title {
    font-size: 24px;
  }

  .panel-actions {
    flex-direction: column;
  }

  .secondary-action {
    width: 100%;
  }
}
</style>
