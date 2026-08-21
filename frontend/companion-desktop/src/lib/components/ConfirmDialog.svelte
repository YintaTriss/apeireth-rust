<script lang="ts">
  import {AlertTriangle, X} from 'lucide-svelte';

  let {
    open = false,
    title = '请确认操作',
    message = '此操作无法撤销，确定要继续吗？',
    confirmText = '确定',
    cancelText = '取消',
    danger = false,
    onConfirm,
    onCancel,
  }: {
    open: boolean;
    title?: string;
    message?: string;
    confirmText?: string;
    cancelText?: string;
    danger?: boolean;
    onConfirm: () => void;
    onCancel: () => void;
  } = $props();

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && open) {
      e.stopPropagation();
      onCancel();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if open}
  <div class="dialog-backdrop" onclick={onCancel} role="presentation">
    <div
      class="dialog-container"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      role="dialog"
      tabindex="-1"
      aria-modal="true"
      aria-labelledby="dialog-title"
    >
      <div class="dialog-header">
        <div class="title-wrap">
          {#if danger}
            <span class="danger-icon"><AlertTriangle size={18} /></span>
          {/if}
          <h3 id="dialog-title">{title}</h3>
        </div>
        <button class="close-btn" onclick={onCancel} aria-label="关闭">
          <X size={16} />
        </button>
      </div>

      <div class="dialog-body">
        <p>{message}</p>
      </div>

      <div class="dialog-footer">
        <button class="quiet-btn" onclick={onCancel}>{cancelText}</button>
        <button class={danger ? 'danger-btn' : 'primary-btn'} onclick={onConfirm}>{confirmText}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .dialog-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.65);
    backdrop-filter: blur(4px);
    display: grid;
    place-items: center;
    z-index: 1000;
    padding: 20px;
    animation: fadeIn 0.15s ease-out;
  }
  .dialog-container {
    width: 100%;
    max-width: 440px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: 12px;
    box-shadow: var(--shadow);
    overflow: hidden;
  }
  .dialog-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 20px 12px;
    border-bottom: 1px solid var(--line);
  }
  .title-wrap {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .danger-icon {
    color: var(--danger);
    display: grid;
    place-items: center;
  }
  .dialog-header h3 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    color: var(--text);
  }
  .close-btn {
    border: 0;
    background: transparent;
    color: var(--muted);
    padding: 4px;
    border-radius: 6px;
    display: grid;
    place-items: center;
    cursor: pointer;
  }
  .close-btn:hover {
    background: var(--surface-2);
    color: var(--text);
  }
  .dialog-body {
    padding: 18px 20px;
  }
  .dialog-body p {
    margin: 0;
    font-size: 13px;
    line-height: 1.6;
    color: var(--muted);
  }
  .dialog-footer {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    padding: 12px 20px 16px;
    background: var(--surface-2);
    border-top: 1px solid var(--line);
  }
  .quiet-btn {
    padding: 7px 14px;
    border-radius: 6px;
    background: transparent;
    border: 1px solid var(--line-strong);
    color: var(--muted);
    font-size: 13px;
    cursor: pointer;
  }
  .quiet-btn:hover {
    color: var(--text);
    background: var(--surface-3);
  }
  .primary-btn {
    padding: 7px 16px;
    border-radius: 6px;
    background: var(--amber);
    border: 1px solid var(--amber);
    color: #1a1408;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
  }
  .primary-btn:hover {
    background: var(--amber-hi);
  }
  .danger-btn {
    padding: 7px 16px;
    border-radius: 6px;
    background: var(--danger);
    border: 1px solid var(--danger);
    color: #fff;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
  }
  .danger-btn:hover {
    opacity: 0.9;
  }
  @keyframes fadeIn {
    from { opacity: 0; transform: scale(0.98); }
    to { opacity: 1; transform: scale(1); }
  }
</style>
