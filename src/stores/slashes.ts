/**
 * 斜杠命令管理 Store
 * 管理可用的斜杠命令
 * 复刻 companion 项目：从会话状态中读取命令
 */

import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import { useClaudeStore } from './claude';

/**
 * 命令类型
 */
export type CommandType = 'command';
export type CommandExecution = 'ui' | 'session';

/**
 * CommandInfo 类型（来自 initialize 响应）
 */
export interface CommandInfo {
  name: string;
  description: string;
  argumentHint?: string | string[];
}

/**
 * 命令项
 */
export interface CommandItem {
  name: string;
  type: CommandType;
  description?: string;
  argumentHint?: string | string[];
  category?: string;
  immediate?: boolean;
  execution?: CommandExecution;
}

const COMMAND_NAME_BOUNDARIES = new Set([':', '-', '_', '/']);

export function normalizeCommandQuery(query: string): string {
  return query.trim().toLowerCase().replace(/^\/+/, '');
}

export function scoreCommandNameMatch(name: string, rawQuery: string): number | null {
  const query = normalizeCommandQuery(rawQuery);
  if (!query) return 0;

  const normalizedName = name.trim().toLowerCase();
  if (!normalizedName) return null;

  if (normalizedName === query) return 1000;

  if (normalizedName.startsWith(query)) {
    return 900 - normalizedName.length;
  }

  const boundaryIndex = Array.from(COMMAND_NAME_BOUNDARIES)
    .map((boundary) => normalizedName.indexOf(`${boundary}${query}`))
    .find((index) => index >= 0);

  if (boundaryIndex !== undefined) {
    return 800 - boundaryIndex;
  }

  const substringIndex = normalizedName.indexOf(query);
  if (substringIndex >= 0) {
    return 700 - substringIndex;
  }

  let nextSearchFrom = 0;
  let previousIndex = -1;
  let score = 0;

  for (const char of query) {
    const index = normalizedName.indexOf(char, nextSearchFrom);
    if (index < 0) return null;

    score += 10;
    if (index === previousIndex + 1) score += 6;
    if (index === 0 || COMMAND_NAME_BOUNDARIES.has(normalizedName[index - 1])) score += 5;

    previousIndex = index;
    nextSearchFrom = index + 1;
  }

  return Math.max(1, score - Math.max(0, normalizedName.length - query.length));
}

export function filterAndSortCommands(commandList: CommandItem[], rawQuery: string): CommandItem[] {
  const query = normalizeCommandQuery(rawQuery);
  if (!query) return commandList;

  return commandList
    .map((command, index) => ({
      command,
      index,
      score: scoreCommandNameMatch(command.name, query),
    }))
    .filter((item): item is { command: CommandItem; index: number; score: number } => item.score !== null)
    .sort((left, right) => right.score - left.score || left.index - right.index)
    .map((item) => item.command);
}

function dedupeCommandsByName(commandList: CommandItem[]): CommandItem[] {
  const deduped = new Map<string, CommandItem>();

  for (const command of commandList) {
    if (!deduped.has(command.name)) {
      deduped.set(command.name, command);
      continue;
    }

    const previous = deduped.get(command.name)!;
    deduped.set(command.name, {
      ...previous,
      ...command,
      description: previous.description || command.description,
      argumentHint: previous.argumentHint || command.argumentHint,
      immediate: previous.immediate ?? command.immediate,
      execution: previous.execution ?? command.execution,
      category: previous.category || command.category,
    });
  }

  return Array.from(deduped.values());
}

const BUILTIN_COMMAND_NAMES = new Set([
  'ask',
  'bug',
  'bypass',
  'clear',
  'code',
  'compact',
  'config',
  'context',
  'cost',
  'doctor',
  'exit',
  'export',
  'help',
  'init',
  'mcp',
  'memory',
  'model',
  'permissions',
  'plan',
  'rename',
  'resume',
  'rewind',
  'stats',
  'status',
  'statusline',
  'tasks',
  'teleport',
  'theme',
  'todos',
  'usage',
]);

const BUILTIN_COMMAND_META: Record<string, Pick<CommandItem, 'description' | 'immediate' | 'execution'>> = {
  clear: {
    description: '清空当前会话并新建对话',
    immediate: true,
    execution: 'ui',
  },
  compact: {
    description: '压缩当前会话上下文',
    immediate: true,
    execution: 'session',
  },
  init: {
    description: '初始化当前项目配置',
    immediate: true,
    execution: 'session',
  },
  rewind: {
    description: '回退到之前的对话节点',
    immediate: true,
    execution: 'session',
  },
};

const REQUIRED_BUILTIN_COMMANDS = ['clear', 'compact', 'init', 'rewind'] as const;

function enrichCommandItem(command: CommandItem): CommandItem {
  const meta = BUILTIN_COMMAND_META[command.name];
  const isBuiltin = BUILTIN_COMMAND_NAMES.has(command.name);

  const nextCategory = command.category === 'general' && isBuiltin
    ? 'builtin'
    : (command.category ?? (isBuiltin ? 'builtin' : undefined));

  if (!meta) {
    return {
      ...command,
      category: nextCategory,
    };
  }

  return {
    ...command,
    category: nextCategory,
    description: command.description || meta.description,
    immediate: command.immediate ?? meta.immediate,
    execution: command.execution ?? meta.execution,
  };
}

/**
 * useSlashesStore - 斜杠命令管理
 */
export const useSlashesStore = defineStore('slashes', () => {
  const claudeStore = useClaudeStore();

  // 所有可用命令（内置命令作为后备）
  const commands = ref<CommandItem[]>([]);

  // 会话特定命令
  const sessionCommands = ref<Map<string, CommandItem[]>>(new Map());

  // 加载状态
  const isLoading = ref(false);
  const error = ref<string | null>(null);

  // ========== 计算属性 ==========

  /**
   * 从当前会话状态获取命令
   * 当前仅使用 initialize 返回的 commands；system.init 的 skills 保留在后端内存中，不透传前端
   */
  const sessionDerivedCommands = computed<CommandItem[]>(() => {
    const sessionData = claudeStore.currentSession;
    console.log('[SlashesStore] Computing sessionDerivedCommands:', {
      hasSessionData: !!sessionData,
      sessionId: sessionData?.sessionId,
      commands: sessionData?.commands,
    });

    if (!sessionData) return [];

    const cmds: CommandItem[] = [];

    // 从会话状态中读取 commands（来自 initialize 响应）
    if (sessionData.commands && sessionData.commands.length > 0) {
      console.log('[SlashesStore] Adding commands from initialize:', sessionData.commands);
      for (const cmd of sessionData.commands) {
        cmds.push(enrichCommandItem({
          name: cmd.name,
          type: 'command',
          description: cmd.description,
          argumentHint: cmd.argumentHint
        }));
      }
    }

    for (const builtinName of REQUIRED_BUILTIN_COMMANDS) {
      if (!cmds.some((cmd) => cmd.name === builtinName)) {
        cmds.push(enrichCommandItem({
          name: builtinName,
          type: 'command',
          category: 'general',
        }));
      }
    }

    const dedupedCommands = dedupeCommandsByName(cmds);
    console.log('[SlashesStore] Final commands:', dedupedCommands);
    return dedupedCommands;
  });

  /**
   * 所有可用命令
   * - 聊天场景：有当前会话时，仅使用当前会话从后端拿到的命令
   * - 非聊天场景：没有当前会话时，才回退到内置命令
   */
  const allCommands = computed<CommandItem[]>(() => {
    if (claudeStore.currentSession) {
      return dedupeCommandsByName(sessionDerivedCommands.value);
    }

    return dedupeCommandsByName(commands.value);
  });

  /**
   * 按类型分组的命令
   */
  const commandsByType = computed(() => {
    const grouped: Record<CommandType, CommandItem[]> = {
      command: [],
    };

    for (const cmd of allCommands.value) {
      grouped[cmd.type].push(cmd);
    }

    return grouped;
  });

  /**
   * 按分类分组的命令
   */
  const commandsByCategory = computed(() => {
    const grouped: Record<string, CommandItem[]> = {
      general: [],
      file: [],
      edit: [],
      git: [],
      test: [],
      other: [],
    };

    for (const cmd of allCommands.value) {
      const category = cmd.category || 'other';
      if (!grouped[category]) {
        grouped[category] = [];
      }
      grouped[category].push(cmd);
    }

    return grouped;
  });

  // ========== Actions ==========

  /**
   * 设置全局命令列表
   */
  function setCommands(cmds: CommandItem[]): void {
    commands.value = cmds.map(enrichCommandItem);
  }

  /**
   * 设置会话特定命令
   */
  function setSessionCommands(sessionId: string, cmds: CommandItem[]): void {
    const newSessionCommands = new Map(sessionCommands.value);
    newSessionCommands.set(sessionId, cmds);
    sessionCommands.value = newSessionCommands;
  }

  /**
   * 获取会话命令（合并全局和会话特定命令）
   */
  function getSessionCommands(_sessionId: string): CommandItem[] {
    // 使用当前会话的所有命令
    return allCommands.value;
  }

  /**
   * 过滤命令
   * 复刻 companion 项目：基于查询文本过滤命令
   */
  function filterCommands(query: string): CommandItem[] {
    // 使用当前会话的所有命令
    return filterAndSortCommands(allCommands.value, query);
  }

  /**
   * 添加命令
   */
  function addCommand(cmd: CommandItem): void {
    commands.value = [...commands.value, enrichCommandItem(cmd)];
  }

  /**
   * 移除命令
   */
  function removeCommand(name: string): void {
    commands.value = commands.value.filter(cmd => cmd.name !== name);
  }

  /**
   * 清空命令
   */
  function clearCommands(): void {
    commands.value = [];
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

  // ========== 内置命令 ==========

  /**
   * 初始化内置命令
   * 这些命令作为后备，当会话没有提供命令时使用
   */
  function initBuiltinCommands(): void {
    commands.value = [
      // 通用命令
      enrichCommandItem({ name: 'help', type: 'command', description: '显示帮助信息', category: 'general' }),
      enrichCommandItem({ name: 'clear', type: 'command', description: '清空对话', category: 'general' }),
      enrichCommandItem({ name: 'cost', type: 'command', description: '查看使用成本', category: 'general' }),
      enrichCommandItem({ name: 'compact', type: 'command', description: '压缩上下文', category: 'general' }),
      enrichCommandItem({ name: 'init', type: 'command', description: '初始化当前项目配置', category: 'general' }),
      enrichCommandItem({ name: 'rewind', type: 'command', description: '回退对话', category: 'general' }),
      enrichCommandItem({ name: 'reset', type: 'command', description: '重置会话', category: 'general' }),

      // 文件命令
      enrichCommandItem({ name: 'read', type: 'command', description: '读取文件内容', category: 'file' }),
      enrichCommandItem({ name: 'write', type: 'command', description: '写入文件', category: 'file' }),
      enrichCommandItem({ name: 'edit', type: 'command', description: '编辑文件', category: 'file' }),
      enrichCommandItem({ name: 'search', type: 'command', description: '搜索文件内容', category: 'file' }),

      // Git 命令
      enrichCommandItem({ name: 'git', type: 'command', description: 'Git 操作', category: 'git' }),
      enrichCommandItem({ name: 'commit', type: 'command', description: '提交更改', category: 'git' }),
      enrichCommandItem({ name: 'diff', type: 'command', description: '查看差异', category: 'git' }),
      enrichCommandItem({ name: 'log', type: 'command', description: '查看提交历史', category: 'git' }),

      // 测试命令
      enrichCommandItem({ name: 'test', type: 'command', description: '运行测试', category: 'test' }),
      enrichCommandItem({ name: 'coverage', type: 'command', description: '查看测试覆盖率', category: 'test' }),
    ];
  }

  // 初始化
  initBuiltinCommands();

  return {
    // 状态
    commands,
    sessionCommands,
    isLoading,
    error,

    // 计算属性
    allCommands,
    sessionDerivedCommands,
    commandsByType,
    commandsByCategory,

    // Actions
    setCommands,
    setSessionCommands,
    getSessionCommands,
    filterCommands,
    addCommand,
    removeCommand,
    clearCommands,
    setLoading,
    setError,
    initBuiltinCommands,
  };
});
