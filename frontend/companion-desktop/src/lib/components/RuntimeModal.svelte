<script lang="ts">
  import {
    Activity,
    CheckCircle2,
    Clock,
    Database,
    Layers3,
    Loader2,
    Radio,
    RotateCw,
    Server,
    Wifi,
    WifiOff,
    Wrench,
    X,
    XCircle,
    AlertTriangle,
  } from 'lucide-svelte';
  import type {CapabilityManifest, RuntimeHealthReport} from '../types';

  let {
    open = false,
    report,
    capabilities = null,
    onClose,
    onRefresh,
    isRefreshing = false,
  }: {
    open: boolean;
    report: RuntimeHealthReport;
    capabilities: CapabilityManifest | null;
    onClose: () => void;
    onRefresh: () => Promise<void> | void;
    isRefreshing?: boolean;
  } = $props();

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && open) {
      e.stopPropagation();
      onClose();
    }
  }

  const iconMap = {
    api: Server,
    companion: Radio,
    memory: Layers3,
    tools: Wrench,
    events: Activity,
    sessions: Database,
  };

  const statusLabel = {
    connecting: '连接中…',
    online: '运行正常',
    ready: '运行正常',
    degraded: '降级运行',
    error: '运行异常',
    offline: '后端离线',
    generating: '处理中',
  };

  function formatTime(ts?: number): string {
    if (!ts) return '未检测';
    return new Date(ts).toLocaleTimeString('zh-CN', {hour: '2-digit', minute: '2-digit', second: '2-digit'});
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if open}
  <div class="modal-backdrop" onclick={onClose} role="presentation">
    <div
      class="modal-card"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      role="dialog"
      tabindex="-1"
      aria-modal="true"
      aria-labelledby="modal-title"
    >
      <div class="modal-head">
        <div class="head-title">
          <Server size={18} class="server-icon" />
          <h2 id="modal-title">运行时诊断与状态</h2>
        </div>
        <button class="close-btn" onclick={onClose} aria-label="关闭">
          <X size={16} />
        </button>
      </div>

      <div class="modal-body">
        <!-- Overall Status Summary -->
        <div class="overall-card" class:online={report.overall === 'online'} class:degraded={report.overall === 'degraded'} class:offline={report.overall === 'offline' || report.overall === 'error'}>
          <div class="overall-info">
            <div class="status-indicator">
              {#if report.overall === 'online'}
                <CheckCircle2 size={20} class="status-icon green" />
              {:else if report.overall === 'degraded'}
                <AlertTriangle size={20} class="status-icon amber" />
              {:else if report.overall === 'connecting'}
                <Loader2 size={20} class="status-icon spin amber" />
              {:else}
                <WifiOff size={20} class="status-icon danger" />
              {/if}
              <div>
                <strong>{statusLabel[report.overall] || '未知状态'}</strong>
                <span class="endpoint-text">{report.baseUrl}</span>
              </div>
            </div>
          </div>
          <div class="overall-meta">
            {#if report.latencyMs !== undefined}
              <span class="meta-item">
                <Clock size={12} />
                延迟: {report.latencyMs}ms
              </span>
            {/if}
            <span class="meta-item">
              检查时间: {formatTime(report.lastChecked)}
            </span>
          </div>
        </div>

        <!-- Subsystems List -->
        <div class="subsystems-wrap">
          <h3 class="section-title">子系统连接状态</h3>
          <div class="subsystem-grid">
            {#each report.subsystems as sub}
              {@const SubIcon = iconMap[sub.key] || Server}
              <div class="subsystem-item" class:ok={sub.status === 'ok'} class:degraded={sub.status === 'degraded'} class:offline={sub.status === 'offline'}>
                <div class="sub-head">
                  <span class="sub-icon"><SubIcon size={14} /></span>
                  <span class="sub-name">{sub.name}</span>
                  <span class="sub-badge {sub.status}">
                    {#if sub.status === 'ok'}
                      已连接
                    {:else if sub.status === 'degraded'}
                      降级
                    {:else if sub.status === 'offline'}
                      未连接
                    {:else}
                      未检测
                    {/if}
                  </span>
                </div>
                <div class="sub-endpoint">
                  <code>{sub.endpoint}</code>
                  {#if sub.latencyMs !== undefined}
                    <span class="sub-lat">{sub.latencyMs}ms</span>
                  {/if}
                </div>
                {#if sub.detail}
                  <p class="sub-detail">{sub.detail}</p>
                {/if}
              </div>
            {/each}
          </div>
        </div>
      </div>

      {#if capabilities}
        <div class="cap-section">
          <div class="cap-head">
            <span class="cap-title">能力清单 (Capability Manifest)</span>
            <span class="cap-version">schema v{capabilities.schema_version}{capabilities.legacy ? ' · legacy' : ''}</span>
          </div>
          <div class="cap-runtime">
            {capabilities.runtime.service} · {capabilities.runtime.version}
          </div>
          <div class="cap-grid">
            {#each capabilities.capabilities as group}
              <div class="cap-group">
                <div class="cap-group-name">{group.name}</div>
                <div class="cap-ops">
                  {#each group.capabilities as cap}
                    {#if cap.supported}
                      <span class="cap-tag" title={`${cap.id}${cap.write ? ' (read/write)' : cap.read ? ' (read)' : ''}`}>
                        {cap.id.split('.').pop()}
                      </span>
                    {/if}
                  {/each}
                </div>
              </div>
            {/each}
          </div>
        </div>
      {/if}

      <div class="modal-foot">
        <span class="foot-hint">提示：Apeireth 服务常驻于本地或指定端点</span>
        <button class="primary-btn" onclick={() => onRefresh()} disabled={isRefreshing}>
          <RotateCw size={13} class={isRefreshing ? 'spin' : ''} />
          <span>{isRefreshing ? '正在检查…' : '重新检测 / 连接'}</span>
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.7);
    backdrop-filter: blur(4px);
    display: grid;
    place-items: center;
    z-index: 1000;
    padding: 20px;
    animation: fadeIn 0.15s ease-out;
  }
  .modal-card {
    width: 100%;
    max-width: 580px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: 12px;
    box-shadow: var(--shadow);
    overflow: hidden;
    display: flex;
    flex-direction: column;
    max-height: 85vh;
  }
  .modal-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 20px;
    border-bottom: 1px solid var(--line);
    background: var(--surface-2);
  }
  .head-title {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text);
  }
  .head-title h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
  }
  :global(.server-icon) {
    color: var(--amber);
  }
  .close-btn {
    border: 0;
    background: transparent;
    color: var(--muted);
    padding: 4px;
    border-radius: 6px;
    cursor: pointer;
    display: grid;
    place-items: center;
  }
  .close-btn:hover {
    background: var(--surface-3);
    color: var(--text);
  }
  .modal-body {
    padding: 20px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  .overall-card {
    padding: 16px 18px;
    border-radius: 9px;
    background: var(--surface-2);
    border: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .overall-card.online {
    border-color: rgba(77, 179, 128, 0.35);
    background: linear-gradient(180deg, var(--surface-2) 0%, rgba(77, 179, 128, 0.05) 100%);
  }
  .overall-card.degraded {
    border-color: var(--amber-line);
  }
  .overall-card.offline {
    border-color: rgba(224, 91, 80, 0.35);
  }
  .status-indicator {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .status-indicator strong {
    display: block;
    font-size: 14px;
    color: var(--text);
  }
  .endpoint-text {
    font-family: var(--mono);
    font-size: 11px;
    color: var(--faint);
  }
  :global(.status-icon.green) { color: var(--green); }
  :global(.status-icon.amber) { color: var(--amber); }
  :global(.status-icon.danger) { color: var(--danger); }
  .overall-meta {
    display: flex;
    gap: 16px;
    font-size: 11px;
    color: var(--muted);
    border-top: 1px solid var(--line);
    padding-top: 10px;
  }
  .meta-item {
    display: flex;
    align-items: center;
    gap: 5px;
    font-family: var(--mono);
  }
  .section-title {
    margin: 0 0 10px;
    font-size: 12px;
    font-weight: 600;
    color: var(--muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  .subsystem-grid {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 10px;
  }
  .subsystem-item {
    padding: 12px;
    background: var(--surface-2);
    border: 1px solid var(--line);
    border-radius: 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .subsystem-item.ok {
    border-left: 3px solid var(--green);
  }
  .subsystem-item.degraded {
    border-left: 3px solid var(--amber);
  }
  .subsystem-item.offline {
    border-left: 3px solid var(--danger);
  }
  .sub-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
  }
  .sub-icon {
    color: var(--muted);
    display: grid;
    place-items: center;
  }
  .sub-name {
    font-size: 12px;
    font-weight: 600;
    color: var(--text);
    flex: 1;
  }
  .sub-badge {
    font-size: 10px;
    padding: 1px 6px;
    border-radius: 999px;
  }
  .sub-badge.ok {
    background: var(--green-wash);
    color: var(--green);
  }
  .sub-badge.degraded {
    background: var(--amber-wash);
    color: var(--amber);
  }
  .sub-badge.offline {
    background: rgba(224, 91, 80, 0.12);
    color: var(--danger);
  }
  .sub-badge.unknown {
    background: var(--surface-3);
    color: var(--muted);
  }
  .sub-endpoint {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 10px;
    color: var(--faint);
  }
  .sub-endpoint code {
    font-family: var(--mono);
  }
  .sub-lat {
    font-family: var(--mono);
  }
  .sub-detail {
    margin: 0;
    font-size: 11px;
    color: var(--muted);
  }
  .modal-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 20px;
    border-top: 1px solid var(--line);
    background: var(--surface-2);
  }
  .foot-hint {
    font-size: 11px;
    color: var(--faint);
  }
  .primary-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 7px 14px;
    border-radius: 6px;
    background: var(--amber);
    border: 1px solid var(--amber);
    color: #1a1408;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }
  .primary-btn:hover:not(:disabled) {
    background: var(--amber-hi);
  }
  .primary-btn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  :global(.spin) {
    animation: spin 1s linear infinite;
  }
  @keyframes fadeIn {
    from { opacity: 0; transform: scale(0.98); }
    to { opacity: 1; transform: scale(1); }
  }
  @media (max-width: 520px) {
    .subsystem-grid {
      grid-template-columns: 1fr;
    }
  }

  .cap-section {
    padding: 14px 20px;
    border-top: 1px solid var(--border, rgba(255,255,255,0.08));
  }
  .cap-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 4px;
  }
  .cap-title {
    font-size: 12px;
    font-weight: 600;
    color: var(--text, #e6e6e6);
  }
  .cap-version {
    font-size: 11px;
    color: var(--text-dim, #888);
  }
  .cap-runtime {
    font-size: 11px;
    color: var(--text-dim, #888);
    margin-bottom: 10px;
    font-family: monospace;
  }
  .cap-grid {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .cap-group-name {
    font-size: 11px;
    color: var(--accent, #f5a623);
    text-transform: capitalize;
    margin-bottom: 3px;
  }
  .cap-ops {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .cap-tag {
    font-size: 10px;
    padding: 2px 6px;
    border-radius: 4px;
    background: rgba(245, 166, 35, 0.12);
    color: var(--accent, #f5a623);
    border: 1px solid rgba(245, 166, 35, 0.2);
  }
</style>
