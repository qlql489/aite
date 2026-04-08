import { JsonRpcServer } from './rpc/server.js';
import { ClaudeSdkService } from './services/ClaudeSdkService.js';
import { SessionHandler } from './rpc/handlers/claude.js';
import { SystemHandler } from './rpc/handlers/system.js';
import { loadConfig, validateConfig } from './config/index.js';
import { startBridgeServer } from './bridgeServer.js';

/**
 * 主入口
 * 启动 Node.js 后端服务
 */
async function main(): Promise<void> {
  console.error('[Node Backend] Starting...');

  try {
    // 加载和验证配置
    const config = loadConfig();
    validateConfig(config);
    console.error('[Node Backend] Config loaded and validated');
    console.error(`[Node Backend] Log level: ${config.logLevel}`);

    // 创建 SDK 服务实例
    const sdkService = new ClaudeSdkService({
      maxAttempts: 3,
      backoffMs: 1000,
    });

    // 创建 JSON-RPC 服务器
    const server = new JsonRpcServer();

    // 创建 handlers
    const sessionHandler = new SessionHandler(sdkService);
    const systemHandler = new SystemHandler(sdkService);

    // 注册 session handlers
    server.register('session.create', (params) => sessionHandler.create(params));
    server.register('session.send_message', (params: unknown) => sessionHandler.sendMessage(params as any));

    // 注册 system handlers
    server.register('system.ping', () => systemHandler.ping());
    server.register('system.get_status', () => systemHandler.getStatus());

    console.error('[Node Backend] RPC handlers registered');

    await startBridgeServer();
    console.error('[Node Backend] OpenAI bridge started');

    // 初始化 SDK
    try {
      await sdkService.initialize();
      console.error('[Node Backend] SDK initialized successfully');
    } catch (error) {
      console.error('[Node Backend] SDK initialization failed:', error);
      // SDK 初始化失败不阻止服务启动
      // 前端可以通过 get_status 查询状态
    }

    // 启动服务器，开始监听 stdin
    server.start();

    console.error('[Node Backend] Ready');
  } catch (error) {
    console.error('[Node Backend] Fatal error:', error);
    process.exit(1);
  }
}

// 启动应用
main().catch((error) => {
  console.error('[Node Backend] Unhandled error:', error);
  process.exit(1);
});
