<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue';
import { useStatsStore } from '../stores/stats';
import type { DailyUsage, ProjectInfo } from '../types';

const statsStore = useStatsStore();

// 响应式数据
const currentPage = ref(1);
const pageSize = 10;
const sortBy = ref<'cost' | 'time'>('cost');

// 项目搜索状态
const projectSearchQuery = ref('');
const showProjectDropdown = ref(false);

// Tooltip 状态
const tooltip = ref<{
  visible: boolean;
  x: number;
  y: number;
  data: DailyUsage | null;
}>({
  visible: false,
  x: 0,
  y: 0,
  data: null
});

// 日期范围选项
const dateRanges = [
  { label: '最近 7 天', value: '7d' as const },
  { label: '最近 30 天', value: '30d' as const },
  { label: '全部时间', value: 'all' as const }
];

// 标签页定义
const tabs = [
  { id: 'overview', label: '概览', icon: 'dashboard' },
  { id: 'models', label: '按模型', icon: 'cpu' },
  { id: 'sessions', label: '会话', icon: 'messages' },
  { id: 'timeline', label: '时间线', icon: 'chart-line' }
];

// 从 store 获取数据
const statistics = computed(() => statsStore.statistics);
const loading = computed(() => statsStore.loading);
const error = computed(() => statsStore.error);
const formattedTotalCost = computed(() => statsStore.formattedTotalCost);
const formattedTotalTokens = computed(() => statsStore.formattedTotalTokens);
const avgCostPerSession = computed(() => statsStore.avgCostPerSession);
const lastUpdate = computed(() => statsStore.lastUpdate);
const selectedDateRange = computed(() => statsStore.selectedDateRange);
const activeTab = computed(() => statsStore.activeTab);
const projectScope = computed(() => statsStore.projectScope);
const availableProjects = computed(() => statsStore.availableProjects);
const selectedProject = computed(() => statsStore.selectedProject);
const projectsLoading = computed(() => statsStore.projectsLoading);
const activeProjectTab = computed<'all' | 'selected'>(() =>
  projectScope.value === 'selected' ? 'selected' : 'all'
);

// 过滤后的项目列表
const filteredProjects = computed(() => {
  if (!projectSearchQuery.value) {
    return availableProjects.value;
  }
  const query = projectSearchQuery.value.toLowerCase();
  return availableProjects.value.filter(p =>
    p.name.toLowerCase().includes(query)
  );
});

// 排序后的会话列表
const sortedSessions = computed(() => {
  if (!statistics.value) return [];
  const sorted = [...statistics.value.sessions].sort((a, b) => {
    if (sortBy.value === 'cost') {
      const costDiff = b.cost - a.cost;
      if (costDiff === 0) {
        return b.timestamp - a.timestamp;
      }
      return costDiff;
    } else {
      return b.timestamp - a.timestamp;
    }
  });
  return sorted.slice(0, 100);
});

// 分页会话列表
const paginatedSessions = computed(() => {
  const start = (currentPage.value - 1) * pageSize;
  const end = start + pageSize;
  return sortedSessions.value.slice(start, end);
});

const totalPages = computed(() => {
  return Math.ceil(sortedSessions.value.length / pageSize);
});

// 格式化最后更新时间
const formatLastUpdate = computed(() => {
  const date = new Date(lastUpdate.value);
  const now = new Date();
  const diff = now.getTime() - date.getTime();
  const minutes = Math.floor(diff / 60000);

  if (minutes < 1) return '刚刚';
  if (minutes < 60) return `${minutes}分钟前`;

  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}小时前`;

  const days = Math.floor(hours / 24);
  return `${days}天前`;
});

// 格式化数字
function formatNumber(num: number): string {
  if (num >= 1e9) return `${(num / 1e9).toFixed(2)}B`;
  if (num >= 1e6) return `${(num / 1e6).toFixed(2)}M`;
  if (num >= 1e3) return `${(num / 1e3).toFixed(2)}K`;
  return num.toString();
}

// 格式化时间
function formatTime(timestamp: number): string {
  const date = new Date(timestamp);
  const now = new Date();
  const diff = now.getTime() - date.getTime();

  if (diff < 0) return '刚刚';

  const hours = Math.floor(diff / 3600000);
  const days = Math.floor(hours / 24);

  if (hours < 1) {
    const minutes = Math.floor(diff / 60000);
    if (minutes <= 0) return '刚刚';
    return `${minutes}分钟前`;
  } else if (days < 1) {
    return `${hours}小时前`;
  } else if (days < 7) {
    return `${days}天前`;
  } else {
    return date.toLocaleDateString('zh-CN', { month: 'short', day: 'numeric' });
  }
}

// 获取 Token 百分比
function getTokenPercentage(type: 'input_tokens' | 'output_tokens' | 'cache_write_tokens' | 'cache_read_tokens'): number {
  if (!statistics.value) return 0;
  const total = statistics.value.total_usage.total_tokens;
  if (total === 0) return 0;
  return ((statistics.value.total_usage as any)[type] / total) * 100;
}

// 获取趋势类名
function getTrendClass(trend: number): string {
  if (trend > 0) return 'trend-up';
  if (trend < 0) return 'trend-down';
  return 'trend-neutral';
}

// 获取趋势图标
function getTrendIcon(trend: number): string {
  if (trend > 0) return 'trending-up';
  if (trend < 0) return 'trending-down';
  return 'minus';
}

// 格式化趋势百分比
function formatTrend(trend: number): string {
  const absValue = Math.abs(trend);
  if (absValue < 1) return '无变化';
  const sign = trend > 0 ? '+' : '';
  return `${sign}${trend.toFixed(1)}%`;
}

// 获取最大成本
function getMaxCost(): number {
  if (!statistics.value || !statistics.value.daily_usage) return 0;
  const days = statistics.value.daily_usage.slice(-7);
  if (days.length === 0) return 0;
  return Math.max(...days.map(d => d.cost), 0.01);
}

// 计算极简柱状图高度
function getDayBarHeightMinimal(value: number): number {
  const maxCost = getMaxCost();
  if (maxCost === 0) return 0;
  return (value / maxCost) * 100;
}

// 格式化极简图表日期
function formatChartDateMinimal(dateStr: string): string {
  const date = new Date(dateStr);
  const month = date.getMonth() + 1;
  const day = date.getDate();
  return `${month}月${day}日`;
}

// Tooltip 显示/隐藏
function showDayTooltip(event: MouseEvent, day: DailyUsage) {
  const rect = (event.target as HTMLElement).getBoundingClientRect();
  tooltip.value = {
    visible: true,
    x: rect.left + rect.width / 2,
    y: rect.top - 10,
    data: day
  };
}

function hideDayTooltip() {
  tooltip.value.visible = false;
}

// 项目选择相关
async function handleProjectTabChange(tab: 'all' | 'selected') {
  if (tab === 'all') {
    await statsStore.setProjectScope('all');
  } else {
    await statsStore.setProjectScope('selected');
    // 加载项目列表
    await statsStore.loadProjects();
  }
  currentPage.value = 1;
}

async function handleSelectProject(project: ProjectInfo) {
  await statsStore.setSelectedProject(project.path);
  showProjectDropdown.value = false;
  projectSearchQuery.value = '';
  currentPage.value = 1;
}

// 事件处理
async function handleRefresh() {
  await statsStore.refresh();
}

async function handleDateRangeChange(range: '7d' | '30d' | 'all') {
  await statsStore.setDateRange(range);
  currentPage.value = 1;
}

function handleTabChange(tab: string) {
  statsStore.setActiveTab(tab);
  currentPage.value = 1;
}

function handleSortChange(sort: 'cost' | 'time') {
  sortBy.value = sort;
  currentPage.value = 1;
}

// 监听标签页变化
watch(activeTab, () => {
  currentPage.value = 1;
});

// 生命周期
onMounted(async () => {
  // 如果没有初始化过，触发数据加载
  if (!statsStore.statistics) {
    statsStore.initialize();
  }

  // 添加全局点击监听器
  document.addEventListener('click', handleClickOutside);
});

// 清理
onUnmounted(() => {
  document.removeEventListener('click', handleClickOutside);
});

// 点击外部关闭下拉菜单
function handleClickOutside(event: MouseEvent) {
  const target = event.target as HTMLElement;
  if (!target.closest('.project-search-dropdown')) {
    showProjectDropdown.value = false;
  }
}
</script>

<template>
  <div class="stats-view">
    <!-- 加载状态 -->
    <div v-if="loading && !statistics" class="loading-state">
      <div class="spinner"></div>
      <p>加载统计数据中...</p>
    </div>

    <!-- 主要内容 -->
    <div v-else class="statistics-content">
      <!-- 项目Tab导航和时间选择器 -->
      <div class="project-tabs-header">
        <div class="project-tabs">
          <button
            :class="['project-tab', { active: activeProjectTab === 'all' }]"
            @click="handleProjectTabChange('all')"
          >
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <circle cx="12" cy="12" r="10"/>
              <path d="M2 12h20"/>
              <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/>
            </svg>
            所有项目
          </button>
          <button
            :class="['project-tab', { active: activeProjectTab === 'selected' }]"
            @click="handleProjectTabChange('selected')"
          >
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M3 7v10a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2z"/>
              <path d="M8 21h8"/>
              <path d="M12 17v4"/>
            </svg>
            选择项目
          </button>
        </div>

        <!-- 日期范围筛选器 -->
        <div class="date-filter">
          <button
            v-for="range in dateRanges"
            :key="range.value"
            :class="['filter-btn', { active: selectedDateRange === range.value }]"
            @click="handleDateRangeChange(range.value)"
          >
            {{ range.label }}
          </button>
        </div>
      </div>

      <!-- 项目选择器（在选择项目tab下显示） -->
      <div v-if="activeProjectTab === 'selected'" class="project-selector-row">
        <div class="project-search-dropdown">
          <div class="search-input-wrapper">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <circle cx="11" cy="11" r="8"/>
              <path d="m21 21-4.35-4.35"/>
            </svg>
            <input
              ref="projectSearchInput"
              v-model="projectSearchQuery"
              type="text"
              placeholder="搜索项目名称..."
              class="project-search-input"
              @focus="showProjectDropdown = true"
            />
            <button
              v-if="projectSearchQuery"
              class="clear-search-btn"
              @click="projectSearchQuery = ''"
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <line x1="18" y1="6" x2="6" y2="18"/>
                <line x1="6" y1="6" x2="18" y2="18"/>
              </svg>
            </button>
          </div>

          <!-- 下拉列表 -->
          <div v-if="showProjectDropdown" class="project-dropdown-menu">
            <div v-if="projectsLoading" class="dropdown-loading">
              加载中...
            </div>
            <div v-else-if="filteredProjects.length === 0" class="dropdown-empty">
              {{ projectSearchQuery ? '未找到匹配的项目' : '暂无项目' }}
            </div>
            <button
              v-for="project in filteredProjects"
              v-else
              :key="project.path"
              :class="['dropdown-project-item', { active: selectedProject?.path === project.path }]"
              @click="handleSelectProject(project)"
            >
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
              </svg>
              {{ project.name }}
            </button>
          </div>
        </div>

        <!-- 当前选择的项目 -->
        <div v-if="selectedProject" class="selected-project-display">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
          </svg>
          {{ selectedProject.name }}
        </div>

        <!-- 分隔线 -->
        <div class="selector-divider"></div>

        <!-- 刷新按钮 -->
        <button class="action-btn" @click="handleRefresh" title="刷新数据">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M21 2v6h-6"/>
            <path d="M3 12a9 9 0 0 1 15-6.7L21 8"/>
            <path d="M3 22v-6h6"/>
            <path d="M21 12a9 9 0 0 1-15 6.7L3 16"/>
          </svg>
        </button>
      </div>

      <!-- 统计内容视图 -->
      <template v-if="activeProjectTab === 'all' || (activeProjectTab === 'selected' && selectedProject)">

      <!-- 所有项目的刷新按钮行 -->
      <div v-if="activeProjectTab === 'all'" class="control-bar-simple">
        <button class="action-btn" @click="handleRefresh" title="刷新数据">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M21 2v6h-6"/>
            <path d="M3 12a9 9 0 0 1 15-6.7L21 8"/>
            <path d="M3 22v-6h6"/>
            <path d="M21 12a9 9 0 0 1-15 6.7L3 16"/>
          </svg>
          刷新数据
        </button>
      </div>

      <!-- 标签页导航 -->
      <div class="tabs">
        <button
          v-for="tab in tabs"
          :key="tab.id"
          :class="['tab', { active: activeTab === tab.id }]"
          @click="handleTabChange(tab.id)"
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <rect x="3" y="3" width="18" height="18" rx="2"/>
            <path d="M3 9h18"/>
            <path d="M9 21V9"/>
          </svg>
          {{ tab.label }}
        </button>
      </div>

      <!-- 标签页内容 -->
      <div class="tab-content">
        <template v-if="statistics">
          <!-- Overview 标签页 -->
          <div v-if="activeTab === 'overview'" class="tab-panel">
            <!-- 项目信息 -->
            <div class="project-info-header">
              <h3 class="project-title">
                <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
                </svg>
                {{ statistics.project_name }}
              </h3>
            </div>

            <!-- 总览卡片 -->
            <div class="overview-cards">
              <div class="stat-card">
                <div class="card-header">
                  <span class="card-icon cost-icon">
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <path d="M12 1v22"/>
                      <path d="M17 5H9.5a3.5 3.5 0 0 0 0 7h5a3.5 3.5 0 0 1 0 7H6"/>
                    </svg>
                  </span>
                  <span class="card-title">总金额</span>
                </div>
                <div class="card-value">{{ formattedTotalCost }}</div>
                <div v-if="statistics.weekly_comparison" class="card-trend" :class="getTrendClass(statistics.weekly_comparison.trends.cost)">
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <polyline :points="getTrendIcon(statistics.weekly_comparison.trends.cost) === 'trending-up' ? '23 6 13.5 15.5 8.5 10.5 1 18' : '23 18 13.5 8.5 8.5 13.5 1 6'"/>
                    <polyline points="17 6 23 6 23 12" v-if="getTrendIcon(statistics.weekly_comparison.trends.cost) === 'trending-up'"/>
                    <polyline points="17 18 23 18 23 12" v-else/>
                  </svg>
                  <span>较上周 {{ formatTrend(statistics.weekly_comparison.trends.cost) }}</span>
                </div>
              </div>

              <div class="stat-card">
                <div class="card-header">
                  <span class="card-icon tokens-icon">
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <path d="M4 19.5v-15A2.5 2.5 0 0 1 6.5 2H20v20H6.5a2.5 2.5 0 0 1 0-5H20"/>
                    </svg>
                  </span>
                  <span class="card-title">总 Token 量</span>
                </div>
                <div class="card-value">{{ formattedTotalTokens }}</div>
                <div v-if="statistics.weekly_comparison" class="card-trend" :class="getTrendClass(statistics.weekly_comparison.trends.tokens)">
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <polyline :points="getTrendIcon(statistics.weekly_comparison.trends.tokens) === 'trending-up' ? '23 6 13.5 15.5 8.5 10.5 1 18' : '23 18 13.5 8.5 8.5 13.5 1 6'"/>
                    <polyline points="17 6 23 6 23 12" v-if="getTrendIcon(statistics.weekly_comparison.trends.tokens) === 'trending-up'"/>
                    <polyline points="17 18 23 18 23 12" v-else/>
                  </svg>
                  <span>较上周 {{ formatTrend(statistics.weekly_comparison.trends.tokens) }}</span>
                </div>
              </div>

              <div class="stat-card">
                <div class="card-header">
                  <span class="card-icon sessions-icon">
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/>
                    </svg>
                  </span>
                  <span class="card-title">总会话</span>
                </div>
                <div class="card-value">{{ statistics.total_sessions }}</div>
                <div v-if="statistics.weekly_comparison" class="card-trend" :class="getTrendClass(statistics.weekly_comparison.trends.sessions)">
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <polyline :points="getTrendIcon(statistics.weekly_comparison.trends.sessions) === 'trending-up' ? '23 6 13.5 15.5 8.5 10.5 1 18' : '23 18 13.5 8.5 8.5 13.5 1 6'"/>
                    <polyline points="17 6 23 6 23 12" v-if="getTrendIcon(statistics.weekly_comparison.trends.sessions) === 'trending-up'"/>
                    <polyline points="17 18 23 18 23 12" v-else/>
                  </svg>
                  <span>较上周 {{ formatTrend(statistics.weekly_comparison.trends.sessions) }}</span>
                </div>
              </div>

              <div class="stat-card">
                <div class="card-header">
                  <span class="card-icon average-icon">
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <line x1="18" y1="20" x2="18" y2="10"/>
                      <line x1="12" y1="20" x2="12" y2="4"/>
                      <line x1="6" y1="20" x2="6" y2="14"/>
                    </svg>
                  </span>
                  <span class="card-title">平均/会话</span>
                </div>
                <div class="card-value">${{ avgCostPerSession.toFixed(4) }}</div>
              </div>
            </div>

            <!-- Token 分解 -->
            <div class="section">
              <h4 class="section-title">Token 使用分解</h4>
              <div class="token-breakdown">
                <div class="breakdown-item">
                  <div class="breakdown-header">
                    <span class="breakdown-label">
                      <span class="breakdown-dot input-dot"></span>
                      输入 Token
                    </span>
                    <span class="breakdown-value">{{ formatNumber(statistics.total_usage.input_tokens) }}</span>
                  </div>
                  <div class="breakdown-bar">
                    <div class="bar-fill input-bar" :style="{ width: getTokenPercentage('input_tokens') + '%' }"></div>
                  </div>
                </div>

                <div class="breakdown-item">
                  <div class="breakdown-header">
                    <span class="breakdown-label">
                      <span class="breakdown-dot output-dot"></span>
                      输出 Token
                    </span>
                    <span class="breakdown-value">{{ formatNumber(statistics.total_usage.output_tokens) }}</span>
                  </div>
                  <div class="breakdown-bar">
                    <div class="bar-fill output-bar" :style="{ width: getTokenPercentage('output_tokens') + '%' }"></div>
                  </div>
                </div>

                <div class="breakdown-item">
                  <div class="breakdown-header">
                    <span class="breakdown-label">
                      <span class="breakdown-dot cache-write-dot"></span>
                      缓存写入
                    </span>
                    <span class="breakdown-value">{{ formatNumber(statistics.total_usage.cache_write_tokens) }}</span>
                  </div>
                  <div class="breakdown-bar">
                    <div class="bar-fill cache-write-bar" :style="{ width: getTokenPercentage('cache_write_tokens') + '%' }"></div>
                  </div>
                </div>

                <div class="breakdown-item">
                  <div class="breakdown-header">
                    <span class="breakdown-label">
                      <span class="breakdown-dot cache-read-dot"></span>
                      缓存读取
                    </span>
                    <span class="breakdown-value">{{ formatNumber(statistics.total_usage.cache_read_tokens) }}</span>
                  </div>
                  <div class="breakdown-bar">
                    <div class="bar-fill cache-read-bar" :style="{ width: getTokenPercentage('cache_read_tokens') + '%' }"></div>
                  </div>
                </div>
              </div>
            </div>

            <!-- 最常用模型 Top 3 -->
            <div v-if="statistics.by_model && statistics.by_model.length > 0" class="section">
              <h4 class="section-title">最常用模型</h4>
              <div class="model-summary">
                <div v-for="(model, index) in statistics.by_model.slice(0, 3)" :key="model.model" class="model-card">
                  <div class="model-rank">#{{ index + 1 }}</div>
                  <div class="model-info">
                    <div class="model-name">{{ model.model }}</div>
                    <div class="model-stats">
                      <span>{{ model.session_count }} 会话</span>
                      <span class="separator">•</span>
                      <span>${{ model.total_cost.toFixed(4) }}</span>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>

          <!-- Models 标签页 -->
          <div v-if="activeTab === 'models'" class="tab-panel">
            <div v-if="statistics.by_model && statistics.by_model.length > 0" class="models-list">
              <div v-for="model in statistics.by_model" :key="model.model" class="model-item">
                <div class="model-header">
                  <div class="model-title">
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <rect x="4" y="4" width="16" height="16" rx="2"/>
                      <rect x="9" y="9" width="6" height="6"/>
                      <path d="M9 4V2"/>
                      <path d="M15 4V2"/>
                      <path d="M9 20v2"/>
                      <path d="M15 20v2"/>
                      <path d="M4 9H2"/>
                      <path d="M4 15H2"/>
                      <path d="M20 9h2"/>
                      <path d="M20 15h2"/>
                    </svg>
                    {{ model.model }}
                  </div>
                  <div class="model-cost">${{ model.total_cost.toFixed(4) }}</div>
                </div>
                <div class="model-details">
                  <div class="detail-item">
                    <span class="detail-label">会话数</span>
                    <span class="detail-value">{{ model.session_count }}</span>
                  </div>
                  <div class="detail-item">
                    <span class="detail-label">总 Token</span>
                    <span class="detail-value">{{ formatNumber(model.total_tokens) }}</span>
                  </div>
                  <div class="detail-item">
                    <span class="detail-label">输入 Token</span>
                    <span class="detail-value">{{ formatNumber(model.input_tokens) }}</span>
                  </div>
                  <div class="detail-item">
                    <span class="detail-label">输出 Token</span>
                    <span class="detail-value">{{ formatNumber(model.output_tokens) }}</span>
                  </div>
                  <div class="detail-item">
                    <span class="detail-label">缓存写入</span>
                    <span class="detail-value">{{ formatNumber(model.cache_creation_tokens) }}</span>
                  </div>
                  <div class="detail-item">
                    <span class="detail-label">缓存读取</span>
                    <span class="detail-value">{{ formatNumber(model.cache_read_tokens) }}</span>
                  </div>
                </div>
              </div>
            </div>
            <div v-else class="empty-state">
              <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <circle cx="12" cy="12" r="10"/>
                <line x1="12" y1="16" x2="12" y2="12"/>
                <line x1="12" y1="8" x2="12.01" y2="8"/>
              </svg>
              <p>暂无模型统计数据</p>
            </div>
          </div>

          <!-- Sessions 标签页 -->
          <div v-if="activeTab === 'sessions'" class="tab-panel">
            <div class="section">
              <div class="section-header">
                <h4 class="section-title">最近会话</h4>
                <div class="sort-buttons">
                  <button
                    :class="['sort-btn', { active: sortBy === 'cost' }]"
                    @click="handleSortChange('cost')"
                    title="按消费排序"
                  >
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <rect x="1" y="4" width="22" height="16" rx="2"/>
                      <line x1="1" y1="10" x2="23" y2="10"/>
                    </svg>
                    按金额
                  </button>
                  <button
                    :class="['sort-btn', { active: sortBy === 'time' }]"
                    @click="handleSortChange('time')"
                    title="按时间排序"
                  >
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <circle cx="12" cy="12" r="10"/>
                      <polyline points="12 6 12 12 16 14"/>
                    </svg>
                    按时间
                  </button>
                </div>
              </div>
              <div class="sessions-list">
                <div v-for="(session, index) in paginatedSessions" :key="session.session_id" class="session-item">
                  <div class="session-rank">{{ (currentPage - 1) * pageSize + index + 1 }}</div>
                  <div class="session-info">
                    <div class="session-title">{{ session.summary || session.session_id }}</div>
                    <div class="session-meta">
                      <span class="session-model">{{ session.model }}</span>
                      <span class="separator">•</span>
                      <span class="session-time">{{ formatTime(session.timestamp) }}</span>
                    </div>
                  </div>
                  <div class="session-stats">
                    <span class="session-cost">${{ session.cost.toFixed(4) }}</span>
                    <span class="session-tokens" :title="`输入: ${session.usage.input_tokens} | 输出: ${session.usage.output_tokens}`">
                      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                        <path d="M4 19.5v-15A2.5 2.5 0 0 1 6.5 2H20v20H6.5a2.5 2.5 0 0 1 0-5H20"/>
                      </svg>
                      {{ formatNumber(session.usage.total_tokens) }}
                    </span>
                  </div>
                </div>

                <div v-if="sortedSessions.length === 0" class="empty-sessions">
                  <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <circle cx="12" cy="12" r="10"/>
                    <line x1="12" y1="16" x2="12" y2="12"/>
                    <line x1="12" y1="8" x2="12.01" y2="8"/>
                  </svg>
                  <p>{{ projectScope === 'all' ? '暂无会话记录' : '当前项目暂无会话记录' }}</p>
                </div>
              </div>

              <!-- 分页控制 -->
              <div v-if="totalPages > 1" class="pagination">
                <button class="page-btn" :disabled="currentPage === 1" @click="currentPage--">
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <polyline points="15 18 9 12 15 6"/>
                  </svg>
                </button>
                <span class="page-info">{{ currentPage }} / {{ totalPages }}</span>
                <button class="page-btn" :disabled="currentPage === totalPages" @click="currentPage++">
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <polyline points="9 18 15 12 9 6"/>
                  </svg>
                </button>
              </div>
            </div>
          </div>

          <!-- Timeline 标签页 -->
          <div v-if="activeTab === 'timeline'" class="tab-panel">
            <div v-if="statistics.daily_usage && statistics.daily_usage.length > 0" class="section">
              <h4 class="section-title">每日使用趋势</h4>
              <div class="daily-chart-minimal">
                <div class="chart-container-minimal">
                  <!-- Y轴标签 -->
                  <div class="y-axis">
                    <div class="y-label">${{ getMaxCost().toFixed(2) }}</div>
                    <div class="y-label">${{ (getMaxCost() / 2).toFixed(2) }}</div>
                    <div class="y-label">$0.00</div>
                  </div>

                  <!-- 图表区域 -->
                  <div class="chart-content">
                    <!-- 网格线 -->
                    <div class="grid-lines">
                      <div class="grid-line"></div>
                      <div class="grid-line"></div>
                      <div class="grid-line"></div>
                    </div>

                    <!-- 柱状图 -->
                    <div class="chart-bars-minimal">
                      <div
                        v-for="day in statistics.daily_usage.slice(-7)"
                        :key="day.date"
                        class="chart-bar-item"
                        @mouseenter="showDayTooltip($event, day)"
                        @mouseleave="hideDayTooltip"
                      >
                        <div class="bar-minimal" :style="{ height: getDayBarHeightMinimal(day.cost) + '%' }"></div>
                        <div class="bar-label">{{ formatChartDateMinimal(day.date) }}</div>
                      </div>
                    </div>
                  </div>
                </div>

                <!-- Tooltip -->
                <div v-if="tooltip.visible" class="chart-tooltip-minimal" :style="{ left: tooltip.x + 'px', top: tooltip.y + 'px' }">
                  <div class="tooltip-date">{{ tooltip.data?.date }}</div>
                  <div class="tooltip-row">
                    <span>消费:</span>
                    <span>${{ tooltip.data?.cost.toFixed(4) }}</span>
                  </div>
                  <div class="tooltip-row">
                    <span>会话:</span>
                    <span>{{ tooltip.data?.sessions }}</span>
                  </div>
                </div>
              </div>
            </div>
            <div v-else class="empty-state">
              <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <line x1="18" y1="20" x2="18" y2="10"/>
                <line x1="12" y1="20" x2="12" y2="4"/>
                <line x1="6" y1="20" x2="6" y2="14"/>
              </svg>
              <p>暂无时间线数据</p>
            </div>
          </div>
        </template>
        <div v-else class="empty-state empty-tab-placeholder">
          <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <line x1="18" y1="20" x2="18" y2="10"/>
            <line x1="12" y1="20" x2="12" y2="4"/>
            <line x1="6" y1="20" x2="6" y2="14"/>
          </svg>
          <p v-if="error">{{ error }}</p>
          <p v-else>暂无使用数据</p>
          <button class="action-btn" @click="handleRefresh">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M21 2v6h-6"/>
              <path d="M3 12a9 9 0 0 1 15-6.7L21 8"/>
            </svg>
            刷新数据
          </button>
        </div>
      </div>

      <!-- 更新时间 -->
      <div v-if="statistics" class="last-update">
        最后更新: {{ formatLastUpdate }}
      </div>
      </template>
    </div>
  </div>
</template>

<style scoped>
.stats-view {
  height: 100%;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

/* 加载状态 */
.loading-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 48px;
  color: var(--text-secondary, #6b7280);
}

.spinner {
  width: 32px;
  height: 32px;
  border: 3px solid var(--border-color, #e5e7eb);
  border-top-color: var(--primary-color, #6366f1);
  border-radius: 50%;
  animation: spin 1s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

/* 主要内容 */
.statistics-content {
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
}

/* 控制栏 */
.control-bar {
  display: flex;
  justify-content: flex-end;
  align-items: center;
  gap: 16px;
  padding: 1rem 1.5rem;
  border-bottom: 1px solid var(--border-color, #e5e7eb);
}

/* 项目选择器 */
.project-tabs-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 16px;
  padding: 0.75rem 1.5rem;
  border-bottom: 1px solid var(--border-color, #e5e7eb);
  background: rgb(248, 250, 252);
}

.project-tabs {
  display: flex;
  gap: 4px;
}

.project-tab {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 16px;
  border: none;
  border-bottom: 2px solid transparent;
  background: transparent;
  color: var(--text-secondary, #6b7280);
  cursor: pointer;
  font-size: 14px;
  font-weight: 500;
  transition: all 0.2s;
}

.project-tab:hover {
  color: var(--text-primary, #1f2937);
}

.project-tab.active {
  color: var(--primary-color, #6366f1);
  border-bottom-color: var(--primary-color, #6366f1);
}

/* 项目选择器行 */
.project-selector-row {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 1rem 1.5rem;
  border-bottom: 1px solid var(--border-color, #e5e7eb);
}

.selector-divider {
  width: 1px;
  height: 24px;
  background: var(--border-color, #e5e7eb);
}

/* 简单控制栏 */
.control-bar-simple {
  display: flex;
  justify-content: flex-end;
  padding: 1rem 1.5rem;
  border-bottom: 1px solid var(--border-color, #e5e7eb);
}

.project-search-dropdown {
  position: relative;
  flex: 1;
  max-width: 400px;
}

.search-input-wrapper {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 8px;
  background: var(--bg-primary, #ffffff);
  transition: all 0.2s;
}

.search-input-wrapper:focus-within {
  border-color: var(--primary-color, #6366f1);
  box-shadow: 0 0 0 3px rgba(99, 102, 241, 0.1);
}

.search-input-wrapper svg {
  flex-shrink: 0;
  color: var(--text-secondary, #6b7280);
}

.project-search-input {
  flex: 1;
  border: none;
  background: transparent;
  outline: none;
  font-size: 14px;
  color: var(--text-primary, #1f2937);
}

.project-search-input::placeholder {
  color: var(--text-secondary, #9ca3af);
}

.clear-search-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 4px;
  border: none;
  background: transparent;
  color: var(--text-secondary, #9ca3af);
  cursor: pointer;
  border-radius: 4px;
  transition: all 0.15s;
}

.clear-search-btn:hover {
  color: var(--text-primary, #1f2937);
  background: var(--bg-secondary, #f3f4f6);
}

/* 项目下拉菜单 */
.project-dropdown-menu {
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  right: 0;
  max-height: 250px;
  overflow-y: auto;
  background: var(--bg-primary, #ffffff);
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 8px;
  box-shadow: 0 4px 20px rgba(0, 0, 0, 0.15);
  z-index: 100;
}

.dropdown-loading,
.dropdown-empty {
  padding: 12px 16px;
  color: var(--text-secondary, #6b7280);
  font-size: 13px;
  text-align: center;
}

.dropdown-project-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 10px 12px;
  border: none;
  background: transparent;
  color: var(--text-primary, #1f2937);
  cursor: pointer;
  font-size: 13px;
  text-align: left;
  transition: all 0.15s;
}

.dropdown-project-item:hover {
  background: var(--bg-secondary, #f9fafb);
}

.dropdown-project-item.active {
  background: var(--primary-color-light, #eef2ff);
  color: var(--primary-color, #6366f1);
}

.dropdown-project-item svg {
  flex-shrink: 0;
  color: var(--text-secondary, #9ca3af);
}

.dropdown-project-item.active svg {
  color: var(--primary-color, #6366f1);
}

/* 当前选择的项目显示 */
.selected-project-display {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  border-radius: 6px;
  background: var(--bg-secondary, #f3f4f6);
  color: var(--text-primary, #1f2937);
  font-size: 13px;
}

.selected-project-display svg {
  flex-shrink: 0;
  color: var(--text-secondary, #6b7280);
}

/* 日期筛选器 */
.date-filter {
  display: flex;
  gap: 4px;
  background: var(--bg-tertiary, #f3f4f6);
  border-radius: 8px;
  padding: 4px;
}

.filter-btn {
  padding: 8px 14px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--text-secondary, #6b7280);
  cursor: pointer;
  font-size: 13px;
  font-weight: 500;
  transition: all 0.2s;
}

.filter-btn:hover {
  background: var(--bg-secondary, #f9fafb);
}

.filter-btn.active {
  background: var(--bg-primary, #ffffff);
  color: var(--text-primary, #1f2937);
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.05);
}

/* 操作按钮 */
.action-group {
  display: flex;
  gap: 8px;
}

.action-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 8px 12px;
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 8px;
  background: var(--bg-primary, #ffffff);
  color: var(--text-primary, #1f2937);
  cursor: pointer;
  font-size: 13px;
  transition: all 0.2s;
}

.action-btn:hover {
  background: var(--bg-secondary, #f9fafb);
}

/* 标签页 */
.tabs {
  display: flex;
  gap: 4px;
  padding: 0 1.5rem;
  border-bottom: 1px solid var(--border-color, #e5e7eb);
}

.tab {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 12px 16px;
  border: none;
  border-bottom: 2px solid transparent;
  background: transparent;
  color: var(--text-secondary, #6b7280);
  cursor: pointer;
  font-size: 13px;
  font-weight: 500;
  transition: all 0.2s;
}

.tab:hover {
  color: var(--text-primary, #1f2937);
}

.tab.active {
  color: var(--primary-color, #6366f1);
  border-bottom-color: var(--primary-color, #6366f1);
}

/* 标签页内容 */
.tab-content {
  flex: 1;
  overflow-y: auto;
  padding: 1.5rem;
}

.tab-panel {
  animation: fadeIn 0.3s ease;
}

@keyframes fadeIn {
  from {
    opacity: 0;
    transform: translateY(10px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

/* 项目信息头部 */
.project-info-header {
  margin-bottom: 1.5rem;
}

.project-title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 1.125rem;
  font-weight: 600;
  margin: 0;
  color: var(--text-primary, #1f2937);
}

.project-title svg {
  color: var(--primary-color, #6366f1);
}

/* 总览卡片 */
.overview-cards {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
  gap: 1rem;
  margin-bottom: 2rem;
}

.stat-card {
  padding: 1.25rem;
  background: var(--bg-secondary, #f9fafb);
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 12px;
  transition: all 0.2s;
}

.stat-card:hover {
  transform: translateY(-2px);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.1);
}

.card-header {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 0.75rem;
}

.card-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  border-radius: 8px;
  font-size: 1.125rem;
  font-weight: 600;
  color: #fff;
}

.cost-icon {
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
}

.sessions-icon {
  background: linear-gradient(135deg, #f093fb 0%, #f5576c 100%);
  display: flex;
}

.tokens-icon {
  background: linear-gradient(135deg, #4facfe 0%, #00f2fe 100%);
  display: flex;
}

.average-icon {
  background: linear-gradient(135deg, #43e97b 0%, #38f9d7 100%);
  display: flex;
}

.card-title {
  font-size: 0.875rem;
  color: var(--text-secondary, #6b7280);
}

.card-value {
  font-size: 1.5rem;
  font-weight: 700;
  color: var(--text-primary, #1f2937);
}

/* 趋势指示器 */
.card-trend {
  display: flex;
  align-items: center;
  gap: 4px;
  margin-top: 0.5rem;
  font-size: 0.75rem;
  padding: 4px 8px;
  border-radius: 6px;
  width: fit-content;
}

.trend-up {
  color: #10b981;
  background: rgba(16, 185, 129, 0.1);
}

.trend-down {
  color: #ef4444;
  background: rgba(239, 68, 68, 0.1);
}

.trend-neutral {
  color: var(--text-secondary, #6b7280);
  background: var(--bg-tertiary, #f3f4f6);
}

/* 章节 */
.section {
  margin-bottom: 2rem;
}

.section-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 1rem;
}

/* 排序按钮组 */
.sort-buttons {
  display: flex;
  gap: 4px;
  background: var(--bg-tertiary, #f3f4f6);
  border-radius: 8px;
  padding: 4px;
}

.sort-btn {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 6px 10px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--text-secondary, #6b7280);
  cursor: pointer;
  font-size: 0.75rem;
  font-weight: 500;
  transition: all 0.2s;
}

.sort-btn:hover {
  background: var(--bg-secondary, #f9fafb);
}

.sort-btn.active {
  background: var(--bg-primary, #ffffff);
  color: var(--text-primary, #1f2937);
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.05);
}

.section-title {
  font-size: 1rem;
  font-weight: 600;
  margin: 0 0 1rem 0;
  color: var(--text-primary, #1f2937);
}

/* Token 分解 */
.token-breakdown {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.breakdown-item {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.breakdown-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  font-size: 0.875rem;
}

.breakdown-label {
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--text-secondary, #6b7280);
}

.breakdown-dot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
}

.input-dot {
  background: #3b82f6;
}

.output-dot {
  background: #10b981;
}

.cache-write-dot {
  background: #f59e0b;
}

.cache-read-dot {
  background: #8b5cf6;
}

.breakdown-value {
  font-weight: 500;
  color: var(--text-primary, #1f2937);
}

.breakdown-bar {
  height: 8px;
  background: var(--bg-tertiary, #f3f4f6);
  border-radius: 4px;
  overflow: hidden;
}

.bar-fill {
  height: 100%;
  border-radius: 4px;
  transition: width 0.3s ease;
}

.input-bar {
  background: #3b82f6;
}

.output-bar {
  background: #10b981;
}

.cache-write-bar {
  background: #f59e0b;
}

.cache-read-bar {
  background: #8b5cf6;
}

/* 模型概要 */
.model-summary {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.model-card {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 1rem;
  background: var(--bg-secondary, #f9fafb);
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 8px;
}

.model-rank {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border-radius: 50%;
  background: var(--primary-color, #6366f1);
  color: #fff;
  font-size: 0.75rem;
  font-weight: 600;
  flex-shrink: 0;
}

.model-info {
  flex: 1;
}

.model-name {
  font-size: 0.875rem;
  font-weight: 500;
  color: var(--text-primary, #1f2937);
  margin-bottom: 2px;
}

.model-stats {
  font-size: 0.75rem;
  color: var(--text-secondary, #6b7280);
}

.separator {
  margin: 0 6px;
}

/* 模型列表 */
.models-list {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.model-item {
  padding: 1rem;
  background: var(--bg-secondary, #f9fafb);
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 12px;
}

.model-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 0.75rem;
  padding-bottom: 0.75rem;
  border-bottom: 1px solid var(--border-color, #e5e7eb);
}

.model-title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 0.875rem;
  font-weight: 600;
  color: var(--text-primary, #1f2937);
}

.model-cost {
  font-size: 1.125rem;
  font-weight: 600;
  color: var(--primary-color, #6366f1);
}

.model-details {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(120px, 1fr));
  gap: 0.75rem;
}

.detail-item {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.detail-label {
  font-size: 0.75rem;
  color: var(--text-secondary, #6b7280);
}

.detail-value {
  font-size: 0.875rem;
  font-weight: 500;
  color: var(--text-primary, #1f2937);
}

/* 会话列表 */
.sessions-list {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  margin-bottom: 1rem;
}

.session-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 0.75rem 1rem;
  background: var(--bg-secondary, #f9fafb);
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 8px;
  transition: all 0.2s;
}

.session-item:hover {
  background: var(--bg-tertiary, #f3f4f6);
}

.session-rank {
  display: flex;
  align-items: center;
  justify-content: center;
  min-width: 24px;
  height: 24px;
  border-radius: 50%;
  background: var(--primary-color, #6366f1);
  color: #fff;
  font-size: 0.75rem;
  font-weight: 600;
  flex-shrink: 0;
}

.session-info {
  flex: 1;
  min-width: 0;
}

.session-title {
  font-size: 0.875rem;
  font-weight: 500;
  color: var(--text-primary, #1f2937);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  margin-bottom: 2px;
}

.session-meta {
  font-size: 0.75rem;
  color: var(--text-secondary, #6b7280);
}

.session-model {
  font-weight: 500;
}

.session-stats {
  display: flex;
  gap: 12px;
  align-items: center;
}

.session-cost {
  font-size: 0.875rem;
  font-weight: 600;
  color: var(--primary-color, #6366f1);
}

.session-tokens {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 0.75rem;
  color: var(--text-secondary, #6b7280);
}

/* 分页 */
.pagination {
  display: flex;
  justify-content: center;
  align-items: center;
  gap: 16px;
  margin-top: 1rem;
}

.page-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 6px;
  background: var(--bg-primary, #ffffff);
  color: var(--text-primary, #1f2937);
  cursor: pointer;
}

.page-btn:hover:not(:disabled) {
  background: var(--bg-secondary, #f9fafb);
}

.page-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.page-info {
  font-size: 0.875rem;
  color: var(--text-primary, #1f2937);
}

/* 时间线图表 */
.daily-chart-minimal {
  background: var(--bg-secondary, #f9fafb);
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 12px;
  padding: 1.5rem;
  position: relative;
}

.chart-container-minimal {
  display: flex;
  gap: 16px;
  min-height: 250px;
}

/* Y轴 */
.y-axis {
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  min-width: 60px;
  padding: 8px 0;
  font-size: 0.75rem;
  color: var(--text-secondary, #6b7280);
}

.y-label {
  text-align: right;
  line-height: 1;
}

/* 图表内容区域 */
.chart-content {
  flex: 1;
  position: relative;
  min-height: 220px;
  display: flex;
  flex-direction: column;
}

/* 网格线 */
.grid-lines {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  bottom: 30px;
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  pointer-events: none;
}

.grid-line {
  height: 1px;
  background: var(--border-color, #e5e7eb);
  opacity: 0.3;
}

/* 柱状图区域 */
.chart-bars-minimal {
  display: flex;
  align-items: flex-end;
  justify-content: space-around;
  gap: 8px;
  height: 190px;
  padding: 0 12px;
  position: relative;
  z-index: 1;
  min-width: 400px;
}

.chart-bar-item {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: flex-end;
  height: 100%;
  cursor: pointer;
  max-width: 80px;
  min-width: 40px;
}

.bar-minimal {
  width: 100%;
  background: linear-gradient(180deg, #7c3aed 0%, #a78bfa 100%);
  border-radius: 4px 4px 0 0;
  transition: all 0.2s ease;
  min-height: 2px;
}

.chart-bar-item:hover .bar-minimal {
  opacity: 0.8;
  transform: translateY(-2px);
}

.bar-label {
  margin-top: 12px;
  font-size: 0.75rem;
  color: var(--text-secondary, #6b7280);
  white-space: nowrap;
  text-align: center;
}

/* Tooltip */
.chart-tooltip-minimal {
  position: fixed;
  transform: translate(-50%, -100%);
  padding: 8px 12px;
  background: var(--bg-primary, #ffffff);
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 8px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
  z-index: 1000;
  pointer-events: none;
  font-size: 0.75rem;
  min-width: 120px;
  margin-bottom: 8px;
}

.chart-tooltip-minimal .tooltip-date {
  font-weight: 600;
  margin-bottom: 6px;
  color: var(--text-primary, #1f2937);
  font-size: 0.875rem;
}

.chart-tooltip-minimal .tooltip-row {
  display: flex;
  justify-content: space-between;
  gap: 16px;
  margin-bottom: 2px;
  color: var(--text-secondary, #6b7280);
}

/* 空状态 */
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 48px;
  color: var(--text-secondary, #6b7280);
}

.empty-state svg {
  opacity: 0.5;
  margin-bottom: 16px;
}

.empty-tab-placeholder {
  min-height: 400px;
  width: 100%;
}

.empty-sessions {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 32px;
  color: var(--text-secondary, #6b7280);
}

.empty-sessions svg {
  opacity: 0.5;
  margin-bottom: 8px;
}

/* 最后更新 */
.last-update {
  margin-top: 1.5rem;
  padding-top: 1rem;
  border-top: 1px solid var(--border-color, #e5e7eb);
  font-size: 0.75rem;
  color: var(--text-secondary, #6b7280);
  text-align: center;
}

/* 深色模式 */
@media (prefers-color-scheme: dark) {
  .stats-view {
    background-color: var(--bg-primary, #111827);
  }

  .control-bar {
    border-bottom-color: var(--border-color, #374151);
  }

  .date-filter {
    background: var(--bg-tertiary, #1f2937);
  }

  .filter-btn {
    color: var(--text-secondary, #9ca3af);
  }

  .filter-btn:hover {
    background: var(--bg-secondary, #1f2937);
  }

  .filter-btn.active {
    background: var(--bg-primary, #111827);
    color: var(--text-primary, #f9fafb);
  }

  .project-select-btn,
  .action-btn {
    background: var(--bg-secondary, #1f2937);
    border-color: var(--border-color, #374151);
    color: var(--text-primary, #f9fafb);
  }

  .action-btn:hover {
    background: var(--bg-tertiary, #374151);
  }

  .project-tab:hover {
    color: var(--text-primary, #f9fafb);
  }

  .project-selector-row {
    border-bottom-color: var(--border-color, #374151);
  }

  .selector-divider {
    background: var(--border-color, #4b5563);
  }

  .control-bar-simple {
    border-bottom-color: var(--border-color, #374151);
  }

  .search-input-wrapper {
    background: var(--bg-secondary, #1f2937);
    border-color: var(--border-color, #374151);
  }

  .search-input-wrapper:focus-within {
    border-color: var(--primary-color, #818cf8);
    box-shadow: 0 0 0 3px rgba(129, 140, 248, 0.1);
  }

  .project-search-input {
    color: var(--text-primary, #f9fafb);
  }

  .project-search-input::placeholder {
    color: var(--text-secondary, #9ca3af);
  }

  .clear-search-btn:hover {
    background: var(--bg-tertiary, #374151);
  }

  .project-dropdown-menu {
    background: var(--bg-secondary, #1f2937);
    border-color: var(--border-color, #374151);
  }

  .dropdown-project-item {
    color: var(--text-primary, #f9fafb);
  }

  .dropdown-project-item:hover {
    background: var(--bg-tertiary, #374151);
  }

  .dropdown-project-item.active {
    background: rgba(129, 140, 248, 0.15);
    color: var(--primary-color, #818cf8);
  }

  .selected-project-display {
    background: var(--bg-tertiary, #374151);
    color: var(--text-primary, #f9fafb);
  }

  .tabs {
    border-bottom-color: var(--border-color, #374151);
  }

  .tab {
    color: var(--text-secondary, #9ca3af);
  }

  .tab:hover {
    color: var(--text-primary, #f9fafb);
  }

  .project-title,
  .section-title,
  .model-title {
    color: var(--text-primary, #f9fafb);
  }

  .stat-card,
  .model-item,
  .session-item,
  .daily-chart-minimal {
    background: var(--bg-secondary, #1f2937);
    border-color: var(--border-color, #374151);
  }

  .stat-card:hover,
  .session-item:hover {
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
  }

  .card-value,
  .breakdown-value,
  .detail-value,
  .session-title,
  .model-name {
    color: var(--text-primary, #f9fafb);
  }

  .card-title,
  .breakdown-label,
  .detail-label,
  .session-meta,
  .model-stats,
  .page-info,
  .bar-label,
  .y-label {
    color: var(--text-secondary, #9ca3af);
  }

  .breakdown-bar,
  .grid-line {
    background: var(--bg-tertiary, #374151);
  }

  .chart-tooltip-minimal {
    background: var(--bg-secondary, #1f2937);
    border-color: var(--border-color, #374151);
  }
}
</style>
