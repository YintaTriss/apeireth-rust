<script lang="ts">
  import {Loader2} from 'lucide-svelte';

  let {
    message = '正在加载数据…',
  }: {
    message?: string;
  } = $props();
</script>

<div class="loading-state-wrap" role="status">
  <Loader2 size={24} class="spin-icon" />
  <p class="loading-text">{message}</p>
</div>

<style>
  .loading-state-wrap {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 60px 24px;
    color: var(--muted);
  }
  :global(.spin-icon) {
    animation: spin 1s linear infinite;
    color: var(--amber);
    margin-bottom: 12px;
  }
  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }
  .loading-text {
    margin: 0;
    font-size: 13px;
    color: var(--faint);
  }
</style>
