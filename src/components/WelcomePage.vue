<script setup lang="ts">
interface Props {
  hasProjects?: boolean;
}

defineProps<Props>();
</script>

<template>
  <div class="welcome-page">
    <div class="welcome-content">
      <!-- Logo/图标 -->
      <div class="welcome-logo">
        <img src="/app-icon.svg" alt="Aite 图标" />
      </div>

      <!-- 标题 -->
      <h1 class="welcome-title">欢迎使用 Aite</h1>

      <!-- 描述 -->
      <p class="welcome-description">
        选择一个项目中的对话记录开始聊天，或者创建一个新对话
      </p>

      <!-- 快捷操作提示 -->
      <div class="welcome-tips">
        <div class="tip-item">
          <svg width="20" height="20" viewBox="0 0 20 20" fill="currentColor">
            <path d="M10 2C5.58172 2 2 5.58172 2 10C2 14.4183 5.58172 18 10 18C14.4183 18 18 14.4183 18 10C18 5.58172 14.4183 2 10 2ZM10 16.5C6.41015 16.5 3.5 13.5899 3.5 10C3.5 6.41015 6.41015 3.5 10 3.5C13.5899 3.5 16.5 6.41015 16.5 10C16.5 13.5899 13.5899 16.5 10 16.5Z"/>
            <path d="M10 5V10.5H13.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
          </svg>
          <span>管理历史对话记录</span>
        </div>
        <div class="tip-item">
          <svg width="20" height="20" viewBox="0 0 20 20" fill="currentColor">
            <path d="M17 3H3C2.44772 3 2 3.44772 2 4V16C2 16.5523 2.44772 17 3 17H17C17.5523 17 18 16.5523 18 16V4C18 3.44772 17.5523 3 17 3ZM16.5 15.5H3.5V4.5H16.5V15.5Z"/>
            <path d="M7 8H13M7 11H10" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
          </svg>
          <span>支持多任务并行执行</span>
        </div>
        <div class="tip-item">
          <svg width="20" height="20" viewBox="0 0 20 20" fill="currentColor">
            <path d="M10 2L10 18M2 10L18 10" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
          </svg>
          <span>点击项目旁的 + 按钮创建新对话</span>
        </div>
      </div>

      <!-- 空状态提示 -->
      <div v-if="!hasProjects" class="empty-projects-tip">
        <p>还没有项目？点击左侧的"打开"或"导入"按钮添加项目</p>
      </div>
    </div>
  </div>
</template>

<style scoped>
.welcome-page {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
  width: 100%;
  background-color: var(--bg-primary, #ffffff);
}

.welcome-content {
  text-align: center;
  max-width: 480px;
  padding: 2rem;
}

.welcome-logo {
  margin-bottom: 1.5rem;
  display: flex;
  justify-content: center;
}

.welcome-logo img {
  width: 72px;
  height: 72px;
  display: block;
  filter: drop-shadow(0 12px 28px rgba(37, 99, 235, 0.18));
  animation: float 3s ease-in-out infinite;
}

@keyframes float {
  0%, 100% {
    transform: translateY(0);
  }
  50% {
    transform: translateY(-8px);
  }
}

.welcome-title {
  font-size: 28px;
  font-weight: 600;
  color: var(--text-primary, #1f2937);
  margin-bottom: 0.75rem;
}

.welcome-description {
  font-size: 15px;
  color: var(--text-secondary, #6b7280);
  margin-bottom: 2.5rem;
  line-height: 1.6;
}

.welcome-tips {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  text-align: left;
  margin-bottom: 2rem;
}

.tip-item {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  padding: 0.875rem 1rem;
  background-color: var(--bg-secondary, #f9fafb);
  border-radius: 10px;
  color: var(--text-secondary, #6b7280);
  font-size: 14px;
  transition: all 0.2s;
}

.tip-item:hover {
  background-color: var(--bg-tertiary, #f3f4f6);
  transform: translateX(4px);
}

.tip-item svg {
  color: var(--primary-color, #3b82f6);
  flex-shrink: 0;
}

.empty-projects-tip {
  padding: 1rem 1.25rem;
  background-color: rgba(59, 130, 246, 0.1);
  border-radius: 10px;
  color: var(--primary-color, #3b82f6);
  font-size: 14px;
}

.empty-projects-tip p {
  margin: 0;
}

/* 深色模式 */
@media (prefers-color-scheme: dark) {
  .welcome-title {
    color: var(--text-primary, #f9fafb);
  }

  .welcome-description {
    color: var(--text-secondary, #d1d5db);
  }

  .tip-item {
    background-color: var(--bg-secondary, #1f2937);
    color: var(--text-secondary, #d1d5db);
  }

  .tip-item:hover {
    background-color: var(--bg-tertiary, #374151);
  }

  .empty-projects-tip {
    background-color: rgba(59, 130, 246, 0.2);
  }
}
</style>
