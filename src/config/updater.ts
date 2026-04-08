import tauriConfig from '../../src-tauri/tauri.conf.json';

type TauriUpdaterConfig = {
  pubkey?: string;
  endpoints?: string[];
};

type TauriConfig = {
  plugins?: {
    updater?: TauriUpdaterConfig;
  };
};

const PLACEHOLDER_ENDPOINT_MARKERS = [
  'REPLACE_WITH_GITHUB_OWNER',
  'REPLACE_WITH_GITHUB_REPO',
  'your-name',
  'your-repo',
] as const;

const updaterConfig = ((tauriConfig as TauriConfig).plugins?.updater ?? {}) as TauriUpdaterConfig;
const configuredPubkey = updaterConfig.pubkey?.trim() ?? '';

function normalizeEndpoints(endpoints?: string[]): string[] {
  if (!Array.isArray(endpoints)) return [];
  return endpoints
    .map(endpoint => endpoint.trim())
    .filter(endpoint => endpoint.length > 0);
}

function isPlaceholderEndpoint(endpoint: string): boolean {
  return PLACEHOLDER_ENDPOINT_MARKERS.some(marker => endpoint.includes(marker));
}

export const AUTO_UPDATER_ENDPOINTS = normalizeEndpoints(updaterConfig.endpoints);
export const AUTO_UPDATER_HAS_VALID_ENDPOINTS =
  AUTO_UPDATER_ENDPOINTS.length > 0 && AUTO_UPDATER_ENDPOINTS.every(endpoint => !isPlaceholderEndpoint(endpoint));
export const AUTO_UPDATER_ENABLED =
  configuredPubkey.length > 0 && AUTO_UPDATER_HAS_VALID_ENDPOINTS;

export const AUTO_UPDATER_DISABLED_MESSAGE = !configuredPubkey
  ? '自动更新未启用：缺少 updater 公钥配置。'
  : AUTO_UPDATER_ENDPOINTS.length === 0
    ? '自动更新未启用：缺少 GitHub Release 更新地址。'
    : !AUTO_UPDATER_HAS_VALID_ENDPOINTS
      ? '自动更新未启用：请先把 updater endpoint 改成当前仓库的 latest.json 地址。'
      : '自动更新当前不可用，请检查 updater 配置。';

export function mapUpdateErrorMessage(error: unknown): string {
  const rawMessage = error instanceof Error ? error.message : String(error);
  const normalizedMessage = rawMessage.toLowerCase();

  if (normalizedMessage.includes('404')) {
    return '未找到 latest.json，请先在 GitHub Release 中上传 updater 产物并生成更新清单。';
  }

  if (
    normalizedMessage.includes('signature') ||
    normalizedMessage.includes('unexpectedkeyid') ||
    normalizedMessage.includes('keyid')
  ) {
    return '更新包签名校验失败，请确认 Release 产物使用了与当前客户端公钥匹配的私钥签名。';
  }

  if (normalizedMessage.includes('tls') || normalizedMessage.includes('certificate')) {
    return '更新地址证书校验失败，请确认 Release 下载地址可通过 HTTPS 正常访问。';
  }

  return rawMessage;
}
