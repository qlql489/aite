import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import { relaunch } from '../lib/tauri-process';
import { check } from '../lib/tauri-updater';
import {
  AUTO_UPDATER_DISABLED_MESSAGE,
  AUTO_UPDATER_ENABLED,
  mapUpdateErrorMessage,
} from '../config/updater';

export type UpdateStatus =
  | 'idle'
  | 'checking'
  | 'available'
  | 'downloading'
  | 'ready'
  | 'latest'
  | 'error';

type CheckOptions = {
  silent?: boolean;
};

let updateHandle: any = null;
let latestTimer: number | null = null;

export const useUpdaterStore = defineStore('updater', () => {
  const isEnabled = ref(AUTO_UPDATER_ENABLED);
  const disabledReason = ref(AUTO_UPDATER_DISABLED_MESSAGE);
  const currentVersion = ref('');
  const updateAvailable = ref(false);
  const updateVersion = ref('');
  const updateDownloaded = ref(false);
  const status = ref<UpdateStatus>('idle');
  const progress = ref(0);
  const errorMessage = ref('');

  const actionLabel = computed(() => {
    if (!isEnabled.value) {
      return '暂未启用';
    }
    switch (status.value) {
      case 'checking':
        return '检查中...';
      case 'available':
        return `下载并安装 v${updateVersion.value}`;
      case 'downloading':
        return `下载中 ${progress.value}%`;
      case 'ready':
        return '重启应用';
      case 'latest':
        return '已是最新版本';
      case 'error':
        return '重新检查';
      default:
        return '检查更新';
    }
  });

  function clearLatestTimer(): void {
    if (latestTimer !== null) {
      window.clearTimeout(latestTimer);
      latestTimer = null;
    }
  }

  async function initializeVersion(): Promise<void> {
    if (currentVersion.value) return;
    try {
      const { getVersion } = await import('@tauri-apps/api/app');
      currentVersion.value = await getVersion();
    } catch (error) {
      console.error('读取应用版本失败:', error);
    }
  }

  function setAvailable(version?: string): void {
    updateAvailable.value = true;
    updateDownloaded.value = false;
    if (version) {
      updateVersion.value = version;
    }
    if (status.value !== 'downloading' && status.value !== 'ready') {
      status.value = 'available';
    }
    errorMessage.value = '';
  }

  function clearAvailable(): void {
    updateAvailable.value = false;
    updateVersion.value = '';
    updateDownloaded.value = false;
    updateHandle = null;
  }

  async function checkForUpdates(options: CheckOptions = {}): Promise<boolean> {
    const { silent = false } = options;
    clearLatestTimer();
    if (!isEnabled.value) {
      clearAvailable();
      if (!silent) {
        errorMessage.value = disabledReason.value;
      }
      status.value = 'idle';
      return false;
    }
    if (!silent) {
      status.value = 'checking';
      errorMessage.value = '';
    }

    try {
      const update = await check();
      if (update) {
        updateHandle = update;
        setAvailable(update.version);
        return true;
      }

      clearAvailable();
      if (!silent) {
        status.value = 'latest';
        latestTimer = window.setTimeout(() => {
          status.value = 'idle';
          latestTimer = null;
        }, 3000);
      } else if (status.value !== 'ready') {
        status.value = 'idle';
      }
      return false;
    } catch (error) {
      if (!silent) {
        errorMessage.value = mapUpdateErrorMessage(error);
        status.value = 'error';
      }
      return false;
    }
  }

  async function downloadAndInstall(): Promise<void> {
    if (!isEnabled.value) {
      status.value = 'idle';
      errorMessage.value = disabledReason.value;
      return;
    }
    if (status.value === 'downloading') return;

    let handle = updateHandle;
    if (!handle) {
      const found = await checkForUpdates({ silent: true });
      if (!found) {
        status.value = 'error';
        errorMessage.value = '未找到可下载的更新包，请先检查更新配置。';
        return;
      }
      handle = updateHandle;
    }

    if (!handle) {
      status.value = 'error';
      errorMessage.value = '更新句柄初始化失败。';
      return;
    }

    status.value = 'downloading';
    progress.value = 0;
    errorMessage.value = '';

    try {
      let totalLength = 0;
      let downloaded = 0;
      await handle.downloadAndInstall((event: {
        event: string;
        data: { contentLength?: number; chunkLength: number };
      }) => {
        if (event.event === 'Started' && event.data.contentLength) {
          totalLength = event.data.contentLength;
        } else if (event.event === 'Progress') {
          downloaded += event.data.chunkLength;
          if (totalLength > 0) {
            progress.value = Math.round((downloaded / totalLength) * 100);
          }
        } else if (event.event === 'Finished') {
          progress.value = 100;
        }
      });
      updateDownloaded.value = true;
      status.value = 'ready';
    } catch (error) {
      errorMessage.value = mapUpdateErrorMessage(error);
      status.value = 'error';
    }
  }

  async function relaunchApp(): Promise<void> {
    try {
      await relaunch();
    } catch (error) {
      errorMessage.value = mapUpdateErrorMessage(error);
      status.value = 'error';
    }
  }

  async function performPrimaryAction(): Promise<void> {
    if (!isEnabled.value) {
      status.value = 'idle';
      errorMessage.value = disabledReason.value;
      return;
    }
    if (status.value === 'ready') {
      await relaunchApp();
      return;
    }

    if (status.value === 'available') {
      await downloadAndInstall();
      return;
    }

    await checkForUpdates();
  }

  return {
    isEnabled,
    disabledReason,
    currentVersion,
    updateAvailable,
    updateVersion,
    updateDownloaded,
    status,
    progress,
    errorMessage,
    actionLabel,
    initializeVersion,
    checkForUpdates,
    downloadAndInstall,
    relaunchApp,
    performPrimaryAction,
  };
});
