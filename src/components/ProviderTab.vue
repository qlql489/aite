<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from 'vue';
import { HugeiconsIcon } from '@hugeicons/vue';
import {
  ApiIcon,
  ArrowLeft02Icon,
  Copy01Icon,
  Delete02Icon,
  Edit01Icon,
  EyeIcon,
  ViewOffIcon,
} from '@hugeicons/core-free-icons';
import { useProviderStore } from '../stores/provider';
import type { ApiProvider, ApiProtocol, AuthType, ProviderModel, UpstreamFormat } from '../types';

const providerStore = useProviderStore();
const selectedProviderId = ref<string | null>(null);
const showCreateModal = ref(false);
const pageMode = ref<'list' | 'edit'>('list');
const selectedPresetName = ref('');

const presets = computed(() => providerStore.getPresetProviders());
const providers = computed(() => providerStore.providers);
const activeProviderId = computed(() => providerStore.activeProviderId);
const inheritSystemConfig = computed(() => providerStore.inheritSystemConfig);
const selectedProvider = computed(() => providerStore.getProvider(selectedProviderId.value));

const emptyForm = (): Partial<ApiProvider> => ({
  name: '',
  homepageUrl: '',
  baseUrl: '',
  enabled: true,
  apiProtocol: 'anthropic',
  authType: 'api_key',
  apiKey: '',
  models: [],
  primaryModel: '',
  extraEnv: {},
  upstreamFormat: 'chat_completions',
});

const form = reactive<Partial<ApiProvider>>(emptyForm());
const modelInput = reactive<ProviderModel>({ model: '', modelName: '' });
const envDraft = reactive({ key: '', value: '' });
const showApiKey = ref(false);

const ZHIPU_ANTHROPIC_BASE_URL = 'https://open.bigmodel.cn/api/anthropic';
const ZHIPU_OPENAI_BASE_URL = 'https://open.bigmodel.cn/api/paas/v4';
const MINIMAX_ANTHROPIC_BASE_URL = 'https://api.minimaxi.com/anthropic';
const MINIMAX_OPENAI_BASE_URL = 'https://api.minimaxi.com/v1';

function isZhipuProviderDraft(provider: Partial<ApiProvider>): boolean {
  const name = provider.name?.toLowerCase() || '';
  const homepageUrl = provider.homepageUrl?.toLowerCase() || '';
  const baseUrl = provider.baseUrl?.toLowerCase() || '';
  return name.includes('智谱') || name.includes('glm') || homepageUrl.includes('bigmodel.cn') || baseUrl.includes('bigmodel.cn');
}

function isMiniMaxProviderDraft(provider: Partial<ApiProvider>): boolean {
  const name = provider.name?.toLowerCase() || '';
  const homepageUrl = provider.homepageUrl?.toLowerCase() || '';
  const baseUrl = provider.baseUrl?.toLowerCase() || '';
  return name.includes('minimax') || homepageUrl.includes('minimaxi.com') || baseUrl.includes('minimaxi.com');
}

function syncKnownProviderBaseUrl(): void {
  const currentBaseUrl = (form.baseUrl || '').trim();

  if (isZhipuProviderDraft(form)) {
    const isKnownZhipuUrl = !currentBaseUrl || currentBaseUrl === ZHIPU_ANTHROPIC_BASE_URL || currentBaseUrl === ZHIPU_OPENAI_BASE_URL;
    if (!isKnownZhipuUrl) return;
    form.baseUrl = form.apiProtocol === 'openai' ? ZHIPU_OPENAI_BASE_URL : ZHIPU_ANTHROPIC_BASE_URL;
    return;
  }

  if (isMiniMaxProviderDraft(form)) {
    const isKnownMiniMaxUrl = !currentBaseUrl || currentBaseUrl === MINIMAX_ANTHROPIC_BASE_URL || currentBaseUrl === MINIMAX_OPENAI_BASE_URL;
    if (!isKnownMiniMaxUrl) return;
    form.baseUrl = form.apiProtocol === 'openai' ? MINIMAX_OPENAI_BASE_URL : MINIMAX_ANTHROPIC_BASE_URL;
  }
}

onMounted(async () => {
  await providerStore.load();
  selectedProviderId.value = null;
  pageMode.value = 'list';
  Object.assign(form, emptyForm());
  showApiKey.value = false;
});


watch(
  () => form.apiProtocol,
  (protocol) => {
    if (protocol === 'openai') {
      form.upstreamFormat = (form.upstreamFormat || 'chat_completions') as UpstreamFormat;
    } else {
      form.upstreamFormat = undefined;
    }
    syncKnownProviderBaseUrl();
  },
);

function loadProviderToForm(provider: ApiProvider | null): void {
  if (!provider) {
    Object.assign(form, emptyForm());
    showApiKey.value = false;
    return;
  }
  Object.assign(form, JSON.parse(JSON.stringify(provider)));
  showApiKey.value = false;
  syncKnownProviderBaseUrl();
}

function openCreateModal(): void {
  showCreateModal.value = true;
  selectedPresetName.value = '';
  Object.assign(form, emptyForm());
  modelInput.model = '';
  envDraft.key = '';
  envDraft.value = '';
  showApiKey.value = false;
}

function closeCreateModal(): void {
  showCreateModal.value = false;
  selectedPresetName.value = '';
  Object.assign(form, emptyForm());
  modelInput.model = '';
  envDraft.key = '';
  envDraft.value = '';
  showApiKey.value = false;
}

function choosePreset(preset: ApiProvider): void {
  selectedPresetName.value = preset.name;
  Object.assign(form, JSON.parse(JSON.stringify(preset)), { id: undefined, apiKey: '' });
  syncKnownProviderBaseUrl();
}

async function createProvider(): Promise<void> {
  const created = await providerStore.addProvider({
    ...form,
    name: form.name?.trim(),
    homepageUrl: form.homepageUrl?.trim() || '',
    baseUrl: form.baseUrl?.trim(),
    enabled: form.enabled !== false,
    apiProtocol: (form.apiProtocol || 'anthropic') as ApiProtocol,
    authType: (form.authType || 'api_key') as AuthType,
    apiKey: form.apiKey?.trim() || undefined,
    models: form.models || [],
    primaryModel: form.primaryModel || form.models?.[0]?.model || '',
    extraEnv: form.extraEnv || {},
    upstreamFormat: form.apiProtocol === 'openai'
      ? ((form.upstreamFormat || 'chat_completions') as UpstreamFormat)
      : undefined,
  });
  selectedProviderId.value = created.id;
  closeCreateModal();
  backToList();
}

function selectProvider(providerId: string): void {
  selectedProviderId.value = providerId;
  loadProviderToForm(providerStore.getProvider(providerId));
}

function openEditor(providerId: string): void {
  selectProvider(providerId);
  pageMode.value = 'edit';
}

function backToList(): void {
  pageMode.value = 'list';
}

async function toggleProviderFromList(providerId: string): Promise<void> {
  await providerStore.toggleProviderEnabled(providerId);
}

function editProviderFromList(providerId: string): void {
  openEditor(providerId);
}

async function duplicateProviderFromList(providerId: string): Promise<void> {
  const duplicated = await providerStore.duplicateProvider(providerId);
  if (duplicated) {
    selectedProviderId.value = duplicated.id;
    loadProviderToForm(duplicated);
    pageMode.value = 'edit';
  }
}

async function deleteProviderFromList(providerId: string): Promise<void> {
  const isEditingCurrent = selectedProviderId.value === providerId;
  await providerStore.removeProvider(providerId);
  if (isEditingCurrent) {
    selectedProviderId.value = null;
    pageMode.value = 'list';
    Object.assign(form, emptyForm());
  }
}

async function saveSelectedProvider(): Promise<void> {
  if (!selectedProviderId.value) return;
  await providerStore.updateProvider(selectedProviderId.value, {
    ...form,
    name: form.name?.trim(),
    homepageUrl: form.homepageUrl?.trim() || '',
    baseUrl: form.baseUrl?.trim(),
    enabled: form.enabled !== false,
    apiProtocol: (form.apiProtocol || 'anthropic') as ApiProtocol,
    authType: (form.authType || 'api_key') as AuthType,
    apiKey: form.apiKey?.trim() || undefined,
    models: form.models || [],
    primaryModel: form.primaryModel || form.models?.[0]?.model || '',
    extraEnv: form.extraEnv || {},
    upstreamFormat: form.apiProtocol === 'openai'
      ? ((form.upstreamFormat || 'chat_completions') as UpstreamFormat)
      : undefined,
  });
  loadProviderToForm(providerStore.getProvider(selectedProviderId.value));
  window.alert('保存成功');
  backToList();
}

function addModel(): void {
  const model = modelInput.model.trim();
  if (!model) return;
  const models = [...(form.models || [])];
  const next = { model, modelName: model };
  const existingIndex = models.findIndex((item) => item.model === model);
  if (existingIndex >= 0) {
    models.splice(existingIndex, 1, next);
  } else {
    models.push(next);
  }
  form.models = models;
  if (!form.primaryModel) {
    form.primaryModel = model;
  }
  modelInput.model = '';
}

function removeModel(index: number): void {
  const models = [...(form.models || [])];
  models.splice(index, 1);
  form.models = models;
  if (form.primaryModel && !models.some((item) => item.model === form.primaryModel)) {
    form.primaryModel = models[0]?.model || '';
  }
}

function addEnv(): void {
  const key = envDraft.key.trim();
  if (!key) return;
  form.extraEnv = {
    ...(form.extraEnv || {}),
    [key]: envDraft.value,
  };
  envDraft.key = '';
  envDraft.value = '';
}

function removeEnv(key: string): void {
  const next = { ...(form.extraEnv || {}) };
  delete next[key];
  form.extraEnv = next;
}

function toggleApiKeyVisibility(): void {
  showApiKey.value = !showApiKey.value;
}

</script>

<template>
  <div class="provider-page">
    <template v-if="pageMode === 'list'">
      <section class="provider-hero-card">
        <div class="provider-hero-copy">
          <div class="provider-hero-title">供应商配置</div>
          <div class="provider-hero-desc">统一管理默认供应商、协议地址与模型配置。</div>
        </div>
      </section>

      <section class="provider-system-card">
        <div class="provider-hero-toggle">
          <div class="provider-hero-toggle-text">
            <div class="provider-hero-toggle-title">继承系统配置</div>
            <div class="provider-hero-toggle-desc">开启后将不注入自定义环境变量，也不会启用这里设置的默认供应商。</div>
          </div>
          <button class="toggle-btn" :class="{ active: inheritSystemConfig }" @click="providerStore.setInheritSystemConfig(!inheritSystemConfig)">
            <span class="toggle-thumb" />
          </button>
        </div>
        <div class="provider-system-note">
          <div class="provider-system-note-title">操作提示</div>
          <div class="provider-system-note-desc">
            切换供应商列表会同步更新 <code>~/.claude/settings.json</code>。如果你也在使用 <code>cc-switch</code> 等会修改供应商配置的工具，建议先不要在那边操作；先勾选“继承系统配置”，再去其他工具里调整，避免配置互相覆盖。
          </div>
        </div>
      </section>

      <section class="provider-list-card">
        <div class="provider-list-header">
          <div>
            <div class="provider-section-title">供应商列表</div>
            <div class="provider-section-desc">选择一个供应商进入详情页编辑；列表内同一时间只能启用一个供应商。</div>
          </div>
          <button class="provider-btn primary" @click="openCreateModal">新建供应商</button>
        </div>

        <div v-if="providers.length" class="provider-list-stack">
          <div
            v-for="provider in providers"
            :key="provider.id"
            class="provider-list-row"
            :class="{ default: activeProviderId === provider.id && !inheritSystemConfig }"
          >
            <button class="provider-list-main" @click="openEditor(provider.id)">
              <span class="provider-tile-icon">
                <HugeiconsIcon :icon="ApiIcon" :size="20" />
              </span>
              <span class="provider-list-copy">
                <span class="provider-list-name">{{ provider.name }}</span>
                <span class="provider-list-subline">
                  <span v-if="activeProviderId === provider.id && !inheritSystemConfig" class="provider-default-badge">使用中</span>
                  <span v-else>{{ provider.baseUrl || '未配置请求地址' }}</span>
                </span>
              </span>
            </button>
            <div class="provider-row-actions">
              <button
                class="provider-action-btn primary-action"
                :class="{ active: activeProviderId === provider.id && !inheritSystemConfig }"
                :disabled="activeProviderId === provider.id && !inheritSystemConfig"
                @click="toggleProviderFromList(provider.id)"
              >
                {{ activeProviderId === provider.id && !inheritSystemConfig ? '使用中' : '启用' }}
              </button>
              <button class="provider-action-icon" title="编辑" @click="editProviderFromList(provider.id)">
                <HugeiconsIcon :icon="Edit01Icon" :size="16" />
              </button>
              <button class="provider-action-icon" title="复制" @click="duplicateProviderFromList(provider.id)">
                <HugeiconsIcon :icon="Copy01Icon" :size="16" />
              </button>
              <button class="provider-action-icon danger" title="删除" @click="deleteProviderFromList(provider.id)">
                <HugeiconsIcon :icon="Delete02Icon" :size="16" />
              </button>
            </div>
          </div>
        </div>
        <div v-else class="provider-empty-card">
          <div class="provider-empty-title">还没有供应商</div>
          <div class="provider-empty-desc">点击右上角按钮，添加第一个供应商。</div>
        </div>
      </section>
    </template>

    <template v-else>
      <section class="provider-edit-page">
        <div class="provider-edit-topbar">
          <button class="provider-back-btn" @click="backToList">
            <HugeiconsIcon :icon="ArrowLeft02Icon" :size="16" />
            <span>返回供应商列表</span>
          </button>
          <div class="provider-actions compact">
            <button class="provider-btn primary" @click="saveSelectedProvider">保存修改</button>
          </div>
        </div>

        <div v-if="selectedProvider" class="provider-edit-card">
          <div class="provider-edit-header">
            <div>
              <div class="provider-detail-title">{{ selectedProvider.name }}</div>
              <div class="provider-detail-desc">编辑当前供应商的协议、请求地址、模型和额外环境变量。</div>
            </div>
            <span v-if="activeProviderId === selectedProvider.id && !inheritSystemConfig" class="provider-default-badge large">当前启用</span>
          </div>

          <div class="provider-form-grid clean">
            <label>
              <span>供应商名称</span>
              <input v-model="form.name" class="provider-input" placeholder="如：智谱 GLM" />
            </label>
            <label>
              <span>官网链接</span>
              <input v-model="form.homepageUrl" class="provider-input" placeholder="https://open.bigmodel.cn" />
            </label>
            <label>
              <span>API 格式</span>
              <div class="protocol-radio-group protocol-segmented">
                <label class="protocol-radio-option slim">
                  <input v-model="form.apiProtocol" type="radio" value="anthropic" />
                  <span>Anthropic</span>
                </label>
                <label class="protocol-radio-option slim">
                  <input v-model="form.apiProtocol" type="radio" value="openai" />
                  <span>OpenAI</span>
                </label>
              </div>
            </label>
            <label>
              <span>请求地址</span>
              <input v-model="form.baseUrl" class="provider-input" placeholder="https://open.bigmodel.cn/api/paas/v4" />
            </label>
            <label v-if="form.apiProtocol === 'openai'" class="provider-span-2">
              <span>上游格式</span>
              <div class="provider-select-wrap">
                <select v-model="form.upstreamFormat" class="provider-input provider-select">
                  <option value="chat_completions">Chat Completions</option>
                  <option value="responses">Responses API</option>
                </select>
              </div>
            </label>
            <label class="provider-span-2">
              <span>API Key</span>
              <div class="provider-input-wrap">
                <input v-model="form.apiKey" class="provider-input has-suffix-btn" :type="showApiKey ? 'text' : 'password'" placeholder="输入 API Key" />
                <button class="provider-input-suffix-btn" type="button" :title="showApiKey ? '隐藏 API Key' : '显示 API Key'" :aria-label="showApiKey ? '隐藏 API Key' : '显示 API Key'" @click="toggleApiKeyVisibility">
                  <HugeiconsIcon :icon="showApiKey ? ViewOffIcon : EyeIcon" :size="18" />
                </button>
              </div>
            </label>
          </div>

          <div class="provider-subsection card-section">
            <div class="provider-subsection-head">
              <div>
                <div class="provider-subtitle">支持的模型</div>
                <div class="provider-subdesc">在列表中勾选默认模型，右侧统一删除。</div>
              </div>
            </div>
            <div v-if="(form.models || []).length" class="provider-model-list">
              <div v-for="(item, index) in form.models" :key="item.model" class="provider-model-row">
                <label class="provider-primary-toggle">
                  <input v-model="form.primaryModel" type="radio" :value="item.model" />
                  <span>{{ item.model }}</span>
                </label>
                <button class="chip-remove model-remove-btn" @click="removeModel(index)">删除</button>
              </div>
            </div>
            <div v-else class="provider-inline-empty">暂未添加模型</div>
            <div class="provider-add-model-row">
              <input v-model="modelInput.model" class="provider-input" placeholder="模型 ID，如 glm-4.5" @keydown.enter.prevent="addModel" />
              <button class="provider-btn secondary" @click="addModel">添加模型</button>
            </div>
          </div>

          <div class="provider-subsection card-section">
            <div class="provider-subsection-head">
              <div>
                <div class="provider-subtitle">额外环境变量</div>
                <div class="provider-subdesc">仅在当前供应商启动时注入。</div>
              </div>
            </div>
            <div class="provider-env-grid">
              <input v-model="envDraft.key" class="provider-input" placeholder="KEY" @keydown.enter.prevent="addEnv" />
              <input v-model="envDraft.value" class="provider-input" placeholder="VALUE" @keydown.enter.prevent="addEnv" />
              <button class="provider-btn secondary" @click="addEnv">添加变量</button>
            </div>
            <div v-if="Object.keys(form.extraEnv || {}).length" class="provider-env-list">
              <div v-for="(value, key) in form.extraEnv" :key="key" class="provider-env-row">
                <code>{{ key }}</code>
                <span>{{ value }}</span>
                <button class="chip-remove model-remove-btn" @click="removeEnv(key)">删除</button>
              </div>
            </div>
            <div v-else class="provider-inline-empty">暂无额外环境变量</div>
          </div>
        </div>
      </section>
    </template>

    <div v-if="showCreateModal" class="provider-modal-mask">
      <div class="provider-modal clean-modal">
        <div class="provider-modal-header">
          <div>
            <div class="provider-modal-title">新增供应商</div>
            <div class="provider-modal-subtitle">先选择预设模板，再补全基础配置。</div>
          </div>
          <button class="provider-btn secondary" @click="closeCreateModal">关闭</button>
        </div>

        <div class="provider-modal-layout">
          <div class="provider-modal-presets">
            <div class="preset-panel-title">预设供应商</div>
            <div class="preset-list">
              <button class="preset-list-item" :class="{ active: selectedPresetName === '' }" @click="selectedPresetName = ''; Object.assign(form, emptyForm())">自定义配置</button>
              <button v-for="preset in presets" :key="preset.name" class="preset-list-item" :class="{ active: selectedPresetName === preset.name }" @click="choosePreset(preset)">
                {{ preset.name }}
              </button>
            </div>
          </div>

          <div class="provider-modal-form provider-form-card modal-card clean-card">
            <div class="provider-form-title">基础信息</div>
            <div class="provider-form-grid clean">
              <label>
                <span>供应商名称</span>
                <input v-model="form.name" class="provider-input" placeholder="如：智谱 GLM" />
              </label>
              <label>
                <span>官网链接</span>
                <input v-model="form.homepageUrl" class="provider-input" placeholder="https://open.bigmodel.cn" />
              </label>
              <label>
                <span>API 格式</span>
                <div class="protocol-radio-group protocol-segmented">
                  <label class="protocol-radio-option slim">
                    <input v-model="form.apiProtocol" type="radio" value="anthropic" />
                    <span>Anthropic</span>
                  </label>
                  <label class="protocol-radio-option slim">
                    <input v-model="form.apiProtocol" type="radio" value="openai" />
                    <span>OpenAI</span>
                  </label>
                </div>
              </label>
              <label>
                <span>请求地址</span>
                <input v-model="form.baseUrl" class="provider-input" placeholder="https://open.bigmodel.cn/api/paas/v4" />
              </label>
              <label v-if="form.apiProtocol === 'openai'" class="provider-span-2">
                <span>上游格式</span>
                <div class="provider-select-wrap">
                  <select v-model="form.upstreamFormat" class="provider-input provider-select">
                    <option value="chat_completions">Chat Completions</option>
                    <option value="responses">Responses API</option>
                  </select>
                </div>
              </label>
              <label class="provider-span-2">
                <span>API Key</span>
                <div class="provider-input-wrap">
                  <input v-model="form.apiKey" class="provider-input has-suffix-btn" :type="showApiKey ? 'text' : 'password'" placeholder="输入 API Key" />
                  <button class="provider-input-suffix-btn" type="button" :title="showApiKey ? '隐藏 API Key' : '显示 API Key'" :aria-label="showApiKey ? '隐藏 API Key' : '显示 API Key'" @click="toggleApiKeyVisibility">
                    <HugeiconsIcon :icon="showApiKey ? ViewOffIcon : EyeIcon" :size="18" />
                  </button>
                </div>
              </label>
            </div>

            <div class="provider-subsection card-section compact-section">
              <div class="provider-subtitle">模型</div>
              <div v-if="(form.models || []).length" class="provider-model-list">
                <div v-for="(item, index) in form.models" :key="item.model" class="provider-model-row">
                  <label class="provider-primary-toggle">
                    <input v-model="form.primaryModel" type="radio" :value="item.model" />
                    <span>{{ item.model }}</span>
                  </label>
                  <button class="chip-remove model-remove-btn" @click="removeModel(index)">删除</button>
                </div>
              </div>
              <div class="provider-add-model-row">
                <input v-model="modelInput.model" class="provider-input" placeholder="模型 ID，如 glm-4.5" @keydown.enter.prevent="addModel" />
                <button class="provider-btn secondary" @click="addModel">添加模型</button>
              </div>
            </div>

            <div class="provider-actions compact bottom-actions">
              <button class="provider-btn secondary" @click="Object.assign(form, emptyForm())">清空</button>
              <button class="provider-btn primary" @click="createProvider">添加到供应商列表</button>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.provider-page { display: flex; flex-direction: column; gap: 1rem; }
.provider-hero-card,
.provider-system-card,
.provider-list-card,
.provider-edit-card,
.provider-modal,
.provider-empty-card { border: 1px solid rgba(226,232,240,.92); background: rgba(255,255,255,.94); border-radius: 18px; box-shadow: 0 10px 28px -24px rgba(15,23,42,.18); }
.provider-hero-card,.provider-system-card { padding:1.1rem 1.2rem; }
.provider-hero-title,.provider-section-title,.provider-detail-title,.provider-modal-title,.provider-form-title { font-size:1rem; font-weight:700; color:var(--text-primary,#111827); }
.provider-hero-desc,.provider-section-desc,.provider-detail-desc,.provider-modal-subtitle,.provider-subdesc,.provider-empty-desc,.provider-inline-empty,.provider-hero-toggle-desc,.provider-list-subline,.provider-system-note-desc { color:var(--text-muted,#64748b); font-size:.84rem; line-height:1.5; }
.provider-hero-toggle { display:flex; align-items:center; justify-content:space-between; gap:1rem; }
.provider-hero-toggle-title { font-weight:600; color:var(--text-primary,#111827); margin-bottom:.15rem; }
.provider-system-card { display:flex; flex-direction:column; gap:.9rem; }
.provider-system-note { padding:.85rem .95rem; border-radius:14px; background:rgba(248,250,252,.88); border:1px solid rgba(226,232,240,.9); }
.provider-system-note-title { font-size:.82rem; font-weight:700; color:var(--text-primary,#111827); margin-bottom:.3rem; }
.provider-system-note-desc code { padding:.05rem .35rem; border-radius:999px; background:rgba(15,23,42,.06); color:var(--text-primary,#111827); font-size:.78rem; }
.toggle-btn { width:52px; height:30px; border:none; border-radius:999px; background:rgba(148,163,184,.3); position:relative; cursor:pointer; flex-shrink:0; }
.toggle-btn.active { background:var(--primary-color,#3b82f6); }
.toggle-thumb { position:absolute; top:3px; left:4px; width:24px; height:24px; border-radius:50%; background:#fff; transition:transform .2s ease; }
.toggle-btn.active .toggle-thumb { transform:translateX(20px); }
.provider-list-card,.provider-edit-page { padding:1rem; }
.provider-list-header,.provider-edit-topbar,.provider-edit-header,.provider-modal-header,.provider-subsection-head { display:flex; align-items:flex-start; justify-content:space-between; gap:1rem; }
.provider-list-stack { display:flex; flex-direction:column; gap:.75rem; margin-top:1rem; }
.provider-list-row { display:grid; grid-template-columns:minmax(0,1fr) auto; align-items:center; gap:1rem; padding:.9rem 1rem; border:1px solid rgba(226,232,240,.92); border-radius:16px; background:#fff; }
.provider-row-actions { display:flex; align-items:center; gap:.45rem; flex-wrap:wrap; justify-content:flex-end; }
.provider-action-btn { border:none; border-radius:14px; padding:.62rem 1rem; font-size:.84rem; font-weight:600; cursor:pointer; background:var(--primary-color,#3b82f6); color:#fff; }
.provider-action-btn.active { background:rgba(34,197,94,.12); color:#15803d; }
.provider-action-icon { width:36px; height:36px; border:none; border-radius:12px; background:rgba(15,23,42,.06); color:var(--text-secondary,#475569); display:inline-flex; align-items:center; justify-content:center; cursor:pointer; font-size:1rem; }
.provider-action-icon.danger { color:#dc2626; }
.provider-action-icon:disabled { opacity:.45; cursor:not-allowed; }
.provider-list-row.default { border-color:rgba(34,197,94,.28); background:linear-gradient(180deg, rgba(240,253,244,.95), rgba(255,255,255,.98)); }
.provider-list-main { display:flex; align-items:center; gap:.9rem; min-width:0; border:none; background:transparent; padding:0; text-align:left; cursor:pointer; }
.provider-tile-icon { width:40px; height:40px; border-radius:12px; background:rgba(59,130,246,.1); color:var(--primary-color,#2563eb); display:inline-flex; align-items:center; justify-content:center; font-weight:700; flex-shrink:0; }
.provider-list-copy { min-width:0; display:flex; flex-direction:column; gap:.25rem; }
.provider-list-name { font-size:.96rem; font-weight:600; color:var(--text-primary,#111827); }
.provider-default-badge { display:inline-flex; align-items:center; padding:.18rem .5rem; border-radius:999px; background:rgba(34,197,94,.12); color:#15803d; font-size:.73rem; font-weight:600; }
.provider-default-badge.large { font-size:.78rem; padding:.24rem .6rem; }
.provider-list-link,.provider-back-btn { border:none; background:transparent; color:var(--primary-color,#2563eb); font-size:.84rem; cursor:pointer; }
.provider-list-link:disabled { opacity:.45; cursor:not-allowed; }
.provider-back-btn { padding:0; font-weight:600; display:inline-flex; align-items:center; gap:.4rem; }
.provider-edit-topbar { margin-bottom:1rem; }
.provider-edit-card { padding:1rem; }
.provider-form-grid.clean { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); gap:.9rem; margin-top:1rem; }
.provider-form-grid label,.provider-subsection { display:flex; flex-direction:column; gap:.45rem; }
.provider-span-2 { grid-column:span 2; }
.provider-input-wrap { position:relative; width:100%; }
.provider-select-wrap { position:relative; width:100%; }
.provider-input { width:100%; box-sizing:border-box; border:1px solid rgba(203,213,225,.92); background:#fff; border-radius:12px; padding:.78rem .9rem; font-size:.92rem; color:var(--text-primary,#111827); }
.provider-input.has-suffix-btn { padding-right:3rem; }
.provider-select {
  appearance:none;
  -webkit-appearance:none;
  -moz-appearance:none;
  padding-right:2.8rem;
  cursor:pointer;
  line-height:1.25;
}
.provider-select-wrap::after {
  content:'';
  position:absolute;
  top:50%;
  right:1rem;
  width:.55rem;
  height:.55rem;
  border-right:2px solid rgba(71,85,105,.8);
  border-bottom:2px solid rgba(71,85,105,.8);
  transform:translateY(-65%) rotate(45deg);
  pointer-events:none;
}
.provider-input:focus { outline:none; border-color:rgba(59,130,246,.55); box-shadow:0 0 0 3px rgba(59,130,246,.08); }
.provider-input-suffix-btn { position:absolute; top:50%; right:.75rem; transform:translateY(-50%); display:inline-flex; align-items:center; justify-content:center; width:28px; height:28px; border:none; border-radius:8px; background:transparent; color:var(--text-muted,#64748b); cursor:pointer; transition:all .18s ease; }
.provider-input-suffix-btn:hover { background:rgba(15,23,42,.05); color:var(--text-primary,#111827); }
.protocol-radio-group { display:flex; align-items:center; gap:1rem; flex-wrap:wrap; min-height:44px; }
.protocol-radio-option { display:inline-flex !important; flex-direction:row !important; align-items:center !important; gap:.45rem; cursor:pointer; color:var(--text-primary,#111827); }
.protocol-radio-option input { margin:0; }
.card-section { margin-top:1rem; padding:1rem; border:1px solid rgba(226,232,240,.92); border-radius:16px; background:rgba(248,250,252,.72); }
.provider-subtitle,.preset-panel-title { font-size:.88rem; font-weight:600; color:var(--text-primary,#111827); }
.provider-model-list,.provider-env-list,.preset-list { display:flex; flex-direction:column; gap:.65rem; }
.provider-model-row,.provider-env-row,.preset-list-item { display:grid; grid-template-columns:minmax(0,1fr) auto; align-items:center; gap:.75rem; padding:.78rem .9rem; border:1px solid rgba(226,232,240,.92); border-radius:12px; background:#fff; }
.provider-env-row { grid-template-columns:160px minmax(0,1fr) auto; }
.provider-add-model-row,.provider-env-grid { margin-top:.85rem; display:grid; grid-template-columns:minmax(0,1fr) auto; gap:.75rem; }
.provider-env-grid { grid-template-columns:minmax(0,1fr) minmax(0,1fr) auto; }
.provider-primary-toggle { display:inline-flex; align-items:center; gap:.5rem; min-width:0; }
.provider-primary-toggle span,.provider-env-row span,.provider-env-row code { overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.provider-actions { display:flex; flex-wrap:wrap; gap:.6rem; }
.provider-actions.compact { justify-content:flex-end; }
.bottom-actions { margin-top:1rem; }
.provider-btn { border:none; border-radius:12px; padding:.7rem .95rem; font-size:.84rem; cursor:pointer; transition:all .18s ease; background:rgba(15,23,42,.06); color:var(--text-primary,#111827); }
.provider-btn.primary { background:var(--primary-color,#3b82f6); color:#fff; }
.provider-btn.secondary.danger,.chip-remove { color:#dc2626; }
.chip-remove { border:none; background:transparent; cursor:pointer; font-size:.84rem; }
.model-remove-btn { min-width:48px; text-align:right; }
.provider-empty-card { margin-top:1rem; padding:1.1rem; }
.provider-empty-title { font-weight:600; color:var(--text-primary,#111827); margin-bottom:.35rem; }
.provider-modal-mask { position:fixed; inset:0; background:rgba(15,23,42,.35); display:flex; align-items:center; justify-content:center; padding:2rem; z-index:30; }
.provider-modal.clean-modal { width:min(960px,100%); max-height:88vh; overflow:auto; padding:1.1rem; }
.provider-modal-layout { display:flex; flex-direction:column; gap:1rem; margin-top:1rem; }
.provider-modal-presets { display:flex; flex-direction:column; gap:.75rem; }
.preset-list { display:flex; flex-direction:row; flex-wrap:wrap; gap:.75rem; align-items:flex-start; }
.preset-list-item { width:auto; min-width:0; padding:.68rem .95rem; border:1px solid rgba(226,232,240,.92); text-align:left; cursor:pointer; flex:0 0 auto; }
.preset-list-item.active { border-color:rgba(59,130,246,.35); color:var(--primary-color,#2563eb); background:rgba(239,246,255,.92); }
.clean-card { border:1px solid rgba(226,232,240,.92); background:rgba(248,250,252,.68); box-shadow:none; }
@media (max-width: 980px) {
  .provider-hero-toggle,.provider-list-header,.provider-edit-topbar,.provider-edit-header,.provider-modal-header { flex-direction:column; align-items:flex-start; }
}
@media (max-width: 720px) {
  .provider-form-grid.clean,.provider-env-grid,.provider-add-model-row,.provider-list-row { grid-template-columns:1fr; }
  .provider-row-actions { justify-content:flex-start; }
  .provider-span-2 { grid-column:span 1; }
  .provider-env-row { grid-template-columns:1fr; }
}
</style>
