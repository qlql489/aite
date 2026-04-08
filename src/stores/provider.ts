import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { ApiProvider, ProviderConfigPayload, ProviderModel, SessionProviderEnv, UpstreamFormat } from '../types';

function nowTs(): number {
  return Date.now();
}

function createProviderId(): string {
  return `provider-${Math.random().toString(36).slice(2, 10)}${Date.now().toString(36)}`;
}

const PRESET_PROVIDERS: Array<Partial<ApiProvider>> = [
  {
    name: 'Anthropic',
    homepageUrl: 'https://www.anthropic.com',
    baseUrl: 'https://api.anthropic.com',
    apiProtocol: 'anthropic',
    authType: 'api_key',
    models: [
      { model: 'claude-opus-4-1', modelName: 'Claude Opus 4.1' },
      { model: 'claude-sonnet-4-5', modelName: 'Claude Sonnet 4.5' },
      { model: 'claude-haiku-4-5', modelName: 'Claude Haiku 4.5' },
    ],
    primaryModel: 'claude-sonnet-4-5',
  },
  {
    name: 'DeepSeek',
    homepageUrl: 'https://www.deepseek.com',
    baseUrl: 'https://api.deepseek.com',
    apiProtocol: 'openai',
    upstreamFormat: 'chat_completions',
    authType: 'api_key',
    models: [
      { model: 'deepseek-chat', modelName: 'DeepSeek Chat' },
      { model: 'deepseek-reasoner', modelName: 'DeepSeek Reasoner' },
    ],
    primaryModel: 'deepseek-chat',
  },
  {
    name: '智谱 GLM',
    homepageUrl: 'https://open.bigmodel.cn',
    baseUrl: 'https://open.bigmodel.cn/api/paas/v4',
    apiProtocol: 'openai',
    upstreamFormat: 'chat_completions',
    authType: 'api_key',
    models: [
      { model: 'glm-4.5', modelName: 'GLM 4.5' },
      { model: 'glm-4.5-air', modelName: 'GLM 4.5 Air' },
      { model: 'glm-4.5-flash', modelName: 'GLM 4.5 Flash' },
    ],
    primaryModel: 'glm-4.5',
  },
  {
    name: 'Qwen',
    homepageUrl: 'https://dashscope.aliyun.com',
    baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1',
    apiProtocol: 'openai',
    upstreamFormat: 'chat_completions',
    authType: 'api_key',
    models: [
      { model: 'qwen-max', modelName: 'Qwen Max' },
      { model: 'qwen-plus', modelName: 'Qwen Plus' },
      { model: 'qwen-turbo', modelName: 'Qwen Turbo' },
    ],
    primaryModel: 'qwen-plus',
  },
  {
    name: 'Kimi',
    homepageUrl: 'https://platform.moonshot.cn',
    baseUrl: 'https://api.moonshot.cn/v1',
    apiProtocol: 'openai',
    upstreamFormat: 'chat_completions',
    authType: 'api_key',
    models: [
      { model: 'moonshot-v1-8k', modelName: 'Moonshot 8K' },
      { model: 'moonshot-v1-32k', modelName: 'Moonshot 32K' },
      { model: 'moonshot-v1-128k', modelName: 'Moonshot 128K' },
    ],
    primaryModel: 'moonshot-v1-32k',
  },
  {
    name: 'MiniMax',
    homepageUrl: 'https://www.minimaxi.com',
    baseUrl: 'https://api.minimaxi.com/v1',
    apiProtocol: 'openai',
    upstreamFormat: 'chat_completions',
    authType: 'api_key',
    models: [
      { model: 'MiniMax-Text-01', modelName: 'MiniMax Text 01' },
    ],
    primaryModel: 'MiniMax-Text-01',
  },
];

export const useProviderStore = defineStore('provider', () => {
  const providers = ref<ApiProvider[]>([]);
  const activeProviderId = ref<string | null>(null);
  const inheritSystemConfig = ref(true);
  const loaded = ref(false);
  const saving = ref(false);

  const enabledProviders = computed(() => providers.value.filter((provider) => provider.enabled !== false));

  const activeProvider = computed(() => {
    if (inheritSystemConfig.value || !activeProviderId.value) return null;
    return enabledProviders.value.find((provider) => provider.id === activeProviderId.value) || null;
  });

  function setExclusiveEnabledProvider(providerId: string | null): void {
    providers.value = providers.value.map((provider) => normalizeProvider({
      ...provider,
      enabled: providerId !== null && provider.id === providerId,
      id: provider.id,
      createdAt: provider.createdAt,
    }));
  }

  function syncProviderState(nextActiveProviderId?: string | null): void {
    const requestedActiveId = nextActiveProviderId === undefined ? activeProviderId.value : nextActiveProviderId;

    if (inheritSystemConfig.value) {
      activeProviderId.value = null;
      setExclusiveEnabledProvider(null);
      return;
    }

    const activeId = requestedActiveId && providers.value.some((provider) => provider.id === requestedActiveId)
      ? requestedActiveId
      : null;

    activeProviderId.value = activeId;
    setExclusiveEnabledProvider(activeId);
  }

  async function load(force = false): Promise<void> {
    if (loaded.value && !force) return;
    const payload = await invoke<ProviderConfigPayload>('get_provider_config');
    providers.value = (payload.providers || []).map((provider) => normalizeProvider(provider));
    inheritSystemConfig.value = payload.inheritSystemConfig ?? true;
    syncProviderState(payload.activeProviderId || null);
    loaded.value = true;
  }

  async function save(): Promise<void> {
    saving.value = true;
    try {
      await invoke('save_provider_config', {
        payload: {
          providers: providers.value,
          activeProviderId: activeProviderId.value,
          inheritSystemConfig: inheritSystemConfig.value,
        },
      });
    } finally {
      saving.value = false;
    }
  }

  function normalizeProvider(provider: Partial<ApiProvider>): ApiProvider {
    const timestamp = nowTs();
    const apiProtocol = provider.apiProtocol || 'anthropic';
    const models = (provider.models || []).filter((item) => item.model.trim()).map((item) => ({
      model: item.model.trim(),
      modelName: item.modelName?.trim() || item.model.trim(),
    }));
    return {
      id: provider.id || createProviderId(),
      name: provider.name?.trim() || '未命名供应商',
      homepageUrl: provider.homepageUrl?.trim() || '',
      baseUrl: provider.baseUrl?.trim() || '',
      enabled: provider.enabled !== false,
      apiProtocol,
      authType: provider.authType || 'api_key',
      apiKey: provider.apiKey?.trim() || undefined,
      models,
      primaryModel: provider.primaryModel || models[0]?.model || '',
      extraEnv: provider.extraEnv || {},
      upstreamFormat: apiProtocol === 'openai'
        ? ((provider.upstreamFormat || 'chat_completions') as UpstreamFormat)
        : undefined,
      createdAt: provider.createdAt || timestamp,
      updatedAt: timestamp,
    };
  }

  async function addProvider(provider: Partial<ApiProvider>): Promise<ApiProvider> {
    const normalized = normalizeProvider({
      ...provider,
      enabled: inheritSystemConfig.value ? false : provider.enabled !== false,
    });
    providers.value = [...providers.value, normalized];
    if (!inheritSystemConfig.value && normalized.enabled) {
      syncProviderState(normalized.id);
    } else {
      syncProviderState(activeProviderId.value);
    }
    await save();
    return getProvider(normalized.id) || normalized;
  }

  async function duplicateProvider(providerId: string): Promise<ApiProvider | null> {
    const target = getProvider(providerId);
    if (!target) return null;
    const duplicated = normalizeProvider({
      ...target,
      id: undefined,
      name: `${target.name} 副本`,
      enabled: false,
      createdAt: undefined,
      updatedAt: undefined,
    });
    providers.value = [duplicated, ...providers.value];
    await save();
    return duplicated;
  }

  async function updateProvider(providerId: string, patch: Partial<ApiProvider>): Promise<void> {
    providers.value = providers.value.map((provider) => {
      if (provider.id !== providerId) return provider;
      return normalizeProvider({
        ...provider,
        ...patch,
        enabled: inheritSystemConfig.value
          ? false
          : (patch.enabled ?? provider.enabled) !== false,
        id: provider.id,
        createdAt: provider.createdAt,
      });
    });

    const updated = getProvider(providerId);
    if (!inheritSystemConfig.value && updated?.enabled !== false) {
      syncProviderState(providerId);
    } else {
      syncProviderState(activeProviderId.value === providerId ? null : activeProviderId.value);
    }
    await save();
  }

  async function removeProvider(providerId: string): Promise<void> {
    providers.value = providers.value.filter((provider) => provider.id !== providerId);
    syncProviderState(activeProviderId.value === providerId ? null : activeProviderId.value);
    await save();
  }

  async function toggleProviderEnabled(providerId: string): Promise<void> {
    if (activeProviderId.value === providerId && !inheritSystemConfig.value) {
      return;
    }
    inheritSystemConfig.value = false;
    syncProviderState(providerId);
    await save();
  }

  async function setActiveProvider(providerId: string | null): Promise<void> {
    if (providerId) {
      inheritSystemConfig.value = false;
    }
    syncProviderState(providerId);
    await save();
  }

  async function setInheritSystemConfig(enabled: boolean): Promise<void> {
    inheritSystemConfig.value = enabled;
    syncProviderState(enabled ? null : activeProviderId.value);
    await save();
  }

  function getProvider(providerId?: string | null): ApiProvider | null {
    if (!providerId) return null;
    return providers.value.find((provider) => provider.id === providerId) || null;
  }

  function getModelsForProvider(providerId?: string | null): ProviderModel[] {
    return getProvider(providerId)?.models || [];
  }

  function getPrimaryModel(providerId?: string | null): string {
    const provider = getProvider(providerId);
    return provider?.primaryModel || provider?.models[0]?.model || '';
  }

  function resolveSessionProvider(sessionProviderId?: string | null, providerOverrideEnabled?: boolean): ApiProvider | null {
    if (providerOverrideEnabled && sessionProviderId) {
      return getProvider(sessionProviderId);
    }
    return activeProvider.value;
  }

  function resolveSessionModel(
    sessionProviderId?: string | null,
    sessionModel?: string | null,
    providerOverrideEnabled?: boolean,
  ): string {
    const provider = resolveSessionProvider(sessionProviderId, providerOverrideEnabled);
    if (!provider) return sessionModel || '';
    if (sessionModel && provider.models.some((item) => item.model === sessionModel)) {
      return sessionModel;
    }
    return provider.primaryModel || provider.models[0]?.model || '';
  }

  function buildSessionProviderEnv(providerId?: string | null): SessionProviderEnv | null {
    if (inheritSystemConfig.value && !providerId) return null;
    const provider = getProvider(providerId);
    if (!provider) return null;
    return {
      baseUrl: provider.baseUrl,
      apiKey: provider.apiKey,
      apiProtocol: provider.apiProtocol,
      authType: provider.authType,
      extraEnv: provider.extraEnv || {},
      upstreamFormat: provider.upstreamFormat,
    };
  }

  function getPresetProviders(): ApiProvider[] {
    return PRESET_PROVIDERS.map((provider) => normalizeProvider(provider));
  }

  return {
    providers,
    enabledProviders,
    activeProviderId,
    activeProvider,
    inheritSystemConfig,
    loaded,
    saving,
    load,
    save,
    addProvider,
    duplicateProvider,
    updateProvider,
    removeProvider,
    toggleProviderEnabled,
    setActiveProvider,
    setInheritSystemConfig,
    getProvider,
    getModelsForProvider,
    getPrimaryModel,
    resolveSessionProvider,
    resolveSessionModel,
    buildSessionProviderEnv,
    getPresetProviders,
  };
});
