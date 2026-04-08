/**
 * 类型定义 - Companion 复刻项目
 * 包含消息、会话、任务等核心数据结构
 */

// ========== 基础类型 ==========

/**
 * 消息角色
 */
export type MessageRole = 'user' | 'assistant' | 'system';

/**
 * 连接状态
 */
export type ConnectionStatus = 'connecting' | 'connected' | 'disconnected';

/**
 * 会话状态
 */
export type SessionStatus = 'idle' | 'running' | 'compacting' | null;

/**
 * 权限模式 - Claude Code 支持的 4 种模式
 *
 * - default: 默认 - 询问所有权限
 * - acceptEdits: 自动批准编辑类工具
 * - bypassPermissions: 绕过所有权限（yolo 模式）
 * - plan: 计划模式
 */
export type PermissionMode =
  | 'default'              // 默认 - 询问所有权限
  | 'acceptEdits'          // 自动批准编辑类工具
  | 'bypassPermissions'    // 绕过所有权限（yolo 模式）
  | 'plan';                // 计划模式

export type ThinkingLevel = 'off' | 'low' | 'medium' | 'high';

export type ApiProtocol = 'anthropic' | 'openai';

export type AuthType = 'auth_token' | 'api_key' | 'both';

export type UpstreamFormat = 'chat_completions' | 'responses';

export interface ProviderModel {
  model: string;
  modelName: string;
}

export interface ApiProvider {
  id: string;
  name: string;
  homepageUrl?: string;
  baseUrl: string;
  enabled?: boolean;
  apiProtocol: ApiProtocol;
  authType: AuthType;
  apiKey?: string;
  models: ProviderModel[];
  primaryModel?: string;
  extraEnv?: Record<string, string>;
  upstreamFormat?: UpstreamFormat;
  createdAt: number;
  updatedAt: number;
}

export interface ProviderConfigPayload {
  providers: ApiProvider[];
  activeProviderId: string | null;
  inheritSystemConfig?: boolean;
}

export interface SessionProviderEnv {
  baseUrl?: string;
  apiKey?: string;
  apiProtocol?: ApiProtocol;
  authType?: AuthType;
  extraEnv?: Record<string, string>;
  upstreamFormat?: UpstreamFormat;
}

export type IDEConnectionStatus = 'connecting' | 'connected' | 'disconnected' | 'error';

export interface IDEConnectionInfo {
  key: string;
  url: string;
  name: string;
  workspaceFolders: string[];
  port: number;
  isValid: boolean;
  authToken?: string;
  ideRunningInWindows?: boolean;
}

export interface IDESelection {
  filePath?: string;
  text?: string;
  lineCount?: number;
  startLine?: number;
  endLine?: number;
}

export interface IDEConnectionState {
  status: IDEConnectionStatus;
  connectedIde?: IDEConnectionInfo | null;
  selection?: IDESelection | null;
  error?: string | null;
}

/**
 * 任务状态
 */
export type TaskStatus = 'pending' | 'in_progress' | 'completed';

// ========== 内容块类型 ==========

/**
 * 文本内容块
 */
export interface TextBlock {
  type: 'text';
  text: string;
}

/**
 * 工具调用块
 * 注意：后端推送的格式包含 id, name, input 字段
 */
export interface ToolUseBlock {
  type: 'tool_use';
  id: string;
  name: string;
  input: Record<string, unknown>;
  _meta?: {
    toolUseId?: string;
    [key: string]: unknown;
  };
}

/**
 * 工具结果块
 * 注意：后端推送的格式支持 tool_use_id 和 toolUseId 两种字段名
 */
export interface ToolResultBlock {
  type: 'tool_result';
  toolUseId?: string;      // 前端格式
  tool_use_id?: string;    // 后端推送格式
  content: string | Array<{
    type: 'text';
    text: string;
  }>;
  isError?: boolean;
  is_error?: boolean;
}

/**
 * 思考过程块
 * 注意：后端推送的格式为 { type: 'thinking', thinking: string, signature?: string }
 */
export interface ThinkingBlock {
  type: 'thinking';
  thinking?: string;  // 后端推送的字段
  content?: string;   // 兼容前端本地格式
  signature?: string; // 可选的签名
}

/**
 * 图片内容块
 */
export interface ImageBlock {
  type: 'image';
  source: {
    type: 'base64';
    media_type: string;
    data: string;
  };
}

/**
 * 内容块联合类型
 */
export type ContentBlock =
  | TextBlock
  | ToolUseBlock
  | ToolResultBlock
  | ThinkingBlock
  | ImageBlock;

// ========== 消息类型 ==========

/**
 * 文件附件 - 通用文件附件类型
 * 复刻 TOKENICODE 项目的文件附件功能
 */
export interface FileAttachment {
  id: string;
  name: string;
  path: string;       // 保存后的临时路径
  size: number;
  type: string;       // MIME 类型
  isImage: boolean;   // 是否为图片
  preview?: string;   // 图片缩略图的 Base64 URL
}

/**
 * 文件附件信息 - 保存在消息中的附件简化类型
 * 参考 TOKENICODE 项目，仅包含显示所需的核心字段
 */
export interface FileAttachmentInfo {
  name: string;
  path: string;
  isImage: boolean;
  preview?: string;
  originalPath?: string;
}

/**
 * 图片附件（旧版，保留兼容性）
 * @deprecated 建议使用 FileAttachment
 */
export interface ImageAttachment {
  media_type: string;
  data: string;
}

export interface OutgoingMessagePayload {
  content: string;
  contentBlocks?: ContentBlock[];
  transportContent?: string;
  transportContentBlocks?: ContentBlock[];
  attachments?: FileAttachmentInfo[];
}

/**
 * Token 使用统计
 */
export interface TokenUsage {
  inputTokens: number;
  outputTokens: number;
  cacheCreationInputTokens?: number;
  cacheReadInputTokens?: number;
}

/**
 * 子代理信息
 */
export interface AgentInfo {
  description: string;
  agentType: string;
}

/**
 * 工具调用
 */
export interface ToolCall {
  id: string;
  name: string;
  input: Record<string, unknown>;
  isRunning?: boolean;  // 工具是否正在运行
  status?: 'pending' | 'running' | 'completed' | 'error';  // 工具状态
}

/**
 * 工具结果
 */
export interface ToolResult {
  toolUseId: string;
  content: string;
  isError?: boolean;
}

// ========== 工具调用审批类型 ==========

/**
 * 工具调用请求（来自后端的权限审批请求）
 */
export interface ToolUseRequest {
  tool_name: string;
  input: Record<string, unknown>;
  tool_use_id: string;
  request_id: string;
  description?: string;
  blocked_path?: string;
  decision_reason?: string;
}

/**
 * 工具调用状态
 */
export type ToolUseStatus =
  | 'pending'        // 等待审批
  | 'approved'       // 已批准
  | 'rejected'       // 已拒绝
  | 'running'        // 运行中
  | 'completed'      // 已完成
  | 'error';         // 出错

/**
 * 工具调用项（用于消息中显示）
 */
export interface ToolUseItem {
  id: string;
  name: string;
  input: Record<string, unknown>;
  result?: string;
  status: ToolUseStatus;
  isError?: boolean;
  duration?: number;
  requestId?: string;
}

/**
 * 消息接口
 * 注意：后端推送的消息格式支持以下字段：
 * - id, role, content, contentBlocks, timestamp
 * - parentToolUseId / parent_tool_use_id (用于子代理嵌套)
 * - isStreaming, model, stopReason, tokenUsage
 */
export interface Message {
  id: string;
  role: MessageRole;
  content: string;
  contentBlocks?: ContentBlock[];
  images?: ImageAttachment[];
  timestamp?: number;  // 改为可选以保持向后兼容
  checkpointUuid?: string;

  // 文件附件 - 参考 TOKENICODE 实现
  attachments?: FileAttachmentInfo[];

  // 流式相关
  isStreaming?: boolean;

  // 子代理相关 - 支持两种命名格式
  parentToolUseId?: string | null;     // 前端格式
  parent_tool_use_id?: string | null;  // 后端推送格式
  agentInfo?: AgentInfo;

  // 分组相关
  groupId?: string;
  groupType?: string;

  // 工具相关
  toolUse?: ToolCall;
  tool_calls?: ToolCall[];
  toolResult?: ToolResult;
  /** 按 tool_use_id 关联的工具结果（用于将 tool_result 与上一条 assistant 的 tool_use 一起显示） */
  toolResults?: Record<string, string>;
  /** 按 tool_use_id 关联的工具结果错误状态 */
  toolResultErrors?: Record<string, boolean>;

  // 统计信息
  model?: string;
  stopReason?: string | null;
  tokenUsage?: TokenUsage;
  /** 后端发送的字段名是 usage，这里添加别名以兼容 */
  usage?: TokenUsage;
  /** 标记此消息是本轮对话的最后一条，应显示 token 使用量 */
  showTokenUsage?: boolean;
}

// ========== 会话类型 ==========

/**
 * Git 信息
 */
export interface GitInfo {
  branch?: string;
  ahead?: number;
  behind?: number;
  totalLinesAdded?: number;
  totalLinesRemoved?: number;
  isWorktree?: boolean;
  repoRoot?: string;
}

/**
 * 会话状态
 */
export interface SessionState {
  sessionId: string;
  cwd: string;
  permissionMode: PermissionMode;
  previousPermissionMode?: PermissionMode;
  thinkingLevel?: ThinkingLevel;
  providerId?: string | null;
  model?: string | null;
  providerOverrideEnabled?: boolean;

  // Git 信息
  gitBranch?: string;
  gitAhead?: number;
  gitBehind?: number;
  totalLinesAdded?: number;
  totalLinesRemoved?: number;
  isWorktree?: boolean;
  repoRoot?: string;

  commands?: Array<{
    name: string;
    description: string;
    argumentHint?: string | string[];
  }>;
  // 斜杠命令
  slashCommands?: string[];
  skills?: string[];

  // 时间戳
  createdAt: number;
  updatedAt?: number;

  // 归档状态
  archived?: boolean;
}

/**
 * SDK 会话信息
 */
export interface SdkSessionInfo {
  sessionId: string;
  pid?: number;
  state: 'starting' | 'connected' | 'running' | 'exited';
  exitCode?: number | null;
  model?: string;
  permissionMode?: string;
  cwd: string;
  createdAt: number;
  archived?: boolean;
  isWorktree?: boolean;
  repoRoot?: string;
  branch?: string;
}

// ========== 权限类型 ==========

/**
 * 权限请求
 */
export interface PermissionRequest {
  request_id: string;
  type: string;
  description: string;
  session_id?: string;
  params?: Record<string, unknown>;
  tool_use_id?: string;  // 关联的工具调用 ID
}

// ========== 任务类型 ==========

/**
 * 任务项
 */
export interface TaskItem {
  id: string;
  subject: string;
  description: string;
  activeForm?: string;
  status: TaskStatus;
  owner?: string;
  blockedBy?: string[];
  metadata?: Record<string, unknown>;
}

// ========== 消息分组类型 ==========

/**
 * 工具消息组
 */
export interface ToolMessageGroup {
  kind: 'tool_msg_group';
  toolName: string;
  items: Array<{
    id: string;
    name: string;
    input: Record<string, unknown>;
    result?: string;
    isError?: boolean;
  }>;
  firstId: string;
  timestamp?: number;
}

/**
 * 子代理组
 */
export interface SubagentGroup {
  kind: 'subagent';
  taskToolUseId: string;
  description: string;
  agentType: string;
  children: FeedEntry[];
}

/**
 * 消息条目
 */
export interface MessageEntry {
  kind: 'message';
  msg: Message;
}

/**
 * Feed 条目联合类型
 */
export type FeedEntry =
  | MessageEntry
  | ToolMessageGroup
  | SubagentGroup;

// ========== WebSocket 消息类型 ==========

/**
 * 浏览器出站消息
 */
export type BrowserOutgoingMessage =
  | UserMessageOut
  | InterruptMessage
  | SetPermissionModeMessage;

export interface UserMessageOut {
  type: 'user_message';
  content: string;
  session_id: string;
  images?: ImageAttachment[];
}

export interface InterruptMessage {
  type: 'interrupt';
}

export interface SetPermissionModeMessage {
  type: 'set_permission_mode';
  mode: PermissionMode;
}

/**
 * 浏览器入站消息
 */
export type BrowserIncomingMessage =
  | SystemMessage
  | UserMessageIn
  | AssistantMessage
  | StreamEvent
  | ToolUseMessage
  | ToolResultMessage
  | PermissionRequestMessage
  | TaskUpdateMessage
  | SessionStatusMessage
  | GitInfoMessage;

export interface SystemMessage {
  type: 'system';
  message: string;
}

export interface UserMessageIn {
  type: 'user_message';
  content: string;
  timestamp: number;
  images?: ImageAttachment[];
}

export interface AssistantMessage {
  type: 'assistant_message';
  content: string;
  contentBlocks?: ContentBlock[];
  timestamp: number;
}

export interface StreamEvent {
  type: 'stream_event';
  content: string;
  timestamp: number;
}

export interface ToolUseMessage {
  type: 'tool_use';
  id: string;
  name: string;
  input: Record<string, unknown>;
}

export interface ToolResultMessage {
  type: 'tool_result';
  toolUseId: string;
  content: string;
  isError?: boolean;
}

export interface PermissionRequestMessage {
  type: 'permission_request';
  requestId: string;
  permission: PermissionRequest;
}

export interface TaskUpdateMessage {
  type: 'task_update';
  task: TaskItem;
}

export interface SessionStatusMessage {
  type: 'session_status';
  status: SessionStatus;
}

export interface GitInfoMessage {
  type: 'git_info';
  info: GitInfo;
}

// ========== API 响应类型 ==========

/**
 * Git 拉取结果
 */
export interface GitPullResult {
  success: boolean;
  output?: string;
  gitAhead?: number;
  gitBehind?: number;
}

// ========== Worktree 类型 ==========

/**
 * Git 仓库信息
 */
export interface GitRepoInfo {
  repoRoot: string;
  repoName: string;
  currentBranch: string;
  defaultBranch: string;
  isWorktree: boolean;
}

/**
 * Git 分支信息
 */
export interface BranchInfo {
  name: string;
  isCurrent: boolean;
  isRemote: boolean;
  worktreePath?: string;
  ahead: number;
  behind: number;
}

/**
 * Worktree 信息
 */
export interface WorktreeInfo {
  path: string;
  branch: string;
  head: string;
  isMainWorktree: boolean;
  isDirty: boolean;
}

/**
 * Worktree 创建结果
 */
export interface WorktreeCreateResult {
  worktreePath: string;
  branch: string;
  isNew: boolean;
}

/**
 * Worktree 映射
 */
export interface WorktreeMapping {
  sessionId: string;
  repoRoot: string;
  branch: string;
  worktreePath: string;
  createdAt: number;
}

/**
 * 会话列表项
 */
export interface SessionListItem {
  sessionId: string;
  title?: string;
  createdAt: number;
  updatedAt?: number;
  messageCount?: number;
  fileSize?: number;
}

/**
 * 对话/会话项
 */
export interface Conversation {
  id: string;
  projectId?: string;
  title: string;
  timestamp: number;
  messageCount: number;
  size: string;
  pinned?: boolean; // 是否固定到顶部
}

// ========== 统计数据类型 ==========

/**
 * Token 使用数据
 */
export interface UsageData {
  input_tokens: number;
  output_tokens: number;
  cache_write_tokens: number;
  cache_read_tokens: number;
  total_tokens: number;
}

/**
 * 会话摘要
 */
export interface SessionSummary {
  session_id: string;
  timestamp: number;
  model: string;
  usage: UsageData;
  cost: number;
  summary?: string;
}

/**
 * 按模型聚合的统计数据
 */
export interface ModelUsage {
  model: string;
  total_cost: number;
  total_tokens: number;
  input_tokens: number;
  output_tokens: number;
  cache_creation_tokens: number;
  cache_read_tokens: number;
  session_count: number;
}

/**
 * 每日使用数据
 */
export interface DailyUsage {
  date: string;
  sessions: number;
  usage: UsageData;
  cost: number;
  models_used: string[];
}

/**
 * 周数据
 */
export interface WeekData {
  sessions: number;
  cost: number;
  tokens: number;
}

/**
 * 趋势数据
 */
export interface TrendData {
  sessions: number;
  cost: number;
  tokens: number;
}

/**
 * 周对比数据
 */
export interface WeeklyComparison {
  current_week: WeekData;
  last_week: WeekData;
  trends: TrendData;
}

/**
 * 项目完整统计数据
 */
export interface ProjectStatistics {
  project_path: string;
  project_name: string;
  total_sessions: number;
  total_usage: UsageData;
  estimated_cost: number;
  sessions: SessionSummary[];
  daily_usage: DailyUsage[];
  weekly_comparison: WeeklyComparison;
  by_model: ModelUsage[];
  last_updated: number;
}

/**
 * 项目范围
 */
export type ProjectScope = 'current' | 'selected' | 'all';

/**
 * 项目信息
 */
export interface ProjectInfo {
  path: string;
  name: string;
}

/**
 * 日期范围
 */
export type DateRange = '7d' | '30d' | 'all';

/**
 * 模型使用数据（来自后端返回的 modelUsage）
 */
export interface ModelUsageData {
  inputTokens: number;
  outputTokens: number;
  cacheReadInputTokens: number;
  cacheCreationInputTokens: number;
  contextWindow: number;
  maxOutputTokens: number;
  costUSD: number;
  model: string;
}
