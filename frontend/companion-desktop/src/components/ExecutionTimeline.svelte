<script lang="ts">
  import {
    AlertTriangle,
    CheckCircle2,
    ChevronDown,
    ChevronRight,
    CircleDot,
    Loader2,
    Sparkles,
    Terminal,
    Wrench,
    XCircle,
  } from 'lucide-svelte';
  import type {ChatMessageEvent} from '../lib/types';

  let {
    events = [],
    streaming = false,
  }: {
    events?: ChatMessageEvent[];
    /** When false, orphan running/pending non-task steps render as done (no forever spinner). */
    streaming?: boolean;
  } = $props();

  let open = $state(false);
  let expanded = $state<Record<string, boolean>>({});
  let userToggled = $state(false);

  function displayStatus(event: ChatMessageEvent): string | undefined {
    const status = event.status;
    if (!status) return streaming ? 'running' : 'done';
    if (status === 'awaiting_approval') return status;
    // Task-bound steps may keep updating after the chat stream ends.
    if (event.taskId && (status === 'running' || status === 'pending')) return status;
    if (!streaming && (status === 'running' || status === 'pending')) return 'done';
    return status;
  }

  const items = $derived(
    [...(events || [])]
      .filter((event) => ['tool', 'mcp', 'task', 'agent', 'error'].includes(event.kind))
      .sort((a, b) => (a.ts || 0) - (b.ts || 0)),
  );

  const resolved = $derived(
    items.map((event) => ({event, status: displayStatus(event)})),
  );

  const summary = $derived.by(() => {
    const total = resolved.length;
    if (!total) return '';
    const running = resolved.filter((item) => item.status === 'running' || item.status === 'pending').length;
    const failed = resolved.filter((item) => item.status === 'failed' || item.event.kind === 'error').length;
    const approval = resolved.filter((item) => item.status === 'awaiting_approval').length;
    if (running) return `正在处理 ${running} 项`;
    if (approval) return `${approval} 项待审批`;
    if (failed) return `${failed} 项失败 · 共 ${total} 项`;
    return `已完成 ${total} 项`;
  });

  $effect(() => {
    if (!userToggled) {
      const anyActive = resolved.some((item) => item.status === 'running' || item.status === 'awaiting_approval');
      open = anyActive;
    }
  });

  function toggle(): void {
    userToggled = true;
    open = !open;
  }

  function toggleDetail(id: string): void {
    expanded[id] = !expanded[id];
  }
</script>

{#if resolved.length}
  <div class="exec-timeline" class:open>
    <button type="button" class="exec-summary" onclick={toggle} aria-expanded={open}>
      <span class="exec-summary-main">
        {#if open}
          <ChevronDown size={14} />
        {:else}
          <ChevronRight size={14} />
        {/if}
        <Wrench size={13} class="exec-icon-accent" />
        <strong>执行过程</strong>
        {#if summary}
          <span class="exec-pill">{summary}</span>
        {/if}
      </span>
      <span class="exec-count">{resolved.length} 步</span>
    </button>

    {#if open}
      <ol class="exec-list">
        {#each resolved as {event, status}}
          {@const hasDetail = !!(event.action || event.receipt)}
          <li class="exec-step" data-status={status} data-kind={event.kind}>
            <div class="exec-step-line">
              <span class="exec-status-icon">
                {#if status === 'running' || status === 'pending'}
                  <Loader2 size={13} class="spinner" />
                {:else if status === 'awaiting_approval'}
                  <AlertTriangle size={13} class="warn" />
                {:else if status === 'failed' || event.kind === 'error'}
                  <XCircle size={13} class="err" />
                {:else if event.kind === 'mcp'}
                  <Sparkles size={13} class="ok" />
                {:else if event.kind === 'task'}
                  <CircleDot size={13} class="ok" />
                {:else}
                  <CheckCircle2 size={13} class="ok" />
                {/if}
              </span>
              <span class="exec-kind-tag">{event.kind}</span>
              {#if typeof event.tier === 'number' && event.tier > 0}
                <span class="exec-tier-tag" data-tier={event.tier}>T{event.tier}</span>
              {/if}
              <span class="exec-step-text">{event.text}</span>
              {#if hasDetail}
                <button
                  type="button"
                  class="exec-detail-toggle"
                  onclick={() => toggleDetail(event.id)}
                  aria-label="切换详情"
                >
                  {#if expanded[event.id]}
                    <ChevronDown size={12} />
                  {:else}
                    <ChevronRight size={12} />
                  {/if}
                </button>
              {/if}
            </div>

            {#if hasDetail && expanded[event.id]}
              <div class="exec-detail-body">
                {#if event.action}
                  <div class="exec-detail-row">
                    <Terminal size={11} />
                    <code>{event.action}</code>
                  </div>
                {/if}
                {#if event.receipt}
                  <pre class="exec-receipt">{event.receipt}</pre>
                {/if}
              </div>
            {/if}
          </li>
        {/each}
      </ol>
    {/if}
  </div>
{/if}
