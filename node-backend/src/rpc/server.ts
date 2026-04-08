import { createInterface } from 'readline';
import { RpcRequest, RpcResponse, RpcNotification, RpcErrorCode } from '../types/index.js';

/**
 * RPC 处理器类型
 */
export type RpcHandler<T = unknown, R = unknown> = (params: T) => Promise<R>;

/**
 * JSON-RPC 2.0 服务器
 * 通过 stdin/stdout 与 Rust 进程通信
 */
export class JsonRpcServer {
  private handlers: Map<string, RpcHandler<unknown, unknown>>;
  private rl: ReturnType<typeof createInterface>;

  constructor() {
    this.handlers = new Map();

    // 创建 readline 接口用于 stdin/stdout 通信
    this.rl = createInterface({
      input: process.stdin,
      output: process.stdout,
      terminal: false,
    });
  }

  /**
   * 注册 RPC 方法处理器
   */
  register<T = unknown, R = unknown>(method: string, handler: RpcHandler<T, R>): void {
    this.handlers.set(method, handler as RpcHandler<unknown, unknown>);
  }

  /**
   * 启动 JSON-RPC 服务器，开始监听 stdin
   */
  start(): void {
    // 使用 stderr 输出日志（避免与 JSON-RPC 通信冲突）
    console.error('[JsonRpcServer] Starting...');

    this.rl.on('line', async (line) => {
      // 跳过空行
      if (!line.trim()) {
        return;
      }

      try {
        const request: RpcRequest = JSON.parse(line);

        // 验证 JSON-RPC 2.0 基本格式
        if (request.jsonrpc !== '2.0') {
          this.sendError(request.id, RpcErrorCode.InvalidRequest, 'Invalid JSON-RPC version');
          return;
        }

        // 处理请求
        await this.handleRequest(request);
      } catch (error) {
        // JSON 解析错误
        if (error instanceof SyntaxError) {
          this.sendError(null, RpcErrorCode.ParseError, 'Parse error');
        } else {
          console.error('[JsonRpcServer] Error processing line:', error);
        }
      }
    });

    this.rl.on('close', () => {
      console.error('[JsonRpcServer] Closed');
      process.exit(0);
    });

    console.error('[JsonRpcServer] Started');
  }

  /**
   * 处理 RPC 请求
   */
  private async handleRequest(request: RpcRequest): Promise<void> {
    const handler = this.handlers.get(request.method);

    if (!handler) {
      this.sendError(
        request.id,
        RpcErrorCode.MethodNotFound,
        `Method not found: ${request.method}`
      );
      return;
    }

    try {
      const result = await handler(request.params ?? {});
      this.sendSuccess(request.id, result);
    } catch (error) {
      console.error(`[JsonRpcServer] Handler error for ${request.method}:`, error);
      this.sendError(
        request.id,
        RpcErrorCode.InternalError,
        error instanceof Error ? error.message : 'Unknown error'
      );
    }
  }

  /**
   * 发送成功响应
   */
  private sendSuccess<T>(id: string, result: T): void {
    const response: RpcResponse<T> = {
      jsonrpc: '2.0',
      id,
      result,
    };
    console.log(JSON.stringify(response));
  }

  /**
   * 发送错误响应
   */
  private sendError(
    id: string | null,
    code: RpcErrorCode,
    message: string,
    data?: unknown
  ): void {
    const response: RpcResponse = {
      jsonrpc: '2.0',
      id: id ?? '', // 对于通知，id 可以为 null
      error: {
        code,
        message,
        ...(data !== undefined && { data }),
      },
    };
    console.log(JSON.stringify(response));
  }

  /**
   * 发送通知到 Rust 侧（单向消息，无响应）
   */
  sendNotification<T>(method: string, params: T): void {
    const notification: RpcNotification<T> = {
      jsonrpc: '2.0',
      method,
      params,
    };
    console.log(JSON.stringify(notification));
  }
}
