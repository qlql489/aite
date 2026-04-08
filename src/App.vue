<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue';
import Projects from './components/Projects.vue';
import SetupOnboarding from './components/SetupOnboarding.vue';
import { useStatsStore } from './stores/stats';
import { useClaudeStore } from './stores/claude';
import { useAutoUpdateCheck } from './composables/useAutoUpdateCheck';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import {
  applyAppearanceConfig,
  applyChatFontSize,
  applyInterfaceFontSize,
  CHAT_FONT_SIZE_EVENT,
  clampChatFontSize,
  DEFAULT_CHAT_FONT_SIZE,
  DEFAULT_INTERFACE_FONT_SIZE,
} from './utils/appearance';

type ThemeMode = 'system' | 'light' | 'dark';
type ResolvedTheme = 'light' | 'dark';

const statsStore = useStatsStore();
const claudeStore = useClaudeStore();
useAutoUpdateCheck();

// 主题颜色管理
const defaultThemeColor = '#3b82f6';
const darkThemeMediaQuery = '(prefers-color-scheme: dark)';
const forcedDarkThemeStyleId = 'forced-dark-theme-overrides';
const userAgent = navigator.userAgent.toLowerCase();
const isWindows = userAgent.includes('windows');
const isMacOS = userAgent.includes('mac');
const setupGate = ref<'loading' | 'wizard' | 'ready'>('loading');
const bootHighlights = [
  '恢复主题与字号偏好',
  '读取首次引导状态',
  '准备进入工作区',
] as const;
const chatFontShortcutEventOptions = { capture: true } as const;
const chatFontIndicatorVisible = ref(false);
const chatFontIndicatorSize = ref(DEFAULT_CHAT_FONT_SIZE);

let currentChatFontSize = DEFAULT_CHAT_FONT_SIZE;
let chatFontIndicatorTimer: number | null = null;

// 将 hex 颜色转换为 RGB
function hexToRgb(hex: string): { r: number; g: number; b: number } | null {
  const result = /^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})$/i.exec(hex);
  return result ? {
    r: parseInt(result[1], 16),
    g: parseInt(result[2], 16),
    b: parseInt(result[3], 16)
  } : null;
}

// 设置主题色
function setThemeColor(color: string) {
  const rgb = hexToRgb(color);
  if (!rgb) return;

  const root = document.documentElement;
  root.style.setProperty('--primary-color', color);
  root.style.setProperty('--primary-hover', adjustBrightness(color, -10));
  root.style.setProperty('--primary-bg', color);
  root.style.setProperty('--color-primary', color);

  // 设置透明度版本
  root.style.setProperty('--primary-color-rgb', `${rgb.r}, ${rgb.g}, ${rgb.b}`);
  root.style.setProperty('--primary-color-10', `rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, 0.1)`);
  root.style.setProperty('--primary-color-15', `rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, 0.15)`);
  root.style.setProperty('--primary-color-20', `rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, 0.2)`);
  root.style.setProperty('--primary-color-25', `rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, 0.25)`);
  root.style.setProperty('--primary-color-30', `rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, 0.3)`);
  root.style.setProperty('--primary-color-40', `rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, 0.4)`);
}

// 调整颜色亮度
function adjustBrightness(hex: string, percent: number): string {
  const rgb = hexToRgb(hex);
  if (!rgb) return hex;

  const adjust = (value: number) => {
    const adjusted = value + (percent / 100) * 255;
    return Math.max(0, Math.min(255, Math.round(adjusted)));
  };

  return `#${adjust(rgb.r).toString(16).padStart(2, '0')}${adjust(rgb.g).toString(16).padStart(2, '0')}${adjust(rgb.b).toString(16).padStart(2, '0')}`;
}

// 加载主题色
async function loadThemeColor() {
  try {
    const color = await invoke<string>('get_theme_color');
    setThemeColor(color || defaultThemeColor);
  } catch (error) {
    console.error('加载主题色失败:', error);
    setThemeColor(defaultThemeColor);
  }
}

function getResolvedTheme(mode: ThemeMode): ResolvedTheme {
  if (mode === 'dark') return 'dark';
  if (mode === 'light') return 'light';
  return window.matchMedia(darkThemeMediaQuery).matches ? 'dark' : 'light';
}

function applyResolvedTheme(mode: ThemeMode) {
  const resolvedTheme = getResolvedTheme(mode);
  const root = document.documentElement;
  const isDark = resolvedTheme === 'dark';

  root.dataset.themeMode = mode;
  root.dataset.themeResolved = resolvedTheme;
  root.style.colorScheme = resolvedTheme;
  root.classList.toggle('dark', isDark);
  claudeStore.setDarkMode(isDark);
}

function convertSelectorForForcedDarkMode(selector: string): string {
  const trimmedSelector = selector.trim();
  if (!trimmedSelector) return trimmedSelector;
  if (trimmedSelector.includes(':root')) {
    return trimmedSelector.replace(/:root/g, 'html[data-theme-resolved="dark"]');
  }
  return `html[data-theme-resolved="dark"] ${trimmedSelector}`;
}

function convertDarkRuleForForcedMode(rule: CSSRule): string {
  if (rule.type !== CSSRule.STYLE_RULE) {
    return rule.cssText;
  }

  const styleRule = rule as CSSStyleRule;
  const convertedSelector = styleRule.selectorText
    .split(',')
    .map(convertSelectorForForcedDarkMode)
    .join(', ');

  return `${convertedSelector} { ${styleRule.style.cssText} }`;
}

function getMediaRuleConditionText(rule: CSSRule): string | null {
  if (rule.type !== CSSRule.MEDIA_RULE) return null;

  const mediaRule = rule as CSSMediaRule & {
    conditionText?: string;
    media?: MediaList & { mediaText?: string };
  };

  if (typeof mediaRule.conditionText === 'string') {
    return mediaRule.conditionText;
  }

  if (typeof mediaRule.media?.mediaText === 'string') {
    return mediaRule.media.mediaText;
  }

  return null;
}

function installForcedDarkThemeOverrides() {
  if (document.getElementById(forcedDarkThemeStyleId)) return;

  const convertedRules: string[] = [];

  for (const sheet of Array.from(document.styleSheets)) {
    let rules: CSSRuleList;
    try {
      rules = sheet.cssRules;
    } catch (error) {
      console.warn('读取样式表失败，跳过主题接管:', error);
      continue;
    }

    for (let index = rules.length - 1; index >= 0; index -= 1) {
      const rule = rules[index];
      const conditionText = getMediaRuleConditionText(rule);
      if (!conditionText?.includes(darkThemeMediaQuery)) continue;

      const mediaRule = rule as CSSMediaRule;

      convertedRules.push(
        Array.from(mediaRule.cssRules)
          .map(convertDarkRuleForForcedMode)
          .join('\n')
      );

      try {
        sheet.deleteRule(index);
      } catch (error) {
        console.warn('移除原始深色媒体规则失败:', error);
      }
    }
  }

  if (convertedRules.length === 0) return;

  const style = document.createElement('style');
  style.id = forcedDarkThemeStyleId;
  style.textContent = convertedRules.join('\n');
  document.head.appendChild(style);
}

async function setThemeMode(mode: ThemeMode) {
  try {
    applyResolvedTheme(mode);
    await getCurrentWindow().setTheme(mode === 'system' ? null : mode);
  } catch (error) {
    console.error('设置主题模式失败:', error);
    applyResolvedTheme(mode);
  }
}

async function loadThemeMode() {
  try {
    const mode = await invoke<ThemeMode>('get_theme_mode');
    await setThemeMode(mode || 'system');
  } catch (error) {
    console.error('加载主题模式失败:', error);
    await setThemeMode('system');
  }
}

async function loadAppearanceConfig() {
  try {
    const [interfaceFontSize, chatFontSize] = await Promise.all([
      invoke<number>('get_interface_font_size'),
      invoke<number>('get_chat_font_size'),
    ]);
    const applied = applyAppearanceConfig({
      interfaceFontSize: interfaceFontSize ?? DEFAULT_INTERFACE_FONT_SIZE,
      chatFontSize: chatFontSize ?? DEFAULT_CHAT_FONT_SIZE,
    });
    currentChatFontSize = applied.chatFontSize ?? DEFAULT_CHAT_FONT_SIZE;
  } catch (error) {
    console.error('加载字号配置失败:', error);
    const applied = applyAppearanceConfig({
      interfaceFontSize: DEFAULT_INTERFACE_FONT_SIZE,
      chatFontSize: DEFAULT_CHAT_FONT_SIZE,
    });
    currentChatFontSize = applied.chatFontSize ?? DEFAULT_CHAT_FONT_SIZE;
  }
}

async function loadSetupGate() {
  try {
    const completed = await invoke<boolean>('get_setup_completed');
    setupGate.value = completed ? 'ready' : 'wizard';
  } catch (error) {
    console.error('加载启动引导状态失败:', error);
    setupGate.value = 'wizard';
  }
}

function showChatFontIndicator(size: number) {
  chatFontIndicatorSize.value = clampChatFontSize(size);
  chatFontIndicatorVisible.value = true;

  if (chatFontIndicatorTimer !== null) {
    window.clearTimeout(chatFontIndicatorTimer);
  }

  chatFontIndicatorTimer = window.setTimeout(() => {
    chatFontIndicatorVisible.value = false;
    chatFontIndicatorTimer = null;
  }, 1100);
}

async function updateChatFontSize(size: number, options?: { showIndicator?: boolean }) {
  const nextSize = clampChatFontSize(size);
  applyChatFontSize(nextSize);
  currentChatFontSize = nextSize;
  if (options?.showIndicator) {
    showChatFontIndicator(nextSize);
  }

  try {
    const savedSize = await invoke<number>('set_chat_font_size', { size: nextSize });
    currentChatFontSize = applyChatFontSize(savedSize);
    if (options?.showIndicator) {
      showChatFontIndicator(savedSize);
    }
  } catch (error) {
    console.error('保存对话内容字号失败:', error);
  }
}

function syncChatFontSize(event: Event) {
  currentChatFontSize = clampChatFontSize((event as CustomEvent<number>).detail);
}

async function handleChatFontShortcut(event: KeyboardEvent) {
  const hasSupportedModifier = (isWindows && event.ctrlKey && !event.metaKey)
    || (isMacOS && event.metaKey && !event.ctrlKey);

  if (!hasSupportedModifier || event.altKey) return;

  const isIncrease = event.code === 'NumpadAdd'
    || event.code === 'Equal'
    || event.key === '+'
    || event.key === '=';
  const isDecrease = event.code === 'NumpadSubtract'
    || event.code === 'Minus'
    || event.key === '-'
    || event.key === '_';

  if (isIncrease || isDecrease) {
    console.debug('[Shortcut] chat font keydown', {
      key: event.key,
      code: event.code,
      metaKey: event.metaKey,
      ctrlKey: event.ctrlKey,
      shiftKey: event.shiftKey,
      altKey: event.altKey,
      isIncrease,
      isDecrease,
    });
  }

  if (!isIncrease && !isDecrease) return;

  event.preventDefault();
  event.stopPropagation();
  console.debug('[Shortcut] chat font resize', {
    direction: isIncrease ? 'increase' : 'decrease',
    from: currentChatFontSize,
    to: currentChatFontSize + (isIncrease ? 1 : -1),
  });
  await updateChatFontSize(currentChatFontSize + (isIncrease ? 1 : -1), { showIndicator: true });
}

let removeSystemThemeListener: (() => void) | null = null;
let removeChatFontShortcutListener: (() => void) | null = null;

// 应用启动时预热统计数据缓存和加载主题色
onMounted(async () => {
  installForcedDarkThemeOverrides();
  statsStore.warmup();
  await Promise.all([
    loadAppearanceConfig(),
    loadThemeMode(),
    loadThemeColor(),
    loadSetupGate(),
  ]);

  const mediaQuery = window.matchMedia(darkThemeMediaQuery);
  const handleSystemThemeChange = () => {
    if ((document.documentElement.dataset.themeMode || 'system') !== 'system') return;
    applyResolvedTheme('system');
  };

  mediaQuery.addEventListener('change', handleSystemThemeChange);
  removeSystemThemeListener = () => {
    mediaQuery.removeEventListener('change', handleSystemThemeChange);
  };

  window.addEventListener('keydown', handleChatFontShortcut, chatFontShortcutEventOptions);
  window.addEventListener(CHAT_FONT_SIZE_EVENT, syncChatFontSize as EventListener);
  removeChatFontShortcutListener = () => {
    window.removeEventListener('keydown', handleChatFontShortcut, chatFontShortcutEventOptions);
    window.removeEventListener(CHAT_FONT_SIZE_EVENT, syncChatFontSize as EventListener);
  };
});

// 暴露给全局以便其他组件调用
(window as any).setThemeColor = setThemeColor;
(window as any).setThemeMode = setThemeMode;
(window as any).setInterfaceFontSize = applyInterfaceFontSize;
(window as any).setChatFontSize = (size: number) => {
  currentChatFontSize = applyChatFontSize(size);
  return currentChatFontSize;
};
(window as any).updateChatFontSize = updateChatFontSize;

onUnmounted(() => {
  removeSystemThemeListener?.();
  removeChatFontShortcutListener?.();
  if (chatFontIndicatorTimer !== null) {
    window.clearTimeout(chatFontIndicatorTimer);
  }
});

function handleSetupComplete() {
  setupGate.value = 'ready';
}
</script>

<template>
  <div class="app-container">
    <transition name="chat-font-indicator">
      <div v-if="chatFontIndicatorVisible" class="chat-font-indicator">
        对话字号 {{ chatFontIndicatorSize }}px
      </div>
    </transition>
    <main class="workspace">
      <SetupOnboarding v-if="setupGate === 'wizard'" @complete="handleSetupComplete" />
      <Projects v-else-if="setupGate === 'ready'" />
      <div v-else class="boot-screen">
        <section class="boot-card">
          <div class="boot-badge">Aite Launch</div>
          <h2 class="boot-title">正在整理你的工作台</h2>
          <p class="boot-description">
            应用会先恢复界面偏好、读取本地启动配置，并判断这台电脑是否需要先走环境引导。
          </p>

          <div class="boot-activity">
            <span class="boot-sigil">/</span>
            <span class="boot-phase">读取本地配置与启动状态</span>
          </div>

          <div class="boot-progress-track" aria-hidden="true">
            <div class="boot-progress-indicator" />
          </div>

          <div class="boot-highlight-list">
            <div v-for="item in bootHighlights" :key="item" class="boot-highlight-item">
              <span class="boot-highlight-bar" />
              <span>{{ item }}</span>
            </div>
          </div>
        </section>
      </div>
    </main>
  </div>
</template>

<style scoped>
.app-container {
  display: flex;
  width: 100vw;
  height: 100vh;
  overflow: hidden;
  background-color: var(--bg-primary, #ffffff);
}

.workspace {
  flex: 1;
  height: 100%;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}

.chat-font-indicator {
  position: fixed;
  top: 1rem;
  left: 50%;
  transform: translateX(-50%);
  z-index: 1200;
  padding: 0.48rem 0.8rem;
  border-radius: 0.8rem;
  border: 1px solid rgba(113, 89, 59, 0.18);
  background: rgba(252, 247, 239, 0.94);
  color: #5b4630;
  box-shadow: 0 10px 30px rgba(31, 24, 16, 0.16);
  backdrop-filter: blur(12px);
  font-size: 0.78rem;
  font-weight: 600;
  letter-spacing: 0.01em;
}

.chat-font-indicator-enter-active,
.chat-font-indicator-leave-active {
  transition: opacity 0.18s ease, transform 0.18s ease;
}

.chat-font-indicator-enter-from,
.chat-font-indicator-leave-to {
  opacity: 0;
  transform: translateX(-50%) translateY(-8px);
}

.boot-screen {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 28px;
  background:
    radial-gradient(circle at top left, rgba(var(--primary-color-rgb), 0.16), transparent 32%),
    linear-gradient(145deg, #fcfaf4 0%, #f7f2e8 48%, #f6f8fb 100%);
}

:global(:root[data-theme-resolved='dark']) .boot-screen {
  background:
    radial-gradient(circle at top left, rgba(var(--primary-color-rgb), 0.22), transparent 32%),
    linear-gradient(145deg, #16181f 0%, #101722 52%, #0d1420 100%);
}

.boot-card {
  width: min(100%, 560px);
  padding: 28px;
  border-radius: 28px;
  background:
    linear-gradient(180deg, rgba(255, 255, 255, 0.82), rgba(255, 255, 255, 0.68)),
    repeating-linear-gradient(
      90deg,
      rgba(15, 23, 42, 0.03) 0,
      rgba(15, 23, 42, 0.03) 1px,
      transparent 1px,
      transparent 18px
    );
  border: 1px solid rgba(15, 23, 42, 0.08);
  box-shadow: 0 28px 60px rgba(15, 23, 42, 0.1);
  backdrop-filter: blur(16px);
}

:global(:root[data-theme-resolved='dark']) .boot-card {
  background:
    linear-gradient(180deg, rgba(17, 24, 39, 0.8), rgba(17, 24, 39, 0.62)),
    repeating-linear-gradient(
      90deg,
      rgba(255, 255, 255, 0.04) 0,
      rgba(255, 255, 255, 0.04) 1px,
      transparent 1px,
      transparent 18px
    );
  border-color: rgba(255, 255, 255, 0.08);
  box-shadow: 0 28px 60px rgba(0, 0, 0, 0.24);
}

.boot-badge {
  display: inline-flex;
  align-items: center;
  padding: 8px 14px;
  border-radius: 999px;
  background: rgba(var(--primary-color-rgb), 0.12);
  color: var(--text-primary);
  font-size: 12px;
  letter-spacing: 0.18em;
  text-transform: uppercase;
  font-weight: 700;
}

.boot-title {
  margin-top: 20px;
  color: var(--text-primary);
  font-size: clamp(28px, 4vw, 38px);
  line-height: 1.05;
  font-family: 'Iowan Old Style', 'Palatino Linotype', 'Book Antiqua', Georgia, serif;
  font-weight: 700;
}

.boot-description {
  margin-top: 14px;
  color: var(--text-secondary);
  font-size: 15px;
  line-height: 1.8;
}

.boot-activity {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-top: 24px;
  color: var(--text-primary);
}

.boot-sigil {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border-radius: 10px;
  background: rgba(var(--primary-color-rgb), 0.12);
  color: var(--primary-color, #3b82f6);
  font-size: 15px;
  font-weight: 800;
  animation: boot-pulse 1.5s ease-in-out infinite;
}

.boot-phase {
  font-size: 14px;
  font-weight: 600;
}

.boot-progress-track {
  position: relative;
  height: 10px;
  margin-top: 16px;
  border-radius: 999px;
  background: rgba(15, 23, 42, 0.08);
  overflow: hidden;
}

:global(:root[data-theme-resolved='dark']) .boot-progress-track {
  background: rgba(255, 255, 255, 0.08);
}

.boot-progress-indicator {
  position: absolute;
  inset: 0 auto 0 0;
  width: 34%;
  border-radius: 999px;
  background: linear-gradient(90deg, #f59e0b 0%, #f97316 56%, #fb7185 100%);
  animation: boot-progress 1.45s ease-in-out infinite;
}

.boot-highlight-list {
  display: grid;
  gap: 12px;
  margin-top: 22px;
}

.boot-highlight-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 14px;
  border-radius: 16px;
  background: rgba(255, 255, 255, 0.56);
  border: 1px solid rgba(15, 23, 42, 0.08);
  color: var(--text-secondary);
  font-size: 13px;
}

:global(:root[data-theme-resolved='dark']) .boot-highlight-item {
  background: rgba(255, 255, 255, 0.04);
  border-color: rgba(255, 255, 255, 0.08);
}

.boot-highlight-bar {
  width: 10px;
  height: 10px;
  border-radius: 999px;
  background: linear-gradient(135deg, #f59e0b 0%, #f97316 100%);
  box-shadow: 0 0 0 5px rgba(249, 115, 22, 0.14);
}

@keyframes boot-pulse {
  0%,
  100% {
    opacity: 1;
    transform: scale(1);
  }
  50% {
    opacity: 0.5;
    transform: scale(0.94);
  }
}

@keyframes boot-progress {
  0% {
    transform: translateX(-110%);
  }
  100% {
    transform: translateX(320%);
  }
}

@media (max-width: 720px) {
  .boot-screen {
    padding: 14px;
  }

  .boot-card {
    padding: 22px;
    border-radius: 22px;
  }
}
</style>

<style>
/* 全局 CSS 变量 */
:root {
  /* 颜色系统 */
  --primary-color: #3b82f6;
  --primary-hover: #2563eb;
  --primary-bg: #3b82f6;
  --color-primary: #3b82f6;

  /* 主题色透明度版本（由 JS 动态设置） */
  --primary-color-rgb: 59, 130, 246;
  --primary-color-10: rgba(59, 130, 246, 0.1);
  --primary-color-15: rgba(59, 130, 246, 0.15);
  --primary-color-20: rgba(59, 130, 246, 0.2);
  --primary-color-25: rgba(59, 130, 246, 0.25);
  --primary-color-30: rgba(59, 130, 246, 0.3);
  --primary-color-40: rgba(59, 130, 246, 0.4);

  --success-color: #10b981;
  --error-color: #ef4444;
  --error-hover: #dc2626;
  --error-bg: #fef2f2;
  --error-border: #fecaca;

  --warning-color: #f59e0b;

  /* 文本颜色 */
  --text-primary: #1f2937;
  --text-secondary: #6b7280;
  --text-muted: #9ca3af;

  /* 背景颜色 */
  --bg-primary: #ffffff;
  --bg-secondary: #f9fafb;
  --bg-tertiary: #f3f4f6;

  /* 边框颜色 */
  --border-color: #e5e7eb;

  /* 侧边栏 */
  --sidebar-bg: #1f2937;
  --sidebar-text: #ffffff;
  --sidebar-text-secondary: #d1d5db;
  --sidebar-hover: #374151;

  --interface-font-size-px: 16px;
  --chat-font-size-px: 14px;
}

:root[data-theme-resolved='dark'] {
  --text-primary: #f9fafb;
  --text-secondary: #d1d5db;
  --text-muted: #9ca3af;

  --bg-primary: #111827;
  --bg-secondary: #1f2937;
  --bg-tertiary: #374151;

  --border-color: #4b5563;

  --header-bg: #0f172a;
  --footer-bg: #1f2937;
}

/* 全局重置和基础样式 */
* {
  margin: 0;
  box-sizing: border-box;
}

html,
body,
#app {
  height: 100%;
  width: 100%;
  overflow: hidden;
}

html {
  font-size: var(--interface-font-size-px, 16px);
}

body {
  font-family: Inter, -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto,
    'Helvetica Neue', Arial, sans-serif;
  font-size: 0.875rem;
  line-height: 1.5;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
}

/* 滚动条样式 */
::-webkit-scrollbar {
  width: 8px;
  height: 8px;
}

::-webkit-scrollbar-track {
  background: var(--bg-secondary);
}

::-webkit-scrollbar-thumb {
  background: var(--border-color);
  border-radius: 4px;
}

::-webkit-scrollbar-thumb:hover {
  background: var(--text-muted);
}
</style>
