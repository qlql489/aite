import { ClaudeSdkService } from '../../services/ClaudeSdkService.js';
import { Session, Message } from '../../types/index.js';

/**
 * Session RPC Handlers
 * 处理会话相关的 RPC 请求
 */
export class SessionHandler {
  constructor(private sdkService: ClaudeSdkService) {}

  /**
   * 创建会话
   */
  async create(params: unknown): Promise<Session> {
    console.error('[SessionHandler] create called');

    try {
      const session = await this.sdkService.createSession();
      return session;
    } catch (error) {
      console.error('[SessionHandler] create error:', error);
      throw error;
    }
  }

  /**
   * 发送消息
   */
  async sendMessage(params: SendMessageParams): Promise<Message> {
    console.error('[SessionHandler] sendMessage called with session_id:', params.session_id);

    // 参数验证
    if (!params.session_id || typeof params.session_id !== 'string') {
      throw new Error('Invalid session_id: must be a non-empty string');
    }

    if (!params.content || typeof params.content !== 'string') {
      throw new Error('Invalid content: must be a non-empty string');
    }

    try {
      const message = await this.sdkService.sendMessage(params.session_id, params.content);
      return message;
    } catch (error) {
      console.error('[SessionHandler] sendMessage error:', error);
      throw error;
    }
  }
}

/**
 * 发送消息参数
 */
export interface SendMessageParams {
  session_id: string;
  content: string;
}
