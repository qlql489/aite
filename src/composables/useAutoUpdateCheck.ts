import { onBeforeUnmount, onMounted } from 'vue';
import { useUpdaterStore } from '../stores/updater';

const CHECK_INTERVAL_MS = 10 * 60 * 1000;
const STARTUP_DELAY_MS = 5000;

export function useAutoUpdateCheck(): void {
  const updaterStore = useUpdaterStore();
  let startupTimer: number | null = null;
  let intervalTimer: number | null = null;

  onMounted(() => {
    void updaterStore.initializeVersion();
    if (!updaterStore.isEnabled) {
      return;
    }

    startupTimer = window.setTimeout(() => {
      void updaterStore.checkForUpdates({ silent: true });
    }, STARTUP_DELAY_MS);

    intervalTimer = window.setInterval(() => {
      void updaterStore.checkForUpdates({ silent: true });
    }, CHECK_INTERVAL_MS);
  });

  onBeforeUnmount(() => {
    if (startupTimer !== null) {
      window.clearTimeout(startupTimer);
    }
    if (intervalTimer !== null) {
      window.clearInterval(intervalTimer);
    }
  });
}
