<script lang="ts">
  import {Copy, Check, Bot, User, Terminal} from 'lucide-svelte';
  import {renderMarkdown} from './markdown';
  import TaskCard from '../components/TaskCard.svelte';
  import ExecutionTimeline from '../components/ExecutionTimeline.svelte';
  import ToolCallCard from './components/ToolCallCard.svelte';
  import type {ChatMessage} from './types';

  let {
    message,
    onOpenTask,
    onRetry,
  }: {
    message: ChatMessage;
    onOpenTask?: (taskId: string) => void;
    onRetry?: (text: string) => void;
  } = $props();

  let copied = $state(false);

  const role = $derived(message.role);
  const text = $derived(message.text || '');
  const streaming = $derived(!!message.streaming);
  const html = $derived(role === 'assistant' && text ? renderMarkdown(text) : '');

  async function copyText() {
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      copied = true;
      setTimeout(() => { copied = false; }, 2000);
    } catch {
      // ignore
    }
  }
</script>

<div class="message-wrapper" class:user={role === 'user'} class:assistant={role === 'assistant'} class:system={role === 'system'}>
  {#if role === 'system'}
    <div class="system-message">
      <span class="system-icon"><Terminal size={12} /></span>
      <span class="system-text">{text}</span>
    </div>
  {:else}
    {#if message.events?.length}
      <ExecutionTimeline events={message.events} streaming={streaming} />
    {/if}

    {#if message.toolCalls?.length}
      <div class="tool-calls-container">
        {#each message.toolCalls as toolCall (toolCall.id)}
          <ToolCallCard {toolCall} />
        {/each}
      </div>
    {/if}

    {#if role === 'assistant'}
      {#if text}
        <div class="md-body" class:streaming>
          {@html html}
          {#if streaming}
            <span class="md-caret" aria-hidden="true"></span>
          {/if}
        </div>
      {:else if streaming && !message.error && !message.toolCalls?.length}
        <div class="typing" aria-label="正在生成"><i></i><i></i><i></i></div>
      {/if}

      {#if message.taskCard}
        <TaskCard card={message.taskCard} onOpen={onOpenTask} />
      {/if}

      {#if message.error}
        <p class="message-error" role="alert">{message.error}</p>
      {/if}

      {#if !streaming && (text || message.error)}
        <div class="message-toolbar">
          <button class="tool-icon-btn" onclick={copyText} title="复制内容" aria-label="复制">
            {#if copied}<Check size={12} class="green" />{:else}<Copy size={12} />{/if}
            <span class="btn-text">{copied ? '已复制' : '复制'}</span>
          </button>
          {#if message.modelInfo?.id}
            <span class="model-tag">{message.modelInfo.id}</span>
          {/if}
        </div>
      {/if}
    {:else}
      <div class="user-bubble">
        <p class="user-text">{text}</p>
        <button class="user-copy-btn" onclick={copyText} title="复制" aria-label="复制">
          {#if copied}<Check size={11} class="green" />{:else}<Copy size={11} />{/if}
        </button>
      </div>
    {/if}
  {/if}
</div>

<style>
  .message-wrapper {
    display: flex;
    flex-direction: column;
    width: 100%;
  }
  .system-message {
    align-self: center;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 12px;
    border-radius: 999px;
    background: var(--surface-2);
    border: 1px solid var(--line);
    color: var(--faint);
    font-size: 11px;
    font-family: var(--mono);
    margin: 6px 0;
  }
  .user-bubble {
    position: relative;
    display: inline-block;
  }

  .user-text {
    padding: 10px 14px;
    background: var(--surface-2);
    border: 1px solid var(--line);
    border-radius: 10px 10px 2px 10px;
    margin: 0;
    line-height: 1.7;
    color: var(--text);
    white-space: pre-wrap;
    word-break: break-word;
  }
  .user-copy-btn {
    position: absolute;
    top: 6px;
    right: -26px;
    opacity: 0;
    transition: opacity 0.15s ease;
    border: 0;
    background: transparent;
    color: var(--muted);
    padding: 2px;
    cursor: pointer;
  }
  .user-bubble:hover .user-copy-btn {
    opacity: 1;
  }
  .user-copy-btn:hover {
    color: var(--amber);
  }
  .tool-calls-container {
    margin-bottom: 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .message-toolbar {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 6px;
    padding-top: 4px;
  }
  .tool-icon-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    border: 0;
    background: transparent;
    color: var(--faint);
    padding: 2px 6px;
    border-radius: 4px;
    font-size: 11px;
    cursor: pointer;
    transition: color 0.15s ease;
  }
  .tool-icon-btn:hover {
    color: var(--muted);
    background: var(--surface-2);
  }
  .model-tag {
    font-family: var(--mono);
    font-size: 10px;
    color: var(--faint);
  }
  :global(.green) {
    color: var(--green);
  }
</style>
