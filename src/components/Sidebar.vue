<script setup lang="ts">
import { ref } from 'vue';
import { HugeiconsIcon } from '@hugeicons/vue';
import {
  FolderIcon,
  ChartIcon,
  Plug01Icon,
  Settings01Icon,
  RobotIcon,
} from '@hugeicons/core-free-icons';

export type MenuItem = 'projects' | 'stats' | 'extensions' | 'settings';

const activeItem = ref<MenuItem>('projects');
const isCollapsed = ref(false);

const emit = defineEmits(['change']);

const menuItems = [
  { id: 'projects' as MenuItem, label: '项目', icon: FolderIcon },
  { id: 'stats' as MenuItem, label: '统计', icon: ChartIcon },
  { id: 'extensions' as MenuItem, label: '扩展', icon: Plug01Icon },
  { id: 'settings' as MenuItem, label: '设置', icon: Settings01Icon },
];

const selectItem = (item: MenuItem) => {
  activeItem.value = item;
  emit('change', item);
};

const toggleCollapse = () => {
  isCollapsed.value = !isCollapsed.value;
};
</script>

<template>
  <aside class="sidebar" :class="{ collapsed: isCollapsed }">
    <div class="sidebar-header">
      <div class="header-left">
        <HugeiconsIcon :icon="RobotIcon" class="app-logo" />
        <h2 v-if="!isCollapsed" class="app-name">Claude</h2>
      </div>
    </div>

    <nav class="sidebar-nav">
      <!-- 折叠/展开按钮 -->
      <button
        class="nav-collapse-btn"
        @click="toggleCollapse"
        :title="isCollapsed ? '展开侧边栏' : '折叠侧边栏'"
      >
        <svg v-if="!isCollapsed" width="18" height="18" viewBox="0 0 16 16">
          <path d="M10 3L6 8L10 13" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" fill="none"/>
        </svg>
        <svg v-else width="18" height="18" viewBox="0 0 16 16">
          <path d="M6 3L10 8L6 13" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" fill="none"/>
        </svg>
      </button>

      <button
        v-for="item in menuItems"
        :key="item.id"
        :class="['nav-item', { active: activeItem === item.id }]"
        @click="selectItem(item.id)"
        :title="isCollapsed ? item.label : ''"
      >
        <HugeiconsIcon :icon="item.icon" class="nav-icon" />
        <span v-if="!isCollapsed" class="nav-label">{{ item.label }}</span>
      </button>
    </nav>
  </aside>
</template>

<style scoped>
.sidebar {
  width: 100px;
  height: 100%;
  background-color: #fafafa;
  color: var(--text-secondary, #6b7280);
  display: flex;
  flex-direction: column;
  border-right: 1px solid var(--border-color, #e5e7eb);
  transition: width 0.2s ease;
}

.sidebar.collapsed {
  width: 56px;
}

.sidebar-header {
  padding: 0.75rem;
  border-bottom: 1px solid var(--border-color, #e5e7eb);
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 48px;
}

.header-left {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  justify-content: center;
}

.app-logo {
  width: 1.5rem;
  height: 1.5rem;
  flex-shrink: 0;
  color: var(--primary-color, #3b82f6);
}

.app-name {
  margin: 0;
  font-size: 0.75rem;
  font-weight: 600;
  color: var(--text-primary, #1f2937);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.sidebar.collapsed .app-name {
  display: none;
}

.sidebar-nav {
  flex: 1;
  padding: 1rem 0.5rem;
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.nav-item {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.625rem;
  background: transparent;
  border: none;
  border-radius: 0.5rem;
  color: var(--text-secondary, #6b7280);
  cursor: pointer;
  transition: all 0.2s ease;
  text-align: left;
  width: 100%;
  justify-content: center;
  -webkit-app-region: no-drag;
  app-region: no-drag;
}

.sidebar:not(.collapsed) .nav-item {
  justify-content: flex-start;
  padding: 0.75rem;
}

.nav-item:hover {
  background-color: #e5e7eb;
  color: var(--text-primary, #1f2937);
}

.nav-item.active {
  background-color: #ffffff;
  color: var(--primary-color, #3b82f6);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
}

.nav-icon {
  width: 1.25rem;
  height: 1.25rem;
  flex-shrink: 0;
}

.nav-label {
  font-size: 0.875rem;
  font-weight: 500;
  line-height: 1;
  white-space: nowrap;
}

/* 折叠按钮（放在导航区域顶部） */
.nav-collapse-btn {
  display: flex;
  align-items: center;
  padding: 0.5rem;
  background: transparent;
  border: none;
  border-radius: 0.5rem;
  color: var(--text-secondary, #6b7280);
  cursor: pointer;
  transition: all 0.2s ease;
  align-self: flex-start;
  width: auto;
  margin-bottom: 0.5rem;
  -webkit-app-region: no-drag;
  app-region: no-drag;
}

.nav-collapse-btn:hover {
  background-color: #e5e7eb;
  color: var(--text-primary, #1f2937);
}

/* 深色模式 */
@media (prefers-color-scheme: dark) {
  .sidebar {
    background-color: #1f2937;
    border-right-color: #374151;
  }

  .app-name {
    color: var(--text-primary, #f9fafb);
  }

  .nav-item {
    color: var(--text-secondary, #9ca3af);
  }

  .nav-item:hover {
    background-color: #374151;
    color: var(--text-primary, #f9fafb);
  }

  .nav-item.active {
    background-color: #374151;
    color: var(--primary-color, #3b82f6);
  }

  .sidebar-header {
    border-bottom-color: #374151;
  }

  .nav-collapse-btn:hover {
    background-color: #374151;
    color: var(--text-primary, #f9fafb);
  }
}
</style>
