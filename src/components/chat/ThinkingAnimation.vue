<script setup lang="ts">
/**
 * AI 执行中动画组件
 * 使用脉冲点动画，响应式适配深色/浅色模式
 */

import { ref, watch, onUnmounted } from 'vue';

interface Props {
  startedAt?: number | null;
  outputTokens?: number;
}

const props = withDefaults(defineProps<Props>(), {
  startedAt: null,
  outputTokens: 0,
});

const elapsed = ref(0);
let elapsedInterval: ReturnType<typeof setInterval> | null = null;

const stopElapsedTimer = () => {
  if (elapsedInterval) {
    clearInterval(elapsedInterval);
    elapsedInterval = null;
  }
};

const startElapsedTimer = (startedAt: number) => {
  stopElapsedTimer();
  elapsed.value = Math.max(0, Date.now() - startedAt);
  elapsedInterval = setInterval(() => {
    elapsed.value = Math.max(0, Date.now() - startedAt);
  }, 1000);
};

watch(() => props.startedAt, (startedAt) => {
  if (!startedAt) {
    stopElapsedTimer();
    elapsed.value = 0;
    return;
  }

  startElapsedTimer(startedAt);
}, { immediate: true });

onUnmounted(() => {
  stopElapsedTimer();
});

// 格式化时间
function formatElapsed(ms: number): string {
  const secs = Math.floor(ms / 1000);
  if (secs < 60) return `${secs}s`;
  const mins = Math.floor(secs / 60);
  return `${mins}m ${secs % 60}s`;
}

// 格式化 token 数
function formatTokens(n: number): string {
  if (n >= 1000) return `${(n / 1000).toFixed(1)}k`;
  return String(n);
}
</script>

<template>
  <div class="thinking-animation">
    <span class="streaming-dot"></span>
    <span>生成中...</span>
    <span v-if="elapsed > 0" class="separator">·</span>
    <span v-if="elapsed > 0">{{ formatElapsed(elapsed) }}</span>
    <span v-if="outputTokens && outputTokens > 0" class="separator">·</span>
    <span v-if="outputTokens && outputTokens > 0">↓ {{ formatTokens(outputTokens) }}</span>
  </div>
</template>

<style scoped>
.thinking-animation {
  display: flex;
  align-items: center;
  gap: 0.375rem;
  font-size: 11px;
  color: var(--text-muted, #9ca3af);
  font-family: 'Monaco', 'Menlo', monospace;
  user-select: none;
}

.streaming-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background-color: var(--primary-color, #3b82f6);
  animation: pulse 1.4s infinite ease-in-out;
}

@keyframes pulse {
  0%, 60%, 100% {
    opacity: 0.4;
    transform: scale(0.8);
  }
  30% {
    opacity: 1;
    transform: scale(1);
  }
}

.separator {
  opacity: 0.6;
}

/* 深色模式 */
@media (prefers-color-scheme: dark) {
  .thinking-animation {
    color: #9ca3af;
  }

  .streaming-dot {
    background-color: #60a5fa;
  }
}
</style>
