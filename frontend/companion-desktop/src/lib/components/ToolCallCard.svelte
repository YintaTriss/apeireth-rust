<script lang="ts">
  import {
    ChevronDown,
    ChevronRight,
    Wrench,
    CheckCircle2,
    XCircle,
    Loader2,
    Clock,
    AlertTriangle,
  } from 'lucide-svelte';
  import type {ToolCallDetails} from '../types';

  let {
    toolCall,
  }: {
    toolCall: ToolCallDetails;
  } = $props();

  let expanded = $state(false);
  let showRawJson = $state(false);

  const statusMap = {
    pending: {label: '等待中', color: 'neutral', icon: Clock},
    running: {label: '执行中', color: 'amber', icon: Loader2},
    succeeded: {label: '已完成', color: 'green', icon: CheckCircle2},
    failed: {label: '失败', color: 'danger', icon: XCircle},
    cancelled: {label: '已取消', color: 'neutral', icon: AlertTriangle},
  };

  const statusConfig = $derived(statusMap[toolCall.status] || statusMap.pending);
  const StatusIcon = $derived(statusConfig.icon);

  function formatDuration(ms?: number): string {
    if (!ms && ms !== 0) return '';
    if (ms < 1000) return `${ms}ms`;
    return `${(ms / 1000).toFixed(1)}s`;
  }
</script>

<div class="tool-card" class:failed={toolCall.status === 'failed'}>
  <div class="tool-header" role="button" tabindex="0" onclick={() => expanded = !expanded} onkeydown={(e) => e.key === 'Enter' && (expanded = !expanded)}>
    <div class="tool-title-group">
      <span class="tool-icon"><Wrench size={13} /></span>
      <strong class="tool-name">{toolCall.name}</strong>
      <span class="tool-status {statusConfig.color}">
        <StatusIcon size={12} class={toolCall.status === 'running' ? 'spin' : ''} />
        {statusConfig.label}
      </span>
      {#if toolCall.durationMs}
        <span class="tool-duration">{formatDuration(toolCall.durationMs)}</span>
      {/if}
    </div>
    <button class="expand-btn" aria-label={expanded ? '收起详情' : '展开详情'}>
      {#if expanded}<ChevronDown size={14} />{:else}<ChevronRight size={14} />{/if}
    </button>
  </div>

  {#if expanded}
    <div class="tool-body">
      {#if toolCall.args || toolCall.rawArgs}
        <div class="tool-section">
          <span class="section-label">调用参数</span>
          {#if typeof toolCall.args === 'object' && toolCall.args !== null}
            <div class="args-preview">
              {#each Object.entries(toolCall.args) as [key, val]}
                <div class="arg-row">
                  <span class="arg-key">{key}:</span>
                  <span class="arg-val">{typeof val === 'string' ? val : JSON.stringify(val)}</span>
                </div>
              {/each}
            </div>
          {:else}
            <pre class="code-block">{toolCall.rawArgs || JSON.stringify(toolCall.args, null, 2)}</pre>
          {/if}
        </div>
      {/if}

      {#if toolCall.resultSummary}
        <div class="tool-section">
          <span class="section-label">执行结果</span>
          <p class="result-text">{toolCall.resultSummary}</p>
        </div>
      {/if}

      {#if toolCall.error}
        <div class="tool-section error-section">
          <span class="section-label">错误信息</span>
          <p class="error-text">{toolCall.error}</p>
        </div>
      {/if}

      {#if toolCall.resultFull || toolCall.args}
        <div class="tool-section raw-section">
          <button class="raw-toggle-btn" onclick={() => showRawJson = !showRawJson}>
            {showRawJson ? '隐藏完整数据' : '查看原始 JSON'}
          </button>
          {#if showRawJson}
            <pre class="code-block json-detail">{JSON.stringify({args: toolCall.args, result: toolCall.resultFull || toolCall.resultSummary, error: toolCall.error}, null, 2)}</pre>
          {/if}
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .tool-card {
    margin: 8px 0;
    border: 1px solid var(--line);
    background: var(--surface-2);
    border-radius: 8px;
    overflow: hidden;
    transition: border-color 0.15s ease;
  }
  .tool-card:hover {
    border-color: var(--line-strong);
  }
  .tool-card.failed {
    border-color: rgba(224, 91, 80, 0.3);
  }
  .tool-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 12px;
    cursor: pointer;
    user-select: none;
    background: var(--surface-2);
  }
  .tool-header:hover {
    background: var(--surface-3);
  }
  .tool-title-group {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
  }
  .tool-icon {
    color: var(--amber);
    display: grid;
    place-items: center;
  }
  .tool-name {
    font-family: var(--mono);
    color: var(--text);
    font-weight: 600;
  }
  .tool-status {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 1px 7px;
    border-radius: 999px;
    font-size: 11px;
  }
  .tool-status.green {
    background: var(--green-wash);
    color: var(--green);
  }
  .tool-status.amber {
    background: var(--amber-wash);
    color: var(--amber);
  }
  .tool-status.danger {
    background: rgba(224, 91, 80, 0.12);
    color: var(--danger);
  }
  .tool-status.neutral {
    background: var(--surface-3);
    color: var(--muted);
  }
  .tool-duration {
    color: var(--faint);
    font-family: var(--mono);
    font-size: 11px;
  }
  .expand-btn {
    border: 0;
    background: transparent;
    color: var(--muted);
    padding: 2px;
    display: grid;
    place-items: center;
  }
  .tool-body {
    padding: 10px 14px 12px;
    border-top: 1px solid var(--line);
    background: var(--surface);
    display: flex;
    flex-direction: column;
    gap: 8px;
    font-size: 12px;
  }
  .section-label {
    display: block;
    color: var(--faint);
    font-size: 11px;
    margin-bottom: 4px;
    font-weight: 500;
  }
  .args-preview {
    background: var(--surface-2);
    padding: 6px 10px;
    border-radius: 6px;
    border: 1px solid var(--line);
    font-family: var(--mono);
    font-size: 11px;
  }
  .arg-row {
    display: flex;
    gap: 6px;
    margin: 2px 0;
  }
  .arg-key {
    color: var(--amber);
  }
  .arg-val {
    color: var(--text);
    word-break: break-all;
  }
  .result-text {
    margin: 0;
    color: var(--text);
    line-height: 1.5;
  }
  .error-section {
    border-left: 2px solid var(--danger);
    padding-left: 8px;
  }
  .error-text {
    margin: 0;
    color: var(--danger);
  }
  .code-block {
    margin: 0;
    padding: 8px 10px;
    background: var(--surface-2);
    border: 1px solid var(--line);
    border-radius: 6px;
    color: var(--muted);
    font-family: var(--mono);
    font-size: 11px;
    white-space: pre-wrap;
    word-break: break-all;
    max-height: 200px;
    overflow-y: auto;
  }
  .raw-section {
    margin-top: 4px;
  }
  .raw-toggle-btn {
    border: 0;
    background: transparent;
    color: var(--faint);
    font-size: 11px;
    cursor: pointer;
    padding: 0;
    text-decoration: underline;
  }
  .raw-toggle-btn:hover {
    color: var(--amber);
  }
  .json-detail {
    margin-top: 6px;
  }
  :global(.spin) {
    animation: spin 1s linear infinite;
  }
</style>
