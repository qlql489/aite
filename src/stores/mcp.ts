/**
 * MCP 服务 Store
 * 管理 MCP 服务器配置
 */

import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import { invoke } from '@tauri-apps/api/core';

const MCP_SERVERS_CACHE_PREFIX = 'aite:mcp:servers:';
const MCP_STATUS_CACHE_PREFIX = 'aite:mcp:status:';
const MCP_SERVERS_CACHE_TTL = 10 * 60 * 1000;
const MCP_STATUS_CACHE_TTL = 5 * 60 * 1000;

// MCP 服务器类型
export type McpServerType = 'stdio' | 'http' | 'sse';

// MCP 服务器规格
export interface McpServerSpec {
  type?: McpServerType;
  command?: string;
  args?: string[];
  url?: string;
  env?: Record<string, string>;
  headers?: Record<string, string>;
}

// MCP 应用启用状态
export interface McpApps {
  claude: boolean;
  codex: boolean;
  gemini: boolean;
}

// MCP 服务器配置
export interface McpServer {
  id: string;
  name?: string;
  description?: string;
  server: McpServerSpec;
  apps: McpApps;
  enabled: boolean;
  homepage?: string;
  docs?: string;
  tags?: string[];
}

// 预设 MCP 服务器
export interface McpPreset {
  id: string;
  name: string;
  description: string;
  server: McpServerSpec;
  homepage?: string;
  docs?: string;
  tags?: string[];
}

// 验证结果
export interface ValidateResult {
  valid: boolean;
  errors: string[];
}

export interface McpServersResponse {
  servers: McpServer[];
  source_path: string;
  source_scope: 'global' | 'project' | string;
  source_target: string;
}

export type McpServerConnectionStatus = 'connected' | 'failed' | 'pending' | 'disabled';

export interface McpServerStatusInfo {
  name: string;
  status: McpServerConnectionStatus;
  error?: string;
}

interface CachePayload<T> {
  timestamp: number;
  data: T;
}

interface LoadServersOptions {
  force?: boolean;
}

interface LoadStatusesOptions {
  force?: boolean;
  silent?: boolean;
}

// 预设服务器列表
export const MCP_PRESETS: McpPreset[] = [
  {
    id: 'filesystem',
    name: 'Filesystem',
    description: '安全访问指定目录的文件系统',
    server: { type: 'stdio', command: 'npx', args: ['-y', '@modelcontextprotocol/server-filesystem', '/path/to/directory'] },
    homepage: 'https://github.com/modelcontextprotocol/servers/tree/main/src/filesystem',
    docs: 'https://modelcontextprotocol.io/docs',
    tags: ['filesystem', 'file'],
  },
  {
    id: 'fetch',
    name: 'Fetch',
    description: 'HTTP/HTTPS 请求服务器',
    server: { type: 'stdio', command: 'npx', args: ['-y', '@modelcontextprotocol/server-fetch'] },
    homepage: 'https://github.com/modelcontextprotocol/servers/tree/main/src/fetch',
    docs: 'https://modelcontextprotocol.io/docs',
    tags: ['http', 'fetch'],
  },
  {
    id: 'memory',
    name: 'Memory',
    description: '基于知识图谱的内存存储',
    server: { type: 'stdio', command: 'npx', args: ['-y', '@modelcontextprotocol/server-memory'] },
    homepage: 'https://github.com/modelcontextprotocol/servers/tree/main/src/memory',
    docs: 'https://modelcontextprotocol.io/docs',
    tags: ['memory', 'knowledge'],
  },
  {
    id: 'time',
    name: 'Time',
    description: '获取当前时间信息',
    server: { type: 'stdio', command: 'npx', args: ['-y', '@modelcontextprotocol/server-time'] },
    homepage: 'https://github.com/modelcontextprotocol/servers/tree/main/src/time',
    docs: 'https://modelcontextprotocol.io/docs',
    tags: ['time', 'utility'],
  },
  {
    id: 'sequential-thinking',
    name: 'Sequential Thinking',
    description: '逐步推理和思考',
    server: { type: 'stdio', command: 'npx', args: ['-y', '@modelcontextprotocol/server-sequential-thinking'] },
    homepage: 'https://github.com/modelcontextprotocol/servers/tree/main/src/sequential-thinking',
    docs: 'https://modelcontextprotocol.io/docs',
    tags: ['thinking', 'reasoning'],
  },
  {
    id: 'puppeteer',
    name: 'Puppeteer',
    description: '浏览器自动化控制',
    server: { type: 'stdio', command: 'npx', args: ['-y', '@modelcontextprotocol/server-puppeteer'] },
    homepage: 'https://github.com/modelcontextprotocol/servers/tree/main/src/puppeteer',
    docs: 'https://modelcontextprotocol.io/docs',
    tags: ['browser', 'automation'],
  },
  {
    id: 'sql',
    name: 'SQL',
    description: 'SQLite 数据库操作',
    server: { type: 'stdio', command: 'npx', args: ['-y', '@modelcontextprotocol/server-sql'] },
    homepage: 'https://github.com/modelcontextprotocol/servers/tree/main/src/sql',
    docs: 'https://modelcontextprotocol.io/docs',
    tags: ['database', 'sql'],
  },
  {
    id: 'git',
    name: 'Git',
    description: 'Git 仓库操作',
    server: { type: 'stdio', command: 'npx', args: ['-y', '@modelcontextprotocol/server-git'] },
    homepage: 'https://github.com/modelcontextprotocol/servers/tree/main/src/git',
    docs: 'https://modelcontextprotocol.io/docs',
    tags: ['git', 'vcs'],
  },
];

export const useMcpStore = defineStore('mcp', () => {
  // ========== 状态 ==========
  const servers = ref<McpServer[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const sourcePath = ref('');
  const sourceScope = ref<'global' | 'project' | ''>('');
  const sourceTarget = ref('');
  const currentWorkspacePath = ref<string | undefined>(undefined);
  const statusLoading = ref(false);
  const serverStatuses = ref<Record<string, McpServerStatusInfo>>({});
  let loadServersRequestId = 0;
  let loadStatusesRequestId = 0;

  // ========== 计算属性 ==========
  const serverCount = computed(() => servers.value.length);

  const serverList = computed(() => servers.value);

  const enabledServers = computed(() =>
    servers.value.filter((s) => s.enabled !== false)
  );

  const serverStatusList = computed(() => Object.values(serverStatuses.value));

  // ========== 方法 ==========

  function getScopedCacheSuffix(workspacePath?: string): string {
    return workspacePath?.trim() || '__global__';
  }

  function readCache<T>(prefix: string, workspacePath: string | undefined, ttl: number): T | null {
    try {
      const raw = localStorage.getItem(`${prefix}${getScopedCacheSuffix(workspacePath)}`);
      if (!raw) return null;

      const parsed = JSON.parse(raw) as CachePayload<T>;
      if (Date.now() - parsed.timestamp > ttl) {
        localStorage.removeItem(`${prefix}${getScopedCacheSuffix(workspacePath)}`);
        return null;
      }

      return parsed.data;
    } catch {
      return null;
    }
  }

  function writeCache<T>(prefix: string, workspacePath: string | undefined, data: T) {
    try {
      const payload: CachePayload<T> = {
        timestamp: Date.now(),
        data,
      };
      localStorage.setItem(
        `${prefix}${getScopedCacheSuffix(workspacePath)}`,
        JSON.stringify(payload),
      );
    } catch (error) {
      console.warn('[MCP Store] Failed to write cache:', error);
    }
  }

  function applyServersResponse(result: McpServersResponse) {
    servers.value = result.servers;
    sourcePath.value = result.source_path;
    sourceScope.value = (result.source_scope as 'global' | 'project') || '';
    sourceTarget.value = result.source_target || '';
  }

  function applyStatusList(result: McpServerStatusInfo[]) {
    serverStatuses.value = result.reduce<Record<string, McpServerStatusInfo>>((acc, item) => {
      acc[item.name] = item;
      return acc;
    }, {});
  }

  function updateServerEnabledLocally(serverId: string, enabled: boolean) {
    servers.value = servers.value.map((server) => {
      if (server.id !== serverId) return server;
      return {
        ...server,
        enabled,
        apps: {
          ...server.apps,
          claude: enabled,
        },
      };
    });
  }

  function updateServerStatusLocally(serverId: string, status: McpServerStatusInfo) {
    serverStatuses.value = {
      ...serverStatuses.value,
      [serverId]: status,
    };
  }

  /**
   * 加载所有 MCP 服务器
   */
  async function loadServers(workspacePath?: string, options: LoadServersOptions = {}) {
    error.value = null;
    const previousWorkspacePath = currentWorkspacePath.value;
    currentWorkspacePath.value = workspacePath?.trim() || undefined;
    const scopedWorkspacePath = currentWorkspacePath.value;
    const requestId = ++loadServersRequestId;

    const cachedServersResponse = !options.force
      ? readCache<McpServersResponse>(
          MCP_SERVERS_CACHE_PREFIX,
          scopedWorkspacePath,
          MCP_SERVERS_CACHE_TTL,
        )
      : null;

    if (cachedServersResponse) {
      applyServersResponse(cachedServersResponse);
      loading.value = false;
      const cachedStatuses = readCache<McpServerStatusInfo[]>(
        MCP_STATUS_CACHE_PREFIX,
        scopedWorkspacePath,
        MCP_STATUS_CACHE_TTL,
      );
      if (cachedStatuses) {
        applyStatusList(cachedStatuses);
      }
    } else {
      if (previousWorkspacePath !== scopedWorkspacePath) {
        servers.value = [];
        serverStatuses.value = {};
      }
      loading.value = servers.value.length === 0;
    }

    try {
      const result = await invoke<McpServersResponse>('get_mcp_servers', {
        workspacePath: scopedWorkspacePath,
      });

      if (requestId !== loadServersRequestId) {
        return;
      }

      applyServersResponse(result);
      writeCache(MCP_SERVERS_CACHE_PREFIX, scopedWorkspacePath, result);
      loading.value = false;
      void loadServerStatuses(scopedWorkspacePath, { silent: true, force: options.force });
    } catch (e) {
      if (requestId !== loadServersRequestId) {
        return;
      }

      error.value = e instanceof Error ? e.message : String(e);
      if (!cachedServersResponse) {
        sourcePath.value = '';
        sourceScope.value = '';
        sourceTarget.value = '';
        serverStatuses.value = {};
      }
      console.error('[MCP Store] Failed to load servers:', e);
    } finally {
      if (requestId === loadServersRequestId) {
        loading.value = false;
      }
    }
  }

  async function loadServerStatuses(workspacePath?: string, options: LoadStatusesOptions = {}) {
    const scopedWorkspacePath = workspacePath?.trim() || currentWorkspacePath.value;
    const requestId = ++loadStatusesRequestId;
    const cachedStatuses = !options.force
      ? readCache<McpServerStatusInfo[]>(
          MCP_STATUS_CACHE_PREFIX,
          scopedWorkspacePath,
          MCP_STATUS_CACHE_TTL,
        )
      : null;

    if (cachedStatuses) {
      applyStatusList(cachedStatuses);
      if (options.silent) {
        statusLoading.value = false;
      }
    } else {
      statusLoading.value = true;
    }

    try {
      const result = await invoke<McpServerStatusInfo[]>('get_mcp_server_status', {
        workspacePath: scopedWorkspacePath,
      });

      if (requestId !== loadStatusesRequestId) {
        return;
      }

      applyStatusList(result);
      writeCache(MCP_STATUS_CACHE_PREFIX, scopedWorkspacePath, result);
    } catch (e) {
      if (requestId !== loadStatusesRequestId) {
        return;
      }
      console.error('[MCP Store] Failed to load server statuses:', e);
    } finally {
      if (requestId === loadStatusesRequestId) {
        statusLoading.value = false;
      }
    }
  }

  /**
   * 验证 MCP 服务器配置
   */
  async function validateServer(server: McpServer): Promise<ValidateResult> {
    try {
      const result = await invoke<[boolean, string[]]>('validate_mcp_server', { server });
      return { valid: result[0], errors: result[1] };
    } catch (e) {
      return { valid: false, errors: [e instanceof Error ? e.message : String(e)] };
    }
  }

  /**
   * 添加或更新 MCP 服务器
   */
  async function upsertServer(server: McpServer, workspacePath?: string): Promise<{ success: boolean; error?: string }> {
    try {
      const scopedWorkspacePath = workspacePath?.trim() || currentWorkspacePath.value;
      await invoke('upsert_mcp_server', { server, workspacePath: scopedWorkspacePath });
      await loadServers(scopedWorkspacePath, { force: true });
      return { success: true };
    } catch (e) {
      const error = e instanceof Error ? e.message : String(e);
      console.error('[MCP Store] Failed to upsert server:', e);
      return { success: false, error };
    }
  }

  /**
   * 删除 MCP 服务器
   */
  async function deleteServer(id: string, workspacePath?: string): Promise<{ success: boolean; error?: string }> {
    try {
      const scopedWorkspacePath = workspacePath?.trim() || currentWorkspacePath.value;
      await invoke('delete_mcp_server', { id, workspacePath: scopedWorkspacePath });
      await loadServers(scopedWorkspacePath, { force: true });
      return { success: true };
    } catch (e) {
      const error = e instanceof Error ? e.message : String(e);
      console.error('[MCP Store] Failed to delete server:', e);
      return { success: false, error };
    }
  }

  /**
   * 切换服务器启用状态
   */
  async function toggleServer(serverId: string, enabled: boolean, workspacePath?: string): Promise<{ success: boolean; error?: string }> {
    const scopedWorkspacePath = workspacePath?.trim() || currentWorkspacePath.value;
    const previousServer = servers.value.find((server) => server.id === serverId);
    const previousStatus = serverStatuses.value[serverId];

    updateServerEnabledLocally(serverId, enabled);
    updateServerStatusLocally(serverId, {
      name: serverId,
      status: enabled ? 'pending' : 'disabled',
      error: enabled ? undefined : '服务器已禁用',
    });
    writeCache(MCP_SERVERS_CACHE_PREFIX, scopedWorkspacePath, {
      servers: servers.value,
      source_path: sourcePath.value,
      source_scope: sourceScope.value,
      source_target: sourceTarget.value,
    });
    writeCache(
      MCP_STATUS_CACHE_PREFIX,
      scopedWorkspacePath,
      Object.values(serverStatuses.value),
    );

    try {
      await invoke('toggle_mcp_server_app', {
        serverId,
        app: 'claude',
        enabled,
        workspacePath: scopedWorkspacePath,
      });
      void loadServerStatuses(scopedWorkspacePath, { force: true, silent: true });
      return { success: true };
    } catch (e) {
      if (previousServer) {
        updateServerEnabledLocally(serverId, previousServer.enabled);
      }
      if (previousStatus) {
        updateServerStatusLocally(serverId, previousStatus);
      } else {
        const nextStatuses = { ...serverStatuses.value };
        delete nextStatuses[serverId];
        serverStatuses.value = nextStatuses;
      }
      writeCache(MCP_SERVERS_CACHE_PREFIX, scopedWorkspacePath, {
        servers: servers.value,
        source_path: sourcePath.value,
        source_scope: sourceScope.value,
        source_target: sourceTarget.value,
      });
      writeCache(
        MCP_STATUS_CACHE_PREFIX,
        scopedWorkspacePath,
        Object.values(serverStatuses.value),
      );
      const error = e instanceof Error ? e.message : String(e);
      console.error('[MCP Store] Failed to toggle server:', e);
      return { success: false, error };
    }
  }

  function getServerStatus(serverId: string): McpServerStatusInfo | undefined {
    return serverStatuses.value[serverId];
  }

  /**
   * 检查 ID 是否已存在
   */
  function isIdExists(id: string): boolean {
    return servers.value.some((s) => s.id === id);
  }

  /**
   * 生成唯一的 ID
   */
  function generateUniqueId(baseId: string): string {
    let id = baseId;
    let counter = 1;
    while (isIdExists(id)) {
      id = `${baseId}-${counter}`;
      counter++;
    }
    return id;
  }

  /**
   * 从预设创建服务器
   */
  function createFromPreset(preset: McpPreset): McpServer {
    return {
      id: preset.id,
      name: preset.name,
      description: preset.description,
      server: preset.server,
      apps: {
        claude: true,
        codex: false,
        gemini: false,
      },
      enabled: true,
      homepage: preset.homepage,
      docs: preset.docs,
      tags: preset.tags,
    };
  }

  /**
   * 初始化 Store
   */
  async function initialize() {
    await loadServers();
  }

  return {
    // 状态
    servers,
    loading,
    error,
    sourcePath,
    sourceScope,
    sourceTarget,
    currentWorkspacePath,
    statusLoading,
    serverStatuses,

    // 计算属性
    serverCount,
    serverList,
    enabledServers,
    serverStatusList,

    // 方法
    loadServers,
    loadServerStatuses,
    validateServer,
    upsertServer,
    deleteServer,
    toggleServer,
    getServerStatus,
    isIdExists,
    generateUniqueId,
    createFromPreset,
    initialize,
  };
});
