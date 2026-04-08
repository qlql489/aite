/**
 * Token 使用统计 Store
 * 从 Claude 会话日志文件读取使用统计数据
 * 支持异步加载和后端缓存
 */

import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type {
  ProjectStatistics,
  ProjectScope,
  DateRange,
  UsageData,
  SessionSummary,
  ModelUsage,
  DailyUsage,
  ProjectInfo,
} from '../types';

export const useStatsStore = defineStore('stats', () => {
  // ========== 状态 ==========
  const statistics = ref<ProjectStatistics | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const lastUpdate = ref<number>(Date.now());
  const initialized = ref(false);

  // 筛选状态
  const selectedDateRange = ref<DateRange>('7d');
  const activeTab = ref<string>('overview');
  const loadedTabs = ref<Set<string>>(new Set(['overview']));
  const projectScope = ref<ProjectScope>('all');

  // 项目选择相关
  const availableProjects = ref<ProjectInfo[]>([]);
  const selectedProjectPath = ref<string | null>(null);
  const projectsLoading = ref(false);

  // ========== 计算属性 ==========

  const hasData = computed(() => statistics.value !== null && statistics.value.total_sessions > 0);

  const formattedTotalCost = computed(() => {
    if (!statistics.value) return '$0.00';
    return `$${statistics.value.estimated_cost.toFixed(2)}`;
  });

  const formattedTotalTokens = computed(() => {
    if (!statistics.value) return '0';
    const tokens = statistics.value.total_usage.total_tokens;
    if (tokens >= 1e9) return `${(tokens / 1e9).toFixed(2)}B`;
    if (tokens >= 1e6) return `${(tokens / 1e6).toFixed(2)}M`;
    if (tokens >= 1e3) return `${(tokens / 1e3).toFixed(2)}K`;
    return tokens.toString();
  });

  const avgCostPerSession = computed(() => {
    if (!statistics.value || statistics.value.total_sessions === 0) return 0;
    return statistics.value.estimated_cost / statistics.value.total_sessions;
  });

  const selectedProject = computed(() => {
    if (!selectedProjectPath.value) return null;
    return availableProjects.value.find(p => p.path === selectedProjectPath.value) || null;
  });

  // ========== 私有方法 ==========

  /**
   * 按日期范围筛选会话
   */
  function filterSessionsByDateRange(sessions: SessionSummary[], range: DateRange): SessionSummary[] {
    if (range === 'all') return sessions;

    const now = Date.now();
    const days = range === '7d' ? 7 : 30;
    const cutoff = now - (days * 24 * 60 * 60 * 1000);

    return sessions.filter(s => s.timestamp > cutoff);
  }

  /**
   * 聚合筛选后的统计数据
   * @param sessions - 筛选后的会话列表
   * @param baseData - 原始完整统计数据（用于获取项目信息等）
   */
  function aggregateFilteredStatistics(sessions: SessionSummary[], baseData: ProjectStatistics): ProjectStatistics | null {
    const totalUsage: UsageData = {
      input_tokens: 0,
      output_tokens: 0,
      cache_write_tokens: 0,
      cache_read_tokens: 0,
      total_tokens: 0,
    };

    let totalCost = 0;
    const modelMap = new Map<string, ModelUsage>();
    const dailyMap = new Map<string, DailyUsage>();

    for (const session of sessions) {
      // 累加使用量
      totalUsage.input_tokens += session.usage.input_tokens;
      totalUsage.output_tokens += session.usage.output_tokens;
      totalUsage.cache_write_tokens += session.usage.cache_write_tokens;
      totalUsage.cache_read_tokens += session.usage.cache_read_tokens;
      totalUsage.total_tokens += session.usage.total_tokens;
      totalCost += session.cost;

      // 按模型聚合
      const modelEntry = modelMap.get(session.model);
      if (modelEntry) {
        modelEntry.total_cost += session.cost;
        modelEntry.total_tokens += session.usage.total_tokens;
        modelEntry.input_tokens += session.usage.input_tokens;
        modelEntry.output_tokens += session.usage.output_tokens;
        modelEntry.cache_creation_tokens += session.usage.cache_write_tokens;
        modelEntry.cache_read_tokens += session.usage.cache_read_tokens;
        modelEntry.session_count += 1;
      } else {
        modelMap.set(session.model, {
          model: session.model,
          total_cost: session.cost,
          total_tokens: session.usage.total_tokens,
          input_tokens: session.usage.input_tokens,
          output_tokens: session.usage.output_tokens,
          cache_creation_tokens: session.usage.cache_write_tokens,
          cache_read_tokens: session.usage.cache_read_tokens,
          session_count: 1,
        });
      }

      // 按日期聚合
      const date = new Date(session.timestamp).toISOString().split('T')[0];
      const dailyEntry = dailyMap.get(date);
      if (dailyEntry) {
        dailyEntry.sessions += 1;
        dailyEntry.usage.input_tokens += session.usage.input_tokens;
        dailyEntry.usage.output_tokens += session.usage.output_tokens;
        dailyEntry.usage.cache_write_tokens += session.usage.cache_write_tokens;
        dailyEntry.usage.cache_read_tokens += session.usage.cache_read_tokens;
        dailyEntry.usage.total_tokens += session.usage.total_tokens;
        dailyEntry.cost += session.cost;
        if (!dailyEntry.models_used.includes(session.model)) {
          dailyEntry.models_used.push(session.model);
        }
      } else {
        dailyMap.set(date, {
          date,
          sessions: 1,
          usage: {
            input_tokens: session.usage.input_tokens,
            output_tokens: session.usage.output_tokens,
            cache_write_tokens: session.usage.cache_write_tokens,
            cache_read_tokens: session.usage.cache_read_tokens,
            total_tokens: session.usage.total_tokens,
          },
          cost: session.cost,
          models_used: [session.model],
        });
      }
    }

    // 按成本排序模型
    const byModel = Array.from(modelMap.values()).sort((a, b) => b.total_cost - a.total_cost);

    // 按日期排序每日数据
    const dailyUsage = Array.from(dailyMap.values()).sort((a, b) => a.date.localeCompare(b.date));

    return {
      ...baseData,
      total_sessions: sessions.length,
      total_usage: totalUsage,
      estimated_cost: totalCost,
      sessions,
      daily_usage: dailyUsage.slice(-30),
      by_model: byModel,
      weekly_comparison: {
        current_week: baseData.weekly_comparison.current_week,
        last_week: baseData.weekly_comparison.last_week,
        trends: baseData.weekly_comparison.trends,
      },
    };
  }

  // ========== 公共方法 ==========

  /**
   * 加载可用项目列表
   */
  async function loadProjects() {
    projectsLoading.value = true;
    try {
      const projects = await invoke<ProjectInfo[]>('get_statistics_projects');
      availableProjects.value = projects;

      // 如果没有选中的项目，选择第一个
      if (!selectedProjectPath.value && projects.length > 0) {
        selectedProjectPath.value = projects[0].path;
      }
    } catch (err) {
      console.error('Failed to load projects:', err);
    } finally {
      projectsLoading.value = false;
    }
  }

  /**
   * 设置选中的项目
   */
  async function setSelectedProject(projectPath: string | null) {
    if (selectedProjectPath.value === projectPath) return;

    selectedProjectPath.value = projectPath;

    // 如果切换到 selected 模式，重新加载数据
    if (projectScope.value === 'selected' && projectPath) {
      await initialize(true); // 强制刷新
    }
  }

  /**
   * 预热缓存（应用启动时调用）
   */
  async function warmup() {
    try {
      // 初始化后端缓存
      await invoke('init_stats_cache');
    } catch (err) {
      console.warn('Failed to init stats cache:', err);
    }

    // 异步加载项目列表
    loadProjects();

    // 异步加载统计数据（不等待）
    initialize();
  }

  /**
   * 初始化统计数据
   */
  async function initialize(_forceRefresh = false) {
    loading.value = true;
    error.value = null;

    const maxRetries = 20; // 最多重试20次
    const retryDelay = 500; // 每次间隔500ms

    for (let i = 0; i < maxRetries; i++) {
      try {
        // 获取原始统计数据（后端处理缓存）
        let data: ProjectStatistics;

        if (projectScope.value === 'selected') {
          if (!selectedProjectPath.value) {
            // 如果没有选择项目，先加载项目列表
            await loadProjects();
            if (!selectedProjectPath.value) {
              throw new Error('没有可用的项目');
            }
          }

          data = await invoke<ProjectStatistics>('get_project_statistics', {
            projectPath: selectedProjectPath.value,
          });
        } else {
          data = await invoke<ProjectStatistics>('get_all_projects_statistics');
        }

        // 按日期范围筛选
        const filteredSessions = filterSessionsByDateRange(data.sessions, selectedDateRange.value);
        const filteredStats = aggregateFilteredStatistics(filteredSessions, data);

        statistics.value = filteredStats;
        lastUpdate.value = Date.now();
        initialized.value = true;
        loading.value = false;
        return; // 成功加载，退出
      } catch (err) {
        console.error('Failed to load usage statistics:', err);
        // 如果是 loading 状态，等待后重试
        if (String(err).includes('loading')) {
          if (i < maxRetries - 1) {
            await new Promise(resolve => setTimeout(resolve, retryDelay));
            continue; // 继续重试
          }
          // 最后一次重试仍失败，保持加载状态
          loading.value = true;
          return;
        }
        // 其他错误
        error.value = '加载统计数据失败';
        statistics.value = null;
        loading.value = false;
        return;
      }
    }
  }

  /**
   * 刷新统计数据
   */
  async function refresh() {
    await initialize();
  }

  /**
   * 切换日期范围
   */
  async function setDateRange(range: DateRange) {
    if (selectedDateRange.value === range) return;

    selectedDateRange.value = range;
    await initialize();
  }

  /**
   * 切换标签页
   */
  function setActiveTab(tab: string) {
    activeTab.value = tab;
    loadedTabs.value.add(tab);
  }

  /**
   * 切换项目范围
   */
  async function setProjectScope(scope: ProjectScope) {
    if (projectScope.value === scope) return;

    projectScope.value = scope;

    // 如果切换到 selected 模式，确保有选中的项目
    if (scope === 'selected') {
      if (availableProjects.value.length === 0) {
        await loadProjects();
      }
      if (!selectedProjectPath.value && availableProjects.value.length > 0) {
        selectedProjectPath.value = availableProjects.value[0].path;
      }
    }

    await initialize(); // 切换后重新加载（会使用缓存）
  }

  /**
   * 导出到 CSV
   */
  function exportToCSV() {
    if (!statistics.value) return;

    const headers = ['Session ID', 'Timestamp', 'Model', 'Input Tokens', 'Output Tokens', 'Cache Write', 'Cache Read', 'Total Tokens', 'Cost'];
    const rows = statistics.value.sessions.map(session => [
      session.session_id,
      new Date(session.timestamp).toISOString(),
      session.model,
      session.usage.input_tokens.toString(),
      session.usage.output_tokens.toString(),
      session.usage.cache_write_tokens.toString(),
      session.usage.cache_read_tokens.toString(),
      session.usage.total_tokens.toString(),
      session.cost.toFixed(4),
    ]);

    const csv = [
      headers.join(','),
      ...rows.map(row => row.join(','))
    ].join('\n');

    const blob = new Blob([csv], { type: 'text/csv' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `claude-usage-${statistics.value.project_name}-${new Date().toISOString().split('T')[0]}.csv`;
    a.click();
    URL.revokeObjectURL(url);
  }

  /**
   * 导出到 JSON
   */
  function exportToJSON() {
    if (!statistics.value) return;

    // 驼峰命名转换用于导出
    const exportData = {
      projectPath: statistics.value.project_path,
      projectName: statistics.value.project_name,
      totalSessions: statistics.value.total_sessions,
      totalUsage: statistics.value.total_usage,
      estimatedCost: statistics.value.estimated_cost,
      sessions: statistics.value.sessions,
      dailyUsage: statistics.value.daily_usage,
      weeklyComparison: statistics.value.weekly_comparison,
      byModel: statistics.value.by_model,
      lastUpdated: statistics.value.last_updated,
    };

    const json = JSON.stringify(exportData, null, 2);
    const blob = new Blob([json], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `claude-usage-${statistics.value.project_name}-${new Date().toISOString().split('T')[0]}.json`;
    a.click();
    URL.revokeObjectURL(url);
  }

  return {
    // 状态
    statistics,
    loading,
    error,
    lastUpdate,
    selectedDateRange,
    activeTab,
    loadedTabs,
    projectScope,
    availableProjects,
    selectedProjectPath,
    projectsLoading,
    selectedProject,

    // 计算属性
    hasData,
    formattedTotalCost,
    formattedTotalTokens,
    avgCostPerSession,

    // 方法
    warmup,
    loadProjects,
    setSelectedProject,
    initialize,
    refresh,
    setDateRange,
    setActiveTab,
    setProjectScope,
    exportToCSV,
    exportToJSON,
  };
});
