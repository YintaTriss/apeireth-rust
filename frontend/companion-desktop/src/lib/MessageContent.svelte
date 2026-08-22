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
        <!-- 他的声音（规范 §5.3/§6.1）：衬线引语级排版 .ap-voice；流式光标 .md-caret -->
        <div class="md-body ap-voice" class:streaming>
          {@html html}
          {#if streaming}
            <span class="md-caret" aria-hidden="true"></span>
          {/if}
        </div>
      {:else if streaming && !message.error && !message.toolCalls?.length}
        <!-- 正在输入 = 呼吸的金色小光环（规范 §5.3，motion.breathe ✅ 2.8s），禁止三个点 -->
        <div class="presence-halo" role="status" aria-label="他正在组织语言"><i></i></div>
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

  /* ---------- 他的声音排版语境（规范 §6.1/.ap-voice 承载于 md-body） ----------
     引语级参数（衬线/2.1 行高/0.13em 字距/31ch 行宽）由 tokens.css 的 .ap-voice 给出；
     此处只做语境化回落：代码与表格按 §6.3 纪律回到等宽/UI 栈、字距归零，
     列表与引用收紧行高以保持对话密度。 */
  .md-body.ap-voice :global(.md-list),
  .md-body.ap-voice :global(.md-quote) {
    line-height: 1.9;
  }
  .md-body.ap-voice :global(.md-table) {
    font-family: var(--ap-font-ui);
    letter-spacing: 0;
    line-height: 1.6;
    font-size: 13px;
  }
  .md-body.ap-voice :global(.md-code) {
    letter-spacing: 0;
    max-width: 100%;
  }
  .md-body.ap-voice :global(.md-inline) {
    letter-spacing: 0.01em;
  }
  .md-body.ap-voice :global(.md-h) {
    letter-spacing: 0.08em;
  }

  /* ---------- 正在输入的呼吸光环（规范 §5.3；数值 = motion.breathe ✅ index.html:32-36） ----------
     与 SceneLayer 的 pulse 元素同一语言：铂白径向微光 + 金环，2.8s 呼吸。 */
  .presence-halo {
    display: flex;
    align-items: center;
    height: 42px;
  }
  .presence-halo i {
    display: block;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    border: 1.5px solid rgba(255, 210, 122, 0.85);
    background: radial-gradient(closest-side, rgba(255, 243, 214, 0.35), rgba(255, 243, 214, 0) 72%);
    box-shadow: 0 0 14px rgba(255, 210, 122, 0.5), inset 0 0 5px rgba(255, 243, 214, 0.45);
    animation: ap-halo-breathe 2.8s ease-in-out infinite;
  }
  @keyframes ap-halo-breathe {
    0%,
    100% {
      opacity: 0.22;
      transform: scale(1);
    }
    50% {
      opacity: 0.65;
      transform: scale(1.09);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .presence-halo i {
      animation: none;
      opacity: 0.45;
    }
  }

  /* 错误行（随本波对话重构补齐，此前全库零定义） */
  .message-error {
    margin: 6px 0 0;
    font-size: 12px;
    line-height: 1.6;
    color: var(--ap-semantic-danger);
  }
</style>
