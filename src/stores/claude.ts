/**
 * Claude 状态管理 Store
 * 复刻 companion 项目的状态管理逻辑
 */

import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import type {
  Message,
  SessionState,
  PermissionRequest,
  TaskItem,
  ConnectionStatus,
  SessionStatus,
  PermissionMode,
  SdkSessionInfo,
  GitInfo,
  ModelUsageData,
  ThinkingLevel,
  SubagentRuntimeState,
  SubagentToolUseEventPayload,
  SubagentToolInputDeltaEventPayload,
  SubagentToolResultStartEventPayload,
  SubagentToolResultDeltaEventPayload,
  SubagentToolResultCompleteEventPayload,
  SubagentRuntimeCall,
  SubagentRuntimeStatus,
} from '../types';

const SUBAGENT_PARENT_TOOL_NAMES = new Set(['Task', 'Agent']);

function isGenericSubagentDescription(value: string | undefined): boolean {
  const normalized = (value || '').trim();
  return !normalized || normalized === 'Subagent';
}

function pickPreferredSubagentText(existing: string | undefined, fallback: string): string {
  return isGenericSubagentDescription(existing) ? fallback : (existing || fallback);
}

/**
 * useClaudeStore - Claude 状态管理
 *
 * 管理以下状态：
 * - 会话管理
 * - 消息管理
 * - 流式响应
 * - 权限请求
 * - 任务管理
 * - 连接状态
 */
export const useClaudeStore = defineStore('claude', () => {
  // ========== 会话状态 ==========
  const sessions = ref<Map<string, SessionState>>(new Map());
  const sdkSessions = ref<SdkSessionInfo[]>([]);
  const currentSessionId = ref<string | null>(null);

  // ========== 消息状态 ==========
  const messages = ref<Map<string, Message[]>>(new Map());

  // ========== 流式响应状态 ==========
  const streaming = ref<Map<string, string>>(new Map());
  const streamingStartedAt = ref<Map<string, number>>(new Map());
  const streamingOutputTokens = ref<Map<string, number>>(new Map());

  // ========== 权限状态 ==========
  const pendingPermissions = ref<Map<string, Map<string, PermissionRequest>>>(new Map());
  const previousPermissionMode = ref<Map<string, PermissionMode>>(new Map());

  // ========== 连接状态 ==========
  const connectionStatus = ref<Map<string, ConnectionStatus>>(new Map());
  const cliConnected = ref<Map<string, boolean>>(new Map());
  const sessionStatus = ref<Map<string, SessionStatus>>(new Map());

  // ========== 任务状态 ==========
  const sessionTasks = ref<Map<string, TaskItem[]>>(new Map());

  // ========== 会话名称 ==========
  const sessionNames = ref<Map<string, string>>(new Map());

  // ========== Git 信息 ==========
  const sessionGitInfo = ref<Map<string, GitInfo>>(new Map());

  // ========== UI 状态 ==========
  const darkMode = ref<boolean>(false);
  const sidebarOpen = ref<boolean>(true);
  const taskPanelOpen = ref<boolean>(false);
  const showThinking = ref<boolean>(true); // 是否显示思考内容
  const thinkingLevel = ref<ThinkingLevel>('medium');
  const defaultPermissionMode = ref<PermissionMode>('default');
  const streamingEnabled = ref<boolean>(true); // 流式输出固定开启

  // ========== 任务完成通知状态 ==========
  // 记录哪些 session 有未读的任务完成通知
  const unreadTaskCompletions = ref<Set<string>>(new Set());

  // ========== 消息完成通知状态 ==========
  // 记录哪些 session 有未读的消息完成通知（页面切后台时）
  const unreadMessageCompletions = ref<Set<string>>(new Set());

  // ========== Token 使用统计 ==========
  // 记录每个会话的最新 token 使用数据
  const sessionModelUsage = ref<Map<string, ModelUsageData>>(new Map());

  // ========== 子代理运行态 ==========
  const subagentRuntime = ref<Map<string, Map<string, SubagentRuntimeState>>>(new Map());

  // ========== 会话持久化状态 ==========
  // 待持久化的会话（尚未收到后端确认）
  const pendingSessions = ref<Map<string, SessionState>>(new Map());
  // 会话持久化状态追踪
  const sessionPersistStatus = ref<Map<string, 'pending' | 'persisted'>>(new Map());

  // ========== 计算属性 ==========

  /**
   * 当前会话
   */
  const currentSession = computed(() => {
    if (!currentSessionId.value) return null;
    return sessions.value.get(currentSessionId.value) || null;
  });

  /**
   * 当前会话的消息
   */
  const currentMessages = computed(() => {
    if (!currentSessionId.value) return [];
    return messages.value.get(currentSessionId.value) || [];
  });

  /**
   * 当前会话的流式文本
   */
  const currentStreaming = computed(() => {
    if (!currentSessionId.value) return null;
    return streaming.value.get(currentSessionId.value) || null;
  });

  /**
   * 当前会话状态
   */
  const currentSessionStatus = computed(() => {
    if (!currentSessionId.value) return null;
    return sessionStatus.value.get(currentSessionId.value) || null;
  });

  /**
   * 当前连接状态
   */
  const currentConnectionStatus = computed(() => {
    if (!currentSessionId.value) return 'disconnected';
    return connectionStatus.value.get(currentSessionId.value) || 'disconnected';
  });

  /**
   * 当前 CLI 连接状态
   */
  const currentCliConnected = computed(() => {
    if (!currentSessionId.value) return false;
    return cliConnected.value.get(currentSessionId.value) || false;
  });

  /**
   * 当前权限模式
   */
  const currentPermissionMode = computed(() => {
    return currentSession.value?.permissionMode || defaultPermissionMode.value;
  });

  /**
   * 是否处于计划模式
   */
  const isPlanMode = computed(() => {
    return currentPermissionMode.value === 'plan';
  });

  /**
   * 当前会话的任务
   */
  const currentTasks = computed(() => {
    if (!currentSessionId.value) return [];
    return sessionTasks.value.get(currentSessionId.value) || [];
  });

  /**
   * 当前待处理的权限请求
   */
  const currentPendingPermissions = computed(() => {
    if (!currentSessionId.value) return [];
    const perms = pendingPermissions.value.get(currentSessionId.value);
    return perms ? Array.from(perms.values()) : [];
  });

  /**
   * 当前会话的 Git 信息
   */
  const currentGitInfo = computed(() => {
    if (!currentSessionId.value) return null;
    return sessionGitInfo.value.get(currentSessionId.value) || null;
  });

  /**
   * 当前会话的模型使用数据
   */
  const currentModelUsage = computed(() => {
    if (!currentSessionId.value) return null;
    return sessionModelUsage.value.get(currentSessionId.value) || null;
  });

  /**
   * 当前会话的 Token 使用百分比
   */
  const currentTokenPercentage = computed(() => {
    const usage = currentModelUsage.value;
    if (!usage || !usage.contextWindow) return 0;
    const totalTokens = usage.inputTokens + usage.cacheReadInputTokens + usage.cacheCreationInputTokens;
    return Math.min(100, Math.round((totalTokens / usage.contextWindow) * 100));
  });

  // ========== 会话操作 ==========

  /**
   * 设置当前会话
   */
  function setCurrentSession(sessionId: string | null): void {
    currentSessionId.value = sessionId;

    const sessionThinkingLevel = sessionId ? sessions.value.get(sessionId)?.thinkingLevel : null;
    if (sessionThinkingLevel) {
      thinkingLevel.value = sessionThinkingLevel;
    }

    if (sessionId) {
      localStorage.setItem('cc-current-session', sessionId);
    } else {
      localStorage.removeItem('cc-current-session');
    }
  }

  /**
   * 添加会话
   */
  function addSession(session: SessionState): void {
    const newSessions = new Map(sessions.value);
    newSessions.set(session.sessionId, session);

    // 初始化消息列表
    if (!messages.value.has(session.sessionId)) {
      const newMessages = new Map(messages.value);
      newMessages.set(session.sessionId, []);
      messages.value = newMessages;
    }

    sessions.value = newSessions;

    if (currentSessionId.value === session.sessionId && session.thinkingLevel) {
      thinkingLevel.value = session.thinkingLevel;
    }
  }

  /**
   * 更新会话
   */
  function updateSession(sessionId: string, updates: Partial<SessionState>): void {
    const existing = sessions.value.get(sessionId);
    if (existing) {
      const nextSession = { ...existing, ...updates };
      const newSessions = new Map(sessions.value);
      newSessions.set(sessionId, nextSession);
      sessions.value = newSessions;

      if (currentSessionId.value === sessionId && nextSession.thinkingLevel) {
        thinkingLevel.value = nextSession.thinkingLevel;
      }
    }
  }

  /**
   * 移除会话
   */
  function removeSession(sessionId: string): void {
    const newSessions = new Map(sessions.value);
    newSessions.delete(sessionId);
    sessions.value = newSessions;

    // 清理相关数据
    const newMessages = new Map(messages.value);
    newMessages.delete(sessionId);
    messages.value = newMessages;

    const newStreaming = new Map(streaming.value);
    newStreaming.delete(sessionId);
    streaming.value = newStreaming;

    const newStreamingStartedAt = new Map(streamingStartedAt.value);
    newStreamingStartedAt.delete(sessionId);
    streamingStartedAt.value = newStreamingStartedAt;

    const newStreamingOutputTokens = new Map(streamingOutputTokens.value);
    newStreamingOutputTokens.delete(sessionId);
    streamingOutputTokens.value = newStreamingOutputTokens;

    const newConnectionStatus = new Map(connectionStatus.value);
    newConnectionStatus.delete(sessionId);
    connectionStatus.value = newConnectionStatus;

    const newCliConnected = new Map(cliConnected.value);
    newCliConnected.delete(sessionId);
    cliConnected.value = newCliConnected;

    const newSessionStatus = new Map(sessionStatus.value);
    newSessionStatus.delete(sessionId);
    sessionStatus.value = newSessionStatus;

    const newPreviousPermissionMode = new Map(previousPermissionMode.value);
    newPreviousPermissionMode.delete(sessionId);
    previousPermissionMode.value = newPreviousPermissionMode;

    const newPendingPermissions = new Map(pendingPermissions.value);
    newPendingPermissions.delete(sessionId);
    pendingPermissions.value = newPendingPermissions;

    const newSessionTasks = new Map(sessionTasks.value);
    newSessionTasks.delete(sessionId);
    sessionTasks.value = newSessionTasks;

    const newSessionNames = new Map(sessionNames.value);
    newSessionNames.delete(sessionId);
    sessionNames.value = newSessionNames;

    const newSessionGitInfo = new Map(sessionGitInfo.value);
    newSessionGitInfo.delete(sessionId);
    sessionGitInfo.value = newSessionGitInfo;

    const newSessionModelUsage = new Map(sessionModelUsage.value);
    newSessionModelUsage.delete(sessionId);
    sessionModelUsage.value = newSessionModelUsage;

    const newSubagentRuntime = new Map(subagentRuntime.value);
    newSubagentRuntime.delete(sessionId);
    subagentRuntime.value = newSubagentRuntime;

    // 更新 SDK 会话列表
    sdkSessions.value = sdkSessions.value.filter(s => s.sessionId !== sessionId);

    // 清除当前会话
    if (currentSessionId.value === sessionId) {
      currentSessionId.value = null;
      localStorage.removeItem('cc-current-session');
    }

    // 持久化会话名称
    saveSessionNames();
  }

  /**
   * 设置 SDK 会话列表
   */
  function setSdkSessions(sessions: SdkSessionInfo[]): void {
    sdkSessions.value = sessions;
  }

  /**
   * 设置会话名称
   */
  function setSessionName(sessionId: string, name: string): void {
    const newNames = new Map(sessionNames.value);
    newNames.set(sessionId, name);
    sessionNames.value = newNames;
    saveSessionNames();
  }

  // ========== 消息操作 ==========

  /**
   * 添加消息
   */
  function appendMessage(sessionId: string, msg: Message): void {
    const newMessages = new Map(messages.value);
    const list = [...(newMessages.get(sessionId) || []), msg];
    newMessages.set(sessionId, list);
    messages.value = newMessages;
  }

  /**
   * 设置消息列表
   */
  function setMessages(sessionId: string, msgs: Message[]): void {
    const newMessages = new Map(messages.value);
    newMessages.set(sessionId, msgs);
    messages.value = newMessages;
  }

  /**
   * 更新最后一条助手消息
   */
  function updateLastAssistantMessage(
    sessionId: string,
    updater: (msg: Message) => Message
  ): void {
    const newMessages = new Map(messages.value);
    const list = [...(newMessages.get(sessionId) || [])];

    for (let i = list.length - 1; i >= 0; i--) {
      if (list[i].role === 'assistant') {
        list[i] = updater(list[i]);
        break;
      }
    }

    newMessages.set(sessionId, list);
    messages.value = newMessages;
  }

  // ========== 流式响应操作 ==========

  /**
   * 设置流式文本
   */
  function setStreaming(sessionId: string, text: string | null): void {
    const newStreaming = new Map(streaming.value);
    if (text === null) {
      newStreaming.delete(sessionId);
    } else {
      newStreaming.set(sessionId, text);
    }
    streaming.value = newStreaming;
  }

  /**
   * 设置流式统计信息
   */
  function setStreamingStats(
    sessionId: string,
    stats: { startedAt?: number; outputTokens?: number } | null
  ): void {
    const newStreamingStartedAt = new Map(streamingStartedAt.value);
    const newStreamingOutputTokens = new Map(streamingOutputTokens.value);

    if (stats === null) {
      newStreamingStartedAt.delete(sessionId);
      newStreamingOutputTokens.delete(sessionId);
    } else {
      if (stats.startedAt !== undefined) {
        newStreamingStartedAt.set(sessionId, stats.startedAt);
      }
      if (stats.outputTokens !== undefined) {
        newStreamingOutputTokens.set(sessionId, stats.outputTokens);
      }
    }

    streamingStartedAt.value = newStreamingStartedAt;
    streamingOutputTokens.value = newStreamingOutputTokens;
  }

  // ========== 权限操作 ==========

  /**
   * 添加权限请求
   */
  function addPermission(sessionId: string, perm: PermissionRequest): void {
    const newPendingPermissions = new Map(pendingPermissions.value);
    const sessionPerms = new Map(newPendingPermissions.get(sessionId) || []);
    sessionPerms.set(perm.request_id, perm);
    newPendingPermissions.set(sessionId, sessionPerms);
    pendingPermissions.value = newPendingPermissions;
  }

  /**
   * 移除权限请求
   */
  function removePermission(sessionId: string, requestId: string): void {
    const newPendingPermissions = new Map(pendingPermissions.value);
    const sessionPerms = newPendingPermissions.get(sessionId);

    if (sessionPerms) {
      const updated = new Map(sessionPerms);
      updated.delete(requestId);
      if (updated.size > 0) {
        newPendingPermissions.set(sessionId, updated);
      } else {
        newPendingPermissions.delete(sessionId);
      }
    }

    pendingPermissions.value = newPendingPermissions;
  }

  /**
   * 清空指定 session 的权限请求
   */
  function clearSessionPermissions(sessionId: string): void {
    if (!pendingPermissions.value.has(sessionId)) return;

    const newPendingPermissions = new Map(pendingPermissions.value);
    newPendingPermissions.delete(sessionId);
    pendingPermissions.value = newPendingPermissions;
  }

  /**
   * 根据 requestId 查找权限请求所在的 session
   */
  function findPermissionSessionId(requestId: string): string | null {
    for (const [sessionId, sessionPerms] of pendingPermissions.value.entries()) {
      if (sessionPerms.has(requestId)) {
        return sessionId;
      }
    }
    return null;
  }

  /**
   * 获取权限请求
   */
  function getPermission(sessionId: string, requestId: string): PermissionRequest | null {
    return pendingPermissions.value.get(sessionId)?.get(requestId) || null;
  }

  /**
   * 设置之前的权限模式
   */
  function setPreviousPermissionMode(sessionId: string, mode: PermissionMode): void {
    const newModes = new Map(previousPermissionMode.value);
    newModes.set(sessionId, mode);
    previousPermissionMode.value = newModes;
  }

  // ========== 任务操作 ==========

  /**
   * 添加任务
   */
  function addTask(sessionId: string, task: TaskItem): void {
    const newSessionTasks = new Map(sessionTasks.value);
    const tasks = [...(newSessionTasks.get(sessionId) || []), task];
    newSessionTasks.set(sessionId, tasks);
    sessionTasks.value = newSessionTasks;
  }

  /**
   * 设置任务列表
   */
  function setTasks(sessionId: string, tasks: TaskItem[]): void {
    const newSessionTasks = new Map(sessionTasks.value);
    newSessionTasks.set(sessionId, tasks);
    sessionTasks.value = newSessionTasks;
  }

  /**
   * 更新任务
   */
  function updateTask(sessionId: string, taskId: string, updates: Partial<TaskItem>): void {
    const newSessionTasks = new Map(sessionTasks.value);
    const tasks = newSessionTasks.get(sessionId);

    if (tasks) {
      newSessionTasks.set(
        sessionId,
        tasks.map(t => (t.id === taskId ? { ...t, ...updates } : t))
      );
      sessionTasks.value = newSessionTasks;
    }
  }

  // ========== 连接状态操作 ==========

  /**
   * 设置连接状态
   */
  function setConnectionStatus(sessionId: string, status: ConnectionStatus): void {
    const newStatus = new Map(connectionStatus.value);
    newStatus.set(sessionId, status);
    connectionStatus.value = newStatus;
  }

  /**
   * 设置 CLI 连接状态
   */
  function setCliConnected(sessionId: string, connected: boolean): void {
    const newConnected = new Map(cliConnected.value);
    newConnected.set(sessionId, connected);
    cliConnected.value = newConnected;
  }

  /**
   * 设置会话状态
   */
  function setSessionStatus(sessionId: string, status: SessionStatus): void {
    const newStatus = new Map(sessionStatus.value);
    newStatus.set(sessionId, status);
    sessionStatus.value = newStatus;
  }

  /**
   * 设置 Git 信息
   */
  function setSessionGitInfo(sessionId: string, gitInfo: GitInfo): void {
    const newGitInfo = new Map(sessionGitInfo.value);
    newGitInfo.set(sessionId, gitInfo);
    sessionGitInfo.value = newGitInfo;
  }

  /**
   * 设置模型使用数据
   */
  function setSessionModelUsage(sessionId: string, modelUsage: ModelUsageData): void {
    const newModelUsage = new Map(sessionModelUsage.value);
    newModelUsage.set(sessionId, modelUsage);
    sessionModelUsage.value = newModelUsage;
  }

  function getTaskMeta(
    sessionId: string,
    taskToolUseId: string,
  ): { description: string; agentType: string } {
    const sessionMessages = messages.value.get(sessionId) || [];

    for (let i = sessionMessages.length - 1; i >= 0; i -= 1) {
      const message = sessionMessages[i];
      const blocks = message.contentBlocks || [];

      for (const block of blocks) {
        if (block.type !== 'tool_use') continue;
        const toolUseBlock = block as { id?: string; name?: string; input?: Record<string, unknown> };
        if (toolUseBlock.id !== taskToolUseId || !SUBAGENT_PARENT_TOOL_NAMES.has(toolUseBlock.name || '')) continue;

        const input = toolUseBlock.input || {};
        return {
          description: String((input as { description?: unknown }).description || 'Subagent'),
          agentType: String((input as { subagent_type?: unknown }).subagent_type || ''),
        };
      }
    }

    return {
      description: 'Subagent',
      agentType: '',
    };
  }

  function getSubagentCallPreview(call: SubagentRuntimeCall): string {
    const input = call.input || {};

    switch (call.name) {
      case 'Read':
      case 'Edit':
      case 'Write':
        return String((input.file_path || input.path || '') as string);
      case 'Bash':
      case 'Exec':
        return String((input.command || input.cmd || '') as string);
      case 'Search':
      case 'Grep':
        return String((input.query || input.pattern || '') as string);
      case 'Glob':
        return String((input.pattern || '') as string);
      default: {
        const values = Object.values(input)
          .filter((value) => value !== undefined && value !== null)
          .map((value) => (typeof value === 'string' ? value : JSON.stringify(value)))
          .filter((value) => value.trim().length > 0);
        return values[0] || '';
      }
    }
  }

  function recomputeSubagentRuntimeState(state: SubagentRuntimeState): SubagentRuntimeState {
    const calls = [...state.calls].sort((a, b) => a.startedAt - b.startedAt);
    const hasError = calls.some((call) => call.status === 'error' || call.isError);
    const hasRunning = calls.some((call) => call.status === 'running');
    const latestCall = calls[calls.length - 1];
    const latestPreview = latestCall
      ? [latestCall.name, getSubagentCallPreview(latestCall)].filter(Boolean).join(' ')
      : state.latestPreview;
    const completedAt = hasRunning
      ? undefined
      : calls.reduce<number | undefined>((acc, call) => {
        if (!call.completedAt) return acc;
        return acc === undefined ? call.completedAt : Math.max(acc, call.completedAt);
      }, state.completedAt);

    return {
      ...state,
      calls,
      toolCallCount: calls.length,
      latestPreview,
      status: hasRunning ? 'running' : (hasError ? 'error' : 'completed'),
      completedAt,
    };
  }

  function updateSubagentRuntimeState(
    sessionId: string,
    taskToolUseId: string,
    updater: (existing: SubagentRuntimeState | undefined, now: number) => SubagentRuntimeState,
  ): void {
    const now = Date.now();
    const sessionMap = new Map(subagentRuntime.value.get(sessionId) || []);
    const nextState = recomputeSubagentRuntimeState(updater(sessionMap.get(taskToolUseId), now));
    sessionMap.set(taskToolUseId, nextState);

    const nextRuntime = new Map(subagentRuntime.value);
    nextRuntime.set(sessionId, sessionMap);
    subagentRuntime.value = nextRuntime;
  }

  function clearSessionSubagentRuntime(sessionId: string): void {
    const nextRuntime = new Map(subagentRuntime.value);
    nextRuntime.delete(sessionId);
    subagentRuntime.value = nextRuntime;
  }

  function upsertSubagentToolUse(sessionId: string, payload: SubagentToolUseEventPayload): void {
    updateSubagentRuntimeState(sessionId, payload.parentToolUseId, (existing, now) => {
      const meta = getTaskMeta(sessionId, payload.parentToolUseId);
      const startedAt = payload.elapsedTimeSeconds != null
        ? now - Math.max(0, Math.round(payload.elapsedTimeSeconds * 1000))
        : existing?.startedAt ?? now;
      const nextCalls = [...(existing?.calls || [])];
      const callIndex = nextCalls.findIndex((call) => call.id === payload.toolUseId);

      if (callIndex === -1) {
        nextCalls.push({
          id: payload.toolUseId,
          name: payload.toolName || 'Tool',
          input: payload.input || {},
          inputJson: payload.input ? JSON.stringify(payload.input, null, 2) : undefined,
          status: 'running',
          startedAt,
          updatedAt: now,
        });
      } else {
        const existingCall = nextCalls[callIndex];
        nextCalls[callIndex] = {
          ...existingCall,
          name: payload.toolName || existingCall.name,
          input: payload.input ?? existingCall.input,
          inputJson: payload.input
            ? JSON.stringify(payload.input, null, 2)
            : existingCall.inputJson,
          updatedAt: now,
          startedAt: Math.min(existingCall.startedAt, startedAt),
          status: existingCall.status === 'completed' || existingCall.status === 'error'
            ? existingCall.status
            : 'running',
        };
      }

      return {
        taskToolUseId: payload.parentToolUseId,
        description: pickPreferredSubagentText(existing?.description, meta.description),
        agentType: existing?.agentType || meta.agentType,
        status: 'running',
        startedAt,
        latestPreview: existing?.latestPreview,
        toolCallCount: nextCalls.length,
        calls: nextCalls,
      };
    });
  }

  function appendSubagentToolInputDelta(
    sessionId: string,
    payload: SubagentToolInputDeltaEventPayload,
  ): void {
    updateSubagentRuntimeState(sessionId, payload.parentToolUseId, (existing, now) => {
      const meta = getTaskMeta(sessionId, payload.parentToolUseId);
      const nextCalls = [...(existing?.calls || [])];
      const callIndex = nextCalls.findIndex((call) => call.id === payload.toolUseId);

      if (callIndex === -1) {
        nextCalls.push({
          id: payload.toolUseId,
          name: 'Tool',
          input: {},
          inputJson: payload.delta,
          status: 'running',
          startedAt: now,
          updatedAt: now,
        });
      } else {
        const existingCall = nextCalls[callIndex];
        const inputJson = `${existingCall.inputJson || ''}${payload.delta}`;
        let input = existingCall.input;

        try {
          const parsed = JSON.parse(inputJson);
          if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
            input = parsed as Record<string, unknown>;
          }
        } catch {
          // ignore partial JSON
        }

        nextCalls[callIndex] = {
          ...existingCall,
          inputJson,
          input,
          updatedAt: now,
          status: existingCall.status === 'completed' || existingCall.status === 'error'
            ? existingCall.status
            : 'running',
        };
      }

      return {
        taskToolUseId: payload.parentToolUseId,
        description: pickPreferredSubagentText(existing?.description, meta.description),
        agentType: existing?.agentType || meta.agentType,
        status: 'running',
        startedAt: existing?.startedAt ?? now,
        latestPreview: existing?.latestPreview,
        toolCallCount: nextCalls.length,
        calls: nextCalls,
      };
    });
  }

  function startSubagentToolResult(
    sessionId: string,
    payload: SubagentToolResultStartEventPayload,
  ): void {
    updateSubagentRuntimeState(sessionId, payload.parentToolUseId, (existing, now) => {
      const meta = getTaskMeta(sessionId, payload.parentToolUseId);
      const nextCalls = [...(existing?.calls || [])];
      const callIndex = nextCalls.findIndex((call) => call.id === payload.toolUseId);

      if (callIndex === -1) {
        nextCalls.push({
          id: payload.toolUseId,
          name: 'Tool',
          input: {},
          result: payload.content,
          status: payload.isError ? 'error' : 'running',
          isError: payload.isError,
          startedAt: now,
          updatedAt: now,
        });
      } else {
        const existingCall = nextCalls[callIndex];
        nextCalls[callIndex] = {
          ...existingCall,
          result: payload.content,
          isError: payload.isError ?? existingCall.isError,
          status: payload.isError ? 'error' : existingCall.status,
          updatedAt: now,
        };
      }

      return {
        taskToolUseId: payload.parentToolUseId,
        description: pickPreferredSubagentText(existing?.description, meta.description),
        agentType: existing?.agentType || meta.agentType,
        status: 'running',
        startedAt: existing?.startedAt ?? now,
        latestPreview: existing?.latestPreview,
        toolCallCount: nextCalls.length,
        calls: nextCalls,
      };
    });
  }

  function appendSubagentToolResultDelta(
    sessionId: string,
    payload: SubagentToolResultDeltaEventPayload,
  ): void {
    updateSubagentRuntimeState(sessionId, payload.parentToolUseId, (existing, now) => {
      const meta = getTaskMeta(sessionId, payload.parentToolUseId);
      const nextCalls = [...(existing?.calls || [])];
      const callIndex = nextCalls.findIndex((call) => call.id === payload.toolUseId);

      if (callIndex === -1) {
        nextCalls.push({
          id: payload.toolUseId,
          name: 'Tool',
          input: {},
          result: payload.delta,
          status: 'running',
          startedAt: now,
          updatedAt: now,
        });
      } else {
        const existingCall = nextCalls[callIndex];
        nextCalls[callIndex] = {
          ...existingCall,
          result: `${existingCall.result || ''}${payload.delta}`,
          updatedAt: now,
          status: existingCall.status === 'completed' || existingCall.status === 'error'
            ? existingCall.status
            : 'running',
        };
      }

      return {
        taskToolUseId: payload.parentToolUseId,
        description: pickPreferredSubagentText(existing?.description, meta.description),
        agentType: existing?.agentType || meta.agentType,
        status: 'running',
        startedAt: existing?.startedAt ?? now,
        latestPreview: existing?.latestPreview,
        toolCallCount: nextCalls.length,
        calls: nextCalls,
      };
    });
  }

  function completeSubagentToolResult(
    sessionId: string,
    payload: SubagentToolResultCompleteEventPayload,
  ): void {
    updateSubagentRuntimeState(sessionId, payload.parentToolUseId, (existing, now) => {
      const meta = getTaskMeta(sessionId, payload.parentToolUseId);
      const nextCalls = [...(existing?.calls || [])];
      const callIndex = nextCalls.findIndex((call) => call.id === payload.toolUseId);

      if (callIndex === -1) {
        nextCalls.push({
          id: payload.toolUseId,
          name: 'Tool',
          input: {},
          status: 'completed',
          startedAt: now,
          updatedAt: now,
          completedAt: now,
        });
      } else {
        const existingCall = nextCalls[callIndex];
        nextCalls[callIndex] = {
          ...existingCall,
          status: existingCall.isError ? 'error' : 'completed',
          updatedAt: now,
          completedAt: now,
        };
      }

      return {
        taskToolUseId: payload.parentToolUseId,
        description: pickPreferredSubagentText(existing?.description, meta.description),
        agentType: existing?.agentType || meta.agentType,
        status: 'running',
        startedAt: existing?.startedAt ?? now,
        latestPreview: existing?.latestPreview,
        toolCallCount: nextCalls.length,
        calls: nextCalls,
      };
    });
  }

  function finalizeSessionSubagentRuntime(sessionId: string): void {
    const sessionMap = subagentRuntime.value.get(sessionId);
    if (!sessionMap || sessionMap.size === 0) return;

    const now = Date.now();
    const nextSessionMap = new Map<string, SubagentRuntimeState>();

    for (const [taskToolUseId, state] of sessionMap.entries()) {
      const finalizedCalls = state.calls.map((call) => {
        if (call.status !== 'running') return call;
        return {
          ...call,
          status: call.isError ? 'error' as SubagentRuntimeStatus : 'completed' as SubagentRuntimeStatus,
          updatedAt: now,
          completedAt: now,
        };
      });

      nextSessionMap.set(
        taskToolUseId,
        recomputeSubagentRuntimeState({
          ...state,
          calls: finalizedCalls,
          completedAt: now,
        }),
      );
    }

    const nextRuntime = new Map(subagentRuntime.value);
    nextRuntime.set(sessionId, nextSessionMap);
    subagentRuntime.value = nextRuntime;
  }

  // ========== UI 操作 ==========

  /**
   * 设置深色模式
   */
  function setDarkMode(enabled: boolean): void {
    darkMode.value = enabled;
    localStorage.setItem('cc-dark-mode', String(enabled));
  }

  /**
   * 切换深色模式
   */
  function toggleDarkMode(): void {
    darkMode.value = !darkMode.value;
    localStorage.setItem('cc-dark-mode', String(darkMode.value));
  }

  /**
   * 设置侧边栏状态
   */
  function setSidebarOpen(open: boolean): void {
    sidebarOpen.value = open;
  }

  /**
   * 设置任务面板状态
   */
  function setTaskPanelOpen(open: boolean): void {
    taskPanelOpen.value = open;
  }

  /**
   * 设置是否显示思考内容
   */
  function setShowThinking(show: boolean): void {
    showThinking.value = show;
    localStorage.setItem('cc-show-thinking', String(show));
  }

  /**
   * 切换思考内容显示
   */
  function toggleShowThinking(): void {
    showThinking.value = !showThinking.value;
    localStorage.setItem('cc-show-thinking', String(showThinking.value));
  }

  /**
   * 设置思考强度
   */
  function setThinkingLevel(level: ThinkingLevel): void {
    thinkingLevel.value = level;
    localStorage.setItem('cc-thinking-level', level);
  }

  /**
   * 设置默认权限模式
   */
  function setDefaultPermissionMode(mode: PermissionMode): void {
    defaultPermissionMode.value = mode;
    localStorage.setItem('cc-default-permission-mode', mode);
  }

  /**
   * 设置是否启用流式输出
   */
  function setStreamingEnabled(_enabled: boolean): void {
    streamingEnabled.value = true;
    localStorage.removeItem('cc-streaming-enabled');
  }

  /**
   * 切换流式输出
   */
  function toggleStreamingEnabled(): void {
    streamingEnabled.value = true;
    localStorage.removeItem('cc-streaming-enabled');
  }

  // ========== 任务完成通知操作 ==========

  /**
   * 标记 session 有未读的任务完成通知
   */
  function addUnreadTaskCompletion(sessionId: string): void {
    const newSet = new Set(unreadTaskCompletions.value);
    newSet.add(sessionId);
    unreadTaskCompletions.value = newSet;
  }

  /**
   * 清除 session 的未读任务完成通知
   */
  function clearUnreadTaskCompletion(sessionId: string): void {
    const newSet = new Set(unreadTaskCompletions.value);
    newSet.delete(sessionId);
    unreadTaskCompletions.value = newSet;
  }

  /**
   * 检查 session 是否有未读的任务完成通知
   */
  function hasUnreadTaskCompletion(sessionId: string): boolean {
    return unreadTaskCompletions.value.has(sessionId);
  }

  // ========== 消息完成通知操作 ==========

  /**
   * 标记 session 有未读的消息完成通知
   */
  function addUnreadMessageCompletion(sessionId: string): void {
    const newSet = new Set(unreadMessageCompletions.value);
    newSet.add(sessionId);
    unreadMessageCompletions.value = newSet;
  }

  /**
   * 清除 session 的未读消息完成通知
   */
  function clearUnreadMessageCompletion(sessionId: string): void {
    const newSet = new Set(unreadMessageCompletions.value);
    newSet.delete(sessionId);
    unreadMessageCompletions.value = newSet;
  }

  /**
   * 检查 session 是否有未读的消息完成通知
   */
  function hasUnreadMessageCompletion(sessionId: string): boolean {
    return unreadMessageCompletions.value.has(sessionId);
  }

  // ========== 重置操作 ==========

  /**
   * 重置所有状态
   */
  function reset(): void {
    sessions.value = new Map();
    sdkSessions.value = [];
    currentSessionId.value = null;
    messages.value = new Map();
    streaming.value = new Map();
    streamingStartedAt.value = new Map();
    streamingOutputTokens.value = new Map();
    pendingPermissions.value = new Map();
    connectionStatus.value = new Map();
    cliConnected.value = new Map();
    sessionStatus.value = new Map();
    previousPermissionMode.value = new Map();
    sessionTasks.value = new Map();
    sessionNames.value = new Map();
    sessionGitInfo.value = new Map();
    sessionModelUsage.value = new Map();
    subagentRuntime.value = new Map();
  }

  // ========== 辅助函数 ==========

  /**
   * 保存会话名称到 localStorage
   */
  function saveSessionNames(): void {
    try {
      localStorage.setItem('cc-session-names', JSON.stringify(Array.from(sessionNames.value.entries())));
    } catch (e) {
      console.error('Failed to save session names:', e);
    }
  }

  /**
   * 从 localStorage 加载会话名称
   */
  function loadSessionNames(): void {
    try {
      const stored = localStorage.getItem('cc-session-names');
      if (stored) {
        sessionNames.value = new Map(JSON.parse(stored));
      }
    } catch (e) {
      console.error('Failed to load session names:', e);
    }
  }

  /**
   * 从 localStorage 加载深色模式设置
   */
  function loadDarkMode(): void {
    try {
      const stored = localStorage.getItem('cc-dark-mode');
      if (stored !== null) {
        darkMode.value = stored === 'true';
      } else {
        darkMode.value = window.matchMedia('(prefers-color-scheme: dark)').matches;
      }
    } catch (e) {
      console.error('Failed to load dark mode:', e);
    }
  }

  /**
   * 从 localStorage 加载思考内容显示设置
   */
  function loadShowThinking(): void {
    try {
      const stored = localStorage.getItem('cc-show-thinking');
      if (stored !== null) {
        showThinking.value = stored === 'true';
      }
    } catch (e) {
      console.error('Failed to load show thinking:', e);
    }
  }

  /**
   * 从 localStorage 加载思考强度设置
   */
  function loadThinkingLevel(): void {
    try {
      const stored = localStorage.getItem('cc-thinking-level');
      if (stored === 'off' || stored === 'low' || stored === 'medium' || stored === 'high') {
        thinkingLevel.value = stored;
      }
    } catch (e) {
      console.error('Failed to load thinking level:', e);
    }
  }

  /**
   * 从 localStorage 加载默认权限模式设置
   */
  function loadDefaultPermissionMode(): void {
    try {
      const stored = localStorage.getItem('cc-default-permission-mode');
      if (
        stored === 'default'
        || stored === 'acceptEdits'
        || stored === 'bypassPermissions'
        || stored === 'plan'
      ) {
        defaultPermissionMode.value = stored;
      }
    } catch (e) {
      console.error('Failed to load default permission mode:', e);
    }
  }

  /**
   * 从 localStorage 加载流式输出设置
   */
  function loadStreamingEnabled(): void {
    try {
      streamingEnabled.value = true;
      localStorage.removeItem('cc-streaming-enabled');
    } catch (e) {
      console.error('Failed to load streaming enabled:', e);
      streamingEnabled.value = true;
    }
  }

  /**
   * 从 localStorage 加载当前会话 ID
   */
  function loadCurrentSession(): void {
    try {
      const stored = localStorage.getItem('cc-current-session');
      if (stored) {
        currentSessionId.value = stored;
      }
    } catch (e) {
      console.error('Failed to load current session:', e);
    }
  }

  // ========== 会话持久化操作 ==========

  /**
   * 立即在 UI 上创建会话记录（待持久化）
   * 用于在发起会话创建请求时立即显示会话，避免等待后端响应
   */
  function createPendingSession(
    sessionId: string,
    projectPath: string,
    overrides?: {
      permissionMode?: PermissionMode;
      thinkingLevel?: ThinkingLevel;
      providerId?: string | null;
      model?: string | null;
      providerOverrideEnabled?: boolean;
    },
  ): void {
    const now = Date.now();
    const pendingSession: SessionState = {
      sessionId,
      cwd: projectPath,
      permissionMode: overrides?.permissionMode ?? defaultPermissionMode.value,
      thinkingLevel: overrides?.thinkingLevel ?? thinkingLevel.value,
      providerId: overrides?.providerId ?? null,
      model: overrides?.model ?? null,
      providerOverrideEnabled: overrides?.providerOverrideEnabled ?? false,
      createdAt: now,
      updatedAt: now,
    };

    // 添加到待持久化会话
    const newPendingSessions = new Map(pendingSessions.value);
    newPendingSessions.set(sessionId, pendingSession);
    pendingSessions.value = newPendingSessions;

    // 标记为待持久化
    const newStatus = new Map(sessionPersistStatus.value);
    newStatus.set(sessionId, 'pending');
    sessionPersistStatus.value = newStatus;

    // 同时添加到正式会话列表（用于 UI 显示）
    addSession(pendingSession);

    console.log('[ClaudeStore] Created pending session:', sessionId);
  }

  /**
   * 标记会话已持久化
   * 当收到后端确认会话已创建时调用
   */
  function markSessionPersisted(sessionId: string): void {
    const newStatus = new Map(sessionPersistStatus.value);
    newStatus.set(sessionId, 'persisted');
    sessionPersistStatus.value = newStatus;

    // 从待持久化列表中移除
    const newPendingSessions = new Map(pendingSessions.value);
    newPendingSessions.delete(sessionId);
    pendingSessions.value = newPendingSessions;

    console.log('[ClaudeStore] Marked session as persisted:', sessionId);
  }

  /**
   * 检查会话是否待持久化
   */
  function isSessionPending(sessionId: string): boolean {
    return sessionPersistStatus.value.get(sessionId) === 'pending';
  }

  /**
   * 重命名会话ID（用于处理临时UUID到真实sessionID的更新）
   */
  function renameSession(oldSessionId: string, newSessionId: string): void {
    // 更新正式 sessions
    const newSessions = new Map(sessions.value);
    const session = newSessions.get(oldSessionId);
    if (session !== undefined) {
      newSessions.set(newSessionId, {
        ...session,
        sessionId: newSessionId,
      });
      newSessions.delete(oldSessionId);
      sessions.value = newSessions;
    }

    // 更新 sessionPersistStatus
    const newStatus = new Map(sessionPersistStatus.value);
    const status = newStatus.get(oldSessionId);
    if (status !== undefined) {
      newStatus.set(newSessionId, status);
      newStatus.delete(oldSessionId);
      sessionPersistStatus.value = newStatus;
    }

    // 更新 pendingSessions
    const newPendingSessions = new Map(pendingSessions.value);
    const pending = newPendingSessions.get(oldSessionId);
    if (pending !== undefined) {
      newPendingSessions.set(newSessionId, pending);
      newPendingSessions.delete(oldSessionId);
      pendingSessions.value = newPendingSessions;
    }

    // 更新 sessionNames
    const newNames = new Map(sessionNames.value);
    const name = newNames.get(oldSessionId);
    if (name !== undefined) {
      newNames.set(newSessionId, name);
      newNames.delete(oldSessionId);
      sessionNames.value = newNames;
    }

    // 更新 previousPermissionMode
    const newModes = new Map(previousPermissionMode.value);
    const mode = newModes.get(oldSessionId);
    if (mode !== undefined) {
      newModes.set(newSessionId, mode);
      newModes.delete(oldSessionId);
      previousPermissionMode.value = newModes;
    }

    // 更新 messages
    const newMessages = new Map(messages.value);
    const msgList = newMessages.get(oldSessionId);
    if (msgList !== undefined) {
      newMessages.set(newSessionId, msgList);
      newMessages.delete(oldSessionId);
      messages.value = newMessages;
    }

    // 更新 pendingPermissions
    const newPermissions = new Map(pendingPermissions.value);
    const perms = newPermissions.get(oldSessionId);
    if (perms !== undefined) {
      newPermissions.set(newSessionId, perms);
      newPermissions.delete(oldSessionId);
      pendingPermissions.value = newPermissions;
    }

    // 更新 streaming 状态
    if (streaming.value.has(oldSessionId)) {
      const newStreaming = new Map(streaming.value);
      const streamValue = newStreaming.get(oldSessionId)!;
      newStreaming.set(newSessionId, streamValue);
      newStreaming.delete(oldSessionId);
      streaming.value = newStreaming;
    }

    // 更新 connectionStatus
    const newConnectionStatus = new Map(connectionStatus.value);
    const connStatus = newConnectionStatus.get(oldSessionId);
    if (connStatus !== undefined) {
      newConnectionStatus.set(newSessionId, connStatus);
      newConnectionStatus.delete(oldSessionId);
      connectionStatus.value = newConnectionStatus;
      console.log('[ClaudeStore] Renamed connectionStatus:', oldSessionId, '->', newSessionId, connStatus);
    } else {
      console.log('[ClaudeStore] No connectionStatus found for:', oldSessionId);
    }

    // 更新 cliConnected
    const newCliConnected = new Map(cliConnected.value);
    const connected = newCliConnected.get(oldSessionId);
    if (connected !== undefined) {
      newCliConnected.set(newSessionId, connected);
      newCliConnected.delete(oldSessionId);
      cliConnected.value = newCliConnected;
      console.log('[ClaudeStore] Renamed cliConnected:', oldSessionId, '->', newSessionId, connected);
    } else {
      console.log('[ClaudeStore] No cliConnected found for:', oldSessionId);
    }

    // 更新 sessionStatus
    const newSessionStatus = new Map(sessionStatus.value);
    const sessStatus = newSessionStatus.get(oldSessionId);
    if (sessStatus !== undefined) {
      newSessionStatus.set(newSessionId, sessStatus);
      newSessionStatus.delete(oldSessionId);
      sessionStatus.value = newSessionStatus;
    }

    // 更新 streamingStartedAt
    const newStreamingStartedAt = new Map(streamingStartedAt.value);
    const startedAt = newStreamingStartedAt.get(oldSessionId);
    if (startedAt !== undefined) {
      newStreamingStartedAt.set(newSessionId, startedAt);
      newStreamingStartedAt.delete(oldSessionId);
      streamingStartedAt.value = newStreamingStartedAt;
    }

    // 更新 streamingOutputTokens
    const newStreamingOutputTokens = new Map(streamingOutputTokens.value);
    const outputTokens = newStreamingOutputTokens.get(oldSessionId);
    if (outputTokens !== undefined) {
      newStreamingOutputTokens.set(newSessionId, outputTokens);
      newStreamingOutputTokens.delete(oldSessionId);
      streamingOutputTokens.value = newStreamingOutputTokens;
    }

    // 更新 sessionGitInfo
    const newSessionGitInfo = new Map(sessionGitInfo.value);
    const gitInfo = newSessionGitInfo.get(oldSessionId);
    if (gitInfo !== undefined) {
      newSessionGitInfo.set(newSessionId, gitInfo);
      newSessionGitInfo.delete(oldSessionId);
      sessionGitInfo.value = newSessionGitInfo;
    }

    // 更新 sessionModelUsage
    const newSessionModelUsage = new Map(sessionModelUsage.value);
    const modelUsage = newSessionModelUsage.get(oldSessionId);
    if (modelUsage !== undefined) {
      newSessionModelUsage.set(newSessionId, modelUsage);
      newSessionModelUsage.delete(oldSessionId);
      sessionModelUsage.value = newSessionModelUsage;
    }

    const newSubagentRuntime = new Map(subagentRuntime.value);
    const runtime = newSubagentRuntime.get(oldSessionId);
    if (runtime !== undefined) {
      newSubagentRuntime.set(newSessionId, runtime);
      newSubagentRuntime.delete(oldSessionId);
      subagentRuntime.value = newSubagentRuntime;
    }

    // 更新 sessionTasks
    const newSessionTasks = new Map(sessionTasks.value);
    const tasks = newSessionTasks.get(oldSessionId);
    if (tasks !== undefined) {
      newSessionTasks.set(newSessionId, tasks);
      newSessionTasks.delete(oldSessionId);
      sessionTasks.value = newSessionTasks;
    }

    // 如果当前选中的是旧 session，同步切换到新 session
    if (currentSessionId.value === oldSessionId) {
      currentSessionId.value = newSessionId;
      localStorage.setItem('cc-current-session', newSessionId);
    }

    console.log('[ClaudeStore] Renamed session:', oldSessionId, '->', newSessionId);
  }

  // 初始化加载
  loadSessionNames();
  loadDarkMode();
  loadShowThinking();
  loadThinkingLevel();
  loadDefaultPermissionMode();
  loadStreamingEnabled();
  loadCurrentSession();

  return {
    // 状态
    sessions,
    sdkSessions,
    currentSessionId,
    messages,
    streaming,
    streamingStartedAt,
    streamingOutputTokens,
    pendingPermissions,
    connectionStatus,
    cliConnected,
    sessionStatus,
    previousPermissionMode,
    sessionTasks,
    sessionNames,
    sessionGitInfo,
    sessionModelUsage,
    subagentRuntime,
    darkMode,
    sidebarOpen,
    taskPanelOpen,
    showThinking,
    thinkingLevel,
    defaultPermissionMode,
    streamingEnabled,
    unreadTaskCompletions,
    unreadMessageCompletions,
    pendingSessions,
    sessionPersistStatus,

    // 计算属性
    currentSession,
    currentMessages,
    currentStreaming,
    currentSessionStatus,
    currentConnectionStatus,
    currentCliConnected,
    currentPermissionMode,
    isPlanMode,
    currentTasks,
    currentPendingPermissions,
    currentGitInfo,
    currentModelUsage,
    currentTokenPercentage,

    // 会话操作
    setCurrentSession,
    addSession,
    updateSession,
    removeSession,
    setSdkSessions,
    setSessionName,

    // 消息操作
    appendMessage,
    setMessages,
    updateLastAssistantMessage,

    // 流式响应操作
    setStreaming,
    setStreamingStats,

    // 权限操作
    addPermission,
    removePermission,
    clearSessionPermissions,
    findPermissionSessionId,
    getPermission,
    setPreviousPermissionMode,

    // 任务操作
    addTask,
    setTasks,
    updateTask,

    // 连接状态操作
    setConnectionStatus,
    setCliConnected,
    setSessionStatus,
    setSessionGitInfo,
    setSessionModelUsage,
    clearSessionSubagentRuntime,
    upsertSubagentToolUse,
    appendSubagentToolInputDelta,
    startSubagentToolResult,
    appendSubagentToolResultDelta,
    completeSubagentToolResult,
    finalizeSessionSubagentRuntime,

    // UI 操作
    setDarkMode,
    toggleDarkMode,
    setSidebarOpen,
    setTaskPanelOpen,
    setShowThinking,
    toggleShowThinking,
    setThinkingLevel,
    setDefaultPermissionMode,
    setStreamingEnabled,
    toggleStreamingEnabled,

    // 任务完成通知操作
    addUnreadTaskCompletion,
    clearUnreadTaskCompletion,
    hasUnreadTaskCompletion,

    // 消息完成通知操作
    addUnreadMessageCompletion,
    clearUnreadMessageCompletion,
    hasUnreadMessageCompletion,

    // 会话持久化操作
    createPendingSession,
    markSessionPersisted,
    isSessionPending,
    renameSession,

    // 重置
    reset,
  };
});
