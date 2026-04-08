<script setup lang="ts">
import { computed, ref } from 'vue';
import { useClaudeStore } from '../../stores/claude';

const claudeStore = useClaudeStore();
const isOpen = ref(false);

const usage = computed(() => claudeStore.currentModelUsage);
const hasData = computed(() => usage.value !== null && (usage.value?.contextWindow || 0) > 0);

const usedTokens = computed(() => {
  if (!usage.value) return 0;
  return (
    (usage.value.inputTokens || 0)
    + (usage.value.cacheReadInputTokens || 0)
    + (usage.value.cacheCreationInputTokens || 0)
  );
});

const percentage = computed(() => {
  if (!usage.value?.contextWindow) return 0;
  return Math.min(100, Math.round((usedTokens.value / usage.value.contextWindow) * 100));
});

const precisePercentage = computed(() => {
  if (!usage.value?.contextWindow) return 0;
  return Math.min(100, (usedTokens.value / usage.value.contextWindow) * 100);
});

const remainingPercentage = computed(() => Math.max(0, 100 - precisePercentage.value));

const radius = 15;
const circumference = 2 * Math.PI * radius;

const strokeDashoffset = computed(() => {
  return circumference - (percentage.value / 100) * circumference;
});

const strokeColor = computed(() => {
  const progress = percentage.value;
  if (progress < 50) return '#8b5cf6';
  if (progress < 75) return '#f59e0b';
  if (progress < 90) return '#ef4444';
  return '#dc2626';
});

const formatExact = (num: number): string => new Intl.NumberFormat('zh-CN').format(num);

const formatPercentage = (num: number): string => {
  if (num >= 100) return '100';
  if (num <= 0) return '0';
  return num.toFixed(1).replace(/\.0$/, '');
};

const openCard = () => {
  if (hasData.value) isOpen.value = true;
};

const closeCard = () => {
  isOpen.value = false;
};
</script>

<template>
  <div
    v-if="hasData"
    class="token-indicator"
    @mouseenter="openCard"
    @mouseleave="closeCard"
  >
    <button
      type="button"
      class="indicator-button"
      :aria-expanded="isOpen"
      @click="isOpen = !isOpen"
    >
      <svg class="progress-ring" width="36" height="36" viewBox="0 0 36 36">
        <circle
          class="progress-ring__circle-bg"
          :r="radius"
          cx="18"
          cy="18"
        />
        <circle
          class="progress-ring__circle"
          :r="radius"
          cx="18"
          cy="18"
          :stroke="strokeColor"
          :stroke-dasharray="circumference"
          :stroke-dashoffset="strokeDashoffset"
        />
      </svg>
      <span class="progress-value">{{ percentage }}%</span>
    </button>

    <div v-if="isOpen" class="token-card">
      <div class="token-card-title">上下文窗口：</div>
      <div class="token-card-line">{{ formatPercentage(precisePercentage) }}% 已用（剩余 {{ formatPercentage(remainingPercentage) }}%）</div>
      <div class="token-card-line">已用 {{ formatExact(usedTokens) }} 标记，共 {{ formatExact(usage!.contextWindow) }}</div>
    </div>
  </div>
</template>

<style scoped>
.token-indicator {
  position: relative;
  display: inline-flex;
  align-items: center;
}

.indicator-button {
  position: relative;
  width: 36px;
  height: 36px;
  border: none;
  border-radius: 999px;
  background: transparent;
  padding: 0;
  cursor: pointer;
}

.progress-ring {
  transform: rotate(-90deg);
}

.progress-ring__circle-bg {
  fill: none;
  stroke: rgba(148, 163, 184, 0.24);
  stroke-width: 3;
}

.progress-ring__circle {
  fill: none;
  stroke-width: 3;
  stroke-linecap: round;
  transition: stroke-dashoffset 0.25s ease, stroke 0.25s ease;
}

.progress-value {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 9px;
  font-weight: 700;
  color: var(--text-secondary, #4b5563);
}

.token-card {
  position: absolute;
  right: 0;
  bottom: calc(100% + 0.55rem);
  min-width: 308px;
  padding: 0.8rem 1rem;
  border-radius: 1rem;
  border: 1px solid var(--border-color, #dcdfe4);
  background: color-mix(in srgb, var(--bg-primary, #ffffff) 96%, #f3f4f6 4%);
  box-shadow: 0 16px 36px rgba(15, 23, 42, 0.12);
  z-index: 120;
}

.token-card-title {
  margin-bottom: 0.2rem;
  text-align: center;
  font-size: 0.9rem;
  font-weight: 700;
  color: var(--text-muted, #9ca3af);
}

.token-card-line {
  text-align: center;
  font-size: 0.98rem;
  font-weight: 600;
  color: var(--text-primary, #111827);
  line-height: 1.45;
}

@media (prefers-color-scheme: dark) {
  .progress-ring__circle-bg {
    stroke: rgba(100, 116, 139, 0.35);
  }

  .progress-value {
    color: var(--text-secondary, #d1d5db);
  }

  .token-card {
    border-color: rgba(148, 163, 184, 0.24);
    background: color-mix(in srgb, var(--bg-primary, #111827) 94%, #1f2937 6%);
    box-shadow: 0 16px 36px rgba(0, 0, 0, 0.32);
  }

  .token-card-title {
    color: var(--text-muted, #9ca3af);
  }

  .token-card-line {
    color: var(--text-primary, #f9fafb);
  }
}
</style>
