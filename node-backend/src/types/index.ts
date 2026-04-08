/**
 * 会话信息
 */
export interface Session {
  id: string;
  created_at: string;
}

/**
 * 消息角色
 */
export type MessageRole = 'user' | 'assistant' | 'system';

/**
 * 媒体来源
 */
export interface MediaSource {
  type: 'image' | 'video' | 'audio' | 'file';
  data?: string;
  url?: string;
  mimeType?: string;
}

/**
 * 内容块
 */
export interface ContentBlock {
  type: 'text' | 'tool_use' | 'tool_result' | 'image';
  text?: string;
  toolUse?: {
    id: string;
    name: string;
    input: Record<string, unknown>;
  };
  toolResult?: {
    toolUseId: string;
    content?: string;
    isError?: boolean;
  };
  mediaSource?: MediaSource;
}

/**
 * 消息内容类型
 */
export type MessageContent = string | ContentBlock[];

/**
 * 工具调用
 */
export interface ToolCall {
  id: string;
  name: string;
  input: Record<string, unknown>;
}

/**
 * 消息
 */
export interface Message {
  id: string;
  role: MessageRole;
  content: MessageContent;
  tool_calls?: ToolCall[];
  created_at: string;
}

/**
 * SDK 状态
 */
export interface SdkStatus {
  initialized: boolean;
  cli_path?: string;
  error?: string;
}

/**
 * 重试配置
 */
export interface RetryConfig {
  maxAttempts: number;
  backoffMs: number;
}

/**
 * JSON-RPC 请求
 */
export interface RpcRequest<T = unknown> {
  jsonrpc: '2.0';
  id: string;
  method: string;
  params?: T;
}

/**
 * JSON-RPC 响应
 */
export interface RpcResponse<T = unknown> {
  jsonrpc: '2.0';
  id: string;
  result?: T;
  error?: RpcError;
}

/**
 * JSON-RPC 错误
 */
export interface RpcError {
  code: number;
  message: string;
  data?: unknown;
}

/**
 * JSON-RPC 通知（单向消息）
 */
export interface RpcNotification<T = unknown> {
  jsonrpc: '2.0';
  method: string;
  params: T;
}

/**
 * JSON-RPC 错误码
 */
export enum RpcErrorCode {
  ParseError = -32700,
  InvalidRequest = -32600,
  MethodNotFound = -32601,
  InvalidParams = -32602,
  InternalError = -32603,
}
