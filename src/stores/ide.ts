import { defineStore } from 'pinia';
import { computed, ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { IDEConnectionInfo, IDEConnectionState, IDEConnectionStatus, IDESelection } from '../types';

const IDE_STATE_EVENT = 'ide:state_changed';
const IDE_SELECTION_EVENT = 'ide:selection_changed';
const POLL_INTERVAL_MS = 10000;

interface RefreshOptions {
  autoConnect?: boolean;
}

interface ConnectOptions {
  rememberSelection?: boolean;
}

export const useIDEStore = defineStore('ide', () => {
  const availableIDEs = ref<IDEConnectionInfo[]>([]);
  const connectionState = ref<IDEConnectionStatus>('disconnected');
  const connectedIde = ref<IDEConnectionInfo | null>(null);
  const selection = ref<IDESelection | null>(null);
  const includeSelectionInContext = ref(true);
  const error = ref<string | null>(null);
  const isScanning = ref(false);
  const activeSessionId = ref('');
  const currentProjectPath = ref('');
  const selectedIdeKey = ref<string | null>(null);
  const listenersReady = ref(false);

  let pollTimer: number | null = null;
  let unlistenFns: UnlistenFn[] = [];
  const sessionSelectedIdeKeys = new Map<string, string | null>();
  const sessionSelectionContextEnabled = new Map<string, boolean>();

  const hasMultipleChoices = computed(() => availableIDEs.value.length > 1);

  const applyConnectionState = (nextState: IDEConnectionState | null | undefined) => {
    if (!nextState) return;
    connectionState.value = nextState.status || 'disconnected';
    connectedIde.value = nextState.connectedIde || null;
    selection.value = nextState.selection || null;
    error.value = nextState.error || null;

    if (nextState.connectedIde?.key) {
      selectedIdeKey.value = nextState.connectedIde.key;
      if (activeSessionId.value) {
        sessionSelectedIdeKeys.set(activeSessionId.value, nextState.connectedIde.key);
      }
    }
  };

  const resetVisibleState = () => {
    availableIDEs.value = [];
    connectionState.value = 'disconnected';
    connectedIde.value = null;
    selection.value = null;
    error.value = null;
    isScanning.value = false;
  };

  const persistSessionSelection = (sessionId: string) => {
    if (!sessionId) return;
    sessionSelectedIdeKeys.set(sessionId, selectedIdeKey.value);
    sessionSelectionContextEnabled.set(sessionId, includeSelectionInContext.value);
  };

  const restoreSessionSelection = (sessionId: string) => {
    selectedIdeKey.value = sessionId ? (sessionSelectedIdeKeys.get(sessionId) || null) : null;
    includeSelectionInContext.value = sessionId
      ? (sessionSelectionContextEnabled.get(sessionId) ?? true)
      : true;
  };

  const setIncludeSelectionInContext = (enabled: boolean) => {
    includeSelectionInContext.value = enabled;
    if (activeSessionId.value) {
      sessionSelectionContextEnabled.set(activeSessionId.value, enabled);
    }
  };

  watch([includeSelectionInContext, activeSessionId], ([enabled, sessionId]) => {
    if (!sessionId) return;
    sessionSelectionContextEnabled.set(sessionId, enabled);
  });

  const ensureListeners = async () => {
    if (listenersReady.value) return;

    const [unlistenState, unlistenSelection] = await Promise.all([
      listen<IDEConnectionState>(IDE_STATE_EVENT, (event) => {
        applyConnectionState(event.payload);
      }),
      listen<IDESelection | null>(IDE_SELECTION_EVENT, (event) => {
        selection.value = event.payload || null;
      }),
    ]);

    unlistenFns = [unlistenState, unlistenSelection];
    listenersReady.value = true;
  };

  const clearPolling = () => {
    if (pollTimer !== null) {
      window.clearInterval(pollTimer);
      pollTimer = null;
    }
  };

  const startPolling = () => {
    clearPolling();
    pollTimer = window.setInterval(() => {
      void refreshDetectedIDEs({ autoConnect: true });
    }, POLL_INTERVAL_MS);
  };

  const autoConnectDetectedIDEs = async (ides: IDEConnectionInfo[]) => {
    if (ides.length === 0) {
      if (connectedIde.value || connectionState.value !== 'disconnected') {
        await disconnectIDE(false);
      }
      return;
    }

    if (ides.length === 1) {
      await connectToIDE(ides[0], { rememberSelection: true });
      return;
    }

    const currentConnected = connectedIde.value
      ? ides.find((item) => item.key === connectedIde.value?.key)
      : null;
    if (currentConnected && connectionState.value === 'connected') {
      return;
    }

    const preferred = selectedIdeKey.value
      ? ides.find((item) => item.key === selectedIdeKey.value)
      : null;

    if (preferred) {
      await connectToIDE(preferred, { rememberSelection: false });
      return;
    }

    connectionState.value = 'disconnected';
    connectedIde.value = null;
    selection.value = null;
    error.value = null;
  };

  const refreshDetectedIDEs = async (options: RefreshOptions = {}) => {
    if (!currentProjectPath.value) {
      availableIDEs.value = [];
      return [];
    }

    isScanning.value = true;
    try {
      const detected = await invoke<IDEConnectionInfo[]>('detect_running_ides', {
        currentDir: currentProjectPath.value,
        returnAll: false,
      });
      availableIDEs.value = detected;

      if (options.autoConnect !== false) {
        await autoConnectDetectedIDEs(detected);
      }

      return detected;
    } catch (invokeError) {
      console.error('[IDE] Failed to detect running IDEs:', invokeError);
      availableIDEs.value = [];
      error.value = invokeError instanceof Error ? invokeError.message : '扫描 IDE 失败';
      return [];
    } finally {
      isScanning.value = false;
    }
  };

  const connectToIDE = async (ide: IDEConnectionInfo, options: ConnectOptions = {}) => {
    const { rememberSelection = true } = options;

    if (rememberSelection) {
      selectedIdeKey.value = ide.key;
      if (activeSessionId.value) {
        sessionSelectedIdeKeys.set(activeSessionId.value, ide.key);
      }
    }

    if (connectedIde.value?.key === ide.key && connectionState.value === 'connected') {
      return;
    }

    connectionState.value = 'connecting';
    connectedIde.value = ide;
    error.value = null;

    try {
      const state = await invoke<IDEConnectionState>('connect_ide', { ide });
      applyConnectionState(state);
    } catch (invokeError) {
      console.error('[IDE] Failed to connect IDE:', invokeError);
      connectionState.value = 'error';
      connectedIde.value = ide;
      error.value = invokeError instanceof Error ? invokeError.message : '连接 IDE 失败';
    }
  };

  const disconnectIDE = async (clearChoice = false) => {
    try {
      await invoke<boolean>('disconnect_ide');
    } catch (invokeError) {
      console.error('[IDE] Failed to disconnect IDE:', invokeError);
    }

    if (clearChoice) {
      if (activeSessionId.value) {
        sessionSelectedIdeKeys.delete(activeSessionId.value);
      }
      selectedIdeKey.value = null;
    }

    connectionState.value = 'disconnected';
    connectedIde.value = null;
    selection.value = null;
    error.value = null;
  };

  const initialize = async (sessionId: string, projectPath: string) => {
    const nextSessionId = sessionId.trim();
    const nextProjectPath = projectPath.trim();
    const sessionChanged = activeSessionId.value !== nextSessionId;
    const projectChanged = currentProjectPath.value !== nextProjectPath;
    const contextChanged = sessionChanged || projectChanged;

    if (contextChanged && activeSessionId.value) {
      persistSessionSelection(activeSessionId.value);
    }

    activeSessionId.value = nextSessionId;
    currentProjectPath.value = nextProjectPath;
    restoreSessionSelection(nextSessionId);

    if (contextChanged) {
      clearPolling();
      resetVisibleState();
      await disconnectIDE(false);
    }

    if (!nextSessionId || !nextProjectPath) {
      clearPolling();
      resetVisibleState();
      await disconnectIDE(false);
      return;
    }

    await ensureListeners();

    if (!contextChanged) {
      try {
        const currentState = await invoke<IDEConnectionState>('get_ide_connection_state');
        applyConnectionState(currentState);
      } catch (invokeError) {
        console.error('[IDE] Failed to get initial state:', invokeError);
      }
    }

    await refreshDetectedIDEs({ autoConnect: true });
    startPolling();
  };

  const teardown = async () => {
    clearPolling();
    while (unlistenFns.length > 0) {
      const unlisten = unlistenFns.pop();
      await unlisten?.();
    }
    listenersReady.value = false;
  };

  return {
    availableIDEs,
    connectionState,
    connectedIde,
    selection,
    includeSelectionInContext,
    error,
    isScanning,
    activeSessionId,
    currentProjectPath,
    selectedIdeKey,
    hasMultipleChoices,
    initialize,
    teardown,
    refreshDetectedIDEs,
    connectToIDE,
    disconnectIDE,
    setIncludeSelectionInContext,
  };
});
