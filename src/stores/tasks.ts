/**
 * 任务管理 Store
 * 管理任务列表和任务状态
 */

import { defineStore } from 'pinia';
import { ref } from 'vue';
import type { TaskItem, TaskStatus } from '../types';
import { useClaudeStore } from './claude';

/**
 * useTasksStore - 任务管理
 */
export const useTasksStore = defineStore('tasks', () => {
  // 所有任务（按会话 ID 分组）
  const sessionTasks = ref<Map<string, TaskItem[]>>(new Map());

  // 当前选中的任务
  const selectedTaskId = ref<string | null>(null);

  // 加载状态
  const isLoading = ref(false);
  const error = ref<string | null>(null);

  // ========== 计算属性 ==========

  /**
   * 获取会话的任务列表
   */
  function getTasksForSession(sessionId: string): TaskItem[] {
    return sessionTasks.value.get(sessionId) || [];
  }

  /**
   * 按状态分组的任务
   */
  function getTasksByStatus(sessionId: string, status: TaskStatus): TaskItem[] {
    const tasks = getTasksForSession(sessionId);
    return tasks.filter(task => task.status === status);
  }

  /**
   * 获取任务的阻塞关系
   */
  function getBlockingTasks(sessionId: string, taskId: string): TaskItem[] {
    const tasks = getTasksForSession(sessionId);
    const task = tasks.find(t => t.id === taskId);
    if (!task || !task.blockedBy) return [];

    return tasks.filter(t => task.blockedBy?.includes(t.id));
  }

  /**
   * 获取被阻塞的任务
   */
  function getBlockedTasks(sessionId: string, taskId: string): TaskItem[] {
    const tasks = getTasksForSession(sessionId);
    return tasks.filter(t => t.blockedBy?.includes(taskId));
  }

  /**
   * 检查任务是否可以开始
   */
  function canStartTask(sessionId: string, taskId: string): boolean {
    const task = getTasksForSession(sessionId).find(t => t.id === taskId);
    if (!task) return false;

    // 检查所有依赖的任务是否已完成
    if (task.blockedBy && task.blockedBy.length > 0) {
      const tasks = getTasksForSession(sessionId);
      const hasUnfinishedDependencies = task.blockedBy.some(depId => {
        const depTask = tasks.find(t => t.id === depId);
        return depTask && depTask.status !== 'completed';
      });
      if (hasUnfinishedDependencies) return false;
    }

    return task.status === 'pending';
  }

  /**
   * 获取可执行的任务
   */
  function getAvailableTasks(sessionId: string): TaskItem[] {
    const tasks = getTasksForSession(sessionId);
    return tasks.filter(task => canStartTask(sessionId, task.id));
  }

  // ========== Actions ==========

  /**
   * 设置会话任务列表
   */
  function setTasksForSession(sessionId: string, tasks: TaskItem[]): void {
    const newSessionTasks = new Map(sessionTasks.value);
    newSessionTasks.set(sessionId, tasks);
    sessionTasks.value = newSessionTasks;
  }

  /**
   * 添加任务
   */
  function addTask(sessionId: string, task: TaskItem): void {
    const tasks = getTasksForSession(sessionId);
    const newTasks = [...tasks, task];
    setTasksForSession(sessionId, newTasks);
  }

  /**
   * 更新任务
   */
  function updateTask(sessionId: string, taskId: string, updates: Partial<TaskItem>): void {
    const tasks = getTasksForSession(sessionId);
    const newTasks = tasks.map(task =>
      task.id === taskId ? { ...task, ...updates } : task
    );
    setTasksForSession(sessionId, newTasks);
  }

  /**
   * 删除任务
   */
  function removeTask(sessionId: string, taskId: string): void {
    const tasks = getTasksForSession(sessionId);
    const newTasks = tasks.filter(task => task.id !== taskId);
    setTasksForSession(sessionId, newTasks);
  }

  /**
   * 开始任务
   */
  function startTask(sessionId: string, taskId: string): void {
    if (!canStartTask(sessionId, taskId)) return;

    updateTask(sessionId, taskId, {
      status: 'in_progress',
      owner: 'user',
    });
  }

  /**
   * 完成任务
   */
  function completeTask(sessionId: string, taskId: string): void {
    updateTask(sessionId, taskId, {
      status: 'completed',
    });

    selectedTaskId.value = null;

    // 检查是否需要添加未读通知
    // 如果任务所属的 session 不是当前选中的 session，则添加红点通知
    try {
      const claudeStore = useClaudeStore();
      if (claudeStore.currentSessionId !== sessionId) {
        claudeStore.addUnreadTaskCompletion(sessionId);
        console.log('[Tasks] Task completed, added unread notification for session:', sessionId);
      }
    } catch (e) {
      console.error('[Tasks] Failed to add unread notification:', e);
    }
  }

  /**
   * 重置任务状态
   */
  function resetTask(sessionId: string, taskId: string): void {
    updateTask(sessionId, taskId, {
      status: 'pending',
      owner: undefined,
    });
  }

  /**
   * 设置任务阻塞关系
   */
  function setTaskBlocks(sessionId: string, taskId: string, blockedBy: string[]): void {
    updateTask(sessionId, taskId, { blockedBy });
  }

  /**
   * 添加任务阻塞
   */
  function addTaskBlock(sessionId: string, taskId: string, blockId: string): void {
    const task = getTasksForSession(sessionId).find(t => t.id === taskId);
    if (!task) return;

    const blockedBy = task.blockedBy || [];
    if (!blockedBy.includes(blockId)) {
      updateTask(sessionId, taskId, {
        blockedBy: [...blockedBy, blockId],
      });
    }
  }

  /**
   * 移除任务阻塞
   */
  function removeTaskBlock(sessionId: string, taskId: string, blockId: string): void {
    const task = getTasksForSession(sessionId).find(t => t.id === taskId);
    if (!task || !task.blockedBy) return;

    updateTask(sessionId, taskId, {
      blockedBy: task.blockedBy.filter(id => id !== blockId),
    });
  }

  /**
   * 清空会话任务
   */
  function clearSessionTasks(sessionId: string): void {
    const newSessionTasks = new Map(sessionTasks.value);
    newSessionTasks.delete(sessionId);
    sessionTasks.value = newSessionTasks;
  }

  /**
   * 设置选中的任务
   */
  function setSelectedTask(taskId: string | null): void {
    selectedTaskId.value = taskId;
  }

  /**
   * 设置加载状态
   */
  function setLoading(loading: boolean): void {
    isLoading.value = loading;
  }

  /**
   * 设置错误
   */
  function setError(err: string | null): void {
    error.value = err;
  }

  /**
   * 获取任务统计
   */
  function getTaskStats(sessionId: string) {
    const tasks = getTasksForSession(sessionId);
    return {
      total: tasks.length,
      pending: tasks.filter(t => t.status === 'pending').length,
      inProgress: tasks.filter(t => t.status === 'in_progress').length,
      completed: tasks.filter(t => t.status === 'completed').length,
    };
  }

  return {
    // 状态
    sessionTasks,
    selectedTaskId,
    isLoading,
    error,

    // 计算属性方法
    getTasksForSession,
    getTasksByStatus,
    getBlockingTasks,
    getBlockedTasks,
    canStartTask,
    getAvailableTasks,
    getTaskStats,

    // Actions
    setTasksForSession,
    addTask,
    updateTask,
    removeTask,
    startTask,
    completeTask,
    resetTask,
    setTaskBlocks,
    addTaskBlock,
    removeTaskBlock,
    clearSessionTasks,
    setSelectedTask,
    setLoading,
    setError,
  };
});
