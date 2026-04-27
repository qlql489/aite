<script setup lang="ts">
import { ref, computed, watch, nextTick, onMounted, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { useSlashesStore, type CommandItem } from '../../stores/slashes';
import { useClaudeStore } from '../../stores/claude';
import { useProviderStore } from '../../stores/provider';
import { useIDEStore } from '../../stores/ide';
import {
  PERMISSION_MODES,
  getPermissionModeDescription,
  getPermissionModeDisplayName,
} from '../../utils/permissionMode';
import { readImageAsBase64 } from '../../utils/image';
import type { ContentBlock, FileAttachmentInfo, GitInfo, OutgoingMessagePayload, PermissionMode, ThinkingLevel } from '../../types';
import type { TodoWritePanelState } from '../../utils/todoWrite';
import SlashCommandSelector from './SlashCommandSelector.vue';
import TokenIndicator from './TokenIndicator.vue';
import GitStatus from './GitStatus.vue';
import IDEStatus from './IDEStatus.vue';
import TodoWriteDock from './TodoWriteDock.vue';

interface ProviderModelOption {
  key: string;
  label: string;
  providerId: string;
  model: string;
  providerOverrideEnabled: boolean;
}

interface FileReference {
  id: string;
  name: string;
  path: string;  // 相对路径，如 src/node/rpc.rs
  originalPath: string;  // 保存后的绝对路径
  isDirectory?: boolean;
}

interface ProjectFileCandidate {
  name: string;
  path: string;
  isDirectory: boolean;
}

interface Props {
  disabled?: boolean;
  streaming?: boolean;
  sessionId?: string;
  gitInfo?: GitInfo | null;
  projectPath?: string;
  rewindDraft?: string;
  rewindDraftVersion?: number;
  todoState?: TodoWritePanelState | null;
}

const props = withDefaults(defineProps<Props>(), {
  disabled: false,
  streaming: false,
  sessionId: '',
  gitInfo: null,
  projectPath: '',
  rewindDraft: '',
  rewindDraftVersion: 0,
  todoState: null,
});

const emit = defineEmits<{
  send: [payload: OutgoingMessagePayload];
  stop: [];
  gitUpdated: [gitInfo: GitInfo | null];
  sessionConfigUpdated: [payload: { sessionId: string; providerId: string | null; model: string | null; providerOverrideEnabled: boolean }];
  executeCommand: [commandName: string];
}>();

// 文件引用列表（不包含图片）
const fileReferences = ref<FileReference[]>([]);
// 图片附件（单独处理）
const imageAttachments = ref<Array<{
  id: string;
  name: string;
  path: string;
  originalPath: string;
  preview: string;
  mediaType: string;
  data: string;
}>>([]);
const previewImage = ref<{ name: string; src: string } | null>(null);
const isDragging = ref(false);
const fileInput = ref<HTMLInputElement>();
const editorRef = ref<HTMLDivElement>();
const showAttachmentMenu = ref(false);
const attachmentMenuRef = ref<HTMLElement | null>(null);

// 斜杠命令相关
const slashesStore = useSlashesStore();
const claudeStore = useClaudeStore();
const providerStore = useProviderStore();
const ideStore = useIDEStore();
const showSlashMenu = ref(false);
const editorText = ref('');
const selectedCommandChip = ref<string | null>(null);
const slashMenuRef = ref<InstanceType<typeof SlashCommandSelector>>();
const sessionDrafts = ref<Map<string, string>>(new Map());
const cliInitialized = ref(false);
const isLoadingCommands = ref(false);
const immediateCommandNames = new Set(['clear', 'compact', 'init', 'rewind']);

// @ 文件引用选择
const showFileMenu = ref(false);
const fileMenuRef = ref<HTMLElement | null>(null);
const fileSearchQuery = ref('');
const selectedFileMenuIndex = ref(0);
const projectFiles = ref<ProjectFileCandidate[]>([]);
const isLoadingProjectFiles = ref(false);
let fileSearchDebounceTimer: number | null = null;
let activeFileSearchRequestId = 0;
let pendingFileInsertRange: Range | null = null;
let adjustHeightFrame: number | null = null;

// 权限模式相关
const showPermissionMenu = ref(false);
const permissionMenuRef = ref<HTMLElement | null>(null);
const showThinkingMenu = ref(false);
const thinkingMenuRef = ref<HTMLElement | null>(null);
const isRestartingThinking = ref(false);
const isRestartingProvider = ref(false);

const THINKING_LEVELS: Array<{ id: ThinkingLevel; label: string }> = [
  { id: 'off', label: '关闭思考' },
  { id: 'low', label: '浅思考' },
  { id: 'medium', label: '中思考' },
  { id: 'high', label: '深思考' },
];

// 当前权限模式
const currentPermissionMode = computed(() => {
  return claudeStore.currentSession?.permissionMode || 'default';
});

const currentProjectRoot = computed(() => {
  return claudeStore.currentSession?.cwd || props.projectPath || '';
});

const currentSessionProvider = computed(() => {
  return providerStore.resolveSessionProvider(
    claudeStore.currentSession?.providerId,
    claudeStore.currentSession?.providerOverrideEnabled,
  );
});

const currentSessionModel = computed(() => {
  return providerStore.resolveSessionModel(
    claudeStore.currentSession?.providerId,
    claudeStore.currentSession?.model,
    claudeStore.currentSession?.providerOverrideEnabled,
  );
});

// 权限模式列表
const permissionModes = computed(() => PERMISSION_MODES);
const getPermissionModeToneClass = (mode: PermissionMode) => `permission-tone-${mode}`;
const currentThinkingLevel = computed<ThinkingLevel>(() => {
  return claudeStore.currentSession?.thinkingLevel || claudeStore.thinkingLevel;
});

const currentThinkingLabel = computed(() => {
  return THINKING_LEVELS.find((item) => item.id === currentThinkingLevel.value)?.label || '中思考';
});
const formatPermissionModeLabel = (mode: PermissionMode) => `${getPermissionModeDisplayName(mode)} (${mode})`;
const currentPermissionModeLabel = computed(() => getPermissionModeDisplayName(currentPermissionMode.value));

const estimateTextUnits = (value: string) => {
  return Array.from(value).reduce((total, char) => {
    const code = char.charCodeAt(0);

    if (code > 255) return total + 1.85;
    if (/[WM@%]/.test(char)) return total + 1.2;
    if (/[ilI1]/.test(char)) return total + 0.6;
    if (/[\s._-]/.test(char)) return total + 0.55;
    return total + 1;
  }, 0);
};

const buildFixedWidth = (values: string[], options?: { min?: number; max?: number; padding?: number }) => {
  const { min = 0, max = 24, padding = 0 } = options || {};
  const longest = values.reduce((currentMax, value) => Math.max(currentMax, estimateTextUnits(value)), 0);
  const width = Math.min(max, Math.max(min, longest + padding));
  return `${width}ch`;
};

const permissionModeTextWidth = computed(() => {
  return buildFixedWidth(
    permissionModes.value.map((mode) => getPermissionModeDisplayName(mode)),
    { min: 8, max: 14, padding: 1.2 },
  );
});

const thinkingLevelTextWidth = computed(() => {
  return buildFixedWidth(
    [...THINKING_LEVELS.map((item) => item.label), '重启中...'],
    { min: 6, max: 10, padding: 0.6 },
  );
});

const providerModelOptions = computed<ProviderModelOption[]>(() => {
  const defaultProviderId = providerStore.activeProviderId;

  return providerStore.providers.flatMap((provider) => {
    return provider.models
      .filter((model) => model.model.trim())
      .map((model) => ({
        key: `${provider.id}::${model.model}`,
        label: `${provider.name} - ${model.modelName || model.model}`,
        providerId: provider.id,
        model: model.model,
        providerOverrideEnabled: provider.id !== defaultProviderId,
      }));
  });
});

const providerModelSelectWidth = computed(() => {
  return buildFixedWidth(
    ['选择供应商 - 模型', ...providerModelOptions.value.map((option) => option.label)],
    { min: 16, max: 30, padding: 1.8 },
  );
});

const currentProviderModelValue = computed(() => {
  const provider = currentSessionProvider.value;
  const model = currentSessionModel.value;
  if (!provider?.id || !model) return '';
  return `${provider.id}::${model}`;
});


const isEditorEmpty = computed(() => !editorText.value.trim());
const sendButtonDisabled = computed(() => {
  return !props.streaming && (
    props.disabled
    || (isEditorEmpty.value && fileReferences.value.length === 0 && imageAttachments.value.length === 0)
  );
});

const buildSessionConfigPayload = (providerId: string | null, model: string | null, providerOverrideEnabled: boolean) => ({
  sessionId: claudeStore.currentSession?.sessionId || '',
  providerId,
  model,
  providerOverrideEnabled,
});

const restartSessionWithCurrentConfig = async (overrides?: { providerId?: string | null; model?: string | null; providerOverrideEnabled?: boolean; thinkingLevel?: ThinkingLevel }) => {
  const session = claudeStore.currentSession;
  if (!session?.sessionId || !session.cwd || props.streaming) return;

  const providerId = overrides?.providerId ?? session.providerId ?? null;
  const providerOverrideEnabled = overrides?.providerOverrideEnabled ?? !!session.providerOverrideEnabled;
  const resolvedProvider = providerStore.resolveSessionProvider(providerId, providerOverrideEnabled);
  const model = overrides?.model ?? providerStore.resolveSessionModel(providerId, session.model, providerOverrideEnabled);
  const providerEnv = resolvedProvider ? providerStore.buildSessionProviderEnv(resolvedProvider.id) : null;
  const thinkingLevel = overrides?.thinkingLevel ?? session.thinkingLevel ?? claudeStore.thinkingLevel;

  claudeStore.updateSession(session.sessionId, {
    providerId,
    providerOverrideEnabled,
    model,
    thinkingLevel,
  });

  emit('sessionConfigUpdated', buildSessionConfigPayload(providerId, model, providerOverrideEnabled));

  claudeStore.setConnectionStatus(session.sessionId, 'connecting');
  claudeStore.setCliConnected(session.sessionId, false);
  claudeStore.setSessionStatus(session.sessionId, 'idle');

  await invoke('close_session', { sessionId: session.sessionId });
  await invoke('create_session', {
    sessionId: session.sessionId,
    projectPath: session.cwd,
    thinkingLevel,
    providerId: resolvedProvider?.id || null,
    model,
    providerEnv,
  });

  if (session.permissionMode && session.permissionMode !== 'default') {
    // 临时注释：重建 session 后先不自动下发 set_permission_mode
    // await invoke('set_permission_mode', {
    //   sessionId: session.sessionId,
    //   mode: session.permissionMode,
    // });
    console.log('[PermissionMode] Auto sync disabled after session restart:', {
      sessionId: session.sessionId,
      mode: session.permissionMode,
    });
  }
};

const toggleThinkingMenu = () => {
  showThinkingMenu.value = !showThinkingMenu.value;
  if (showThinkingMenu.value) {
    showPermissionMenu.value = false;
  }
};

const setThinkingLevel = async (level: ThinkingLevel, event?: Event) => {
  if (event) {
    event.stopPropagation();
    event.preventDefault();
  }

  showThinkingMenu.value = false;

  if (level === currentThinkingLevel.value) {
    return;
  }

  claudeStore.setThinkingLevel(level);

  const session = claudeStore.currentSession;
  if (!session?.sessionId || !session.cwd || props.streaming || isRestartingThinking.value) {
    if (session?.sessionId) {
      claudeStore.updateSession(session.sessionId, { thinkingLevel: level });
    }
    return;
  }

  isRestartingThinking.value = true;

  try {
    await restartSessionWithCurrentConfig({ thinkingLevel: level });
  } catch (error) {
    console.error('[ThinkingLevel] Failed to restart session:', error);
    claudeStore.setConnectionStatus(session.sessionId, 'disconnected');
  } finally {
    isRestartingThinking.value = false;
  }
};

const setSessionProviderModel = async (selectionKey: string | null) => {
  const session = claudeStore.currentSession;
  if (!session?.sessionId || !selectionKey || isRestartingProvider.value) return;

  const selectedOption = providerModelOptions.value.find((option) => option.key === selectionKey);
  if (!selectedOption) return;

  const providerId = selectedOption.providerOverrideEnabled ? selectedOption.providerId : null;

  isRestartingProvider.value = true;
  try {
    await restartSessionWithCurrentConfig({
      providerId,
      providerOverrideEnabled: selectedOption.providerOverrideEnabled,
      model: selectedOption.model,
    });
  } catch (error) {
    console.error('[Provider] Failed to switch provider-model:', error);
    claudeStore.setConnectionStatus(session.sessionId, 'disconnected');
  } finally {
    isRestartingProvider.value = false;
  }
};

// 切换权限模式菜单显示
const togglePermissionMenu = () => {
  showPermissionMenu.value = !showPermissionMenu.value;
};

// 循环切换权限模式
const cyclePermissionMode = async () => {
  const modes = permissionModes.value;
  const currentMode = currentPermissionMode.value;
  const currentIndex = modes.indexOf(currentMode);
  const sessionId = claudeStore.currentSession?.sessionId;

  if (currentIndex === -1 || !sessionId) return;

  const nextIndex = (currentIndex + 1) % modes.length;
  const nextMode = modes[nextIndex];

  try {
    claudeStore.updateSession(sessionId, { permissionMode: nextMode });
    await invoke('set_permission_mode', { sessionId, mode: nextMode });
  } catch (error) {
    console.error('[PermissionMode] Failed to cycle mode:', error);
    claudeStore.updateSession(sessionId, { permissionMode: currentMode });
  }
};

// 设置权限模式
const setPermissionMode = async (mode: PermissionMode, event?: Event) => {
  if (event) {
    event.stopPropagation();
    event.preventDefault();
  }

  showPermissionMenu.value = false;
  const sessionId = claudeStore.currentSession?.sessionId;
  if (!sessionId) return;

  try {
    await invoke('set_permission_mode', { sessionId, mode });
    claudeStore.updateSession(sessionId, { permissionMode: mode });
  } catch (error) {
    console.error('[PermissionMode] Failed to set mode:', error);
  }
};

// 从后端加载命令
const loadCommandsFromBackend = async () => {
  if (isLoadingCommands.value) return;

  try {
    isLoadingCommands.value = true;
    const currentSession = claudeStore.currentSession;
    const sessionId = currentSession?.sessionId;
    if (!sessionId) return;

    const [commands] = await invoke<[CommandInfo[] | null, string[] | null]>('get_commands_and_skills', {
      sessionId,
    });

    if (claudeStore.currentSession?.sessionId !== sessionId) return;

    claudeStore.updateSession(sessionId, {
      commands: commands || undefined,
    });

    await nextTick();
    checkCliInitialized();
  } catch (e) {
    console.error('[SlashCommands] Failed to load commands:', e);
  } finally {
    isLoadingCommands.value = false;
  }
};

interface CommandInfo {
  name: string;
  description: string;
  argumentHint?: string | string[];
}

const checkCliInitialized = () => {
  const currentSession = claudeStore.currentSession;
  const hasCommands = currentSession?.commands && currentSession.commands.length > 0;
  const hasSkills = currentSession?.skills && currentSession.skills.length > 0;
  cliInitialized.value = !!(hasCommands || hasSkills);
};

const allCommands = computed(() => slashesStore.allCommands);

const getImmediateCommandName = (text: string): string | null => {
  const match = text.trim().match(/^\/([^\s]+)$/);
  if (!match) return null;

  const commandName = match[1];
  return immediateCommandNames.has(commandName) ? commandName : null;
};

const escapeXml = (value: string) => value
  .replace(/&/g, '&amp;')
  .replace(/</g, '&lt;')
  .replace(/>/g, '&gt;')
  .replace(/"/g, '&quot;')
  .replace(/'/g, '&apos;');

const buildFileReferenceTransportText = (references: FileReference[]) => {
  if (references.length === 0) return null;

  const lines = ['<project-file-references>'];

  references.forEach((reference) => {
    const type = reference.isDirectory ? 'directory' : 'file';
    const displayName = getDisplayFileName(reference.path, reference.name) || reference.name;
    lines.push(
      `  <project-file path="${escapeXml(reference.path)}" type="${type}" display-name="${escapeXml(displayName)}" />`,
    );
  });

  lines.push('</project-file-references>');
  return lines.join('\n');
};

const buildSlashCommandTransportPayload = (
  content: string,
  contentBlocks?: ContentBlock[],
): Pick<OutgoingMessagePayload, 'transportContent' | 'transportContentBlocks'> | null => {
  const normalizedContent = content
    .replace(/\u00A0/g, ' ')
    .replace(/\r/g, '')
    .trim();

  const match = normalizedContent.match(/^\/([^\s]+)(?:\s+([\s\S]*))?$/);
  if (!match) return null;

  const [, commandName, commandArgs = ''] = match;
  const command = findNonImmediateCommand(commandName);
  if (!command) return null;

  const transportContent = [
    `<command-message>${escapeXml(commandName)}</command-message>`,
    `<command-name>${escapeXml(`/${commandName}`)}</command-name>`,
    `<command-args>${escapeXml(commandArgs)}</command-args>`,
  ].join('\n');

  const imageBlocks = (contentBlocks || []).filter((block) => block.type === 'image');
  const transportContentBlocks = [
    { type: 'text', text: transportContent } as ContentBlock,
    ...imageBlocks,
  ];

  return {
    transportContent,
    transportContentBlocks,
  };
};

const MAX_IDE_SELECTION_CHARS = 12000;

const resolveIDEFileReference = (filePath: string) => {
  const normalizedFilePath = filePath.replace(/\\/g, '/');
  const normalizedProjectRoot = currentProjectRoot.value.replace(/\\/g, '/').replace(/\/+$/, '');

  if (!normalizedProjectRoot) {
    return normalizedFilePath;
  }

  if (normalizedFilePath === normalizedProjectRoot) {
    return normalizedFilePath.split('/').pop() || normalizedFilePath;
  }

  if (normalizedFilePath.startsWith(`${normalizedProjectRoot}/`)) {
    return normalizedFilePath.slice(normalizedProjectRoot.length + 1);
  }

  return normalizedFilePath;
};

const buildIDEContextText = (): string | null => {
  if (ideStore.connectionState !== 'connected') return null;
  if (!ideStore.includeSelectionInContext) return null;

  const currentSelection = ideStore.selection;
  if (!currentSelection) return null;

  const lines: string[] = ['[IDE 上下文]'];
  if (ideStore.connectedIde?.name) {
    lines.push(`IDE: ${ideStore.connectedIde.name}`);
  }

  if (currentSelection.filePath) {
    lines.push(`文件: ${currentSelection.filePath}`);
  }

  if (currentSelection.startLine && currentSelection.endLine) {
    const lineLabel = currentSelection.startLine === currentSelection.endLine
      ? `L${currentSelection.startLine}`
      : `L${currentSelection.startLine}-L${currentSelection.endLine}`;
    lines.push(`选区: ${lineLabel}`);
  } else if (currentSelection.lineCount) {
    lines.push(`选中行数: ${currentSelection.lineCount}`);
  }

  if (currentSelection.text?.trim()) {
    const truncatedText = currentSelection.text.length > MAX_IDE_SELECTION_CHARS
      ? `${currentSelection.text.slice(0, MAX_IDE_SELECTION_CHARS)}\n…[IDE 选区内容已截断]`
      : currentSelection.text;
    lines.push('', '```');
    lines.push(truncatedText);
    lines.push('```');
  } else if (currentSelection.filePath) {
    lines.push('', `相关文件: @${resolveIDEFileReference(currentSelection.filePath)}`);
  }

  return lines.join('\n');
};

const applyHiddenTransportText = (
  payload: OutgoingMessagePayload,
  hiddenText: string,
): void => {
  const baseTransportContent = payload.transportContent ?? payload.content;
  const baseTransportBlocks = payload.transportContentBlocks ?? payload.contentBlocks ?? [];

  payload.transportContent = baseTransportContent.trim()
    ? `${baseTransportContent}\n\n${hiddenText}`
    : hiddenText;
  payload.transportContentBlocks = [
    ...baseTransportBlocks,
    { type: 'text', text: hiddenText } as ContentBlock,
  ];
};

const getNormalizedEditorText = () => {
  return getPlainText()
    .replace(/\u00A0/g, ' ')
    .replace(/\r/g, '')
    .replace(/\n+/g, '\n')
    .trim();
};

const findNonImmediateCommand = (commandName: string) => {
  return allCommands.value.find((command) => command.name === commandName && !command.immediate) || null;
};

const parseDraftCommandChip = (text: string) => {
  const normalizedText = text
    .replace(/\u00A0/g, ' ')
    .replace(/\r/g, '')
    .trim();

  if (!normalizedText.startsWith('/')) return null;

  const match = normalizedText.match(/^\/([^\s]+)(?:\s+(.*))?$/s);
  if (!match) return null;

  const [, commandName, trailingText = ''] = match;
  const command = findNonImmediateCommand(commandName);
  if (!command) return null;

  return {
    commandName,
    trailingText,
  };
};

const updateCurrentSessionDraft = (text: string, targetSessionId?: string) => {
  const sessionId = (targetSessionId ?? props.sessionId)?.trim();
  if (!sessionId) return;

  const nextDrafts = new Map(sessionDrafts.value);
  if (text) {
    nextDrafts.set(sessionId, text);
  } else {
    nextDrafts.delete(sessionId);
  }
  sessionDrafts.value = nextDrafts;
};

const restoreSessionDraft = async (sessionId?: string) => {
  if (!editorRef.value) return;

  const draft = sessionId ? (sessionDrafts.value.get(sessionId) || '') : '';
  const parsedCommandChip = parseDraftCommandChip(draft);

  if (parsedCommandChip) {
    editorRef.value.innerHTML = '';
    addCommandChipToEditor(parsedCommandChip.commandName, {
      focusEditor: false,
      persistDraft: false,
    });

    if (parsedCommandChip.trailingText) {
      editorRef.value.appendChild(document.createTextNode(parsedCommandChip.trailingText));
    }

    editorText.value = getNormalizedEditorText();
  } else {
    editorRef.value.innerText = draft;
    editorText.value = draft;
    selectedCommandChip.value = null;
  }

  showSlashMenu.value = false;
  showFileMenu.value = false;

  await nextTick();
  queueAdjustHeight();
};

const currentCommand = computed(() => {
  const match = editorText.value.match(/^\/(\S*)$/);
  return match ? match[1] : '';
});

const slashQuery = computed(() => currentCommand.value);

const shouldShowSlashMenu = computed(() => {
  return !selectedCommandChip.value &&
    editorText.value.startsWith('/') &&
    /^\/\S*$/.test(editorText.value) &&
    allCommands.value.length > 0;
});

const handleSessionBindingChange = async (newSessionId?: string, oldSessionId?: string) => {
  if (oldSessionId && oldSessionId !== newSessionId) {
    updateCurrentSessionDraft(editorText.value, oldSessionId);
  }

  fileReferences.value = [];
  imageAttachments.value = [];
  selectedCommandChip.value = null;
  pendingFileInsertRange = null;

  await nextTick();
  await restoreSessionDraft(newSessionId);
};

watch(() => claudeStore.currentSession, () => {
  checkCliInitialized();
}, { immediate: true });

watch(shouldShowSlashMenu, (nextVisible) => {
  if (nextVisible && !showSlashMenu.value) {
    showSlashMenu.value = true;
    return;
  }

  if (!nextVisible && showSlashMenu.value) {
    showSlashMenu.value = false;
  }
});

watch(() => props.sessionId, (newSessionId, oldSessionId) => {
  void handleSessionBindingChange(newSessionId, oldSessionId);
}, { immediate: true });

watch(showSlashMenu, (isOpen) => {
  if (isOpen) {
    void nextTick(() => {
      editorRef.value?.focus();
    });
  }
});

const getDisplayFileName = (path: string, fallback?: string) => {
  const normalizedPath = (path || '').replace(/\\/g, '/').replace(/\/+$/, '');
  if (!normalizedPath) return fallback || path;

  const segments = normalizedPath.split('/').filter(Boolean);
  return segments[segments.length - 1] || fallback || path;
};

watch(fileSearchQuery, () => {
  selectedFileMenuIndex.value = 0;

  if (!showFileMenu.value) return;
  if (fileMenuRef.value) {
    fileMenuRef.value.scrollTop = 0;
  }
  queueProjectFileSearch();
});

watch(projectFiles, (files) => {
  if (selectedFileMenuIndex.value >= files.length) {
    selectedFileMenuIndex.value = Math.max(0, files.length - 1);
  }
});

const scrollSelectedFileMenuItemIntoView = () => {
  if (!showFileMenu.value) return;

  void nextTick(() => {
    const selected = fileMenuRef.value?.querySelector(`[data-file-index="${selectedFileMenuIndex.value}"]`) as HTMLElement | null;
    selected?.scrollIntoView({ block: 'nearest' });
  });
};

watch(selectedFileMenuIndex, () => {
  scrollSelectedFileMenuItemIntoView();
});

watch(showFileMenu, (isOpen) => {
  if (isOpen) {
    scrollSelectedFileMenuItemIntoView();
  }
});

watch(() => currentProjectRoot.value, () => {
  activeFileSearchRequestId += 1;
  if (fileSearchDebounceTimer !== null) {
    window.clearTimeout(fileSearchDebounceTimer);
    fileSearchDebounceTimer = null;
  }
  projectFiles.value = [];
  showFileMenu.value = false;
  fileSearchQuery.value = '';
  selectedFileMenuIndex.value = 0;
  pendingFileInsertRange = null;
});

const deleteTextAfterRange = (range: Range, count: number) => {
  let remaining = count;
  let currentNode: Node | null = range.startContainer;
  let currentOffset = range.startOffset;

  while (currentNode && remaining > 0) {
    if (currentNode.nodeType === Node.TEXT_NODE) {
      const textNode = currentNode as Text;
      const available = textNode.data.length - currentOffset;
      const removable = Math.min(available, remaining);
      if (removable > 0) {
        textNode.deleteData(currentOffset, removable);
        remaining -= removable;
      }

      if (remaining === 0) break;
      currentOffset = 0;
    }

    const nextNode = getNextTextNode(currentNode, editorRef.value);
    if (!nextNode) break;
    currentNode = nextNode;
    currentOffset = 0;
  }
};

const deleteTextBeforeRange = (range: Range, count: number) => {
  let remaining = count;
  let currentNode: Node | null = range.startContainer;
  let currentOffset = range.startOffset;

  while (currentNode && remaining > 0) {
    if (currentNode.nodeType === Node.TEXT_NODE) {
      const textNode = currentNode as Text;
      const removable = Math.min(currentOffset, remaining);
      if (removable > 0) {
        textNode.deleteData(currentOffset - removable, removable);
        currentOffset -= removable;
        remaining -= removable;
      }

      if (remaining === 0) break;
    }

    const previousNode = getPreviousTextNode(currentNode, editorRef.value);
    if (!previousNode) break;
    currentNode = previousNode;
    currentOffset = previousNode.data.length;
  }
};

const getNextTextNode = (node: Node, root?: HTMLElement) => {
  let current: Node | null = node;

  while (current && current !== root) {
    if (current.firstChild) {
      current = current.firstChild;
    } else if (current.nextSibling) {
      current = current.nextSibling;
    } else {
      while (current && current !== root && !current.nextSibling) {
        current = current.parentNode;
      }
      current = current && current !== root ? current.nextSibling : null;
    }

    if (current?.nodeType === Node.TEXT_NODE) {
      return current as Text;
    }
  }

  return null;
};

const getPreviousTextNode = (node: Node, root?: HTMLElement) => {
  let current: Node | null = node;

  while (current && current !== root) {
    if (current.previousSibling) {
      current = current.previousSibling;
      while (current?.lastChild) {
        current = current.lastChild;
      }
    } else {
      current = current.parentNode;
      continue;
    }

    if (current?.nodeType === Node.TEXT_NODE) {
      return current as Text;
    }
  }

  return null;
};

const readTextAfterRange = (range: Range, count: number) => {
  if (count <= 0) return '';

  let remaining = count;
  let currentNode: Node | null = range.startContainer;
  let currentOffset = range.startOffset;
  let result = '';

  while (currentNode && remaining > 0) {
    if (currentNode.nodeType === Node.TEXT_NODE) {
      const textNode = currentNode as Text;
      const available = textNode.data.slice(currentOffset, currentOffset + remaining);
      result += available;
      remaining -= available.length;
      currentOffset = 0;
    }

    if (remaining === 0) break;

    const nextNode = getNextTextNode(currentNode, editorRef.value);
    if (!nextNode) break;
    currentNode = nextNode;
    currentOffset = 0;
  }

  return result;
};

const moveRangeStartBackward = (range: Range, count: number) => {
  if (count <= 0) return true;

  let remaining = count;
  let currentNode: Node | null = range.startContainer;
  let currentOffset = range.startOffset;

  while (currentNode && remaining > 0) {
    if (currentNode.nodeType === Node.TEXT_NODE) {
      const textNode = currentNode as Text;
      const movable = Math.min(currentOffset, remaining);

      if (movable > 0) {
        currentOffset -= movable;
        remaining -= movable;
        range.setStart(textNode, currentOffset);
      }

      if (remaining === 0) {
        return true;
      }
    }

    const previousNode = getPreviousTextNode(currentNode, editorRef.value);
    if (!previousNode) {
      return false;
    }

    currentNode = previousNode;
    currentOffset = previousNode.data.length;
    range.setStart(previousNode, currentOffset);
  }

  return remaining === 0;
};

const getInlineFileTriggerAtSelection = () => {
  if (!editorRef.value) return null;

  const selection = window.getSelection();
  if (!selection?.rangeCount) return null;

  const caretRange = selection.getRangeAt(0);
  if (!caretRange.collapsed || !editorRef.value.contains(caretRange.startContainer)) {
    return null;
  }

  const textBeforeCaret = readTextBeforeRange(caretRange.cloneRange(), 512);
  const match = textBeforeCaret.match(/(^|[\s\u00A0([{'"`:,;，。！？、])@([^\s\u00A0@]*)$/);
  if (!match) {
    return null;
  }

  const query = match[2] || '';
  const triggerRange = caretRange.cloneRange();
  const moved = moveRangeStartBackward(triggerRange, query.length + 1);
  if (!moved) {
    return null;
  }

  triggerRange.collapse(true);

  return {
    query,
    range: triggerRange,
  };
};

const readInlineFileSearchQuery = () => {
  if (!editorRef.value) return null;

  if (!pendingFileInsertRange) {
    const inlineTrigger = getInlineFileTriggerAtSelection();
    if (!inlineTrigger) {
      return null;
    }
    pendingFileInsertRange = inlineTrigger.range.cloneRange();
    return inlineTrigger.query;
  }

  const triggerText = readTextAfterRange(pendingFileInsertRange.cloneRange(), 512);
  if (!triggerText.startsWith('@')) {
    return null;
  }

  const match = triggerText.match(/^@([^\s\u00A0]*)/);
  if (!match) {
    return null;
  }

  return match[1];
};

const syncFileMenuQueryFromEditor = () => {
  if (!showFileMenu.value) return;

  const nextQuery = readInlineFileSearchQuery();
  if (nextQuery === null) {
    closeFileMenu(false);
    return;
  }

  if (nextQuery !== fileSearchQuery.value) {
    fileSearchQuery.value = nextQuery;
  }
};

const readTextBeforeRange = (range: Range, count: number) => {
  if (count <= 0) return '';

  let remaining = count;
  let currentNode: Node | null = range.startContainer;
  let currentOffset = range.startOffset;
  let result = '';

  while (currentNode && remaining > 0) {
    if (currentNode.nodeType === Node.TEXT_NODE) {
      const textNode = currentNode as Text;
      const start = Math.max(0, currentOffset - remaining);
      const chunk = textNode.data.slice(start, currentOffset);
      result = chunk + result;
      remaining -= chunk.length;
      currentOffset = 0;
    }

    if (remaining === 0) break;

    const previousNode = getPreviousTextNode(currentNode, editorRef.value);
    if (!previousNode) break;
    currentNode = previousNode;
    currentOffset = previousNode.data.length;
  }

  return result;
};

const shouldOpenInlineFileMenuFromTypingTrigger = () => {
  if (!editorRef.value) return false;

  const selection = window.getSelection();
  if (!selection?.rangeCount) return false;

  const range = selection.getRangeAt(0);
  if (!range.collapsed || !editorRef.value.contains(range.startContainer)) {
    return false;
  }

  const previousChar = readTextBeforeRange(range.cloneRange(), 1);
  if (!previousChar) {
    return true;
  }

  return /[\s\u00A0([{'"`:,;，。！？、]/.test(previousChar);
};

const removeTypedFileTrigger = (query: string) => {
  if (!pendingFileInsertRange || !editorRef.value) return pendingFileInsertRange;

  const range = pendingFileInsertRange.cloneRange();
  const afterWithTrigger = readTextAfterRange(range, query.length + 1);
  const afterQueryOnly = query ? readTextAfterRange(range, query.length) : '';
  const beforeTrigger = readTextBeforeRange(range, 1);
  const afterTrigger = readTextAfterRange(range, 1);

  if (afterWithTrigger === `@${query}`) {
    deleteTextAfterRange(range, query.length + 1);
  } else {
    if (query && afterQueryOnly === query) {
      deleteTextAfterRange(range, query.length);
    }

    if (beforeTrigger === '@') {
      deleteTextBeforeRange(range, 1);
    } else if (afterTrigger === '@') {
      deleteTextAfterRange(range, 1);
    }
  }

  const normalizedRange = document.createRange();
  normalizedRange.setStart(range.startContainer, range.startOffset);
  normalizedRange.collapse(true);

  const selection = window.getSelection();
  selection?.removeAllRanges();
  selection?.addRange(normalizedRange);

  return normalizedRange;
};

const restorePendingFileSelection = () => {
  if (!pendingFileInsertRange) return;
  const selection = window.getSelection();
  if (!selection) return;

  const range = pendingFileInsertRange.cloneRange();
  selection.removeAllRanges();
  selection.addRange(range);
};

const closeFileMenu = (restoreSelection = true) => {
  activeFileSearchRequestId += 1;
  if (fileSearchDebounceTimer !== null) {
    window.clearTimeout(fileSearchDebounceTimer);
    fileSearchDebounceTimer = null;
  }
  showFileMenu.value = false;
  fileSearchQuery.value = '';
  selectedFileMenuIndex.value = 0;
  projectFiles.value = [];
  isLoadingProjectFiles.value = false;

  if (restoreSelection) {
    nextTick(() => {
      editorRef.value?.focus();
      restorePendingFileSelection();
    });
  } else {
    pendingFileInsertRange = null;
  }
};

const loadProjectFiles = async () => {
  const rootPath = currentProjectRoot.value;
  const query = fileSearchQuery.value.trim();
  const requestId = ++activeFileSearchRequestId;

  if (!rootPath) {
    projectFiles.value = [];
    isLoadingProjectFiles.value = false;
    return;
  }

  if (!query) {
    projectFiles.value = [];
    isLoadingProjectFiles.value = false;
    return;
  }

  isLoadingProjectFiles.value = true;

  try {
    const response = await invoke<Array<{ name: string; path: string; is_dir: boolean }>>('search_project_files', {
      path: rootPath,
      query,
      maxResults: 200,
    });

    if (requestId !== activeFileSearchRequestId || query !== fileSearchQuery.value.trim() || rootPath !== currentProjectRoot.value) {
      return;
    }

    projectFiles.value = response.map((entry) => ({
      name: entry.name,
      path: entry.path,
      isDirectory: entry.is_dir,
    }));
  } catch (error) {
    if (requestId !== activeFileSearchRequestId) return;
    console.error('[FileSelector] Failed to search project files:', error);
    projectFiles.value = [];
  } finally {
    if (requestId === activeFileSearchRequestId) {
      isLoadingProjectFiles.value = false;
    }
  }
};

const queueProjectFileSearch = () => {
  if (fileSearchDebounceTimer !== null) {
    window.clearTimeout(fileSearchDebounceTimer);
  }

  fileSearchDebounceTimer = window.setTimeout(() => {
    fileSearchDebounceTimer = null;
    void loadProjectFiles();
  }, 120);
};

const openFileMenu = (options: { range?: Range | null; initialQuery?: string } = {}) => {
  if (!editorRef.value) return;

  if (options.range && editorRef.value.contains(options.range.startContainer)) {
    pendingFileInsertRange = options.range.cloneRange();
  } else {
    const selection = window.getSelection();
    if (selection?.rangeCount) {
      pendingFileInsertRange = selection.getRangeAt(0).cloneRange();
    } else {
      editorRef.value.focus();
      setCaretToEnd();
      const currentSelection = window.getSelection();
      pendingFileInsertRange = currentSelection?.rangeCount ? currentSelection.getRangeAt(0).cloneRange() : null;
    }
  }

  showSlashMenu.value = false;
  showFileMenu.value = true;
  projectFiles.value = [];
  selectedFileMenuIndex.value = 0;
  fileSearchQuery.value = options.initialQuery || '';
};

const buildProjectFileAbsolutePath = (relativePath: string) => {
  const root = currentProjectRoot.value;
  if (!root) return relativePath;
  return `${root.replace(/[\/]$/, '')}/${relativePath}`;
};

const getReferencePathFromAbsolutePath = (absolutePath: string) => {
  const cwd = claudeStore.currentSession?.cwd;
  if (!cwd) return absolutePath;

  const normalizedCwd = cwd.replace(/\\/g, '/').replace(/\/+$/, '');
  const normalizedPath = absolutePath.replace(/\\/g, '/');

  if (normalizedPath === normalizedCwd) {
    return normalizedPath.split('/').pop() || normalizedPath;
  }

  if (normalizedPath.startsWith(`${normalizedCwd}/`)) {
    return normalizedPath.slice(normalizedCwd.length + 1);
  }

  return absolutePath;
};

const extractPlainTextWithLineBreaks = (node: Node, options: { isRoot?: boolean } = {}) => {
  if (node.nodeType === Node.TEXT_NODE) {
    return node.textContent || '';
  }

  if (!(node instanceof HTMLElement)) {
    return '';
  }

  if (node.hasAttribute('data-file-ref')) {
    const fileRefId = node.getAttribute('data-file-ref');
    const fileRef = fileReferences.value.find((item) => item.id === fileRefId);
    return fileRef ? `@${fileRef.path}` : '';
  }

  if (node.tagName === 'BR') {
    return '\n';
  }

  const isRoot = options.isRoot ?? false;
  const isBlockElement = !isRoot && ['DIV', 'P', 'LI', 'PRE', 'BLOCKQUOTE'].includes(node.tagName);

  let text = '';
  node.childNodes.forEach((child) => {
    text += extractPlainTextWithLineBreaks(child);
  });

  if (isBlockElement && text && !text.endsWith('\n')) {
    text += '\n';
  }

  return text;
};

const insertPlainTextAtCursor = (text: string) => {
  const normalizedText = text.replace(/\r\n/g, '\n');

  // 优先走原生编辑命令，确保粘贴进入浏览器撤销栈，Cmd/Ctrl+Z 可回退
  if (typeof document !== 'undefined' && typeof document.execCommand === 'function') {
    const html = normalizedText
      .split('\n')
      .map((line) => escapeXml(line))
      .join('<br>');

    if (document.execCommand('insertHTML', false, html)) {
      return true;
    }
  }

  const selection = window.getSelection();
  if (!selection?.rangeCount) return false;

  const range = selection.getRangeAt(0);
  range.deleteContents();

  const fragment = document.createDocumentFragment();
  const lines = normalizedText.split('\n');

  lines.forEach((line, index) => {
    if (line) {
      fragment.appendChild(document.createTextNode(line));
    }

    if (index < lines.length - 1) {
      fragment.appendChild(document.createElement('br'));
    }
  });

  const lastNode = fragment.lastChild;
  range.insertNode(fragment);

  const nextRange = document.createRange();
  if (lastNode) {
    nextRange.setStartAfter(lastNode);
  } else {
    nextRange.setStart(range.startContainer, range.startOffset);
  }
  nextRange.collapse(true);

  selection.removeAllRanges();
  selection.addRange(nextRange);

  return true;
};

const selectProjectFile = (file: ProjectFileCandidate) => {
  const id = `${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
  fileReferences.value.push({
    id,
    name: file.name,
    path: file.path,
    originalPath: buildProjectFileAbsolutePath(file.path),
    isDirectory: file.isDirectory,
  });

  const insertRange = removeTypedFileTrigger(fileSearchQuery.value);
  addFileReferenceToEditor(id, file.name, file.path, false, insertRange);
  closeFileMenu(false);
};

const handleFileMenuKeydown = (e: KeyboardEvent) => {
  const files = projectFiles.value;
  const count = files.length;

  switch (e.key) {
    case 'ArrowDown':
      e.preventDefault();
      if (count > 0) {
        selectedFileMenuIndex.value = (selectedFileMenuIndex.value + 1) % count;
      }
      return true;
    case 'ArrowUp':
      e.preventDefault();
      if (count > 0) {
        selectedFileMenuIndex.value = (selectedFileMenuIndex.value - 1 + count) % count;
      }
      return true;
    case 'Enter':
    case 'Tab':
      e.preventDefault();
      if (files[selectedFileMenuIndex.value]) {
        selectProjectFile(files[selectedFileMenuIndex.value]);
      } else {
        closeFileMenu();
      }
      return true;
    case 'Escape':
      e.preventDefault();
      closeFileMenu();
      return true;
    case 'Backspace':
      return false;
    default:
      if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) {
        if (/\s/.test(e.key)) {
          closeFileMenu(false);
          return false;
        }

        return false;
      }
      return false;
  }
};

// 发送消息
const send = () => {
  // 获取纯文本内容
  const text = getPlainText();
  const hasFiles = fileReferences.value.length > 0;
  const hasImages = imageAttachments.value.length > 0;

  if (!text.trim() && !hasFiles && !hasImages) return;
  if (props.disabled) return;

  const immediateCommandName = !hasFiles && !hasImages
    ? getImmediateCommandName(text)
    : null;

  if (immediateCommandName) {
    handleImmediateCommand(immediateCommandName);
    return;
  }

  // 构建发送内容：文本中已按编辑器顺序内联 @ 文件引用
  const content = text;

  const contentBlocks = [] as OutgoingMessagePayload['contentBlocks'];
  if (text.trim()) {
    contentBlocks?.push({
      type: 'text',
      text: text.trim(),
    });
  }

  if (hasImages) {
    imageAttachments.value.forEach((image) => {
      contentBlocks?.push({
        type: 'image',
        source: {
          type: 'base64',
          media_type: image.mediaType,
          data: image.data,
        },
      });
    });
  }

  const attachments: FileAttachmentInfo[] = [
    ...fileReferences.value.map((file) => ({
      name: file.name,
      path: file.path,
      isImage: false,
      originalPath: file.originalPath,
    })),
    ...imageAttachments.value.map((image) => ({
      name: image.name,
      path: image.path,
      isImage: true,
      preview: image.preview,
      originalPath: image.originalPath,
    })),
  ];

  const payload: OutgoingMessagePayload = {
    content,
    contentBlocks: contentBlocks && contentBlocks.length > 0 ? contentBlocks : undefined,
    attachments: attachments.length > 0 ? attachments : undefined,
  };

  const commandTransportPayload = buildSlashCommandTransportPayload(
    content,
    payload.contentBlocks,
  );

  if (commandTransportPayload) {
    payload.transportContent = commandTransportPayload.transportContent;
    payload.transportContentBlocks = commandTransportPayload.transportContentBlocks;
  }

  const fileReferenceTransportText = buildFileReferenceTransportText(fileReferences.value);
  if (fileReferenceTransportText) {
    applyHiddenTransportText(payload, fileReferenceTransportText);
  }

  const ideContextText = buildIDEContextText();
  if (ideContextText) {
    applyHiddenTransportText(payload, ideContextText);
  }

  // 如果正在 streaming，先停止
  if (props.streaming) {
    emit('stop');
    nextTick(() => {
      emit('send', payload);
    });
  } else {
    emit('send', payload);
  }

  // 清空输入
  clearEditor();
  pendingFileInsertRange = null;
};

// 获取纯文本内容（不包含文件引用块）
const getPlainText = () => {
  if (!editorRef.value) return '';

  const clone = editorRef.value.cloneNode(true) as HTMLDivElement;
  let text = extractPlainTextWithLineBreaks(clone, { isRoot: true });

  text = text.replace(/[\u200B-\u200D\uFEFF]/g, '');

  return text;
};

// 清空编辑器
const clearEditor = () => {
  if (editorRef.value) {
    editorRef.value.innerHTML = '';
  }
  editorText.value = '';
  selectedCommandChip.value = null;
  updateCurrentSessionDraft('');
  fileReferences.value = [];
  imageAttachments.value = [];
  showSlashMenu.value = false;
  showFileMenu.value = false;
  queueAdjustHeight();
};

const applyExternalDraft = async (draft: string) => {
  if (!editorRef.value) return;

  const parsedCommandChip = parseDraftCommandChip(draft || '');

  if (parsedCommandChip) {
    editorRef.value.innerHTML = '';
    addCommandChipToEditor(parsedCommandChip.commandName, {
      focusEditor: false,
      persistDraft: false,
    });

    if (parsedCommandChip.trailingText) {
      editorRef.value.appendChild(document.createTextNode(parsedCommandChip.trailingText));
    }

    editorText.value = getNormalizedEditorText();
  } else {
    editorRef.value.innerText = draft || '';
    editorText.value = draft || '';
    selectedCommandChip.value = null;
  }

  fileReferences.value = [];
  imageAttachments.value = [];
  updateCurrentSessionDraft(editorText.value);

  await nextTick();
  queueAdjustHeight();
  editorRef.value.focus();
  setCaretToEnd();
};

// 停止生成
const stop = () => {
  emit('stop');
};

// 处理键盘事件
const handlePermissionModeShortcut = (e: KeyboardEvent) => {
  if (props.disabled) return false;
  if (e.key !== 'Tab' || !e.shiftKey || e.ctrlKey || e.metaKey || e.altKey) return false;

  e.preventDefault();
  void cyclePermissionMode();
  return true;
};

const handleKeyDown = (e: KeyboardEvent) => {
  if (e.key === 'Escape' && props.streaming) {
    e.preventDefault();
    e.stopPropagation();
    stop();
    return;
  }

  if (showSlashMenu.value) {
    slashMenuRef.value?.handleKeydown(e);
    return;
  }

  if (showFileMenu.value) {
    const handled = handleFileMenuKeydown(e);
    if (handled) return;
  }

  if (!showFileMenu.value && !e.ctrlKey && !e.metaKey && !e.altKey && e.key === '@' && shouldOpenInlineFileMenuFromTypingTrigger()) {
    void openFileMenu();
    return;
  }

  if (e.key === 'Escape') {
    if (showThinkingMenu.value) {
      showThinkingMenu.value = false;
      return;
    }

    if (showPermissionMenu.value) {
      showPermissionMenu.value = false;
    } else {
      showSlashMenu.value = false;
      showFileMenu.value = false;
    }
    return;
  }

  if (e.ctrlKey || e.metaKey) {
    if (e.key === 'Enter') {
      e.preventDefault();
      send();
    }
    return;
  }

  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault();
    send();
    return;
  }

  // Backspace 删除文件引用
  if (e.key === 'Backspace') {
    const selection = window.getSelection();
    if (selection && selection.rangeCount > 0) {
      const range = selection.getRangeAt(0);
      const previousNode = getNodeBeforeCaret(range);
      if (previousNode instanceof HTMLElement && previousNode.getAttribute('data-command-chip')) {
        e.preventDefault();
        removeCommandChip();
        return;
      }
      if (previousNode instanceof HTMLElement && previousNode.getAttribute('data-file-ref')) {
        e.preventDefault();
        removeFileReference(previousNode.getAttribute('data-file-ref')!);
        return;
      }
    }
  }
};

// 选择斜杠命令
const handleImmediateCommand = (commandName: string) => {
  emit('executeCommand', commandName);
  clearEditor();
  pendingFileInsertRange = null;
  nextTick(() => {
    editorRef.value?.focus();
  });
};

const handleSelectCommand = (command: CommandItem) => {
  if (command.immediate) {
    handleImmediateCommand(command.name);
    return;
  }

  if (editorRef.value) {
    editorRef.value.innerHTML = '';
  }
  addCommandChipToEditor(command.name);
  showSlashMenu.value = false;
  nextTick(() => {
    editorRef.value?.focus();
    setCaretToEnd();
  });
};

// 设置光标到末尾
const setCaretToEnd = () => {
  if (!editorRef.value) return;
  const range = document.createRange();
  const selection = window.getSelection();
  range.selectNodeContents(editorRef.value);
  range.collapse(false);
  selection?.removeAllRanges();
  selection?.addRange(range);
};

// 触发文件选择
const triggerFileSelect = () => {
  showAttachmentMenu.value = false;
  fileInput.value?.click();
};

const toggleAttachmentMenu = () => {
  showAttachmentMenu.value = !showAttachmentMenu.value;
};

const addDirectoryReference = (absolutePath: string) => {
  const id = Math.random().toString(36).substring(7);
  const displayPath = getReferencePathFromAbsolutePath(absolutePath);
  const displayName = getDisplayFileName(displayPath);

  fileReferences.value.push({
    id,
    name: displayName,
    path: displayPath,
    originalPath: absolutePath,
    isDirectory: true,
  });

  addFileReferenceToEditor(id, displayName, displayPath, false);
};

const triggerFolderSelect = async () => {
  showAttachmentMenu.value = false;

  try {
    const selected = await open({
      directory: true,
      multiple: true,
      title: '选择文件夹',
    });

    if (!selected) return;

    const folderPaths = Array.isArray(selected) ? selected : [selected];
    for (const folderPath of folderPaths) {
      if (typeof folderPath === 'string') {
        addDirectoryReference(folderPath);
      }
    }
  } catch (error) {
    console.error('[Attachment] Failed to select folders:', error);
  }
};

// 处理文件选择
const handleFileSelect = async (e: Event) => {
  const target = e.target as HTMLInputElement;
  const files = target.files;
  if (!files) return;

  for (const file of Array.from(files)) {
    await addFile(file);
  }

  target.value = '';
};

// 计算相对路径
const getRelativePath = (absolutePath: string): string => {
  const cwd = claudeStore.currentSession?.cwd;
  if (!cwd) {
    // 如果没有 cwd，返回文件名
    return absolutePath.split(/[\\/]/).pop() || absolutePath;
  }

  // 移除 cwd 前缀
  if (absolutePath.startsWith(cwd)) {
    let relative = absolutePath.slice(cwd.length);
    if (relative.startsWith('/') || relative.startsWith('\\')) {
      relative = relative.slice(1);
    }
    return relative;
  }

  return absolutePath.split(/[\\/]/).pop() || absolutePath;
};

// 添加文件
const addFile = async (file: File) => {
  const id = Math.random().toString(36).substring(7);
  const isImage = file.type.startsWith('image/');
  const maxSize = 10 * 1024 * 1024;

  if (file.size > maxSize) {
    alert(`文件 ${file.name} 太大，最大支持 10MB`);
    return;
  }

  // 读取文件字节数据
  const readFileAsBytes = (file: File): Promise<Uint8Array> => {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => {
        resolve(new Uint8Array(reader.result as ArrayBuffer));
      };
      reader.onerror = () => reject(new Error('Failed to read file'));
      reader.readAsArrayBuffer(file);
    });
  };

  // 生成缩略图
  const generateThumbnail = (file: File): Promise<string | undefined> => {
    if (!file.type.startsWith('image/')) return Promise.resolve(undefined);

    return new Promise((resolve) => {
      const reader = new FileReader();
      reader.onload = () => {
        const img = new Image();
        img.onload = () => {
          const canvas = document.createElement('canvas');
          const maxSize = 64;
          const scale = Math.min(maxSize / img.width, maxSize / img.height, 1);
          canvas.width = img.width * scale;
          canvas.height = img.height * scale;
          const ctx = canvas.getContext('2d');
          if (ctx) {
            ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
            resolve(canvas.toDataURL('image/jpeg', 0.7));
          } else {
            resolve(undefined);
          }
        };
        img.onerror = () => resolve(undefined);
        img.src = reader.result as string;
      };
      reader.onerror = () => resolve(undefined);
      reader.readAsDataURL(file);
    });
  };

  const bytes = await readFileAsBytes(file);
  const cwd = claudeStore.currentSession?.cwd;
  let tempPath = '';

  try {
    tempPath = await invoke<string>('save_temp_file', {
      name: file.name,
      data: Array.from(bytes),
      cwd: cwd || null,
    });
  } catch (err) {
    console.error('Failed to save file:', file.name, err);
    alert(`保存文件 ${file.name} 失败`);
    return;
  }

  const relativePath = getRelativePath(tempPath);

  if (isImage) {
    const preview = await generateThumbnail(file);
    const data = await readImageAsBase64(file);
    imageAttachments.value.push({
      id,
      name: file.name,
      path: relativePath,
      originalPath: tempPath,
      preview: preview || '',
      mediaType: file.type || 'image/png',
      data,
    });
  } else {
    fileReferences.value.push({
      id,
      name: file.name,
      path: relativePath,
      originalPath: tempPath,
    });
    addFileReferenceToEditor(id, file.name, relativePath, false);
  }
};

// 在编辑器中添加文件引用块
const addFileReferenceToEditor = (id: string, name: string, path: string, isImage: boolean, insertRange?: Range | null) => {
  if (!editorRef.value) return;

  const displayName = getDisplayFileName(path, name);
  const fileReference = fileReferences.value.find((item) => item.id === id);
  const hoverTitle = fileReference?.originalPath || path;
  const isDirectory = !!fileReference?.isDirectory;
  const span = document.createElement('span');
  span.setAttribute('data-file-ref', id);
  span.className = 'file-reference-chip';
  span.contentEditable = 'false';
  span.title = hoverTitle;
  span.innerHTML = `
    ${isImage
      ? '<svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="2" y="2" width="12" height="12" rx="2"></rect><circle cx="5.5" cy="5.5" r="1" fill="currentColor"></circle><path d="M2 11l3-3 2 2 3-4 4 5"></path></svg>'
      : isDirectory
        ? '<svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M1.75 4.75a1.5 1.5 0 0 1 1.5-1.5h2.5l1.25 1.5h5a1.5 1.5 0 0 1 1.5 1.5v4.5a1.5 1.5 0 0 1-1.5 1.5h-8.75a1.5 1.5 0 0 1-1.5-1.5z"></path></svg>'
        : '<svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M4 2h5l4 4v8H4V2z"></path><path d="M9 2v4h4"></path></svg>'}
    <span class="file-path">${displayName}</span>
    <button type="button" class="remove-btn" data-id="${id}">×</button>
  `;

  const removeBtn = span.querySelector('.remove-btn');
  removeBtn?.addEventListener('click', (e) => {
    e.preventDefault();
    e.stopPropagation();
    removeFileReference(id);
  });

  const space = document.createTextNode(' ');

  if (insertRange && editorRef.value.contains(insertRange.startContainer)) {
    const range = insertRange.cloneRange();
    range.collapse(true);
    range.insertNode(space);
    range.insertNode(span);

    const selection = window.getSelection();
    const caretRange = document.createRange();
    caretRange.setStartAfter(space);
    caretRange.collapse(true);
    selection?.removeAllRanges();
    selection?.addRange(caretRange);
  } else {
    editorRef.value.appendChild(span);
    editorRef.value.appendChild(space);
    setCaretToEnd();
  }

  editorRef.value.focus();
  queueAdjustHeight();
};

const getNodeBeforeCaret = (range: Range): Node | null => {
  const root = editorRef.value;
  if (!root) return null;

  const resolvePreviousNonWhitespaceNode = (node: Node | null): Node | null => {
    let current = node;

    while (current) {
      if (current.nodeType === Node.TEXT_NODE) {
        const text = current.textContent?.replace(/\u00A0/g, ' ').trim() || '';
        if (text) {
          return current;
        }
        current = current.previousSibling;
        continue;
      }

      return current;
    }

    return null;
  };

  let container = range.startContainer;
  let offset = range.startOffset;

  if (container.nodeType === Node.TEXT_NODE) {
    if (offset > 0) return null;
    return resolvePreviousNonWhitespaceNode(container.previousSibling);
  }

  const element = container as Element;
  if (element === root) {
    return resolvePreviousNonWhitespaceNode(root.childNodes[offset - 1] || null);
  }

  if (offset > 0) {
    return resolvePreviousNonWhitespaceNode(element.childNodes[offset - 1] || null);
  }

  return resolvePreviousNonWhitespaceNode(element.previousSibling);
};

const addCommandChipToEditor = (
  commandName: string,
  options: {
    focusEditor?: boolean;
    persistDraft?: boolean;
  } = {},
) => {
  if (!editorRef.value) return;

  const { focusEditor = true, persistDraft = true } = options;

  const span = document.createElement('span');
  span.setAttribute('data-command-chip', commandName);
  span.className = 'command-reference-chip';
  span.contentEditable = 'false';
  span.title = `/${commandName}`;
  span.innerHTML = `<span class="command-path">/${commandName}</span>`;

  const space = document.createTextNode(' ');

  editorRef.value.appendChild(span);
  editorRef.value.appendChild(space);
  selectedCommandChip.value = commandName;
  editorText.value = getNormalizedEditorText();
  if (persistDraft) {
    updateCurrentSessionDraft(editorText.value);
  }
  if (focusEditor) {
    editorRef.value.focus();
    setCaretToEnd();
  }
  queueAdjustHeight();
};

const removeCommandChip = () => {
  if (editorRef.value) {
    const span = editorRef.value.querySelector('[data-command-chip]');
    if (span) {
      const nextSibling = span.nextSibling;
      span.remove();
      if (nextSibling?.nodeType === Node.TEXT_NODE && nextSibling.textContent === '\u00A0') {
        nextSibling.remove();
      }
    }
  }

  selectedCommandChip.value = null;
  editorText.value = getNormalizedEditorText();
  updateCurrentSessionDraft(editorText.value);
  editorRef.value?.focus();
  queueAdjustHeight();
};

// 移除文件引用
const removeFileReference = (id: string) => {
  // 从列表中移除
  fileReferences.value = fileReferences.value.filter(f => f.id !== id);
  imageAttachments.value = imageAttachments.value.filter(i => i.id !== id);

  // 从编辑器中移除
  if (editorRef.value) {
    const span = editorRef.value.querySelector(`[data-file-ref="${id}"]`);
    if (span) {
      // 同时移除后面的空格
      const nextSibling = span.nextSibling;
      span.remove();
      if (nextSibling?.nodeType === Node.TEXT_NODE && nextSibling.textContent === '\u00A0') {
        nextSibling.remove();
      }
    }
  }

  editorRef.value?.focus();
  queueAdjustHeight();
};

const removeImageAttachment = (id: string) => {
  imageAttachments.value = imageAttachments.value.filter((image) => image.id !== id);
  editorRef.value?.focus();
};

const openImagePreview = (image: { name: string; mediaType: string; data: string }) => {
  previewImage.value = {
    name: image.name,
    src: `data:${image.mediaType};base64,${image.data}`,
  };
};

const closeImagePreview = () => {
  previewImage.value = null;
};


// 处理拖拽
const handleDragOver = (e: DragEvent) => {
  e.preventDefault();
  isDragging.value = true;
};

const handleDragLeave = (e: DragEvent) => {
  e.preventDefault();
  isDragging.value = false;
};

const handleDrop = async (e: DragEvent) => {
  e.preventDefault();
  isDragging.value = false;

  const files = e.dataTransfer?.files;
  if (!files) return;

  for (const file of Array.from(files)) {
    await addFile(file);
  }
};

// 处理粘贴
const handlePaste = async (e: ClipboardEvent) => {
  const items = e.clipboardData?.items;
  const clipboardData = e.clipboardData;
  if (!items || !clipboardData) return;

  let handledImage = false;
  for (const item of Array.from(items)) {
    if (item.type.startsWith('image/')) {
      const file = item.getAsFile();
      if (file) {
        e.preventDefault();
        await addFile(file);
        handledImage = true;
      }
    }
  }

  if (handledImage) return;

  const text = clipboardData.getData('text/plain');
  if (!text) return;

  e.preventDefault();
  insertPlainTextAtCursor(text);
  handleInput();
};

// 监听输入变化
const handleInput = () => {
  const text = getNormalizedEditorText();
  editorText.value = text;
  selectedCommandChip.value = editorRef.value?.querySelector('[data-command-chip]')?.getAttribute('data-command-chip') || null;
  updateCurrentSessionDraft(text);
  queueAdjustHeight();

  if (showFileMenu.value) {
    syncFileMenuQueryFromEditor();
  } else {
    const inlineTrigger = getInlineFileTriggerAtSelection();
    if (inlineTrigger) {
      openFileMenu({
        range: inlineTrigger.range,
        initialQuery: inlineTrigger.query,
      });
    }
  }

  // 当用户输入 / 时，若当前没有可用命令则从后端加载
  const currentSession = claudeStore.currentSession;
  const hasSessionCommands = !!currentSession?.commands?.length;

  if (text === '/' && currentSession && !hasSessionCommands && !isLoadingCommands.value) {
    loadCommandsFromBackend();
  }

  checkCliInitialized();

  if (shouldShowSlashMenu.value && !showSlashMenu.value) {
    showSlashMenu.value = true;
  } else if (!shouldShowSlashMenu.value && showSlashMenu.value) {
    showSlashMenu.value = false;
  }
};

// 自适应高度
const adjustHeight = () => {
  const editor = editorRef.value;
  if (!editor) return;

  editor.style.height = 'auto';
  const contentHeight = editor.scrollHeight;
  const newHeight = Math.max(44, Math.min(contentHeight, 200));
  editor.style.height = `${newHeight}px`;
  editor.style.overflowY = contentHeight > 200 ? 'auto' : 'hidden';
};

const queueAdjustHeight = () => {
  if (typeof window === 'undefined') return;

  if (adjustHeightFrame !== null) {
    window.cancelAnimationFrame(adjustHeightFrame);
  }

  adjustHeightFrame = window.requestAnimationFrame(() => {
    adjustHeightFrame = null;
    adjustHeight();
  });
};

watch(() => [editorText.value, fileReferences.value.length], () => {
  queueAdjustHeight();
});

watch(() => props.rewindDraftVersion, () => {
  void applyExternalDraft(props.rewindDraft || '');
});

watch(
  () => [props.sessionId, currentProjectRoot.value] as const,
  ([sessionId, projectRoot]) => {
    void ideStore.initialize(sessionId || '', projectRoot || '');
  },
  { immediate: true },
);

// 点击外部关闭权限模式菜单
const handleClickOutside = (e: MouseEvent) => {
  const target = e.target as Node;

  if (showThinkingMenu.value && thinkingMenuRef.value) {
    if (target && !thinkingMenuRef.value.contains(target)) {
      showThinkingMenu.value = false;
    }
  }

  if (showPermissionMenu.value && permissionMenuRef.value) {
    if (target && permissionMenuRef.value && !permissionMenuRef.value.contains(target)) {
      showPermissionMenu.value = false;
    }
  }

  if (showFileMenu.value && fileMenuRef.value) {
    if (target && !fileMenuRef.value.contains(target) && target !== editorRef.value && !editorRef.value?.contains(target)) {
      closeFileMenu(false);
    }
  }

  if (showAttachmentMenu.value && attachmentMenuRef.value) {
    if (target && !attachmentMenuRef.value.contains(target)) {
      showAttachmentMenu.value = false;
    }
  }
};

const handleDocumentKeyDown = (e: KeyboardEvent) => {
  if (e.key === 'Escape' && showAttachmentMenu.value) {
    showAttachmentMenu.value = false;
    return;
  }

  if (e.key === 'Escape' && props.streaming) {
    e.preventDefault();
    stop();
    return;
  }

  if (e.key === 'Escape' && previewImage.value) {
    closeImagePreview();
    return;
  }
  handlePermissionModeShortcut(e);
};

onMounted(() => {
  void providerStore.load();
  void restoreSessionDraft(props.sessionId);
  document.addEventListener('mousedown', handleClickOutside);
  document.addEventListener('keydown', handleDocumentKeyDown);
  queueAdjustHeight();
});

onUnmounted(() => {
  document.removeEventListener('mousedown', handleClickOutside);
  document.removeEventListener('keydown', handleDocumentKeyDown);
  if (adjustHeightFrame !== null) {
    window.cancelAnimationFrame(adjustHeightFrame);
  }
  if (fileSearchDebounceTimer !== null) {
    window.clearTimeout(fileSearchDebounceTimer);
  }
  void ideStore.teardown();
});
</script>

<template>
  <div class="message-input-container">
    <TodoWriteDock v-if="todoState" :todo-state="todoState" />

    <!-- 输入区域包装器 -->
    <div
      class="input-wrapper"
      :class="{ dragging: isDragging, disabled }"
      @dragover="handleDragOver"
      @dragleave="handleDragLeave"
      @drop="handleDrop"
    >
      <!-- 斜杠命令选择器 -->
      <SlashCommandSelector
        ref="slashMenuRef"
        v-model="showSlashMenu"
        :query="slashQuery"
        @select="handleSelectCommand"
      />

      <div
        v-if="showFileMenu"
        ref="fileMenuRef"
        class="file-selector"
      >
        <div class="file-selector-header">
          <span class="file-selector-title">添加项目文件或文件夹</span>
        </div>
        <div v-if="isLoadingProjectFiles" class="file-selector-empty">正在搜索项目文件…</div>
        <template v-else>
          <div v-if="!fileSearchQuery.trim()" class="file-selector-empty">输入关键字搜索整个项目</div>
          <button
            v-for="(file, index) in projectFiles"
            :key="file.path"
            :data-file-index="index"
            class="file-selector-item"
            :class="{ selected: index === selectedFileMenuIndex }"
            @mousedown.prevent="selectProjectFile(file)"
          >
            <span class="file-selector-icon">{{ file.isDirectory ? '📁' : '📄' }}</span>
            <span class="file-selector-path">{{ file.path }}</span>
          </button>
          <div v-if="fileSearchQuery.trim() && projectFiles.length === 0" class="file-selector-empty">没有找到匹配的文件或文件夹</div>
        </template>
      </div>

      <div v-if="imageAttachments.length > 0" class="image-attachments-row">
        <div
          v-for="image in imageAttachments"
          :key="image.id"
          class="image-attachment-chip"
          :title="image.originalPath || image.path"
          @click="openImagePreview(image)"
        >
          <img v-if="image.preview" :src="image.preview" :alt="image.name" class="image-attachment-thumb" />
          <div v-else class="image-attachment-fallback">IMG</div>
          <span class="image-attachment-name">{{ getDisplayFileName(image.path, image.name) }}</span>
          <button
            type="button"
            class="image-attachment-remove"
            @click.stop="removeImageAttachment(image.id)"
          >
            ×
          </button>
        </div>
      </div>

      <div v-if="previewImage" class="image-preview-overlay" @click.self="closeImagePreview">
        <div class="image-preview-dialog">
          <button type="button" class="image-preview-close" @click="closeImagePreview">×</button>
          <img :src="previewImage.src" :alt="previewImage.name" class="image-preview-full" />
          <div class="image-preview-caption">{{ previewImage.name }}</div>
        </div>
      </div>

      <!-- ContentEditable 编辑器 -->
      <div
        ref="editorRef"
        class="message-input-editor"
        :class="{ 'is-empty': !editorRef?.innerText.trim() }"
        :placeholder="disabled ? '未连接' : streaming ? '输入消息打断 AI 响应...' : '输入消息... (/ 查看命令 @添加文件)'"
        :disabled="disabled"
        contenteditable="true"
        @keydown="handleKeyDown"
        @paste="handlePaste"
        @input="handleInput"
      ></div>

      <!-- 工具栏 -->
      <div class="input-toolbar">
        <div class="toolbar-left">
          <!-- 思考强度选择 -->
          <div class="toolbar-cluster toolbar-cluster-compact">
            <div class="permission-mode-selector" ref="permissionMenuRef" @mousedown.stop>
              <button
                class="permission-mode-btn"
                :class="[getPermissionModeToneClass(currentPermissionMode), { active: showPermissionMenu }]"
                @click.stop.prevent="togglePermissionMenu"
                :title="`权限模式：${currentPermissionModeLabel}（Shift+Tab 切换）`"
                :disabled="disabled"
              >
                <span class="pill-accent" aria-hidden="true">
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                    <path d="M12 3 5 6v6c0 4.4 2.98 8.48 7 9 4.02-.52 7-4.6 7-9V6l-7-3Z"/>
                    <path d="M9.5 12 11 13.5 14.5 10"/>
                  </svg>
                </span>
                <span class="mode-text fixed-width-text" :style="{ width: permissionModeTextWidth }">{{ currentPermissionModeLabel }}</span>
                <svg class="dropdown-arrow" width="12" height="12" viewBox="0 0 12 12" fill="currentColor" aria-hidden="true">
                  <path d="M2 4l4 4 4-4"/>
                </svg>
              </button>

              <!-- 权限模式弹出菜单 -->
              <div v-if="showPermissionMenu" class="permission-mode-menu">
                <div
                  v-for="mode in permissionModes"
                  :key="mode"
                  class="permission-mode-option"
                  :class="[getPermissionModeToneClass(mode), { active: mode === currentPermissionMode }]"
                  @click.stop="setPermissionMode(mode, $event)"
                >
                  <span class="permission-option-copy">
                    <span class="option-name">{{ formatPermissionModeLabel(mode) }}</span>
                    <span class="option-description">{{ getPermissionModeDescription(mode) }}</span>
                  </span>
                  <span v-if="mode === currentPermissionMode" class="option-check" aria-hidden="true">
                    <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="2">
                      <polyline points="1.5 6 4.5 9 10.5 2"/>
                    </svg>
                  </span>
                </div>
              </div>
            </div>
          </div>

          <div class="toolbar-cluster toolbar-cluster-featured">
            <div class="thinking-level-selector" ref="thinkingMenuRef" @mousedown.stop>
              <button
                class="thinking-level-btn"
                :class="{ active: showThinkingMenu, off: currentThinkingLevel === 'off', restarting: isRestartingThinking }"
                @click.stop.prevent="toggleThinkingMenu"
                :title="`当前思考强度：${currentThinkingLabel}`"
                :disabled="disabled || streaming || isRestartingThinking"
              >
                <span class="pill-accent thinking-accent" aria-hidden="true">
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                    <path d="M9 18h6"/>
                    <path d="M10 22h4"/>
                    <path d="M12 2a7 7 0 0 0-4 12.75c.52.39.86.97.96 1.61L9 18h6l.04-1.64c.1-.64.44-1.22.96-1.61A7 7 0 0 0 12 2Z"/>
                  </svg>
                </span>
                <span class="thinking-level-label fixed-width-text" :style="{ width: thinkingLevelTextWidth }">{{ isRestartingThinking ? '重启中...' : currentThinkingLabel }}</span>
                <svg class="dropdown-arrow" width="12" height="12" viewBox="0 0 12 12" fill="currentColor" aria-hidden="true">
                  <path d="M2 4l4 4 4-4"/>
                </svg>
              </button>

              <div v-if="showThinkingMenu" class="thinking-level-menu">
                <div
                  v-for="level in THINKING_LEVELS"
                  :key="level.id"
                  class="thinking-level-option"
                  :class="{ active: level.id === currentThinkingLevel }"
                  @click.stop="setThinkingLevel(level.id, $event)"
                >
                  <span class="option-name">{{ level.label }}</span>
                  <span v-if="level.id === currentThinkingLevel" class="option-check" aria-hidden="true">
                    <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="2">
                      <polyline points="1.5 6 4.5 9 10.5 2"/>
                    </svg>
                  </span>
                </div>
              </div>
            </div>
          </div>

          <div class="toolbar-cluster session-config-selectors">
            <label class="session-select-shell">
              <span class="session-select-icon" aria-hidden="true">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                  <rect x="3" y="6" width="7" height="5" rx="1.5"/>
                  <rect x="14" y="13" width="7" height="5" rx="1.5"/>
                  <path d="M10 8.5h2a2 2 0 0 1 2 2v2.5"/>
                  <path d="M14 15.5h-2a2 2 0 0 1-2-2V11"/>
                </svg>
              </span>
              <select
                class="session-select permission-like-select"
                :value="currentProviderModelValue"
                :style="{ width: providerModelSelectWidth }"
                :disabled="disabled || isRestartingProvider"
                @change="setSessionProviderModel(($event.target as HTMLSelectElement).value || null)"
              >
                <option value="" disabled>选择供应商 - 模型</option>
                <option
                  v-for="option in providerModelOptions"
                  :key="option.key"
                  :value="option.key"
                >
                  {{ option.label }}
                </option>
              </select>
            </label>
          </div>

          <div class="toolbar-cluster">
            <button
              class="thinking-visibility-btn"
              :class="{ active: claudeStore.showThinking }"
              @click="claudeStore.toggleShowThinking()"
              :title="claudeStore.showThinking ? '隐藏思考内容' : '显示思考内容'"
            >
              <span class="pill-accent" aria-hidden="true">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M2 12s3.5-6 10-6 10 6 10 6-3.5 6-10 6-10-6-10-6Z"/>
                  <circle cx="12" cy="12" r="3"/>
                </svg>
              </span>
              <span class="thinking-label">思考内容</span>
            </button>
          </div>
        </div>

        <div class="toolbar-right">
          <!-- 文件上传 -->
          <div class="attachment-menu-shell" ref="attachmentMenuRef" @mousedown.stop>
            <button
              class="toolbar-btn"
              @click.stop.prevent="toggleAttachmentMenu"
              title="添加文件或文件夹"
              :disabled="disabled"
            >
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <path d="m21.44 11.05-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66L9.41 17.41a2 2 0 0 1-2.83-2.83l8.48-8.48"/>
              </svg>
            </button>

            <div v-if="showAttachmentMenu" class="attachment-menu">
              <button type="button" class="attachment-menu-item" @click="triggerFileSelect">
                <span class="attachment-menu-icon">📄</span>
                <span class="attachment-menu-copy">添加文件</span>
              </button>
              <button type="button" class="attachment-menu-item" @click="triggerFolderSelect">
                <span class="attachment-menu-icon">📁</span>
                <span class="attachment-menu-copy">添加文件夹</span>
              </button>
            </div>
          </div>

          <div class="token-indicator-wrapper toolbar-token-indicator">
            <TokenIndicator />
          </div>

          <!-- 发送/终止按钮 -->
          <button
            class="send-btn"
            :class="{ terminate: streaming }"
            :disabled="sendButtonDisabled"
            @click="streaming ? stop() : send()"
            :title="streaming ? '终止' : '发送消息'"
          >
            <svg v-if="!streaming" width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
              <path d="M2.01 21L23 12 2.01 3 2 10l15 2-15 2z"/>
            </svg>
            <svg v-else width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
              <rect x="6" y="6" width="12" height="12" rx="1"/>
            </svg>
            <span v-if="streaming" class="send-label">终止</span>
          </button>
        </div>
      </div>
    </div>

    <div v-if="projectPath" class="input-footer-row">
      <div class="input-footer-left">
        <div v-if="projectPath" class="git-status-inline">
          <GitStatus
            :git-info="gitInfo || null"
            :project-path="projectPath"
            @updated="emit('gitUpdated', $event)"
          />
          <IDEStatus />
        </div>
      </div>
    </div>

    <!-- 隐藏的文件输入 -->
    <input
      ref="fileInput"
      type="file"
      multiple
      @change="handleFileSelect"
      style="display: none;"
    />
  </div>
</template>

<style scoped>
.message-input-container {
  display: flex;
  flex-direction: column;
  gap: 0.2rem;
  position: relative;
}

.input-footer-row {
  display: flex;
  align-items: center;
  justify-content: flex-start;
  gap: 0.75rem;
  min-height: 22px;
  margin-top: 0.1rem;
}

.input-footer-left {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  flex: 1;
  min-width: 0;
}

.git-status-inline {
  display: flex;
  align-items: center;
  justify-content: flex-start;
  gap: 0.55rem;
  flex: 1;
  min-width: 0;
}

.token-indicator-wrapper {
  display: flex;
  justify-content: center;
  align-items: center;
  flex-shrink: 0;
  min-height: 32px;
}

.toolbar-token-indicator {
  margin-left: 0.125rem;
  margin-right: 0.125rem;
}

.input-wrapper {
  position: relative;
  border: 1px solid var(--primary-color-25, rgba(59, 130, 246, 0.22));
  border-radius: 0.75rem;
  background-color: var(--bg-primary, #ffffff);
  box-shadow:
    0 0 0 1px var(--primary-color-10, rgba(59, 130, 246, 0.1)),
    0 10px 24px -18px var(--primary-color-25, rgba(59, 130, 246, 0.22)),
    0 8px 18px -16px rgba(15, 23, 42, 0.12);
}

.input-wrapper.dragging {
  border-color: var(--primary-color, #3b82f6);
  border-style: dashed;
  background-color: rgba(59, 130, 246, 0.05);
  box-shadow:
    0 16px 36px -18px var(--primary-color-40, rgba(59, 130, 246, 0.4)),
    0 10px 24px -16px rgba(15, 23, 42, 0.18);
}

.input-wrapper.disabled {
  opacity: 0.6;
  pointer-events: none;
}

/* ContentEditable 编辑器 */
.message-input-editor {
  display: block;
  box-sizing: border-box;
  min-height: 44px;
  max-height: 200px;
  overflow-y: hidden;
  padding: 0.75rem;
  font-size: 0.875rem;
  line-height: 1.5;
  color: var(--text-primary, #1f2937);
  font-family: inherit;
  white-space: pre-wrap;
  word-break: break-word;
  outline: none;
}

.message-input-editor:empty:before {
  content: attr(placeholder);
  color: var(--text-muted, #9ca3af);
  pointer-events: none;
}

.message-input-editor.is-empty:empty:before {
  content: attr(placeholder);
}

.message-input-editor:disabled {
  cursor: not-allowed;
}

.file-selector {
  position: absolute;
  left: 0.5rem;
  right: 0.5rem;
  bottom: calc(100% - 0.25rem);
  max-height: 280px;
  overflow-y: auto;
  background-color: var(--bg-primary, #ffffff);
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 0.75rem;
  box-shadow: 0 12px 30px rgba(0, 0, 0, 0.12);
  z-index: 120;
  padding: 0.375rem;
}

.file-selector-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
  padding: 0.375rem 0.5rem 0.5rem;
  border-bottom: 1px solid var(--border-color, #f3f4f6);
  margin-bottom: 0.25rem;
}

.file-selector-title {
  font-size: 0.75rem;
  font-weight: 600;
  color: var(--text-primary, #1f2937);
}

.file-selector-query {
  font-size: 0.75rem;
  color: var(--text-secondary, #6b7280);
  font-family: 'Monaco', 'Menlo', monospace;
}

.file-selector-item {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  width: 100%;
  padding: 0.5rem 0.625rem;
  border: none;
  background: transparent;
  border-radius: 0.5rem;
  cursor: pointer;
  text-align: left;
  color: var(--text-primary, #1f2937);
}

.file-selector-item:hover {
  background-color: var(--bg-tertiary, #f3f4f6);
}

.file-selector-item.selected {
  background-color: var(--primary-color, #3b82f6);
  color: #ffffff;
}

.file-selector-icon {
  flex-shrink: 0;
  opacity: 0.8;
}

.file-selector-path {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 0.8125rem;
  font-family: 'Monaco', 'Menlo', monospace;
}

.file-selector-empty {
  padding: 0.75rem 0.625rem;
  font-size: 0.8125rem;
  color: var(--text-secondary, #6b7280);
}

/* 文件引用块 */
.message-input-editor :deep(.file-reference-chip) {
  display: inline-flex;
  align-items: center;
  gap: 0.28rem;
  max-width: min(100%, 240px);
  min-height: 1.45rem;
  padding: 0.1rem 0.42rem;
  margin: 0 0.125rem;
  background: linear-gradient(180deg, rgba(250, 246, 238, 0.96), rgba(241, 235, 226, 0.92));
  border: 1px solid rgba(167, 148, 123, 0.34);
  border-radius: 0.5rem;
  font-size: 0.78rem;
  line-height: 1.1;
  color: #644c34;
  user-select: none;
  vertical-align: middle;
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.62), 0 1px 2px rgba(46, 33, 17, 0.08);
}

.message-input-editor :deep(.file-reference-chip) svg {
  flex-shrink: 0;
  width: 11px;
  height: 11px;
  color: currentColor;
  opacity: 0.82;
}

.message-input-editor :deep(.file-reference-chip .file-path) {
  font-family: 'Monaco', 'Menlo', monospace;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: currentColor;
  font-weight: 600;
}

.message-input-editor :deep(.file-reference-chip .remove-btn) {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 14px;
  height: 14px;
  border: none;
  background: transparent;
  color: currentColor;
  cursor: pointer;
  border-radius: 999px;
  font-size: 12px;
  line-height: 1;
  padding: 0;
  opacity: 0.7;
}

.message-input-editor :deep(.file-reference-chip .remove-btn:hover) {
  background-color: rgba(118, 90, 56, 0.12);
  color: #4b3825;
  opacity: 1;
}

.message-input-editor :deep(.command-reference-chip) {
  display: inline-flex;
  align-items: center;
  margin: 0.125rem 0.15rem 0.125rem 0;
  padding: 0.15rem 0.55rem;
  border-radius: 999px;
  border: 1px solid rgba(59, 130, 246, 0.2);
  background-color: #dbeafe;
  color: #1e40af;
  box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.18);
  user-select: none;
  vertical-align: middle;
}

.message-input-editor :deep(.command-reference-chip .command-path) {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  font-family: 'Monaco', 'Menlo', 'Courier New', monospace;
  font-size: 0.78rem;
  font-weight: 500;
  line-height: 1.25;
  letter-spacing: 0;
  color: inherit;
}

.image-attachments-row {
  display: flex;
  gap: 0.375rem;
  padding: 0.4rem 0.75rem 0.2rem;
  overflow-x: auto;
}

.image-attachment-chip {
  display: inline-flex;
  align-items: center;
  gap: 0.375rem;
  padding: 0.28rem 0.45rem;
  border-radius: 999px;
  border: 1px solid var(--primary-color-30, rgba(59, 130, 246, 0.3));
  background: var(--primary-color-15, rgba(59, 130, 246, 0.15));
  min-width: 0;
  flex-shrink: 0;
  color: var(--primary-hover, #2563eb);
  box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.18);
}

.image-attachment-thumb,
.image-attachment-fallback {
  width: 1.5rem;
  height: 1.5rem;
  border-radius: 0.4rem;
  flex-shrink: 0;
}

.image-attachment-thumb {
  object-fit: cover;
}

.image-attachment-fallback {
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(var(--primary-color-rgb, 59, 130, 246), 0.14);
  color: currentColor;
  font-size: 0.7rem;
  font-weight: 600;
}

.image-attachment-name {
  max-width: 10rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: currentColor;
  font-size: 0.76rem;
  line-height: 1.1;
  font-weight: 600;
}

.image-attachment-remove {
  width: 1rem;
  height: 1rem;
  border: none;
  border-radius: 999px;
  background: transparent;
  color: currentColor;
  cursor: pointer;
  flex-shrink: 0;
  font-size: 0.95rem;
  line-height: 1;
  padding: 0;
  opacity: 0.72;
}

.image-attachment-remove:hover {
  background: rgba(var(--primary-color-rgb, 59, 130, 246), 0.16);
  color: var(--primary-hover, #1d4ed8);
  opacity: 1;
}

.image-preview-overlay {
  position: absolute;
  inset: 0;
  z-index: 40;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 1rem;
  background: rgba(17, 24, 39, 0.6);
  backdrop-filter: blur(2px);
}

.image-preview-dialog {
  position: relative;
  max-width: min(90vw, 720px);
  max-height: 80vh;
  padding: 0.75rem 0.75rem 0.5rem;
  border-radius: 1rem;
  background: var(--bg-primary, #ffffff);
  box-shadow: 0 24px 60px rgba(15, 23, 42, 0.28);
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.image-preview-close {
  position: absolute;
  top: 0.5rem;
  right: 0.5rem;
  width: 1.75rem;
  height: 1.75rem;
  border: none;
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.88);
  color: var(--text-secondary, #4b5563);
  cursor: pointer;
  font-size: 1.1rem;
  line-height: 1;
}

.image-preview-full {
  max-width: 100%;
  max-height: calc(80vh - 3.5rem);
  object-fit: contain;
  border-radius: 0.75rem;
}

.image-preview-caption {
  font-size: 0.8rem;
  color: var(--text-secondary, #4b5563);
  text-align: center;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* 工具栏 */
.input-toolbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 0.5rem;
  flex-wrap: wrap;
  padding: 0.45rem 0.75rem 0.55rem;
  border-top: none;
  background: transparent;
  backdrop-filter: none;
}

.toolbar-left {
  display: flex;
  gap: 0.75rem;
  align-items: center;
  flex-wrap: wrap;
  min-width: 0;
  flex: 1;
}

.session-config-selectors {
  display: flex;
  gap: 0.375rem;
  align-items: center;
  flex-wrap: wrap;
}

.toolbar-cluster {
  display: flex;
  align-items: center;
  gap: 0.375rem;
  min-height: auto;
  padding: 0;
  border: none;
  background: transparent;
  box-shadow: none;
}

.toolbar-cluster-featured {
  gap: 0.625rem;
}

.toolbar-cluster-compact {
  gap: 0.25rem;
}

.session-select-shell {
  display: flex;
  align-items: center;
  gap: 0.35rem;
  min-height: auto;
  padding: 0;
  border: none;
  background: transparent;
  box-shadow: none;
}

.session-select {
  min-width: 110px;
  max-width: 172px;
  font-size: 0.8rem;
  flex: 0 0 auto;
}

.session-select-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  color: var(--text-muted, #9ca3af);
  flex-shrink: 0;
}

.permission-like-select {
  appearance: none;
  -webkit-appearance: none;
  -moz-appearance: none;
  padding: 0.125rem 0.9rem 0.125rem 0;
  background-color: transparent;
  background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 12 12' fill='none'%3E%3Cpath d='M2 4l4 4 4-4' fill='%239ca3af'/%3E%3C/svg%3E");
  background-repeat: no-repeat;
  background-position: right 0 center;
  border: none;
  color: var(--text-primary, #334155);
  font-family: inherit;
  font-weight: 600;
  transition: color 0.18s ease, opacity 0.18s ease;
  cursor: pointer;
}

.permission-like-select:hover:not(:disabled) {
  color: var(--primary-color, #2563eb);
}

.permission-like-select:focus {
  outline: none;
}

.session-select-shell:focus-within {
  box-shadow: none;
}

.permission-like-select:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.thinking-level-selector {
  position: relative;
}

.thinking-level-btn {
  display: flex;
  align-items: center;
  gap: 0.35rem;
  min-height: auto;
  padding: 0.125rem 0.2rem 0.125rem 0;
  background: transparent;
  border: none;
  border-radius: 0;
  color: var(--text-secondary, #64748b);
  cursor: pointer;
  transition: color 0.18s ease, opacity 0.18s ease;
  box-shadow: none;
}

.thinking-level-btn:hover:not(:disabled) {
  color: var(--primary-color, #2563eb);
}

.thinking-level-btn.active {
  color: var(--primary-color, #2563eb);
}

.thinking-level-btn.restarting {
  opacity: 0.8;
  cursor: wait;
}

.thinking-level-btn.off {
  color: var(--text-secondary, #64748b);
}

.pill-accent {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  border-radius: 0;
  flex-shrink: 0;
}

.thinking-accent {
  color: var(--primary-color, #2563eb);
  background: transparent;
  box-shadow: none;
}

.thinking-level-label {
  user-select: none;
  font-size: 0.82rem;
  line-height: 1.1;
  font-weight: 600;
  color: currentColor;
}

.fixed-width-text {
  display: inline-block;
  white-space: nowrap;
  text-align: left;
  flex: 0 0 auto;
}

.thinking-level-menu {
  position: absolute;
  bottom: calc(100% + 0.5rem);
  left: 0;
  min-width: 248px;
  background: rgba(255, 255, 255, 0.98);
  border: none;
  border-radius: 1rem;
  box-shadow: 0 16px 40px -24px rgba(15, 23, 42, 0.22);
  padding: 0.4rem 0.45rem;
  z-index: 20;
  backdrop-filter: blur(18px);
  animation: slideUp 0.2s ease-out;
}

.thinking-level-option {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.875rem;
  padding: 0.65rem 0.75rem;
  border-radius: 0.75rem;
  border: none;
  color: var(--text-primary, #111827);
  cursor: pointer;
  transition: color 0.18s ease, opacity 0.18s ease;
}

.thinking-level-option:hover {
  background: transparent;
  border-color: transparent;
  transform: none;
  color: var(--primary-color, #2563eb);
}

.thinking-level-option.active {
  background: transparent;
  border-color: transparent;
  color: var(--primary-color, #3b82f6);
}

.thinking-visibility-btn {
  display: flex;
  align-items: center;
  gap: 0.35rem;
  min-height: auto;
  padding: 0.125rem 0.2rem;
  background: transparent;
  border: none;
  border-radius: 0;
  color: var(--text-muted, #9ca3af);
  cursor: pointer;
  transition: color 0.18s ease, opacity 0.18s ease;
  font-size: 0.75rem;
}

.thinking-visibility-btn:hover:not(:disabled) {
  color: var(--text-primary, #334155);
}

.thinking-visibility-btn.active {
  color: var(--primary-color, #1d4ed8);
}

.thinking-visibility-btn .thinking-label {
  font-size: 0.82rem;
  font-weight: 500;
  user-select: none;
  line-height: 1.1;
}

/* 权限模式选择器 */
.permission-mode-selector {
  position: relative;
}

.permission-mode-btn {
  display: flex;
  align-items: center;
  gap: 0.35rem;
  min-height: auto;
  padding: 0.125rem 0.35rem;
  background: transparent;
  border: 1px solid transparent;
  border-radius: 0.45rem;
  color: var(--text-secondary, #64748b);
  cursor: pointer;
  transition: color 0.18s ease, opacity 0.18s ease, border-color 0.18s ease;
  box-shadow: none;
}

.permission-mode-btn:hover:not(:disabled) {
  color: var(--primary-color, #2563eb);
}

.permission-mode-btn.active {
  color: var(--primary-color, #2563eb);
}

.permission-mode-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.mode-text {
  font-size: 0.82rem;
  font-weight: 600;
  line-height: 1.1;
  color: currentColor;
}

.dropdown-arrow {
  transition: transform 0.2s;
  flex-shrink: 0;
  color: currentColor;
  opacity: 0.5;
}

.thinking-level-btn.active .dropdown-arrow,
.permission-mode-btn.active .dropdown-arrow {
  transform: rotate(180deg);
}

.permission-mode-menu {
  position: absolute;
  bottom: calc(100% + 0.375rem);
  left: 0;
  min-width: 260px;
  background: rgba(255, 255, 255, 0.98);
  border: none;
  border-radius: 1rem;
  box-shadow: 0 16px 40px -24px rgba(15, 23, 42, 0.22);
  z-index: 100;
  overflow: hidden;
  padding: 0.4rem 0.45rem;
  backdrop-filter: blur(18px);
  animation: slideUp 0.2s ease-out;
}

@keyframes slideUp {
  from {
    opacity: 0;
    transform: translateY(8px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

.permission-mode-option {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.875rem;
  padding: 0.65rem 0.75rem;
  cursor: pointer;
  transition: color 0.18s ease, opacity 0.18s ease, border-color 0.18s ease;
  border: 1px solid transparent;
  border-radius: 0.75rem;
}

.permission-mode-option:hover {
  background-color: transparent;
  border-color: transparent;
  transform: none;
  color: var(--primary-color, #2563eb);
}

.permission-mode-option.active {
  background-color: transparent;
  border-color: transparent;
  color: var(--primary-color, #2563eb);
}

.permission-option-copy {
  display: flex;
  flex-direction: column;
  gap: 0.2rem;
  min-width: 0;
}

.option-name {
  font-size: 0.8rem;
  font-weight: 600;
  color: currentColor;
}

.option-description {
  font-size: 0.72rem;
  line-height: 1.35;
  color: var(--text-secondary, #64748b);
}

.option-check {
  width: auto;
  height: auto;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 0;
  background: transparent;
  color: var(--primary-color, #2563eb);
  box-shadow: none;
  flex-shrink: 0;
}

.permission-tone-default {
  border-color: transparent;
}

.permission-tone-acceptEdits {
  border-color: rgba(59, 130, 246, 0.75);
}

.permission-tone-bypassPermissions {
  border-color: rgba(239, 68, 68, 0.82);
}

.permission-tone-plan {
  border-color: rgba(234, 179, 8, 0.88);
}

.toolbar-right {
  display: flex;
  gap: 0.35rem;
  align-items: center;
  margin-left: auto;
  flex-shrink: 0;
}

.attachment-menu-shell {
  position: relative;
}

.attachment-menu {
  position: absolute;
  right: 0;
  bottom: calc(100% + 0.55rem);
  min-width: 142px;
  padding: 0.35rem;
  border-radius: 0.9rem;
  background: rgba(255, 255, 255, 0.98);
  border: 1px solid var(--border-color, #e5e7eb);
  box-shadow: 0 16px 40px -24px rgba(15, 23, 42, 0.22);
  backdrop-filter: blur(18px);
  z-index: 25;
  animation: slideUp 0.18s ease-out;
}

.attachment-menu-item {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 0.55rem;
  padding: 0.6rem 0.7rem;
  border: none;
  border-radius: 0.7rem;
  background: transparent;
  color: var(--text-primary, #111827);
  cursor: pointer;
  text-align: left;
  transition: background-color 0.18s ease, color 0.18s ease;
}

.attachment-menu-item:hover {
  background: var(--primary-color-10, rgba(59, 130, 246, 0.1));
  color: var(--primary-hover, #2563eb);
}

.attachment-menu-icon {
  width: 1rem;
  text-align: center;
  flex-shrink: 0;
}

.attachment-menu-copy {
  font-size: 0.8rem;
  font-weight: 600;
  white-space: nowrap;
}

.toolbar-btn {
  width: auto;
  height: auto;
  background: transparent;
  border: none;
  cursor: pointer;
  color: var(--text-secondary, #6b7280);
  padding: 0.15rem 0.2rem;
  border-radius: 0;
  transition: color 0.18s ease, opacity 0.18s ease;
  display: flex;
  align-items: center;
  justify-content: center;
  box-shadow: none;
}

.toolbar-btn:hover:not(:disabled) {
  color: var(--primary-color, #2563eb);
}

.toolbar-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* 发送按钮 */
.send-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.25rem;
  min-width: 0;
  height: auto;
  padding: 0.15rem 0.2rem;
  background: transparent;
  color: var(--primary-color, #2563eb);
  border: none;
  border-radius: 0;
  cursor: pointer;
  transition: color 0.18s ease, opacity 0.18s ease;
  box-shadow: none;
}

.send-btn:hover:not(:disabled) {
  color: var(--primary-hover, #1d4ed8);
  transform: none;
  box-shadow: none;
}

.send-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
  transform: none;
}

.send-btn.terminate {
  gap: 0.375rem;
  padding: 0.375rem 0.75rem;
  background-color: #ef4444;
  color: #ffffff;
  border-radius: 0.375rem;
  box-shadow: none;
}

.send-btn.terminate:hover:not(:disabled) {
  background-color: #dc2626;
  color: #ffffff;
  box-shadow: none;
}

.send-btn.terminate .send-label {
  font-size: 0.875rem;
  font-weight: 500;
}

/* 深色模式 */
@media (prefers-color-scheme: dark) {
  .input-wrapper {
    background-color: transparent;
    border-color: var(--primary-color-25, rgba(59, 130, 246, 0.22));
    box-shadow:
      0 0 0 1px rgba(var(--primary-color-rgb, 59, 130, 246), 0.1),
      0 12px 24px -18px rgba(var(--primary-color-rgb, 59, 130, 246), 0.22),
      0 10px 22px -16px rgba(0, 0, 0, 0.34);
  }

  .message-input-editor {
    color: var(--text-primary, #f9fafb);
  }

  .message-input-editor :deep(.file-reference-chip) {
    background: linear-gradient(180deg, rgba(74, 60, 42, 0.96), rgba(58, 46, 32, 0.94));
    border-color: rgba(198, 170, 131, 0.28);
    color: #f4e7d4;
  }

  .message-input-editor :deep(.file-reference-chip .file-path) {
    color: currentColor;
  }

  .message-input-editor :deep(.file-reference-chip .remove-btn:hover) {
    background-color: rgba(244, 231, 212, 0.12);
  }

  .file-selector {
    background: var(--bg-secondary, #111827);
    border-color: var(--border-color, #374151);
    box-shadow: 0 12px 30px rgba(0, 0, 0, 0.35);
  }

  .file-selector-header {
    border-bottom-color: var(--border-color, #374151);
  }

  .file-selector-title,
  .file-selector-item {
    color: var(--text-primary, #f9fafb);
  }

  .file-selector-query,
  .file-selector-empty {
    color: var(--text-secondary, #9ca3af);
  }

  .file-selector-item:hover {
    background-color: var(--bg-tertiary, #374151);
  }

  .file-selector-item.selected {
    background-color: var(--primary-color, #3b82f6);
    color: #ffffff;
  }

  .image-attachment-chip {
    background: rgba(var(--primary-color-rgb, 59, 130, 246), 0.22);
    border-color: rgba(var(--primary-color-rgb, 59, 130, 246), 0.36);
    color: #dbeafe;
  }

  .attachment-menu {
    background: rgba(17, 24, 39, 0.98);
    border-color: var(--border-color, #374151);
    box-shadow: 0 18px 38px rgba(0, 0, 0, 0.34);
  }

  .attachment-menu-item {
    color: var(--text-primary, #f9fafb);
  }

  .attachment-menu-item:hover {
    background: rgba(var(--primary-color-rgb, 59, 130, 246), 0.18);
    color: #dbeafe;
  }

  .image-attachment-fallback {
    background: rgba(255, 255, 255, 0.12);
  }

  .image-attachment-remove:hover {
    background: rgba(255, 255, 255, 0.12);
    color: #ffffff;
  }

  .thinking-level-btn {
    background: transparent;
    border: none;
  }

  .thinking-level-btn.off {
    background: transparent;
    border: none;
  }

  .thinking-level-menu {
    background: rgba(15, 23, 42, 0.94);
    border: none;
    box-shadow: 0 16px 40px -24px rgba(0, 0, 0, 0.58);
  }

  .thinking-level-option {
    color: var(--text-primary, #f9fafb);
  }

  .thinking-level-option:hover {
    background: transparent;
    border-color: transparent;
  }

  .thinking-visibility-btn {
    color: var(--text-secondary, #94a3b8);
    background: transparent;
  }

  .thinking-visibility-btn:hover {
    color: var(--text-primary, #f8fafc);
    background: transparent;
  }

  .toolbar-btn:hover:not(:disabled) {
    background: transparent;
  }

  .permission-mode-btn {
    background: transparent;
    border: none;
    color: var(--text-secondary, #cbd5e1);
  }

  .permission-mode-btn:hover:not(:disabled) {
    background: transparent;
  }

  .permission-mode-btn.active {
    background: transparent;
  }

  .permission-mode-menu {
    background: rgba(15, 23, 42, 0.95);
    border: none;
    box-shadow: 0 16px 40px -24px rgba(0, 0, 0, 0.58);
  }

  .permission-mode-option:hover {
    background-color: transparent;
    border-color: transparent;
  }

  .permission-mode-option.active {
    background-color: transparent;
    border-color: transparent;
  }

  .option-name {
    color: var(--text-primary, #f9fafb);
  }

  .option-description {
    color: var(--text-secondary, #94a3b8);
  }

  .session-select-icon,
  .dropdown-arrow {
    color: var(--text-muted, #94a3b8);
  }

  .toolbar-cluster,
  .session-select-shell,
  .toolbar-btn,
  .input-toolbar {
    background: transparent;
    border: none;
  }

  .toolbar-cluster-featured {
    background: transparent;
    border: none;
  }

  .session-select-shell:focus-within {
    box-shadow: none;
  }

  .permission-like-select,
  .mode-text,
  .thinking-level-label {
    color: var(--text-primary, #f8fafc);
  }
}
@media (max-width: 900px) {
  .toolbar-left {
    width: 100%;
  }

  .toolbar-cluster-featured,
  .session-config-selectors {
    width: 100%;
  }

  .session-select-shell {
    flex: 1 1 180px;
  }

  .thinking-level-btn,
  .thinking-visibility-btn {
    flex: 1;
  }

  .toolbar-right {
    width: 100%;
    justify-content: flex-end;
    margin-left: 0;
  }

  .toolbar-token-indicator {
    margin-left: 0;
    margin-right: 0;
  }
}
</style>
