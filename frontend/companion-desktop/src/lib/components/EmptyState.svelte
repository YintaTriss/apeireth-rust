<script lang="ts">
  import type {Snippet} from 'svelte';

  let {
    icon = '⌁',
    title = '暂无数据',
    description = '',
    children,
  }: {
    icon?: string;
    title?: string;
    description?: string;
    children?: Snippet;
  } = $props();
</script>

<div class="empty-state-wrap">
  <div class="empty-mark" aria-hidden="true">{icon}</div>
  <h3 class="empty-title">{title}</h3>
  {#if description}
    <p class="empty-desc">{description}</p>
  {/if}
  {#if children}
    <div class="empty-actions">
      {@render children()}
    </div>
  {/if}
</div>

<style>
  .empty-state-wrap {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 56px 24px;
    text-align: center;
    color: var(--muted);
    user-select: none;
  }
  .empty-mark {
    width: 48px;
    height: 48px;
    border-radius: 12px;
    background: var(--surface-2);
    border: 1px solid var(--line);
    color: var(--amber);
    display: grid;
    place-items: center;
    font-size: 20px;
    margin-bottom: 14px;
  }
  .empty-title {
    margin: 0 0 6px;
    font-size: 15px;
    font-weight: 600;
    color: var(--text);
  }
  .empty-desc {
    margin: 0;
    font-size: 13px;
    color: var(--muted);
    max-width: 420px;
    line-height: 1.6;
  }
  .empty-actions {
    margin-top: 18px;
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    justify-content: center;
  }
</style>
