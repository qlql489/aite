import { ClaudeSdkService } from '../../services/ClaudeSdkService.js';
import { SdkStatus } from '../../types/index.js';

/**
 * System RPC Handlers
 * 处理系统相关的 RPC 请求
 */
export class SystemHandler {
  constructor(private sdkService: ClaudeSdkService) {}

  /**
   * Ping - 健康检查
   */
  async ping(): Promise<{ message: string }> {
    console.error('[SystemHandler] ping called');
    return { message: 'pong' };
  }

  /**
   * 获取 SDK 状态
   */
  async getStatus(): Promise<SdkStatus> {
    console.error('[SystemHandler] getStatus called');
    return this.sdkService.getStatus();
  }
}
