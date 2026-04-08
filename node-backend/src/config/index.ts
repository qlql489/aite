import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';

/**
 * 应用配置
 * 注意：API Key 由 claude-cli 管理，不在应用中处理
 */
export interface Config {
  cliPath?: string;
  logLevel: 'debug' | 'info' | 'warn' | 'error';
}

/**
 * 加载配置
 * 从环境变量读取配置
 */
export function loadConfig(): Config {
  return {
    cliPath: process.env.CLAUDE_CLI_PATH,
    logLevel: (process.env.LOG_LEVEL as Config['logLevel']) || 'info',
  };
}

/**
 * 验证配置
 */
export function validateConfig(config: Config): void {
  if (config.cliPath && config.cliPath.trim() === '') {
    throw new Error('CLAUDE_CLI_PATH cannot be empty if provided');
  }
}

/**
 * 获取缓存目录路径
 */
export function getCacheDir(): string {
  const homeDir = os.homedir();
  const cacheDir = path.join(homeDir, '.aite');

  // 确保缓存目录存在
  if (!fs.existsSync(cacheDir)) {
    fs.mkdirSync(cacheDir, { recursive: true });
  }

  return cacheDir;
}

/**
 * 获取缓存的 CLI 路径
 */
export function getCachedCliPath(): string | null {
  try {
    const cacheDir = getCacheDir();
    const cacheFile = path.join(cacheDir, 'cli-path.json');

    if (!fs.existsSync(cacheFile)) {
      return null;
    }

    const content = fs.readFileSync(cacheFile, 'utf-8');
    const data = JSON.parse(content);

    return data.cliPath || null;
  } catch (error) {
    console.error('[config] Failed to read cached CLI path:', error);
    return null;
  }
}

/**
 * 缓存 CLI 路径
 */
export function setCachedCliPath(cliPath: string): void {
  try {
    const cacheDir = getCacheDir();
    const cacheFile = path.join(cacheDir, 'cli-path.json');

    const data = {
      cliPath,
      timestamp: new Date().toISOString(),
    };

    fs.writeFileSync(cacheFile, JSON.stringify(data, null, 2));
  } catch (error) {
    console.error('[config] Failed to cache CLI path:', error);
  }
}

/**
 * 清除缓存的 CLI 路径
 */
export function clearCachedCliPath(): void {
  try {
    const cacheDir = getCacheDir();
    const cacheFile = path.join(cacheDir, 'cli-path.json');

    if (fs.existsSync(cacheFile)) {
      fs.unlinkSync(cacheFile);
    }
  } catch (error) {
    console.error('[config] Failed to clear cached CLI path:', error);
  }
}
