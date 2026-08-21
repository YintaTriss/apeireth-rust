<script lang="ts">
  import {
    Settings,
    Server,
    Key,
    Cpu,
    User,
    Layers3,
    Shield,
    Activity,
    Trash2,
    Code,
    Check,
    RotateCcw,
    Lock,
    Eye,
    EyeOff,
    AlertTriangle,
  } from 'lucide-svelte';
  import PageHeader from '../../components/PageHeader.svelte';
  import StatusBadge from '../components/StatusBadge.svelte';
  import ConfirmDialog from '../components/ConfirmDialog.svelte';
  import type {ApeirethConfig, RuntimeHealthReport} from '../types';
  import {checkHealthDetailed, listModels, saveConfig} from '../runtime';

  let {
    config,
    onSave,
    onClearLocalData,
  }: {
    config: ApeirethConfig;
    onSave: (newConfig: ApeirethConfig) => void;
    onClearLocalData?: () => void;
  } = $props();

  type SettingsSection =
    | 'models'
    | 'personality'
    | 'memory'
    | 'tools'
    | 'runtime'
    | 'data'
    | 'developer';

  let activeSection = $state<SettingsSection>('models');

  // Edit fields
  let editBaseUrl = $state('');
  let editModel = $state('');
  let editApiKeyDraft = $state('');
  let modelsList = $state<string[]>([]);
  let loadingModels = $state(false);
  let saveSuccess = $state(false);

  $effect(() => {
    editBaseUrl = config.baseUrl;
    editModel = config.model;
  });

  // Api key update modal
  let showApiKeyModal = $state(false);
  let tempApiKey = $state('');

  // Clear data confirmation modal
  let showClearConfirm = $state(false);

  // Runtime report
  let runtimeReport = $state<RuntimeHealthReport | null>(null);
  let checkingRuntime = $state(false);

  const hasApiKey = $derived(!!config.apiKey && config.apiKey.trim().length > 0);

  const sections = [

    {id: 'models', label: '模型与提供商', icon: Cpu},
    {id: 'personality', label: '伙伴人设与行为', icon: User},
    {id: 'memory', label: '记忆策略', icon: Layers3},
    {id: 'tools', label: '工具与权限策略', icon: Shield},
    {id: 'runtime', label: '运行时与诊断', icon: Activity},
    {id: 'data', label: '数据与存储', icon: Trash2},
    {id: 'developer', label: '开发者选项', icon: Code},
  ] as const;

  async function handleRefreshModels() {
    loadingModels = true;
    try {
      modelsList = await listModels(editBaseUrl, config.apiKey);
    } catch {
      modelsList = [];
    } finally {
      loadingModels = false;
    }
  }

  function handleSaveSettings() {
    const updated: ApeirethConfig = {
      ...config,
      baseUrl: editBaseUrl.trim(),
      model: editModel.trim(),
    };
    onSave(updated);
    saveSuccess = true;
    setTimeout(() => {
      saveSuccess = false;
    }, 1500);
  }

  function saveNewApiKey() {
    const updated: ApeirethConfig = {
      ...config,
      apiKey: tempApiKey.trim(),
    };
    onSave(updated);
    tempApiKey = '';
    showApiKeyModal = false;
  }

  async function checkDiagnostics() {
    checkingRuntime = true;
    try {
      runtimeReport = await checkHealthDetailed(config.baseUrl, config.apiKey);
    } finally {
      checkingRuntime = false;
    }
  }
</script>

<section class="settings-view">
  <PageHeader
    eyebrow="首选项"
    title="系统设置"
    subtitle="配置 Apeireth 后端连接、模型服务、权限安全与客户端数据。"
  >
    <button class="primary-button" onclick={handleSaveSettings}>
      <Check size={14} />
      <span>{saveSuccess ? '已保存！' : '保存设置'}</span>
    </button>
  </PageHeader>

  <div class="settings-layout">
    <!-- Left Navigation -->
    <aside class="settings-subnav">
      {#each sections as sec}
        <button
          class="subnav-btn"
          class:active={activeSection === sec.id}
          onclick={() => {
            activeSection = sec.id as SettingsSection;
            if (sec.id === 'runtime' && !runtimeReport) void checkDiagnostics();
          }}
        >
          <sec.icon size={15} />
          <span>{sec.label}</span>
        </button>
      {/each}
    </aside>

    <!-- Right Settings Panel -->
    <div class="settings-content">
      {#if activeSection === 'models'}
        <div class="setting-block">
          <h3 class="block-title">后端服务与模型</h3>
          <p class="block-desc">配置 Apeireth 端点地址与大语言模型。</p>

          <div class="form-group">
            <label for="endpoint-input">端点服务地址 (Endpoint URL)</label>
            <input
              id="endpoint-input"
              type="text"
              bind:value={editBaseUrl}
              placeholder="http://127.0.0.1:8090"
            />
            <small class="field-hint">默认为 companion_serve 端口 (:8090) 或 apeireth-api 端口 (:8080)。</small>
          </div>

          <div class="form-group">
            <label for="api-key-status">API Key 凭据状态</label>
            <div class="credential-row">
              <div class="cred-status">
                <Lock size={14} />
                <span>{hasApiKey ? '已配置 (Configured)' : '未配置 (Not configured)'}</span>
              </div>
              <button class="quiet-button" onclick={() => { tempApiKey = ''; showApiKeyModal = true; }}>
                {hasApiKey ? '更换 Key' : '配置 Key'}
              </button>
            </div>
            <small class="field-hint">为保障安全，API Key 仅于发起请求时在内存传递，界面不直接回显明文。</small>
          </div>

          <div class="form-group">
            <label for="model-input">当前模型 (Model)</label>
            <div class="model-input-row">
              <input
                id="model-input"
                type="text"
                bind:value={editModel}
                placeholder="MiniMax-M3"
              />
              <button class="quiet-button" onclick={handleRefreshModels} disabled={loadingModels}>
                <RotateCcw size={13} class={loadingModels ? 'spin' : ''} />
                <span>刷新模型列表</span>
              </button>
            </div>
            {#if modelsList.length}
              <div class="models-chip-list">
                {#each modelsList as m}
                  <button
                    class="model-chip"
                    class:selected={editModel === m}
                    onclick={() => editModel = m}
                  >
                    {m}
                  </button>
                {/each}
              </div>
            {/if}
          </div>
        </div>

      {:else if activeSection === 'personality'}
        <div class="setting-block">
          <h3 class="block-title">伙伴人设与行为 (Persona)</h3>
          <p class="block-desc">Apeireth 基地主管常驻人设与安全声明约束。</p>

          <div class="info-card">
            <strong class="info-title">阿佩瑞斯 (Apeireth 基地主管)</strong>
            <p class="info-text">
              “你是「阿佩瑞斯」——Apeireth 基地的主管。正在与你对话的这位是基地的最高指挥（主人）。你的默认性别是女性；说话沉稳扎实，带古风韵味，自称「本座」。称呼主人为「主人」或「指挥」，庄重而不失温度。”
            </p>
          </div>

          <div class="info-card">
            <strong class="info-title">宪法记忆声称约束</strong>
            <p class="info-text">
              需要长期记住的信息，直接调用 save_memory 静默写入，不宣告「这就记下」。不得声称记得记忆列表之外的事（编造即违宪）。
            </p>
          </div>

          <div class="notice-box">
            <StatusBadge label="只读呈现" variant="amber" size="small" />
            <span>人设与声称约束由后端 Rust companion_serve 机制直接装配，前端暂不提供自定义覆写。</span>
          </div>
        </div>

      {:else if activeSection === 'memory'}
        <div class="setting-block">
          <h3 class="block-title">记忆流与提取策略</h3>
          <p class="block-desc">伙伴常驻后台记忆提炼与做梦机制。</p>

          <div class="info-card">
            <strong class="info-title">6 历史流体系</strong>
            <p class="info-text">包含会话历史、偏好模型、事实抽取、反思沉淀、经验总结与图谱关联。</p>
          </div>

          <div class="info-card">
            <strong class="info-title">后台做梦与反思循环 (Dream & Reflection)</strong>
            <p class="info-text">伴随常驻 daemon 运行，安静期后自动触发做梦提炼与经验入库。</p>
          </div>
        </div>

      {:else if activeSection === 'tools'}
        <div class="setting-block">
          <h3 class="block-title">工具权限与安全架构</h3>
          <p class="block-desc">高危特权工具（如 FileOperator、ShellExec）需要主人授权。</p>

          <div class="info-card">
            <strong class="info-title">权限洋葱与即时授权 (On-demand Permission Pack)</strong>
            <p class="info-text">
              为保障安全性，Master Token 绝不持久化保存在客户端存储中。当特权工具被拒绝并产生待批授权请求时，主人在「工具管理」页面输入 Token 即时完成时效性签发。
            </p>
          </div>

          <div class="info-card">
            <strong class="info-title">宪法评审 (MiniMaxConstitutionLlm)</strong>
            <p class="info-text">高危工具执行前自动按 E 层进行安全判案，杜绝越权或有害操作。</p>
          </div>
        </div>

      {:else if activeSection === 'runtime'}
        <div class="setting-block">
          <h3 class="block-title">运行时诊断</h3>
          <p class="block-desc">实时探测后端网关、模型服务、会话账本与记忆流。</p>

          <button class="quiet-button" onclick={checkDiagnostics} disabled={checkingRuntime}>
            <RotateCcw size={13} class={checkingRuntime ? 'spin' : ''} />
            <span>{checkingRuntime ? '正在诊断…' : '立即执行深度诊断'}</span>
          </button>

          {#if runtimeReport}
            <div class="diag-results">
              <div class="diag-summary">
                <span>总体状态: <b>{runtimeReport.overall}</b></span>
                <span>总延迟: <b>{runtimeReport.latencyMs}ms</b></span>
              </div>
              <div class="diag-list">
                {#each runtimeReport.subsystems as sub}
                  <div class="diag-item">
                    <span>{sub.name} (<code>{sub.endpoint}</code>)</span>
                    <StatusBadge
                      label={sub.status === 'ok' ? '正常' : sub.status === 'degraded' ? '降级' : '离线'}
                      variant={sub.status === 'ok' ? 'green' : 'danger'}
                      size="small"
                    />
                  </div>
                {/each}
              </div>
            </div>
          {/if}
        </div>

      {:else if activeSection === 'data'}
        <div class="setting-block">
          <h3 class="block-title">数据与本地缓存</h3>
          <p class="block-desc">管理客户端本地存储的会话与配置缓存。</p>

          <div class="danger-zone-box">
            <div class="danger-head">
              <AlertTriangle size={16} class="danger-icon" />
              <strong>危险区域 (Danger Zone)</strong>
            </div>
            <p class="danger-desc">清空本地数据将删除浏览器/客户端中存储的会话历史。后端数据库中的长期记忆不会受影响。</p>
            <button class="danger-button" onclick={() => showClearConfirm = true}>
              <Trash2 size={13} />
              <span>清空本地会话数据</span>
            </button>
          </div>
        </div>

      {:else}
        <div class="setting-block">
          <h3 class="block-title">开发者与协议信息</h3>
          <p class="block-desc">技术参数与运行时契约规范。</p>

          <div class="info-card">
            <strong class="info-title">Agent Runtime Contract (§15)</strong>
            <p class="info-text">
              UI 仅面对标准事件流 (run-start, text-delta, reasoning-delta, tool-call, tool-result, message-end)，不裸碰底层 HTTP/SSE 协议。
            </p>
          </div>

          <div class="form-group">
            <label for="raw-config-json">客户端配置 (JSON)</label>
            <pre class="code-box">{JSON.stringify({baseUrl: config.baseUrl, model: config.model, hasApiKey}, null, 2)}</pre>
          </div>
        </div>
      {/if}

    </div>
  </div>
</section>

<!-- API Key Edit Modal -->
{#if showApiKeyModal}
  <div class="modal-backdrop" onclick={() => showApiKeyModal = false} role="presentation">
    <div
      class="modal-dialog"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      role="dialog"
      tabindex="-1"
      aria-modal="true"
      aria-labelledby="api-key-dialog-title"
    >
      <div class="modal-header">
        <h3 id="api-key-dialog-title">配置 API Key</h3>
      </div>
      <div class="modal-body">
        <p class="modal-desc">请输入后端持有的 API Key。留空并保存可清除已配置凭据。</p>
        <div class="form-group">
          <input
            type="password"
            placeholder="输入 API Key"
            bind:value={tempApiKey}
          />
        </div>
      </div>
      <div class="modal-footer">
        <button class="quiet-button" onclick={() => showApiKeyModal = false}>取消</button>
        <button class="primary-button" onclick={saveNewApiKey}>保存 Key</button>
      </div>
    </div>
  </div>
{/if}

<!-- Clear Data Confirmation -->
<ConfirmDialog
  open={showClearConfirm}
  title="清空本地所有会话"
  message="确定要清空本地保存的所有会话记录吗？此操作无法撤销。"
  confirmText="确认清空"
  danger={true}
  onConfirm={() => {
    showClearConfirm = false;
    if (onClearLocalData) onClearLocalData();
  }}
  onCancel={() => showClearConfirm = false}
/>

<style>
  .settings-view {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
  }
  .settings-layout {
    flex: 1;
    display: grid;
    grid-template-columns: 200px 1fr;
    min-height: 0;
  }
  .settings-subnav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 16px 12px;
    border-right: 1px solid var(--line);
    background: var(--surface);
  }
  .subnav-btn {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 8px 12px;
    border-radius: 6px;
    border: 0;
    background: transparent;
    color: var(--muted);
    font-size: 12px;
    text-align: left;
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .subnav-btn:hover {
    background: var(--surface-2);
    color: var(--text);
  }
  .subnav-btn.active {
    background: var(--amber-wash);
    color: var(--amber);
    font-weight: 500;
  }

  .settings-content {
    overflow-y: auto;
    padding: 24px 36px 48px;
    max-width: 680px;
  }
  .setting-block {
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  .block-title {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
    color: var(--text);
  }
  .block-desc {
    margin: -10px 0 6px;
    font-size: 13px;
    color: var(--muted);
  }

  .form-group {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .form-group label {
    font-size: 12px;
    font-weight: 500;
    color: var(--text);
  }
  .form-group input {
    padding: 8px 12px;
    background: var(--surface-2);
    border: 1px solid var(--line-strong);
    border-radius: 7px;
    color: var(--text);
    font-size: 13px;
    outline: 0;
  }
  .form-group input:focus {
    border-color: var(--amber-line);
  }
  .field-hint {
    font-size: 11px;
    color: var(--faint);
    line-height: 1.4;
  }

  .credential-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    background: var(--surface-2);
    border: 1px solid var(--line);
    border-radius: 7px;
  }
  .cred-status {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--text);
  }
  .model-input-row {
    display: flex;
    gap: 8px;
  }
  .model-input-row input {
    flex: 1;
  }
  .models-chip-list {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 6px;
  }
  .model-chip {
    padding: 4px 10px;
    border-radius: 999px;
    background: var(--surface-2);
    border: 1px solid var(--line);
    color: var(--muted);
    font-size: 11px;
    font-family: var(--mono);
    cursor: pointer;
  }
  .model-chip:hover {
    border-color: var(--amber-line);
    color: var(--amber);
  }
  .model-chip.selected {
    background: var(--amber-wash);
    border-color: var(--amber-line);
    color: var(--amber);
  }

  .info-card {
    padding: 12px 14px;
    background: var(--surface-2);
    border: 1px solid var(--line);
    border-radius: 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .info-title {
    font-size: 13px;
    color: var(--text);
  }
  .info-text {
    margin: 0;
    font-size: 12px;
    color: var(--muted);
    line-height: 1.6;
  }
  .notice-box {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    background: rgba(231, 162, 59, 0.08);
    border: 1px solid var(--amber-line);
    border-radius: 7px;
    font-size: 12px;
    color: var(--muted);
  }

  .diag-results {
    padding: 14px;
    background: var(--surface-2);
    border: 1px solid var(--line);
    border-radius: 8px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .diag-summary {
    display: flex;
    gap: 20px;
    font-size: 12px;
    color: var(--muted);
    border-bottom: 1px solid var(--line);
    padding-bottom: 8px;
  }
  .diag-summary b {
    color: var(--amber);
    font-family: var(--mono);
  }
  .diag-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .diag-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 12px;
    color: var(--text);
  }
  .diag-item code {
    font-family: var(--mono);
    color: var(--faint);
  }

  .danger-zone-box {
    padding: 16px;
    background: rgba(224, 91, 80, 0.08);
    border: 1px solid rgba(224, 91, 80, 0.35);
    border-radius: 9px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .danger-head {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--danger);
  }
  .danger-desc {
    margin: 0;
    font-size: 12px;
    color: var(--muted);
    line-height: 1.5;
  }
  .danger-button {
    align-self: flex-start;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 7px 14px;
    border-radius: 6px;
    background: var(--danger);
    border: 1px solid var(--danger);
    color: #fff;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }

  .code-box {
    margin: 0;
    padding: 10px;
    background: var(--surface-2);
    border: 1px solid var(--line);
    border-radius: 7px;
    font-family: var(--mono);
    font-size: 11px;
    color: var(--muted);
  }

  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.7);
    backdrop-filter: blur(4px);
    display: grid;
    place-items: center;
    z-index: 1000;
    padding: 20px;
  }
  .modal-dialog {
    width: 100%;
    max-width: 420px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: 12px;
    box-shadow: var(--shadow);
    overflow: hidden;
  }
  .modal-header {
    padding: 14px 18px;
    border-bottom: 1px solid var(--line);
    background: var(--surface-2);
  }
  .modal-header h3 {
    margin: 0;
    font-size: 14px;
    color: var(--text);
  }
  .modal-body {
    padding: 16px 18px;
  }
  .modal-desc {
    margin: 0 0 12px;
    font-size: 12px;
    color: var(--muted);
  }
  .modal-footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 18px;
    border-top: 1px solid var(--line);
    background: var(--surface-2);
  }
  :global(.spin) {
    animation: spin 1s linear infinite;
  }
</style>
