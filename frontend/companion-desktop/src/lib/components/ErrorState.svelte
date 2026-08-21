<script lang="ts">
  import {AlertCircle, RotateCcw} from 'lucide-svelte';

  let {
    title = '加载失败',
    message = '后端服务响应异常或不可达',
    onRetry,
  }: {
    title?: string;
    message?: string;
    onRetry?: () => void;
  } = $props();
</script>

<div class="error-state-wrap" role="alert">
  <div class="error-icon"><AlertCircle size={24} /></div>
  <h3 class="error-title">{title}</h3>
  <p class="error-message">{message}</p>
  {#if onRetry}
    <button class="retry-btn" onclick={onRetry}>
      <RotateCcw size={14} />
      <span>重试</span>
    </button>
  {/if}
</div>

<style>
  .error-state-wrap {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 48px 24px;
    text-align: center;
    color: var(--muted);
  }
  .error-icon {
    width: 44px;
    height: 44px;
    border-radius: 10px;
    background: rgba(224, 91, 80, 0.12);
    border: 1px solid rgba(224, 91, 80, 0.3);
    color: var(--danger);
    display: grid;
    place-items: center;
    margin-bottom: 14px;
  }
  .error-title {
    margin: 0 0 6px;
    font-size: 15px;
    font-weight: 600;
    color: var(--text);
  }
  .error-message {
    margin: 0 0 16px;
    font-size: 13px;
    color: var(--muted);
    max-width: 460px;
    line-height: 1.6;
    word-break: break-word;
  }
  .retry-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 7px 14px;
    border-radius: 6px;
    background: var(--surface-2);
    border: 1px solid var(--line-strong);
    color: var(--text);
    font-size: 13px;
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .retry-btn:hover {
    border-color: var(--amber-line);
    color: var(--amber);
  }
</style>
