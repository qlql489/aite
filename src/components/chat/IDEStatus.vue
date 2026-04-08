<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useIDEStore } from '../../stores/ide';

const ideStore = useIDEStore();

const containerRef = ref<HTMLElement | null>(null);
const ideMenuOpen = ref(false);

const hasMultipleIDEs = computed(() => ideStore.availableIDEs.length > 1);
const shouldRender = computed(() => ideStore.availableIDEs.length > 0 || Boolean(ideStore.connectedIde));
const activeIdeKey = computed(() => ideStore.connectedIde?.key || ideStore.selectedIdeKey || '');

const connectionTone = computed(() => {
  if (ideStore.connectionState === 'connected') return 'connected';
  if (ideStore.connectionState === 'connecting') return 'connecting';
  if (ideStore.connectionState === 'error') return 'error';
  return 'idle';
});

const triggerLabel = computed(() => {
  if (ideStore.connectedIde) return ideStore.connectedIde.name;
  if (ideStore.availableIDEs.length === 1) return ideStore.availableIDEs[0].name;
  if (ideStore.connectionState === 'connecting') return 'IDE 连接中…';
  if (ideStore.connectionState === 'error') return '连接 IDE';
  if (ideStore.isScanning) return '扫描 IDE…';
  return '选择 IDE';
});

const triggerTitle = computed(() => {
  if (ideStore.connectedIde) {
    return `${ideStore.connectedIde.name} · ${ideStore.connectedIde.port}`;
  }
  if (ideStore.connectionState === 'connecting') return 'IDE 连接中…';
  if (ideStore.connectionState === 'error') return ideStore.error || 'IDE 连接失败';
  if (ideStore.isScanning) return '正在扫描可用 IDE';
  if (ideStore.availableIDEs.length === 1) {
    const ide = ideStore.availableIDEs[0];
    return `${ide.name} · ${ide.port}`;
  }
  return '选择 IDE';
});

const selectionLabel = computed(() => {
  const currentSelection = ideStore.selection;
  if (!currentSelection) return '';

  const fileName = currentSelection.filePath
    ? currentSelection.filePath.split(/[\\/]/).pop() || currentSelection.filePath
    : '';

  if (currentSelection.startLine && currentSelection.endLine) {
    const lineLabel = currentSelection.startLine === currentSelection.endLine
      ? `L${currentSelection.startLine}`
      : `L${currentSelection.startLine}-L${currentSelection.endLine}`;
    return fileName ? `${fileName} · ${lineLabel}` : lineLabel;
  }

  if (currentSelection.lineCount) {
    return fileName
      ? `${fileName} · 已选 ${currentSelection.lineCount} 行`
      : `已选 ${currentSelection.lineCount} 行`;
  }

  return fileName;
});

const selectionToggleTitle = computed(() => {
  if (!selectionLabel.value) return '';
  return ideStore.includeSelectionInContext
    ? '当前发送会带上 IDE 选中内容，点击后改为不带'
    : '当前发送不带 IDE 选中内容，点击后改为带上';
});

const toggleMenu = () => {
  if (!hasMultipleIDEs.value) return;
  ideMenuOpen.value = !ideMenuOpen.value;
};

const toggleSelectionContext = () => {
  const nextValue = !ideStore.includeSelectionInContext;
  const maybeSetter = (ideStore as typeof ideStore & {
    setIncludeSelectionInContext?: (enabled: boolean) => void;
  }).setIncludeSelectionInContext;

  if (typeof maybeSetter === 'function') {
    maybeSetter(nextValue);
    return;
  }

  ideStore.includeSelectionInContext = nextValue;
};

const selectIde = async (ideKey: string) => {
  const ide = ideStore.availableIDEs.find((item) => item.key === ideKey);
  if (!ide) return;
  ideMenuOpen.value = false;
  await ideStore.connectToIDE(ide, { rememberSelection: true });
};

const onDocumentClick = (event: MouseEvent) => {
  const target = event.target as Node | null;
  if (!target || !containerRef.value?.contains(target)) {
    ideMenuOpen.value = false;
  }
};

watch(hasMultipleIDEs, (value) => {
  if (!value) ideMenuOpen.value = false;
});

onMounted(() => {
  document.addEventListener('mousedown', onDocumentClick);
});

onBeforeUnmount(() => {
  document.removeEventListener('mousedown', onDocumentClick);
});
</script>

<template>
  <div v-if="shouldRender" ref="containerRef" class="ide-status-shell">
    <button
      v-if="hasMultipleIDEs"
      class="ide-trigger"
      :class="connectionTone"
      type="button"
      :title="triggerTitle"
      @click="toggleMenu"
    >
      <span class="ide-trigger-main">
        <span class="ide-chip-icon" aria-hidden="true">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none">
            <path d="M4 5.5A2.5 2.5 0 0 1 6.5 3h11A2.5 2.5 0 0 1 20 5.5v8A2.5 2.5 0 0 1 17.5 16h-4.25l-2.1 2.55a.5.5 0 0 1-.77 0L8.25 16H6.5A2.5 2.5 0 0 1 4 13.5v-8Z" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round" />
            <path d="M9 8.5 7.5 10 9 11.5" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />
            <path d="M15 8.5 16.5 10 15 11.5" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />
            <path d="M12.5 8 11.5 12" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" />
          </svg>
        </span>
        <span class="ide-chip-label">{{ triggerLabel }}</span>
      </span>
      <span class="ide-trigger-tail">
        <span class="ide-state-dot" :class="connectionTone" aria-hidden="true"></span>
        <svg class="chevron" width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
          <path d="M3.5 5.25L7 8.75L10.5 5.25" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
      </span>
    </button>

    <div
      v-else
      class="ide-trigger ide-trigger-static"
      :class="connectionTone"
      :title="triggerTitle"
    >
      <span class="ide-trigger-main">
        <span class="ide-chip-icon" aria-hidden="true">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none">
            <path d="M4 5.5A2.5 2.5 0 0 1 6.5 3h11A2.5 2.5 0 0 1 20 5.5v8A2.5 2.5 0 0 1 17.5 16h-4.25l-2.1 2.55a.5.5 0 0 1-.77 0L8.25 16H6.5A2.5 2.5 0 0 1 4 13.5v-8Z" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round" />
            <path d="M9 8.5 7.5 10 9 11.5" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />
            <path d="M15 8.5 16.5 10 15 11.5" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />
            <path d="M12.5 8 11.5 12" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" />
          </svg>
        </span>
        <span class="ide-chip-label">{{ triggerLabel }}</span>
      </span>
      <span class="ide-state-dot" :class="connectionTone" aria-hidden="true"></span>
    </div>

    <div v-if="ideMenuOpen" class="ide-menu">
      <div class="ide-section-title">可连接 IDE</div>

      <div class="ide-list">
        <button
          v-for="ide in ideStore.availableIDEs"
          :key="ide.key"
          class="ide-option"
          :class="{ active: ide.key === activeIdeKey }"
          type="button"
          @click="selectIde(ide.key)"
        >
          <span class="ide-option-main">
            <span class="ide-option-name">{{ ide.name }}</span>
            <span class="ide-option-port">:{{ ide.port }}</span>
          </span>
          <span v-if="ide.key === activeIdeKey" class="ide-check">✓</span>
        </button>
      </div>
    </div>

    <button
      v-if="selectionLabel"
      class="ide-selection-toggle"
      :class="{ inactive: !ideStore.includeSelectionInContext }"
      type="button"
      :title="selectionToggleTitle"
      @mousedown.stop.prevent="toggleSelectionContext"
      @click.stop.prevent
    >
      <span class="ide-selection-icon" aria-hidden="true">
        <svg v-if="ideStore.includeSelectionInContext" width="15" height="15" viewBox="0 0 24 24" fill="none">
          <path
            d="M2.75 12C4.75 8.55 8.02 6.75 12 6.75C15.98 6.75 19.25 8.55 21.25 12C19.25 15.45 15.98 17.25 12 17.25C8.02 17.25 4.75 15.45 2.75 12Z"
            stroke="currentColor"
            stroke-width="1.9"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
          <circle cx="12" cy="12" r="2.75" stroke="currentColor" stroke-width="1.9" />
        </svg>
        <svg v-else width="15" height="15" viewBox="0 0 24 24" fill="none">
          <path
            d="M2.75 12C4.75 8.55 8.02 6.75 12 6.75C15.98 6.75 19.25 8.55 21.25 12C19.25 15.45 15.98 17.25 12 17.25C8.02 17.25 4.75 15.45 2.75 12Z"
            stroke="currentColor"
            stroke-width="1.9"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
          <circle cx="12" cy="12" r="2.75" stroke="currentColor" stroke-width="1.9" />
          <path d="M4.5 19L19.5 5" stroke="currentColor" stroke-width="2.1" stroke-linecap="round" />
        </svg>
      </span>
      <span
        class="ide-selection-text"
        :title="ideStore.selection?.filePath || selectionLabel"
      >
        {{ selectionLabel }}
      </span>
    </button>

    <span v-else-if="ideStore.error" class="ide-error-text" :title="ideStore.error">
      {{ ideStore.error }}
    </span>
  </div>
</template>

<style scoped>
.ide-status-shell {
  position: relative;
  display: inline-flex;
  align-items: center;
  gap: 0.55rem;
  min-width: 0;
}

.ide-trigger {
  border: none;
  background: #f3f3f3;
  color: #6d6d6d;
  border-radius: 999px;
  box-sizing: border-box;
  display: inline-flex;
  align-items: center;
  gap: 0.5rem;
  min-height: 25px;
  padding: 0.34rem 0.68rem;
  font-size: 11px;
  font-weight: 500;
  cursor: pointer;
  flex-shrink: 0;
  transition: background-color 0.18s ease, color 0.18s ease;
}

.ide-trigger:hover {
  background: #ebebeb;
  color: #202020;
}

.ide-trigger.connected {
  background: rgba(240, 253, 244, 0.96);
  color: #15803d;
}

.ide-trigger.connected:hover {
  background: rgba(220, 252, 231, 0.98);
  color: #166534;
}

.ide-trigger-static {
  cursor: default;
  padding-right: 0.62rem;
}

.ide-trigger-static:hover {
  background: #f3f3f3;
  color: #6d6d6d;
}

.ide-trigger-main,
.ide-trigger-tail,
.ide-option-main {
  display: inline-flex;
  align-items: center;
}

.ide-trigger-main {
  gap: 0.45rem;
  min-width: 0;
}

.ide-trigger-tail {
  gap: 0.35rem;
  flex-shrink: 0;
}

.ide-chip-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}

.ide-chip-label {
  min-width: 0;
  max-width: 180px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  line-height: 1.2;
}

.ide-state-dot {
  width: 7px;
  height: 7px;
  border-radius: 999px;
  background: #a3a3a3;
}

.ide-state-dot.connected {
  background: #23a55a;
}

.ide-state-dot.connecting {
  background: #3b82f6;
  box-shadow: 0 0 0 3px rgba(59, 130, 246, 0.14);
}

.ide-state-dot.error {
  background: #ef4444;
}

.ide-menu {
  position: absolute;
  left: 0;
  bottom: calc(100% + 0.55rem);
  z-index: 30;
  width: 260px;
  max-width: min(420px, calc(100vw - 32px));
  background: #ffffff;
  border-radius: 12px;
  box-shadow: 0 20px 40px rgba(15, 23, 42, 0.16);
  padding: 8px 8px 6px;
}

.ide-section-title {
  padding: 2px 6px 8px;
  font-size: 14px;
  font-weight: 500;
  color: #747474;
}

.ide-list {
  max-height: 220px;
  overflow-y: auto;
  padding-right: 4px;
}

.ide-option {
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

.ide-option-main {
  gap: 8px;
  min-width: 0;
}

.ide-option-name,
.ide-option-port {
  white-space: nowrap;
}

.ide-option-name {
  overflow: hidden;
  text-overflow: ellipsis;
}

.ide-option-port {
  color: #8b8b8b;
}

.ide-check {
  font-size: 18px;
  line-height: 1;
}

.ide-selection-toggle {
  border: none;
  background: transparent;
  padding: 0.18rem 0.32rem;
  display: inline-flex;
  align-items: center;
  gap: 0.36rem;
  min-width: 0;
  min-height: 25px;
  border-radius: 0.45rem;
  cursor: pointer;
  color: #6b7280;
  pointer-events: auto;
  position: relative;
  z-index: 1;
  user-select: none;
  transition: color 0.18s ease, opacity 0.18s ease;
}

.ide-selection-toggle:hover {
  color: #374151;
  background: rgba(148, 163, 184, 0.12);
}

.ide-selection-toggle.inactive {
  color: #9ca3af;
}

.ide-selection-toggle.inactive:hover {
  color: #6b7280;
  background: rgba(148, 163, 184, 0.1);
}

.ide-selection-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}

.ide-selection-icon svg {
  display: block;
}

.ide-selection-text,
.ide-error-text {
  display: inline-flex;
  align-items: center;
  min-width: 0;
  max-width: min(38vw, 320px);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 11px;
  line-height: 1.2;
}

.ide-selection-text {
  color: inherit;
}

.ide-error-text {
  color: #dc2626;
}
</style>
