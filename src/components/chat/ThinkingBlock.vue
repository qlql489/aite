<script setup lang="ts">
import { ref, computed } from 'vue';

interface Props {
  content: string;
  collapsed?: boolean;
  cornerStyle?: 'none' | 'top' | 'bottom' | 'all';
}

const props = withDefaults(defineProps<Props>(), {
  collapsed: false,
  cornerStyle: 'all',
});

const isOpen = ref(!props.collapsed);

const thinkingLines = computed(() => {
  return props.content.split('\n').filter(line => line.trim());
});

const previewText = computed(() => thinkingLines.value[0] || '');
</script>

<template>
  <div class="message-block thinking-block" :class="`corner-${cornerStyle}`">
    <button
      class="message-block-header thinking-header"
      :class="{ expanded: isOpen }"
      @click="isOpen = !isOpen"
    >
      <span class="thinking-icon">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M9 18h6"/>
          <path d="M10 21h4"/>
          <path d="M12 3a6 6 0 0 0-3.8 10.6c.5.4.8 1 .8 1.6V16a1 1 0 0 0 1 1h4a1 1 0 0 0 1-1v-.8c0-.6.3-1.2.8-1.6A6 6 0 0 0 12 3Z"/>
        </svg>
      </span>

      <div class="message-block-main">
        <span class="message-block-title thinking-title">思考</span>
        <span v-if="!isOpen && previewText" class="message-block-description thinking-description">{{ previewText }}</span>
      </div>

      <div class="message-block-status">
        <span :class="['expand-arrow', { expanded: isOpen }]">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <polyline points="6 9 12 15 18 9"/>
          </svg>
        </span>
      </div>
    </button>

    <div v-if="isOpen" class="message-block-content thinking-content">
      <div
        v-for="(line, index) in thinkingLines"
        :key="index"
        class="thinking-line"
      >
        {{ line }}
      </div>
    </div>
  </div>
</template>

<style scoped>
.message-block {
  margin: 0;
  border: 1px solid var(--border-color, #e5e7eb);
  overflow: hidden;
  background-color: var(--bg-secondary, #f9fafb);
}

.message-block.corner-none {
  border-radius: 0;
}

.message-block.corner-top {
  border-radius: 8px 8px 0 0;
}

.message-block.corner-bottom {
  border-radius: 0 0 8px 8px;
}

.message-block.corner-all {
  border-radius: 8px;
}

.message-block-header,
.thinking-header {
  display: flex;
  align-items: center;
  gap: 0.65rem;
  width: 100%;
  padding: 0.42rem 0.95rem;
  background: transparent;
  border: none;
  cursor: pointer;
  transition: background-color 0.15s;
}

.thinking-header:hover {
  background-color: rgba(148, 163, 184, 0.08);
}

.thinking-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 1rem;
  height: 1rem;
  color: var(--text-secondary, #6b7280);
  flex-shrink: 0;
}

.message-block-main {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  min-width: 0;
  flex: 1;
}

.message-block-title,
.thinking-title {
  font-size: 13px;
  font-weight: 500;
  color: var(--text-primary, #1f2937);
  white-space: nowrap;
  flex-shrink: 0;
}

.message-block-description,
.thinking-description {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 11px;
  color: var(--text-secondary, #6b7280);
}

.message-block-status {
  display: flex;
  align-items: center;
  flex-shrink: 0;
}

.expand-arrow {
  display: flex;
  align-items: center;
  color: var(--text-muted, #9ca3af);
  transition: transform 0.2s;
}

.expand-arrow.expanded {
  transform: rotate(180deg);
}

.thinking-content {
  padding: 0.2rem 0.95rem 0.45rem 2.6rem;
  border-top: 1px solid var(--border-color, #e5e7eb);
  background-color: var(--bg-tertiary, #f3f4f6);
}

.thinking-line {
  font-size: 11px;
  color: var(--text-secondary, #6b7280);
  line-height: 1.45;
  padding: 0.125rem 0;
}

.thinking-line:not(:last-child) {
  margin-bottom: 0.125rem;
}

@media (prefers-color-scheme: dark) {
  .message-block {
    background-color: var(--bg-secondary, #1f2937);
    border-color: var(--border-color, #374151);
  }

  .thinking-header:hover {
    background-color: rgba(75, 85, 99, 0.16);
  }

  .thinking-icon {
    color: var(--text-secondary, #9ca3af);
  }

  .thinking-title {
    color: var(--text-primary, #f9fafb);
  }

  .thinking-description,
  .thinking-line {
    color: var(--text-secondary, #9ca3af);
  }

  .thinking-content {
    background-color: var(--bg-tertiary, #374151);
    border-top-color: var(--border-color, #374151);
  }
}
</style>
