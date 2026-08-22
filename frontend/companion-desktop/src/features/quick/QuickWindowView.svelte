<script lang="ts">
  import {onMount} from 'svelte';
  import {ArrowUp, Loader2, Sparkles, X} from 'lucide-svelte';
  import StatusDot from '../../components/StatusDot.svelte';
  import {renderMarkdown} from '../../lib/markdown';
  import {checkHealthDetailed, createAgentRuntime, loadConfig} from '../../lib/runtime';
  import type {ApeirethConfig, HealthState} from '../../lib/types';

  let config = $state<ApeirethConfig>(loadConfig());
  let query = $state('');
  let responseText = $state('');
  let busy = $state(false);
  let error = $state('');
  let inputEl = $state<HTMLInputElement | null>(null);
  let health = $state<HealthState>('connecting');

  const runtime = createAgentRuntime(config);
  const responseHtml = $derived(responseText ? renderMarkdown(responseText) : '');
  const healthText = $derived(
    busy
      ? '生成中'
      : health === 'online' || health === 'ready'
        ? '已连接'
        : health === 'connecting'
          ? '连接中…'
          : health === 'degraded'
            ? '部分可用'
            : '未连接',
  );
  const dotOff = $derived(!busy && (health === 'offline' || health === 'error'));
  const dotActive = $derived(
    busy || health === 'connecting' || health === 'generating' || health === 'degraded',
  );

  async function probeHealth(): Promise<void> {
    try {
      const report = await checkHealthDetailed(config.baseUrl, config.apiKey);
      health = report.overall;
    } catch {
      health = 'offline';
    }
  }

  async function handleClose(): Promise<void> {
    try {
      const {getCurrentWebviewWindow} = await import('@tauri-apps/api/webviewWindow');
      await getCurrentWebviewWindow().hide();
    } catch {
      window.close();
    }
  }

  async function submit(): Promise<void> {
    const text = query.trim();
    if (!text || busy) return;
    busy = true;
    error = '';
    responseText = '';

    try {
      await runtime.run(
        {
          messages: [{role: 'user', content: text}],
          model: {id: config.model, provider: 'apeireth'},
          sessionId: 'quick-session',
          context: {user: '主人'},
        },
        (event) => {
          if (event.type === 'text-delta') {
            responseText += event.text;
          }
        },
      );
    } catch (caught) {
      error = caught instanceof Error ? caught.message : String(caught);
    } finally {
      busy = false;
    }
  }

  onMount(() => {
    // 透明窗钩子: html/body 背景透明, 配壳的 transparent 窗 (base.css html.companion-bg)
    document.documentElement.classList.add('companion-bg');
    inputEl?.focus();
    void probeHealth();
    const probeTimer = window.setInterval(() => void probeHealth(), 15_000);
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        void handleClose();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => {
      document.documentElement.classList.remove('companion-bg');
      window.clearInterval(probeTimer);
      window.removeEventListener('keydown', handleKeyDown);
    };
  });
</script>

<div class="quick-window-shell">
  <div class="quick-header" data-tauri-drag-region>
    <div class="quick-brand" title="后端：{healthText}">
      <StatusDot size="tiny" off={dotOff} active={dotActive} />
      <Sparkles size={14} class="exec-icon-accent" />
      <span>Apeireth 快捷助手</span>
    </div>
    <button type="button" class="quick-close-btn" onclick={handleClose} aria-label="关闭">
      <X size={13} />
    </button>
  </div>

  <div class="quick-input-container">
    <input
      bind:this={inputEl}
      bind:value={query}
      placeholder="快速向阿佩瑞斯提问、记录想法或下达指令…"
      onkeydown={(e) => {
        if (e.key === 'Enter' && !e.shiftKey) {
          e.preventDefault();
          void submit();
        }
      }}
    />
    <button
      type="button"
      class="primary-button quick-send-btn"
      disabled={busy || !query.trim()}
      onclick={submit}
      aria-label="发送"
    >
      {#if busy}
        <Loader2 size={14} class="spinner" />
      {:else}
        <ArrowUp size={14} />
      {/if}
    </button>
  </div>

  <div class="quick-tags-row">
    <button type="button" class="quick-tag" onclick={() => { query = '总结今天的核心记忆与进展'; void submit(); }}>
      今日记忆总结
    </button>
    <button type="button" class="quick-tag" onclick={() => { query = '检查当前正在推进的目标状态'; void submit(); }}>
      目标状态检查
    </button>
    <button type="button" class="quick-tag" onclick={() => { query = '系统与器官健康度状态'; void submit(); }}>
      系统器官体检
    </button>
  </div>

  {#if responseText || busy || error}
    <div class="quick-response-area md-body">
      {#if responseText}
        {@html responseHtml}
        {#if busy}<span class="md-caret" aria-hidden="true"></span>{/if}
      {:else if busy}
        <div class="typing"><i></i><i></i><i></i></div>
      {/if}
      {#if error}
        <p class="error-banner" role="alert">{error}</p>
      {/if}
    </div>
  {/if}
</div>
