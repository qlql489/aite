<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { useSlashesStore } from '../stores/slashes';
import { useMcpStore, type McpServer, type McpServerStatusInfo } from '../stores/mcp';
import { useSkillsStore, type SkillFile } from '../stores/skills';
import ProjectTreeRow, { type ProjectTreeNode } from './ProjectTreeRow.vue';
import { HugeiconsIcon } from '@hugeicons/vue';
import {
  ServerStack01Icon,
  Wifi01Icon,
  GlobeIcon,
  PlusSignIcon,
  GridIcon,
  Brackets,
  Delete02Icon,
  Edit01Icon,
  CommandIcon,
  ZapIcon,
  Plug01Icon,
  RefreshIcon,
  EyeIcon,
  PaintBrushIcon,
  Search01Icon,
  FloppyDiskIcon,
  Edit02Icon,
  LayoutTwoColumnIcon,
  FolderOpenIcon,
  Loading02Icon,
} from '@hugeicons/core-free-icons';

const props = withDefaults(defineProps<{
  mode?: 'settings' | 'workspace';
  workspacePath?: string;
}>(), {
  mode: 'settings',
  workspacePath: undefined,
});

// Tab 类型
type TabType = 'command' | 'skill' | 'mcp' | 'plugin';

// MCP Tab 内部 Tab
type McpTab = 'list' | 'json';

interface ProjectTreeResponse {
  root_path: string;
  root_name: string;
  summary: {
    total_files: number;
    total_dirs: number;
    displayed_entries: number;
  };
  tree: ProjectTreeNode[];
  truncated: boolean;
}

interface ProjectFileResponse {
  path: string;
  content: string;
  size: number;
}

// 状态
const activeTab = ref<TabType>('command');
const slashesStore = useSlashesStore();
const mcpStore = useMcpStore();
const isWorkspaceMode = computed(() => props.mode === 'workspace');
const workspacePath = computed(() => props.workspacePath?.trim() || undefined);
const extensionsLoading = ref(false);
const extensionsError = ref('');

// MCP Tab 状态
const mcpTab = ref<McpTab>('list');
const mcpJsonText = ref('');

// Skills store
const skillsStore = useSkillsStore();
const commandFiles = ref<SkillFile[]>([]);
const skillFiles = ref<SkillFile[]>([]);

// 命令列表 - 从文件系统读取
const commands = computed(() => commandFiles.value);

// Skill 列表 - 从文件系统读取
const skills = computed(() => skillFiles.value);

// Skill Tab 状态
const skillSearch = ref('');
const selectedSkill = ref<SkillFile | null>(null);
const skillViewMode = ref<'edit' | 'preview' | 'split'>('edit');
const skillContent = ref('');
const skillSaving = ref(false);
const skillSaved = ref(false);
const showSkillCreate = ref(false);
const skillCreateMode = ref<'menu' | 'write'>('menu');
const newSkillName = ref('');
const skillLoading = ref(false);
const skillCreatePending = ref(false);
const skillToastMessage = ref('');
const showSkillToast = ref(false);
let skillToastTimer: ReturnType<typeof setTimeout> | null = null;
const skillTreeQuery = ref('');
const skillTreeLoading = ref(false);
const skillTreeError = ref('');
const skillTreeData = ref<ProjectTreeResponse | null>(null);
const skillExpandedPaths = ref<Set<string>>(new Set());
const selectedSkillTreeFile = ref('');
const selectedSkillRootPath = ref('');
const skillFileSize = ref(0);
const commandMainRef = ref<HTMLElement | null>(null);
const skillMainRef = ref<HTMLElement | null>(null);
const skillEditorLayoutRef = ref<HTMLElement | null>(null);
const commandListWidth = ref(280);
const skillListWidth = ref(280);
const skillTreeWidth = ref(280);
const activeResizeTarget = ref<'commandList' | 'skillList' | 'skillTree' | null>(null);
const resizeStartX = ref(0);
const resizeStartWidth = ref(0);

// 命令 Tab 状态
const commandSearch = ref('');
const selectedCommand = ref<SkillFile | null>(null);
const commandViewMode = ref<'edit' | 'preview' | 'split'>('edit');
const commandContent = ref('');
const commandSaving = ref(false);
const commandSaved = ref(false);
const commandLoading = ref(false);
const showCommandCreate = ref(false);
const newCommandName = ref('');
const newCommandScope = ref<'global' | 'project'>('global');
const showDeleteConfirmDialog = ref(false);
const pendingDeleteItem = ref<SkillFile | null>(null);
const pendingDeleteType = ref<'command' | 'skill' | null>(null);

// 过滤后的 skills
const filteredSkills = computed(() => {
  if (!skillSearch.value) return skills.value;
  const query = skillSearch.value.toLowerCase();
  return skills.value.filter(s =>
    s.name.toLowerCase().includes(query) ||
    (s.description && s.description.toLowerCase().includes(query))
  );
});

// 过滤后的 commands
const filteredCommands = computed(() => {
  if (!commandSearch.value) return commands.value;
  const query = commandSearch.value.toLowerCase();
  return commands.value.filter(c =>
    c.name.toLowerCase().includes(query) ||
    (c.description && c.description.toLowerCase().includes(query))
  );
});

// 按 source 分组的 commands
const commandsBySource = computed(() => {
  const grouped: Record<string, SkillFile[]> = {
    project: [],
    global: [],
    plugin: [],
  };
  for (const cmd of filteredCommands.value) {
    if (grouped[cmd.source]) {
      grouped[cmd.source].push(cmd);
    }
  }
  return grouped;
});

const filteredSkillTree = computed(() => {
  if (!skillTreeData.value) return [] as ProjectTreeNode[];
  const normalized = skillTreeQuery.value.trim().toLowerCase();
  if (!normalized) return skillTreeData.value.tree;

  const matchesNode = (node: ProjectTreeNode): boolean => {
    if (node.name.toLowerCase().includes(normalized)) return true;
    return node.children.some(matchesNode);
  };

  return skillTreeData.value.tree.filter(matchesNode);
});

// MCP 服务器
const mcpServers = computed(() => mcpStore.serverList);
const mcpSourceScope = computed(() => mcpStore.sourceScope);
const mcpTogglingIds = ref<Set<string>>(new Set());

// Plugin 列表（模拟数据）
const plugins = ref([
  { id: 1, name: 'Git Integration', description: 'Git 版本控制集成', version: '1.0.0', enabled: true, icon: RefreshIcon },
  { id: 2, name: 'File Watcher', description: '文件变化监控', version: '0.9.0', enabled: true, icon: EyeIcon },
  { id: 3, name: 'Theme Manager', description: '主题管理器', version: '1.2.0', enabled: false, icon: PaintBrushIcon }
]);

// MCP 对话框状态
const showMcpDialog = ref(false);
const editingMcpServer = ref<McpServer | null>(null);
const mcpEditMode = ref<'form' | 'json'>('form');
const mcpJsonError = ref<string | null>(null);

// MCP 表单数据
const mcpForm = ref({
  id: '',
  name: '',
  description: '',
  serverType: 'stdio' as 'stdio' | 'http' | 'sse',
  command: '',
  args: '',
  url: '',
  env: '{}',
  headers: '{}',
});

// Tab 配置
const tabs = [
  { id: 'command' as TabType, label: '命令', icon: CommandIcon },
  { id: 'skill' as TabType, label: '技能', icon: ZapIcon },
  { id: 'mcp' as TabType, label: 'MCP 服务', icon: Plug01Icon },
];
const visibleTabs = computed(() => tabs);
const projectCommandGroupLabel = computed(() => (
  isWorkspaceMode.value ? 'WORKSPACE' : 'PROJECT'
));
const isAnyPanelResizing = computed(() => activeResizeTarget.value !== null);

const RESIZE_HANDLE_WIDTH = 10;
const PANEL_WIDTH_CONSTRAINTS = {
  commandList: { min: 220, minRemaining: 420 },
  skillList: { min: 220, minRemaining: 460 },
  skillTree: { min: 220, minRemaining: 380 },
} as const;

function getResizeContainerWidth(target: 'commandList' | 'skillList' | 'skillTree') {
  if (target === 'commandList') {
    return commandMainRef.value?.getBoundingClientRect().width ?? 0;
  }
  if (target === 'skillList') {
    return skillMainRef.value?.getBoundingClientRect().width ?? 0;
  }
  return skillEditorLayoutRef.value?.getBoundingClientRect().width ?? 0;
}

function getPanelWidthRef(target: 'commandList' | 'skillList' | 'skillTree') {
  if (target === 'commandList') return commandListWidth;
  if (target === 'skillList') return skillListWidth;
  return skillTreeWidth;
}

function clampPanelWidth(target: 'commandList' | 'skillList' | 'skillTree', width: number) {
  const constraints = PANEL_WIDTH_CONSTRAINTS[target];
  const containerWidth = getResizeContainerWidth(target);

  if (containerWidth <= 0) {
    return Math.max(width, constraints.min);
  }

  const maxWidth = Math.max(
    constraints.min,
    containerWidth - constraints.minRemaining - RESIZE_HANDLE_WIDTH
  );

  return Math.min(Math.max(width, constraints.min), maxWidth);
}

function syncResizablePanels() {
  commandListWidth.value = clampPanelWidth('commandList', commandListWidth.value);
  skillListWidth.value = clampPanelWidth('skillList', skillListWidth.value);
  skillTreeWidth.value = clampPanelWidth('skillTree', skillTreeWidth.value);
}

function onPanelResize(event: MouseEvent) {
  const target = activeResizeTarget.value;
  if (!target) return;

  const nextWidth = resizeStartWidth.value + (event.clientX - resizeStartX.value);
  getPanelWidthRef(target).value = clampPanelWidth(target, nextWidth);
}

function stopPanelResize() {
  activeResizeTarget.value = null;
  document.body.style.cursor = '';
  document.body.style.userSelect = '';
  document.removeEventListener('mousemove', onPanelResize);
  document.removeEventListener('mouseup', stopPanelResize);
}

function startPanelResize(target: 'commandList' | 'skillList' | 'skillTree', event: MouseEvent) {
  activeResizeTarget.value = target;
  resizeStartX.value = event.clientX;
  resizeStartWidth.value = getPanelWidthRef(target).value;
  document.body.style.cursor = 'col-resize';
  document.body.style.userSelect = 'none';
  document.addEventListener('mousemove', onPanelResize);
  document.addEventListener('mouseup', stopPanelResize);
}

function resetSelectionState() {
  selectedCommand.value = null;
  commandContent.value = '';
  commandLoading.value = false;
  commandSaved.value = false;

  selectedSkill.value = null;
  skillContent.value = '';
  skillLoading.value = false;
  skillSaved.value = false;
  skillTreeData.value = null;
  skillTreeError.value = '';
  selectedSkillTreeFile.value = '';
  selectedSkillRootPath.value = '';
  skillFileSize.value = 0;
}

async function loadExtensions() {
  if (isWorkspaceMode.value && !workspacePath.value) {
    commandFiles.value = [];
    skillFiles.value = [];
    extensionsError.value = '未提供工作区路径';
    return;
  }

  extensionsLoading.value = true;
  extensionsError.value = '';

  try {
    const result = await skillsStore.loadSkills({
      workspacePath: workspacePath.value,
    });
    commandFiles.value = result.commands;
    skillFiles.value = result.skills;
  } catch (e) {
    commandFiles.value = [];
    skillFiles.value = [];
    extensionsError.value = e instanceof Error ? e.message : String(e);
    console.error('[Extensions] Failed to load extensions:', e);
  } finally {
    extensionsLoading.value = false;
  }
}

// 切换 Tab
function switchTab(tab: TabType) {
  activeTab.value = tab;
  if (tab === 'command' || tab === 'skill') {
    slashesStore.initBuiltinCommands();
  } else if (tab === 'mcp') {
    void mcpStore.loadServers(workspacePath.value);
    initMcpJsonConfig();
  }
}

watch(
  [() => props.mode, () => props.workspacePath],
  () => {
    resetSelectionState();
    if (activeTab.value === 'mcp') {
      void mcpStore.loadServers(workspacePath.value).then(() => {
        initMcpJsonConfig();
      });
    }
    void loadExtensions();
  },
  { immediate: true }
);

watch(activeTab, () => {
  requestAnimationFrame(() => {
    syncResizablePanels();
  });
});

watch(
  mcpServers,
  () => {
    if (activeTab.value === 'mcp') {
      initMcpJsonConfig();
    }
  },
  { deep: true }
);

watch(
  [extensionsLoading, filteredCommands, filteredSkills, skillTreeData],
  () => {
    requestAnimationFrame(() => {
      syncResizablePanels();
    });
  }
);

// 初始化 MCP JSON 文本
function initMcpJsonText(server?: McpServer) {
  if (server) {
    const config: Record<string, unknown> = {};
    if (server.server.type && server.server.type !== 'stdio') {
      config.type = server.server.type;
    }
    if (server.server.command) {
      config.command = server.server.command;
    }
    if (server.server.args && server.server.args.length > 0) {
      config.args = server.server.args;
    }
    if (server.server.url) {
      config.url = server.server.url;
    }
    if (server.server.env && Object.keys(server.server.env).length > 0) {
      config.env = server.server.env;
    }
    if (server.server.headers && Object.keys(server.server.headers).length > 0) {
      config.headers = server.server.headers;
    }
    mcpJsonText.value = JSON.stringify(config, null, 2);
  } else {
    mcpJsonText.value = '{\n  "command": "",\n  "args": []\n}';
  }
}

function getParentDirectory(path: string): string {
  const normalized = path.replace(/\\/g, '/');
  const index = normalized.lastIndexOf('/');
  return index >= 0 ? normalized.slice(0, index) : normalized;
}

function getFilename(path: string): string {
  const normalized = path.replace(/\\/g, '/');
  const index = normalized.lastIndexOf('/');
  return index >= 0 ? normalized.slice(index + 1) : normalized;
}

function updateLocalEntry(filePath: string, content: string) {
  const firstLine = content.split('\n')[0]?.trim() || '';
  const description = firstLine.startsWith('#')
    ? firstLine.replace(/^#+\s*/, '')
    : firstLine;

  const update = (items: SkillFile[]) => {
    const entry = items.find(item => item.filePath === filePath);
    if (!entry) {
      return;
    }
    entry.content = content;
    if (description) {
      entry.description = description;
    }
  };

  update(commandFiles.value);
  update(skillFiles.value);
}

async function loadSkillTree(skill: SkillFile) {
  const rootPath = getParentDirectory(skill.filePath);
  selectedSkillRootPath.value = rootPath;
  skillTreeLoading.value = true;
  skillTreeError.value = '';

  try {
    const response = await invoke<ProjectTreeResponse>('read_project_tree', {
      path: rootPath,
      depth: 5,
      max_entries: 4000,
    });

    skillTreeData.value = response;
    skillExpandedPaths.value = new Set(
      response.tree.filter((node) => node.is_dir).slice(0, 6).map((node) => node.path)
    );
  } catch (e) {
    skillTreeData.value = null;
    skillExpandedPaths.value = new Set();
    skillTreeError.value = e instanceof Error ? e.message : '读取技能目录失败';
  } finally {
    skillTreeLoading.value = false;
  }
}

async function openSkillTreeFile(filePath: string) {
  if (!selectedSkillRootPath.value || !selectedSkill.value) return;

  skillLoading.value = true;
  try {
    const response = await invoke<ProjectFileResponse>('read_project_file', {
      rootPath: selectedSkillRootPath.value,
      filePath,
    });

    selectedSkillTreeFile.value = filePath;
    skillContent.value = response.content;
    skillFileSize.value = response.size;
    skillSaved.value = false;
  } catch (e) {
    console.error('Failed to load skill file:', e);
    alert(`读取技能文件失败: ${e instanceof Error ? e.message : String(e)}`);
  } finally {
    skillLoading.value = false;
  }
}

function toggleSkillTreePath(path: string) {
  const next = new Set(skillExpandedPaths.value);
  if (next.has(path)) {
    next.delete(path);
  } else {
    next.add(path);
  }
  skillExpandedPaths.value = next;
}

// 添加 MCP 服务器
function addMcpServer() {
  editingMcpServer.value = null;
  mcpForm.value = {
    id: '',
    name: '',
    description: '',
    serverType: 'stdio',
    command: '',
    args: '',
    url: '',
    env: '{}',
    headers: '{}',
  };
  mcpEditMode.value = 'form';
  mcpJsonError.value = null;
  initMcpJsonText();
  showMcpDialog.value = true;
}

// 编辑 MCP 服务器
function editMcpServer(server: McpServer) {
  editingMcpServer.value = server;
  mcpForm.value = {
    id: server.id,
    name: server.name || '',
    description: server.description || '',
    serverType: server.server.type || 'stdio',
    command: server.server.command || '',
    args: server.server.args?.join('\n') || '',
    url: server.server.url || '',
    env: server.server.env ? JSON.stringify(server.server.env, null, 2) : '{}',
    headers: server.server.headers ? JSON.stringify(server.server.headers, null, 2) : '{}',
  };
  mcpEditMode.value = 'form';
  mcpJsonError.value = null;
  initMcpJsonText(server);
  showMcpDialog.value = true;
}

function getMcpServerStatus(server: McpServer): McpServerStatusInfo | undefined {
  return mcpStore.getServerStatus(server.id);
}

function getMcpServerStatusText(server: McpServer): string {
  if (!server.enabled) return '已禁用';

  switch (getMcpServerStatus(server)?.status) {
    case 'connected':
      return '已连接';
    case 'failed':
      return '连接失败';
    case 'pending':
      return '连接中';
    case 'disabled':
      return '已禁用';
    default:
      return mcpStore.statusLoading ? '检测中' : '未知';
  }
}

function getMcpServerStatusClass(server: McpServer): string {
  if (!server.enabled) return 'disabled';

  switch (getMcpServerStatus(server)?.status) {
    case 'connected':
      return 'connected';
    case 'failed':
      return 'failed';
    case 'pending':
      return 'pending';
    case 'disabled':
      return 'disabled';
    default:
      return 'unknown';
  }
}

function getMcpServerStatusTitle(server: McpServer): string {
  const status = getMcpServerStatus(server);
  return status?.error
    ? `${getMcpServerStatusText(server)}：${status.error}`
    : getMcpServerStatusText(server);
}

async function toggleMcpServer(server: McpServer, enabled: boolean) {
  const next = new Set(mcpTogglingIds.value);
  next.add(server.id);
  mcpTogglingIds.value = next;

  try {
    const result = await mcpStore.toggleServer(server.id, enabled, workspacePath.value);
    if (!result.success) {
      alert(`切换失败: ${result.error}`);
    }
  } finally {
    const updated = new Set(mcpTogglingIds.value);
    updated.delete(server.id);
    mcpTogglingIds.value = updated;
  }
}

// 保存 MCP 服务器
async function saveMcpServer() {
  mcpJsonError.value = null;

  // 如果是 JSON 模式
  if (mcpEditMode.value === 'json') {
    try {
      const parsed = JSON.parse(mcpJsonText.value);
      if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
        mcpJsonError.value = 'JSON 必须是一个对象';
        return;
      }

      // 验证必要字段
      if (!mcpForm.value.id.trim()) {
        mcpJsonError.value = '服务器名称不能为空';
        return;
      }

      const config = parsed as Record<string, unknown>;
      const server: McpServer = {
        id: mcpForm.value.id,
        name: mcpForm.value.name || mcpForm.value.id,
        description: mcpForm.value.description,
        server: {
          type: (config.type as 'stdio' | 'http' | 'sse') || 'stdio',
          command: config.command as string | undefined,
          args: config.args as string[] | undefined,
          url: config.url as string | undefined,
          env: config.env as Record<string, string> | undefined,
          headers: config.headers as Record<string, string> | undefined,
        },
        apps: { claude: true, codex: false, gemini: false },
        enabled: editingMcpServer.value?.enabled ?? true,
      };

      const result = await mcpStore.upsertServer(server, workspacePath.value);
      if (result.success) {
        showMcpDialog.value = false;
        mcpTab.value = 'list';
      } else {
        alert('保存失败: ' + result.error);
      }
    } catch {
      mcpJsonError.value = 'JSON 配置格式无效';
      return;
    }
    return;
  }

  // Form 模式验证
  if (!mcpForm.value.id.trim()) {
    mcpJsonError.value = '服务器名称不能为空';
    return;
  }

  if (mcpForm.value.serverType === 'stdio') {
    if (!mcpForm.value.command.trim()) {
      mcpJsonError.value = 'stdio 类型服务器需要填写命令';
      return;
    }
  } else {
    if (!mcpForm.value.url.trim()) {
      mcpJsonError.value = 'SSE/HTTP 类型服务器需要填写 URL';
      return;
    }
  }

  // 解析 env
  let env: Record<string, string> | undefined;
  try {
    const parsed = JSON.parse(mcpForm.value.env);
    if (typeof parsed === 'object' && parsed !== null && !Array.isArray(parsed)) {
      env = Object.keys(parsed).length > 0 ? parsed : undefined;
    } else {
      mcpJsonError.value = '环境变量必须是 JSON 对象';
      return;
    }
  } catch {
    mcpJsonError.value = '环境变量 JSON 格式无效';
    return;
  }

  // 解析 headers (仅 SSE/HTTP)
  let headers: Record<string, string> | undefined;
  if (mcpForm.value.serverType !== 'stdio') {
    try {
      const parsed = JSON.parse(mcpForm.value.headers);
      if (typeof parsed === 'object' && parsed !== null && !Array.isArray(parsed)) {
        headers = Object.keys(parsed).length > 0 ? parsed : undefined;
      } else {
        mcpJsonError.value = '请求头必须是 JSON 对象';
        return;
      }
    } catch {
      mcpJsonError.value = '请求头 JSON 格式无效';
      return;
    }
  }

  // 解析 args
  const serverArgs = mcpForm.value.args
    .split('\n')
    .map((s) => s.trim())
    .filter(Boolean);

  const server: McpServer = {
    id: mcpForm.value.id,
    name: mcpForm.value.name || mcpForm.value.id,
    description: mcpForm.value.description,
    server: {
      type: mcpForm.value.serverType,
      command: mcpForm.value.serverType === 'stdio' ? mcpForm.value.command : undefined,
      args: serverArgs.length > 0 ? serverArgs : undefined,
      url: mcpForm.value.serverType !== 'stdio' ? mcpForm.value.url : undefined,
      env,
      headers,
    },
    apps: { claude: true, codex: false, gemini: false },
    enabled: editingMcpServer.value?.enabled ?? true,
  };

  const result = await mcpStore.upsertServer(server, workspacePath.value);
  if (result.success) {
    showMcpDialog.value = false;
    mcpTab.value = 'list';
  } else {
    alert('保存失败: ' + result.error);
  }
}

// 删除 MCP 服务器
async function deleteMcpServer(server: McpServer) {
  if (confirm(`确定要删除 MCP 服务器 "${server.name || server.id}" 吗？`)) {
    await mcpStore.deleteServer(server.id, workspacePath.value);
  }
}

// 获取服务器类型标签
function getServerTypeLabel(type?: string): string {
  const labels: Record<string, string> = {
    stdio: 'stdio',
    http: 'HTTP',
    sse: 'SSE',
  };
  return labels[type || 'stdio'] || 'stdio';
}

// 获取服务器类型图标组件
function getServerTypeIcon(type?: string) {
  const icons: Record<string, unknown> = {
    stdio: ServerStack01Icon,
    http: GlobeIcon,
    sse: Wifi01Icon,
  };
  return icons[type || 'stdio'] || ServerStack01Icon;
}

// 初始化 MCP JSON 配置文本
function initMcpJsonConfig() {
  const servers: Record<string, Record<string, unknown>> = {};
  for (const server of mcpServers.value) {
    const config: Record<string, unknown> = {};
    if (server.server.type && server.server.type !== 'stdio') {
      config.type = server.server.type;
    }
    if (server.server.command) {
      config.command = server.server.command;
    }
    if (server.server.args && server.server.args.length > 0) {
      config.args = server.server.args;
    }
    if (server.server.url) {
      config.url = server.server.url;
    }
    if (server.server.env && Object.keys(server.server.env).length > 0) {
      config.env = server.server.env;
    }
    if (server.server.headers && Object.keys(server.server.headers).length > 0) {
      config.headers = server.server.headers;
    }
    servers[server.id] = config;
  }
  mcpJsonText.value = JSON.stringify(servers, null, 2);
}

// 保存 MCP JSON 配置
async function saveMcpJsonConfig() {
  try {
    const parsed = JSON.parse(mcpJsonText.value);
    if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
      mcpJsonError.value = 'JSON 必须是一个对象';
      return;
    }

    // 逐个添加或更新服务器
    for (const [id, spec] of Object.entries(parsed)) {
      if (typeof spec !== 'object' || spec === null) {
        continue;
      }
      const serverSpec = spec as Record<string, unknown>;
      const existingServer = mcpServers.value.find((server) => server.id === id);
      const server: McpServer = {
        id,
        name: id,
        server: {
          type: (serverSpec.type as 'stdio' | 'http' | 'sse') || 'stdio',
          command: serverSpec.command as string | undefined,
          args: serverSpec.args as string[] | undefined,
          url: serverSpec.url as string | undefined,
        },
        apps: { claude: true, codex: false, gemini: false },
        enabled: existingServer?.enabled ?? true,
      };
      await mcpStore.upsertServer(server, workspacePath.value);
    }
  } catch {
    mcpJsonError.value = 'JSON 配置格式无效';
  }
}

// 切换到 JSON 编辑模式时更新 JSON 文本
function switchMcpEditMode(mode: 'form' | 'json') {
  mcpEditMode.value = mode;
  mcpJsonError.value = null;
  if (mode === 'json') {
    // 构建当前表单配置为 JSON
    const currentConfig: Record<string, unknown> = {};
    if (mcpForm.value.serverType !== 'stdio') {
      currentConfig.type = mcpForm.value.serverType;
      if (mcpForm.value.url) currentConfig.url = mcpForm.value.url;
    } else {
      if (mcpForm.value.command) currentConfig.command = mcpForm.value.command;
    }
    const argsArr = mcpForm.value.args.split('\n').map(s => s.trim()).filter(Boolean);
    if (argsArr.length > 0) currentConfig.args = argsArr;
    try {
      const envParsed = JSON.parse(mcpForm.value.env);
      if (Object.keys(envParsed).length > 0) currentConfig.env = envParsed;
    } catch { /* ignore */ }
    try {
      const headersParsed = JSON.parse(mcpForm.value.headers);
      if (Object.keys(headersParsed).length > 0) currentConfig.headers = headersParsed;
    } catch { /* ignore */ }
    mcpJsonText.value = JSON.stringify(currentConfig, null, 2);
  }
}

// 选择 Skill
async function selectSkill(skill: SkillFile) {
  selectedSkill.value = skill;
  skillTreeQuery.value = '';
  await loadSkillTree(skill);
  await openSkillTreeFile(getFilename(skill.filePath));
  skillViewMode.value = 'edit';
  skillSaved.value = false;
}

// 选择 Command
async function selectCommand(cmd: SkillFile) {
  selectedCommand.value = cmd;
  commandLoading.value = true;
  try {
    commandContent.value = await skillsStore.fetchSkillContent(cmd.filePath);
  } catch (e) {
    console.error('Failed to load command content:', e);
    commandContent.value = '';
  } finally {
    commandLoading.value = false;
  }
  commandViewMode.value = 'edit';
  commandSaved.value = false;
}

// 保存 Command
async function saveCommand() {
  if (!selectedCommand.value) return;
  commandSaving.value = true;
  try {
    await skillsStore.saveSkill(selectedCommand.value.filePath, commandContent.value);
    updateLocalEntry(selectedCommand.value.filePath, commandContent.value);
    commandSaved.value = true;
    setTimeout(() => {
      commandSaved.value = false;
    }, 2000);
  } catch (e) {
    console.error('Failed to save command:', e);
  } finally {
    commandSaving.value = false;
  }
}

// 删除 Command
function requestDeleteCommand(command: SkillFile) {
  if (command.source === 'plugin') {
    return;
  }

  pendingDeleteType.value = 'command';
  pendingDeleteItem.value = command;
  showDeleteConfirmDialog.value = true;
}

function requestDeleteSkill(skill: SkillFile) {
  pendingDeleteType.value = 'skill';
  pendingDeleteItem.value = skill;
  showDeleteConfirmDialog.value = true;
}

function closeDeleteConfirmDialog() {
  showDeleteConfirmDialog.value = false;
  pendingDeleteItem.value = null;
  pendingDeleteType.value = null;
}

async function confirmDelete() {
  const item = pendingDeleteItem.value;
  const type = pendingDeleteType.value;

  if (!item || !type) {
    closeDeleteConfirmDialog();
    return;
  }

  try {
    if (type === 'command') {
      await skillsStore.deleteCommand(item.filePath);
    } else {
      await skillsStore.deleteSkill(item.filePath);
    }
    await loadExtensions();
  } catch (e) {
    console.error(`Failed to delete ${type}:`, e);
    return;
  }

  if (type === 'command' && selectedCommand.value?.filePath === item.filePath) {
    selectedCommand.value = null;
    commandContent.value = '';
  }

  if (type === 'skill' && selectedSkill.value?.filePath === item.filePath) {
    selectedSkill.value = null;
    skillContent.value = '';
    skillTreeData.value = null;
    skillTreeError.value = '';
    selectedSkillTreeFile.value = '';
    selectedSkillRootPath.value = '';
  }

  closeDeleteConfirmDialog();
}

// 保存 Skill
async function saveSkill() {
  if (!selectedSkill.value || !selectedSkillTreeFile.value || !selectedSkillRootPath.value) return;
  skillSaving.value = true;
  try {
    await invoke<ProjectFileResponse>('write_project_file', {
      rootPath: selectedSkillRootPath.value,
      filePath: selectedSkillTreeFile.value,
      content: skillContent.value,
    });
    updateLocalEntry(selectedSkill.value.filePath, skillContent.value);

    if (selectedSkillTreeFile.value === getFilename(selectedSkill.value.filePath)) {
      await loadExtensions();
      const refreshed = skillFiles.value.find(skill => skill.filePath === selectedSkill.value?.filePath);
      if (refreshed) {
        selectedSkill.value = refreshed;
      }
    }

    skillSaved.value = true;
    setTimeout(() => {
      skillSaved.value = false;
    }, 2000);
  } catch (e) {
    console.error('Failed to save skill:', e);
  } finally {
    skillSaving.value = false;
  }
}

// 创建 Skill
async function createSkill() {
  if (!newSkillName.value.trim()) return;
  skillCreatePending.value = true;
  try {
    const skillName = newSkillName.value.trim();
    const content = `---\nname: ${skillName}\ndescription: 新建技能\n---\n\n# ${skillName}\n\nWrite your skill prompt here.`;
    const scope = isWorkspaceMode.value ? 'project' : 'global';
    const created = await skillsStore.createSkill(skillName, content, scope, {
      workspacePath: workspacePath.value,
    });
    await loadExtensions();
    const createdSkill = skillFiles.value.find(skill => skill.filePath === created.filePath) || created;
    await selectSkill(createdSkill);
  } catch (e) {
    console.error('Failed to create skill:', e);
    alert(`创建技能失败: ${e instanceof Error ? e.message : String(e)}`);
  } finally {
    skillCreatePending.value = false;
  }
  closeSkillCreateDialog();
}

function closeSkillCreateDialog() {
  showSkillCreate.value = false;
  skillCreateMode.value = 'menu';
  newSkillName.value = '';
  skillCreatePending.value = false;
}

function openSkillCreateDialog() {
  showSkillCreate.value = true;
  skillCreateMode.value = 'menu';
}

function openSkillWriteForm() {
  skillCreateMode.value = 'write';
}

function showImportSuccessToast(message: string) {
  skillToastMessage.value = message;
  showSkillToast.value = true;

  if (skillToastTimer) {
    clearTimeout(skillToastTimer);
  }

  skillToastTimer = setTimeout(() => {
    showSkillToast.value = false;
    skillToastTimer = null;
  }, 2400);
}

async function importSkillFolder() {
  skillCreatePending.value = true;
  try {
    const selected = await open({
      directory: true,
      multiple: false,
      title: '选择技能文件夹（需包含 SKILL.md）',
    });

    if (!selected || typeof selected !== 'string') {
      return;
    }

    const scope = isWorkspaceMode.value ? 'project' : 'global';
    const created = await skillsStore.importSkillFolder(selected, scope, {
      workspacePath: workspacePath.value,
    });
    await loadExtensions();
    const importedSkill = skillFiles.value.find(skill => skill.filePath === created.filePath) || created;
    await selectSkill(importedSkill);
    showImportSuccessToast(`已导入技能：${importedSkill.name}`);
    closeSkillCreateDialog();
  } catch (e) {
    console.error('Failed to import skill folder:', e);
    alert(`导入技能文件夹失败: ${e instanceof Error ? e.message : String(e)}`);
  } finally {
    skillCreatePending.value = false;
  }
}

async function createCommand() {
  if (!newCommandName.value.trim()) return;
  try {
    const commandName = newCommandName.value.trim();
    const content = `# ${commandName}\n\nWrite your command prompt here.`;
    const scope = isWorkspaceMode.value ? 'project' : newCommandScope.value;
    const created = await skillsStore.createCommand(commandName, content, scope, {
      workspacePath: workspacePath.value,
    });
    await loadExtensions();
    const createdCommand = commandFiles.value.find(command => command.filePath === created.filePath) || created;
    await selectCommand(createdCommand);
  } catch (e) {
    console.error('Failed to create command:', e);
  }
  showCommandCreate.value = false;
  newCommandName.value = '';
  newCommandScope.value = isWorkspaceMode.value ? 'project' : 'global';
}

function handleCommandKeyDown(e: KeyboardEvent) {
  if (e.key === 'Tab') {
    e.preventDefault();
    const textarea = e.target as HTMLTextAreaElement;
    const start = textarea.selectionStart;
    const end = textarea.selectionEnd;
    commandContent.value = commandContent.value.substring(0, start) + '  ' + commandContent.value.substring(end);
    requestAnimationFrame(() => {
      textarea.selectionStart = start + 2;
      textarea.selectionEnd = start + 2;
    });
  }
  if ((e.metaKey || e.ctrlKey) && e.key === 's') {
    e.preventDefault();
    saveCommand();
  }
}

// Skill 编辑器键盘事件
function handleSkillKeyDown(e: KeyboardEvent) {
  if (e.key === 'Tab') {
    e.preventDefault();
    const textarea = e.target as HTMLTextAreaElement;
    const start = textarea.selectionStart;
    const end = textarea.selectionEnd;
    skillContent.value = skillContent.value.substring(0, start) + '  ' + skillContent.value.substring(end);
    requestAnimationFrame(() => {
      textarea.selectionStart = start + 2;
      textarea.selectionEnd = start + 2;
    });
  }
  // Ctrl/Cmd + S 保存
  if ((e.metaKey || e.ctrlKey) && e.key === 's') {
    e.preventDefault();
    saveSkill();
  }
}

// 切换插件启用状态
function togglePlugin(plugin: typeof plugins.value[0]) {
  plugin.enabled = !plugin.enabled;
}

// 初始化
slashesStore.initBuiltinCommands();
void mcpStore.loadServers(workspacePath.value);

onMounted(() => {
  window.addEventListener('resize', syncResizablePanels);
  requestAnimationFrame(() => {
    syncResizablePanels();
  });
});

onBeforeUnmount(() => {
  stopPanelResize();
  window.removeEventListener('resize', syncResizablePanels);
});
</script>

<template>
  <div class="extensions-view" :class="{ 'is-resizing': isAnyPanelResizing }">
    <transition name="toast-fade">
      <div v-if="showSkillToast" class="skill-toast skill-toast-success">
        {{ skillToastMessage }}
      </div>
    </transition>

    <!-- Tab 导航 -->
    <div class="tabs-header">
      <div class="tabs-nav">
        <button
          v-for="tab in visibleTabs"
          :key="tab.id"
          :class="['tab-btn', { active: activeTab === tab.id }]"
          @click="switchTab(tab.id)"
        >
          <HugeiconsIcon :icon="tab.icon" class="tab-icon" />
          <span class="tab-label">{{ tab.label }}</span>
        </button>
      </div>
    </div>

    <!-- Tab 内容 -->
    <div class="tab-content">
      <!-- 命令 Tab -->
      <div v-if="activeTab === 'command'" class="tab-panel command-panel">
        <div v-if="extensionsLoading" class="extensions-feedback">加载扩展中...</div>
        <div v-if="extensionsError" class="extensions-feedback error">{{ extensionsError }}</div>

        <!-- Header -->
        <div class="command-header-bar">
          <div class="panel-header-left">
            <h3 class="command-title">Commands</h3>
            <span class="command-count">{{ commands.length }} {{ isWorkspaceMode ? '个工作区命令' : 'commands' }}</span>
          </div>
          <button class="btn-primary btn-sm" @click="showCommandCreate = true">
            <HugeiconsIcon :icon="PlusSignIcon" class="btn-icon-sm" />
            新建命令
          </button>
        </div>

        <!-- Main content -->
        <div ref="commandMainRef" class="command-main">
          <!-- Left: command list -->
          <div class="command-list-container" :style="{ width: `${commandListWidth}px` }">
            <!-- Search -->
            <div class="command-search">
              <HugeiconsIcon :icon="Search01Icon" class="search-icon" />
              <input
                v-model="commandSearch"
                type="text"
                placeholder="Search commands..."
                class="command-search-input"
              />
            </div>

            <!-- List -->
            <div class="command-list-items">
              <div v-if="filteredCommands.length === 0" class="command-empty">
                <HugeiconsIcon :icon="CommandIcon" class="empty-icon-lg" />
                <p>{{ commandSearch ? 'No commands match your search' : 'No commands yet' }}</p>
                <button v-if="!commandSearch" class="btn-secondary btn-sm" @click="showCommandCreate = true">
                  <HugeiconsIcon :icon="PlusSignIcon" class="btn-icon-sm" />
                  Create one
                </button>
              </div>

              <!-- Project commands -->
              <div v-if="commandsBySource.project.length > 0" class="command-group">
                <div class="group-header">{{ projectCommandGroupLabel }}</div>
                <div v-for="cmd in commandsBySource.project" :key="cmd.filePath"
                  :class="['command-item', { selected: selectedCommand?.name === cmd.name && selectedCommand?.filePath === cmd.filePath }]"
                  @click="selectCommand(cmd)"
                >
                  <HugeiconsIcon :icon="CommandIcon" class="command-item-icon" />
                  <div class="command-item-content">
                    <div class="command-item-name">/{{ cmd.name }}</div>
                    <div class="command-item-desc">{{ cmd.description || '无描述' }}</div>
                  </div>
                    <button
                      type="button"
                      class="skill-delete-btn"
                      @click.stop.prevent="requestDeleteCommand(cmd)"
                      title="删除命令"
                    >
                    <HugeiconsIcon :icon="Delete02Icon" class="icon-xs" />
                  </button>
                </div>
              </div>

              <!-- Global commands -->
              <div v-if="commandsBySource.global.length > 0" class="command-group">
                <div class="group-header">GLOBAL</div>
                <div v-for="cmd in commandsBySource.global" :key="cmd.filePath"
                  :class="['command-item', { selected: selectedCommand?.name === cmd.name && selectedCommand?.filePath === cmd.filePath }]"
                  @click="selectCommand(cmd)"
                >
                  <HugeiconsIcon :icon="CommandIcon" class="command-item-icon" />
                  <div class="command-item-content">
                    <div class="command-item-name">/{{ cmd.name }}</div>
                    <div class="command-item-desc">{{ cmd.description || '无描述' }}</div>
                  </div>
                    <button
                      type="button"
                      class="skill-delete-btn"
                      @click.stop.prevent="requestDeleteCommand(cmd)"
                      title="删除命令"
                    >
                    <HugeiconsIcon :icon="Delete02Icon" class="icon-xs" />
                  </button>
                </div>
              </div>

              <!-- Plugin commands -->
              <div v-if="commandsBySource.plugin.length > 0" class="command-group">
                <div class="group-header">PLUGINS</div>
                <div v-for="cmd in commandsBySource.plugin" :key="cmd.filePath"
                  :class="['command-item', { selected: selectedCommand?.name === cmd.name && selectedCommand?.filePath === cmd.filePath }]"
                  @click="selectCommand(cmd)"
                >
                  <HugeiconsIcon :icon="CommandIcon" class="command-item-icon" />
                  <div class="command-item-content">
                    <div class="command-item-name">/{{ cmd.name }}</div>
                    <div class="command-item-desc">{{ cmd.description || '无描述' }}</div>
                    <span v-if="cmd.pluginName" class="command-plugin-badge">{{ cmd.pluginName }}</span>
                  </div>
                </div>
              </div>
            </div>
          </div>

          <div
            class="panel-resizer"
            :class="{ 'is-resizing': activeResizeTarget === 'commandList' }"
            @mousedown.prevent="startPanelResize('commandList', $event)"
          ></div>

          <!-- Right: editor panel -->
          <div class="command-detail-container">
            <div v-if="selectedCommand" class="command-editor">
              <!-- Toolbar -->
              <div class="command-editor-toolbar">
                <div class="command-editor-info">
                  <span class="command-editor-name">/{{ selectedCommand.name }}</span>
                  <span :class="['command-source-badge', selectedCommand.source]">
                    {{ selectedCommand.source }}
                    <span v-if="selectedCommand.pluginName">({{ selectedCommand.pluginName }})</span>
                  </span>
                </div>
                <div class="command-editor-actions">
                  <!-- View mode toggles -->
                  <button
                    :class="['toolbar-btn', { active: commandViewMode === 'edit' }]"
                    @click="commandViewMode = 'edit'"
                    title="Edit"
                  >
                    <HugeiconsIcon :icon="Edit02Icon" class="icon-sm" />
                  </button>
                  <button
                    :class="['toolbar-btn', { active: commandViewMode === 'preview' }]"
                    @click="commandViewMode = 'preview'"
                    title="Preview"
                  >
                    <HugeiconsIcon :icon="EyeIcon" class="icon-sm" />
                  </button>
                  <button
                    :class="['toolbar-btn', { active: commandViewMode === 'split' }]"
                    @click="commandViewMode = 'split'"
                    title="Split"
                  >
                    <HugeiconsIcon :icon="LayoutTwoColumnIcon" class="icon-sm" />
                  </button>

                  <div class="toolbar-divider"></div>

                  <!-- Save -->
                  <button
                    class="toolbar-btn save-btn"
                    @click="saveCommand"
                    :disabled="commandSaving"
                  >
                    <HugeiconsIcon v-if="commandSaving" :icon="Loading02Icon" class="icon-sm animate-spin" />
                    <HugeiconsIcon v-else :icon="FloppyDiskIcon" class="icon-sm" />
                    {{ commandSaving ? 'Saving' : commandSaved ? 'Saved' : 'Save' }}
                  </button>

                  <button
                    type="button"
                    v-if="selectedCommand.source !== 'plugin'"
                    class="toolbar-btn"
                    @click.stop.prevent="requestDeleteCommand(selectedCommand)"
                    title="删除命令"
                  >
                    <HugeiconsIcon :icon="Delete02Icon" class="icon-sm" />
                  </button>
                </div>
              </div>

              <!-- Content area -->
              <div class="command-editor-content">
                <div v-if="commandViewMode === 'edit'" class="command-textarea-wrapper">
                  <textarea
                    v-if="!commandLoading"
                    v-model="commandContent"
                    class="command-textarea"
                    placeholder="Write your command prompt in Markdown..."
                    @keydown="handleCommandKeyDown"
                  ></textarea>
                  <div v-else class="command-loading">
                    <HugeiconsIcon :icon="Loading02Icon" class="icon-sm animate-spin" />
                    <span>Loading...</span>
                  </div>
                </div>
                <div v-else-if="commandViewMode === 'preview'" class="command-preview">
                  <div class="command-preview-content">
                    <p class="command-preview-placeholder">Command content preview</p>
                  </div>
                </div>
                <div v-else-if="commandViewMode === 'split'" class="command-split">
                  <div class="command-textarea-wrapper">
                    <textarea
                      v-if="!commandLoading"
                      v-model="commandContent"
                      class="command-textarea"
                      placeholder="Write your command prompt in Markdown..."
                      @keydown="handleCommandKeyDown"
                    ></textarea>
                    <div v-else class="command-loading">
                      <HugeiconsIcon :icon="Loading02Icon" class="icon-sm animate-spin" />
                      <span>Loading...</span>
                    </div>
                  </div>
                  <div class="command-preview">
                    <div class="command-preview-content">
                      <p class="command-preview-placeholder">Command content preview</p>
                    </div>
                  </div>
                </div>
              </div>

              <!-- Footer -->
              <div class="command-editor-footer">
                <span class="command-file-path">{{ selectedCommand.filePath }}</span>
              </div>
            </div>

            <!-- Empty state -->
            <div v-else class="command-detail-empty">
              <HugeiconsIcon :icon="CommandIcon" class="empty-icon-lg" />
              <p class="empty-title">No command selected</p>
              <p class="empty-desc">Select a command from the list or create a new one</p>
              <button class="btn-secondary btn-sm" @click="showCommandCreate = true">
                <HugeiconsIcon :icon="PlusSignIcon" class="btn-icon-sm" />
                新建命令
              </button>
            </div>
          </div>
        </div>

        <div v-if="showCommandCreate" class="dialog-overlay" @click.self="showCommandCreate = false">
          <div class="dialog dialog-sm">
            <div class="dialog-header">
              <h3>新建命令</h3>
              <button class="close-btn" @click="showCommandCreate = false">×</button>
            </div>
            <div class="dialog-body">
              <div class="form-group">
                <label>命令名称</label>
                <input
                  v-model="newCommandName"
                  type="text"
                  placeholder="my-command"
                  class="form-input"
                  @keydown.enter="createCommand"
                />
              </div>
            </div>
            <div class="dialog-footer">
              <button class="btn-secondary" @click="showCommandCreate = false">取消</button>
              <button class="btn-primary" @click="createCommand" :disabled="!newCommandName.trim()">创建</button>
            </div>
          </div>
        </div>
      </div>

      <!-- Skill Tab -->
      <div v-if="activeTab === 'skill'" class="tab-panel skill-panel">
        <div v-if="extensionsLoading" class="extensions-feedback">加载扩展中...</div>
        <div v-if="extensionsError" class="extensions-feedback error">{{ extensionsError }}</div>

        <!-- Header -->
        <div class="skill-header-bar">
          <h3 class="skill-title">Skills</h3>
          <button class="btn-primary btn-sm" @click="openSkillCreateDialog">
            <HugeiconsIcon :icon="PlusSignIcon" class="btn-icon-sm" />
            New Skill
          </button>
        </div>

        <!-- Main content: left list + right editor -->
        <div ref="skillMainRef" class="skill-main">
          <!-- Left: skill list -->
          <div class="skill-list-container" :style="{ width: `${skillListWidth}px` }">
            <!-- Search -->
            <div class="skill-search">
              <HugeiconsIcon :icon="Search01Icon" class="search-icon" />
              <input
                v-model="skillSearch"
                type="text"
                placeholder="Search skills..."
                class="skill-search-input"
              />
            </div>

            <!-- List -->
            <div class="skill-list-items">
              <div v-if="filteredSkills.length === 0" class="skill-empty">
                <HugeiconsIcon :icon="ZapIcon" class="empty-icon-lg" />
                <p>{{ skillSearch ? 'No skills match your search' : 'No skills yet' }}</p>
                <button v-if="!skillSearch" class="btn-secondary btn-sm" @click="openSkillCreateDialog">
                  <HugeiconsIcon :icon="PlusSignIcon" class="btn-icon-sm" />
                  Create one
                </button>
              </div>

              <div v-for="skill in filteredSkills" :key="skill.filePath"
                :class="['skill-item', { selected: selectedSkill?.filePath === skill.filePath }]"
                @click="selectSkill(skill)"
              >
                <HugeiconsIcon :icon="ZapIcon" class="skill-item-icon" />
                <div class="skill-item-content">
                  <div class="skill-item-name">/{{ skill.name }}</div>
                  <div class="skill-item-desc">{{ skill.description || '无描述' }}</div>
                </div>
                <button
                  type="button"
                  class="skill-delete-btn"
                  @click.stop.prevent="requestDeleteSkill(skill)"
                  title="删除技能"
                >
                  <HugeiconsIcon :icon="Delete02Icon" class="icon-xs" />
                </button>
              </div>
            </div>
          </div>

          <div
            class="panel-resizer"
            :class="{ 'is-resizing': activeResizeTarget === 'skillList' }"
            @mousedown.prevent="startPanelResize('skillList', $event)"
          ></div>

          <!-- Right: editor -->
          <div class="skill-editor-container">
            <div v-if="selectedSkill" class="skill-editor">
              <!-- Toolbar -->
              <div class="skill-editor-toolbar">
                <div class="skill-editor-info">
                  <span class="skill-editor-name">/{{ selectedSkill.name }}</span>
                  <span :class="['command-source-badge', selectedSkill.source]">{{ selectedSkill.source }}</span>
                </div>
                <div class="skill-editor-actions">
                  <!-- View mode toggles -->
                  <button
                    :class="['toolbar-btn', { active: skillViewMode === 'edit' }]"
                    @click="skillViewMode = 'edit'"
                    title="Edit"
                  >
                    <HugeiconsIcon :icon="Edit02Icon" class="icon-sm" />
                  </button>

                  <div class="toolbar-divider"></div>

                  <!-- Save -->
                  <button
                    class="toolbar-btn save-btn"
                    @click="saveSkill"
                    :disabled="skillSaving"
                  >
                    <HugeiconsIcon v-if="skillSaving" :icon="Loading02Icon" class="icon-sm animate-spin" />
                    <HugeiconsIcon v-else :icon="FloppyDiskIcon" class="icon-sm" />
                    {{ skillSaving ? 'Saving' : skillSaved ? 'Saved' : 'Save' }}
                  </button>

                  <!-- Delete -->
                  <button
                    type="button"
                    class="toolbar-btn"
                    @click.stop.prevent="requestDeleteSkill(selectedSkill)"
                    title="删除技能"
                  >
                    <HugeiconsIcon :icon="Delete02Icon" class="icon-sm" />
                  </button>
                </div>
              </div>

              <!-- Content area -->
              <div class="skill-editor-content">
                <div ref="skillEditorLayoutRef" class="skill-editor-layout">
                  <aside class="skill-tree-panel" :style="{ width: `${skillTreeWidth}px` }">
                    <div class="skill-tree-panel-header">
                      <div>
                        <div class="skill-tree-title">技能文件</div>
                        <div class="skill-tree-subtitle">{{ skillTreeData?.root_name || selectedSkill.name }}</div>
                      </div>
                    </div>

                    <div class="skill-tree-search">
                      <HugeiconsIcon :icon="Search01Icon" class="search-icon" />
                      <input
                        v-model="skillTreeQuery"
                        type="text"
                        placeholder="搜索文件..."
                        class="skill-search-input"
                      />
                    </div>

                    <div v-if="skillTreeLoading" class="skill-tree-state">
                      <HugeiconsIcon :icon="Loading02Icon" class="icon-sm animate-spin" />
                      <span>加载目录中...</span>
                    </div>
                    <div v-else-if="skillTreeError" class="skill-tree-state skill-tree-error">
                      {{ skillTreeError }}
                    </div>
                    <div v-else-if="filteredSkillTree.length === 0" class="skill-tree-state">
                      {{ skillTreeQuery ? '没有匹配的文件' : '暂无可展示文件' }}
                    </div>
                    <div v-else class="skill-tree-list">
                      <ProjectTreeRow
                        v-for="node in filteredSkillTree"
                        :key="node.path"
                        :node="node"
                        :depth="0"
                        :expanded-paths="skillExpandedPaths"
                        :loading-paths="new Set()"
                        :query="skillTreeQuery.trim().toLowerCase()"
                        :selected-path="selectedSkillTreeFile"
                        @toggle="toggleSkillTreePath"
                        @open-file="openSkillTreeFile"
                      />
                    </div>
                  </aside>

                  <div
                    class="panel-resizer panel-resizer-inner"
                    :class="{ 'is-resizing': activeResizeTarget === 'skillTree' }"
                    @mousedown.prevent="startPanelResize('skillTree', $event)"
                  ></div>

                  <div class="skill-editor-main">
                    <div class="skill-current-file-bar">
                      <span class="path-label">当前文件</span>
                      <span class="path-value">{{ selectedSkillTreeFile || getFilename(selectedSkill.filePath) }}</span>
                      <span v-if="skillFileSize > 0" class="skill-file-size">{{ skillFileSize }} B</span>
                    </div>

                    <div v-if="skillViewMode === 'edit'" class="skill-textarea-wrapper">
                      <textarea
                        v-if="!skillLoading"
                        v-model="skillContent"
                        class="skill-textarea"
                        placeholder="Write your skill prompt in Markdown..."
                        @keydown="handleSkillKeyDown"
                      ></textarea>
                      <div v-else class="command-loading">
                        <HugeiconsIcon :icon="Loading02Icon" class="icon-sm animate-spin" />
                        <span>Loading...</span>
                      </div>
                    </div>
                    <div v-else-if="skillViewMode === 'preview'" class="skill-preview">
                      <div class="skill-preview-content">
                        <p class="skill-preview-placeholder">Skill content preview</p>
                        <p class="skill-preview-hint">当前数据源不包含 skill 内容，请通过 Claude CLI 管理 skill 文件</p>
                      </div>
                    </div>
                    <div v-else-if="skillViewMode === 'split'" class="skill-split">
                      <div class="skill-textarea-wrapper">
                        <textarea
                          v-if="!skillLoading"
                          v-model="skillContent"
                          class="skill-textarea"
                          placeholder="Write your skill prompt in Markdown..."
                          @keydown="handleSkillKeyDown"
                        ></textarea>
                        <div v-else class="command-loading">
                          <HugeiconsIcon :icon="Loading02Icon" class="icon-sm animate-spin" />
                          <span>Loading...</span>
                        </div>
                      </div>
                      <div class="skill-preview">
                        <div class="skill-preview-content">
                          <p class="skill-preview-placeholder">Skill content preview</p>
                          <p class="skill-preview-hint">当前数据源不包含 skill 内容</p>
                        </div>
                      </div>
                    </div>
                  </div>
                </div>
              </div>

              <!-- Footer -->
              <div class="skill-editor-footer">
                <span class="skill-file-path">{{ selectedSkillRootPath || selectedSkill.filePath }}</span>
              </div>
            </div>

            <!-- Empty state -->
            <div v-else class="skill-editor-empty">
              <HugeiconsIcon :icon="ZapIcon" class="empty-icon-lg" />
              <p class="empty-title">No skill selected</p>
              <p class="empty-desc">Select a skill from the list or create a new one</p>
              <button class="btn-secondary btn-sm" @click="openSkillCreateDialog">
                <HugeiconsIcon :icon="PlusSignIcon" class="btn-icon-sm" />
                新建技能
              </button>
            </div>
          </div>
        </div>

        <!-- Create Skill Dialog -->
        <div v-if="showSkillCreate" class="dialog-overlay" @click.self="closeSkillCreateDialog">
          <div class="dialog dialog-sm">
            <div class="dialog-header">
              <h3>新建技能</h3>
              <button class="close-btn" @click="closeSkillCreateDialog">×</button>
            </div>
            <div class="dialog-body">
              <template v-if="skillCreateMode === 'menu'">
                <button class="skill-create-option" @click="openSkillWriteForm">
                  <div class="skill-create-option-icon">
                    <HugeiconsIcon :icon="Edit02Icon" class="skill-create-option-svg" />
                  </div>
                  <div>
                    <div class="skill-create-option-title">直接编写技能</div>
                    <div class="skill-create-option-desc">保留当前创建方式，手动输入技能名称并新建</div>
                  </div>
                </button>

                <button class="skill-create-option" @click="importSkillFolder" :disabled="skillCreatePending">
                  <div class="skill-create-option-icon">
                    <HugeiconsIcon :icon="FolderOpenIcon" class="skill-create-option-svg" />
                  </div>
                  <div>
                    <div class="skill-create-option-title">导入文件夹</div>
                    <div class="skill-create-option-desc">选择包含 `SKILL.md` 的技能目录并复制到当前技能目录</div>
                  </div>
                </button>
              </template>

              <div v-else class="form-group">
                <label>技能名称</label>
                <input
                  v-model="newSkillName"
                  type="text"
                  placeholder="my-skill"
                  class="form-input"
                  @keydown.enter="createSkill"
                />
              </div>
            </div>
            <div class="dialog-footer">
              <button
                v-if="skillCreateMode === 'write'"
                class="btn-secondary"
                @click="skillCreateMode = 'menu'"
              >
                返回
              </button>
              <button v-else class="btn-secondary" @click="closeSkillCreateDialog">取消</button>
              <button
                v-if="skillCreateMode === 'write'"
                class="btn-primary"
                @click="createSkill"
                :disabled="!newSkillName.trim() || skillCreatePending"
              >
                {{ skillCreatePending ? '创建中...' : '创建' }}
              </button>
            </div>
          </div>
        </div>

      </div>

      <!-- MCP 服务 Tab -->
      <div v-if="activeTab === 'mcp'" class="tab-panel">
        <div class="panel-header">
          <div class="panel-header-left">
            <h2 class="panel-title">MCP 服务</h2>
            <div class="mcp-panel-meta">
              <span class="panel-count">{{ mcpServers.length }} 个服务器</span>
              <span class="mcp-scope-badge">
                {{ mcpSourceScope === 'project' ? '项目配置' : '全局配置' }}
              </span>
            </div>
          </div>
          <div class="panel-actions">
            <button class="btn-primary btn-with-icon" @click="addMcpServer">
              <HugeiconsIcon :icon="PlusSignIcon" class="btn-icon" />
              新建
            </button>
          </div>
        </div>

        <!-- MCP Tab 切换 -->
        <div class="mcp-tabs">
          <button
            :class="['mcp-tab-btn', { active: mcpTab === 'list' }]"
            @click="mcpTab = 'list'; initMcpJsonConfig()"
          >
            <HugeiconsIcon :icon="GridIcon" class="mcp-tab-icon" />
            服务
          </button>
          <button
            :class="['mcp-tab-btn', { active: mcpTab === 'json' }]"
            @click="mcpTab = 'json'; initMcpJsonConfig()"
          >
            <HugeiconsIcon :icon="Brackets" class="mcp-tab-icon" />
            JSON
          </button>
        </div>

        <div v-if="mcpStore.loading" class="loading-state">
          <span class="loading-icon">⏳</span>
          <p>加载中...</p>
        </div>

        <!-- 服务器列表 -->
        <div v-else-if="mcpTab === 'list'">
          <div v-if="mcpServers.length === 0" class="empty-state">
            <HugeiconsIcon :icon="Plug01Icon" class="empty-icon" />
            <p>暂无 MCP 服务器</p>
            <p class="hint">{{ isWorkspaceMode ? '当前工作区还没有单独的 MCP 配置' : '添加 MCP 服务器以扩展 Claude 的功能' }}</p>
          </div>

          <div v-else class="mcp-list">
            <div
              v-for="server in mcpServers"
              :key="server.id"
              :class="['mcp-card', { disabled: !server.enabled }]"
            >
              <div class="mcp-card-header">
                <div class="mcp-header-left">
                  <HugeiconsIcon :icon="getServerTypeIcon(server.server.type)" class="mcp-type-icon" :class="server.server.type || 'stdio'" />
                  <span class="mcp-name">{{ server.id }}</span>
                  <span class="mcp-type-badge">{{ getServerTypeLabel(server.server.type) }}</span>
                  <span
                    :class="['mcp-status-badge', getMcpServerStatusClass(server)]"
                    :title="getMcpServerStatusTitle(server)"
                  >
                    <span class="mcp-status-dot"></span>
                    {{ getMcpServerStatusText(server) }}
                  </span>
                </div>
                <div class="mcp-header-right" @click.stop>
                  <label class="toggle-switch" :title="server.enabled ? '禁用服务器' : '启用服务器'">
                    <input
                      type="checkbox"
                      :checked="server.enabled"
                      :disabled="mcpTogglingIds.has(server.id)"
                      @change="toggleMcpServer(server, ($event.target as HTMLInputElement).checked)"
                    />
                    <span class="toggle-slider"></span>
                  </label>
                  <button class="icon-btn" @click="editMcpServer(server)" title="编辑">
                    <HugeiconsIcon :icon="Edit01Icon" class="icon-small" />
                  </button>
                  <button class="icon-btn delete" @click="deleteMcpServer(server)" title="删除">
                    <HugeiconsIcon :icon="Delete02Icon" class="icon-small" />
                  </button>
                </div>
              </div>

              <div class="mcp-card-content">
                <code class="mcp-config-display">
                  {{ server.server.url || (server.server.command + ' ' + (server.server.args || []).join(' ')) }}
                </code>

                <div v-if="server.server.args && server.server.args.length > 0" class="mcp-tags-section">
                  <span class="mcp-tags-label">参数:</span>
                  <div class="mcp-tags">
                    <span v-for="(arg, i) in server.server.args" :key="i" class="mcp-tag">{{ arg }}</span>
                  </div>
                </div>

                <div v-if="server.server.env && Object.keys(server.server.env).length > 0" class="mcp-tags-section">
                  <span class="mcp-tags-label">环境变量:</span>
                  <div class="mcp-tags">
                    <span v-for="(_, key) in server.server.env" :key="key" class="mcp-tag">{{ key }}=***</span>
                  </div>
                </div>

                <div v-if="server.server.headers && Object.keys(server.server.headers).length > 0" class="mcp-tags-section">
                  <span class="mcp-tags-label">请求头:</span>
                  <div class="mcp-tags">
                    <span v-for="(_, key) in server.server.headers" :key="key" class="mcp-tag">{{ key }}</span>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>

        <!-- JSON 配置 -->
        <div v-else-if="mcpTab === 'json'" class="mcp-json-content">
          <div class="json-editor-container">
            <div class="json-editor-header">
              <span class="json-editor-label">MCP 服务器配置</span>
              <button class="btn-primary btn-sm" @click="saveMcpJsonConfig">保存配置</button>
            </div>
            <textarea
              v-model="mcpJsonText"
              class="json-editor"
              placeholder='{"server-name": {"command": "npx", "args": ["-y", "@server/name"]}}'
            ></textarea>
            <p v-if="mcpJsonError" class="json-error">{{ mcpJsonError }}</p>
          </div>
        </div>
      </div>

      <!-- 插件 Tab -->
      <div v-if="activeTab === 'plugin'" class="tab-panel">
        <div class="panel-header">
          <h2 class="panel-title">插件列表</h2>
          <button class="btn-primary">+ 安装插件</button>
        </div>
        <div class="plugin-list">
          <div v-for="plugin in plugins" :key="plugin.id" class="plugin-card">
            <HugeiconsIcon :icon="plugin.icon" class="plugin-icon" />
            <div class="plugin-info">
              <h3 class="plugin-name">{{ plugin.name }}</h3>
              <p class="plugin-description">{{ plugin.description }}</p>
              <p class="plugin-version">版本 {{ plugin.version }}</p>
            </div>
            <div class="plugin-actions">
              <button
                :class="['btn-toggle', { active: plugin.enabled }]"
                @click="togglePlugin(plugin)"
              >
                {{ plugin.enabled ? '已启用' : '已禁用' }}
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- MCP 添加/编辑对话框 -->
    <div v-if="showMcpDialog" class="dialog-overlay">
      <div class="dialog dialog-wide dialog-mcp">
        <div class="dialog-header">
          <h3>{{ editingMcpServer ? `编辑 MCP 服务器: ${editingMcpServer.id}` : '添加 MCP 服务器' }}</h3>
        </div>
        <div class="dialog-body">
          <!-- 服务器名称 -->
          <div class="form-group">
            <label>服务器名称</label>
            <input
              v-model="mcpForm.id"
              type="text"
              placeholder="my-mcp-server"
              :disabled="!!editingMcpServer"
              class="form-input"
            />
          </div>

          <!-- 编辑模式切换 -->
          <div class="edit-mode-toggle">
            <span class="edit-mode-label">编辑模式:</span>
            <button
              :class="['mode-btn', { active: mcpEditMode === 'form' }]"
              @click="switchMcpEditMode('form')"
            >
              表单
            </button>
            <button
              :class="['mode-btn', { active: mcpEditMode === 'json' }]"
              @click="switchMcpEditMode('json')"
            >
              JSON
            </button>
          </div>

          <!-- JSON 模式 -->
          <div v-if="mcpEditMode === 'json'" class="form-group mcp-json-form-group">
            <label>服务器配置 (JSON)</label>
            <textarea
              v-model="mcpJsonText"
              class="form-textarea form-mono mcp-json-textarea"
              placeholder='{"command": "npx", "args": ["-y", "@server/name"]}'
            ></textarea>
          </div>

          <!-- 表单模式 -->
          <template v-else>
            <!-- 服务器类型 -->
            <div class="form-group">
              <label>服务器类型</label>
              <div class="server-type-tabs">
                <button
                  :class="['type-tab', { active: mcpForm.serverType === 'stdio' }]"
                  @click="mcpForm.serverType = 'stdio'"
                >
                  <HugeiconsIcon :icon="ServerStack01Icon" class="type-icon" />
                  stdio
                </button>
                <button
                  :class="['type-tab', { active: mcpForm.serverType === 'sse' }]"
                  @click="mcpForm.serverType = 'sse'"
                >
                  <HugeiconsIcon :icon="Wifi01Icon" class="type-icon" />
                  SSE
                </button>
                <button
                  :class="['type-tab', { active: mcpForm.serverType === 'http' }]"
                  @click="mcpForm.serverType = 'http'"
                >
                  <HugeiconsIcon :icon="GlobeIcon" class="type-icon" />
                  HTTP
                </button>
              </div>
            </div>

            <!-- STDIO 类型表单 -->
            <template v-if="mcpForm.serverType === 'stdio'">
              <div class="form-group">
                <label>命令</label>
                <input
                  v-model="mcpForm.command"
                  type="text"
                  placeholder="npx -y @modelcontextprotocol/server-name"
                  class="form-input form-mono"
                />
              </div>
              <div class="form-group">
                <label>参数 (每行一个)</label>
                <textarea
                  v-model="mcpForm.args"
                  class="form-textarea form-mono"
                  placeholder="--flag&#10;value"
                ></textarea>
              </div>
            </template>

            <!-- SSE/HTTP 类型表单 -->
            <template v-else>
              <div class="form-group">
                <label>URL</label>
                <input
                  v-model="mcpForm.url"
                  type="text"
                  :placeholder="mcpForm.serverType === 'sse' ? 'http://localhost:3001/sse' : 'http://localhost:3001'"
                  class="form-input form-mono"
                />
              </div>
              <div class="form-group">
                <label>请求头 (JSON)</label>
                <textarea
                  v-model="mcpForm.headers"
                  class="form-textarea form-mono"
                  placeholder='{"Authorization": "Bearer ..."}'
                ></textarea>
              </div>
            </template>

            <!-- 环境变量 -->
            <div class="form-group">
              <label>环境变量 (JSON)</label>
              <textarea
                v-model="mcpForm.env"
                class="form-textarea form-mono"
                placeholder='{"API_KEY": "..."}'
              ></textarea>
            </div>
          </template>

          <!-- 错误信息 -->
          <p v-if="mcpJsonError" class="form-error">{{ mcpJsonError }}</p>
        </div>
        <div class="dialog-footer">
          <button class="btn-secondary" @click="showMcpDialog = false">取消</button>
          <button class="btn-primary" @click="saveMcpServer">
            {{ editingMcpServer ? '保存修改' : '添加服务器' }}
          </button>
        </div>
      </div>
    </div>

    <div v-if="showDeleteConfirmDialog" class="dialog-overlay" @click.self="closeDeleteConfirmDialog">
      <div class="dialog dialog-sm">
        <div class="dialog-header">
          <h3>{{ pendingDeleteType === 'command' ? '删除命令' : '删除技能' }}</h3>
          <button type="button" class="close-btn" @click="closeDeleteConfirmDialog">×</button>
        </div>
        <div class="dialog-body">
          <p class="delete-confirm-text">
            确认删除{{ pendingDeleteType === 'command' ? '命令' : '技能' }}
            <strong v-if="pendingDeleteItem">/{{ pendingDeleteItem.name }}</strong> 吗？
          </p>
          <p class="delete-confirm-hint">删除后不可恢复。</p>
        </div>
        <div class="dialog-footer">
          <button type="button" class="btn-secondary" @click="closeDeleteConfirmDialog">取消</button>
          <button type="button" class="btn-danger" @click.stop.prevent="confirmDelete">删除</button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.extensions-view {
  height: 100%;
  display: flex;
  flex-direction: column;
  background-color: rgb(248, 250, 252);
  overflow: hidden;
}

.extensions-view.is-resizing {
  cursor: col-resize;
}

.extensions-feedback {
  margin-bottom: 1rem;
  padding: 0.85rem 1rem;
  border-radius: 12px;
  background: #eef2ff;
  color: #4338ca;
  font-size: 0.88rem;
}

.extensions-feedback.error {
  background: #fef2f2;
  color: #b91c1c;
}

/* Tab 导航 */
.tabs-header {
  padding: 0 2rem;
  border-bottom: 1px solid var(--border-color, #e5e7eb);
  background-color: rgb(248, 250, 252);
}

.tabs-nav {
  display: flex;
  gap: 0.5rem;
}

.tab-btn {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.75rem 1rem;
  background: transparent;
  border: none;
  border-bottom: 2px solid transparent;
  color: var(--text-secondary, #6b7280);
  cursor: pointer;
  font-size: 0.875rem;
  font-weight: 500;
  transition: all 0.2s;
}

.tab-btn:hover {
  color: var(--text-primary, #1f2937);
}

.tab-btn.active {
  color: var(--primary-color, #3b82f6);
  border-bottom-color: var(--primary-color, #3b82f6);
}

.tab-icon {
  width: 1rem;
  height: 1rem;
}

/* Tab 内容 */
.tab-content {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
  overflow: hidden;
  background-color: rgb(248, 250, 252);
}

.tab-panel {
  animation: fadeIn 0.2s ease;
  display: flex;
  flex: 1;
  flex-direction: column;
  min-height: 0;
  padding: 1.5rem 2rem;
  box-sizing: border-box;
}

/* MCP JSON 内容区 - 全高布局 */
.mcp-json-content {
  display: flex;
  flex-direction: column;
  min-height: calc(100vh - 250px);
}

@keyframes fadeIn {
  from { opacity: 0; }
  to { opacity: 1; }
}

.panel-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 1rem;
}

.panel-header-left {
  display: flex;
  align-items: baseline;
  gap: 0.75rem;
}

.mcp-panel-meta {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  flex-wrap: wrap;
}

.mcp-scope-badge {
  display: inline-flex;
  align-items: center;
  padding: 0.2rem 0.5rem;
  border-radius: 999px;
  background-color: var(--bg-tertiary, #f3f4f6);
  color: var(--text-secondary, #6b7280);
  font-size: 0.75rem;
  font-weight: 600;
}

.mcp-source-bar {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  min-width: 0;
}

.panel-title {
  margin: 0;
  font-size: 1.25rem;
  font-weight: 600;
  color: var(--text-primary, #1f2937);
}

.panel-count {
  font-size: 0.875rem;
  color: var(--text-secondary, #6b7280);
}

.panel-actions {
  display: flex;
  gap: 0.5rem;
}

/* 按钮样式 */
.btn-primary {
  padding: 0.5rem 1rem;
  background-color: var(--primary-color, #3b82f6);
  color: #ffffff;
  border: none;
  border-radius: 0.375rem;
  font-size: 0.875rem;
  font-weight: 500;
  cursor: pointer;
  transition: background-color 0.2s;
}

.btn-primary:hover {
  background-color: var(--primary-hover, #2563eb);
}

.btn-secondary {
  padding: 0.5rem 1rem;
  background-color: var(--bg-tertiary, #f3f4f6);
  color: var(--text-primary, #1f2937);
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 0.375rem;
  font-size: 0.875rem;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s;
}

.btn-secondary:hover {
  background-color: var(--bg-secondary, #f9fafb);
}

.btn-toggle {
  padding: 0.375rem 0.75rem;
  background-color: var(--bg-tertiary, #f3f4f6);
  color: var(--text-secondary, #6b7280);
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 0.375rem;
  font-size: 0.75rem;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s;
}

.btn-toggle.active {
  background-color: rgba(16, 185, 129, 0.1);
  color: var(--success-color, #10b981);
  border-color: var(--success-color, #10b981);
}

/* 命令/Skill 列表 */
.command-list,
.skill-list {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.command-card,
.skill-card {
  padding: 1rem;
  background-color: var(--bg-secondary, #f9fafb);
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 0.5rem;
}

.command-header,
.skill-header {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin-bottom: 0.5rem;
}

.command-name,
.skill-name {
  font-weight: 600;
  color: var(--text-primary, #1f2937);
}

.command-type-badge,
.skill-type-badge {
  padding: 0.125rem 0.375rem;
  font-size: 0.625rem;
  font-weight: 600;
  text-transform: uppercase;
  background-color: var(--primary-color, #3b82f6);
  color: #ffffff;
  border-radius: 0.25rem;
}

.skill-type-badge {
  background-color: #f59e0b;
}

.command-description,
.skill-description {
  margin: 0;
  font-size: 0.875rem;
  color: var(--text-secondary, #6b7280);
}

.command-category {
  display: inline-block;
  margin-top: 0.5rem;
  padding: 0.125rem 0.5rem;
  font-size: 0.625rem;
  background-color: var(--bg-tertiary, #f3f4f6);
  color: var(--text-secondary, #6b7280);
  border-radius: 0.25rem;
}

/* MCP 服务列表 */
.mcp-list {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 0.75rem;
}

.mcp-card {
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 0.5rem;
  background-color: var(--bg-secondary, #f9fafb);
  overflow: hidden;
}

.mcp-card.disabled {
  opacity: 0.6;
}

.mcp-card-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 0.75rem 1rem;
  cursor: pointer;
}

.mcp-card-header:hover {
  background-color: var(--bg-tertiary, #f3f4f6);
}

.mcp-header-left {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.expand-icon {
  font-size: 0.625rem;
  color: var(--text-secondary, #6b7280);
  transition: transform 0.2s;
}

.expand-icon.expanded {
  transform: rotate(90deg);
}

.mcp-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  background-color: var(--primary-color, #3b82f6);
  color: #ffffff;
  border-radius: 0.375rem;
  font-size: 0.75rem;
  font-weight: 600;
}

.mcp-name {
  font-weight: 600;
  color: var(--text-primary, #1f2937);
}

.mcp-type-badge {
  padding: 0.125rem 0.375rem;
  font-size: 0.625rem;
  font-weight: 600;
  background-color: var(--bg-tertiary, #f3f4f6);
  color: var(--text-secondary, #6b7280);
  border-radius: 0.25rem;
}

/* Toggle 开关 */
.toggle-switch {
  position: relative;
  display: inline-block;
  width: 36px;
  height: 20px;
}

.toggle-switch input {
  opacity: 0;
  width: 0;
  height: 0;
}

.toggle-slider {
  position: absolute;
  cursor: pointer;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background-color: var(--bg-tertiary, #e5e7eb);
  border-radius: 10px;
  transition: 0.2s;
}

.toggle-slider::before {
  position: absolute;
  content: "";
  height: 14px;
  width: 14px;
  left: 3px;
  bottom: 3px;
  background-color: white;
  border-radius: 50%;
  transition: 0.2s;
}

.toggle-switch input:checked + .toggle-slider {
  background-color: var(--success-color, #10b981);
}

.toggle-switch input:checked + .toggle-slider::before {
  transform: translateX(16px);
}

/* MCP 卡片内容 */
.mcp-card-content {
  padding: 0.75rem 1rem;
  border-top: 1px solid var(--border-color, #e5e7eb);
  background-color: var(--bg-primary, #ffffff);
}

.mcp-description {
  margin: 0 0 0.75rem 0;
  font-size: 0.875rem;
  color: var(--text-secondary, #6b7280);
}

.mcp-command,
.mcp-url {
  margin-bottom: 0.5rem;
  font-size: 0.875rem;
}

.mcp-command .label,
.mcp-url .label {
  color: var(--text-secondary, #6b7280);
  margin-right: 0.5rem;
}

.mcp-command code,
.mcp-url code {
  padding: 0.25rem 0.5rem;
  background-color: var(--bg-tertiary, #f3f4f6);
  border-radius: 0.25rem;
  font-family: 'SF Mono', Monaco, Consolas, monospace;
  font-size: 0.75rem;
}

.mcp-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 0.375rem;
  margin-bottom: 0.75rem;
}

.tag {
  padding: 0.125rem 0.5rem;
  font-size: 0.625rem;
  background-color: var(--bg-tertiary, #f3f4f6);
  color: var(--text-secondary, #6b7280);
  border-radius: 0.25rem;
}

.mcp-actions {
  display: flex;
  gap: 0.5rem;
  padding-top: 0.75rem;
  border-top: 1px solid var(--border-color, #e5e7eb);
}

.action-btn {
  padding: 0.375rem 0.75rem;
  background-color: var(--bg-tertiary, #f3f4f6);
  color: var(--text-primary, #1f2937);
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 0.375rem;
  font-size: 0.75rem;
  cursor: pointer;
  transition: all 0.2s;
}

.action-btn:hover {
  background-color: var(--bg-secondary, #f9fafb);
}

.action-btn.delete {
  color: var(--error-color, #ef4444);
}

.action-btn.delete:hover {
  background-color: var(--error-bg, #fef2f2);
}

/* 插件列表 */
.plugin-list {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.plugin-card {
  display: flex;
  align-items: flex-start;
  gap: 1rem;
  padding: 1.25rem;
  background-color: var(--bg-secondary, #f9fafb);
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 0.75rem;
}

.plugin-icon {
  width: 2rem;
  height: 2rem;
  color: var(--text-secondary, #6b7280);
}

.plugin-info {
  flex: 1;
}

.plugin-name {
  margin: 0 0 0.25rem 0;
  font-size: 1rem;
  font-weight: 600;
  color: var(--text-primary, #1f2937);
}

.plugin-description {
  margin: 0 0 0.5rem 0;
  font-size: 0.875rem;
  color: var(--text-secondary, #6b7280);
}

.plugin-version {
  margin: 0;
  font-size: 0.75rem;
  color: var(--text-muted, #9ca3af);
}

.plugin-actions {
  display: flex;
  align-items: center;
}

/* 空状态 */
.empty-state,
.loading-state {
  text-align: center;
  padding: 3rem 1rem;
  color: var(--text-secondary, #6b7280);
}

.empty-icon,
.loading-icon {
  width: 3rem;
  height: 3rem;
  display: block;
  margin-bottom: 1rem;
  opacity: 0.5;
  color: var(--text-muted, #9ca3af);
}

.hint {
  font-size: 0.875rem;
  color: var(--text-muted, #9ca3af);
}

/* 对话框 */
.dialog-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background-color: rgba(0, 0, 0, 0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
}

.dialog {
  width: 480px;
  max-height: 80vh;
  background-color: var(--bg-primary, #ffffff);
  border-radius: 0.75rem;
  box-shadow: 0 20px 25px -5px rgba(0, 0, 0, 0.1);
  overflow: hidden;
}

.dialog-large {
  width: 640px;
}

.dialog-mcp {
  max-height: 90vh;
  display: flex;
  flex-direction: column;
}

.dialog-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 1rem 1.5rem;
  border-bottom: 1px solid var(--border-color, #e5e7eb);
}

.dialog-header h3 {
  margin: 0;
  font-size: 1.125rem;
  font-weight: 600;
}

.close-btn {
  background: none;
  border: none;
  font-size: 1.5rem;
  color: var(--text-secondary, #6b7280);
  cursor: pointer;
}

.dialog-body {
  padding: 1.5rem;
  max-height: 60vh;
  overflow-y: auto;
}

.dialog-mcp .dialog-body {
  flex: 1;
  max-height: none;
  min-height: 0;
}

.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 0.75rem;
  padding: 1rem 1.5rem;
  border-top: 1px solid var(--border-color, #e5e7eb);
}

.form-group {
  margin-bottom: 1rem;
}

.form-group label {
  display: block;
  margin-bottom: 0.375rem;
  font-size: 0.875rem;
  font-weight: 500;
  color: var(--text-primary, #1f2937);
}

.form-group input,
.form-group select {
  width: 100%;
  padding: 0.5rem 0.75rem;
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 0.375rem;
  font-size: 0.875rem;
  background-color: var(--bg-primary, #ffffff);
  color: var(--text-primary, #1f2937);
}

.form-group input:focus,
.form-group select:focus {
  outline: none;
  border-color: var(--primary-color, #3b82f6);
}

.form-group input:disabled {
  background-color: var(--bg-tertiary, #f3f4f6);
  cursor: not-allowed;
}

/* 预设列表 */
.preset-list {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 0.75rem;
}

.preset-card {
  padding: 1rem;
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 0.5rem;
  cursor: pointer;
  transition: all 0.2s;
}

.preset-card:hover {
  border-color: var(--primary-color, #3b82f6);
  background-color: var(--bg-secondary, #f9fafb);
}

.preset-name {
  margin: 0 0 0.375rem 0;
  font-size: 0.875rem;
  font-weight: 600;
  color: var(--text-primary, #1f2937);
}

.preset-description {
  margin: 0 0 0.5rem 0;
  font-size: 0.75rem;
  color: var(--text-secondary, #6b7280);
}

.preset-command {
  display: block;
  padding: 0.25rem 0.5rem;
  font-size: 0.625rem;
  background-color: var(--bg-tertiary, #f3f4f6);
  border-radius: 0.25rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* MCP Tabs */
.mcp-tabs {
  display: flex;
  gap: 0.25rem;
  margin-bottom: 1rem;
  background-color: var(--bg-tertiary, #f3f4f6);
  padding: 0.25rem;
  border-radius: 0.5rem;
}

.mcp-tab-btn {
  display: flex;
  align-items: center;
  gap: 0.375rem;
  padding: 0.5rem 1rem;
  background: transparent;
  border: none;
  border-radius: 0.375rem;
  color: var(--text-secondary, #6b7280);
  font-size: 0.875rem;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s;
}

.mcp-tab-btn:hover {
  color: var(--text-primary, #1f2937);
}

.mcp-tab-btn.active {
  background-color: var(--bg-primary, #ffffff);
  color: var(--text-primary, #1f2937);
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.05);
}

.mcp-tab-icon {
  width: 1rem;
  height: 1rem;
}

/* MCP 卡片样式 */
.mcp-card {
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 0.5rem;
  background-color: var(--bg-secondary, #ffffff);
  overflow: hidden;
  margin-bottom: 0.75rem;
}

.mcp-card-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 0.75rem 1rem;
}

.mcp-header-left {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  flex: 1;
  min-width: 0;
}

.mcp-header-right {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  flex-shrink: 0;
}

.mcp-type-icon {
  width: 1.25rem;
  height: 1.25rem;
  color: var(--text-muted, #6b7280);
}

.mcp-type-icon.stdio {
  color: var(--text-muted, #6b7280);
}

.mcp-type-icon.sse {
  color: #3b82f6;
}

.mcp-type-icon.http {
  color: #10b981;
}

.mcp-name {
  font-weight: 600;
  font-size: 0.875rem;
  color: var(--text-primary, #1f2937);
}

.mcp-type-badge {
  padding: 0.125rem 0.375rem;
  font-size: 0.625rem;
  font-weight: 600;
  background-color: var(--bg-tertiary, #f3f4f6);
  color: var(--text-secondary, #6b7280);
  border-radius: 0.25rem;
}

.mcp-status-badge {
  display: inline-flex;
  align-items: center;
  gap: 0.3125rem;
  padding: 0.125rem 0.375rem;
  font-size: 0.625rem;
  font-weight: 600;
  background-color: rgba(107, 114, 128, 0.12);
  color: var(--text-secondary, #6b7280);
  border-radius: 0.25rem;
}

.mcp-status-badge.connected {
  background-color: rgba(16, 185, 129, 0.12);
  color: var(--success-color, #10b981);
}

.mcp-status-badge.failed {
  background-color: rgba(239, 68, 68, 0.12);
  color: var(--danger-color, #ef4444);
}

.mcp-status-badge.pending,
.mcp-status-badge.unknown {
  background-color: rgba(59, 130, 246, 0.12);
  color: var(--accent-color, #3b82f6);
}

.mcp-status-badge.disabled {
  background-color: rgba(107, 114, 128, 0.12);
  color: var(--text-secondary, #6b7280);
}

.mcp-status-dot {
  width: 0.4375rem;
  height: 0.4375rem;
  border-radius: 999px;
  background-color: currentColor;
  flex-shrink: 0;
}

.mcp-card-content {
  padding: 0.75rem 1rem;
  border-top: 1px solid var(--border-color, #e5e7eb);
  background-color: var(--bg-primary, #f9fafb);
}

.mcp-config-display {
  display: block;
  font-family: 'SF Mono', Monaco, Consolas, monospace;
  font-size: 0.75rem;
  color: var(--text-secondary, #6b7280);
  background-color: var(--bg-tertiary, #f3f4f6);
  padding: 0.5rem;
  border-radius: 0.25rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.mcp-tags-section {
  margin-top: 0.5rem;
}

.mcp-tags-label {
  display: block;
  font-size: 0.75rem;
  color: var(--text-muted, #9ca3af);
  margin-bottom: 0.375rem;
}

.mcp-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 0.375rem;
}

.mcp-tag {
  padding: 0.125rem 0.5rem;
  font-size: 0.625rem;
  font-family: 'SF Mono', Monaco, Consolas, monospace;
  background-color: var(--bg-tertiary, #f3f4f6);
  color: var(--text-secondary, #6b7280);
  border-radius: 0.25rem;
}

/* 图标按钮 */
.icon-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 2rem;
  height: 2rem;
  background: transparent;
  border: none;
  border-radius: 0.375rem;
  cursor: pointer;
  font-size: 0.875rem;
  transition: background-color 0.2s;
}

.icon-btn:hover {
  background-color: var(--bg-tertiary, #f3f4f6);
}

.icon-btn.delete:hover {
  background-color: var(--error-bg, #fef2f2);
}

.icon-small {
  width: 0.875rem;
  height: 0.875rem;
}

.btn-icon {
  width: 0.875rem;
  height: 0.875rem;
}

.btn-with-icon {
  display: flex;
  align-items: center;
  gap: 0.375rem;
}

/* JSON 编辑器 */
.json-editor-container {
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 0.5rem;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}

.json-editor-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 0.75rem 1rem;
  background-color: var(--bg-secondary, #f9fafb);
  border-bottom: 1px solid var(--border-color, #e5e7eb);
}

.json-editor-label {
  font-size: 0.875rem;
  font-weight: 500;
  color: var(--text-primary, #1f2937);
}

.json-editor {
  width: 100%;
  min-height: calc(100vh - 320px);
  padding: 1rem;
  font-family: 'SF Mono', Monaco, Consolas, monospace;
  font-size: 0.8125rem;
  line-height: 1.5;
  border: none;
  background-color: var(--bg-primary, #ffffff);
  color: var(--text-primary, #1f2937);
  resize: vertical;
}

.json-editor:focus {
  outline: none;
}

.json-error {
  margin: 0;
  padding: 0.75rem 1rem;
  font-size: 0.875rem;
  color: var(--error-color, #ef4444);
  background-color: var(--error-bg, #fef2f2);
}

/* 对话框样式 */
.dialog-wide {
  width: 550px;
  max-height: 85vh;
}

.mcp-json-form-group {
  display: flex;
  flex-direction: column;
  min-height: 0;
}

.mcp-json-textarea {
  min-height: 360px;
  height: min(60vh, 560px);
}

.edit-mode-toggle {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin-bottom: 1rem;
}

.edit-mode-label {
  font-size: 0.875rem;
  font-weight: 500;
  color: var(--text-primary, #1f2937);
}

.mode-btn {
  padding: 0.375rem 0.75rem;
  background-color: var(--bg-tertiary, #f3f4f6);
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 0.375rem;
  font-size: 0.875rem;
  cursor: pointer;
  transition: all 0.2s;
}

.mode-btn.active {
  background-color: var(--primary-color, #3b82f6);
  color: #ffffff;
  border-color: var(--primary-color, #3b82f6);
}

.server-type-tabs {
  display: flex;
  gap: 0.25rem;
  background-color: var(--bg-tertiary, #f3f4f6);
  padding: 0.25rem;
  border-radius: 0.5rem;
}

.type-tab {
  display: flex;
  align-items: center;
  gap: 0.375rem;
  flex: 1;
  padding: 0.5rem 0.75rem;
  background: transparent;
  border: none;
  border-radius: 0.375rem;
  font-size: 0.875rem;
  color: var(--text-secondary, #6b7280);
  cursor: pointer;
  transition: all 0.2s;
}

.type-tab:hover {
  color: var(--text-primary, #1f2937);
}

.type-tab.active {
  background-color: var(--bg-primary, #ffffff);
  color: var(--text-primary, #1f2937);
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.05);
}

.type-icon {
  width: 1rem;
  height: 1rem;
}

.form-input,
.form-textarea {
  width: 100%;
  padding: 0.5rem 0.75rem;
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 0.375rem;
  font-size: 0.875rem;
  background-color: var(--bg-primary, #ffffff);
  color: var(--text-primary, #1f2937);
}

.form-input:focus,
.form-textarea:focus {
  outline: none;
  border-color: var(--primary-color, #3b82f6);
}

.form-textarea {
  min-height: 80px;
  resize: vertical;
}

.form-mono {
  font-family: 'SF Mono', Monaco, Consolas, monospace;
}

.form-error {
  margin: 0;
  padding: 0.5rem;
  font-size: 0.875rem;
  color: var(--error-color, #ef4444);
  background-color: var(--error-bg, #fef2f2);
  border-radius: 0.375rem;
}

.btn-sm {
  padding: 0.375rem 0.75rem;
  font-size: 0.8125rem;
}

/* 深色模式 */
@media (prefers-color-scheme: dark) {
  .extensions-view {
    background-color: var(--bg-primary, #111827);
  }

  .tabs-header {
    background-color: var(--bg-primary, #111827);
    border-bottom-color: var(--border-color, #374151);
  }

  .tab-btn {
    color: var(--text-secondary, #9ca3af);
  }

  .tab-btn:hover {
    color: var(--text-primary, #f9fafb);
  }

  .panel-title {
    color: var(--text-primary, #f9fafb);
  }

  .command-card,
  .skill-card,
  .mcp-card,
  .plugin-card {
    background-color: var(--bg-secondary, #1f2937);
    border-color: var(--border-color, #374151);
  }

  .command-name,
  .skill-name,
  .mcp-name,
  .plugin-name {
    color: var(--text-primary, #f9fafb);
  }

  .command-description,
  .skill-description,
  .mcp-description,
  .plugin-description {
    color: var(--text-secondary, #d1d5db);
  }

  .mcp-card-header:hover {
    background-color: var(--bg-tertiary, #374151);
  }

  .mcp-card-content {
    background-color: var(--bg-secondary, #1f2937);
  }

  .dialog {
    background-color: var(--bg-primary, #1f2937);
  }

  .dialog-header,
  .dialog-footer {
    border-color: var(--border-color, #374151);
  }

  .dialog-header h3 {
    color: var(--text-primary, #f9fafb);
  }

  .form-group label {
    color: var(--text-primary, #f9fafb);
  }

  .form-group input,
  .form-group select {
    background-color: var(--bg-secondary, #1f2937);
    border-color: var(--border-color, #374151);
    color: var(--text-primary, #f9fafb);
  }

  .preset-card {
    border-color: var(--border-color, #374151);
  }

  .preset-card:hover {
    background-color: var(--bg-tertiary, #374151);
  }

  /* MCP Tabs */
  .mcp-tabs {
    background-color: var(--bg-tertiary, #374151);
  }

  .mcp-tab-btn {
    color: var(--text-secondary, #9ca3af);
  }

  .mcp-tab-btn:hover {
    color: var(--text-primary, #f9fafb);
  }

  .mcp-tab-btn.active {
    background-color: var(--bg-secondary, #1f2937);
    color: var(--text-primary, #f9fafb);
  }

  /* MCP Card */
  .mcp-card {
    background-color: var(--bg-secondary, #1f2937);
    border-color: var(--border-color, #374151);
  }

  .mcp-card-content {
    background-color: var(--bg-primary, #111827);
    border-color: var(--border-color, #374151);
  }

  .mcp-name {
    color: var(--text-primary, #f9fafb);
  }

  .mcp-type-badge {
    background-color: var(--bg-tertiary, #374151);
    color: var(--text-secondary, #9ca3af);
  }

  .mcp-config-display {
    background-color: var(--bg-tertiary, #374151);
    color: var(--text-secondary, #d1d5db);
  }

  .mcp-tag {
    background-color: var(--bg-tertiary, #374151);
    color: var(--text-secondary, #9ca3af);
  }

  /* JSON Editor */
  .json-editor-container {
    border-color: var(--border-color, #374151);
  }

  .json-editor-header {
    background-color: var(--bg-secondary, #1f2937);
    border-color: var(--border-color, #374151);
  }

  .json-editor-label {
    color: var(--text-primary, #f9fafb);
  }

  .json-editor {
    background-color: var(--bg-primary, #111827);
    color: var(--text-primary, #f9fafb);
  }

  /* Edit Mode */
  .edit-mode-label {
    color: var(--text-primary, #f9fafb);
  }

  .mode-btn {
    background-color: var(--bg-tertiary, #374151);
    border-color: var(--border-color, #374151);
    color: var(--text-secondary, #9ca3af);
  }

  .mode-btn.active {
    background-color: var(--primary-color, #3b82f6);
    color: #ffffff;
  }

  .server-type-tabs {
    background-color: var(--bg-tertiary, #374151);
  }

  .type-tab {
    color: var(--text-secondary, #9ca3af);
  }

  .type-tab:hover {
    color: var(--text-primary, #f9fafb);
  }

  .type-tab.active {
    background-color: var(--bg-secondary, #1f2937);
    color: var(--text-primary, #f9fafb);
  }

  .form-input,
  .form-textarea {
    background-color: var(--bg-secondary, #1f2937);
    border-color: var(--border-color, #374151);
    color: var(--text-primary, #f9fafb);
  }
}

/* ========== Command Tab Styles ========== */
.command-panel,
.skill-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
}

.command-header-bar,
.skill-header-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 1rem;
}

.command-title,
.skill-title {
  font-size: 1.125rem;
  font-weight: 600;
  margin: 0;
  color: var(--text-primary, #1f2937);
}

.command-count,
.skill-count {
  font-size: 0.875rem;
  color: var(--text-secondary, #6b7280);
}

.command-main,
.skill-main {
  display: flex;
  gap: 0.5rem;
  flex: 1;
  min-height: 0;
}

.panel-resizer {
  width: 10px;
  flex-shrink: 0;
  cursor: col-resize;
  position: relative;
  border-radius: 999px;
}

.panel-resizer::before {
  content: '';
  position: absolute;
  top: 0;
  bottom: 0;
  left: 50%;
  width: 1px;
  transform: translateX(-50%);
  background-color: var(--border-color, #dbe3f0);
  transition: background-color 0.15s ease, box-shadow 0.15s ease;
}

.panel-resizer::after {
  content: '';
  position: absolute;
  top: 50%;
  left: 50%;
  width: 4px;
  height: 38px;
  transform: translate(-50%, -50%);
  border-radius: 999px;
  background-color: rgba(148, 163, 184, 0.45);
  transition: background-color 0.15s ease, transform 0.15s ease;
}

.panel-resizer:hover::before,
.panel-resizer.is-resizing::before {
  background-color: var(--primary-color, #3b82f6);
  box-shadow: 0 0 0 1px rgba(59, 130, 246, 0.12);
}

.panel-resizer:hover::after,
.panel-resizer.is-resizing::after {
  background-color: var(--primary-color, #3b82f6);
  transform: translate(-50%, -50%) scaleY(1.05);
}

/* Left list container */
.command-list-container,
.skill-list-container {
  width: 280px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 0.5rem;
  overflow: hidden;
  background-color: var(--bg-primary, #ffffff);
}

.command-search,
.skill-search {
  position: relative;
  padding: 0.5rem;
  border-bottom: 1px solid var(--border-color, #e5e7eb);
}

.search-icon {
  position: absolute;
  left: 0.75rem;
  top: 50%;
  transform: translateY(-50%);
  width: 0.875rem;
  height: 0.875rem;
  color: var(--text-secondary, #6b7280);
}

.command-search-input,
.skill-search-input {
  width: 100%;
  padding: 0.5rem 0.5rem 0.5rem 2rem;
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 0.375rem;
  font-size: 0.875rem;
  background-color: var(--bg-primary, #ffffff);
  color: var(--text-primary, #1f2937);
}

.command-search-input:focus,
.skill-search-input:focus {
  outline: none;
  border-color: var(--primary-color, #3b82f6);
}

.command-list-items,
.skill-list-items {
  flex: 1;
  overflow-y: auto;
  padding: 0.25rem;
}

.command-empty,
.skill-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 2rem 1rem;
  text-align: center;
  color: var(--text-secondary, #6b7280);
}

.empty-icon-lg {
  width: 2rem;
  height: 2rem;
  opacity: 0.4;
  margin-bottom: 0.5rem;
}

.command-item,
.skill-item {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.5rem 0.75rem;
  border-radius: 0.375rem;
  cursor: pointer;
  transition: background-color 0.15s;
}

.command-item:hover,
.skill-item:hover {
  background-color: var(--bg-tertiary, #f3f4f6);
}

.command-item.selected,
.skill-item.selected {
  background-color: var(--primary-color, #3b82f6);
  color: #ffffff;
}

.command-item-icon,
.skill-item-icon {
  width: 1rem;
  height: 1rem;
  flex-shrink: 0;
  color: var(--text-secondary, #6b7280);
}

.command-item.selected .command-item-icon,
.skill-item.selected .skill-item-icon {
  color: #ffffff;
}

.command-item-content,
.skill-item-content {
  flex: 1;
  min-width: 0;
}

.command-item-name,
.skill-item-name {
  font-size: 0.875rem;
  font-weight: 500;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.command-item-desc,
.skill-item-desc {
  font-size: 0.75rem;
  color: var(--text-secondary, #6b7280);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.command-item.selected .command-item-desc,
.skill-item.selected .skill-item-desc {
  color: rgba(255, 255, 255, 0.8);
}

.command-item-category {
  font-size: 0.625rem;
  padding: 0.125rem 0.375rem;
  background-color: var(--bg-tertiary, #f3f4f6);
  color: var(--text-secondary, #6b7280);
  border-radius: 0.25rem;
  flex-shrink: 0;
}

.command-item.selected .command-item-category {
  background-color: rgba(255, 255, 255, 0.2);
  color: #ffffff;
}

/* Command group header */
.command-group {
  margin-bottom: 0.5rem;
}

.group-header {
  font-size: 0.625rem;
  font-weight: 600;
  color: var(--text-secondary, #6b7280);
  padding: 0.5rem 0.75rem 0.25rem;
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

/* Plugin badge */
.command-plugin-badge {
  display: inline-block;
  font-size: 0.625rem;
  padding: 0.0625rem 0.375rem;
  background-color: rgba(139, 92, 246, 0.1);
  color: #8b5cf6;
  border-radius: 0.25rem;
  margin-top: 0.25rem;
}

/* Command editor */
.command-editor {
  display: flex;
  flex-direction: column;
  height: 100%;
}

.command-editor-toolbar {
  display: flex;
  align-items: center;
  padding: 0.75rem 1rem;
  border-bottom: 1px solid var(--border-color, #e5e7eb);
  flex-shrink: 0;
}

.command-editor-info {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.command-editor-name {
  font-size: 1rem;
  font-weight: 600;
  color: var(--text-primary, #1f2937);
}

.command-source-badge {
  font-size: 0.625rem;
  padding: 0.125rem 0.5rem;
  border-radius: 0.25rem;
  font-weight: 500;
}

.command-source-badge.global {
  background-color: rgba(16, 185, 129, 0.1);
  color: #10b981;
  border: 1px solid rgba(16, 185, 129, 0.3);
}

.command-source-badge.project {
  background-color: rgba(59, 130, 246, 0.1);
  color: #3b82f6;
  border: 1px solid rgba(59, 130, 246, 0.3);
}

.command-source-badge.plugin {
  background-color: rgba(139, 92, 246, 0.1);
  color: #8b5cf6;
  border: 1px solid rgba(139, 92, 246, 0.3);
}

.command-editor-content {
  flex: 1;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}

.command-editor-actions {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  margin-left: auto;
}

.command-textarea-wrapper {
  height: 100%;
}

.command-textarea {
  width: 100%;
  height: 100%;
  padding: 1rem;
  border: none;
  resize: none;
  font-family: 'SF Mono', Monaco, Consolas, monospace;
  font-size: 0.875rem;
  line-height: 1.5;
  background-color: var(--bg-primary, #ffffff);
  color: var(--text-primary, #1f2937);
}

.command-textarea:focus {
  outline: none;
}

.command-preview {
  height: 100%;
  overflow-y: auto;
  padding: 1rem;
  background-color: var(--bg-primary, #ffffff);
}

.command-preview-content {
  max-width: 65ch;
}

.command-preview-placeholder {
  font-size: 0.875rem;
  font-weight: 500;
  color: var(--text-primary, #1f2937);
  margin-bottom: 0.5rem;
}

.command-split {
  display: flex;
  height: 100%;
}

.command-split .command-textarea-wrapper {
  width: 50%;
  border-right: 1px solid var(--border-color, #e5e7eb);
}

.command-split .command-preview {
  width: 50%;
}

.command-editor-footer {
  padding: 0.375rem 1rem;
  border-top: 1px solid var(--border-color, #e5e7eb);
  flex-shrink: 0;
}

.command-file-path {
  font-size: 0.75rem;
  font-family: 'SF Mono', Monaco, Consolas, monospace;
  color: var(--text-secondary, #6b7280);
}

.command-loading {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.5rem;
  height: 100%;
  color: var(--text-secondary, #6b7280);
}

.command-editor-desc {
  font-size: 0.875rem;
  color: var(--text-secondary, #6b7280);
  margin-bottom: 1rem;
}

.command-editor-path {
  display: flex;
  align-items: flex-start;
  gap: 0.5rem;
}

.path-label {
  font-size: 0.75rem;
  color: var(--text-secondary, #6b7280);
  flex-shrink: 0;
}

.path-value {
  font-family: 'SF Mono', Monaco, Consolas, monospace;
  font-size: 0.75rem;
  padding: 0.25rem 0.5rem;
  background-color: var(--bg-tertiary, #f3f4f6);
  border-radius: 0.25rem;
  word-break: break-all;
}

.skill-item-content {
  flex: 1;
  min-width: 0;
}

.skill-delete-btn {
  opacity: 1;
  padding: 0.25rem;
  background: transparent;
  border: none;
  border-radius: 0.25rem;
  cursor: pointer;
  color: var(--text-secondary, #6b7280);
  transition: all 0.15s;
}

.skill-delete-btn:hover {
  background-color: rgba(239, 68, 68, 0.1);
  color: #ef4444;
}

/* Right detail/editor container */
.command-detail-container,
.skill-editor-container {
  flex: 1;
  min-width: 0;
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 0.5rem;
  overflow: hidden;
  background-color: var(--bg-primary, #ffffff);
}

.command-detail-empty,
.skill-editor-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 100%;
  text-align: center;
  color: var(--text-secondary, #6b7280);
}

.empty-title {
  font-size: 0.875rem;
  font-weight: 500;
  margin: 0.5rem 0 0.25rem;
  color: var(--text-primary, #1f2937);
}

.empty-desc {
  font-size: 0.75rem;
  margin: 0 0 1rem;
}

.delete-confirm-text {
  font-size: 0.95rem;
  color: var(--text-primary, #1f2937);
  margin: 0 0 0.5rem;
}

.delete-confirm-hint {
  font-size: 0.8125rem;
  color: var(--text-secondary, #6b7280);
  margin: 0;
}

.btn-danger {
  padding: 0.5rem 1rem;
  border: none;
  border-radius: 0.375rem;
  background-color: #ef4444;
  color: #ffffff;
  cursor: pointer;
  transition: background-color 0.15s;
}

.btn-danger:hover {
  background-color: #dc2626;
}

/* Command detail */
.command-detail {
  padding: 1.5rem;
}

.command-detail-header {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin-bottom: 1rem;
}

.command-detail-name {
  font-size: 1.25rem;
  font-weight: 600;
  color: var(--text-primary, #1f2937);
}

.command-badge {
  font-size: 0.625rem;
  padding: 0.125rem 0.5rem;
  background-color: var(--primary-color, #3b82f6);
  color: #ffffff;
  border-radius: 0.25rem;
  font-weight: 600;
  text-transform: uppercase;
}

.command-detail-desc {
  font-size: 0.875rem;
  color: var(--text-secondary, #6b7280);
  margin-bottom: 1rem;
}

.command-detail-category,
.command-detail-hint {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin-bottom: 0.5rem;
  font-size: 0.875rem;
}

.category-label,
.hint-label {
  color: var(--text-secondary, #6b7280);
}

.category-value {
  padding: 0.125rem 0.5rem;
  background-color: var(--bg-tertiary, #f3f4f6);
  border-radius: 0.25rem;
  font-size: 0.75rem;
}

.hint-value {
  font-family: 'SF Mono', Monaco, Consolas, monospace;
  font-size: 0.75rem;
  padding: 0.125rem 0.375rem;
  background-color: var(--bg-tertiary, #f3f4f6);
  border-radius: 0.25rem;
}

.command-detail-usage {
  margin-top: 1.5rem;
  padding-top: 1rem;
  border-top: 1px solid var(--border-color, #e5e7eb);
}

.usage-title {
  font-size: 0.75rem;
  font-weight: 600;
  color: var(--text-secondary, #6b7280);
  margin: 0 0 0.5rem;
  text-transform: uppercase;
}

.usage-example {
  display: block;
  padding: 0.5rem 0.75rem;
  background-color: var(--bg-primary, #ffffff);
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 0.375rem;
  font-family: 'SF Mono', Monaco, Consolas, monospace;
  font-size: 0.875rem;
  color: var(--text-primary, #1f2937);
}

/* Skill editor */
.skill-editor {
  display: flex;
  flex-direction: column;
  height: 100%;
}

.skill-editor-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.5rem 1rem;
  border-bottom: 1px solid var(--border-color, #e5e7eb);
  flex-shrink: 0;
}

.skill-editor-info {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.skill-editor-name {
  font-size: 0.875rem;
  font-weight: 600;
  color: var(--text-primary, #1f2937);
}

.skill-badge {
  font-size: 0.625rem;
  padding: 0.125rem 0.375rem;
  border: 1px solid rgba(59, 130, 246, 0.4);
  border-radius: 0.25rem;
  color: var(--primary-color, #3b82f6);
  font-weight: 500;
}

.skill-editor-actions {
  display: flex;
  align-items: center;
  gap: 0.25rem;
}

.toolbar-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.25rem;
  padding: 0.375rem;
  background: transparent;
  border: none;
  border-radius: 0.25rem;
  cursor: pointer;
  color: var(--text-secondary, #6b7280);
  transition: all 0.15s;
}

.toolbar-btn:hover {
  background-color: var(--bg-tertiary, #f3f4f6);
  color: var(--text-primary, #1f2937);
}

.toolbar-btn.active {
  background-color: var(--bg-tertiary, #e5e7eb);
  color: var(--text-primary, #1f2937);
}

.toolbar-btn.save-btn {
  padding: 0.25rem 0.5rem;
  font-size: 0.75rem;
}

.toolbar-btn.delete-confirm {
  background-color: #ef4444;
  color: #ffffff;
}

.toolbar-divider {
  width: 1px;
  height: 1rem;
  background-color: var(--border-color, #e5e7eb);
  margin: 0 0.25rem;
}

.icon-xs {
  width: 0.75rem;
  height: 0.75rem;
}

.icon-sm {
  width: 0.875rem;
  height: 0.875rem;
}

.skill-editor-content {
  flex: 1;
  min-height: 0;
  overflow: hidden;
}

.skill-editor-layout {
  display: flex;
  height: 100%;
  min-height: 0;
  gap: 0.375rem;
}

.skill-tree-panel {
  min-width: 0;
  display: flex;
  flex-direction: column;
  background-color: var(--bg-secondary, #f9fafb);
}

.skill-tree-panel-header {
  padding: 0.9rem 1rem 0.65rem;
  border-bottom: 1px solid var(--border-color, #e5e7eb);
}

.skill-tree-title {
  font-size: 0.8rem;
  font-weight: 600;
  color: var(--text-primary, #111827);
}

.skill-tree-subtitle {
  margin-top: 0.2rem;
  font-size: 0.75rem;
  color: var(--text-secondary, #6b7280);
  word-break: break-all;
}

.skill-tree-search {
  position: relative;
  padding: 0.75rem;
  border-bottom: 1px solid var(--border-color, #e5e7eb);
}

.skill-tree-search .search-icon {
  position: absolute;
  left: 1.35rem;
  top: 50%;
  transform: translateY(-50%);
}

.skill-tree-search .skill-search-input {
  padding-left: 2rem;
}

.skill-tree-list {
  flex: 1;
  min-height: 0;
  overflow: auto;
  padding: 0.5rem;
}

.skill-tree-state {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 1rem;
  font-size: 0.8125rem;
  color: var(--text-secondary, #6b7280);
}

.skill-tree-error {
  color: #dc2626;
}

.skill-editor-main {
  flex: 1;
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.panel-resizer-inner {
  width: 8px;
}

.skill-current-file-bar {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.5rem 1rem;
  border-bottom: 1px solid var(--border-color, #e5e7eb);
  background-color: var(--bg-primary, #ffffff);
}

.skill-file-size {
  margin-left: auto;
  font-size: 0.75rem;
  color: var(--text-secondary, #6b7280);
}

.skill-textarea-wrapper {
  height: 100%;
  min-height: 0;
}

.skill-textarea {
  width: 100%;
  height: 100%;
  padding: 1rem;
  border: none;
  resize: none;
  font-family: 'SF Mono', Monaco, Consolas, monospace;
  font-size: 0.875rem;
  line-height: 1.5;
  background-color: var(--bg-primary, #ffffff);
  color: var(--text-primary, #1f2937);
}

.skill-textarea:focus {
  outline: none;
}

.skill-preview {
  height: 100%;
  overflow-y: auto;
  padding: 1rem;
  background-color: var(--bg-primary, #ffffff);
}

.skill-preview-content {
  max-width: 65ch;
}

.skill-preview-placeholder {
  font-size: 0.875rem;
  color: var(--text-secondary, #6b7280);
}

.skill-preview-hint {
  font-size: 0.75rem;
  color: var(--text-secondary, #6b7280);
  margin-top: 0.5rem;
}

.skill-split {
  display: flex;
  height: 100%;
}

.skill-split .skill-textarea-wrapper {
  width: 50%;
  border-right: 1px solid var(--border-color, #e5e7eb);
}

.skill-split .skill-preview {
  width: 50%;
}

.skill-editor-footer {
  padding: 0.375rem 1rem;
  border-top: 1px solid var(--border-color, #e5e7eb);
  flex-shrink: 0;
}

.skill-file-path {
  font-size: 0.75rem;
  color: var(--text-secondary, #6b7280);
}

/* Create skill dialog */
.skill-create-option {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 0.875rem;
  padding: 1rem;
  margin-bottom: 0.75rem;
  background-color: var(--bg-primary, #ffffff);
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 0.75rem;
  text-align: left;
  cursor: pointer;
  transition: all 0.15s ease;
}

.skill-create-option:hover:not(:disabled) {
  border-color: var(--primary-color, #3b82f6);
  box-shadow: 0 4px 14px rgba(59, 130, 246, 0.08);
}

.skill-create-option:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.skill-create-option-icon {
  width: 2.75rem;
  height: 2.75rem;
  display: flex;
  align-items: center;
  justify-content: center;
  background-color: var(--bg-tertiary, #f3f4f6);
  border-radius: 0.75rem;
  flex-shrink: 0;
}

.skill-create-option-svg {
  width: 1.25rem;
  height: 1.25rem;
  color: var(--text-secondary, #6b7280);
}

.skill-create-option-title {
  font-size: 0.95rem;
  font-weight: 600;
  color: var(--text-primary, #111827);
}

.skill-create-option-desc {
  margin-top: 0.2rem;
  font-size: 0.8rem;
  line-height: 1.45;
  color: var(--text-secondary, #6b7280);
}

.scope-options {
  display: flex;
  gap: 0.5rem;
}

.scope-btn {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.375rem;
  padding: 0.5rem 1rem;
  background-color: var(--bg-tertiary, #f3f4f6);
  border: 1px solid var(--border-color, #e5e7eb);
  border-radius: 0.375rem;
  cursor: pointer;
  font-size: 0.875rem;
  color: var(--text-secondary, #6b7280);
  transition: all 0.15s;
}

.scope-btn:hover {
  background-color: var(--bg-secondary, #f9fafb);
}

.scope-btn.active {
  background-color: var(--primary-color, #3b82f6);
  border-color: var(--primary-color, #3b82f6);
  color: #ffffff;
}

.scope-icon {
  width: 1rem;
  height: 1rem;
}

/* Buttons */
.btn-sm {
  padding: 0.375rem 0.75rem;
  font-size: 0.75rem;
}

.btn-icon-sm {
  width: 0.75rem;
  height: 0.75rem;
}

.dialog-sm {
  max-width: 400px;
}

.skill-toast {
  position: fixed;
  top: 1.25rem;
  right: 1.25rem;
  z-index: 1200;
  max-width: 320px;
  padding: 0.75rem 1rem;
  border-radius: 0.75rem;
  box-shadow: 0 12px 30px rgba(0, 0, 0, 0.18);
  font-size: 0.875rem;
  font-weight: 500;
}

.skill-toast-success {
  background-color: #16a34a;
  color: #ffffff;
}

.toast-fade-enter-active,
.toast-fade-leave-active {
  transition: opacity 0.2s ease, transform 0.2s ease;
}

.toast-fade-enter-from,
.toast-fade-leave-to {
  opacity: 0;
  transform: translateY(-8px);
}

.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 0.5rem;
  padding: 1rem;
  border-top: 1px solid var(--border-color, #e5e7eb);
}

.animate-spin {
  animation: spin 1s linear infinite;
}

@keyframes spin {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}

/* Dark mode support for command/skill */
@media (prefers-color-scheme: dark) {
  .command-list-container,
  .skill-list-container,
  .command-detail-container,
  .skill-editor-container {
    background-color: var(--bg-secondary, #1f2937);
    border-color: var(--border-color, #374151);
  }

  .command-search-input,
  .skill-search-input {
    background-color: var(--bg-primary, #1f2937);
    border-color: var(--border-color, #374151);
    color: var(--text-primary, #f9fafb);
  }

  .command-item:hover,
  .skill-item:hover {
    background-color: var(--bg-tertiary, #374151);
  }

  .command-empty,
  .skill-empty,
  .command-detail-empty,
  .skill-editor-empty {
    color: var(--text-secondary, #9ca3af);
  }

  .empty-title {
    color: var(--text-primary, #f9fafb);
  }

  .command-detail-name,
  .skill-editor-name {
    color: var(--text-primary, #f9fafb);
  }

  .command-detail-desc,
  .command-item-desc,
  .skill-item-desc {
    color: var(--text-secondary, #9ca3af);
  }

  .skill-textarea {
    background-color: var(--bg-primary, #1f2937);
    color: var(--text-primary, #f9fafb);
  }

  .skill-preview {
    background-color: var(--bg-primary, #1f2937);
  }

  .skill-tree-panel,
  .skill-tree-panel-header,
  .skill-tree-search,
  .skill-current-file-bar {
    background-color: var(--bg-secondary, #1f2937);
    border-color: var(--border-color, #374151);
  }

  .skill-tree-title {
    color: var(--text-primary, #f9fafb);
  }

  .skill-tree-subtitle,
  .skill-tree-state,
  .skill-file-size {
    color: var(--text-secondary, #9ca3af);
  }

  .skill-split .skill-textarea-wrapper {
    border-color: var(--border-color, #374151);
  }

  .panel-resizer::before {
    background-color: var(--border-color, #334155);
  }

  .panel-resizer::after {
    background-color: rgba(100, 116, 139, 0.55);
  }

  .usage-example {
    background-color: var(--bg-tertiary, #374151);
    border-color: var(--border-color, #4b5563);
    color: var(--text-primary, #f9fafb);
  }

  .category-value,
  .hint-value {
    background-color: var(--bg-tertiary, #374151);
  }

  .toolbar-btn:hover {
    background-color: var(--bg-tertiary, #374151);
  }

  .toolbar-btn.active {
    background-color: var(--bg-primary, #1f2937);
  }

  .scope-btn {
    background-color: var(--bg-tertiary, #374151);
    border-color: var(--border-color, #4b5563);
    color: var(--text-secondary, #9ca3af);
  }

  .scope-btn:hover {
    background-color: var(--bg-secondary, #1f2937);
  }

  .skill-create-option {
    background-color: var(--bg-secondary, #1f2937);
    border-color: var(--border-color, #374151);
  }

  .skill-create-option-icon,
  .scope-toggle-btn {
    background-color: var(--bg-tertiary, #374151);
    border-color: var(--border-color, #4b5563);
  }

  .skill-create-option-title {
    color: var(--text-primary, #f9fafb);
  }

  .skill-create-option-desc {
    color: var(--text-secondary, #9ca3af);
  }
}

@media (max-width: 960px) {
  .mcp-list {
    grid-template-columns: 1fr;
  }

  .command-main,
  .skill-main,
  .skill-editor-layout {
    gap: 0.5rem;
  }

  .command-main,
  .skill-main,
  .skill-editor-layout,
  .command-split,
  .skill-split {
    flex-direction: column;
  }

  .command-list-container,
  .skill-list-container,
  .skill-tree-panel {
    width: 100% !important;
  }

  .panel-resizer {
    display: none;
  }

  .skill-tree-panel {
    border-right: none;
    border-bottom: 1px solid var(--border-color, #e5e7eb);
  }

  .command-split .command-textarea-wrapper,
  .skill-split .skill-textarea-wrapper {
    width: 100%;
    border-right: none;
    border-bottom: 1px solid var(--border-color, #e5e7eb);
  }

  .command-split .command-preview,
  .skill-split .skill-preview {
    width: 100%;
  }
}
</style>
