<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch, nextTick } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import { Store } from '@tauri-apps/plugin-store';
import { open } from '@tauri-apps/plugin-dialog';
import { useSlashesStore } from '../stores/slashes';
import { useClaudeStore } from '../stores/claude';
import { useProviderStore } from '../stores/provider';
import { HugeiconsIcon } from '@hugeicons/vue';
import { FolderIcon, AddIcon, RefreshIcon, FileImportIcon, Settings01Icon, Search01Icon } from '@hugeicons/core-free-icons';
import ImportProjectsDialog from './ImportProjectsDialog.vue';
import WelcomePage from './WelcomePage.vue';
import Settings from './Settings.vue';
import tauriConfig from '../../src-tauri/tauri.conf.json';
import ProjectDirectoryPanel from './ProjectDirectoryPanel.vue';
import MessageList from './chat/MessageList.vue';
import MessageInput from './chat/MessageInput.vue';
import ThinkingAnimation from './chat/ThinkingAnimation.vue';
import { extractTodoWritePanelState } from '../utils/todoWrite';
import type { Message, ModelUsageData } from './chat';
import type {
  GitInfo,
  OutgoingMessagePayload,
  PermissionMode,
  ThinkingLevel,
  TokenUsage,
  SubagentToolUseEventPayload,
  SubagentToolInputDeltaEventPayload,
  SubagentToolResultStartEventPayload,
  SubagentToolResultDeltaEventPayload,
  SubagentToolResultCompleteEventPayload,
} from '../types';
import { buildRewindSummary, parseRewindTurns, type RewindAction, type RewindTurn } from '../utils/rewind';

// 初始化 stores
const slashesStore = useSlashesStore();
const claudeStore = useClaudeStore();
const providerStore = useProviderStore();

// Session 类型定义（与后端 Rust 结构体匹配）
interface Session {
  id: string;
  created_at: string;
}

interface SessionFileMetadata {
  exists: boolean;
  modifiedAtMs: number | null;
  size: number;
}

// 初始化 Store（使用自定义路径 ~/.aite/projects.json）
let store: Store | null = null;

const getStore = async (): Promise<Store> => {
  if (!store) {
    console.log('[Store] 正在加载 Store...');

    // 确保数据目录存在
    await invoke('ensure_data_dir');

    // 获取数据文件路径
    const dataPath = await invoke<string>('get_data_file_path', { filename: 'projects.json' });

    store = await Store.load(dataPath, {
      defaults: {
        projects: [],
        conversations: [],
        expanded: []
      },
      autoSave: false // 手动控制保存时机
    });
    console.log('[Store] Store 已加载，数据路径:', dataPath);
  }
  return store;
};

interface Conversation {
  id: string;
  title: string;
  time: string;
  timestamp: number; // Unix 时间戳（秒）
  messageCount: number;
  size: string; // 文件大小，如 "1.2 MB"
  projectId: number;
  pinned?: boolean; // 是否固定到顶部
  permissionMode?: PermissionMode;
  providerId?: string | null;
  model?: string | null;
  providerOverrideEnabled?: boolean;
  thinkingLevel?: ThinkingLevel;
}

interface Project {
  id: number;
  name: string;
  path: string;
  description: string;
  lastModified: string;
}

interface PersistedProjectsData {
  projects?: Project[];
  conversations?: Conversation[];
  expanded?: number[];
}

const getContextWindowForModel = (model: string): number => {
  if (model.includes('200k') || model.includes('glm-4')) return 200000;
  if (model.includes('claude-3-5-sonnet')) return 200000;
  if (model.includes('claude-3-5-haiku')) return 200000;
  if (model.includes('gpt-4')) return 128000;
  if (model.includes('gpt-3.5')) return 16000;
  return 128000;
};

const getMaxOutputTokensForModel = (model: string): number => {
  if (model.includes('glm-4')) return 32000;
  if (model.includes('claude-3-5-sonnet')) return 8192;
  if (model.includes('claude-3-5-haiku')) return 8192;
  if (model.includes('gpt-4')) return 4096;
  return 4096;
};

const resolveConversationProvider = (conversation?: Partial<Conversation> | null) => {
  const provider = providerStore.resolveSessionProvider(
    conversation?.providerId,
    conversation?.providerOverrideEnabled,
  );
  const model = providerStore.resolveSessionModel(
    conversation?.providerId,
    conversation?.model,
    conversation?.providerOverrideEnabled,
  );
  return {
    providerId: provider?.id || null,
    model: model || null,
    providerOverrideEnabled: !!conversation?.providerOverrideEnabled,
    providerEnv: provider ? providerStore.buildSessionProviderEnv(provider.id) : null,
  };
};

const getConversationPermissionMode = (conversation?: Partial<Conversation> | null): PermissionMode => {
  return conversation?.permissionMode || claudeStore.defaultPermissionMode;
};

const normalizeTokenUsage = (usage: any): TokenUsage | undefined => {
  if (!usage) return undefined;

  return {
    inputTokens: usage.inputTokens ?? usage.input_tokens ?? 0,
    outputTokens: usage.outputTokens ?? usage.output_tokens ?? 0,
    cacheCreationInputTokens: usage.cacheCreationInputTokens ?? usage.cache_creation_input_tokens ?? 0,
    cacheReadInputTokens: usage.cacheReadInputTokens ?? usage.cache_read_input_tokens ?? 0,
  };
};

const hasNonZeroTokenUsage = (usage?: TokenUsage): boolean => {
  if (!usage) return false;

  return (
    (usage.inputTokens || 0) > 0
    || (usage.outputTokens || 0) > 0
    || (usage.cacheCreationInputTokens || 0) > 0
    || (usage.cacheReadInputTokens || 0) > 0
  );
};

const markHistoricalTurnTokenUsage = (restoredMessages: Message[]) => {
  let lastAssistantWithUsageIndex = -1;

  for (let index = 0; index < restoredMessages.length; index += 1) {
    const message = restoredMessages[index];

    if (message.role === 'user') {
      if (lastAssistantWithUsageIndex >= 0) {
        restoredMessages[lastAssistantWithUsageIndex].showTokenUsage = true;
        lastAssistantWithUsageIndex = -1;
      }
      continue;
    }

    const tokenUsage = message.tokenUsage || message.usage;
    if (message.role === 'assistant' && hasNonZeroTokenUsage(tokenUsage)) {
      lastAssistantWithUsageIndex = index;
    }
  }

  if (lastAssistantWithUsageIndex >= 0) {
    restoredMessages[lastAssistantWithUsageIndex].showTokenUsage = true;
  }
};

const resolveFinalTokenUsage = (
  streamEndUsage?: TokenUsage,
  existingUsage?: TokenUsage,
): TokenUsage | undefined => {
  if (hasNonZeroTokenUsage(streamEndUsage)) {
    return streamEndUsage;
  }

  if (hasNonZeroTokenUsage(existingUsage)) {
    return existingUsage;
  }

  return streamEndUsage ?? existingUsage;
};

const unwrapModelUsageEntry = (backendModelUsage: any, model?: string) => {
  if (!backendModelUsage) return undefined;

  const isSingleUsage =
    backendModelUsage.inputTokens !== undefined
    || backendModelUsage.input_tokens !== undefined
    || backendModelUsage.contextWindow !== undefined
    || backendModelUsage.context_window !== undefined;

  if (isSingleUsage) return backendModelUsage;

  if (model && backendModelUsage[model]) {
    return backendModelUsage[model];
  }

  return Object.values(backendModelUsage)[0];
};

const buildModelUsageData = (usage: any, model: string, backendModelUsage?: any): ModelUsageData => {
  const normalizedUsage = normalizeTokenUsage(usage);
  const normalizedBackendUsage = unwrapModelUsageEntry(backendModelUsage, model);

  return {
    // ResultMessage.usage 表示本次响应的真实 token 用量；modelUsage 可能是模型维度累计值，
    // 这里只把后者当作上下文窗口等元信息来源，避免输入框圆环被累计值放大到 100%。
    inputTokens: normalizedUsage?.inputTokens ?? normalizedBackendUsage?.inputTokens ?? normalizedBackendUsage?.input_tokens ?? 0,
    outputTokens: normalizedUsage?.outputTokens ?? normalizedBackendUsage?.outputTokens ?? normalizedBackendUsage?.output_tokens ?? 0,
    cacheReadInputTokens: normalizedUsage?.cacheReadInputTokens ?? normalizedBackendUsage?.cacheReadInputTokens ?? normalizedBackendUsage?.cache_read_input_tokens ?? 0,
    cacheCreationInputTokens: normalizedUsage?.cacheCreationInputTokens ?? normalizedBackendUsage?.cacheCreationInputTokens ?? normalizedBackendUsage?.cache_creation_input_tokens ?? 0,
    contextWindow: normalizedBackendUsage?.contextWindow ?? normalizedBackendUsage?.context_window ?? getContextWindowForModel(model),
    maxOutputTokens: normalizedBackendUsage?.maxOutputTokens ?? normalizedBackendUsage?.max_output_tokens ?? getMaxOutputTokensForModel(model),
    costUSD: normalizedBackendUsage?.costUSD ?? normalizedBackendUsage?.cost_usd ?? 0,
    model: normalizedBackendUsage?.model ?? model,
  };
};

const restoreHistoryMessages = (
  historyMessages: any[],
  fallbackModel?: string | null,
): {
  restoredMessages: Message[];
  latestHistoryModelUsage: ModelUsageData | null;
} => {
  let latestHistoryModelUsage: ModelUsageData | null = null;

  const restoredMessages = historyMessages.map((msg: any) => {
    const tokenUsage = normalizeTokenUsage(msg.tokenUsage || msg.usage);
    const model = msg.model || fallbackModel || undefined;

    if (msg.role === 'assistant' && tokenUsage && model) {
      latestHistoryModelUsage = buildModelUsageData(
        tokenUsage,
        model,
        msg.modelUsage || msg.model_usage,
      );
    }

    return {
      id: msg.id,
      role: msg.role,
      content: typeof msg.content === 'string' ? msg.content : JSON.stringify(msg.content),
      contentBlocks: msg.contentBlocks,
      tool_calls: msg.toolCalls || [],
      toolResults: msg.toolResults || {},
      toolResultErrors: msg.toolResultErrors || {},
      timestamp: normalizeMessageTimestamp(msg.timestamp ?? msg.createdAt),
      checkpointUuid: msg.checkpointUuid,
      parentToolUseId: msg.parent_tool_use_id ?? msg.parentToolUseId,
      model,
      tokenUsage,
      usage: tokenUsage,
      showTokenUsage: false,
    };
  });

  markHistoricalTurnTokenUsage(restoredMessages);

  return {
    restoredMessages,
    latestHistoryModelUsage,
  };
};

const buildHistoryLoadErrorMessage = (error: unknown): Message => ({
  id: 'history-load-error',
  role: 'assistant',
  content: `无法加载会话历史: ${error}`,
  timestamp: Date.now(),
});

const SESSION_FILE_POLL_INTERVAL_MS = 5000;

// 展开状态管理
const expandedProjects = ref<Set<number>>(new Set());

// 项目列表（从导入添加）
const projects = ref<Project[]>([]);

// 聊天记录关联到项目
const conversations = ref<Conversation[]>([]);

const isSavedDataHydrating = ref(true);
const hasLoadedSavedData = ref(false);

const buildEmptySaveLogEntry = (source: string) => {
  return [
    new Date().toISOString(),
    '[projects-empty-save]',
    `source=${source}`,
    `projects=${projects.value.length}`,
    `conversations=${conversations.value.length}`,
    `expanded=${expandedProjects.value.size}`,
    `selectedConversation=${selectedConversation.value?.id || 'none'}`,
  ].join(' ');
};

// 加载保存的数据
const loadSavedData = async () => {
  try {
    console.log('[Store] 正在加载保存的数据...');
    const s = await getStore();
    console.log('[Store] Store 实例已获取');

    let persistedData: PersistedProjectsData = {};

    try {
      const rawData = await invoke<string | null>('read_data_file', { filename: 'projects.json' });
      if (rawData) {
        persistedData = JSON.parse(rawData) as PersistedProjectsData;
        console.log('[Store] 已从原始文件加载 projects.json');
      }
    } catch (rawError) {
      console.error('[Store] 读取原始 projects.json 失败，将回退到 Store:', rawError);
    }

    // 加载项目列表
    const savedProjects = persistedData.projects ?? await s.get<Project[]>('projects');
    console.log('[Store] 已加载的项目:', savedProjects?.length || 0);
    if (savedProjects) {
      projects.value = savedProjects;
      displayProjects.value = [...savedProjects];
    }

    // 加载对话记录
    const savedConversations = persistedData.conversations ?? await s.get<Conversation[]>('conversations');
    console.log('[Store] 已加载的对话:', savedConversations?.length || 0);
    if (savedConversations) {
      // 兼容旧数据：为没有 timestamp 字段的会话添加默认值
      // 兼容旧数据：为没有 pinned 字段的会话添加默认值 false
      conversations.value = savedConversations.map((conv: any) => ({
        ...conv,
        timestamp: conv.timestamp || 0,
        pinned: conv.pinned || false,
        providerId: conv.providerId || null,
        model: conv.model || null,
        providerOverrideEnabled: conv.providerOverrideEnabled || false,
        thinkingLevel: conv.thinkingLevel || 'medium',
      }));
    }

    // 加载展开状态
    const savedExpanded = persistedData.expanded ?? await s.get<number[]>('expanded');
    console.log('[Store] 已加载的展开状态:', savedExpanded?.length || 0);
    if (savedExpanded) {
      expandedProjects.value = new Set(savedExpanded);
    }

    console.log('[Store] 数据加载完成:', {
      projects: projects.value.length,
      conversations: conversations.value.length,
      expanded: expandedProjects.value.size
    });
    hasLoadedSavedData.value = true;
  } catch (e) {
    console.error('[Store] 加载数据失败:', e);
  } finally {
    isSavedDataHydrating.value = false;
  }
};

// 保存数据到 Store
const saveData = async (source = 'unknown') => {
  try {
    if (isSavedDataHydrating.value || !hasLoadedSavedData.value) {
      console.log('[Store] 跳过保存，等待保存数据加载完成');
      return;
    }

    console.log('[Store] 正在保存数据...');
    const s = await getStore();

    await s.set('projects', projects.value);

    await s.set('conversations', conversations.value);

    await s.set('expanded', Array.from(expandedProjects.value));

    const isEmptySave = projects.value.length === 0
      && conversations.value.length === 0
      && expandedProjects.value.size === 0;

    if (isEmptySave) {
      const logEntry = buildEmptySaveLogEntry(source);
      console.warn('[Store] 检测到即将写入空的 projects.json:', logEntry);
      await invoke('append_data_log', {
        filename: 'projects-save.log',
        content: logEntry,
      });
    }

    await invoke('backup_data_file', { filename: 'projects.json' });
    await s.save(); // 持久化到文件
  } catch (e) {
    console.error('[Store] 保存数据失败:', e);
    throw e;
  }
};

// 监听数据变化并自动保存
watch([projects, conversations, expandedProjects], async () => {
  if (isSavedDataHydrating.value || !hasLoadedSavedData.value) {
    return;
  }
  await saveData('watch');
}, { deep: true });

const selectedConversation = ref<Conversation | null>(null);

// 工作目录是否已设置
const workingDirectorySet = ref(false);

// 当前会话 ID（用于新建对话时）
const currentSessionId = ref<string | null>(null);

const resolveSessionModelForUsage = (sessionId: string): string => {
  const sessionModel = claudeStore.sessions.get(sessionId)?.model;
  if (sessionModel) return sessionModel;

  const sessionMessages = sessionId === currentSessionId.value
    ? messages.value
    : (claudeStore.messages.get(sessionId) || []);
  for (let index = sessionMessages.length - 1; index >= 0; index -= 1) {
    const messageModel = sessionMessages[index].model;
    if (messageModel) return messageModel;
  }

  if (selectedConversation.value?.id === sessionId && selectedConversation.value.model) {
    return selectedConversation.value.model;
  }

  return 'unknown';
};

// 会话连接状态管理
// conversation ID -> session ID 的映射
const conversationSessionMap = ref<Map<string, string>>(new Map());
// project ID -> 未发送首条消息的草稿会话
const projectDraftConversations = ref<Map<number, Conversation>>(new Map());

// 分栏拖拽相关
const leftPanelWidth = ref(280);
const isResizing = ref(false);
const minLeftWidth = 280;
const maxLeftWidth = 600;
const isWorkspaceVisible = ref(false);
const workspacePanelWidth = ref(760);
const isWorkspaceResizing = ref(false);
const currentGitInfo = ref<GitInfo | null>(null);
const chatContentShellRef = ref<HTMLElement | null>(null);

// 拖拽状态
const resizeStartX = ref(0);
const resizeStartWidth = ref(0);
const workspaceResizeStartX = ref(0);
const workspaceResizeStartWidth = ref(760);
const minWorkspacePanelWidth = 320;
const maxWorkspacePanelWidth = 1200;
const minChatMainColumnWidth = 420;

// 导入对话框相关
const importDialogOpen = ref(false);

// 刷新状态管理
const refreshingProjects = ref<Set<number>>(new Set()); // 正在刷新的项目ID集合
let autoRefreshInterval: number | null = null; // 自动刷新定时器

const draggedTabSessionId = ref<string | null>(null);
const dragOverTabSessionId = ref<string | null>(null);
const dragOverTabPosition = ref<'before' | 'after' | null>(null);
const suppressTabClick = ref(false);

// 项目拖拽相关状态
const draggedProjectId = ref<number | null>(null);
const dragOverProjectId = ref<number | null>(null);
const draggedElement = ref<{ top: number; left: number; width: number; height: number } | null>(null);
const projectsListBounds = ref<{ top: number; bottom: number; left: number; width: number } | null>(null);
const projectMouseDownPosition = ref<{ x: number; y: number } | null>(null);
const isMouseDown = ref(false);
const displayProjects = ref<Project[]>([]); // 显示用的项目列表（拖拽时顺序会变）
const suppressProjectClick = ref(false);
const isSettingsVisible = ref(false);
const appVersion = tauriConfig.version;
const projectSearchQuery = ref('');
const projectSortMode = ref<'project' | 'time' | 'chat'>('project');
const isProjectSortMenuOpen = ref(false);
const projectSortMenuRef = ref<HTMLElement | null>(null);
const projectSortButtonRef = ref<HTMLElement | null>(null);

const projectSortOptions = [
  { value: 'project' as const, label: '按项目' },
  { value: 'time' as const, label: '时间顺序列表' },
  { value: 'chat' as const, label: '聊天优先' },
];

const normalizeProjectSearch = (value: string): string => value.trim().toLocaleLowerCase();

const matchesProjectSearch = (projectName: string, query: string): boolean => {
  const normalizedQuery = normalizeProjectSearch(query);
  if (!normalizedQuery) return true;

  const normalizedName = projectName.toLocaleLowerCase();
  if (normalizedName.includes(normalizedQuery)) return true;

  let queryIndex = 0;
  for (const char of normalizedName) {
    if (char === normalizedQuery[queryIndex]) {
      queryIndex += 1;
      if (queryIndex === normalizedQuery.length) return true;
    }
  }

  return false;
};

const isProjectSearchActive = computed(() => normalizeProjectSearch(projectSearchQuery.value).length > 0);

const getProjectLatestConversationTimestamp = (projectId: number): number => {
  let latestTimestamp = 0;
  for (const conversation of conversations.value) {
    if (conversation.projectId !== projectId) continue;
    latestTimestamp = Math.max(latestTimestamp, conversation.timestamp || 0);
  }
  return latestTimestamp;
};

const filteredDisplayProjects = computed(() => {
  const filtered = displayProjects.value.filter(project =>
    matchesProjectSearch(project.name, projectSearchQuery.value)
  );

  if (projectSortMode.value === 'project') {
    return filtered;
  }

  const sorted = [...filtered];

  if (projectSortMode.value === 'time') {
    sorted.sort((a, b) => {
      const timeDiff = new Date(b.lastModified).getTime() - new Date(a.lastModified).getTime();
      if (timeDiff !== 0) return timeDiff;
      return a.name.localeCompare(b.name, 'zh-CN');
    });
    return sorted;
  }

  sorted.sort((a, b) => {
    const chatDiff = getProjectLatestConversationTimestamp(b.id) - getProjectLatestConversationTimestamp(a.id);
    if (chatDiff !== 0) return chatDiff;

    const timeDiff = new Date(b.lastModified).getTime() - new Date(a.lastModified).getTime();
    if (timeDiff !== 0) return timeDiff;

    return a.name.localeCompare(b.name, 'zh-CN');
  });
  return sorted;
});

const currentProjectSortLabel = computed(() => {
  return projectSortOptions.find(option => option.value === projectSortMode.value)?.label || '按项目';
});

const toggleProjectSortMenu = () => {
  isProjectSortMenuOpen.value = !isProjectSortMenuOpen.value;
};

const selectProjectSortMode = (mode: 'project' | 'time' | 'chat') => {
  projectSortMode.value = mode;
  isProjectSortMenuOpen.value = false;
};

const handleProjectSortOutsideClick = (event: MouseEvent) => {
  if (!isProjectSortMenuOpen.value) return;

  const target = event.target as Node | null;
  if (projectSortMenuRef.value?.contains(target) || projectSortButtonRef.value?.contains(target)) {
    return;
  }

  isProjectSortMenuOpen.value = false;
};

// 监听 projects 变化，同步到 displayProjects
watch(() => projects.value, (newProjects) => {
  if (!isDragging) {
    displayProjects.value = [...newProjects];
  }
}, { deep: true });

// 获取项目在鼠标下方的插入位置
const getDropIndex = (clientY: number): number => {
  // 获取被拖拽项目在 displayProjects 中的当前索引
  const draggedIndex = displayProjects.value.findIndex(p => p.id === draggedProjectId.value);
  if (draggedIndex === -1) return 0;

  // 如果鼠标超出项目列表范围，保持当前位置
  if (!projectsListBounds.value) return draggedIndex;

  const { top, bottom } = projectsListBounds.value;

  // 如果鼠标在项目列表上方，返回 0（插入到最前面）
  if (clientY < top - 20) return 0;

  // 如果鼠标在项目列表下方，返回末尾（插入到最后面）
  if (clientY > bottom + 20) return displayProjects.value.length;

  // 鼠标在项目列表范围内，正常计算
  const elements = document.querySelectorAll('.project-wrapper:not(.dragging-ghost):not(.is-dragged)');

  for (let i = 0; i < elements.length; i++) {
    const rect = elements[i].getBoundingClientRect();
    // 如果鼠标在项目的上半部分，插入到当前位置
    // 如果在下半部分，插入到下一个位置
    if (clientY >= rect.top && clientY <= rect.bottom) {
      const insertIndex = clientY < rect.top + rect.height / 2 ? i : i + 1;

      // 将其他项目的索引转换为完整列表的索引
      // 如果插入位置在被拖拽项目的原始位置之前，直接返回
      // 如果在之后，需要 +1（因为被拖拽项目占用了一个位置）
      if (insertIndex <= draggedIndex) {
        return insertIndex;
      } else {
        return insertIndex + 1;
      }
    }
  }

  return draggedIndex;
};

// 项目鼠标按下
const handleProjectMouseDown = (event: MouseEvent, projectId: number) => {
  if (isProjectSearchActive.value) return;
  if (projectSortMode.value !== 'project') return;
  if (event.button !== 0) return;

  const target = event.target as HTMLElement | null;
  if (target?.closest('button')) return;

  event.preventDefault();

  isMouseDown.value = true;
  draggedProjectId.value = projectId;
  projectMouseDownPosition.value = {
    x: event.clientX,
    y: event.clientY
  };

  // 获取被拖拽元素的位置信息
  const currentTarget = event.currentTarget as HTMLElement;
  const rect = currentTarget.getBoundingClientRect();

  // 获取项目列表容器的位置和宽度
  const projectsList = document.querySelector('.projects-list');
  const listRect = projectsList?.getBoundingClientRect();

  draggedElement.value = {
    top: rect.top,
    left: rect.left,
    width: rect.width,
    height: rect.height
  };

  if (listRect) {
    projectsListBounds.value = {
      top: listRect.top,
      bottom: listRect.bottom,
      left: listRect.left,
      width: listRect.width
    };
  }

  console.log('[拖拽] mousedown, projectId:', projectId);
  document.addEventListener('mousemove', handleMouseMove, { passive: false });
  document.addEventListener('mouseup', handleMouseUp);
};

// 全局鼠标移动
const handleMouseMove = (event: MouseEvent) => {
  if (!isMouseDown.value || draggedProjectId.value === null) return;

  // 移动超过阈值才开始拖拽
  if (!isDragging) {
    const startPosition = projectMouseDownPosition.value;
    if (!startPosition) return;

    const deltaX = event.clientX - startPosition.x;
    const deltaY = event.clientY - startPosition.y;
    const moveDistance = Math.hypot(deltaX, deltaY);

    if (moveDistance < 6) {
      return;
    }
    isDragging = true;
    suppressProjectClick.value = true;
  }

  event.preventDefault();

  // 更新被拖拽元素的位置（用于 ghost 元素）
  const width = draggedElement.value?.width || 0;
  const height = draggedElement.value?.height || 42;

  draggedElement.value = {
    top: event.clientY - height / 2,
    left: draggedElement.value?.left || 0,
    width: width,
    height: height
  };

  // 计算应该插入的位置
  const dropIndex = getDropIndex(event.clientY);

  // 获取被拖拽的项目
  const draggedProject = projects.value.find(p => p.id === draggedProjectId.value);
  if (!draggedProject) return;

  // 创建新的显示顺序：排除被拖拽的项目，然后插入到新位置
  const otherProjects = displayProjects.value.filter(p => p.id !== draggedProjectId.value);
  const newOrder = [...otherProjects];
  newOrder.splice(dropIndex, 0, draggedProject);

  // 只在顺序真正改变时更新
  const currentOrder = displayProjects.value.map(p => p.id).join(',');
  const newOrderIds = newOrder.map(p => p.id).join(',');

  if (currentOrder !== newOrderIds) {
    displayProjects.value = newOrder;
  }
};

// 全局鼠标松开
const handleMouseUp = (_event: MouseEvent) => {
  if (!isMouseDown.value) return;

  document.removeEventListener('mousemove', handleMouseMove);
  document.removeEventListener('mouseup', handleMouseUp);

  const draggedId = draggedProjectId.value;

  if (isDragging && draggedId !== null) {
    // 应用新的顺序
    const oldOrder = projects.value.map(p => p.id).join(',');
    const newOrder = displayProjects.value.map(p => p.id).join(',');

    if (oldOrder !== newOrder) {
      projects.value = [...displayProjects.value];
      saveData();
      console.log('[拖拽] 项目已移动');
    }
  }

  // 重置状态
  isMouseDown.value = false;
  isDragging = false;
  draggedProjectId.value = null;
  dragOverProjectId.value = null;
  draggedElement.value = null;
  projectMouseDownPosition.value = null;
  projectsListBounds.value = null;

  if (!suppressProjectClick.value) {
    return;
  }

  window.setTimeout(() => {
    suppressProjectClick.value = false;
  }, 0);
};


// 打开导入对话框
const openImportDialog = () => {
  importDialogOpen.value = true;
};

// 打开文件夹选择框并添加到项目列表
const openFolder = async () => {
  try {
    const selected = await open({
      directory: true,
      multiple: false,
      title: '选择项目文件夹'
    });

    if (selected && typeof selected === 'string') {
      // 检查项目是否已存在
      const existingIndex = projects.value.findIndex(p => p.path === selected);

      if (existingIndex === -1) {
        // 项目不存在，添加新项目
        const projectId = Date.now();
        const pathParts = selected.split(/[/\\]/);
        const projectName = pathParts[pathParts.length - 1] || selected;

        const newProject: Project = {
          id: projectId,
          name: projectName,
          path: selected,
          description: '本地项目',
          lastModified: new Date().toISOString().split('T')[0]
        };
        projects.value.push(newProject);

        // 按最后修改时间排序
        projects.value.sort((a, b) =>
          new Date(b.lastModified).getTime() - new Date(a.lastModified).getTime()
        );

        console.log('项目已添加:', newProject);
      } else {
        console.log('项目已存在:', selected);
      }
    }
  } catch (error) {
    console.error('选择文件夹失败:', error);
  }
};

// 处理导入成功
const onProjectImported = async (importedProject: any) => {
  console.log('项目已导入:', importedProject);

  // 检查项目是否已存在（通过 cwd 路径判断）
  const existingIndex = projects.value.findIndex(p => p.path === importedProject.cwd);

  let projectId: number;

  if (existingIndex === -1) {
    // 项目不存在，添加新项目
    projectId = Date.now(); // 简单生成唯一 ID
    const newProject: Project = {
      id: projectId,
      name: importedProject.project_name,
      path: importedProject.cwd,
      description: `${importedProject.session_count} 个会话`,
      lastModified: new Date().toISOString().split('T')[0]
    };
    projects.value.push(newProject);

    // 按最后修改时间排序
    projects.value.sort((a, b) =>
      new Date(b.lastModified).getTime() - new Date(a.lastModified).getTime()
    );
  } else {
    // 项目已存在，使用现有项目ID
    projectId = projects.value[existingIndex].id;
    // 更新最后修改时间
    projects.value[existingIndex].lastModified = new Date().toISOString().split('T')[0];
  }

  // 添加会话记录到对话列表
  if (importedProject.sessions && Array.isArray(importedProject.sessions)) {
    importedProject.sessions.forEach((session: any) => {
      // 检查会话是否已存在
      const existingConv = conversations.value.find(c => c.id === session.session_id);

      if (!existingConv) {
        // 保存原始时间戳（秒）
        const timestamp = parseInt(session.updated_at || session.created_at || '0');

        conversations.value.push({
          id: session.session_id,
          title: session.title || 'Untitled',
          time: '', // 将由 formatRelativeTime 动态计算
          timestamp: timestamp,
          messageCount: session.message_count || 0,
          size: formatFileSize(session.file_size || 0),
          projectId: projectId,
          pinned: false, // 新会话默认不固定
          thinkingLevel: 'medium',
        });
      }
    });

    // 按时间排序对话（新的在前）
    conversations.value.sort((a, b) => b.id.localeCompare(a.id));
  }

  // 显式保存数据到 Store
  try {
    console.log('保存导入的项目数据...');
    await saveData();
    console.log('项目数据已保存');
  } catch (e) {
    console.error('保存项目数据失败:', e);
  }
};

// 格式化文件大小
function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

// 格式化相对时间
function formatRelativeTime(timestamp: number): string {
  if (timestamp === 0) return '现在';

  const now = Math.floor(Date.now() / 1000);
  const diff = now - timestamp;

  const seconds = diff;
  const minutes = Math.floor(seconds / 60);
  const hours = Math.floor(minutes / 60);
  const days = Math.floor(hours / 24);
  const weeks = Math.floor(days / 7);
  const months = Math.floor(days / 30);

  if (seconds < 60) return '现在';
  if (minutes < 60) return `${minutes}分钟前`;
  if (hours < 24) return `${hours}小时前`;
  if (days < 7) return `${days}天前`;
  if (weeks < 4) return `${weeks}周前`;
  return `${months}个月前`;
}

function normalizeMessageTimestamp(value: unknown): number {
  if (typeof value === 'number' && Number.isFinite(value)) {
    return value < 1_000_000_000_000 ? value * 1000 : value;
  }

  if (typeof value === 'string') {
    const trimmed = value.trim();
    if (!trimmed) return Date.now();

    const numeric = Number(trimmed);
    if (!Number.isNaN(numeric)) {
      return normalizeMessageTimestamp(numeric);
    }

    const parsed = Date.parse(trimmed);
    if (!Number.isNaN(parsed)) {
      return parsed;
    }
  }

  return Date.now();
}

function getErrorMessage(error: unknown): string {
  if (error instanceof Error && error.message) {
    return error.message;
  }
  if (typeof error === 'string' && error.trim()) {
    return error.trim();
  }
  return '未知错误';
}

function summarizeIncomingMessage(msg: any): string {
  const messageData = msg?.message || msg || {};
  const role = String(messageData.role ?? msg?.role ?? 'unknown');
  const messageId = String(messageData.id ?? msg?.id ?? msg?.uuid ?? 'unknown');
  const rawContent = messageData.content;

  let contentSummary = '';
  if (typeof rawContent === 'string') {
    contentSummary = rawContent;
  } else if (Array.isArray(rawContent)) {
    contentSummary = rawContent
      .map((block: any) => {
        if (!block || typeof block !== 'object') return String(block ?? '');
        if (typeof block.text === 'string') return block.text;
        if (typeof block.content === 'string') return block.content;
        if (typeof block.type === 'string') return `[${block.type}]`;
        return '[unknown block]';
      })
      .join(' ');
  } else if (rawContent && typeof rawContent === 'object') {
    if (Array.isArray((rawContent as any).content)) {
      contentSummary = `[content blocks: ${(rawContent as any).content.length}]`;
    } else if (Array.isArray((rawContent as any).blocks)) {
      contentSummary = `[content blocks: ${(rawContent as any).blocks.length}]`;
    } else {
      contentSummary = '[structured content]';
    }
  }

  const trimmedSummary = contentSummary.trim();
  const preview = trimmedSummary
    ? (trimmedSummary.length > 120 ? `${trimmedSummary.slice(0, 120)}...` : trimmedSummary)
    : '无可预览内容';

  return `角色: ${role}，ID: ${messageId}，预览: ${preview}`;
}

function appendMessageParseError(msg: any, error: unknown) {
  const errorText = getErrorMessage(error);
  const summary = summarizeIncomingMessage(msg);

  messages.value.push({
    id: `parse_error_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`,
    role: 'system',
    content: `消息解析异常，已跳过该条消息并继续处理后续消息。\n${summary}\n错误: ${errorText}`,
    timestamp: Date.now(),
  });
}

// 获取项目的聊天记录（pinned 的优先，然后按时间戳降序排序，最新的在前）
const getProjectConversations = (projectId: number) => {
  const convs = conversations.value.filter(c => c.projectId === projectId);
  return convs.sort((a, b) => {
    // pinned 的排在前面
    if (a.pinned && !b.pinned) return -1;
    if (!a.pinned && b.pinned) return 1;
    // 都 pinned 或都不 pinned 时，按时间戳降序排序
    return b.timestamp - a.timestamp;
  });
};

// 每个项目显示的对话数量限制
const conversationLimit = ref<Map<number, number>>(new Map());

// 获取项目的显示限制
const getConversationLimit = (projectId: number) => {
  if (!conversationLimit.value.has(projectId)) {
    conversationLimit.value.set(projectId, 10);
  }
  return conversationLimit.value.get(projectId) || 10;
};

// 显示的聊天记录（根据限制数量）
const getVisibleConversations = (projectId: number) => {
  const convs = getProjectConversations(projectId);
  const limit = getConversationLimit(projectId);
  return convs.slice(0, limit);
};

// 是否有更多记录
const hasMoreConversations = (projectId: number) => {
  const limit = getConversationLimit(projectId);
  return getProjectConversations(projectId).length > limit;
};

// 显示更多对话
const showMoreConversations = (projectId: number) => {
  const currentLimit = getConversationLimit(projectId);
  const total = getProjectConversations(projectId).length;
  const newLimit = Math.min(currentLimit + 10, total);
  conversationLimit.value.set(projectId, newLimit);
};

// 折叠对话列表，仅保留最近 10 条
const collapseConversations = (projectId: number) => {
  conversationLimit.value.set(projectId, 10);
};

const hasConversationInList = (conversationId: string) => {
  return conversations.value.some(conversation => conversation.id === conversationId);
};

function setProjectDraftConversation(projectId: number, conversation: Conversation) {
  const nextMap = new Map(projectDraftConversations.value);
  nextMap.set(projectId, conversation);
  projectDraftConversations.value = nextMap;
}

function clearProjectDraftConversation(projectId: number, conversationId?: string) {
  const currentDraft = projectDraftConversations.value.get(projectId);
  if (!currentDraft) return;
  if (conversationId && currentDraft.id !== conversationId) return;

  const nextMap = new Map(projectDraftConversations.value);
  nextMap.delete(projectId);
  projectDraftConversations.value = nextMap;
}

function getProjectDraftConversation(projectId: number): Conversation | null {
  const draftConversation = projectDraftConversations.value.get(projectId);
  if (!draftConversation) return null;

  if (hasConversationInList(draftConversation.id)) {
    clearProjectDraftConversation(projectId, draftConversation.id);
    return null;
  }

  return draftConversation;
}

function findDraftConversationBySessionId(sessionId: string): Conversation | null {
  for (const conversation of projectDraftConversations.value.values()) {
    if (conversation.id === sessionId) {
      return conversation;
    }
  }

  return null;
}

const shouldExposeActiveSession = (sessionId: string) => {
  return !claudeStore.isSessionPending(sessionId) || hasConversationInList(sessionId);
};

// 刷新项目的会话列表
const refreshProjectSessions = async (projectId: number) => {
  const project = projects.value.find(p => p.id === projectId);
  if (!project) {
    console.error('Project not found:', projectId);
    return;
  }

  // 防止重复刷新
  if (refreshingProjects.value.has(projectId)) {
    console.log(`Project ${projectId} is already refreshing`);
    return;
  }

  try {
    // 添加到刷新中集合
    const newRefreshing = new Set(refreshingProjects.value);
    newRefreshing.add(projectId);
    refreshingProjects.value = newRefreshing;

    console.log(`[Refresh] Starting refresh for project ${project.name} (ID: ${projectId})`);

    interface SessionInfo {
      sessionId: string;
      title: string;
      messageCount: number;
      fileSize: number;
      updatedAt: number;
      createdAt: number;
    }

    // 调用后端扫描项目会话
    console.log(`[Refresh] Calling get_project_sessions with path: ${project.path}`);
    const sessions = await invoke<SessionInfo[]>('get_project_sessions', {
      projectPath: project.path
    });

    console.log(`[Refresh] Project ${project.name}: received ${sessions.length} sessions`);
    console.log(`[Refresh] Sessions data:`, sessions);
    console.log(`[Refresh] Sessions JSON:`, JSON.stringify(sessions, null, 2));

    // 更新会话列表
    // 1. 移除该项目下已不存在的会话（但保留待持久化的会话）
    const newSessionIds = new Set(sessions.map(s => s.sessionId));
    console.log(`[Refresh] New session IDs:`, Array.from(newSessionIds));
    console.log(`[Refresh] Pending sessions:`, Array.from(claudeStore.sessionPersistStatus.entries()));

    const beforeCount = conversations.value.length;
    conversations.value = conversations.value.filter(c => {
      // 保留条件：
      // 1. 不属于该项目
      // 2. 或者属于该项目且在 newSessionIds 中
      // 3. 或者是待持久化的会话
      const isPending = claudeStore.isSessionPending(c.id);
      const belongsToProject = c.projectId === projectId;
      const inNewIds = newSessionIds.has(c.id);

      return c.projectId !== projectId || (belongsToProject && inNewIds) || isPending;
    });
    console.log(`[Refresh] Filtered conversations: ${beforeCount} -> ${conversations.value.length}`);

    // 2. 添加或更新会话
    let addedCount = 0;
    let updatedCount = 0;

    sessions.forEach(session => {
      const existingIndex = conversations.value.findIndex(c => c.id === session.sessionId);
      const timestamp = parseInt(String(session.updatedAt || session.createdAt || '0'));

      const convData: Conversation = {
        id: session.sessionId,
        title: session.title || 'Untitled',
        time: '',
        timestamp: timestamp,
        messageCount: session.messageCount || 0,
        size: formatFileSize(session.fileSize || 0),
        projectId: projectId,
        permissionMode: existingIndex >= 0
          ? conversations.value[existingIndex].permissionMode || claudeStore.defaultPermissionMode
          : claudeStore.defaultPermissionMode,
        providerId: null,
        model: null,
        providerOverrideEnabled: false,
        thinkingLevel: existingIndex >= 0 ? conversations.value[existingIndex].thinkingLevel || 'medium' : 'medium',
      };

      if (existingIndex >= 0) {
        // 更新现有会话，保留本地已有的有效标题
        const existingConv = conversations.value[existingIndex];
        // 如果本地已有有效标题（不是 Untitled），则保留
        const titleToUse = (existingConv.title && existingConv.title !== 'Untitled')
          ? existingConv.title
          : (session.title || 'Untitled');

        conversations.value[existingIndex] = {
          ...existingConv,
          ...convData,
          title: titleToUse
        };
        updatedCount++;
      } else {
        // 添加新会话
        conversations.value.push(convData);
        addedCount++;
      }
    });

    console.log(`[Refresh] Added ${addedCount} new conversations, updated ${updatedCount} existing conversations`);

    // 按时间排序（新的在前）
    conversations.value.sort((a, b) => b.id.localeCompare(a.id));

    console.log(`[Refresh] Updated conversations for project ${project.name}, total: ${conversations.value.length}`);
  } catch (error) {
    console.error(`[Refresh] Failed to refresh project ${project.name}:`, error);
  } finally {
    // 从刷新中集合移除
    const newRefreshing = new Set(refreshingProjects.value);
    newRefreshing.delete(projectId);
    refreshingProjects.value = newRefreshing;
  }
};



// 停止自动刷新定时器
const stopAutoRefresh = () => {
  if (autoRefreshInterval !== null) {
    clearInterval(autoRefreshInterval);
    autoRefreshInterval = null;
    console.debug('[Auto Refresh] Stopped auto-refresh interval');
  }
};

// 点击项目行：切换展开/收起状态
// 鼠标按下位置，用于区分点击和拖拽
let isDragging = false;

const selectProject = (projectId: number) => {
  // 如果刚刚发生过拖拽，不处理本次点击
  if (suppressProjectClick.value || isDragging) {
    return;
  }
  // 切换展开状态
  if (expandedProjects.value.has(projectId)) {
    expandedProjects.value.delete(projectId);
  } else {
    expandedProjects.value.add(projectId);
  }
};

// 创建新对话：设置工作目录但不加载历史记录
const startNewConversation = async (projectId: number) => {
  // 获取项目信息
  const project = projects.value.find(p => p.id === projectId);
  if (!project) {
    console.error('Project not found:', projectId);
    return;
  }

  const existingDraftConversation = getProjectDraftConversation(projectId);
  if (existingDraftConversation) {
    const draftSessionId = conversationSessionMap.value.get(existingDraftConversation.id) || existingDraftConversation.id;

    try {
      const sessionExists = await invoke<boolean>('check_session_exists', {
        sessionId: draftSessionId
      });

      if (sessionExists) {
        selectedConversation.value = existingDraftConversation;
        originalConversationTitle.value = existingDraftConversation.title || '';
        expandedProjects.value.add(projectId);
        workingDirectorySet.value = true;
        applySessionMessagesToCurrentView(
          draftSessionId,
          claudeStore.messages.get(draftSessionId) || [],
        );

        const currentStatus = claudeStore.connectionStatus.get(draftSessionId);
        claudeStore.setConnectionStatus(
          draftSessionId,
          currentStatus && currentStatus !== 'disconnected' ? currentStatus : 'connected',
        );
        return;
      }
    } catch (error) {
      console.error('[startNewConversation] Failed to restore draft session:', error);
    }

    clearProjectDraftConversation(projectId, existingDraftConversation.id);
  }

  const previousConversation = selectedConversation.value;
  const previousTitle = originalConversationTitle.value;

  // 生成临时 conversation ID（用于 UI 显示和状态追踪）
  const tempConversationId = `temp-${Date.now()}`;
  const tempConversation: Conversation = {
    id: tempConversationId,
    title: '新对话',
    time: '刚刚',
    timestamp: Math.floor(Date.now() / 1000),
    messageCount: 0,
    size: '0 B',
    projectId: project.id,
    permissionMode: claudeStore.defaultPermissionMode,
    providerId: providerStore.activeProviderId,
    model: providerStore.getPrimaryModel(providerStore.activeProviderId),
    providerOverrideEnabled: false,
    thinkingLevel: claudeStore.thinkingLevel,
  };

  const providerState = resolveConversationProvider(tempConversation);

  try {
    // 清空当前消息，并立即切到一个临时会话壳，避免欢迎页闪烁
    selectedConversation.value = tempConversation;
    messages.value = [];
    originalConversationTitle.value = tempConversation.title;

    // 清除临时会话可能残留的流式状态（避免新建会话时消息错乱）
    clearSessionStreamingUiState(tempConversationId);

    // 确保项目被展开，以便 sendMessage 能找到当前项目
    expandedProjects.value.add(projectId);

    console.log('[startNewConversation] Creating new session for project:', projectId);
    console.log('[startNewConversation] Using temp conversation ID:', tempConversationId);

    // 先设置当前会话和连接状态（确保 UI 能显示"连接中"）
    claudeStore.setCurrentSession(tempConversationId);
    claudeStore.setConnectionStatus(tempConversationId, 'connecting');
    currentSessionId.value = tempConversationId;

    // 创建 session 并设置工作目录（一次调用）
    const session = await invoke<Session>('create_session', {
      projectPath: project.path,
      thinkingLevel: tempConversation.thinkingLevel || claudeStore.thinkingLevel,
      providerId: providerState.providerId,
      model: providerState.model,
      providerEnv: providerState.providerEnv,
    });
    const sessionId = session.id;

    console.log('[startNewConversation] Session created:', sessionId);

    // 刷新活动 Session 列表
    refreshActiveSessions();

    // 第二阶段 sessionId 只作为内部运行态使用，
    // 等 CLI 返回最终 sessionId 后再把会话正式写入聊天列表和 tab。
    claudeStore.createPendingSession(sessionId, project.path, {
      permissionMode: getConversationPermissionMode(tempConversation),
      thinkingLevel: tempConversation.thinkingLevel || claudeStore.thinkingLevel,
      providerId: tempConversation.providerId,
      model: tempConversation.model,
      providerOverrideEnabled: tempConversation.providerOverrideEnabled,
    });

    const nextSessionMap = new Map(conversationSessionMap.value);
    nextSessionMap.set(tempConversationId, sessionId);
    conversationSessionMap.value = nextSessionMap;

    selectedConversation.value = tempConversation;
    setProjectDraftConversation(projectId, tempConversation);

    // 检查后端是否已经设置了连接状态（可能在我们等待 create_session 时就发送了）
    const existingStatus = claudeStore.connectionStatus.get(sessionId);
    const finalStatus = existingStatus || 'connecting';
    console.log('[startNewConversation] Checking existing status for session:', sessionId, {
      existingStatus,
      finalStatus
    });

    // 将状态迁移到实际的 sessionId
    claudeStore.setConnectionStatus(sessionId, finalStatus);
    claudeStore.setConnectionStatus(tempConversationId, 'disconnected'); // 清理临时状态

    // 更新当前会话为实际的 sessionId
    claudeStore.setCurrentSession(sessionId);
    currentSessionId.value = sessionId;

    // 同时更新本地变量（用于 sendMessage 等其他用途）
    workingDirectorySet.value = true;

    console.log('[startNewConversation] Setup complete for session:', sessionId, {
      storeSessionId: claudeStore.currentSessionId,
      connectionStatus: claudeStore.connectionStatus.get(sessionId),
      allStatus: Object.fromEntries(claudeStore.connectionStatus)
    });
  } catch (error) {
    selectedConversation.value = previousConversation;
    originalConversationTitle.value = previousTitle;
    console.error('Failed to create session or set working directory:', error);
  }
};

// 判断项目是否展开
const isProjectExpanded = (projectId: number) => {
  return expandedProjects.value.has(projectId);
};

// 选中聊天记录
const selectConversation = async (conv: Conversation) => {
  console.log('[selectConversation] START ====', { convId: conv.id, convTitle: conv.title });
  const selectionRequestId = ++conversationSelectionRequestId.value;
  persistCurrentSessionMessages();
  selectedConversation.value = conv;
  console.log('[selectConversation] selectedConversation set:', selectedConversation.value?.id);

  const targetSessionIdForView = getActualSessionIdForConversation(conv.id) || conv.id;
  const targetSessionMessages = claudeStore.messages.get(targetSessionIdForView) || [];

  // 清除该 session 的未读任务完成通知
  claudeStore.clearUnreadTaskCompletion(conv.id);
  console.log('[selectConversation] Cleared unread notification for session:', conv.id);

  // 清除该 session 的未读消息完成通知
  claudeStore.clearUnreadMessageCompletion(conv.id);
  console.log('[selectConversation] Cleared unread message completion for session:', conv.id);

  // 保存原始标题（用于回退）
  originalConversationTitle.value = conv.title || '';

  // 切换会话时先恢复目标 session 的快照，避免空数组被误写回目标 session。
  applySessionMessagesToCurrentView(targetSessionIdForView, targetSessionMessages);

  // 获取项目路径
  const project = projects.value.find(p => p.id === conv.projectId);
  if (!project) {
    console.error('Project not found for conversation:', conv.id);
    return;
  }

  // 先设置为 connecting 状态（使用 conv.id）
  claudeStore.setConnectionStatus(conv.id, 'connecting');

  try {
    // 检查该 conversation 是否已有关联的 session
    const existingSessionId = getActualSessionIdForConversation(conv.id);
    console.log('[selectConversation] existingSessionId:', existingSessionId, 'for conv:', conv.id);

    if (existingSessionId) {
      // 检查 session 是否仍然活跃
      const sessionExists = await invoke<boolean>('check_session_exists', {
        sessionId: existingSessionId
      });

      if (!isConversationSelectionStillCurrent(selectionRequestId, conv.id)) {
        console.log('[selectConversation] Selection changed while checking existing session, abort apply:', {
          convId: conv.id,
          selectionRequestId,
          currentSelectionRequestId: conversationSelectionRequestId.value,
          currentSelectedConversationId: selectedConversation.value?.id || null,
        });
        return;
      }

      if (sessionExists) {
        // Session 仍然活跃，直接复用
        console.log(`Reusing existing session: ${existingSessionId} for conversation: ${conv.id}`);

        applySessionMessagesToCurrentView(
          existingSessionId,
          claudeStore.messages.get(existingSessionId) || [],
        );
        // 复用会话时，设置为 connected（因为会话已经存在）
        claudeStore.setConnectionStatus(existingSessionId, 'connected');

        if (claudeStore.streaming.has(existingSessionId)) {
          console.log('[selectConversation] Restored conversation from session snapshot:', existingSessionId);
          return;
        }

        if ((claudeStore.messages.get(existingSessionId)?.length || 0) > 0) {
          console.log('[selectConversation] Snapshot restored, reloading history to refresh nested subagent state:', existingSessionId);
          await reloadConversationHistory(conv, {
            forceApply: true,
            preserveCurrentMessagesOnError: true,
            selectionRequestId,
          });
          return;
        }

        await reloadConversationHistory(conv, { forceApply: true, selectionRequestId });
        return;
      } else {
        // Session 已失效，清理映射
        console.log(`Session ${existingSessionId} no longer exists, cleaning up`);
        const newMap = new Map(conversationSessionMap.value);
        newMap.delete(conv.id);
        conversationSessionMap.value = newMap;
      }
    }

    // 没有可用的 session，需要创建新的
    console.log(`Creating new session for conversation: ${conv.id}`);

    // 立即加载历史消息（不需要等待 session 创建完成）
    const loadHistory = async () => {
      console.log('[selectConversation] Loading history messages...');
      await reloadConversationHistory(conv, {
        preserveCurrentMessagesOnError: false,
        selectionRequestId,
      });
    };

    // 并行执行：立即加载历史消息 + 创建 session
    await Promise.all([
      loadHistory(),
      (async () => {
        try {
          // 创建 session（传递 sessionId 用于 --resume，projectPath 用于工作目录）
          // 注意：后端返回的 session.id 会等于传入的 sessionId（即 conv.id）
          const session = await invoke<Session>('create_session', {
            sessionId: conv.id,
            projectPath: project.path,
            thinkingLevel: conv.thinkingLevel || 'medium',
            providerId: resolveConversationProvider(conv).providerId,
            model: resolveConversationProvider(conv).model,
            providerEnv: resolveConversationProvider(conv).providerEnv,
          });
          const sessionId = session.id;

          console.log('[selectConversation] Session created for conversation:', {
            convId: conv.id,
            sessionId,
            areEqual: sessionId === conv.id
          });
          const shouldApplyCreatedSessionToCurrentView = isConversationSelectionStillCurrent(selectionRequestId, conv.id);
          if (!shouldApplyCreatedSessionToCurrentView) {
            console.log('[selectConversation] Selection changed before session creation settled, skip current-view apply:', {
              convId: conv.id,
              sessionId,
              selectionRequestId,
              currentSelectionRequestId: conversationSelectionRequestId.value,
              currentSelectedConversationId: selectedConversation.value?.id || null,
            });
          }

          // 刷新活动 Session 列表
          refreshActiveSessions();

          // 保存 conversation ID 到 session ID 的映射
          const sessionMap = new Map(conversationSessionMap.value);
          sessionMap.set(conv.id, sessionId);
          conversationSessionMap.value = sessionMap;

          // 在前端创建基本的会话状态
          const sessionData = {
            sessionId: sessionId,
            cwd: project.path,
            permissionMode: getConversationPermissionMode(conv),
            thinkingLevel: conv.thinkingLevel || 'medium',
            providerId: conv.providerId || null,
            model: resolveConversationProvider(conv).model,
            providerOverrideEnabled: !!conv.providerOverrideEnabled,
            createdAt: Date.now(),
          };
          claudeStore.addSession(sessionData);
          if (shouldApplyCreatedSessionToCurrentView) {
            currentSessionId.value = sessionId;
            workingDirectorySet.value = true;
            applySessionMessagesToCurrentView(
              sessionId,
              claudeStore.messages.get(sessionId) || [],
            );
          }

          if (sessionData.permissionMode !== 'default') {
            // 临时注释：创建 session 后先不自动下发 set_permission_mode
            // await invoke('set_permission_mode', {
            //   sessionId,
            //   mode: sessionData.permissionMode,
            // });
            console.log('[PERMISSION] Auto sync disabled after session creation:', {
              sessionId,
              mode: sessionData.permissionMode,
            });
          }

          if (sessionId !== conv.id) {
            const restoredHistoryUsage = claudeStore.sessionModelUsage.get(conv.id);
            if (restoredHistoryUsage) {
              claudeStore.setSessionModelUsage(sessionId, restoredHistoryUsage);
            }
          }

          // 连接建立完成，设置为 connected
          claudeStore.setConnectionStatus(sessionId, 'connected');

          console.log('[selectConversation] Session created and connected:', sessionId);
        } catch (error) {
          console.error('[selectConversation] Failed to create session:', error);
          // 设置连接状态为"未连接"
          const sessionId = currentSessionId.value || conv.id;
          claudeStore.setConnectionStatus(sessionId, 'disconnected');
        }
      })()
    ]);
  } catch (error) {
    console.error('Failed to load session messages:', error);

    // 设置连接状态为"未连接"
    const sessionId = currentSessionId.value || conv.id;
    claudeStore.setConnectionStatus(sessionId, 'disconnected');

    // 显示错误消息
    messages.value = [{
      id: 'error',
      role: 'assistant',
      content: `无法加载会话历史: ${error}`,
      timestamp: Date.now()
    }];
  }
};

// 拖拽开始
const startResize = (e: MouseEvent) => {
  isResizing.value = true;
  resizeStartX.value = e.clientX;
  resizeStartWidth.value = leftPanelWidth.value;
  document.body.style.cursor = 'col-resize';
  document.body.style.userSelect = 'none';
  document.addEventListener('mousemove', onResize);
  document.addEventListener('mouseup', stopResize);
};

const clampLeftPanelWidth = (width: number) => {
  return Math.min(Math.max(width, minLeftWidth), maxLeftWidth);
};

// 拖拽中
const onResize = (e: MouseEvent) => {
  if (!isResizing.value) return;
  const deltaX = e.clientX - resizeStartX.value;
  leftPanelWidth.value = clampLeftPanelWidth(resizeStartWidth.value + deltaX);
};

// 拖拽结束
const stopResize = () => {
  isResizing.value = false;
  document.body.style.cursor = '';
  document.body.style.userSelect = '';
  document.removeEventListener('mousemove', onResize);
  document.removeEventListener('mouseup', stopResize);
};

const clampWorkspacePanelWidth = (width: number) => {
  const containerWidth = chatContentShellRef.value?.getBoundingClientRect().width ?? window.innerWidth;
  const layoutMaxWidth = Math.max(
    minWorkspacePanelWidth,
    containerWidth - minChatMainColumnWidth,
  );
  return Math.min(
    Math.max(width, minWorkspacePanelWidth),
    Math.min(layoutMaxWidth, maxWorkspacePanelWidth),
  );
};

const syncWorkspacePanelWidth = () => {
  workspacePanelWidth.value = clampWorkspacePanelWidth(workspacePanelWidth.value);
};

const startWorkspaceResize = (event: MouseEvent) => {
  isWorkspaceResizing.value = true;
  workspaceResizeStartX.value = event.clientX;
  workspaceResizeStartWidth.value = workspacePanelWidth.value;
  document.body.style.cursor = 'col-resize';
  document.body.style.userSelect = 'none';
  document.addEventListener('mousemove', onWorkspaceResize);
  document.addEventListener('mouseup', stopWorkspaceResize);
};

const onWorkspaceResize = (event: MouseEvent) => {
  if (!isWorkspaceResizing.value) return;
  const deltaX = workspaceResizeStartX.value - event.clientX;
  workspacePanelWidth.value = clampWorkspacePanelWidth(workspaceResizeStartWidth.value + deltaX);
};

const stopWorkspaceResize = () => {
  isWorkspaceResizing.value = false;
  document.body.style.cursor = '';
  document.body.style.userSelect = '';
  document.removeEventListener('mousemove', onWorkspaceResize);
  document.removeEventListener('mouseup', stopWorkspaceResize);
};

// 消息列表
const messages = ref<Message[]>([]);
// 当前会话是否正在流式响应（从 claudeStore 获取，确保 session 独享）
const isStreaming = computed(() => {
  return currentSessionId.value ? claudeStore.streaming.has(currentSessionId.value) : false;
});

const currentStreamingStartedAt = computed(() => {
  return currentSessionId.value
    ? claudeStore.streamingStartedAt.get(currentSessionId.value) || null
    : null;
});
const currentSubagentRuntime = computed(() => {
  return currentSessionId.value
    ? claudeStore.subagentRuntime.get(currentSessionId.value) || new Map()
    : new Map();
});
const streamingPlaceholderIds = ref<Map<string, string>>(new Map());
const streamDataReceivedBySession = ref<Map<string, boolean>>(new Map());
const streamUsageReceivedBySession = ref<Map<string, boolean>>(new Map());
const isHydratingSessionMessages = ref(false);
// 记录哪些 session 是用户主动停止的，用于跳过 stop 后那次 stream_end 的 token 统计刷新。
const manuallyStoppedSessions = ref<Set<string>>(new Set());
const rewindBusy = ref(false);
const rewindDraftText = ref('');
const rewindDraftVersion = ref(0);

// 活动 Session 列表（从后端获取）
const activeSessions = ref<string[]>([]);

const markSessionStoppedManually = (sessionId: string) => {
  const next = new Set(manuallyStoppedSessions.value);
  next.add(sessionId);
  manuallyStoppedSessions.value = next;
};

const clearSessionManualStop = (sessionId: string) => {
  if (!manuallyStoppedSessions.value.has(sessionId)) return;
  const next = new Set(manuallyStoppedSessions.value);
  next.delete(sessionId);
  manuallyStoppedSessions.value = next;
};

const consumeSessionManualStop = (sessionId: string): boolean => {
  const wasStoppedManually = manuallyStoppedSessions.value.has(sessionId);
  if (!wasStoppedManually) return false;
  clearSessionManualStop(sessionId);
  return true;
};

const cloneMessage = (message: Message): Message => ({
  ...message,
  contentBlocks: message.contentBlocks ? [...message.contentBlocks] : undefined,
  images: message.images ? [...message.images] : undefined,
  attachments: message.attachments ? [...message.attachments] : undefined,
  tool_calls: message.tool_calls ? [...message.tool_calls] : undefined,
  toolResults: message.toolResults ? { ...message.toolResults } : undefined,
  toolResultErrors: message.toolResultErrors ? { ...message.toolResultErrors } : undefined,
  tokenUsage: message.tokenUsage ? { ...message.tokenUsage } : undefined,
  usage: message.usage ? { ...message.usage } : undefined,
});

const cloneMessages = (sessionMessages: Message[]): Message[] => {
  return sessionMessages.map(cloneMessage);
};

const getStreamingPlaceholderId = (sessionId: string): string | null => {
  return streamingPlaceholderIds.value.get(sessionId) || null;
};

const setStreamingPlaceholderId = (sessionId: string, placeholderId: string | null) => {
  const next = new Map(streamingPlaceholderIds.value);
  if (placeholderId) {
    next.set(sessionId, placeholderId);
  } else {
    next.delete(sessionId);
  }
  streamingPlaceholderIds.value = next;
};

const hasSessionReceivedStreamData = (sessionId: string): boolean => {
  return streamDataReceivedBySession.value.get(sessionId) || false;
};

const setSessionReceivedStreamData = (sessionId: string, received: boolean) => {
  const next = new Map(streamDataReceivedBySession.value);
  if (received) {
    next.set(sessionId, true);
  } else {
    next.delete(sessionId);
  }
  streamDataReceivedBySession.value = next;
};

const hasSessionReceivedStreamUsage = (sessionId: string): boolean => {
  return streamUsageReceivedBySession.value.get(sessionId) || false;
};

const setSessionReceivedStreamUsage = (sessionId: string, received: boolean) => {
  const next = new Map(streamUsageReceivedBySession.value);
  if (received) {
    next.set(sessionId, true);
  } else {
    next.delete(sessionId);
  }
  streamUsageReceivedBySession.value = next;
};

const updateSessionInputTokenUsage = (sessionId: string, usage: any) => {
  const model = resolveSessionModelForUsage(sessionId);
  const existingUsage = claudeStore.sessionModelUsage.get(sessionId);
  const nextUsage = buildModelUsageData(usage, model);
  claudeStore.setSessionModelUsage(
    sessionId,
    {
      ...nextUsage,
      contextWindow: existingUsage?.contextWindow ?? nextUsage.contextWindow,
      maxOutputTokens: existingUsage?.maxOutputTokens ?? nextUsage.maxOutputTokens,
      costUSD: existingUsage?.costUSD ?? nextUsage.costUSD,
      model: existingUsage?.model ?? nextUsage.model,
    },
  );
};

const clearSessionStreamingUiState = (sessionId: string) => {
  setStreamingPlaceholderId(sessionId, null);
  setSessionReceivedStreamData(sessionId, false);
  setSessionReceivedStreamUsage(sessionId, false);
};

const renameSessionStreamingUiState = (oldSessionId: string, newSessionId: string) => {
  const placeholderId = getStreamingPlaceholderId(oldSessionId);
  const hasReceivedData = hasSessionReceivedStreamData(oldSessionId);
  const hasReceivedUsage = hasSessionReceivedStreamUsage(oldSessionId);

  clearSessionStreamingUiState(oldSessionId);

  if (placeholderId) {
    setStreamingPlaceholderId(newSessionId, placeholderId);
  }
  if (hasReceivedData) {
    setSessionReceivedStreamData(newSessionId, true);
  }
  if (hasReceivedUsage) {
    setSessionReceivedStreamUsage(newSessionId, true);
  }
};

const getSessionMessagesSnapshot = (sessionId: string): Message[] => {
  if (sessionId === currentSessionId.value) {
    return cloneMessages(messages.value);
  }

  return cloneMessages(claudeStore.messages.get(sessionId) || []);
};

const setSessionMessagesSnapshot = (sessionId: string, nextMessages: Message[]) => {
  const clonedMessages = cloneMessages(nextMessages);
  claudeStore.setMessages(sessionId, clonedMessages);

  if (sessionId === currentSessionId.value) {
    messages.value = clonedMessages;
  }
};

const createStreamingPlaceholderId = (sessionId: string) => (
  `streaming_${sessionId}_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`
);

const createStreamingPlaceholderMessage = (sessionId: string, content = ''): Message => ({
  id: createStreamingPlaceholderId(sessionId),
  role: 'assistant',
  content,
  timestamp: claudeStore.streamingStartedAt.get(sessionId) || Date.now(),
  isStreaming: true,
});

const findStreamingPlaceholderIndex = (sessionId: string, sessionMessages: Message[]): number => {
  const trackedPlaceholderId = getStreamingPlaceholderId(sessionId);
  if (trackedPlaceholderId) {
    const trackedIndex = sessionMessages.findIndex(message => message.id === trackedPlaceholderId);
    if (trackedIndex !== -1) {
      return trackedIndex;
    }
  }

  for (let index = sessionMessages.length - 1; index >= 0; index -= 1) {
    if (!sessionMessages[index].isStreaming) continue;
    setStreamingPlaceholderId(sessionId, sessionMessages[index].id);
    return index;
  }

  return -1;
};

const cleanupSessionStreamingMessages = (sessionMessages: Message[]): Message[] => {
  return sessionMessages
    .filter((message) => {
      if (!message.isStreaming) return true;

      const hasStructuredContent = !!message.contentBlocks?.length;
      const hasContent = Boolean(message.content || hasStructuredContent || message.attachments?.length || message.images?.length);
      return hasContent;
    })
    .map((message) => ({
      ...message,
      isStreaming: false,
    }));
};

const ensureStreamingPlaceholder = (sessionId: string, sessionMessages: Message[]) => {
  const placeholderIndex = findStreamingPlaceholderIndex(sessionId, sessionMessages);

  if (placeholderIndex !== -1) {
    sessionMessages[placeholderIndex] = {
      ...sessionMessages[placeholderIndex],
      isStreaming: true,
    };
    setStreamingPlaceholderId(sessionId, sessionMessages[placeholderIndex].id);
    return {
      sessionMessages,
      placeholder: sessionMessages[placeholderIndex],
      placeholderIndex,
    };
  }

  const placeholder = createStreamingPlaceholderMessage(
    sessionId,
    claudeStore.streaming.get(sessionId) || '',
  );
  sessionMessages.push(placeholder);
  setStreamingPlaceholderId(sessionId, placeholder.id);
  return {
    sessionMessages,
    placeholder,
    placeholderIndex: sessionMessages.length - 1,
  };
};

const appendStreamingDeltaToPlaceholder = (
  sessionId: string,
  sessionMessages: Message[],
  delta: string,
) => {
  const { sessionMessages: nextMessages, placeholder, placeholderIndex } =
    ensureStreamingPlaceholder(sessionId, sessionMessages);

  nextMessages[placeholderIndex] = {
    ...placeholder,
    content: `${placeholder.content || ''}${delta}`,
    isStreaming: true,
  };

  return {
    sessionMessages: nextMessages,
    placeholder: nextMessages[placeholderIndex],
    placeholderIndex,
  };
};

const splitStreamingPlaceholderAfterStructuredMessage = (
  sessionId: string,
  sessionMessages: Message[],
  structuredMessage: Message,
) => {
  const placeholderIndex = findStreamingPlaceholderIndex(sessionId, sessionMessages);

  if (placeholderIndex === -1) {
    sessionMessages.push(structuredMessage);
    return {
      sessionMessages,
      placeholder: null as Message | null,
    };
  }

  const closedPlaceholder: Message = {
    ...sessionMessages[placeholderIndex],
    isStreaming: false,
  };
  const nextPlaceholder = createStreamingPlaceholderMessage(sessionId, '');

  sessionMessages.splice(
    placeholderIndex,
    1,
    closedPlaceholder,
    structuredMessage,
    nextPlaceholder,
  );

  setStreamingPlaceholderId(sessionId, nextPlaceholder.id);
  setSessionReceivedStreamData(sessionId, false);

  return {
    sessionMessages,
    placeholder: nextPlaceholder,
  };
};

const finalizeInterruptedStreamingState = (sessionId: string) => {
  const sessionMessages = getSessionMessagesSnapshot(sessionId);
  const cleanedMessages = cleanupSessionStreamingMessages(sessionMessages);
  setSessionMessagesSnapshot(sessionId, cleanedMessages);
  clearSessionStreamingUiState(sessionId);
};

const persistCurrentSessionMessages = () => {
  if (!currentSessionId.value || isHydratingSessionMessages.value) return;
  claudeStore.setMessages(currentSessionId.value, cloneMessages(messages.value));
};

const restoreStreamingUiState = (sessionId: string) => {
  clearSessionStreamingUiState(sessionId);

  if (sessionId !== currentSessionId.value) return;
  if (!claudeStore.streaming.has(sessionId)) return;

  const sessionMessages = cloneMessages(messages.value);
  const { sessionMessages: nextMessages, placeholder } = ensureStreamingPlaceholder(sessionId, sessionMessages);
  placeholder.content = placeholder.content || claudeStore.streaming.get(sessionId) || '';
  placeholder.isStreaming = true;
  setSessionReceivedStreamData(sessionId, placeholder.content.length > 0);
  messages.value = nextMessages;
};

const applySessionMessagesToCurrentView = (sessionId: string, nextMessages: Message[]) => {
  isHydratingSessionMessages.value = true;
  try {
    currentSessionId.value = sessionId;
    claudeStore.setCurrentSession(sessionId);
    messages.value = cloneMessages(nextMessages);
    restoreStreamingUiState(sessionId);
    claudeStore.setMessages(sessionId, cloneMessages(messages.value));
  } finally {
    isHydratingSessionMessages.value = false;
  }
};

const extractMessageContentBlocks = (msg: any, messageData: any): any[] => {
  const rawContent = messageData.content;

  const tryParseStructuredString = (value: string): any[] => {
    const candidates = [value];

    try {
      const parsed = JSON.parse(value);
      if (typeof parsed === 'string') {
        candidates.push(parsed);
      } else if (Array.isArray(parsed)) {
        return parsed;
      } else if (parsed && typeof parsed === 'object') {
        if (Array.isArray((parsed as any).content)) return (parsed as any).content;
        if (Array.isArray((parsed as any).Blocks)) return (parsed as any).Blocks;
        if (Array.isArray((parsed as any).blocks)) return (parsed as any).blocks;
      }
    } catch {
      // ignore
    }

    for (const candidate of candidates) {
      const normalized = candidate
        .replace(/\\"/g, '"')
        .replace(/\\n/g, '\n')
        .replace(/\\t/g, '\t');

      if (normalized === candidate) continue;

      try {
        const reparsed = JSON.parse(normalized);
        if (Array.isArray(reparsed)) return reparsed;
        if (reparsed && typeof reparsed === 'object') {
          if (Array.isArray((reparsed as any).content)) return (reparsed as any).content;
          if (Array.isArray((reparsed as any).Blocks)) return (reparsed as any).Blocks;
          if (Array.isArray((reparsed as any).blocks)) return (reparsed as any).blocks;
        }
      } catch {
        // ignore
      }
    }

    return [];
  };

  if (Array.isArray(rawContent)) {
    return rawContent;
  }
  if (typeof rawContent === 'string') {
    const parsedBlocks = tryParseStructuredString(rawContent);
    if (parsedBlocks.length > 0) {
      return parsedBlocks;
    }
  }
  if (Array.isArray(msg.contentBlocks)) {
    return msg.contentBlocks;
  }
  if (rawContent && typeof rawContent === 'object' && Array.isArray((rawContent as any).content)) {
    return (rawContent as any).content;
  }
  if (rawContent && typeof rawContent === 'object' && Array.isArray((rawContent as any).Blocks)) {
    return (rawContent as any).Blocks;
  }
  if (rawContent && typeof rawContent === 'object' && Array.isArray((rawContent as any).blocks)) {
    return (rawContent as any).blocks;
  }

  return [];
};

const extractMessageContentText = (rawContent: any, contentBlocks: any[]): string => {
  if (contentBlocks.length > 0) {
    const textBlocks = contentBlocks.filter((block: any) => block.type === 'text');
    const joinedText = textBlocks
      .map((block: any) => block.content ?? block.text ?? '')
      .join('\n');

    if (joinedText) {
      return joinedText;
    }

    if (contentBlocks.some((block: any) => block.type === 'tool_result')) {
      const toolResult = contentBlocks.find((block: any) => block.type === 'tool_result');
      return (toolResult && (toolResult.content ?? toolResult.text)) ?? '';
    }
  }

  if (typeof rawContent === 'string') {
    return rawContent;
  }

  return '';
};

const finalizeSessionTokenUsage = (
  sessionId: string,
  resultMessage: any,
  shouldSkipUsageRefresh: boolean,
) => {
  const sessionMessages = getSessionMessagesSnapshot(sessionId);
  const resultUsage = normalizeTokenUsage(resultMessage?.usage);
  const resultModelUsage = resultMessage?.modelUsage || resultMessage?.model_usage;

  let lastAssistantIndex = -1;
  for (let i = sessionMessages.length - 1; i >= 0; i -= 1) {
    if (sessionMessages[i].role === 'assistant') {
      lastAssistantIndex = i;
      break;
    }
  }

  const newMessages = sessionMessages.map((msg, index) => {
    const updated = { ...msg, isStreaming: false };

    if (!shouldSkipUsageRefresh && index === lastAssistantIndex) {
      const existingTokenUsage = updated.tokenUsage || updated.usage;
      const finalTokenUsage = resolveFinalTokenUsage(resultUsage, existingTokenUsage);

      if (finalTokenUsage) {
        updated.tokenUsage = finalTokenUsage;
        updated.usage = finalTokenUsage;
      }

      updated.showTokenUsage = true;
    }

    return updated;
  });

  setSessionMessagesSnapshot(sessionId, newMessages);

  if (shouldSkipUsageRefresh) {
    console.log('[TOKEN] Skip tokenUsage/sessionModelUsage refresh for manually stopped stream_end:', { sessionId });
    return newMessages;
  }

  const lastAssistantMessage = lastAssistantIndex >= 0 ? newMessages[lastAssistantIndex] : null;
  const latestAssistantUsage = resolveFinalTokenUsage(
    resultUsage,
    lastAssistantMessage?.tokenUsage || lastAssistantMessage?.usage,
  );
  const existingSessionModelUsage = sessionId
    ? claudeStore.sessionModelUsage.get(sessionId)
    : null;
  const shouldPreserveStreamUsage = hasSessionReceivedStreamUsage(sessionId);

  if (shouldPreserveStreamUsage) {
    console.log('[TOKEN] Preserve stream_usage token counters and refresh metadata from stream_end:', {
      sessionId,
      hasResultModelUsage: !!resultModelUsage,
    });
  } else {
    console.log('[TOKEN] Updated tokenUsage for last assistant message from stream_end');
  }

  if (sessionId && (latestAssistantUsage || existingSessionModelUsage)) {
    const nextSessionModelUsage = buildModelUsageData(
      latestAssistantUsage ?? existingSessionModelUsage,
      lastAssistantMessage?.model
        || claudeStore.sessions.get(sessionId)?.model
        || selectedConversation.value?.model
        || existingSessionModelUsage?.model
        || 'unknown',
      resultModelUsage,
    );

    claudeStore.setSessionModelUsage(
      sessionId,
      shouldPreserveStreamUsage && existingSessionModelUsage
        ? {
            ...nextSessionModelUsage,
            inputTokens: existingSessionModelUsage.inputTokens,
            outputTokens: existingSessionModelUsage.outputTokens,
            cacheReadInputTokens: existingSessionModelUsage.cacheReadInputTokens,
            cacheCreationInputTokens: existingSessionModelUsage.cacheCreationInputTokens,
          }
        : nextSessionModelUsage,
    );
    console.log('[TOKEN] Updated sessionModelUsage from stream_end:', { sessionId });
  }

  return newMessages;
};

const applyIncomingMessageToBackgroundSession = (sessionId: string, msg: any) => {
  const messageData = msg.message || msg;
  const rawContent = messageData.content;
  const contentBlocks = extractMessageContentBlocks(msg, messageData);
  const contentText = extractMessageContentText(rawContent, contentBlocks);
  const normalizedRole = (messageData.role ?? msg.role ?? 'assistant').toString().toLowerCase();
  const isUser = normalizedRole === 'user';
  const isSystem = normalizedRole === 'system';
  const sessionMessages = getSessionMessagesSnapshot(sessionId);

  if (isUser && contentBlocks.length > 0 && contentBlocks.every((block: any) => block.type === 'tool_result')) {
    const toolResultBlock = contentBlocks.find((block: any) => block.type === 'tool_result');
    if (toolResultBlock) {
      let parsedToolResult = toolResultBlock;
      if (typeof toolResultBlock.content === 'string') {
        try {
          parsedToolResult = JSON.parse(toolResultBlock.content);
        } catch (error) {
          console.error('[MESSAGE] Failed to parse background tool_result content:', error);
        }
      }

      const toolUseId = parsedToolResult.tool_use_id ?? parsedToolResult.toolUseId;
      const resultContent = typeof parsedToolResult.content === 'string'
        ? parsedToolResult.content
        : (parsedToolResult.text ?? '');
      const isError = parsedToolResult.is_error ?? parsedToolResult.isError ?? false;

      if (toolUseId && resultContent !== undefined) {
        const getToolUseId = (block: any): string | null => {
          if (block.type !== 'tool_use') return null;
          if (block.id) return block.id;
          try {
            const parsed = typeof block.content === 'string' ? JSON.parse(block.content) : block.content;
            return parsed?.id ?? null;
          } catch {
            return null;
          }
        };

        for (let i = sessionMessages.length - 1; i >= 0; i--) {
          const sessionMessage = sessionMessages[i];
          if (sessionMessage.role !== 'assistant' || !sessionMessage.contentBlocks) continue;
          const hasMatch = sessionMessage.contentBlocks.some((block: any) => getToolUseId(block) === toolUseId);
          if (!hasMatch) continue;

          sessionMessages[i] = {
            ...sessionMessage,
            toolResults: { ...(sessionMessage.toolResults || {}), [toolUseId]: resultContent },
            toolResultErrors: { ...(sessionMessage.toolResultErrors || {}), [toolUseId]: isError },
          };
          setSessionMessagesSnapshot(sessionId, sessionMessages);
          return;
        }
      }
    }
  }

  const isStreamingAssistant = !isUser && !isSystem && contentBlocks.length > 0
    && !contentBlocks.some((block: any) => block.type === 'tool_use' || block.type === 'tool_result');
  const placeholderIndex = findStreamingPlaceholderIndex(sessionId, sessionMessages);
  const hasStreamingData = hasSessionReceivedStreamData(sessionId);

  if (placeholderIndex !== -1 && isStreamingAssistant && hasStreamingData) {
    const placeholder = sessionMessages[placeholderIndex];
    sessionMessages[placeholderIndex] = {
      ...placeholder,
      content: placeholder.content || contentText || '',
      contentBlocks: placeholder.contentBlocks,
      model: messageData.model || placeholder.model,
      tokenUsage: messageData.usage ? normalizeTokenUsage(messageData.usage) : placeholder.tokenUsage,
      usage: messageData.usage ? normalizeTokenUsage(messageData.usage) : placeholder.usage,
      isStreaming: true,
    };
    setSessionMessagesSnapshot(sessionId, sessionMessages);
    return;
  }

  const convertedMsg: Message = {
    id: `${messageData.id ?? msg.id ?? 'msg'}_${Date.now()}_${Math.random().toString(36).slice(2)}`,
    role: isSystem ? 'system' : (isUser ? 'user' : 'assistant'),
    content: contentText || '',
    timestamp: normalizeMessageTimestamp(msg.timestamp ?? messageData.createdAt),
    checkpointUuid: msg.checkpointUuid ?? messageData.checkpointUuid,
    contentBlocks: contentBlocks.length > 0 ? contentBlocks : undefined,
    parentToolUseId: msg.parent_tool_use_id ?? msg.parentToolUseId,
    model: messageData.model,
    tokenUsage: messageData.usage ? normalizeTokenUsage(messageData.usage) : undefined,
    usage: messageData.usage ? normalizeTokenUsage(messageData.usage) : undefined,
  };

  const shouldInsertBeforePlaceholder = placeholderIndex !== -1
    && !isUser
    && !isSystem
    && !hasStreamingData
    && contentBlocks.length > 0;

  if (placeholderIndex !== -1 && !isUser && !isSystem && contentBlocks.length > 0 && hasStreamingData) {
    const { sessionMessages: nextMessages } = splitStreamingPlaceholderAfterStructuredMessage(
      sessionId,
      sessionMessages,
      convertedMsg,
    );
    setSessionMessagesSnapshot(sessionId, nextMessages);
    return;
  }

  if (shouldInsertBeforePlaceholder) {
    sessionMessages.splice(placeholderIndex, 0, convertedMsg);
  } else {
    sessionMessages.push(convertedMsg);
  }

  setSessionMessagesSnapshot(sessionId, sessionMessages);
};

watch(messages, () => {
  persistCurrentSessionMessages();
}, { deep: true });

// SDK 连接状态
// 当前会话的连接状态（统一从 claudeStore 获取）
const currentConversationConnectionState = computed(() => {
  // 访问 connectionStatus 确保响应式依赖被追踪（Map.get() 不会触发响应式）
  const connectionStatus = claudeStore.connectionStatus;

  // 优先从选中的 conversation 获取 sessionId，再查连接状态
  if (selectedConversation.value) {
    const sessionId = conversationSessionMap.value.get(selectedConversation.value.id);
    if (sessionId) {
      // 有 sessionId 映射，用 sessionId 查状态
      const state = connectionStatus.get(sessionId) || 'disconnected';
      console.log('[ConnectionState] Using conversation session state:', {
        conversationId: selectedConversation.value.id,
        sessionId,
        state,
        allStates: Object.fromEntries(connectionStatus)
      });
      return state;
    } else {
      // 没有 sessionId 映射（session 还未创建），先用 conversation id 查状态（临时状态）
      const convId = selectedConversation.value.id;
      const state = connectionStatus.get(convId) || 'disconnected';
      if (state !== 'disconnected') {
        console.log('[ConnectionState] Using conversation id for temp state:', {
          conversationId: convId,
          state,
          allStates: Object.fromEntries(connectionStatus)
        });
        return state;
      }
    }
  }
  // 如果没有选中的 conversation（新建会话场景），使用 store 中当前 session 的连接状态
  if (currentSessionId.value) {
    const state = connectionStatus.get(currentSessionId.value) || 'disconnected';
    console.log('[ConnectionState] Using session state:', {
      sessionId: currentSessionId.value,
      state,
      allStates: Object.fromEntries(connectionStatus)
    });
    return state;
  }
  console.log('[ConnectionState] No session or conversation, returning disconnected');
  return 'disconnected';
});

// 当前会话的连接状态文本
const currentConnectionStateText = computed(() => {
  switch (currentConversationConnectionState.value) {
    case 'connecting':
      return '建立连接中...';
    case 'connected':
      return '已连接';
    case 'disconnected':
    default:
      return '未连接';
  }
});

// 是否可以发送消息（选中会话后允许直接输入，发送时会按需恢复或创建 session）
const canSendMessage = computed(() => {
  if (!currentProject.value?.path) return false;
  if (selectedConversation.value) return true;
  return currentSessionId.value !== null && currentConversationConnectionState.value !== 'disconnected';
});

// 待处理的权限请求
const pendingPermissions = computed(() => {
  if (!currentSessionId.value) return [];
  // Pinia 3 setup store 中 reactive() 会自动解包 ref，直接访问即可
  const allPerms = claudeStore.pendingPermissions;
  if (!allPerms) return [];
  const perms = allPerms.get(currentSessionId.value);
  return perms ? Array.from(perms.values()) : [];
});

const permissionRequestsInFlight = ref<Set<string>>(new Set());

function resolvePermissionSessionId(requestId: string): string | null {
  if (currentSessionId.value) {
    const sessionPermissions = claudeStore.pendingPermissions.get(currentSessionId.value);
    if (sessionPermissions?.has(requestId)) {
      return currentSessionId.value;
    }
  }
  return claudeStore.findPermissionSessionId(requestId);
}

function markPermissionRequestInFlight(requestId: string): boolean {
  if (permissionRequestsInFlight.value.has(requestId)) {
    console.warn('[PERMISSION] Ignoring duplicate permission action:', requestId);
    return false;
  }

  const next = new Set(permissionRequestsInFlight.value);
  next.add(requestId);
  permissionRequestsInFlight.value = next;
  return true;
}

function clearPermissionRequestInFlight(requestId: string): void {
  if (!permissionRequestsInFlight.value.has(requestId)) return;
  const next = new Set(permissionRequestsInFlight.value);
  next.delete(requestId);
  permissionRequestsInFlight.value = next;
}

async function syncSessionPermissionMode(sessionId: string): Promise<void> {
  const conversation = getSessionConversation(sessionId);
  if (!conversation || !hasConversationInList(conversation.id)) {
    console.log('[PERMISSION] Skip sync after connection because session is not a persisted history conversation:', {
      sessionId,
      conversationId: conversation?.id ?? null,
    });
    return;
  }

  const sessionPermissionMode = getConversationPermissionMode(conversation);

  try {
    await invoke('set_permission_mode', { sessionId, mode: sessionPermissionMode });
    console.log('[PERMISSION] Synced permission mode after connection:', {
      sessionId,
      conversationId: conversation.id,
      mode: sessionPermissionMode,
    });
  } catch (error) {
    console.error('[PERMISSION] Failed to sync permission mode after connection:', {
      sessionId,
      conversationId: conversation.id,
      mode: sessionPermissionMode,
      error,
    });
  }
}

async function syncSessionPermissionModeBeforeFirstMessage(sessionId: string): Promise<boolean> {
  const sessionMessages = getSessionMessagesSnapshot(sessionId);
  const hasReadableMessages = sessionMessages.some(message => message.role !== 'system');

  if (hasReadableMessages) {
    console.log('[PERMISSION] Skip sync before first message because session already has readable messages:', {
      sessionId,
      messageCount: sessionMessages.length,
    });
    return true;
  }

  const selectedConversationPermissionMode = selectedConversation.value
    && matchesConversationSession(selectedConversation.value, sessionId)
    ? getConversationPermissionMode(selectedConversation.value)
    : null;
  const sessionPermissionMode =
    selectedConversationPermissionMode
    || claudeStore.sessions.get(sessionId)?.permissionMode
    || conversations.value.find(c => c.id === sessionId)?.permissionMode
    || 'default';

  try {
    await invoke('set_permission_mode', { sessionId, mode: sessionPermissionMode });
    console.log('[PERMISSION] Initialized permission mode before first message:', {
      sessionId,
      mode: sessionPermissionMode,
    });
    return true;
  } catch (error) {
    console.error('[PERMISSION] Failed to sync permission mode before first message:', {
      sessionId,
      mode: sessionPermissionMode,
      error,
    });
    messages.value.push({
      id: `permission_sync_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`,
      role: 'assistant',
      content: '发送失败：首条消息发送前同步权限模式失败，请重试。',
      timestamp: Date.now(),
    });
    return false;
  }
}

async function resolveLatestSessionIdForSend(originalSessionId: string): Promise<string> {
  for (let attempt = 0; attempt < 10; attempt += 1) {
    const selectedConversationId = selectedConversation.value?.id;
    const mappedSessionId = selectedConversationId
      ? conversationSessionMap.value.get(selectedConversationId) || null
      : null;
    const updatedSelectedSessionId = selectedConversationId && selectedConversationId !== originalSessionId
      ? selectedConversationId
      : null;
    const updatedCurrentSessionId = currentSessionId.value && currentSessionId.value !== originalSessionId
      ? currentSessionId.value
      : null;
    const resolvedSessionId = mappedSessionId || updatedSelectedSessionId || updatedCurrentSessionId;

    if (resolvedSessionId) {
      if (resolvedSessionId !== originalSessionId) {
        console.log('[SEND] Resolved remapped session ID after permission sync:', {
          originalSessionId,
          resolvedSessionId,
          attempt,
        });
      }
      return resolvedSessionId;
    }

    await nextTick();
    if (attempt < 9) {
      await new Promise((resolve) => window.setTimeout(resolve, 20));
    }
  }

  return originalSessionId;
}

// 批准权限请求（仅批准当前）
async function approvePermission(requestId: string, updatedInput?: Record<string, unknown>) {
  const targetSessionId = resolvePermissionSessionId(requestId);
  if (!targetSessionId || !markPermissionRequestInFlight(requestId)) return;

  try {
    await invoke('respond_to_permission', {
      sessionId: targetSessionId,
      requestId,
      action: 'approve',
      updatedInput: updatedInput || undefined,
    });
    claudeStore.removePermission(targetSessionId, requestId);
    console.log('Permission approved:', requestId);
  } catch (error) {
    console.error('Failed to approve permission:', error);
  } finally {
    clearPermissionRequestInFlight(requestId);
  }
}

// 批准权限请求（不再询问此工具）
async function approvePermissionAlways(requestId: string) {
  const targetSessionId = resolvePermissionSessionId(requestId);
  if (!targetSessionId || !markPermissionRequestInFlight(requestId)) return;

  try {
    await invoke('respond_to_permission', {
      sessionId: targetSessionId,
      requestId,
      action: 'approve_always',
    });
    claudeStore.removePermission(targetSessionId, requestId);
    console.log('Permission approved always:', requestId);
  } catch (error) {
    console.error('Failed to approve permission always:', error);
  } finally {
    clearPermissionRequestInFlight(requestId);
  }
}

// 拒绝权限请求
async function rejectPermission(requestId: string, reason?: string) {
  const targetSessionId = resolvePermissionSessionId(requestId);
  if (!targetSessionId || !markPermissionRequestInFlight(requestId)) return;

  try {
    await invoke('respond_to_permission', {
      sessionId: targetSessionId,
      requestId,
      action: 'reject',
      reason: reason?.trim() || 'Rejected by user',
    });
    claudeStore.removePermission(targetSessionId, requestId);
    console.log('Permission rejected:', requestId);

    // 拒绝权限后停止流式响应
    await stopStreaming();
  } catch (error) {
    console.error('Failed to reject permission:', error);
  } finally {
    clearPermissionRequestInFlight(requestId);
  }
}

// 刷新活动 Session 列表
async function refreshActiveSessions() {
  try {
    console.log('[ActiveSessions] Fetching active sessions...');
    const sessions = await invoke<string[]>('get_active_sessions');
    const visibleSessions = sessions.filter(shouldExposeActiveSession);
    const existingOrder = activeSessions.value.filter(sessionId => visibleSessions.includes(sessionId));
    const appendedSessions = visibleSessions.filter(sessionId => !existingOrder.includes(sessionId));
    activeSessions.value = [...existingOrder, ...appendedSessions];
    console.log('[ActiveSessions] Refreshed:', activeSessions.value, 'Count:', activeSessions.value.length);
  } catch (error) {
    console.error('[ActiveSessions] Failed to get active sessions:', error);
  }
}

function handleTabDragStart(sessionId: string, event: DragEvent) {
  if (isDraftSessionTab(sessionId)) {
    event.preventDefault();
    return;
  }

  draggedTabSessionId.value = sessionId;
  dragOverTabSessionId.value = null;
  dragOverTabPosition.value = null;
  suppressTabClick.value = true;

  if (event.dataTransfer) {
    event.dataTransfer.effectAllowed = 'move';
    event.dataTransfer.setData('text/plain', sessionId);
  }
}

function handleTabDragOver(targetSessionId: string, event: DragEvent) {
  const draggedSessionId = draggedTabSessionId.value;
  if (!draggedSessionId) return;

  event.preventDefault();

  if (draggedSessionId === targetSessionId) {
    dragOverTabSessionId.value = null;
    dragOverTabPosition.value = null;
    return;
  }

  const tabElement = event.currentTarget as HTMLElement | null;
  if (!tabElement) return;

  const { left, width } = tabElement.getBoundingClientRect();
  const relativeX = event.clientX - left;
  const position = relativeX < width / 2 ? 'before' : 'after';

  dragOverTabSessionId.value = targetSessionId;
  dragOverTabPosition.value = position;

  if (event.dataTransfer) {
    event.dataTransfer.dropEffect = 'move';
  }
}

function handleTabDragLeave(targetSessionId: string) {
  if (dragOverTabSessionId.value !== targetSessionId) return;
  dragOverTabSessionId.value = null;
  dragOverTabPosition.value = null;
}

function handleTabDrop(targetSessionId: string, event: DragEvent) {
  event.preventDefault();

  const draggedSessionId = draggedTabSessionId.value;
  const insertPosition = dragOverTabPosition.value;

  if (!draggedSessionId || draggedSessionId === targetSessionId || !insertPosition) {
    dragOverTabSessionId.value = null;
    dragOverTabPosition.value = null;
    return;
  }

  const currentTabs = [...activeSessions.value];
  const fromIndex = currentTabs.indexOf(draggedSessionId);
  const targetIndex = currentTabs.indexOf(targetSessionId);

  if (fromIndex === -1 || targetIndex === -1) {
    draggedTabSessionId.value = null;
    dragOverTabSessionId.value = null;
    dragOverTabPosition.value = null;
    return;
  }

  const [draggedTab] = currentTabs.splice(fromIndex, 1);
  const adjustedTargetIndex = currentTabs.indexOf(targetSessionId);
  const insertIndex = insertPosition === 'before' ? adjustedTargetIndex : adjustedTargetIndex + 1;
  currentTabs.splice(insertIndex, 0, draggedTab);
  activeSessions.value = currentTabs;
  draggedTabSessionId.value = null;
  dragOverTabSessionId.value = null;
  dragOverTabPosition.value = null;
}

function handleTabClick(sessionId: string) {
  if (isDraftSessionTab(sessionId)) {
    return;
  }

  if (suppressTabClick.value) {
    suppressTabClick.value = false;
    return;
  }

  switchToSession(sessionId);
}

function handleTabDragEnd() {
  draggedTabSessionId.value = null;
  dragOverTabSessionId.value = null;
  dragOverTabPosition.value = null;
  window.setTimeout(() => {
    suppressTabClick.value = false;
  }, 0);
}

// 切换到指定 Session
async function switchToSession(sessionId: string) {
  console.log('[SessionTab] Switching to session:', sessionId);

  // 清除该 session 的未读通知
  claudeStore.clearUnreadTaskCompletion(sessionId);
  claudeStore.clearUnreadMessageCompletion(sessionId);
  console.log('[SessionTab] Cleared unread notifications for session:', sessionId);

  // 查找对应的 conversation
  // sessionId 可能是 conversation.id 也可能是实际的 sessionId
  let targetConversation = conversations.value.find(c => c.id === sessionId);

  if (!targetConversation) {
    targetConversation = findDraftConversationBySessionId(sessionId) || undefined;
  }

  // 如果直接没找到，尝试通过映射查找
  if (!targetConversation) {
    for (const [convId, sessId] of conversationSessionMap.value.entries()) {
      if (sessId === sessionId) {
        targetConversation = conversations.value.find(c => c.id === convId);
        break;
      }
    }
  }

  if (targetConversation) {
    console.log('[SessionTab] Found conversation:', targetConversation.id, targetConversation.title);
    await selectConversation(targetConversation);
  } else {
    console.warn('[SessionTab] No conversation found for session:', sessionId);
  }
}

function getActualSessionIdForConversation(conversationId: string): string | null {
  if (activeSessions.value.includes(conversationId)) {
    return conversationId;
  }

  return conversationSessionMap.value.get(conversationId) || null;
}

function getSessionPermissionKeys(sessionId: string): string[] {
  const keys = new Set<string>([sessionId]);

  const mappedSessionId = conversationSessionMap.value.get(sessionId);
  if (mappedSessionId) {
    keys.add(mappedSessionId);
  }

  for (const [conversationId, actualSessionId] of conversationSessionMap.value.entries()) {
    if (conversationId === sessionId || actualSessionId === sessionId) {
      keys.add(conversationId);
      keys.add(actualSessionId);
    }
  }

  return Array.from(keys);
}

function resetChatSelection() {
  selectedConversation.value = null;
  messages.value = [];
  originalConversationTitle.value = '';
  currentSessionId.value = null;
  claudeStore.setCurrentSession(null);
}

async function activateFallbackSession(removedSessionId: string, previousSessions: string[]) {
  const removedIndex = previousSessions.indexOf(removedSessionId);
  const remainingSessions = previousSessions.filter(id => id !== removedSessionId);
  activeSessions.value = remainingSessions;

  const removedConversation = getSessionConversation(removedSessionId);
  const removedConversationId = removedConversation?.id;
  const isCurrentSessionRemoved = currentSessionId.value === removedSessionId;
  const isSelectedConversationRemoved = removedConversationId && selectedConversation.value?.id === removedConversationId;

  if (!isCurrentSessionRemoved && !isSelectedConversationRemoved) {
    return;
  }

  if (remainingSessions.length === 0) {
    resetChatSelection();
    return;
  }

  const fallbackIndex = Math.min(removedIndex, remainingSessions.length - 1);
  const fallbackSessionId = remainingSessions[Math.max(fallbackIndex, 0)];
  await switchToSession(fallbackSessionId);
}

function removeConversationSessionMapping(conversationId: string) {
  if (!conversationSessionMap.value.has(conversationId)) return;
  const nextMap = new Map(conversationSessionMap.value);
  nextMap.delete(conversationId);
  conversationSessionMap.value = nextMap;
}

// 关闭指定 Session
async function closeSession(sessionId: string) {
  console.log('[SessionTab] Closing session:', sessionId);
  const previousSessions = [...activeSessions.value];
  try {
    await invoke('close_session', { sessionId });
    await activateFallbackSession(sessionId, previousSessions);
    claudeStore.removeSession(sessionId);

    const draftConversation = findDraftConversationBySessionId(sessionId);
    if (draftConversation) {
      clearProjectDraftConversation(draftConversation.projectId, draftConversation.id);
    }

    console.log('[SessionTab] Session closed:', sessionId);
  } catch (error) {
    console.error('Failed to close session:', error);
  }
}

// 获取 Session 对应的消息列表
function getSessionMessages(sessionId: string): Message[] {
  if (sessionId === currentSessionId.value) {
    return messages.value;
  }
  return claudeStore.messages.get(sessionId) || [];
}

function extractMessagePreview(message: Message): string {
  const textParts: string[] = [];

  for (const block of message.contentBlocks || []) {
    if (block.type === 'text') {
      textParts.push(block.text);
    }
  }

  const blockText = textParts.join(' ').trim();
  const content = (blockText || message.content || '')
    .replace(/\s+/g, ' ')
    .trim();

  return content;
}

function getSessionConversation(sessionId: string): Conversation | undefined {
  if (selectedConversation.value?.id === sessionId) {
    return selectedConversation.value;
  }

  const draftConversation = findDraftConversationBySessionId(sessionId);
  if (draftConversation) return draftConversation;

  const directConversation = conversations.value.find(conv => conv.id === sessionId);
  if (directConversation) return directConversation;

  for (const [convId, mappedSessionId] of conversationSessionMap.value.entries()) {
    if (mappedSessionId === sessionId) {
      return conversations.value.find(conv => conv.id === convId);
    }
  }

  return undefined;
}

function getSessionProjectName(sessionId: string): string {
  const conversation = getSessionConversation(sessionId);
  if (!conversation) return '';

  const project = projects.value.find(item => item.id === conversation.projectId);
  return project?.name || '';
}

// 获取 Session 显示名称
function getSessionDisplayName(sessionId: string): string {
  if (isDraftSessionTab(sessionId)) {
    return '新会话（初始化中）';
  }

  const conversationTitle = getSessionConversation(sessionId)?.title?.trim();
  if (conversationTitle && !['Untitled', '新对话', 'New Conversation'].includes(conversationTitle)) {
    return conversationTitle;
  }

  const sessionMessages = getSessionMessages(sessionId);
  const firstReadableMessage = sessionMessages.find(message => {
    if (message.role === 'system') return false;
    return Boolean(extractMessagePreview(message));
  });

  const preview = firstReadableMessage ? extractMessagePreview(firstReadableMessage) : '';
  if (preview) return preview;

  const name = claudeStore.sessionNames.get(sessionId);
  if (name) return name;

  return sessionId.slice(0, 8);
}

function getSessionTabTooltip(sessionId: string): string {
  if (isDraftSessionTab(sessionId)) {
    return '新会话正在初始化，待最终 session ID 返回后会替换为真实标签';
  }

  const projectName = getSessionProjectName(sessionId);
  const displayName = getSessionDisplayName(sessionId);
  return projectName ? `${projectName}
${displayName}` : displayName;
}

// 获取 Tab 状态类名
function getTabStatusClass(sessionId: string): string {
  if (isDraftSessionTab(sessionId)) {
    return 'connecting';
  }

  // 优先检查是否正在流式输出
  if (claudeStore.streaming.has(sessionId)) {
    return 'streaming';
  }
  // 否则返回连接状态
  return claudeStore.connectionStatus.get(sessionId) || 'disconnected';
}

// 判断 Tab 是否有完成状态（任务完成或消息完成）
function hasTabCompletion(sessionId: string): boolean {
  if (isDraftSessionTab(sessionId)) {
    return false;
  }

  const hasTask = claudeStore.hasUnreadTaskCompletion(sessionId);
  const hasMessage = claudeStore.hasUnreadMessageCompletion(sessionId);
  return hasTask || hasMessage;
}

// 判断 Tab 是否有待批准的权限申请
function hasPendingPermission(sessionId: string): boolean {
  if (isDraftSessionTab(sessionId)) {
    return false;
  }

  return getSessionPermissionKeys(sessionId).some((key) => {
    const sessionPermissions = claudeStore.pendingPermissions.get(key);
    return Boolean(sessionPermissions && sessionPermissions.size > 0);
  });
}


// 获取当前项目（优先从选中会话获取，否则从展开的项目获取）
const currentProject = computed(() => {
  // 优先从选中的会话获取项目
  const conv = selectedConversation.value;
  if (conv) {
    return projects.value.find(p => p.id === conv.projectId);
  }

  // 新对话场景：从展开的项目中获取（通常只有一个展开的项目）
  const expandedIds = Array.from(expandedProjects.value);
  if (expandedIds.length > 0) {
    // 优先返回最近展开的项目
    const projectId = expandedIds[expandedIds.length - 1];
    return projects.value.find(p => p.id === projectId);
  }

  return null;
});

const rewindTurns = computed<RewindTurn[]>(() => parseRewindTurns(messages.value));
const todoPanelState = computed(() => extractTodoWritePanelState(messages.value));
const isSessionSearchVisible = ref(false);
const sessionSearchQuery = ref('');
const sessionSearchMatchCount = ref(0);
const sessionSearchActiveIndex = ref(0);
const sessionSearchInputRef = ref<HTMLInputElement | null>(null);
const conversationSelectionRequestId = ref(0);

const hasSessionSearchQuery = computed(() => sessionSearchQuery.value.trim().length > 0);
const hasSessionSearchResults = computed(() => sessionSearchMatchCount.value > 0);
const sessionSearchStatusText = computed(() => {
  if (!hasSessionSearchQuery.value) return '搜索当前会话';
  if (!hasSessionSearchResults.value) return '无结果';
  return `${sessionSearchActiveIndex.value + 1} / ${sessionSearchMatchCount.value}`;
});

function isConversationSelectionStillCurrent(requestId: number, conversationId: string): boolean {
  return conversationSelectionRequestId.value === requestId
    && selectedConversation.value?.id === conversationId;
}

// 当前项目名称
const currentProjectName = computed(() => {
  return currentProject.value?.name || '';
});

const shouldShowWelcomePage = computed(() => !isSettingsVisible.value && !selectedConversation.value && activeSessions.value.length === 0);

const canShowWorkspace = computed(() => Boolean(currentProject.value?.path));

async function showSessionSearch(select = true) {
  isSessionSearchVisible.value = true;
  await nextTick();
  focusSessionSearchInput(select);
}

function focusSessionSearchInput(select = true) {
  if (!sessionSearchInputRef.value) return;
  sessionSearchInputRef.value.focus();
  if (select) {
    sessionSearchInputRef.value.select();
  }
}

function handleSearchResultsChange(payload: { count: number }) {
  sessionSearchMatchCount.value = payload.count;

  if (payload.count === 0) {
    sessionSearchActiveIndex.value = 0;
    return;
  }

  if (sessionSearchActiveIndex.value >= payload.count) {
    sessionSearchActiveIndex.value = 0;
  }
}

function goToSessionSearchMatch(direction: 1 | -1) {
  if (!hasSessionSearchResults.value) return;

  const count = sessionSearchMatchCount.value;
  const nextIndex = sessionSearchActiveIndex.value + direction;
  sessionSearchActiveIndex.value = nextIndex < 0 ? count - 1 : nextIndex % count;
}

function clearSessionSearch() {
  sessionSearchQuery.value = '';
  sessionSearchMatchCount.value = 0;
  sessionSearchActiveIndex.value = 0;
}

function hideSessionSearch() {
  clearSessionSearch();
  isSessionSearchVisible.value = false;
}

function handleWindowSearchShortcut(event: KeyboardEvent) {
  const isSearchShortcut = (event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'f';

  if (isSearchShortcut) {
    if (shouldShowWelcomePage.value || isSettingsVisible.value) return;
    event.preventDefault();
    if (isSessionSearchVisible.value) {
      hideSessionSearch();
      return;
    }
    void showSessionSearch(true);
    return;
  }

  if (document.activeElement !== sessionSearchInputRef.value) return;

  if (event.key === 'Enter') {
    event.preventDefault();
    goToSessionSearchMatch(event.shiftKey ? -1 : 1);
    return;
  }

  if (event.key === 'Escape') {
    event.preventDefault();
    if (sessionSearchQuery.value) {
      clearSessionSearch();
    } else {
      hideSessionSearch();
    }
  }
}

const toggleWorkspacePanel = () => {
  if (!canShowWorkspace.value) return;
  isWorkspaceVisible.value = !isWorkspaceVisible.value;
  if (isWorkspaceVisible.value) {
    requestAnimationFrame(() => {
      syncWorkspacePanelWidth();
    });
  }
};

function upsertConversationInList(conversation: Conversation) {
  const existingIndex = conversations.value.findIndex((item) => item.id === conversation.id);

  if (existingIndex >= 0) {
    conversations.value[existingIndex] = {
      ...conversations.value[existingIndex],
      ...conversation,
    };
  } else {
    conversations.value.unshift(conversation);
  }

  conversations.value.sort((a, b) => b.id.localeCompare(a.id));
}

watch(() => sessionSearchQuery.value, (value, previousValue) => {
  if (value.trim() !== previousValue.trim()) {
    sessionSearchActiveIndex.value = 0;
  }
});

watch(() => selectedConversation.value?.id, () => {
  hideSessionSearch();
});

function persistSelectedConversationAfterFirstSend() {
  const activeConversation = selectedConversation.value;
  if (!activeConversation) return;

  const conversationId = activeConversation.id;
  if (!conversationId) return;

  const mappedSessionId = conversationSessionMap.value.get(conversationId);
  if (conversationId.startsWith('temp-') || (mappedSessionId && claudeStore.isSessionPending(mappedSessionId))) {
    return;
  }

  clearProjectDraftConversation(activeConversation.projectId, conversationId);
  if (hasConversationInList(conversationId)) return;

  const firstReadableMessage = messages.value.find((message) => {
    if (message.role === 'system') return false;
    return Boolean(extractMessagePreview(message));
  });

  const previewTitle = firstReadableMessage
    ? extractMessagePreview(firstReadableMessage).slice(0, 50)
    : '';

  const persistedConversation: Conversation = {
    ...activeConversation,
    id: conversationId,
    title: previewTitle || activeConversation.title,
    timestamp: Math.floor(Date.now() / 1000),
    messageCount: Math.max(activeConversation.messageCount, messages.value.length),
  };

  upsertConversationInList(persistedConversation);
  selectedConversation.value = persistedConversation;
  originalConversationTitle.value = persistedConversation.title;
  void refreshActiveSessions();
}

watch(
  () => isWorkspaceVisible.value,
  (visible) => {
    if (!visible) {
      stopWorkspaceResize();
      return;
    }

    requestAnimationFrame(() => {
      syncWorkspacePanelWidth();
    });
  },
);

watch(
  () => currentProject.value?.path,
  async (projectPath) => {
    if (!projectPath) {
      currentGitInfo.value = null;
      return;
    }

    try {
      currentGitInfo.value = await invoke<GitInfo>('get_git_info', {
        projectPath,
      });
    } catch (error) {
      console.error('加载 Git 信息失败:', error);
      currentGitInfo.value = null;
    }
  },
  { immediate: true }
);

// 原始会话标题（用于回退）
const originalConversationTitle = ref<string>('');

// 截取对话标题（统一使用后端返回的 conv.title，不再前端重新解析）
const truncatedTitle = computed(() => {
  // 新对话场景：没有选中会话
  if (!selectedConversation.value) {
    return '新对话';
  }

  // 直接使用后端返回的标题
  const title = selectedConversation.value.title;
  if (title && title !== 'Untitled') {
    return title.length > 50 ? title.substring(0, 50) + '...' : title;
  }

  // 回退到原始标题
  if (originalConversationTitle.value) {
    const origTitle = originalConversationTitle.value;
    return origTitle.length > 50 ? origTitle.substring(0, 50) + '...' : origTitle;
  }

  return '选择一个对话';
});

// 当前显示的 sessionId（优先使用 selectedConversation 的，否则使用 currentSessionId）
const displaySessionId = computed(() => {
  const activeConversation = selectedConversation.value;
  if (activeConversation) {
    const mappedSessionId = conversationSessionMap.value.get(activeConversation.id);
    if (activeConversation.id.startsWith('temp-') || (mappedSessionId && claudeStore.isSessionPending(mappedSessionId))) {
      return '等待初始化...';
    }
    return activeConversation.id;
  }
  return currentSessionId.value || '等待创建...';
});

const messageBindingSessionId = computed(() => {
  if (selectedConversation.value) {
    return selectedConversation.value.id;
  }
  return currentSessionId.value || '';
});

const isDraftConversation = computed(() => {
  const activeConversation = selectedConversation.value;
  return !!activeConversation
    && activeConversation.id.startsWith('temp-')
    && !hasConversationInList(activeConversation.id);
});

const visibleSessionTabs = computed(() => {
  if (isDraftConversation.value && selectedConversation.value) {
    return [selectedConversation.value.id];
  }
  return activeSessions.value;
});

const shouldShowSessionTabs = computed(() => {
  if (isDraftConversation.value) return visibleSessionTabs.value.length > 0;
  return visibleSessionTabs.value.length > 1;
});

function isDraftSessionTab(sessionId: string): boolean {
  return isDraftConversation.value && selectedConversation.value?.id === sessionId;
}

const sessionFilePath = ref('');
const isCopyingSessionPath = ref(false);
const sessionPathCopied = ref(false);
const isReloadingHistory = ref(false);
const sessionFileFingerprint = ref<string | null>(null);
const isPollingSessionFileChanges = ref(false);
let sessionPathCopiedTimer: ReturnType<typeof setTimeout> | null = null;
let sessionFileWatchTimer: ReturnType<typeof setInterval> | null = null;

const clearSessionPathCopiedState = () => {
  if (sessionPathCopiedTimer !== null) {
    clearTimeout(sessionPathCopiedTimer);
    sessionPathCopiedTimer = null;
  }
  sessionPathCopied.value = false;
};

const refreshSessionFilePath = async () => {
  const projectPath = currentProject.value?.path;
  const sessionId = displaySessionId.value;

  if (!projectPath || !sessionId || sessionId === '等待创建...') {
    sessionFilePath.value = '';
    return;
  }

  try {
    sessionFilePath.value = await invoke<string>('get_session_file_path', {
      projectPath,
      sessionId,
    });
  } catch (error) {
    console.error('获取 session 文件路径失败:', error);
    sessionFilePath.value = '';
  }
};

const buildSessionFileFingerprint = (metadata: SessionFileMetadata | null): string | null => {
  if (!metadata?.exists) return null;
  return `${metadata.modifiedAtMs ?? 'unknown'}:${metadata.size}`;
};

const getConversationSessionFileMetadata = async (
  conversation: Conversation,
): Promise<SessionFileMetadata | null> => {
  const project = projects.value.find(p => p.id === conversation.projectId);
  if (!project) return null;

  try {
    return await invoke<SessionFileMetadata>('get_session_file_metadata', {
      projectPath: project.path,
      sessionId: conversation.id,
    });
  } catch (error) {
    console.error('获取 session 文件元信息失败:', error);
    return null;
  }
};

const syncSessionFileFingerprint = async (conversation: Conversation) => {
  const metadata = await getConversationSessionFileMetadata(conversation);
  sessionFileFingerprint.value = buildSessionFileFingerprint(metadata);
};

const reloadConversationHistory = async (
  conversation: Conversation,
  options: {
    forceApply?: boolean;
    preserveCurrentMessagesOnError?: boolean;
    selectionRequestId?: number;
  } = {},
) => {
  const project = projects.value.find(p => p.id === conversation.projectId);
  if (!project) {
    console.error('Project not found for conversation:', conversation.id);
    return false;
  }

  const fallbackModel = resolveConversationProvider(conversation).model;
  const targetSessionId = conversationSessionMap.value.get(conversation.id) || conversation.id;

  try {
    const historyMessages = await invoke<any[]>('load_session_messages', {
      projectPath: project.path,
      sessionId: conversation.id,
    });

    const { restoredMessages, latestHistoryModelUsage } = restoreHistoryMessages(
      historyMessages,
      fallbackModel,
    );

    claudeStore.setMessages(targetSessionId, cloneMessages(restoredMessages));

    const canApplyToCurrentView = options.selectionRequestId == null
      || isConversationSelectionStillCurrent(options.selectionRequestId, conversation.id);

    if ((options.forceApply || targetSessionId === currentSessionId.value) && canApplyToCurrentView) {
      applySessionMessagesToCurrentView(targetSessionId, restoredMessages);
    }

    if (latestHistoryModelUsage) {
      claudeStore.setSessionModelUsage(targetSessionId, latestHistoryModelUsage);
    }

    console.log(`[History] Reloaded ${restoredMessages.length} messages for session ${conversation.id}`);
    return true;
  } catch (error) {
    console.error('[History] Failed to reload conversation history:', error);

    const canApplyErrorToCurrentView = options.selectionRequestId == null
      || isConversationSelectionStillCurrent(options.selectionRequestId, conversation.id);

    if ((options.forceApply || (!options.preserveCurrentMessagesOnError && targetSessionId === currentSessionId.value)) && canApplyErrorToCurrentView) {
      const errorMessage = buildHistoryLoadErrorMessage(error);
      claudeStore.setMessages(targetSessionId, [cloneMessage(errorMessage)]);
      applySessionMessagesToCurrentView(targetSessionId, [errorMessage]);
    }

    return false;
  }
};

const pollSelectedSessionFileChanges = async () => {
  const conversation = selectedConversation.value;
  if (!conversation || isDraftConversation.value) return;
  if (!isPageVisible.value || isReloadingHistory.value || isStreaming.value) return;
  if (isPollingSessionFileChanges.value) return;

  isPollingSessionFileChanges.value = true;
  try {
    const metadata = await getConversationSessionFileMetadata(conversation);
    const nextFingerprint = buildSessionFileFingerprint(metadata);

    if (sessionFileFingerprint.value === null) {
      sessionFileFingerprint.value = nextFingerprint;
      return;
    }

    if (nextFingerprint === null || nextFingerprint === sessionFileFingerprint.value) {
      return;
    }

    const reloaded = await reloadConversationHistory(conversation, {
      forceApply: true,
      preserveCurrentMessagesOnError: true,
    });

    if (reloaded) {
      sessionFileFingerprint.value = nextFingerprint;
    }
  } finally {
    isPollingSessionFileChanges.value = false;
  }
};

const stopSessionFileWatcher = (resetFingerprint = true) => {
  if (sessionFileWatchTimer !== null) {
    clearInterval(sessionFileWatchTimer);
    sessionFileWatchTimer = null;
  }

  if (resetFingerprint) {
    sessionFileFingerprint.value = null;
  }
};

const startSessionFileWatcher = async () => {
  const conversation = selectedConversation.value;
  if (!conversation || isDraftConversation.value) {
    stopSessionFileWatcher(true);
    return;
  }

  if (sessionFileWatchTimer === null) {
    sessionFileWatchTimer = setInterval(() => {
      void pollSelectedSessionFileChanges();
    }, SESSION_FILE_POLL_INTERVAL_MS);
  }

  await syncSessionFileFingerprint(conversation);
};

const copySessionFilePath = async () => {
  if (!sessionFilePath.value || isCopyingSessionPath.value) return;

  try {
    isCopyingSessionPath.value = true;
    await navigator.clipboard.writeText(sessionFilePath.value);
    clearSessionPathCopiedState();
    sessionPathCopied.value = true;
    sessionPathCopiedTimer = setTimeout(() => {
      sessionPathCopied.value = false;
      sessionPathCopiedTimer = null;
    }, 2000);
  } catch (error) {
    console.error('复制 session 文件路径失败:', error);
  } finally {
    isCopyingSessionPath.value = false;
  }
};

watch(
  [() => currentProject.value?.path, () => displaySessionId.value],
  () => {
    clearSessionPathCopiedState();
    refreshSessionFilePath();
  },
  { immediate: true }
);

watch(
  () => [selectedConversation.value?.id, currentProject.value?.path, isDraftConversation.value] as const,
  async ([conversationId, projectPath, draft]) => {
    if (!conversationId || !projectPath || draft) {
      stopSessionFileWatcher(true);
      return;
    }

    stopSessionFileWatcher(true);
    await startSessionFileWatcher();
  },
  { immediate: true }
);

// 权限模式状态
const permissionMode = computed<PermissionMode>(() => {
  return claudeStore.currentSession?.permissionMode || selectedConversation.value?.permissionMode || claudeStore.defaultPermissionMode;
});
const isPlanMode = computed(() => permissionMode.value === 'plan');


// 事件监听器
let unlistenConnectionStatus: UnlistenFn | null = null;
let unlistenSessionStatus: UnlistenFn | null = null;
let unlistenStreamStart: UnlistenFn | null = null;
let unlistenStreamData: UnlistenFn | null = null;
let unlistenStreamUsage: UnlistenFn | null = null;
let unlistenStreamEnd: UnlistenFn | null = null;
let unlistenStreamError: UnlistenFn | null = null;
let unlistenMessage: UnlistenFn | null = null;
let unlistenSessionCreated: UnlistenFn | null = null;
let unlistenSessionIdUpdated: UnlistenFn | null = null;
let unlistenCommandsUpdated: UnlistenFn | null = null;
let unlistenUserCheckpoint: UnlistenFn | null = null;
let unlistenSubagentToolUse: UnlistenFn | null = null;
let unlistenSubagentToolInputDelta: UnlistenFn | null = null;
let unlistenSubagentToolResultStart: UnlistenFn | null = null;
let unlistenSubagentToolResultDelta: UnlistenFn | null = null;
let unlistenSubagentToolResultComplete: UnlistenFn | null = null;

// 连接状态刷新定时器
let connectionRefreshTimer: ReturnType<typeof setTimeout> | null = null;
let unlistenPermission: UnlistenFn | null = null;

// 页面可见性状态
const isPageVisible = ref(!document.hidden);

// 获取聊天记录项的连接状态
const getConversationConnectionStatus = (conv: Conversation) => {
  const sessionId = conversationSessionMap.value.get(conv.id);
  if (sessionId) {
    return claudeStore.connectionStatus.get(sessionId) || 'disconnected';
  }
  return claudeStore.connectionStatus.get(conv.id) || 'disconnected';
};

const hasConversationCompletion = (conv: Conversation) => {
  const sessionId = conversationSessionMap.value.get(conv.id) || conv.id;
  return claudeStore.hasUnreadTaskCompletion(sessionId) || claudeStore.hasUnreadMessageCompletion(sessionId);
};

// 获取连接状态文本
const getConnectionStatusText = (status: string) => {
  switch (status) {
    case 'connected': return '已连接';
    case 'connecting': return '连接中...';
    case 'disconnected': return '未连接';
    default: return '未连接';
  }
};

// 判断会话是否正在流式输出
const isConversationStreaming = (conv: Conversation) => {
  const sessionId = conversationSessionMap.value.get(conv.id) || conv.id;
  return claudeStore.streaming.has(sessionId);
};

const getConversationStatusDotTitle = (conv: Conversation) => {
  if (isConversationStreaming(conv)) return '正在执行...';
  if (hasConversationCompletion(conv)) return '执行完成';
  return getConnectionStatusText(getConversationConnectionStatus(conv));
};

// 播放完成提示音
const playCompletionSound = () => {
  try {
    // 使用 Web Audio API 创建一个简短的提示音
    const audioContext = new (window.AudioContext || (window as any).webkitAudioContext)();
    const oscillator = audioContext.createOscillator();
    const gainNode = audioContext.createGain();

    oscillator.connect(gainNode);
    gainNode.connect(audioContext.destination);

    oscillator.frequency.value = 800; // 频率
    oscillator.type = 'sine'; // 波形

    gainNode.gain.setValueAtTime(0.3, audioContext.currentTime);
    gainNode.gain.exponentialRampToValueAtTime(0.01, audioContext.currentTime + 0.1);

    oscillator.start(audioContext.currentTime);
    oscillator.stop(audioContext.currentTime + 0.1);
  } catch (error) {
    console.error('Failed to play completion sound:', error);
  }
};

// 设置事件监听
onMounted(async () => {
  window.addEventListener('keydown', handleWindowSearchShortcut);
  window.addEventListener('resize', syncWorkspacePanelWidth);
  document.addEventListener('mousedown', handleProjectSortOutsideClick);
  leftPanelWidth.value = clampLeftPanelWidth(leftPanelWidth.value);

  // 测试 Store 插件
  try {
    console.log('[Store] 测试 Store 插件...');
    const testStore = await Store.load('test.json');
    await testStore.set('test', { value: 'hello' });
    await testStore.save();
    console.log('[Store] Store 插件正常工作！');
  } catch (e) {
    console.error('[Store] Store 插件测试失败:', e);
  }

  // 加载保存的数据
  await loadSavedData();
  await providerStore.load();

  // 初始化 displayProjects
  displayProjects.value = [...projects.value];

  // 初始化斜杠命令（作为后备）
  slashesStore.initBuiltinCommands();

  // 刷新活动 Session 列表
  await refreshActiveSessions();

  // 监听连接状态变化
  unlistenConnectionStatus = await listen('claude:connection_status', (event: { payload: { sessionId: string, status: 'connecting' | 'connected' | 'disconnected' } }) => {
    console.log('[CONNECTION] Status event:', event.payload);
    claudeStore.setConnectionStatus(event.payload.sessionId, event.payload.status);

    // 当状态为 connected 时，检查是否需要刷新会话列表
    if (event.payload.status === 'connected') {
      void syncSessionPermissionMode(event.payload.sessionId);

      // 刷新活动 Session 列表
      refreshActiveSessions();

      // 使用防抖避免频繁刷新
      if (connectionRefreshTimer) {
        clearTimeout(connectionRefreshTimer);
      }

      connectionRefreshTimer = setTimeout(async () => {
        // 检查是否有待持久化的会话
        if (claudeStore.isSessionPending(event.payload.sessionId)) {
          // 找到对应的项目并刷新会话列表（在标记为持久化之前刷新，确保会话不会被过滤）
          const conversation = conversations.value.find(c => c.id === event.payload.sessionId);
          if (conversation) {
            console.log('[CONNECTION] Refreshing project sessions after connection established');
            await refreshProjectSessions(conversation.projectId);
          }

          // 刷新完成后再标记会话已持久化
          // 注意：实际的持久化标记应该在 session_created 事件中处理
          // 这里只是为了确保连接建立后的状态同步
        }
        connectionRefreshTimer = null;
      }, 150); // 150ms 防抖
    }
  });

  unlistenSessionStatus = await listen('claude:session_status', (event: { payload: { sessionId: string, status: 'running' | 'compacting' | 'idle' | null } }) => {
    const { sessionId, status } = event.payload;
    console.log('[SESSION STATUS] Event:', event.payload);

    claudeStore.setSessionStatus(sessionId, status ?? 'idle');
  });

  // 监听会话创建事件
  unlistenSessionCreated = await listen('claude:session_created', (event: { payload: { sessionId: string, projectPath: string } }) => {
    console.log('[SESSION] Session created event:', event.payload);
    if (hasConversationInList(event.payload.sessionId)) {
      claudeStore.markSessionPersisted(event.payload.sessionId);
      return;
    }

    console.log('[SESSION] Deferring persistence until final CLI session ID:', event.payload.sessionId);
  });

  unlistenCommandsUpdated = await listen('claude:commands_updated', (event: { payload: { sessionId: string, commands?: Array<{ name: string, description: string, argumentHint?: string | string[] }> | null } }) => {
    console.log('[COMMANDS] Commands updated:', event.payload);
    claudeStore.updateSession(event.payload.sessionId, {
      commands: event.payload.commands || undefined,
      updatedAt: Date.now(),
    });
  });

  // 监听 sessionID 更新事件
  unlistenSessionIdUpdated = await listen('session-id-updated', (event: { payload: { oldSessionId: string, newSessionId: string } }) => {
    console.log('[SessionID] Updated:', event.payload.oldSessionId, '->', event.payload.newSessionId);
    renameSessionStreamingUiState(event.payload.oldSessionId, event.payload.newSessionId);

    const nextSessionMap = new Map(conversationSessionMap.value);
    let remappedConversationId: string | null = null;
    for (const [conversationId, mappedSessionId] of nextSessionMap.entries()) {
      if (mappedSessionId === event.payload.oldSessionId) {
        nextSessionMap.set(conversationId, event.payload.newSessionId);
        remappedConversationId = conversationId;
      }
    }
    conversationSessionMap.value = nextSessionMap;

    // 更新当前会话ID（如果匹配）
    if (currentSessionId.value === event.payload.oldSessionId) {
      console.log('[SessionID] Updating current session ID');
      currentSessionId.value = event.payload.newSessionId;

      // 更新消息中的 sessionId
      messages.value.forEach(msg => {
        (msg as any).sessionId = event.payload.newSessionId;
      });

      // 更新 claude store 中的 session
      claudeStore.renameSession(event.payload.oldSessionId, event.payload.newSessionId);

      // 更新对话列表中的 sessionID
      const conversation = conversations.value.find(c => c.id === event.payload.oldSessionId);
      if (conversation) {
        conversation.id = event.payload.newSessionId;
        console.log('[SessionID] Updated conversation ID:', event.payload.newSessionId);
      }

      if (selectedConversation.value?.id === event.payload.oldSessionId) {
        selectedConversation.value = {
          ...selectedConversation.value,
          id: event.payload.newSessionId,
        };
      }

      for (const [projectId, conversation] of projectDraftConversations.value.entries()) {
        if (conversation.id !== event.payload.oldSessionId) continue;

        setProjectDraftConversation(projectId, {
          ...conversation,
          id: event.payload.newSessionId,
        });
        break;
      }

      if (selectedConversation.value && !hasConversationInList(event.payload.newSessionId)) {
        const firstReadableMessage = messages.value.find(message => {
          if (message.role === 'system') return false;
          return Boolean(extractMessagePreview(message));
        });
        const previewTitle = firstReadableMessage
          ? extractMessagePreview(firstReadableMessage).slice(0, 50)
          : '';
        const newConversation: Conversation = {
          ...selectedConversation.value,
          id: event.payload.newSessionId,
          title: previewTitle || selectedConversation.value.title,
          timestamp: Math.floor(Date.now() / 1000),
          messageCount: Math.max(selectedConversation.value.messageCount, messages.value.length),
        };

        upsertConversationInList(newConversation);
        selectedConversation.value = newConversation;
        originalConversationTitle.value = newConversation.title;
      }

      claudeStore.markSessionPersisted(event.payload.newSessionId);

      if (remappedConversationId && remappedConversationId.startsWith('temp-')) {
        for (const [projectId, conversation] of projectDraftConversations.value.entries()) {
          if (conversation.id === remappedConversationId) {
            clearProjectDraftConversation(projectId, remappedConversationId);
            break;
          }
        }
        removeConversationSessionMapping(remappedConversationId);
      }

      void refreshActiveSessions();

      // 保存更新后的数据（不使用 await，让它在后台执行）
      saveData().catch(err => console.error('[SessionID] Failed to save data:', err));
    }
  });

  // 监听流式开始
  unlistenStreamStart = await listen<string>('claude:stream_start', (event) => {
    const sessionId = event.payload;
    console.log('[STREAM] Stream start event received for session:', sessionId);
    clearSessionManualStop(sessionId);
    claudeStore.clearSessionSubagentRuntime(sessionId);
    claudeStore.setSessionStatus(sessionId, 'running');
    // 更新 claudeStore.streaming 状态
    claudeStore.setStreaming(sessionId, '');
    claudeStore.setStreamingStats(sessionId, {
      startedAt: claudeStore.streamingStartedAt.get(sessionId) || Date.now(),
      outputTokens: 0,
    });
    const sessionMessages = getSessionMessagesSnapshot(sessionId);
    const { sessionMessages: nextMessages, placeholder } = ensureStreamingPlaceholder(sessionId, sessionMessages);
    setSessionMessagesSnapshot(sessionId, nextMessages);
    setSessionReceivedStreamData(sessionId, false);
    setSessionReceivedStreamUsage(sessionId, false);
    setStreamingPlaceholderId(sessionId, placeholder.id);
  });

  // 监听流式数据
  unlistenStreamData = await listen<any>('claude:stream_data', (event) => {
    const { sessionId, data } = event.payload;
    const currentStreamingText = claudeStore.streaming.get(sessionId) || '';
    const nextStreamingText = currentStreamingText + data;

    claudeStore.setStreaming(sessionId, nextStreamingText);

    const sessionMessages = getSessionMessagesSnapshot(sessionId);
    const { sessionMessages: nextMessages, placeholder } = appendStreamingDeltaToPlaceholder(
      sessionId,
      sessionMessages,
      data,
    );
    setSessionMessagesSnapshot(sessionId, nextMessages);
    setSessionReceivedStreamData(sessionId, true);
    setStreamingPlaceholderId(sessionId, placeholder.id);
  });

  // 监听流式 usage，仅用于输入框 token 圆环
  unlistenStreamUsage = await listen<any>('claude:stream_usage', (event) => {
    const { sessionId, usage } = event.payload;
    if (!sessionId || !usage) return;

    setSessionReceivedStreamUsage(sessionId, true);
    updateSessionInputTokenUsage(sessionId, usage);
  });

  // 监听流式结束
  unlistenStreamEnd = await listen<any>('claude:stream_end', (event) => {
    const { sessionId, message } = event.payload;
    console.log('[STREAM] Stream end event received for session:', sessionId, message);
    const shouldSkipUsageRefresh = consumeSessionManualStop(sessionId);
    claudeStore.finalizeSessionSubagentRuntime(sessionId);

    claudeStore.setSessionStatus(sessionId, 'idle');

    // 清除该 session 的流式状态
    claudeStore.setStreaming(sessionId, null);
    claudeStore.setStreamingStats(sessionId, null);

    // 如果有多个活跃的 session，添加未读消息完成通知（用于 tab 红点提示）
    // 这个逻辑需要在返回检查之前，以确保非当前 session 也能收到通知
    console.log('[STREAM] Checking notification condition:', {
      activeSessions: activeSessions.value,
      activeSessionsLength: activeSessions.value.length,
      currentSessionId: currentSessionId.value,
      eventSessionId: sessionId,
    });

    if (activeSessions.value.length > 1 && sessionId) {
      claudeStore.addUnreadMessageCompletion(sessionId);
      console.log('[STREAM] Added unread message completion for session:', sessionId);
      console.log('[STREAM] unreadMessageCompletions after add:', Array.from(claudeStore.unreadMessageCompletions));
    } else {
      console.log('[STREAM] Skipped adding notification. Reason:', activeSessions.value.length <= 1 ? 'Only one active session' : 'No session ID');
    }

    if (sessionId !== currentSessionId.value) {
      const sessionMessages = getSessionMessagesSnapshot(sessionId);
      const placeholderIndex = findStreamingPlaceholderIndex(sessionId, sessionMessages);

      if (placeholderIndex !== -1) {
        if (!sessionMessages[placeholderIndex].content) {
          sessionMessages.splice(placeholderIndex, 1);
        } else {
          sessionMessages[placeholderIndex] = {
            ...sessionMessages[placeholderIndex],
            isStreaming: false,
          };
        }
        setSessionMessagesSnapshot(sessionId, sessionMessages);
      }

      console.log('[TOKEN] stream_end message:', message);
      finalizeSessionTokenUsage(sessionId, message, shouldSkipUsageRefresh);
      clearSessionStreamingUiState(sessionId);

      return;
    }

    // 如果没有收到流式数据，说明是非流式模式
    // 需要移除占位消息（如果存在）
    if (!hasSessionReceivedStreamData(sessionId)) {
      const placeholderId = getStreamingPlaceholderId(sessionId);
      const placeholderIndex = placeholderId
        ? messages.value.findIndex(m => m.id === placeholderId)
        : -1;
      if (placeholderIndex !== -1) {
        messages.value.splice(placeholderIndex, 1);
      }
    }

    // 重置所有消息的 isStreaming 标志，同时合并完整消息数据（包括 usage）
    console.log('[TOKEN] stream_end message:', message);
    messages.value = finalizeSessionTokenUsage(sessionId, message, shouldSkipUsageRefresh);

    // 清空当前流式消息引用
    clearSessionStreamingUiState(sessionId);

    // 播放完成提示音
    playCompletionSound();

  });

  // 监听流式错误
  unlistenStreamError = await listen('claude:stream_error', (event: { payload: string }) => {
    console.error('[STREAM ERROR] ========== Stream error ==========', event.payload);

    if (currentSessionId.value) {
      claudeStore.setSessionStatus(currentSessionId.value, 'idle');
      claudeStore.setStreaming(currentSessionId.value, null);
      claudeStore.setStreamingStats(currentSessionId.value, null);
      clearSessionStreamingUiState(currentSessionId.value);
    }

    // 防御性修复：重置所有消息的 isStreaming
    messages.value = messages.value.map(msg => ({
      ...msg,
      isStreaming: false
    }));

    // isStreaming 现在是从 claudeStore.streaming 获取，会自动更新
    console.error('[STREAM ERROR] ========== Reset all streaming states ==========');
  });

  // 监听单个消息事件（用于多消息显示）
  unlistenMessage = await listen<any>('claude:message', (event) => {

    const msg = event.payload;
    console.log('[MESSAGE] Received message:', msg);

    // 检查 session_id，只处理当前 session 的消息
    const msgSessionId = msg.session_id || msg.sessionId;
    if (msgSessionId && msgSessionId !== currentSessionId.value) {
      console.log('[MESSAGE] Routing message to background session:', msgSessionId, 'current:', currentSessionId.value);
      try {
        applyIncomingMessageToBackgroundSession(msgSessionId, msg);
      } catch (error) {
        console.error('[MESSAGE] Failed to process background session message:', error, msg);
      }
      return;
    }

    try {
      // 后端推送的消息格式支持：
      // {
      //   type: 'assistant',
      //   message: {
      //     id: 'msg_xxx',
      //     role: 'assistant',
      //     model: 'qwen3.5-plus',
      //     content: [{ type: 'thinking', thinking: '...' }, { type: 'text', text: '...' }],
      //     usage: { input_tokens, output_tokens }
      //   },
      //   parent_tool_use_id: null,
      //   session_id: 'xxx',
      //   uuid: 'xxx'
      // }

      // 提取实际的消息内容（支持嵌套格式和扁平格式）
      const messageData = msg.message || msg;

      // 提取 content blocks（支持多种字段名与序列化格式）
      let contentBlocks: any[] = [];
      const rawContent = messageData.content;

      // 尝试多种方式提取 content blocks
      if (Array.isArray(rawContent)) {
        contentBlocks = rawContent;
      } else if (Array.isArray(msg.contentBlocks)) {
        contentBlocks = msg.contentBlocks;
      } else if (rawContent && typeof rawContent === 'object' && Array.isArray((rawContent as any).content)) {
        // 支持 { content: [{ type, text }] } 格式
        contentBlocks = (rawContent as any).content;
      } else if (rawContent && typeof rawContent === 'object' && Array.isArray((rawContent as any).Blocks)) {
        contentBlocks = (rawContent as any).Blocks;
      } else if (rawContent && typeof rawContent === 'object' && Array.isArray((rawContent as any).blocks)) {
        contentBlocks = (rawContent as any).blocks;
      }

      // 提取纯文本内容
      let contentText = '';
      if (typeof rawContent === 'string') {
        contentText = rawContent;
      } else if (contentBlocks.length > 0) {
        // 过滤出 text 类型的块
        const textBlocks = contentBlocks.filter((block: any) => block.type === 'text');

        contentText = textBlocks
          .map((block: any) => {
            // 后端 ContentBlock 使用 content 字段存储文本内容
            // 兼容处理：优先使用 content 字段，其次使用 text 字段
            const text = block.content ?? block.text ?? '';
            return text;
          })
          .join('\n');

        // user 消息可能是 tool_result（命令输出），无 type=text 时用 tool_result 内容作为显示
        if (!contentText && contentBlocks.some((b: any) => b.type === 'tool_result')) {
          const tr = contentBlocks.find((b: any) => b.type === 'tool_result');
          contentText = (tr && (tr.content ?? tr.text)) ?? '';
        }
      }

      const normalizedRole = (messageData.role ?? msg.role ?? 'assistant').toString().toLowerCase();
      const isUser = normalizedRole === 'user';
      const isSystem = normalizedRole === 'system';

      // user 且仅含 tool_result：将结果挂到上一条 assistant 中对应 tool_use_id 的 tool_use 上，不单独成条
      if (isUser && contentBlocks.length > 0 && contentBlocks.every((b: any) => b.type === 'tool_result')) {
        const tr = contentBlocks.find((b: any) => b.type === 'tool_result');
        if (tr) {
          // 解析 tool_result 内容（可能是 JSON 字符串或对象）
          let trData = tr;
          if (typeof tr.content === 'string') {
            try {
              trData = JSON.parse(tr.content);
            } catch (e) {
              console.error('[MESSAGE] Failed to parse tool_result content:', e);
            }
          }

          const toolUseId = trData.tool_use_id ?? trData.toolUseId;
          const resultContent = typeof trData.content === 'string' ? trData.content : (trData.text ?? '');
          const isError = trData.is_error ?? trData.isError ?? false;

          if (toolUseId && resultContent !== undefined) {
            const getToolUseId = (block: any): string | null => {
              if (block.type !== 'tool_use') return null;
              if (block.id) return block.id;
              try {
                const parsed = typeof block.content === 'string' ? JSON.parse(block.content) : block.content;
                return parsed?.id ?? null;
              } catch {
                return null;
              }
            };
            for (let i = messages.value.length - 1; i >= 0; i--) {
              const m = messages.value[i];
              if (m.role !== 'assistant' || !m.contentBlocks) continue;
              const hasMatch = m.contentBlocks.some((b: any) => getToolUseId(b) === toolUseId);
              if (hasMatch) {
                const next = {
                  ...m,
                  toolResults: { ...(m.toolResults || {}), [toolUseId]: resultContent },
                  toolResultErrors: { ...(m.toolResultErrors || {}), [toolUseId]: isError }
                };
                messages.value = messages.value.slice(0, i).concat(next, messages.value.slice(i + 1));
                return;
              }
            }
          }
        }
      }

      // 每条消息必须用唯一 id
      const uniqueId = `${messageData.id ?? msg.id ?? 'msg'}_${Date.now()}_${Math.random().toString(36).slice(2)}`;

      // 如果存在流式消息（由 stream_start 创建的占位消息），只更新 assistant 类型的流式回复
      // 其他消息（如 tool_use、tool_result）应该正常追加到列表中
      // 检查是否是 assistant 消息且不含 tool_use（即真正的流式文本回复）
      const isStreamingAssistant = !isUser && !isSystem && contentBlocks.length > 0 &&
        !contentBlocks.some((b: any) => b.type === 'tool_use' || b.type === 'tool_result');
      const activeSessionId = msgSessionId || currentSessionId.value;
      const currentPlaceholderId = activeSessionId ? getStreamingPlaceholderId(activeSessionId) : null;
      const currentSessionHasStreamingData = activeSessionId
        ? hasSessionReceivedStreamData(activeSessionId)
        : false;

      // 只有在真正收到流式数据时，才更新占位消息
      if (currentPlaceholderId && isStreamingAssistant && currentSessionHasStreamingData) {
        // 更新现有流式消息的内容
        const msgIndex = messages.value.findIndex(m => m.id === currentPlaceholderId);
        if (msgIndex !== -1) {
          // 保留原来的 timestamp（不覆盖）
          const existingMsg = messages.value[msgIndex];
          messages.value[msgIndex] = {
            ...existingMsg,
            content: existingMsg.content || contentText || '',
            contentBlocks: existingMsg.contentBlocks,
            model: messageData.model || existingMsg.model,
            tokenUsage: messageData.usage
              ? normalizeTokenUsage(messageData.usage)
              : existingMsg.tokenUsage,
          };
          console.log('[TOKEN] Updated streaming message tokenUsage:', messages.value[msgIndex].tokenUsage);
          if (activeSessionId) {
            setStreamingPlaceholderId(activeSessionId, messages.value[msgIndex].id);
          }
        }
        return;
      }

      const convertedMsg: Message = {
        id: uniqueId,
        role: isSystem ? 'system' : (isUser ? 'user' : 'assistant'),
        content: contentText || '',
        timestamp: normalizeMessageTimestamp(msg.timestamp ?? messageData.createdAt),
        checkpointUuid: msg.checkpointUuid ?? messageData.checkpointUuid,
        contentBlocks: contentBlocks.length > 0 ? contentBlocks : undefined,
        parentToolUseId: msg.parent_tool_use_id ?? msg.parentToolUseId,
        model: messageData.model,
        tokenUsage: messageData.usage ? normalizeTokenUsage(messageData.usage) : undefined,
      };
      console.log('[TOKEN] Created message tokenUsage:', convertedMsg.tokenUsage, 'messageData.usage:', messageData.usage);

      // 调试日志：输出 tool_use 消息的 contentBlocks
      if (contentBlocks.some((b: any) => b.type === 'tool_use')) {
        console.log('[🔍 TOOL_USE] Creating tool_use message:', {
          id: uniqueId,
          contentBlocksCount: contentBlocks.length,
          contentBlocks: contentBlocks.map((b: any) => ({
            type: b.type,
            contentType: typeof b.content,
            contentPreview: typeof b.content === 'string' ? b.content.substring(0, 100) : b.content
          }))
        });
      }

      // 在真正收到流式文本之前，assistant 结构化块都应该插入到占位消息之前，
      // 这样可以保持 thinking/tool_use 按实际到达顺序排列，避免后续的 thinking
      // 因为“插前面”而跑到已追加的 tool_use 前面。
      const shouldInsertBeforePlaceholder = !!currentPlaceholderId
        && !isUser
        && !isSystem
        && !currentSessionHasStreamingData
        && contentBlocks.length > 0;

      if (currentPlaceholderId && !isUser && !isSystem && contentBlocks.length > 0 && currentSessionHasStreamingData) {
        const placeholderIndex = messages.value.findIndex(m => m.id === currentPlaceholderId);
        if (placeholderIndex !== -1) {
          const closedPlaceholder: Message = {
            ...messages.value[placeholderIndex],
            isStreaming: false,
          };
          const nextPlaceholder = createStreamingPlaceholderMessage(activeSessionId || currentSessionId.value || 'unknown', '');

          messages.value.splice(
            placeholderIndex,
            1,
            closedPlaceholder,
            convertedMsg,
            nextPlaceholder,
          );

          if (activeSessionId) {
            setStreamingPlaceholderId(activeSessionId, nextPlaceholder.id);
            setSessionReceivedStreamData(activeSessionId, false);
          }
          return;
        }
      }

      if (shouldInsertBeforePlaceholder) {
        const placeholderIndex = messages.value.findIndex(m => m.id === currentPlaceholderId);
        if (placeholderIndex !== -1) {
          messages.value.splice(placeholderIndex, 0, convertedMsg);
        } else {
          messages.value.push(convertedMsg);
        }
      } else {
        // 其他情况正常追加
        messages.value.push(convertedMsg);
      }

    // 输入框 token 圆环只在 stream_end 时更新，避免中间消息用到不完整的 usage/modelUsage。
  } catch (error) {
      console.error('[MESSAGE] Failed to process incoming message:', error, msg);
      appendMessageParseError(msg, error);
    }

  });

  unlistenSubagentToolUse = await listen<SubagentToolUseEventPayload>(
    'claude:subagent_tool_use',
    (event) => {
      const data = event.payload;
      claudeStore.upsertSubagentToolUse(data.sessionId, data);
    },
  );

  unlistenSubagentToolInputDelta = await listen<SubagentToolInputDeltaEventPayload>(
    'claude:subagent_tool_input_delta',
    (event) => {
      const data = event.payload;
      claudeStore.appendSubagentToolInputDelta(data.sessionId, data);
    },
  );

  unlistenSubagentToolResultStart = await listen<SubagentToolResultStartEventPayload>(
    'claude:subagent_tool_result_start',
    (event) => {
      const data = event.payload;
      claudeStore.startSubagentToolResult(data.sessionId, data);
    },
  );

  unlistenSubagentToolResultDelta = await listen<SubagentToolResultDeltaEventPayload>(
    'claude:subagent_tool_result_delta',
    (event) => {
      const data = event.payload;
      claudeStore.appendSubagentToolResultDelta(data.sessionId, data);
    },
  );

  unlistenSubagentToolResultComplete = await listen<SubagentToolResultCompleteEventPayload>(
    'claude:subagent_tool_result_complete',
    (event) => {
      const data = event.payload;
      claudeStore.completeSubagentToolResult(data.sessionId, data);
    },
  );

  unlistenUserCheckpoint = await listen<{
    sessionId: string;
    checkpointUuid?: string | null;
  }>('claude:user_checkpoint', (event) => {
    const payload = event.payload;
    if (!payload.checkpointUuid) return;
    if (payload.sessionId !== currentSessionId.value) return;

    for (let i = messages.value.length - 1; i >= 0; i -= 1) {
      if (messages.value[i].role !== 'user') continue;
      messages.value = messages.value.slice(0, i).concat({
        ...messages.value[i],
        checkpointUuid: payload.checkpointUuid,
      }, messages.value.slice(i + 1));
      break;
    }
  });

  // 监听权限请求事件
  unlistenPermission = await listen<{
    sessionId: string;
    requestId: string;
    permission: any; // 使用 any 因为后端发送的格式可能与 PermissionRequest 类型不完全匹配
  }>('permission-request', (event) => {
    const data = event.payload;
    console.log('📋 [PERMISSION] Received permission request from backend');
    console.log('📋 [PERMISSION] Full payload:', JSON.stringify(data, null, 2));

    // 调试：输出 currentSessionId 的值和类型
    console.log('🔍 [PERMISSION] currentSessionId.value:', currentSessionId.value);
    console.log('🔍 [PERMISSION] data.sessionId:', data.sessionId);
    console.log('🔍 [PERMISSION] sessionId match:', data.sessionId === currentSessionId.value);
    console.log('🔍 [PERMISSION] data.sessionId type:', typeof data.sessionId);
    console.log('🔍 [PERMISSION] currentSessionId.value type:', typeof currentSessionId.value);

    console.log('[PERMISSION] Adding permission to store for session:', data.sessionId);
    // 优先使用顶层的 tool_use_id，其次使用 params 中的
    const permissionWithToolUseId = {
      ...data.permission,
      session_id: data.sessionId,
      tool_use_id: data.permission.tool_use_id || data.permission.params?.tool_use_id,
    };
    console.log('[PERMISSION] Permission with tool_use_id:', {
      requestId: permissionWithToolUseId.request_id,
      toolUseId: permissionWithToolUseId.tool_use_id,
      originalTopLevel: data.permission.tool_use_id,
      originalParams: data.permission.params?.tool_use_id,
      isActiveSession: data.sessionId === currentSessionId.value,
    });
    claudeStore.addPermission(data.sessionId, permissionWithToolUseId);
  });

  // 启动自动刷新定时器（已禁用，改为手动刷新）

  // 监听页面可见性变化
  const handleVisibilityChange = () => {
    isPageVisible.value = !document.hidden;
  };

  document.addEventListener('visibilitychange', handleVisibilityChange);
  (window as any).__visibilityChangeHandler = handleVisibilityChange;
});

// 清理事件监听
onUnmounted(() => {
  unlistenConnectionStatus?.();
  unlistenSessionStatus?.();
  unlistenStreamStart?.();
  unlistenStreamData?.();
  unlistenStreamUsage?.();
  unlistenStreamEnd?.();
  unlistenStreamError?.();
  unlistenMessage?.();
  unlistenSubagentToolUse?.();
  unlistenSubagentToolInputDelta?.();
  unlistenSubagentToolResultStart?.();
  unlistenSubagentToolResultDelta?.();
  unlistenSubagentToolResultComplete?.();
  unlistenUserCheckpoint?.();
  unlistenPermission?.();
  unlistenSessionCreated?.();
  unlistenSessionIdUpdated?.();
  unlistenCommandsUpdated?.();

  // 清理连接状态刷新定时器
  if (connectionRefreshTimer !== null) {
    clearTimeout(connectionRefreshTimer);
    connectionRefreshTimer = null;
  }

  clearSessionPathCopiedState();
  stopSessionFileWatcher(true);
  stopResize();
  stopWorkspaceResize();
  window.removeEventListener('keydown', handleWindowSearchShortcut);
  window.removeEventListener('resize', syncWorkspacePanelWidth);
  document.removeEventListener('mousedown', handleProjectSortOutsideClick);

  // 清理页面可见性监听器
  const handler = (window as any).__visibilityChangeHandler;
  if (handler) {
    document.removeEventListener('visibilitychange', handler);
    delete (window as any).__visibilityChangeHandler;
  }

  // 停止自动刷新定时器
  stopAutoRefresh();
});

// 发送消息
// 注意：文件引用已包含在 content 中，格式为 @path/to/file
const sendMessage = async (payload: string | OutgoingMessagePayload) => {
  const normalizedPayload: OutgoingMessagePayload = typeof payload === 'string'
    ? { content: payload }
    : payload;
  const content = normalizedPayload.content;
  const contentBlocks = normalizedPayload.contentBlocks;
  const transportContent = normalizedPayload.transportContent ?? content;
  const transportContentBlocks = normalizedPayload.transportContentBlocks ?? contentBlocks;

  if (!transportContent.trim() && (!transportContentBlocks || transportContentBlocks.length === 0)) return;

  // 获取当前项目路径
  const conv = selectedConversation.value;
  const currentProject = conv
    ? projects.value.find(p => p.id === conv.projectId)
    : projects.value.find(p => expandedProjects.value.has(p.id));

  if (!currentProject) {
    messages.value.push({
      id: Date.now().toString(),
      role: 'assistant',
      content: '请先选择一个项目',
      timestamp: Date.now()
    });
    return;
  }

  const ensureSessionForSend = async () => {
    if (currentSessionId.value && currentConversationConnectionState.value !== 'disconnected') {
      return currentSessionId.value;
    }

    const activeConversation = selectedConversation.value;
    const isDraftConversation = !!activeConversation
      && activeConversation.id.startsWith('temp-')
      && !hasConversationInList(activeConversation.id);
    const providerState = resolveConversationProvider(activeConversation);
    const requestedSessionId = activeConversation && !activeConversation.id.startsWith('temp-')
      ? activeConversation.id
      : undefined;

    try {
      const session = await invoke<Session>('create_session', {
        sessionId: requestedSessionId,
        projectPath: currentProject.path,
        thinkingLevel: activeConversation?.thinkingLevel || claudeStore.thinkingLevel,
        providerId: providerState.providerId,
        model: providerState.model,
        providerEnv: providerState.providerEnv,
      });
      const sessionId = session.id;

      currentSessionId.value = sessionId;
      workingDirectorySet.value = true;
      claudeStore.setCurrentSession(sessionId);
      claudeStore.setConnectionStatus(sessionId, 'connected');

      const existingSession = claudeStore.sessions.get(sessionId);
      if (existingSession) {
        claudeStore.updateSession(sessionId, {
          cwd: currentProject.path,
          permissionMode: getConversationPermissionMode(activeConversation),
          thinkingLevel: activeConversation?.thinkingLevel || claudeStore.thinkingLevel,
          providerId: activeConversation?.providerId || providerState.providerId,
          model: providerState.model,
          providerOverrideEnabled: !!activeConversation?.providerOverrideEnabled,
          updatedAt: Date.now(),
        });
      } else if (isDraftConversation) {
        claudeStore.createPendingSession(sessionId, currentProject.path, {
          permissionMode: getConversationPermissionMode(activeConversation),
          thinkingLevel: activeConversation?.thinkingLevel || claudeStore.thinkingLevel,
          providerId: activeConversation?.providerId || providerState.providerId,
          model: providerState.model,
          providerOverrideEnabled: !!activeConversation?.providerOverrideEnabled,
        });
      } else {
        claudeStore.addSession({
          sessionId,
          cwd: currentProject.path,
          permissionMode: getConversationPermissionMode(activeConversation),
          thinkingLevel: activeConversation?.thinkingLevel || claudeStore.thinkingLevel,
          providerId: activeConversation?.providerId || providerState.providerId,
          model: providerState.model,
          providerOverrideEnabled: !!activeConversation?.providerOverrideEnabled,
          createdAt: Date.now(),
          updatedAt: Date.now(),
        });
      }

      const permissionMode = getConversationPermissionMode(activeConversation);
      if (permissionMode !== 'default') {
        // 临时注释：创建 session 后先不自动下发 set_permission_mode
        // await invoke('set_permission_mode', { sessionId, mode: permissionMode });
        console.log('[PERMISSION] Auto sync disabled after session creation:', {
          sessionId,
          mode: permissionMode,
        });
      }

      if (activeConversation) {
        const originalConversationId = activeConversation.id;
        const nextConversation: Conversation = {
          ...activeConversation,
          id: sessionId,
          providerId: activeConversation.providerId || providerState.providerId,
          model: activeConversation.model || providerState.model,
        };

        const sessionMap = new Map(conversationSessionMap.value);
        sessionMap.set(originalConversationId, sessionId);
        conversationSessionMap.value = sessionMap;

        if (isDraftConversation) {
          const draftConversation: Conversation = {
            ...activeConversation,
            providerId: activeConversation.providerId || providerState.providerId,
            model: activeConversation.model || providerState.model,
          };
          selectedConversation.value = draftConversation;
          setProjectDraftConversation(draftConversation.projectId, draftConversation);
        } else {
          selectedConversation.value = nextConversation;
          upsertConversationInList(nextConversation);
        }
      }

      // 刷新活动 Session 列表
      refreshActiveSessions();

      console.log('Created or resumed session before sending:', sessionId);
      return sessionId;
    } catch (e) {
      console.error('Failed to create or resume session:', e);
      messages.value.push({
        id: Date.now().toString(),
        role: 'assistant',
        content: `无法创建会话: ${e}`,
        timestamp: Date.now()
      });
      return null;
    }
  };

  const ensuredSessionId = await ensureSessionForSend();
  if (!ensuredSessionId) return;

  const permissionModeSynced = await syncSessionPermissionModeBeforeFirstMessage(ensuredSessionId);
  if (!permissionModeSynced) return;

  const sendSessionId = await resolveLatestSessionIdForSend(ensuredSessionId);
  console.log('[SEND] Ready to send user message with session ID:', {
    ensuredSessionId,
    sendSessionId,
    currentSessionId: currentSessionId.value,
    selectedConversationId: selectedConversation.value?.id || null,
  });

  finalizeInterruptedStreamingState(sendSessionId);

  const attachments = normalizedPayload.attachments;

  // 添加用户消息
  const userMessage: Message = {
    id: Date.now().toString(),
    role: 'user',
    content: content,
    contentBlocks,
    timestamp: Date.now(),
    attachments,
  };
  console.log('[SEND] Adding user message:', userMessage);
  messages.value.push(userMessage);
  setSessionMessagesSnapshot(sendSessionId, messages.value);
  persistSelectedConversationAfterFirstSend();

  claudeStore.setSessionStatus(sendSessionId, 'running');
  claudeStore.setStreaming(sendSessionId, '');
  claudeStore.setStreamingStats(sendSessionId, {
    startedAt: Date.now(),
    outputTokens: 0,
  });

  try {
    // 调用 Rust 后端发送消息（使用支持多消息的 API）
    await invoke('send_message_with_multi_stream', {
      sessionId: sendSessionId,
      content: transportContent,
      contentBlocks: transportContentBlocks,
    });

  } catch (error) {
    console.error('[SEND ERROR] ========== 发送消息失败 ==========', error);
    claudeStore.setSessionStatus(sendSessionId, 'idle');
    claudeStore.setStreaming(sendSessionId, null);
    claudeStore.setStreamingStats(sendSessionId, null);
    // 防御性修复：重置所有消息的 isStreaming
    messages.value = messages.value.map(msg => ({
      ...msg,
      isStreaming: false
    }));
    // isStreaming 现在是从 claudeStore.streaming 获取，会自动更新
    console.error('[SEND ERROR] ========== Reset all streaming states ==========');
  }
};

// 停止流式响应
const stopStreaming = async () => {
  console.log('[STOP] ========== Stop streaming called ==========');
  const sessionId = currentSessionId.value;
  if (sessionId) {
    markSessionStoppedManually(sessionId);
  }

  try {
    if (sessionId) {
      await invoke('stop_streaming', {
        sessionId
      });
    }
  } catch (error) {
    if (sessionId) {
      clearSessionManualStop(sessionId);
    }
    console.error('[STOP] 停止流式响应失败:', error);
  }

  // 清除流式状态
  if (sessionId) {
    claudeStore.setSessionStatus(sessionId, 'idle');
    claudeStore.setStreaming(sessionId, null);
    claudeStore.setStreamingStats(sessionId, null);
    for (const key of getSessionPermissionKeys(sessionId)) {
      claudeStore.clearSessionPermissions(key);
    }
    finalizeInterruptedStreamingState(sessionId);
  } else {
    messages.value = messages.value.map(msg => ({
      ...msg,
      isStreaming: false,
    }));
  }

  console.log('[STOP] ========== Stop streaming complete ==========');
};

const setRewindDraft = (text: string) => {
  rewindDraftText.value = text;
  rewindDraftVersion.value += 1;
};

const appendSystemMessage = (content: string) => {
  messages.value.push({
    id: `rewind_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`,
    role: 'system',
    content,
    timestamp: Date.now(),
  });
};

const closeCurrentSessionForRewind = async () => {
  const sessionId = currentSessionId.value;
  if (!sessionId) return;

  try {
    await invoke('close_session', { sessionId });
  } catch (error) {
    console.warn('[REWIND] 关闭当前会话失败，继续回退流程:', error);
  }

  claudeStore.setConnectionStatus(sessionId, 'disconnected');
  claudeStore.setCliConnected(sessionId, false);
  claudeStore.setSessionStatus(sessionId, 'idle');
  claudeStore.setStreaming(sessionId, null);
  claudeStore.setStreamingStats(sessionId, null);
  clearSessionStreamingUiState(sessionId);

  await refreshActiveSessions();
};

const restoreFilesFromCheckpoint = async (turn: RewindTurn): Promise<boolean> => {
  const sessionId = currentSessionId.value || selectedConversation.value?.id;
  const cwd = currentProject.value?.path;
  if (!sessionId || !cwd || !turn.checkpointUuid) return false;

  try {
    await invoke('rewind_files_to_checkpoint', {
      sessionId,
      checkpointUuid: turn.checkpointUuid,
      cwd,
    });
    return true;
  } catch (error) {
    console.error('[REWIND] 代码回退失败:', error);
    return false;
  }
};

const executeRewind = async ({ turn, action }: { turn: RewindTurn; action: RewindAction }) => {
  if (rewindBusy.value) return;

  rewindBusy.value = true;

  try {
    if (isStreaming.value) {
      await stopStreaming();
    }

    const originalText = messages.value[turn.startMsgIdx]?.content || '';
    const summaryText = buildRewindSummary(messages.value, turn.startMsgIdx);
    const needsFileRestore = action === 'restore_all' || action === 'restore_code';
    const fileRestoreOk = needsFileRestore ? await restoreFilesFromCheckpoint(turn) : false;

    if (action === 'restore_all' || action === 'restore_conversation' || action === 'summarize') {
      messages.value = messages.value.slice(0, turn.startMsgIdx);
    }

    await closeCurrentSessionForRewind();

    if (action !== 'summarize') {
      setRewindDraft(originalText);
    }

    if (action === 'restore_all') {
      appendSystemMessage(
        fileRestoreOk
          ? `已回退到第 ${turn.index} 轮，代码和对话已恢复。`
          : `已回退到第 ${turn.index} 轮，对话已恢复，代码恢复不可用。`,
      );
      return;
    }

    if (action === 'restore_conversation') {
      appendSystemMessage(`已回退到第 ${turn.index} 轮，仅恢复对话内容。`);
      return;
    }

    if (action === 'restore_code') {
      appendSystemMessage(
        fileRestoreOk
          ? `已恢复第 ${turn.index} 轮对应的代码状态。`
          : '代码恢复失败，当前仅保留了输入框中的原始提问。',
      );
      return;
    }

    appendSystemMessage(
      summaryText
        ? `从第 ${turn.index} 轮开始的后续内容已总结：\n${summaryText}`
        : `从第 ${turn.index} 轮开始的后续内容已清理。`,
    );
  } finally {
    rewindBusy.value = false;
  }
};

const executeBuiltinCommand = async (commandName: string) => {
  const normalized = commandName.replace(/^\//, '').trim();

  if (normalized === 'clear') {
    if (!currentProject.value) {
      messages.value.push({
        id: Date.now().toString(),
        role: 'assistant',
        content: '请先选择一个项目',
        timestamp: Date.now(),
      });
      return;
    }

    await startNewConversation(currentProject.value.id);
    return;
  }

  if (['compact', 'init', 'rewind'].includes(normalized)) {
    if (isStreaming.value) {
      await stopStreaming();
    }

    await sendMessage(`/${normalized}`);
  }
};

const updateConversationThinkingLevel = (sessionId: string, thinkingLevel: ThinkingLevel) => {
  conversations.value = conversations.value.map((conversation) => {
    if (conversation.id !== sessionId) return conversation;
    return {
      ...conversation,
      thinkingLevel,
    };
  });

  if (selectedConversation.value?.id === sessionId) {
    selectedConversation.value = {
      ...selectedConversation.value,
      thinkingLevel,
    };
  }
};

const matchesConversationSession = (conversation: Conversation, sessionId: string) => {
  return conversation.id === sessionId || conversationSessionMap.value.get(conversation.id) === sessionId;
};

const updateConversationPermissionMode = (sessionId: string, permissionMode: PermissionMode) => {
  conversations.value = conversations.value.map((conversation) => {
    if (!matchesConversationSession(conversation, sessionId)) return conversation;
    return {
      ...conversation,
      permissionMode,
    };
  });

  if (selectedConversation.value && matchesConversationSession(selectedConversation.value, sessionId)) {
    selectedConversation.value = {
      ...selectedConversation.value,
      permissionMode,
    };
  }

  for (const [projectId, conversation] of projectDraftConversations.value.entries()) {
    if (!matchesConversationSession(conversation, sessionId)) continue;

    setProjectDraftConversation(projectId, {
      ...conversation,
      permissionMode,
    });
    break;
  }
};

watch(
  () => [claudeStore.currentSession?.sessionId, claudeStore.currentSession?.thinkingLevel] as const,
  ([sessionId, thinkingLevel]) => {
    if (!sessionId || !thinkingLevel) return;
    updateConversationThinkingLevel(sessionId, thinkingLevel);
  },
  { immediate: true },
);

watch(
  () => [claudeStore.currentSession?.sessionId, claudeStore.currentSession?.permissionMode] as const,
  ([sessionId, permissionMode]) => {
    if (!sessionId || !permissionMode) return;
    updateConversationPermissionMode(sessionId, permissionMode);
  },
  { immediate: true },
);

const updateConversationSessionConfig = (payload: {
  sessionId: string;
  providerId: string | null;
  model: string | null;
  providerOverrideEnabled: boolean;
}) => {
  conversations.value = conversations.value.map((conversation) => {
    if (conversation.id !== payload.sessionId) return conversation;
    return {
      ...conversation,
      providerId: payload.providerId,
      model: payload.model,
      providerOverrideEnabled: payload.providerOverrideEnabled,
    };
  });

  claudeStore.updateSession(payload.sessionId, {
    providerId: payload.providerId,
    model: payload.model,
    providerOverrideEnabled: payload.providerOverrideEnabled,
  });

  if (selectedConversation.value?.id === payload.sessionId) {
    selectedConversation.value = {
      ...selectedConversation.value,
      providerId: payload.providerId,
      model: payload.model,
      providerOverrideEnabled: payload.providerOverrideEnabled,
    };
  }

  for (const [projectId, conversation] of projectDraftConversations.value.entries()) {
    if (conversation.id !== payload.sessionId) continue;

    setProjectDraftConversation(projectId, {
      ...conversation,
      providerId: payload.providerId,
      model: payload.model,
      providerOverrideEnabled: payload.providerOverrideEnabled,
    });
    break;
  }
};

const copyAssistantMessage = async (content: string) => {
  try {
    await navigator.clipboard.writeText(content);
  } catch (error) {
    console.error('[COPY] 复制 AI 回复失败:', error);
  }
};

const regenerateAssistantMessage = async (messageId: string) => {
  const assistantIndex = messages.value.findIndex(msg => msg.id === messageId && msg.role === 'assistant');
  if (assistantIndex === -1) return;

  const previousUserMessage = [...messages.value]
    .slice(0, assistantIndex)
    .reverse()
    .find(msg => msg.role === 'user');

  if (!previousUserMessage) {
    console.warn('[REGENERATE] 未找到可复用的上一条用户消息');
    return;
  }

  await sendMessage({
    content: previousUserMessage.content,
    contentBlocks: previousUserMessage.contentBlocks,
    attachments: previousUserMessage.attachments,
  });
};

// 删除项目
const deleteProject = (projectId: number, event: Event) => {
  event.stopPropagation();

  // 检查是否有选中的对话属于该项目
  if (selectedConversation.value && selectedConversation.value.projectId === projectId) {
    selectedConversation.value = null;
    messages.value = [];
  }

  // 从项目列表中移除
  const index = projects.value.findIndex(p => p.id === projectId);
  if (index !== -1) {
    projects.value.splice(index, 1);
  }

  // 从对话列表中移除该项目的所有对话
  conversations.value = conversations.value.filter(c => c.projectId !== projectId);

  // 从展开状态中移除
  expandedProjects.value.delete(projectId);
  clearProjectDraftConversation(projectId);

  console.log(`项目 ${projectId} 已删除`);
};

// 切换对话的固定状态
const toggleConversationPin = async (conv: Conversation, _projectId: number, event: Event) => {
  event.stopPropagation();
  const index = conversations.value.findIndex(c => c.id === conv.id);
  if (index !== -1) {
    conversations.value[index].pinned = !conversations.value[index].pinned;
    await saveData();
  }
};

// 删除对话记录（同时删除磁盘文件）
const deleteConversation = async (conv: Conversation, projectId: number, event: Event) => {
  event.stopPropagation();

  const project = projects.value.find(p => p.id === projectId);
  if (!project) {
    console.error('Project not found for conversation:', conv.id);
    return;
  }

  const actualSessionId = getActualSessionIdForConversation(conv.id);
  const previousSessions = [...activeSessions.value];

  try {
    if (actualSessionId) {
      try {
        await invoke('close_session', { sessionId: actualSessionId });
        await activateFallbackSession(actualSessionId, previousSessions);
        claudeStore.removeSession(actualSessionId);
      } catch (closeError) {
        console.error('关闭会话失败:', closeError);
      }
    }

    try {
      await invoke('delete_session_file', {
        projectPath: project.path,
        sessionId: conv.id
      });
      console.log(`会话文件已删除: ${conv.id}`);
    } catch (fileError: any) {
      if (fileError?.includes?.('not found') || fileError?.includes?.('not found')) {
        console.log(`会话文件不存在（可能是新创建的对话），继续删除: ${conv.id}`);
      } else {
        throw fileError;
      }
    }

    removeConversationSessionMapping(conv.id);

    const index = conversations.value.findIndex(c => c.id === conv.id);
    if (index !== -1) {
      conversations.value.splice(index, 1);
    }

    if (selectedConversation.value?.id === conv.id && activeSessions.value.length === 0) {
      resetChatSelection();
    }

    claudeStore.clearUnreadTaskCompletion(conv.id);
    claudeStore.clearUnreadMessageCompletion(conv.id);

    await saveData();

    console.log(`对话 ${conv.id} 已从列表中移除`);
  } catch (error) {
    console.error('删除会话文件失败:', error);
  }
};
</script>

<template>
  <div :class="['projects-view', { dark: claudeStore.darkMode }]">
    <Settings v-if="isSettingsVisible" @close="isSettingsVisible = false" />

    <template v-else>
    <!-- 左侧：项目列表（可拖拽调整宽度） -->
    <div class="left-panel" :style="{ width: leftPanelWidth + 'px', minWidth: minLeftWidth + 'px' }">
      <div class="section-header">
        <div class="button-row">
          <button class="action-button btn-open" @click="openFolder">
            <HugeiconsIcon :icon="FolderIcon" class="folder-icon-small" />
            <span>打开文件夹</span>
          </button>
          <button class="action-button btn-import" @click="openImportDialog">
            <HugeiconsIcon :icon="FileImportIcon" class="folder-icon-small" />
            <span>导入 Claude Code 历史</span>
          </button>
        </div>
      </div>
      <div class="project-toolbar">
        <div class="project-toolbar-title">项目</div>
        <label class="project-search-input-wrap toolbar-search">
          <HugeiconsIcon :icon="Search01Icon" class="project-search-icon" />
          <input
            v-model="projectSearchQuery"
            type="text"
            class="project-search-input"
            placeholder="搜索项目"
          />
        </label>
        <div class="project-sort-wrap">
          <button
            ref="projectSortButtonRef"
            type="button"
            class="project-sort-btn"
            :title="`排序：${currentProjectSortLabel}`"
            @click="toggleProjectSortMenu"
          >
            <svg viewBox="0 0 20 20" fill="none" class="project-sort-btn-icon" aria-hidden="true">
              <path d="M4 5H16" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/>
              <path d="M7 10H13" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/>
              <path d="M9 15H11" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/>
            </svg>
          </button>

          <div
            v-if="isProjectSortMenuOpen"
            ref="projectSortMenuRef"
            class="project-sort-menu"
          >
            <div class="project-sort-menu-title">整理</div>
            <button
              v-for="option in projectSortOptions"
              :key="option.value"
              type="button"
              class="project-sort-option"
              :class="{ active: projectSortMode === option.value }"
              @click="selectProjectSortMode(option.value)"
            >
              <span class="project-sort-option-icon" aria-hidden="true">
                <svg v-if="option.value === 'project'" viewBox="0 0 20 20" fill="none">
                  <path d="M2.5 6.5A2 2 0 0 1 4.5 4.5H8l1.5 2H15.5a2 2 0 0 1 2 2v5A2 2 0 0 1 15.5 15.5h-11a2 2 0 0 1-2-2v-7Z" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round"/>
                </svg>
                <svg v-else-if="option.value === 'time'" viewBox="0 0 20 20" fill="none">
                  <circle cx="10" cy="10" r="7" stroke="currentColor" stroke-width="1.6"/>
                  <path d="M10 6.5V10.2L12.6 12.2" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/>
                </svg>
                <svg v-else viewBox="0 0 20 20" fill="none">
                  <path d="M6.2 6.6A4.2 4.2 0 0 1 13.4 5l1.1-1.1V7h-3.1l1.1-1.1A2.8 2.8 0 0 0 7.7 7" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/>
                  <path d="M13.8 13.4A4.2 4.2 0 0 1 6.6 15l-1.1 1.1V13h3.1l-1.1 1.1A2.8 2.8 0 0 0 12.3 13" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/>
                </svg>
              </span>
              <span class="project-sort-option-label">{{ option.label }}</span>
              <svg
                v-if="projectSortMode === option.value"
                viewBox="0 0 20 20"
                fill="none"
                class="project-sort-option-check"
                aria-hidden="true"
              >
                <path d="M4.5 10.5L8 14L15.5 6.5" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"/>
              </svg>
            </button>
          </div>
        </div>
      </div>
      <div class="projects-list" :class="{ dragging: isDragging }">

        <!-- 被拖拽的 ghost 元素（跟随鼠标） -->
        <div
          v-if="isDragging && draggedElement && draggedProjectId"
          class="project-wrapper dragging-ghost"
          :style="{
            position: 'fixed',
            top: draggedElement.top + 'px',
            left: draggedElement.left + 'px',
            width: draggedElement.width + 'px',
            zIndex: 1000,
            pointerEvents: 'none'
          }"
        >
          <div class="project-item">
            <div class="project-left">
              <HugeiconsIcon :icon="FolderIcon" class="project-icon" />
              <div class="project-info">
                <div class="project-name">{{ projects.find(p => p.id === draggedProjectId)?.name }}</div>
              </div>
            </div>
          </div>
        </div>

        <!-- 正常的项目列表 -->
        <TransitionGroup
          name="project-list"
          tag="div"
          class="projects-container"
        >
          <div
            v-for="project in filteredDisplayProjects"
            :key="project.id"
            class="project-wrapper"
            :data-project-id="project.id"
            :class="{
              'drag-over': dragOverProjectId === project.id && isDragging,
              'is-dragged': project.id === draggedProjectId && isDragging,
              'search-active': isProjectSearchActive
            }"
            @mousedown="handleProjectMouseDown($event, project.id)"
          >
          <!-- 项目行 -->
          <div class="project-item" @click="isSettingsVisible = false; selectProject(project.id)">
            <div class="project-left">
              <HugeiconsIcon :icon="FolderIcon" class="project-icon" />
              <div class="project-info">
                <div class="project-name">{{ project.name }}</div>
                <!-- <div class="project-path">{{ project.path }}</div> -->
              </div>
            </div>
            <div class="project-right">
              <button
                class="btn-new-chat"
                title="创建新对话"
                draggable="false"
                @click.stop="isSettingsVisible = false; startNewConversation(project.id)"
              >
                <HugeiconsIcon :icon="AddIcon" />
              </button>
              <button
                class="btn-refresh"
                :class="{ refreshing: refreshingProjects.has(project.id) }"
                title="刷新会话列表"
                draggable="false"
                @click.stop="refreshProjectSessions(project.id)"
              >
                <HugeiconsIcon :icon="RefreshIcon" :class="{ 'spin': refreshingProjects.has(project.id) }" />
              </button>
              <button
                class="btn-delete"
                title="删除项目"
                draggable="false"
                @click.stop="deleteProject(project.id, $event)"
              >
                <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
                  <path d="M3 4h10M6 4V3h4v1M5 4h6v9H5V4z" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round" fill="none"/>
                </svg>
              </button>
            </div>
          </div>

          <!-- 聊天记录列表（展开时显示，带过渡动画） -->
          <Transition name="conversations-expand">
            <div v-if="isProjectExpanded(project.id)" class="conversations-nested" @mousedown.stop>
              <div
                v-for="conv in getVisibleConversations(project.id)"
                :key="conv.id"
                :class="['conversation-item', {
                  active: selectedConversation?.id === conv.id
                }]"
                @click="isSettingsVisible = false; selectConversation(conv)"
              >
                <!-- 连接状态指示点 -->
                <span
                  :class="[
                    'conv-status-dot',
                    getConversationConnectionStatus(conv),
                    {
                      streaming: isConversationStreaming(conv),
                      completed: !isConversationStreaming(conv) && hasConversationCompletion(conv),
                      visible: getConversationConnectionStatus(conv) !== 'disconnected' || isConversationStreaming(conv) || hasConversationCompletion(conv),
                    }
                  ]"
                  :title="getConversationStatusDotTitle(conv)"
                ></span>
                <!-- Pin 按钮 -->
                <button
                  class="btn-pin-conv"
                  :class="{ pinned: conv.pinned }"
                  :title="conv.pinned ? '取消固定' : '固定到顶部'"
                  @click.stop="toggleConversationPin(conv, project.id, $event)"
                >
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <line x1="12" y1="17" x2="12" y2="22"></line>
                    <path d="M5 17h14v-1.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V6h1a2 2 0 0 0 0-4H8a2 2 0 0 0 0 4h1v4.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24Z"></path>
                  </svg>
                </button>
                <div class="conv-main">
                  <div class="conv-title">{{ conv.title || 'Untitled' }}</div>
                  <div class="conv-preview">{{ conv.messageCount }} 条消息 · {{ conv.size }}</div>
                </div>
                <div class="conv-right">
                  <div class="conv-time">{{ formatRelativeTime(conv.timestamp) }}</div>
                  <button
                    class="btn-delete-conv"
                    title="删除对话"
                    @click.stop="deleteConversation(conv, project.id, $event)"
                  >
                    <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
                      <path d="M3 4h10M6 4V3h4v1M5 4h6v9H5V4z" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round" fill="none"/>
                    </svg>
                  </button>
                </div>
              </div>

              <!-- 聊天记录展开/折叠按钮 -->
              <div
                v-if="hasMoreConversations(project.id) || getConversationLimit(project.id) > 10"
                class="conversation-actions"
              >
                <button
                  v-if="hasMoreConversations(project.id)"
                  type="button"
                  class="show-more"
                  @click.stop="showMoreConversations(project.id)"
                >
                  显示更多 ({{ getProjectConversations(project.id).length - getConversationLimit(project.id) }})
                </button>
                <button
                  v-if="getConversationLimit(project.id) > 10"
                  type="button"
                  class="show-more show-less"
                  @click.stop="collapseConversations(project.id)"
                >
                  折叠
                </button>
              </div>

              <!-- 空状态 -->
              <div v-if="getProjectConversations(project.id).length === 0" class="empty-state">
                暂无对话记录
              </div>
            </div>
          </Transition>
          </div>
        </TransitionGroup>
        <div v-if="filteredDisplayProjects.length === 0" class="empty-state project-search-empty">
          未找到匹配的项目
        </div>
      </div>

      <div class="left-panel-footer">
        <button
          class="settings-entry"
          @click="isSettingsVisible = true"
        >
          <HugeiconsIcon :icon="Settings01Icon" class="settings-entry-icon" />
          <span class="settings-entry-label">设置</span>
        </button>
        <span class="settings-entry-version">v{{ appVersion }}</span>
      </div>

    </div>

    <!-- 拖拽分隔条 -->
    <div
      class="resizer"
      @mousedown="startResize"
      :class="{ 'is-resizing': isResizing }"
    ></div>

    <!-- 右侧：聊天区域 -->
    <div class="right-panel">
      <!-- 没有选择聊天记录时显示欢迎页面 -->
      <WelcomePage v-if="shouldShowWelcomePage" :has-projects="projects.length > 0" />

      <!-- 选择聊天记录后显示聊天界面 -->
      <template v-else>
        <!-- Session Tab 栏（草稿会话显示初始化 tab，多会话时显示真实 tab） -->
        <div v-if="shouldShowSessionTabs" class="session-tabs">
          <div
            v-for="sessionId in visibleSessionTabs"
            :key="sessionId"
            :class="['session-tab', {
              active: isDraftSessionTab(sessionId) || sessionId === currentSessionId,
              dragging: sessionId === draggedTabSessionId,
              'drag-over-before': sessionId === dragOverTabSessionId && dragOverTabPosition === 'before',
              'drag-over-after': sessionId === dragOverTabSessionId && dragOverTabPosition === 'after',
            }]"
            :data-streaming="getTabStatusClass(sessionId) === 'streaming'"
            :data-completion="hasTabCompletion(sessionId)"
            :data-awaiting-approval="hasPendingPermission(sessionId)"
            :title="getSessionTabTooltip(sessionId)"
            :draggable="!isDraftSessionTab(sessionId)"
            @dragstart="handleTabDragStart(sessionId, $event)"
            @dragover="handleTabDragOver(sessionId, $event)"
            @dragleave="handleTabDragLeave(sessionId)"
            @drop="handleTabDrop(sessionId, $event)"
            @dragend="handleTabDragEnd"
            @click="handleTabClick(sessionId)"
          >
            <span class="tab-name">{{ getSessionDisplayName(sessionId) }}</span>
            <span
              v-if="hasPendingPermission(sessionId)"
              class="tab-approval-badge"
              title="当前会话有待批准的权限申请"
            >
              等待批准
            </span>
            <button
              v-if="!isDraftSessionTab(sessionId)"
              class="tab-close"
              @click.stop="closeSession(sessionId)"
              title="关闭此会话"
            >
              ×
            </button>
          </div>
        </div>

        <!-- 聊天头部 -->
        <div class="chat-header">
          <!-- 左侧：状态、标题、sessionId -->
          <div class="chat-header-left">
            <div class="chat-project-name">{{ currentProjectName }}</div>
            <div class="chat-header-row">
              <!-- 连接状态指示器 -->
              <div :class="['connection-status', currentConversationConnectionState]" title="会话连接状态">
                <span class="status-dot"></span>
                <span class="status-text">{{ currentConnectionStateText }}</span>
              </div>
              <div class="chat-title">
                {{ truncatedTitle || '选择一个对话' }}
                <span v-if="isPlanMode" class="mode-badge">计划模式</span>
              </div>
            </div>
            <div class="chat-session-id-row">
              <div class="chat-session-id">{{ displaySessionId }}</div>
              <button
                v-if="sessionFilePath"
                class="chat-session-copy-btn"
                type="button"
                :title="sessionPathCopied ? '已复制 session 文件地址' : '复制 session 文件地址'"
                :aria-label="sessionPathCopied ? '已复制 session 文件地址' : '复制 session 文件地址'"
                @click="copySessionFilePath"
              >
                <svg v-if="!sessionPathCopied" width="14" height="14" viewBox="0 0 24 24" fill="none" aria-hidden="true">
                  <rect x="9" y="9" width="10" height="10" rx="2" stroke="currentColor" stroke-width="1.8" />
                  <path d="M7 15H6C4.89543 15 4 14.1046 4 13V6C4 4.89543 4.89543 4 6 4H13C14.1046 4 15 4.89543 15 6V7" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" />
                </svg>
                <svg v-else width="14" height="14" viewBox="0 0 24 24" fill="none" aria-hidden="true">
                  <path d="M20 6L9 17L4 12" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
                </svg>
              </button>
            </div>
          </div>

          <div v-if="canShowWorkspace" class="chat-header-actions">
            <button
              class="chat-header-btn"
              :class="{ active: isWorkspaceVisible }"
              type="button"
              :title="isWorkspaceVisible ? '关闭工作区' : '展开工作区'"
              @click="toggleWorkspacePanel"
            >
              <svg class="workspace-toggle-icon" width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
                <path d="M5.5 3.5L10.5 8L5.5 12.5" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" />
                <path d="M3 3V13" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
              </svg>
              <span>{{ isWorkspaceVisible ? '关闭工作区' : '展开工作区' }}</span>
            </button>
          </div>
        </div>

        <div ref="chatContentShellRef" class="chat-content-shell">
          <div class="chat-main-column">
            <div v-if="isSessionSearchVisible" class="session-search-bar">
              <label class="session-search-input-wrap">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <circle cx="11" cy="11" r="7" />
                  <path d="m20 20-3.5-3.5" />
                </svg>
                <input
                  ref="sessionSearchInputRef"
                  v-model="sessionSearchQuery"
                  type="text"
                  class="session-search-input"
                  placeholder="搜索当前 session 的消息..."
                >
              </label>

              <div class="session-search-actions">
                <span class="session-search-status">{{ sessionSearchStatusText }}</span>
                <button
                  type="button"
                  class="session-search-btn"
                  :disabled="!hasSessionSearchResults"
                  title="上一条匹配"
                  @click="goToSessionSearchMatch(-1)"
                >
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <path d="m18 15-6-6-6 6" />
                  </svg>
                </button>
                <button
                  type="button"
                  class="session-search-btn"
                  :disabled="!hasSessionSearchResults"
                  title="下一条匹配"
                  @click="goToSessionSearchMatch(1)"
                >
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <path d="m6 9 6 6 6-6" />
                  </svg>
                </button>
                <button
                  v-if="hasSessionSearchQuery"
                  type="button"
                  class="session-search-btn clear"
                  title="清空搜索"
                  @click="clearSessionSearch"
                >
                  清空
                </button>
                <button
                  type="button"
                  class="session-search-btn clear"
                  title="关闭搜索"
                  @click="hideSessionSearch"
                >
                  关闭
                </button>
              </div>
            </div>

            <!-- 消息列表 -->
            <MessageList
              :messages="messages"
              :is-streaming="isStreaming"
              :pending-permissions="pendingPermissions"
              :session-id="currentSessionId || ''"
              :subagent-runtime="currentSubagentRuntime"
              :rewind-turns="rewindTurns"
              :rewind-busy="rewindBusy"
              :search-query="sessionSearchQuery"
              :active-search-result-index="sessionSearchActiveIndex"
              @approve="approvePermission"
              @approve-always="approvePermissionAlways"
              @reject="rejectPermission"
              @copy="copyAssistantMessage"
              @regenerate="regenerateAssistantMessage"
              @rewind="executeRewind"
              @search-results-change="handleSearchResultsChange"
            />

            <!-- 炫酷的执行中动画 -->
            <div class="thinking-animation-wrapper">
              <ThinkingAnimation
                v-if="isStreaming"
                :started-at="currentStreamingStartedAt ?? undefined"
              />
            </div>

            <!-- 输入区域 -->
            <div class="input-section">
              <MessageInput
                :session-id="messageBindingSessionId"
                :disabled="!canSendMessage"
                :streaming="isStreaming"
                :git-info="currentGitInfo"
                :project-path="currentProject?.path || ''"
                :rewind-draft="rewindDraftText"
                :rewind-draft-version="rewindDraftVersion"
                :todo-state="todoPanelState"
                @send="sendMessage"
                @stop="stopStreaming"
                @execute-command="executeBuiltinCommand"
                @git-updated="currentGitInfo = $event"
                @session-config-updated="updateConversationSessionConfig"
              />
            </div>
          </div>

          <Transition name="workspace-panel">
            <div
              v-if="currentProject?.path && isWorkspaceVisible"
              class="workspace-panel-shell"
            >
              <div
                class="workspace-panel-resizer"
                :class="{ 'is-resizing': isWorkspaceResizing }"
                @mousedown.prevent="startWorkspaceResize"
              ></div>
              <ProjectDirectoryPanel
                :project-path="currentProject.path"
                :project-name="currentProject.name"
                :panel-width="workspacePanelWidth"
                @close="isWorkspaceVisible = false"
              />
            </div>
          </Transition>
        </div>
      </template>
    </div>

    <!-- 导入项目对话框 -->
    <ImportProjectsDialog
      :open="importDialogOpen"
      @close="importDialogOpen = false"
      @imported="onProjectImported"
    />
    </template>
  </div>
</template>

<style scoped>
/* 执行中动画包装器 */
.thinking-animation-wrapper {
  padding-left: 2.5rem;
  padding-top: 0.75rem;
  padding-bottom: 0.5rem;
}

.projects-view {
  display: flex;
  height: 100%;
  background-color: var(--bg-primary, #ffffff);
}

/* 左侧面板 - 项目列表 */
.left-panel {
  display: flex;
  flex-direction: column;
  border-right: none;
  background-color: #fafafa;
  flex-shrink: 0;
}

/* 拖拽分隔条 */
.resizer {
  width: 4px;
  background-color: var(--border-color, #e5e7eb);
  cursor: col-resize;
  flex-shrink: 0;
  transition: background-color 0.2s;
  position: relative;
}

.resizer:hover,
.resizer.is-resizing {
  background-color: var(--primary-color, #3b82f6);
}

.resizer::after {
  content: '';
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  width: 12px;
  height: 24px;
  background-color: transparent;
}

.resizer:hover::after,
.resizer.is-resizing::after {
  background-color: rgba(255, 255, 255, 0.5);
  border-radius: 2px;
}

.section-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 0.625rem 0.45rem 0.375rem;
  border-bottom: 1px solid var(--border-color, #e5e7eb);
  flex-shrink: 0;
}

.section-title {
  margin: 0;
  font-size: 0.875rem;
  font-weight: 600;
  color: var(--text-primary, #1f2937);
}

.projects-list {
  padding: 0.25rem;
  overflow-y: auto;
  flex: 1;
  -ms-overflow-style: none;
}

.project-toolbar {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.55rem 0.45rem 0.35rem;
  flex-shrink: 0;
}

.project-toolbar-title {
  flex-shrink: 0;
  font-size: 0.82rem;
  font-weight: 500;
  color: var(--text-muted, #9ca3af);
  letter-spacing: 0.01em;
}

.project-search-input-wrap {
  display: flex;
  align-items: center;
  gap: 0.45rem;
  width: 100%;
  padding: 0.45rem 0.65rem;
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 999px;
  background: var(--bg-primary, #ffffff);
  color: var(--text-muted, #9ca3af);
  transition: border-color 0.2s ease, box-shadow 0.2s ease;
}

.toolbar-search {
  flex: 1;
  min-width: 0;
}

.project-search-input-wrap:focus-within {
  border-color: rgba(59, 130, 246, 0.35);
  box-shadow: 0 0 0 3px rgba(59, 130, 246, 0.08);
}

.project-search-icon {
  width: 0.9rem;
  height: 0.9rem;
  flex-shrink: 0;
}

.project-search-input {
  width: 100%;
  min-width: 0;
  border: none;
  outline: none;
  background: transparent;
  font-size: 0.875rem;
  color: var(--text-primary, #111827);
}

.project-search-input::placeholder {
  color: var(--text-muted, #9ca3af);
}

.project-sort-wrap {
  position: relative;
  flex-shrink: 0;
}

.project-sort-btn {
  width: 2rem;
  height: 2rem;
  border: none;
  border-radius: 999px;
  background: transparent;
  color: var(--text-secondary, #6b7280);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  transition: background-color 0.18s ease, color 0.18s ease;
  -webkit-app-region: no-drag;
  app-region: no-drag;
}

.project-sort-btn:hover {
  background: rgba(15, 23, 42, 0.05);
  color: var(--text-primary, #111827);
}

.project-sort-btn-icon {
  width: 1rem;
  height: 1rem;
}

.project-sort-menu {
  position: absolute;
  top: calc(100% + 0.4rem);
  right: 0;
  width: 220px;
  padding: 0.55rem;
  border-radius: 1.1rem;
  border: 1px solid rgba(148, 163, 184, 0.22);
  background: rgba(255, 255, 255, 0.96);
  backdrop-filter: blur(18px);
  box-shadow: 0 20px 40px rgba(15, 23, 42, 0.12);
  z-index: 30;
}

.project-sort-menu-title {
  padding: 0.35rem 0.45rem 0.5rem;
  font-size: 0.78rem;
  font-weight: 600;
  color: var(--text-muted, #9ca3af);
}

.project-sort-option {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 0.65rem;
  padding: 0.72rem 0.65rem;
  border: none;
  border-radius: 0.9rem;
  background: transparent;
  color: var(--text-primary, #1f2937);
  cursor: pointer;
  text-align: left;
  transition: background-color 0.18s ease, color 0.18s ease;
  -webkit-app-region: no-drag;
  app-region: no-drag;
}

.project-sort-option:hover,
.project-sort-option.active {
  background: rgba(15, 23, 42, 0.045);
}

.project-sort-option-icon {
  width: 1.15rem;
  height: 1.15rem;
  color: var(--text-secondary, #6b7280);
  flex-shrink: 0;
}

.project-sort-option-icon svg,
.project-sort-option-check {
  width: 100%;
  height: 100%;
}

.project-sort-option-label {
  flex: 1;
  min-width: 0;
  font-size: 0.875rem;
  font-weight: 500;
}

.project-sort-option-check {
  width: 1rem;
  height: 1rem;
  color: var(--text-primary, #111827);
  flex-shrink: 0;
}

.left-panel-footer {
  padding: 0.625rem 0.75rem 0.75rem;
  border-top: 1px solid var(--border-color, #e5e7eb);
  background-color: #fafafa;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.5rem;
}

.settings-entry {
  display: inline-flex;
  align-items: center;
  gap: 0.55rem;
  padding: 0.5rem 0.2rem;
  border: none;
  background-color: transparent;
  color: var(--text-secondary, #6b7280);
  font-size: 0.9375rem;
  font-weight: 600;
  cursor: pointer;
  transition: color 0.2s ease, background-color 0.2s ease;
  flex-shrink: 0;
  border-radius: 0.625rem;
}

.settings-entry:hover {
  color: var(--text-primary, #1f2937);
  background-color: rgba(15, 23, 42, 0.05);
}

.settings-entry-icon {
  width: 1.1rem;
  height: 1.1rem;
  color: currentColor;
  flex-shrink: 0;
}

.settings-entry-label {
  flex-shrink: 0;
}

.settings-entry-version {
  font-size: 0.75rem;
  color: var(--text-muted, #9ca3af);
  flex-shrink: 0;
}

.projects-list::-webkit-scrollbar {
  display: none;
}

.button-row {
  display: flex;
  flex-direction: column;
  gap: 0.125rem;
  width: 100%;
}

.action-button {
  width: 100%;
  min-height: 2.2rem;
  padding: 0.45rem 0.5rem;
  border-radius: 0.9rem;
  font-size: 0.8125rem;
  font-weight: 500;
  cursor: pointer;
  transition: background-color 0.18s ease, color 0.18s ease;
  background: transparent;
  color: var(--text-primary, #2f2f2f);
  border: 1px solid transparent;
  display: flex;
  align-items: center;
  justify-content: flex-start;
  gap: 0.625rem;
  -webkit-app-region: no-drag;
  app-region: no-drag;
}

.action-button:hover {
  background: rgba(15, 23, 42, 0.045);
  color: var(--text-primary, #111827);
}

.action-button:active {
  background: rgba(15, 23, 42, 0.08);
}

.folder-icon-small {
  width: 18px;
  height: 18px;
  flex-shrink: 0;
}

.btn-open .folder-icon-small,
.btn-import .folder-icon-small {
  color: currentColor;
}

/* 项目包装器 */
.project-wrapper {
  margin-bottom: 0.5rem;
  transition: all 0.25s cubic-bezier(0.2, 0, 0.2, 1);
}

/* 项目列表容器 */
.projects-container {
  display: flex;
  flex-direction: column;
}

/* TransitionGroup 移动动画 - 这是关键 */
.project-list-move {
  transition: transform 0.25s cubic-bezier(0.2, 0, 0.2, 1);
}

/* 项目进入/离开动画 */
.project-list-enter-active,
.project-list-leave-active {
  transition: all 0.25s cubic-bezier(0.2, 0, 0.2, 1);
}

.project-list-enter-from {
  opacity: 0;
  transform: translateY(-10px);
}

.project-list-leave-to {
  opacity: 0;
  transform: translateY(10px);
}

/* 确保离开的项目脱离文档流 */
.project-list-leave-active {
  position: absolute;
  width: 100%;
}

/* 拖拽样式 */
.project-wrapper {
  cursor: grab;
  user-select: none;
  -webkit-user-select: none;
  -moz-user-select: none;
  -ms-user-select: none;
}

.project-wrapper.search-active {
  cursor: default;
}

.project-wrapper:active {
  cursor: grabbing;
}

/* 被拖拽的 ghost 元素（跟随鼠标） */
.project-wrapper.dragging-ghost {
  opacity: 0.95;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.2);
  transition: none !important;
  pointer-events: none;
}

.project-wrapper.dragging-ghost .project-item {
  background: var(--bg-primary, #ffffff);
  border: 1px solid var(--primary-color, #3b82f6);
}

/* 列表中被拖拽的项目（显示为占位符） */
.project-wrapper.is-dragged {
  opacity: 0.3 !important;
  pointer-events: none;
}

/* 占位符 */
.project-wrapper.placeholder {
  pointer-events: none;
}

.project-wrapper.placeholder .project-item {
  min-height: 42px;
  border: 2px dashed var(--text-muted, #9ca3af);
  background: rgba(156, 163, 175, 0.1);
  border-radius: 0.5rem;
}

/* 拖拽时的其他项目平滑过渡 - TransitionGroup 会自动处理移动动画 */
.projects-list.dragging .project-wrapper:not(.dragging-ghost):not(.is-dragged) {
  /* TransitionGroup 的 -move 类会处理移动动画 */
}

/* 拖拽的目标位置指示 */
.project-wrapper.drag-over .project-item {
  border-top: 3px solid var(--primary-color, #3b82f6);
  transition: border-color 0.15s ease;
}

/* 防止拖拽时选中任何文本 */
.projects-list.dragging,
.projects-list.dragging * {
  user-select: none !important;
  -webkit-user-select: none !important;
  -moz-user-select: none !important;
  -ms-user-select: none !important;
}

/* 项目行 */
.project-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.25rem;
  padding: 0.5rem 0.5rem;
  border-radius: 0.5rem;
  cursor: pointer;
  transition: all 0.2s;
  background-color: transparent;
}

.project-item:hover {
  background-color: rgba(0, 0, 0, 0.04);
}

.project-left {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  flex: 1;
  min-width: 0;
}

.project-icon {
  width: 1rem;
  height: 1rem;
  flex-shrink: 0;
  color: var(--text-secondary, #6b7280);
}

.btn-icon {
  width: 0.75rem;
  height: 0.75rem;
}

.project-info {
  flex: 1;
  min-width: 0;
}

.project-name {
  font-size: 0.875rem;
  font-weight: 500;
  color: var(--text-primary, #1f2937);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.project-path {
  font-size: 0.75rem;
  color: var(--text-muted, #9ca3af);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: 'Monaco', 'Menlo', monospace;
}

/* 右侧按钮容器 */
.project-right {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  flex-shrink: 0;
}

/* Safari 14.0 / macOS 11.2 等旧版 WebKit 不支持 flex gap，使用 margin 兜底 */
.project-right > * + * {
  margin-left: 0.25rem;
}

@supports (gap: 0.25rem) {
  .project-right > * + * {
    margin-left: 0;
  }
}

/* 新对话按钮 */
.btn-new-chat {
  width: 14px;
  height: 14px;
  padding: 0;
  background: transparent;
  border: none;
  border-radius: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  color: var(--text-secondary, #6b7280);
  transition: all 0.2s;
  flex-shrink: 0;
}

.btn-new-chat:hover {
  background-color: var(--bg-tertiary, #f3f4f6);
  color: var(--primary-color, #3b82f6);
}

/* 删除按钮 */
.btn-delete {
  width: 20px;
  height: 20px;
  padding: 0;
  background: transparent;
  border: none;
  border-radius: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  color: var(--text-secondary, #6b7280);
  transition: all 0.2s;
  flex-shrink: 0;
}

.btn-delete:hover {
  background-color: #fee2e2;
  color: #ef4444;
}

/* 展开按钮 */
.btn-expand {
  width: 14px;
  height: 14px;
  padding: 0;
  background: transparent;
  border: none;
  border-radius: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  color: var(--text-secondary, #6b7280);
  transition: all 0.2s;
  flex-shrink: 0;
}

.btn-expand:hover {
  background-color: var(--bg-tertiary, #f3f4f6);
  color: var(--text-primary, #1f2937);
}

.btn-expand svg {
  transition: transform 0.2s;
}

.btn-expand.expanded svg {
  transform: rotate(180deg);
}

/* 刷新按钮 */
.btn-refresh {
  width: 14px;
  height: 14px;
  padding: 0;
  background: transparent;
  border: none;
  border-radius: 4px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-secondary, #6b7280);
  transition: all 0.2s;
}

.btn-refresh:hover {
  background-color: var(--bg-tertiary, #f3f4f6);
  color: var(--primary-color, #3b82f6);
}

.btn-refresh.refreshing {
  color: var(--primary-color, #3b82f6);
}

/* 旋转动画 */
.btn-refresh svg.spin {
  animation: spin 1s linear infinite;
}

@keyframes spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}

/* 嵌套的聊天记录列表 */
.conversations-nested {
  margin-left: 0.75rem;
  margin-right: 0.25rem;
  margin-top: 0.125rem;
  margin-bottom: 0.25rem;
}

/* 展开折叠动画 */
.conversations-expand-enter-active,
.conversations-expand-leave-active {
  transition: all 0.25s ease-out;
  overflow: hidden;
}

.conversations-expand-enter-from,
.conversations-expand-leave-to {
  opacity: 0;
  max-height: 0;
  transform: translateY(-8px);
}

.conversations-expand-enter-to,
.conversations-expand-leave-from {
  opacity: 1;
  max-height: 500px;
  transform: translateY(0);
}

.conversation-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.25rem;
  padding: 0.5rem;
  border-radius: 0.375rem;
  cursor: pointer;
  transition: all 0.2s;
  margin-bottom: 0.0625rem;
  background-color: transparent;
}

.conversation-item:hover {
  background-color: rgba(0, 0, 0, 0.04);
}

.conversation-item.active {
  background-color: rgba(59, 130, 246, 0.1);
  border: 1px solid var(--primary-color, #3b82f6);
}

/* 连接状态指示点 */
.conv-status-dot {
  position: relative;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
  opacity: 0;
  transition: opacity 0.2s ease, transform 0.2s ease, box-shadow 0.2s ease;
}

.conv-status-dot.visible {
  opacity: 1;
}

/* 已连接状态 - 主题色点 */
.conv-status-dot.connected {
  background-color: var(--primary-color, #3b82f6);
  box-shadow: 0 0 0 2px var(--primary-color-20, rgba(59, 130, 246, 0.2));
}

/* 连接中状态 */
.conv-status-dot.connecting {
  background-color: #94a3b8;
  box-shadow: 0 0 0 2px rgba(148, 163, 184, 0.18);
}

/* 流式输出中状态 - 小转圈 */
.conv-status-dot.streaming {
  width: 10px;
  height: 10px;
  background-color: transparent;
  border: 1.6px solid rgba(59, 130, 246, 0.22);
  border-top-color: var(--primary-color, #3b82f6);
  box-shadow: none;
  animation: conv-status-spin 0.8s linear infinite;
}

.conv-status-dot.completed {
  background-color: #16a34a;
  box-shadow: 0 0 0 2px rgba(22, 163, 74, 0.16);
  animation: conv-status-complete 1.8s ease-in-out infinite;
}

.conv-status-dot.completed::after {
  content: '';
  position: absolute;
  inset: -3px;
  border-radius: 999px;
  border: 1px solid rgba(22, 163, 74, 0.22);
  animation: conv-status-complete-ring 1.8s ease-in-out infinite;
}

@keyframes conv-status-spin {
  to {
    transform: rotate(360deg);
  }
}

@keyframes conv-status-complete {
  0%, 100% {
    transform: scale(1);
    box-shadow: 0 0 0 2px rgba(22, 163, 74, 0.16);
  }

  50% {
    transform: scale(1.06);
    box-shadow: 0 0 0 3px rgba(22, 163, 74, 0.2);
  }
}

@keyframes conv-status-complete-ring {
  0%, 100% {
    opacity: 0.45;
    transform: scale(0.96);
  }

  50% {
    opacity: 0.16;
    transform: scale(1.18);
  }
}

/* 任务完成红点 */
.task-completion-dot {
  position: relative;
  flex-shrink: 0;
  width: 8px;
  height: 8px;
  margin-right: 4px;
}

.task-completion-dot::before {
  content: '';
  position: absolute;
  top: 0;
  left: 0;
  width: 8px;
  height: 8px;
  background-color: #ef4444;
  border-radius: 50%;
  animation: pulse-dot 2s infinite;
}

@keyframes pulse-dot {
  0%, 100% {
    opacity: 1;
    transform: scale(1);
  }
  50% {
    opacity: 0.6;
    transform: scale(1.1);
  }
}

/* 消息完成红点 */
.message-completion-dot {
  position: absolute;
  flex-shrink: 0;
  width: 8px;
  height: 8px;
  margin-left: 4px;
  margin-top: 4px;
}

.message-completion-dot::before {
  content: '';
  position: absolute;
  top: 0;
  left: 0;
  width: 8px;
  height: 8px;
  background-color: #ef4444;
  border-radius: 50%;
  animation: pulse-dot 2s infinite;
}

.conv-main {
  flex: 1;
  min-width: 0;
}

.conv-title {
  font-size: 0.8125rem;
  font-weight: 500;
  color: var(--text-primary, #1f2937);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.conv-preview {
  font-size: 0.75rem;
  color: var(--text-muted, #9ca3af);
  margin-top: 0.125rem;
}

.conv-time {
  font-size: 0.75rem;
  color: var(--text-muted, #9ca3af);
  flex-shrink: 0;
}

.conv-right {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  flex-shrink: 0;
}

/* Pin 按钮 */
.btn-pin-conv {
  width: 16px;
  height: 16px;
  padding: 0;
  background: transparent;
  border: none;
  border-radius: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  color: var(--text-muted, #9ca3af);
  transition: color 0.2s, background-color 0.2s;
  flex-shrink: 0;
  opacity: 0;
  visibility: hidden;
  pointer-events: none;
}

.conversation-item:hover .btn-pin-conv {
  opacity: 1;
  visibility: visible;
  pointer-events: auto;
}

.btn-pin-conv:hover {
  background-color: #e0e7ff;
  color: var(--primary-color, #3b82f6);
}

.btn-pin-conv.pinned {
  opacity: 1;
  visibility: visible;
  pointer-events: auto;
  color: var(--primary-color, #3b82f6);
}

.btn-pin-conv.pinned:hover {
  background-color: #fee2e2;
  color: #ef4444;
}

/* 删除按钮 */
.btn-delete-conv {
  width: 16px;
  height: 16px;
  padding: 0;
  background: transparent;
  border: none;
  border-radius: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  color: var(--text-muted, #9ca3af);
  transition: all 0.2s;
  flex-shrink: 0;
  opacity: 0;
}

.conversation-item:hover .btn-delete-conv {
  opacity: 1;
}

.btn-delete-conv:hover {
  background-color: #fee2e2;
  color: #ef4444;
}

.conversation-actions {
  display: flex;
  gap: 0.5rem;
  padding-top: 0.25rem;
}

/* 显示更多按钮 */
.show-more {
  flex: 1;
  padding: 0.5rem;
  text-align: center;
  font-size: 0.75rem;
  color: var(--primary-color, #3b82f6);
  cursor: pointer;
  border-radius: 0.375rem;
  border: none;
  background: transparent;
  transition: all 0.2s;
}

.show-more:hover {
  background-color: var(--bg-tertiary, #f3f4f6);
}

.show-less {
  color: var(--text-secondary, #6b7280);
}

.empty-state {
  padding: 1rem;
  text-align: center;
  color: var(--text-muted, #9ca3af);
  font-size: 0.75rem;
}

.project-search-empty {
  padding-top: 1.5rem;
}

/* 右侧面板 */
.right-panel {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  background-color: var(--bg-primary, #ffffff);
  overflow: hidden;
}

.chat-header-actions {
  display: flex;
  align-items: flex-end;
  align-self: flex-end;
  flex-shrink: 0;
  gap: 0.5rem;
  margin-top: 0;
  padding-bottom: 0.05rem;
}

.chat-header-btn {
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
  border: 1px solid var(--border-color, #e5e7eb);
  background: var(--bg-secondary, #f9fafb);
  color: var(--text-secondary, #6b7280);
  border-radius: 0.625rem;
  padding: 0.45rem 0.7rem;
  font-size: 0.78rem;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.2s ease;
}

.workspace-toggle-icon {
  transition: transform 0.22s ease;
}

.chat-header-btn.active .workspace-toggle-icon {
  transform: rotate(180deg);
}

.chat-header-btn:hover,
.chat-header-btn.active {
  color: var(--text-primary, #1f2937);
  border-color: var(--primary-color, #3b82f6);
  background: var(--primary-color-10, rgba(59, 130, 246, 0.1));
}

.chat-header-btn:disabled {
  cursor: not-allowed;
  opacity: 0.65;
}

/* Session Tabs */
.session-tabs {
  display: flex;
  align-items: flex-end;
  gap: 0.25rem;
  padding: 0.35rem 1rem 0;
  background: var(--bg-primary, #ffffff);
  border-bottom: 1px solid var(--border-color, #e5e7eb);
  overflow-x: auto;
  overflow-y: hidden;
  min-height: 42px;
}

.dark .session-tabs {
  background: var(--bg-primary, #111827);
}

.session-tab {
  display: flex;
  align-items: center;
  gap: 0.45rem;
  flex: 1 1 0;
  min-width: 96px;
  max-width: 156px;
  height: 38px;
  margin-bottom: -1px;
  padding: 0 0.8rem;
  background: var(--bg-secondary, #f3f4f6);
  border: 1px solid var(--border-color, #e5e7eb);
  border-bottom: none;
  border-radius: 10px 10px 0 0;
  cursor: pointer;
  font-size: 0.875rem;
  font-weight: 500;
  white-space: nowrap;
  overflow: hidden;
  transition: all 0.15s ease;
}

.dark .session-tab {
  background: var(--bg-secondary, #1f2937);
  border-color: var(--border-color, #4b5563);
}

.session-tab:hover {
  background: var(--bg-hover, #e5e7eb);
}

.dark .session-tab:hover {
  background: var(--bg-hover, #4b5563);
}

.session-tab.active {
  background: var(--color-primary, #3b82f6);
  border-color: var(--color-primary, #3b82f6);
  color: white;
  transform: translateY(0);
}

.session-tab.dragging {
  opacity: 0.55;
}

.session-tab.drag-over-before,
.session-tab.drag-over-after {
  position: relative;
}

.session-tab.drag-over-before::before,
.session-tab.drag-over-after::after {
  content: '';
  position: absolute;
  top: 6px;
  bottom: 6px;
  width: 3px;
  border-radius: 999px;
  background: var(--color-primary, #3b82f6);
  box-shadow: 0 0 0 2px rgba(59, 130, 246, 0.14);
}

.session-tab.drag-over-before::before {
  left: -2px;
}

.session-tab.drag-over-after::after {
  right: -2px;
}

.session-tab[data-awaiting-approval="true"] {
  border-color: rgba(34, 197, 94, 0.45);
}

.session-tab.active[data-awaiting-approval="true"] {
  border-color: var(--color-primary, #3b82f6);
}

/* 后台回复中的 Tab 整体光泽扫过动效（非 active 但有 streaming 状态） */
.session-tab:not(.active)[data-streaming="true"] {
  position: relative;
  overflow: hidden;
}

.session-tab:not(.active)[data-streaming="true"]::before {
  content: '';
  position: absolute;
  top: 0;
  left: -100%;
  width: 100%;
  height: 100%;
  background: linear-gradient(90deg,
    transparent 0%,
    var(--primary-color-25, rgba(59, 130, 246, 0.4)) 50%,
    transparent 100%
  );
  animation: shimmer-sweep 1.5s ease-in-out infinite;
  z-index: 0;
  pointer-events: none;
}

@keyframes shimmer-sweep {
  0% {
    left: -100%;
  }
  100% {
    left: 100%;
  }
}

/* Tab 完成状态边框柔色渐变呼吸动效（非 active 但有 completion 状态） */
.session-tab:not(.active)[data-completion="true"] {
  position: relative;
  animation: border-breathe 2s ease-in-out infinite;
}

@keyframes border-breathe {
  0%, 100% {
    border-color: var(--primary-color, rgba(59, 130, 246, 0.6));
    box-shadow: 0 0 0 0 var(--primary-color-20, rgba(59, 130, 246, 0.3));
  }
  50% {
    border-color: var(--primary-color, rgba(59, 130, 246, 1));
    box-shadow: 0 0 12px 3px var(--primary-color-30, rgba(59, 130, 246, 0.5));
  }
}

.tab-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.tab-approval-badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  height: 1.35rem;
  padding: 0 0.6rem;
  border-radius: 999px;
  background: #e5e7eb;
  color: #22c55e;
  font-size: 0.75rem;
  font-weight: 700;
  line-height: 1;
  flex-shrink: 0;
}

.dark .tab-approval-badge {
  background: rgba(229, 231, 235, 0.16);
  color: #4ade80;
}

.session-tab.active .tab-approval-badge {
  background: rgba(255, 255, 255, 0.88);
  color: #16a34a;
}

.tab-status {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: #9ca3af;
}

.tab-status.connected {
  background: var(--primary-color, #3b82f6);
}

.tab-status.connecting {
  background: #f59e0b;
}

.tab-status.disconnected {
  background: #9ca3af;
}

/* Tab 流式输出状态 - 主题色（无脉冲） */
.tab-status.streaming {
  background: var(--primary-color, #3b82f6);
}

/* Tab 通知红点 */
.tab-notification-badge {
  position: relative;
  width: 8px;
  height: 8px;
}

.tab-notification-badge::before {
  content: '';
  position: absolute;
  top: 0;
  left: 0;
  width: 8px;
  height: 8px;
  background-color: #ef4444;
  border-radius: 50%;
  animation: pulse-dot 2s infinite;
}

/* 激活状态的 Tab 通知红点更明显 */
.session-tab.active .tab-notification-badge::before {
  background-color: #ffffff;
}

.tab-close {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  padding: 0;
  border: none;
  background: transparent;
  color: inherit;
  font-size: 1rem;
  line-height: 1;
  cursor: pointer;
  opacity: 0.6;
  border-radius: 3px;
  margin-left: 0.25rem;
}

.tab-close:hover {
  opacity: 1;
  background: rgba(0, 0, 0, 0.1);
}

.session-tab.active .tab-close:hover {
  background: rgba(255, 255, 255, 0.2);
}

.chat-content-shell {
  display: flex;
  gap: 0;
  flex: 1;
  min-height: 0;
}

.workspace-panel-shell {
  display: flex;
  flex-shrink: 0;
  min-width: 0;
  min-height: 0;
}

.workspace-panel-resizer {
  width: 10px;
  cursor: col-resize;
  flex-shrink: 0;
  position: relative;
  background: transparent;
}

.workspace-panel-resizer::before {
  content: '';
  position: absolute;
  top: 0;
  bottom: 0;
  left: 50%;
  width: 1px;
  transform: translateX(-50%);
  background: var(--border-color, #e5e7eb);
  transition: width 0.2s ease, background-color 0.2s ease;
}

.workspace-panel-resizer:hover::before,
.workspace-panel-resizer.is-resizing::before {
  width: 2px;
  background: rgba(var(--primary-color-rgb, 59, 130, 246), 0.55);
}

.workspace-panel-enter-active,
.workspace-panel-leave-active {
  transition: opacity 0.22s ease, transform 0.22s ease;
}

.workspace-panel-enter-from,
.workspace-panel-leave-to {
  opacity: 0;
  transform: translateX(10px);
}

.workspace-panel-enter-to,
.workspace-panel-leave-from {
  opacity: 1;
  transform: translateX(0);
}

.chat-main-column {
  flex: 1;
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.chat-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-end;
  gap: 1rem;
  padding: 0.35rem 1.5rem 0.5rem;
  border-bottom: 1px solid var(--border-color, #e5e7eb);
}

.chat-header-left {
  display: flex;
  flex-direction: column;
  gap: 0.125rem;
  flex: 1;
  min-width: 0;
}

.chat-header-row {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.session-search-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
  padding: 0.75rem 1.5rem;
  border-bottom: 1px solid var(--border-color, #e5e7eb);
  background: linear-gradient(180deg, rgba(248, 250, 252, 0.96), rgba(255, 255, 255, 0.96));
  backdrop-filter: blur(10px);
}

.session-search-input-wrap {
  display: flex;
  align-items: center;
  gap: 0.65rem;
  flex: 1;
  min-width: 0;
  padding: 0.65rem 0.85rem;
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 0.85rem;
  background: var(--bg-primary, #ffffff);
  color: var(--text-muted, #9ca3af);
  transition: border-color 0.2s ease, box-shadow 0.2s ease;
}

.session-search-input-wrap:focus-within {
  border-color: rgba(59, 130, 246, 0.35);
  box-shadow: 0 0 0 3px rgba(59, 130, 246, 0.08);
}

.session-search-input {
  width: 100%;
  min-width: 0;
  border: none;
  outline: none;
  background: transparent;
  font-size: 0.875rem;
  color: var(--text-primary, #111827);
}

.session-search-input::placeholder {
  color: var(--text-muted, #9ca3af);
}

.session-search-actions {
  display: flex;
  align-items: center;
  gap: 0.45rem;
}

.session-search-status {
  min-width: 5.5rem;
  text-align: right;
  font-size: 0.75rem;
  font-weight: 600;
  color: var(--text-secondary, #6b7280);
}

.session-search-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 2rem;
  height: 2rem;
  padding: 0 0.65rem;
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 0.65rem;
  background: var(--bg-primary, #ffffff);
  color: var(--text-secondary, #6b7280);
  cursor: pointer;
  transition: all 0.2s ease;
}

.session-search-btn:hover:not(:disabled) {
  border-color: rgba(59, 130, 246, 0.3);
  color: var(--primary-color, #3b82f6);
}

.session-search-btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.session-search-btn.clear {
  font-size: 0.75rem;
  font-weight: 600;
}

/* 连接状态指示器 */
.connection-status {
  display: flex;
  align-items: center;
  gap: 0.375rem;
  padding: 0.25rem 0.625rem;
  border-radius: 0.375rem;
  font-size: 0.75rem;
  font-weight: 500;
  transition: all 0.2s;
}

.status-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  animation: pulse 2s infinite;
}

@keyframes pulse {
  0%, 100% {
    opacity: 1;
  }
  50% {
    opacity: 0.5;
  }
}

.connection-status.connected {
  background-color: #d1fae5;
  color: #065f46;
}

.connection-status.connected .status-dot {
  background-color: #10b981;
}

.connection-status.disconnected {
  background-color: #fef3c7;
  color: #92400e;
}

.connection-status.disconnected .status-dot {
  background-color: #f59e0b;
}

.connection-status.connecting {
  background-color: #e0e7ff;
  color: #4338ca;
}

.connection-status.connecting .status-dot {
  background-color: #6366f1;
  animation: blink 1s infinite;
}

@keyframes blink {
  0%, 50% {
    opacity: 1;
  }
  51%, 100% {
    opacity: 0.3;
  }
}

.chat-title {
  font-size: 1rem;
  font-weight: 600;
  color: var(--text-primary, #1f2937);
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.chat-project-name {
  font-size: 1rem;
  font-weight: 600;
  color: var(--text-primary, #1f2937);
  text-align: center;
}

.chat-session-id {
  font-size: 0.7rem;
  color: var(--text-muted, #9ca3af);
  font-family: 'Monaco', 'Menlo', 'Courier New', monospace;
  word-break: break-all;
  white-space: normal;
  line-height: 1.3;
}

.chat-session-id-row {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  align-self: flex-start;
  min-width: 0;
  max-width: 100%;
}

.chat-session-id-row .chat-session-id {
  flex: 0 1 auto;
  min-width: 0;
}

.chat-session-copy-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 1.5rem;
  height: 1.5rem;
  flex-shrink: 0;
  padding: 0;
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 0.375rem;
  background: var(--bg-secondary, #f9fafb);
  color: var(--text-muted, #6b7280);
  cursor: pointer;
  transition: all 0.15s ease;
}

.chat-session-copy-btn:hover {
  color: var(--text-primary, #374151);
  border-color: var(--color-primary, #3b82f6);
}

.chat-session-copy-btn:active {
  transform: scale(0.96);
}

.mode-badge {
  font-size: 0.75rem;
  padding: 0.125rem 0.5rem;
  background-color: #fef3c7;
  color: #92400e;
  border-radius: 0.25rem;
  font-weight: 500;
}

.chat-actions {
  display: flex;
  gap: 0.5rem;
  position: absolute;
  right: 1.5rem;
}

.btn-icon {
  width: 32px;
  height: 32px;
  padding: 0;
  background: transparent;
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 0.375rem;
  font-size: 1rem;
  cursor: pointer;
  color: var(--text-secondary, #6b7280);
  transition: all 0.2s;
}

.btn-icon:hover {
  background-color: var(--bg-secondary, #f9fafb);
  border-color: var(--text-secondary, #6b7280);
}

/* 权限模式按钮 */
.btn-icon.permission-toggle {
  min-width: 32px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.permission-icon {
  font-size: 1.1rem;
  line-height: 1;
}

.btn-icon.plan-mode {
  background-color: #fef3c7;
  border-color: #fbbf24;
  color: #92400e;
}

.btn-icon.plan-mode:hover {
  background-color: #fde68a;
}

.btn-icon.plan-mode:hover {
  background-color: var(--accent-hover, #fde68a);
}

/* 输入区域 */
.input-section {
  padding: 1rem 1.5rem;
  border-top: none;
  background-color: var(--bg-primary, #ffffff);
}

@media (max-width: 1180px) {
  .workspace-panel-shell {
    display: none;
  }
}

@media (max-width: 960px) {
  .session-search-bar {
    flex-direction: column;
    align-items: stretch;
  }

  .session-search-actions {
    justify-content: space-between;
  }

  .session-search-status {
    min-width: 0;
    text-align: left;
  }
}

/* 深色模式 */
@media (prefers-color-scheme: dark) {
  .left-panel-footer {
    background-color: #111827;
    border-top-color: #374151;
  }

  .settings-entry {
    background-color: transparent;
    color: #d1d5db;
  }

  .settings-entry:hover {
    color: #f9fafb;
    background-color: rgba(255, 255, 255, 0.08);
  }

  .settings-entry-version {
    color: #9ca3af;
  }

  .projects-view {
    background-color: var(--bg-primary, #111827);
  }

  .left-panel {
    background-color: #1f2937;
  }

  .resizer {
    background-color: var(--border-color, #374151);
  }

  .resizer:hover,
  .resizer.is-resizing {
    background-color: var(--primary-color, #3b82f6);
  }

  .section-header {
    border-bottom-color: var(--border-color, #374151);
  }

  .project-toolbar-title {
    color: #9ca3af;
  }

  .project-search-input-wrap {
    background: rgba(17, 24, 39, 0.86);
    border-color: rgba(75, 85, 99, 0.7);
    color: #9ca3af;
  }

  .project-search-input {
    color: #f3f4f6;
  }

  .action-button {
    color: #d1d5db;
    background: transparent;
  }

  .action-button:hover {
    color: #f3f4f6;
    background: rgba(255, 255, 255, 0.08);
  }

  .project-sort-btn {
    color: #9ca3af;
  }

  .project-sort-btn:hover {
    color: #f3f4f6;
    background: rgba(255, 255, 255, 0.08);
  }

  .project-sort-menu {
    background: rgba(17, 24, 39, 0.96);
    border-color: rgba(75, 85, 99, 0.55);
    box-shadow: 0 20px 40px rgba(2, 6, 23, 0.34);
  }

  .project-sort-menu-title {
    color: #9ca3af;
  }

  .project-sort-option {
    color: #f3f4f6;
  }

  .project-sort-option:hover,
  .project-sort-option.active {
    background: rgba(255, 255, 255, 0.08);
  }

  .project-sort-option-icon {
    color: #9ca3af;
  }

  .project-sort-option-check {
    color: #f3f4f6;
  }

  .section-title {
    color: var(--text-primary, #f9fafb);
  }

  .project-item,
  .conversation-item {
    background-color: transparent;
  }

  .project-item:hover,
  .conversation-item:hover {
    background-color: rgba(255, 255, 255, 0.06);
  }

  .project-name,
  .conv-title {
    color: var(--text-primary, #f9fafb);
  }

  .btn-expand:hover {
    background-color: var(--bg-tertiary, #4b5563);
  }

  .show-more:hover {
    background-color: var(--bg-tertiary, #374151);
  }

  .conversation-item.active {
    background-color: rgba(59, 130, 246, 0.15);
  }

  /* 深色模式下的连接状态背景颜色 */
  .conversation-item.status-connected {
    background-color: rgba(16, 185, 129, 0.15);
  }

  .conversation-item.status-connecting {
    background-color: rgba(99, 102, 241, 0.15);
  }

  .conversation-item.status-disconnected {
    /* 初始化状态不显示黄色背景 */
    background-color: transparent;
  }

  .conv-status-dot.connected {
    box-shadow: 0 0 0 2px rgba(96, 165, 250, 0.18);
  }

  .conv-status-dot.connecting {
    background-color: #64748b;
    box-shadow: 0 0 0 2px rgba(100, 116, 139, 0.22);
  }

  .conv-status-dot.streaming {
    border-color: rgba(96, 165, 250, 0.22);
    border-top-color: #60a5fa;
  }

  .conv-status-dot.completed {
    background-color: #4ade80;
    box-shadow: 0 0 0 2px rgba(74, 222, 128, 0.16);
  }

  .conv-status-dot.completed::after {
    border-color: rgba(74, 222, 128, 0.22);
  }

  .right-panel {
    background-color: var(--bg-primary, #111827);
  }

  .chat-main-column {
    background-color: transparent;
  }

  .chat-header {
    border-bottom-color: var(--border-color, #374151);
  }

  .session-search-bar {
    background: linear-gradient(180deg, rgba(17, 24, 39, 0.96), rgba(31, 41, 55, 0.96));
    border-bottom-color: var(--border-color, #374151);
  }

  .session-search-input-wrap,
  .session-search-btn {
    background: var(--bg-secondary, #111827);
    border-color: var(--border-color, #374151);
    color: var(--text-secondary, #d1d5db);
  }

  .session-search-input {
    color: var(--text-primary, #f9fafb);
  }

  .chat-title {
    color: var(--text-primary, #f9fafb);
  }

  .chat-project-name {
    color: var(--text-primary, #f9fafb);
    text-align: center;
  }

  .chat-session-id {
    color: var(--text-muted, #6b7280);
  }

  .chat-session-copy-btn {
    background: var(--bg-secondary, #1f2937);
    border-color: var(--border-color, #374151);
    color: var(--text-muted, #9ca3af);
  }

  .chat-session-copy-btn:hover {
    color: var(--text-primary, #f9fafb);
    border-color: var(--color-primary, #60a5fa);
  }

  /* 连接状态深色模式 */
  .connection-status.connected {
    background-color: rgba(16, 185, 129, 0.2);
    color: #6ee7b7;
  }

  .connection-status.connected .status-dot {
    background-color: #10b981;
  }

  .connection-status.disconnected {
    background-color: rgba(245, 158, 11, 0.2);
    color: #fcd34d;
  }

  .connection-status.disconnected .status-dot {
    background-color: #f59e0b;
  }

  .connection-status.connecting {
    background-color: rgba(99, 102, 241, 0.2);
    color: #a5b4fc;
  }

  .connection-status.connecting .status-dot {
    background-color: #6366f1;
  }

  .connection-status.connecting {
    background-color: rgba(99, 102, 241, 0.2);
    color: #a5b4fc;
  }

  .connection-status.connecting .status-dot {
    background-color: #6366f1;
  }

  .btn-refresh:hover {
    background-color: var(--bg-tertiary, #4b5563);
    color: #60a5fa;
  }

  .btn-refresh.refreshing {
    color: #60a5fa;
  }

  .message.assistant .message-content {
    background-color: var(--bg-secondary, #1f2937);
  }

  .message-text {
    color: var(--text-primary, #f9fafb);
  }

  .input-section {
    background-color: var(--bg-primary, #111827);
    border-top: none;
  }

  .input-container {
    background-color: var(--bg-primary, #0f172a);
    border-color: var(--border-color, #374151);
  }

  .message-input {
    color: var(--text-primary, #f9fafb);
  }
}
</style>
