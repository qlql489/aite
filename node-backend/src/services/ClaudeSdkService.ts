import { promises as fs } from 'fs';
import * as path from 'path';
import * as os from 'os';
import { exec } from 'child_process';
import { promisify } from 'util';
import {
  unstable_v2_createSession,
  unstable_v2_prompt,
  SDKSession,
  SDKMessage,
  SDKResultMessage,
  SDKSessionOptions,
} from '@anthropic-ai/claude-agent-sdk';
import { Session, Message, SdkStatus, RetryConfig } from '../types/index.js';

const execAsync = promisify(exec);

/**
 * Claude SDK 服务
 * 封装 @anthropic-ai/claude-agent-sdk 的调用
 */
export class ClaudeSdkService {
  private sessions: Map<string, SDKSession> = new Map();
  private cliPath: string | null = null;
  private initialized = false;
  private readonly retryConfig: RetryConfig;

  constructor(retryConfig?: Partial<RetryConfig>) {
    this.retryConfig = {
      maxAttempts: retryConfig?.maxAttempts ?? 3,
      backoffMs: retryConfig?.backoffMs ?? 1000,
    };
  }

  /**
   * 检查 SDK 是否已初始化
   */
  isInitialized(): boolean {
    return this.initialized;
  }

  /**
   * 获取 SDK 状态
   */
  getStatus(): SdkStatus {
    return {
      initialized: this.initialized,
      cli_path: this.cliPath ?? undefined,
      error: this.initialized ? undefined : 'SDK not initialized',
    };
  }

  /**
   * 查找 claude-cli 可执行文件
   */
  private async findClaudeCli(): Promise<string> {
    // 步骤 1: 检查环境变量
    const envPath = process.env.CLAUDE_CLI_PATH;
    if (envPath) {
      if (await this.fileExists(envPath)) {
        console.error(`[ClaudeSdkService] Using CLI from env: ${envPath}`);
        return envPath;
      }
    }

    // 步骤 2: 使用 which/where 搜索 PATH
    const platform = os.platform();
    const command = platform === 'win32' ? 'where claude' : 'which claude';

    try {
      const { stdout } = await execAsync(command);
      const cliPath = stdout.trim().split('\n')[0];
      if (cliPath && await this.fileExists(cliPath)) {
        console.error(`[ClaudeSdkService] Found CLI in PATH: ${cliPath}`);
        return cliPath;
      }
    } catch (error) {
      // which/where 找不到时会抛出错误，继续下一步
    }

    // 步骤 3: 搜索常见位置
    const commonPaths = this.getCommonCliPaths(platform);
    for (const searchPath of commonPaths) {
      if (await this.fileExists(searchPath)) {
        console.error(`[ClaudeSdkService] Found CLI in common path: ${searchPath}`);
        return searchPath;
      }
    }

    throw new Error(
      'Claude CLI not found. Please install claude-cli or set CLAUDE_CLI_PATH environment variable.'
    );
  }

  /**
   * 获取常见 CLI 路径
   */
  private getCommonCliPaths(platform: string): string[] {
    const paths: string[] = [];
    const homeDir = os.homedir();

    if (platform === 'win32') {
      paths.push(
        path.join(process.env.APPDATA || '', 'npm', 'claude.cmd'),
        path.join(process.env.APPDATA || '', 'npm', 'claude.exe')
      );
    } else {
      paths.push(
        '/usr/local/bin/claude',
        path.join(homeDir, '.local', 'bin', 'claude'),
        path.join(homeDir, '.npm', 'global', 'bin', 'claude'),
        '/opt/homebrew/bin/claude',
        '/usr/bin/claude'
      );
    }

    return paths;
  }

  /**
   * 检查文件是否存在
   */
  private async fileExists(filePath: string): Promise<boolean> {
    try {
      await fs.access(filePath, fs.constants.F_OK | fs.constants.X_OK);
      return true;
    } catch {
      return false;
    }
  }

  /**
   * 初始化 SDK
   * 验证 claude-cli 是否可用
   */
  async initialize(): Promise<void> {
    if (this.initialized) {
      console.error('[ClaudeSdkService] Already initialized');
      return;
    }

    try {
      console.error('[ClaudeSdkService] Initializing...');

      // 查找 claude-cli
      this.cliPath = await this.findClaudeCli();

      // 验证 claude-cli 是否可以运行
      const { stdout } = await execAsync(`${this.cliPath} --version`);
      console.error(`[ClaudeSdkService] Claude CLI version: ${stdout.trim()}`);

      this.initialized = true;
      console.error(`[ClaudeSdkService] Initialized successfully with CLI: ${this.cliPath}`);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error('[ClaudeSdkService] Initialization failed:', errorMessage);
      throw new Error(`Failed to initialize Claude SDK: ${errorMessage}`);
    }
  }

  /**
   * 创建会话
   */
  async createSession(model: string = 'claude-sonnet-4-5-20250929'): Promise<Session> {
    this.ensureInitialized();

    return this.callWithRetry(async () => {
      const options: SDKSessionOptions = {
        model,
        pathToClaudeCodeExecutable: this.cliPath ?? undefined,
      };

      const sdkSession = unstable_v2_createSession(options);
      const sessionId = crypto.randomUUID();

      // 存储 session（注意：sessionId 在第一次消息后才会被 SDK 设置）
      this.sessions.set(sessionId, sdkSession);

      console.error(`[ClaudeSdkService] Created session: ${sessionId}`);

      return {
        id: sessionId,
        created_at: new Date().toISOString(),
      };
    });
  }

  /**
   * 发送消息
   */
  async sendMessage(sessionId: string, content: string): Promise<Message> {
    this.ensureInitialized();

    const sdkSession = this.sessions.get(sessionId);
    if (!sdkSession) {
      throw new Error(`Session not found: ${sessionId}`);
    }

    return this.callWithRetry(async () => {
      // 发送消息
      await sdkSession.send(content);

      // 读取响应流
      let finalMessage: Message | null = null;
      for await (const sdkMessage of sdkSession.stream()) {
        finalMessage = this.convertSdkMessage(sdkMessage);
      }

      if (!finalMessage) {
        throw new Error('No response received from Claude');
      }

      console.error(`[ClaudeSdkService] Sent message to session ${sessionId}`);
      return finalMessage;
    });
  }

  /**
   * 关闭会话
   */
  async closeSession(sessionId: string): Promise<void> {
    const sdkSession = this.sessions.get(sessionId);
    if (sdkSession) {
      sdkSession.close();
      this.sessions.delete(sessionId);
      console.error(`[ClaudeSdkService] Closed session: ${sessionId}`);
    }
  }

  /**
   * 转换 SDK 消息为应用消息格式
   */
  private convertSdkMessage(sdkMessage: SDKMessage): Message {
    if (sdkMessage.type === 'assistant') {
      return {
        id: crypto.randomUUID(),
        role: 'assistant',
        content: JSON.stringify(sdkMessage.message),
        created_at: new Date().toISOString(),
      };
    } else if (sdkMessage.type === 'result') {
      const result = sdkMessage as SDKResultMessage;

      // 根据 subtype 区分成功和错误情况
      if (result.subtype === 'success') {
        return {
          id: crypto.randomUUID(),
          role: 'assistant',
          content: result.result,
          created_at: new Date().toISOString(),
        };
      } else {
        // 错误情况
        return {
          id: crypto.randomUUID(),
          role: 'assistant',
          content: `Error: ${result.errors.join(', ')}`,
          created_at: new Date().toISOString(),
        };
      }
    }

    // 其他消息类型
    return {
      id: crypto.randomUUID(),
      role: 'system',
      content: JSON.stringify(sdkMessage),
      created_at: new Date().toISOString(),
    };
  }

  /**
   * 带重试的调用
   */
  private async callWithRetry<T>(fn: () => Promise<T>): Promise<T> {
    let lastError: Error | null = null;

    for (let attempt = 0; attempt < this.retryConfig.maxAttempts; attempt++) {
      try {
        return await fn();
      } catch (error) {
        lastError = error instanceof Error ? error : new Error(String(error));

        if (!this.isRetryable(lastError)) {
          throw lastError;
        }

        if (attempt < this.retryConfig.maxAttempts - 1) {
          const backoffMs = Math.pow(2, attempt) * this.retryConfig.backoffMs;
          console.error(
            `[ClaudeSdkService] Retry ${attempt + 1}/${this.retryConfig.maxAttempts} ` +
              `after ${backoffMs}ms (error: ${lastError.message})`
          );

          await this.sleep(backoffMs);
        }
      }
    }

    throw lastError;
  }

  /**
   * 判断错误是否可重试
   */
  private isRetryable(error: Error): boolean {
    const retryablePatterns = [
      /timeout/i,
      /network/i,
      /connection/i,
      /temporarily/i,
      /rate limit/i,
      /ECONNRESET/i,
      /ETIMEDOUT/i,
    ];

    return retryablePatterns.some((pattern) => pattern.test(error.message));
  }

  /**
   * 确保 SDK 已初始化
   */
  private ensureInitialized(): void {
    if (!this.initialized) {
      throw new Error('SDK not initialized. Call initialize() first.');
    }
  }

  /**
   * 延迟函数
   */
  private sleep(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }
}
