<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { HugeiconsIcon } from '@hugeicons/vue';
import { storeToRefs } from 'pinia';
import {
  ApiIcon,
  Settings01Icon,
  ChartIcon,
  Plug01Icon,
} from '@hugeicons/core-free-icons';
import CliTab from './CliTab.vue';
import Stats from './Stats.vue';
import Extensions from './Extensions.vue';
import ProviderTab from './ProviderTab.vue';
import { useUpdaterStore } from '../stores/updater';
import { useClaudeStore } from '../stores/claude';
import {
  PERMISSION_MODES,
  getPermissionModeDescription,
  getPermissionModeDisplayName,
} from '../utils/permissionMode';
import type { PermissionMode } from '../types';
import {
  applyChatFontSize,
  applyInterfaceFontSize,
  CHAT_FONT_SIZE_EVENT,
  clampChatFontSize,
  clampInterfaceFontSize,
  DEFAULT_CHAT_FONT_SIZE,
  DEFAULT_INTERFACE_FONT_SIZE,
  INTERFACE_FONT_SIZE_EVENT,
  MAX_CHAT_FONT_SIZE,
  MAX_INTERFACE_FONT_SIZE,
  MIN_CHAT_FONT_SIZE,
  MIN_INTERFACE_FONT_SIZE,
} from '../utils/appearance';

// 标签页状态
type TabId = 'extensions' | 'providers' | 'stats' | 'general';
type ThemeMode = 'system' | 'light' | 'dark';
const activeTab = ref<TabId>('extensions');

const tabs = [
  { id: 'extensions' as TabId, label: '扩展', icon: Plug01Icon },
  { id: 'providers' as TabId, label: '供应商', icon: ApiIcon },
  { id: 'stats' as TabId, label: '统计', icon: ChartIcon },
  { id: 'general' as TabId, label: '通用', icon: Settings01Icon },
];

const debugEnabled = ref(false);
const themeColor = ref('#3b82f6');
const themeMode = ref<ThemeMode>('system');
const interfaceFontSize = ref(DEFAULT_INTERFACE_FONT_SIZE);
const chatFontSize = ref(DEFAULT_CHAT_FONT_SIZE);
const showThemeModeSetting = false;
const showLanguageSetting = false;
const loading = ref(true);
const userAgent = navigator.userAgent.toLowerCase();
const isWindows = userAgent.includes('windows');
const isMac = userAgent.includes('macintosh') || userAgent.includes('mac os');
const updaterStore = useUpdaterStore();
const claudeStore = useClaudeStore();
const {
  isEnabled: autoUpdateEnabled,
  disabledReason: autoUpdateDisabledReason,
  currentVersion,
  updateAvailable,
  updateVersion,
  status: updateStatus,
  progress: updateProgress,
  errorMessage: updateErrorMessage,
  actionLabel: updateActionLabel,
} = storeToRefs(updaterStore);
const { defaultPermissionMode } = storeToRefs(claudeStore);

const chatFontSizeDescription = computed(() => {
  if (isWindows) {
    return '调整聊天消息正文；Windows 下可用 Ctrl + + / - 快速增减';
  }

  if (isMac) {
    return '调整聊天消息正文；macOS 下可用 Command + + / - 快速增减';
  }

  return '调整聊天消息正文；可用系统对应的缩放快捷键快速增减';
});

// 预设颜色选项
const presetColors = [
  { name: '默认蓝', value: '#3b82f6' },
  { name: '翠绿', value: '#10b981' },
  { name: '紫色', value: '#8b5cf6' },
  { name: '粉色', value: '#ec4899' },
  { name: '橙色', value: '#f97316' },
  { name: '青色', value: '#06b6d4' },
  { name: '红色', value: '#ef4444' },
  { name: '琥珀', value: '#f59e0b' },
];

function syncInterfaceFontSize(event: Event) {
  interfaceFontSize.value = clampInterfaceFontSize((event as CustomEvent<number>).detail);
}

function syncChatFontSize(event: Event) {
  chatFontSize.value = clampChatFontSize((event as CustomEvent<number>).detail);
}

// 组件挂载时从后端加载配置
onMounted(async () => {
  await updaterStore.initializeVersion();

  window.addEventListener(INTERFACE_FONT_SIZE_EVENT, syncInterfaceFontSize as EventListener);
  window.addEventListener(CHAT_FONT_SIZE_EVENT, syncChatFontSize as EventListener);

  try {
    const enabled = await invoke<boolean>('get_debug_enabled');
    debugEnabled.value = enabled;
    console.log('从后端加载调试日志配置:', enabled);
  } catch (error) {
    console.error('加载调试日志配置失败:', error);
    debugEnabled.value = false;
  }

  try {
    const color = await invoke<string>('get_theme_color');
    themeColor.value = color || '#3b82f6';
    console.log('从后端加载主题色:', themeColor.value);
  } catch (error) {
    console.error('加载主题色失败:', error);
    themeColor.value = '#3b82f6';
  }

  try {
    const mode = await invoke<ThemeMode>('get_theme_mode');
    themeMode.value = mode || 'system';
    console.log('从后端加载主题模式:', themeMode.value);
  } catch (error) {
    console.error('加载主题模式失败:', error);
    themeMode.value = 'system';
  }

  try {
    const size = await invoke<number>('get_interface_font_size');
    interfaceFontSize.value = applyInterfaceFontSize(size);
    console.log('从后端加载界面字号:', interfaceFontSize.value);
  } catch (error) {
    console.error('加载界面字号失败:', error);
    interfaceFontSize.value = applyInterfaceFontSize(DEFAULT_INTERFACE_FONT_SIZE);
  }

  try {
    const size = await invoke<number>('get_chat_font_size');
    chatFontSize.value = applyChatFontSize(size);
    console.log('从后端加载对话字号:', chatFontSize.value);
  } catch (error) {
    console.error('加载对话字号失败:', error);
    chatFontSize.value = applyChatFontSize(DEFAULT_CHAT_FONT_SIZE);
  } finally {
    loading.value = false;
  }
});

onUnmounted(() => {
  window.removeEventListener(INTERFACE_FONT_SIZE_EVENT, syncInterfaceFontSize as EventListener);
  window.removeEventListener(CHAT_FONT_SIZE_EVENT, syncChatFontSize as EventListener);
});

async function onDebugToggle() {
  const newValue = !debugEnabled.value;

  try {
    await invoke('set_debug_enabled', { enabled: newValue });
    debugEnabled.value = newValue;
    console.log('调试日志配置已保存:', newValue);
  } catch (error) {
    console.error('保存调试日志配置失败:', error);
  }
}

async function updateThemeMode(mode: ThemeMode) {
  try {
    await invoke('set_theme_mode', { mode });
    themeMode.value = mode;

    const setThemeModeFunc = (window as any).setThemeMode;
    if (typeof setThemeModeFunc === 'function') {
      await setThemeModeFunc(mode);
    }

    console.log('主题模式已保存:', mode);
  } catch (error) {
    console.error('保存主题模式失败:', error);
  }
}

// 更新主题色
async function updateThemeColor(color: string) {
  try {
    await invoke('set_theme_color', { color });
    themeColor.value = color;

    // 更新全局 CSS 变量
    const setThemeColorFunc = (window as any).setThemeColor;
    if (typeof setThemeColorFunc === 'function') {
      setThemeColorFunc(color);
    }

    console.log('主题色已保存:', color);
  } catch (error) {
    console.error('保存主题色失败:', error);
  }
}

async function updateInterfaceFontSize(size: number) {
  const nextSize = clampInterfaceFontSize(size);
  interfaceFontSize.value = applyInterfaceFontSize(nextSize);

  try {
    const savedSize = await invoke<number>('set_interface_font_size', { size: nextSize });
    interfaceFontSize.value = applyInterfaceFontSize(savedSize);
    console.log('界面字号已保存:', savedSize);
  } catch (error) {
    console.error('保存界面字号失败:', error);
  }
}

async function updateChatFontSize(size: number) {
  const nextSize = clampChatFontSize(size);
  chatFontSize.value = applyChatFontSize(nextSize);

  try {
    const savedSize = await invoke<number>('set_chat_font_size', { size: nextSize });
    chatFontSize.value = applyChatFontSize(savedSize);
    console.log('对话字号已保存:', savedSize);
  } catch (error) {
    console.error('保存对话字号失败:', error);
  }
}

function updateDefaultPermissionMode(mode: PermissionMode) {
  claudeStore.setDefaultPermissionMode(mode);
}

const isUpdateBusy = computed(() =>
  updateStatus.value === 'checking' || updateStatus.value === 'downloading'
);

async function handleUpdateAction() {
  await updaterStore.performPrimaryAction();
}

const emit = defineEmits<{
  (e: 'close'): void;
}>();

</script>

<template>
  <div class="settings-view">
    <div class="settings-layout">
      <aside class="settings-sidebar">
        <div class="view-header">
          <button class="back-button" @click="emit('close')">
            <svg class="back-icon" width="24" height="24" viewBox="0 0 24 24" fill="none" aria-hidden="true">
              <path d="M20 12H7" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"/>
              <path d="M11 17L6 12L11 7" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"/>
            </svg>
            <span>返回应用</span>
          </button>
          <h1 class="view-title">设置</h1>
        </div>

        <nav class="tabs-nav">
          <button
            v-for="tab in tabs"
            :key="tab.id"
            :class="['tab-button', { active: activeTab === tab.id }]"
            @click="activeTab = tab.id"
          >
            <span class="tab-button-icon-wrap">
              <HugeiconsIcon :icon="tab.icon" :size="18" class="tab-button-icon" />
            </span>
            <span>{{ tab.label }}</span>
          </button>
        </nav>
      </aside>

      <div class="tabs-content">
      <div v-if="activeTab === 'extensions'" class="tab-pane tab-pane-embedded">
        <Extensions />
      </div>

      <div v-else-if="activeTab === 'providers'" class="tab-pane">
        <ProviderTab />
      </div>

      <div v-else-if="activeTab === 'stats'" class="tab-pane tab-pane-embedded">
        <Stats />
      </div>

      <div v-else class="tab-pane">
        <div class="settings-group">
          <div class="group-header">通用</div>
          <div class="settings-items">
            <div class="setting-item">
              <div class="setting-info">
                <label class="setting-label">主题色</label>
                <span class="setting-description">选择应用的主题颜色</span>
              </div>
              <div class="theme-color-picker">
                <div class="color-presets">
                  <button
                    v-for="preset in presetColors"
                    :key="preset.value"
                    :class="['color-preset', { active: themeColor === preset.value }]"
                    :style="{ backgroundColor: preset.value }"
                    :title="preset.name"
                    @click="updateThemeColor(preset.value)"
                  >
                    <span v-if="themeColor === preset.value" class="check-icon">✓</span>
                  </button>
                </div>
                <div class="color-input-wrapper">
                  <input
                    type="color"
                    :value="themeColor"
                    @input="(e: any) => updateThemeColor(e.target.value)"
                    class="color-input-native"
                  />
                  <span class="color-value">{{ themeColor }}</span>
                </div>
              </div>
            </div>
            <div v-if="showThemeModeSetting" class="setting-item">
              <div class="setting-info">
                <label class="setting-label">主题</label>
              </div>
              <select
                class="setting-select"
                :value="themeMode"
                @change="(e) => updateThemeMode((e.target as HTMLSelectElement).value as ThemeMode)"
              >
                <option value="system">跟随系统</option>
                <option value="light">浅色</option>
                <option value="dark">深色</option>
              </select>
            </div>
            <div v-if="showLanguageSetting" class="setting-item">
              <div class="setting-info">
                <label class="setting-label">语言</label>
              </div>
              <select class="setting-select">
                <option>中文</option>
                <option>English</option>
              </select>
            </div>
            <div class="setting-item">
              <div class="setting-info">
                <label class="setting-label">界面字体大小</label>
                <span class="setting-description">调整设置页、侧栏和大部分界面的基础字号</span>
              </div>
              <div class="font-size-control">
                <button
                  class="font-size-stepper"
                  :disabled="loading || interfaceFontSize <= MIN_INTERFACE_FONT_SIZE"
                  @click="updateInterfaceFontSize(interfaceFontSize - 1)"
                >
                  -
                </button>
                <input
                  class="font-size-slider"
                  type="range"
                  :min="MIN_INTERFACE_FONT_SIZE"
                  :max="MAX_INTERFACE_FONT_SIZE"
                  :value="interfaceFontSize"
                  :disabled="loading"
                  @input="(e: Event) => updateInterfaceFontSize(Number((e.target as HTMLInputElement).value))"
                />
                <span class="font-size-value">{{ interfaceFontSize }}px</span>
                <button
                  class="font-size-stepper"
                  :disabled="loading || interfaceFontSize >= MAX_INTERFACE_FONT_SIZE"
                  @click="updateInterfaceFontSize(interfaceFontSize + 1)"
                >
                  +
                </button>
              </div>
            </div>
            <div class="setting-item">
              <div class="setting-info">
                <label class="setting-label">对话内容字体大小</label>
                <span class="setting-description">{{ chatFontSizeDescription }}</span>
              </div>
              <div class="font-size-control">
                <button
                  class="font-size-stepper"
                  :disabled="loading || chatFontSize <= MIN_CHAT_FONT_SIZE"
                  @click="updateChatFontSize(chatFontSize - 1)"
                >
                  -
                </button>
                <input
                  class="font-size-slider"
                  type="range"
                  :min="MIN_CHAT_FONT_SIZE"
                  :max="MAX_CHAT_FONT_SIZE"
                  :value="chatFontSize"
                  :disabled="loading"
                  @input="(e: Event) => updateChatFontSize(Number((e.target as HTMLInputElement).value))"
                />
                <span class="font-size-value">{{ chatFontSize }}px</span>
                <button
                  class="font-size-stepper"
                  :disabled="loading || chatFontSize >= MAX_CHAT_FONT_SIZE"
                  @click="updateChatFontSize(chatFontSize + 1)"
                >
                  +
                </button>
              </div>
            </div>
            <div class="setting-item">
              <div class="setting-info">
                <label class="setting-label">默认权限模式</label>
                <span class="setting-description">
                  新建会话时默认使用的权限模式。当前为“{{ getPermissionModeDisplayName(defaultPermissionMode) }}”。
                </span>
              </div>
              <select
                class="setting-select"
                :value="defaultPermissionMode"
                @change="(e) => updateDefaultPermissionMode((e.target as HTMLSelectElement).value as PermissionMode)"
              >
                <option
                  v-for="mode in PERMISSION_MODES"
                  :key="mode"
                  :value="mode"
                >
                  {{ getPermissionModeDisplayName(mode) }} - {{ getPermissionModeDescription(mode) }}
                </option>
              </select>
            </div>
            <div class="setting-item">
              <div class="setting-info">
                <label class="setting-label">调试日志</label>
                <span class="setting-description">开启后将后台 Rust、Node 和 CLI 日志追加到 `~/.aite/debug.log`</span>
              </div>
              <button
                :class="['setting-toggle', { active: debugEnabled, disabled: loading }]"
                @click="!loading && onDebugToggle()"
              >
                <span class="toggle-slider"></span>
              </button>
            </div>
          </div>
        </div>


        <div class="settings-group settings-cli-group">
          <div class="group-header">CLI</div>
          <div class="settings-cli-content">
            <CliTab />
          </div>
        </div>

        <div class="settings-group">
          <div class="group-header">应用更新</div>
          <div class="settings-items">
            <div class="setting-item">
              <div class="setting-info">
                <label class="setting-label">当前版本</label>
                <span class="setting-description">
                  {{ currentVersion ? `v${currentVersion}` : '读取中...' }}
                </span>
              </div>
              <div class="update-version">
                <span class="update-version-chip">
                  {{ currentVersion ? `v${currentVersion}` : '--' }}
                </span>
              </div>
            </div>

            <div class="setting-item">
              <div class="setting-info">
                <label class="setting-label">自动更新</label>
                <span class="setting-description">
                  {{ autoUpdateEnabled
                    ? '启动 5 秒后自动检查，之后每 10 分钟后台检查一次；发现新版本后手动下载安装并重启。'
                    : autoUpdateDisabledReason }}
                </span>
              </div>
              <div class="update-actions">
                <span
                  v-if="updateAvailable && updateVersion"
                  class="update-badge"
                >
                  新版本 v{{ updateVersion }}
                </span>
                <span
                  v-else-if="updateStatus === 'latest'"
                  class="update-badge update-badge-success"
                >
                  已是最新版本
                </span>
                <button
                  class="update-button"
                  :class="{
                    downloading: updateStatus === 'downloading',
                    ready: updateStatus === 'ready',
                  }"
                  :disabled="isUpdateBusy || !autoUpdateEnabled"
                  @click="handleUpdateAction"
                >
                  {{ updateActionLabel }}
                </button>
              </div>
            </div>

            <div
              v-if="updateStatus === 'downloading'"
              class="setting-item update-progress-item"
            >
              <div class="setting-info">
                <label class="setting-label">下载进度</label>
                <span class="setting-description">更新包正在下载和校验中。</span>
              </div>
              <div class="update-progress">
                <div class="update-progress-track">
                  <div
                    class="update-progress-fill"
                    :style="{ width: `${updateProgress}%` }"
                  ></div>
                </div>
                <span class="update-progress-text">{{ updateProgress }}%</span>
              </div>
            </div>

            <div
              v-if="updateErrorMessage"
              class="setting-item update-error-item"
            >
              <div class="setting-info">
                <label class="setting-label">更新错误</label>
                <span class="setting-description">
                  {{ updateErrorMessage }}
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
.settings-view {
  width: 100%;
  height: 100%;
  flex: 1;
  min-width: 0;
  background-color: #f8fafc;
  overflow: hidden;
}

.settings-layout {
  width: 100%;
  height: 100%;
  display: flex;
}

.settings-sidebar {
  width: 220px;
  display: flex;
  flex-direction: column;
  border-right: 1px solid #e2e8f0;
  background-color: #ffffff;
  flex-shrink: 0;
}

.view-header {
  padding: 1.5rem 1rem 1rem 0.875rem;
  background-color: #ffffff;
  border-bottom: 1px solid #e2e8f0;
}

.back-button {
  width: 100%;
  margin-bottom: 0.5rem;
  padding: 0.7rem 0.7rem;
  border: none;
  border-radius: 0.75rem;
  background-color: transparent;
  color: #4b5563;
  font-size: 0.95rem;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.2s ease;
  display: inline-flex;
  align-items: center;
  gap: 0.625rem;
  align-self: stretch;
}

.back-button:hover {
  background-color: #e5e7eb;
  color: #374151;
}

.back-icon {
  flex-shrink: 0;
}

.view-title {
  margin: 0;
  font-size: 1.75rem;
  font-weight: 700;
  color: #1e293b;
  letter-spacing: -0.025em;
}

/* 标签页导航 */
.tabs-nav {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
  padding: 0.75rem;
  background-color: #ffffff;
}

.tab-button {
  position: relative;
  padding: 0.8rem 1rem;
  background-color: transparent;
  border: none;
  border-radius: 0.75rem;
  font-size: 0.9375rem;
  font-weight: 600;
  color: #64748b;
  cursor: pointer;
  transition: all 0.2s ease;
  text-align: left;
  display: flex;
  align-items: center;
  gap: 0.625rem;
}

.tab-button-icon-wrap {
  width: 1rem;
  height: 1rem;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}

.tab-button-icon {
  width: 1rem;
  height: 1rem;
  flex-shrink: 0;
  color: currentColor;
  display: block;
}

.tab-button::after {
  content: '';
  position: absolute;
  top: 12px;
  left: 0;
  width: 4px;
  height: calc(100% - 24px);
  border-radius: 999px;
  background-color: #3b82f6;
  transform: scaleY(0);
  transition: transform 0.2s ease;
}

.tab-button:hover {
  color: #1e293b;
  background-color: rgba(59, 130, 246, 0.08);
}

.tab-button.active {
  color: #3b82f6;
  background-color: rgba(59, 130, 246, 0.1);
}

.tab-button.active::after {
  transform: scaleY(1);
}

/* 标签页内容 */
.tabs-content {
  flex: 1;
  min-width: 0;
  min-height: 0;
  overflow-y: auto;
  padding: 1.25rem 2rem 1.5rem;
  background-color: #f8fafc;
  display: flex;
  justify-content: center;
  align-items: flex-start;
}

.tab-pane {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  min-height: auto;
  width: min(100%, 780px);
  padding-bottom: 1rem;
}

.tab-pane:not(.tab-pane-embedded) {
  max-width: 780px;
}

.tab-pane-embedded {
  display: flex;
  flex: 1;
  min-height: 0;
  align-self: stretch;
  width: 100%;
  max-width: none;
  padding: 0;
  overflow: hidden;
}

.tab-pane-embedded > * {
  flex: 1;
  min-height: 0;
}

.settings-cli-group {
  overflow: hidden;
}

.settings-cli-content {
  background-color: #ffffff;
}

.settings-cli-content :deep(.cli-tab) {
  gap: 0;
  background-color: transparent;
  border: none;
  border-radius: 0;
  overflow: visible;
}

.settings-cli-content :deep(.cli-header) {
  padding: 1rem 1.25rem;
}

.settings-cli-content :deep(.cli-title) {
  gap: 0.5rem;
}

.settings-cli-content :deep(.cli-title h2) {
  font-size: 1rem;
}

.settings-cli-content :deep(.cli-status) {
  padding: 0.25rem 0.625rem;
}

.settings-cli-content :deep(.info-item) {
  gap: 0.375rem;
  padding: 0.875rem 1.25rem;
}

.settings-cli-content :deep(.warning-banner),
.settings-cli-content :deep(.not-found),
.settings-cli-content :deep(.error-state) {
  padding: 0.875rem 1.25rem;
}

/* 设置组 */
.settings-group {
  background-color: #ffffff;
  border: 1px solid #e2e8f0;
  border-radius: 0.875rem;
  overflow: hidden;
  box-shadow: 0 1px 3px 0 rgba(0, 0, 0, 0.1), 0 1px 2px -1px rgba(0, 0, 0, 0.1);
}

.group-header {
  padding: 0.9rem 1.25rem;
  font-size: 0.875rem;
  font-weight: 700;
  color: #475569;
  text-transform: uppercase;
  letter-spacing: 0.075em;
  background-color: #f8fafc;
  border-bottom: 1px solid #e2e8f0;
}

.settings-items {
  padding: 0.125rem 0;
}

.setting-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.9rem 1.25rem;
  border-bottom: 1px solid #f1f5f9;
  transition: background-color 0.15s ease;
}

.setting-item:last-child {
  border-bottom: none;
}

.setting-item:hover {
  background-color: #f8fafc;
}

.setting-info {
  flex: 1;
}

.setting-label {
  font-size: 0.9375rem;
  line-height: 1.35;
  font-weight: 500;
  color: #1e293b;
}

.setting-description {
  display: block;
  font-size: 0.8125rem;
  line-height: 1.35;
  color: #64748b;
  margin-top: 0.125rem;
}

/* 主题色选择器 */
.theme-color-picker {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
  align-items: flex-end;
}

.color-presets {
  display: flex;
  gap: 0.5rem;
  flex-wrap: wrap;
  justify-content: flex-end;
}

.color-preset {
  position: relative;
  width: 32px;
  height: 32px;
  border-radius: 50%;
  border: 2px solid transparent;
  cursor: pointer;
  transition: all 0.2s ease;
  display: flex;
  align-items: center;
  justify-content: center;
}

.color-preset:hover {
  transform: scale(1.1);
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.15);
}

.color-preset.active {
  border-color: #1e293b;
  box-shadow: 0 0 0 2px #ffffff, 0 2px 8px rgba(0, 0, 0, 0.15);
}

.check-icon {
  color: #ffffff;
  font-size: 0.875rem;
  font-weight: bold;
  text-shadow: 0 1px 2px rgba(0, 0, 0, 0.3);
}

.color-input-wrapper {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.color-input-native {
  width: 40px;
  height: 32px;
  border: none;
  border-radius: 4px;
  cursor: pointer;
  padding: 0;
  background: none;
}

.color-value {
  font-size: 0.875rem;
  font-family: 'SF Mono', 'Monaco', 'Cascadia Code', 'Roboto Mono', monospace;
  color: #64748b;
  min-width: 90px;
}

.setting-input,
.setting-select {
  padding: 0.625rem 1rem;
  background-color: #f8fafc;
  border: 1px solid #e2e8f0;
  border-radius: 0.5rem;
  font-size: 0.9375rem;
  color: #1e293b;
  min-width: 200px;
  font-weight: 500;
  transition: border-color 0.2s ease, box-shadow 0.2s ease, background-color 0.2s ease;
  appearance: none;
  background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' fill='none' viewBox='0 0 20 20'%3E%3Cpath stroke='%2364748b' stroke-linecap='round' stroke-linejoin='round' stroke-width='1.5' d='M6 8l4 4 4-4'/%3E%3C/svg%3E");
  background-repeat: no-repeat;
  background-position: right 0.75rem center;
  background-size: 1.25rem;
  padding-right: 2.5rem;
}

.setting-input:hover,
.setting-select:hover {
  border-color: #94a3b8;
  background-color: #f1f5f9;
}

.setting-input:focus,
.setting-select:focus {
  outline: none;
  border-color: #3b82f6;
  background-color: #f8fafc;
  box-shadow: 0 0 0 3px rgba(59, 130, 246, 0.1);
}

/* 下拉选项样式 */
.setting-select option {
  background-color: #ffffff;
  color: #1e293b;
  padding: 0.5rem 0.75rem;
}

.setting-toggle {
  position: relative;
  width: 48px;
  height: 26px;
  background-color: #cbd5e1;
  border: none;
  border-radius: 13px;
  cursor: pointer;
  padding: 0;
  transition: background-color 0.2s ease;
  box-shadow: inset 0 1px 2px rgba(0, 0, 0, 0.1);
}

.setting-toggle:hover {
  background-color: #94a3b8;
}

.setting-toggle.active {
  background-color: #3b82f6;
  box-shadow: 0 1px 3px rgba(59, 130, 246, 0.3);
}

.setting-toggle:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.toggle-slider {
  position: absolute;
  top: 3px;
  left: 3px;
  width: 20px;
  height: 20px;
  background-color: #ffffff;
  border-radius: 50%;
  transition: transform 0.2s ease;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.2);
}

.setting-toggle.active .toggle-slider {
  transform: translateX(22px);
}

.font-size-control {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  min-width: min(100%, 360px);
}

.font-size-stepper {
  width: 2rem;
  height: 2rem;
  border: 1px solid #cbd5e1;
  border-radius: 0.625rem;
  background-color: #ffffff;
  color: #334155;
  font-size: 1rem;
  font-weight: 700;
  cursor: pointer;
  transition: all 0.2s ease;
}

.font-size-stepper:hover:not(:disabled) {
  border-color: #94a3b8;
  background-color: #f8fafc;
}

.font-size-stepper:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.font-size-slider {
  flex: 1;
  accent-color: var(--primary-color, #3b82f6);
}

.font-size-value {
  min-width: 3.5rem;
  text-align: center;
  font-size: 0.875rem;
  font-weight: 600;
  color: #475569;
}

.update-actions {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  flex-wrap: wrap;
  justify-content: flex-end;
}

.update-button {
  border: none;
  border-radius: 0.75rem;
  background: linear-gradient(135deg, var(--primary-color) 0%, var(--primary-hover) 100%);
  color: #ffffff;
  font-size: 0.875rem;
  font-weight: 600;
  padding: 0.7rem 1rem;
  cursor: pointer;
  transition: transform 0.2s ease, box-shadow 0.2s ease, opacity 0.2s ease;
  box-shadow: 0 10px 24px rgba(var(--primary-color-rgb), 0.18);
}

.update-button:hover:not(:disabled) {
  transform: translateY(-1px);
  box-shadow: 0 14px 28px rgba(var(--primary-color-rgb), 0.22);
}

.update-button:disabled {
  cursor: wait;
  opacity: 0.72;
}

.update-button.ready {
  background: linear-gradient(135deg, #059669 0%, #10b981 100%);
  box-shadow: 0 10px 24px rgba(16, 185, 129, 0.22);
}

.update-button.downloading {
  min-width: 132px;
}

.update-badge,
.update-version-chip {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-height: 34px;
  padding: 0.45rem 0.85rem;
  border-radius: 999px;
  background-color: rgba(var(--primary-color-rgb), 0.12);
  color: var(--primary-color);
  font-size: 0.8125rem;
  font-weight: 600;
}

.update-badge-success {
  background-color: rgba(16, 185, 129, 0.12);
  color: #047857;
}

.update-version {
  display: flex;
  align-items: center;
  justify-content: flex-end;
}

.update-progress-item {
  align-items: center;
}

.update-progress {
  width: min(240px, 100%);
  display: flex;
  align-items: center;
  gap: 0.75rem;
}

.update-progress-track {
  flex: 1;
  height: 10px;
  border-radius: 999px;
  background-color: #e2e8f0;
  overflow: hidden;
}

.update-progress-fill {
  height: 100%;
  border-radius: inherit;
  background: linear-gradient(90deg, var(--primary-color) 0%, var(--primary-hover) 100%);
  transition: width 0.2s ease;
}

.update-progress-text {
  min-width: 44px;
  text-align: right;
  font-size: 0.8125rem;
  color: #475569;
  font-weight: 600;
}

.update-error-item {
  align-items: flex-start;
}

/* 深色模式 */
@media (prefers-color-scheme: dark) {
  .settings-view {
    background-color: #0f172a;
  }

  .settings-sidebar {
    background-color: #1e293b;
    border-right-color: #334155;
  }

  .view-header {
    background-color: #1e293b;
    border-bottom-color: #334155;
  }

  .back-button {
    background-color: transparent;
    color: #f3f4f6;
  }

  .back-button:hover {
    background-color: #374151;
    color: #f9fafb;
  }

  .tabs-nav {
    background-color: #1e293b;
  }

  .tab-button {
    color: #94a3b8;
  }

  .tab-button:hover {
    color: #e2e8f0;
    background-color: rgba(96, 165, 250, 0.12);
  }

  .tab-button.active {
    color: #e2e8f0;
    background-color: rgba(96, 165, 250, 0.14);
  }

  .tab-button.active::after {
    background-color: #60a5fa;
  }

  .tabs-content {
    background-color: #0f172a;
  }

  .settings-group {
    background-color: #1e293b;
    border-color: #334155;
    box-shadow: 0 1px 3px 0 rgba(0, 0, 0, 0.3), 0 1px 2px -1px rgba(0, 0, 0, 0.2);
  }

  .group-header {
    background-color: #0f172a;
    color: #94a3b8;
    border-bottom-color: #334155;
  }

  .setting-item {
    border-bottom-color: #334155;
  }

  .setting-item:hover {
    background-color: #1a2336;
  }

  .setting-label {
    color: #e2e8f0;
  }

  .setting-description {
    color: #94a3b8;
  }

  /* 深色模式下的主题色选择器 */
  .color-preset.active {
    border-color: #e2e8f0;
    box-shadow: 0 0 0 2px #1e293b, 0 2px 8px rgba(0, 0, 0, 0.3);
  }

  .color-value {
    color: #94a3b8;
  }

  .setting-input,
  .setting-select {
    background-color: #1e293b;
    border-color: #334155;
    color: #e2e8f0;
    background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' fill='none' viewBox='0 0 20 20'%3E%3Cpath stroke='%2394a3b8' stroke-linecap='round' stroke-linejoin='round' stroke-width='1.5' d='M6 8l4 4 4-4'/%3E%3C/svg%3E");
  }

  .setting-input:hover,
  .setting-select:hover {
    border-color: #64748b;
    background-color: #334155;
  }

  .setting-input:focus,
  .setting-select:focus {
    border-color: #60a5fa;
    background-color: #1e293b;
    box-shadow: 0 0 0 3px rgba(96, 165, 250, 0.15);
  }

  .setting-select option {
    background-color: #1e293b;
    color: #e2e8f0;
  }

  .setting-toggle {
    background-color: #475569;
  }

  .setting-toggle:hover {
    background-color: #64748b;
  }

  .setting-toggle.active {
    background-color: #2563eb;
    box-shadow: 0 1px 3px rgba(37, 99, 235, 0.4);
  }

  .font-size-stepper {
    background-color: #1e293b;
    border-color: #334155;
    color: #e2e8f0;
  }

  .font-size-stepper:hover:not(:disabled) {
    background-color: #334155;
    border-color: #475569;
  }

  .font-size-value {
    color: #cbd5e1;
  }

  .update-badge,
  .update-version-chip {
    background-color: rgba(96, 165, 250, 0.16);
    color: #bfdbfe;
  }

  .update-badge-success {
    background-color: rgba(16, 185, 129, 0.18);
    color: #6ee7b7;
  }

  .update-progress-track {
    background-color: #334155;
  }

  .update-progress-text {
    color: #cbd5e1;
  }
}

/* 响应式 */
@media (max-width: 640px) {
  .settings-layout {
    flex-direction: column;
  }

  .settings-sidebar {
    width: 100%;
    border-right: none;
    border-bottom: 1px solid #e2e8f0;
  }

  .view-header {
    padding: 1.5rem 1.5rem 1.25rem;
  }

  .view-title {
    font-size: 1.5rem;
  }

  .tabs-nav {
    padding: 0 1rem 1rem;
    flex-direction: row;
    overflow-x: auto;
    -webkit-overflow-scrolling: touch;
  }

  .tab-button {
    padding: 0.875rem 1.25rem;
    font-size: 0.875rem;
    white-space: nowrap;
  }

  .tab-button::after {
    top: auto;
    bottom: 0;
    left: 1rem;
    width: calc(100% - 2rem);
    height: 3px;
    transform: scaleX(0);
  }

  .tab-button.active::after {
    transform: scaleX(1);
  }

  .tabs-content {
    padding: 1.5rem 1.5rem;
  }

  .tab-pane {
    gap: 1.25rem;
  }

  .group-header {
    padding: 1rem 1.25rem;
  }

  .setting-item {
    padding: 1rem 1.25rem;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.75rem;
  }

  .setting-input,
  .setting-select {
    width: 100%;
    min-width: unset;
  }

  .setting-toggle {
    align-self: flex-end;
  }

  .font-size-control {
    width: 100%;
    min-width: unset;
  }

  .update-actions,
  .update-version,
  .update-progress {
    width: 100%;
    justify-content: flex-start;
  }

  .update-button {
    width: 100%;
  }
}
</style>
