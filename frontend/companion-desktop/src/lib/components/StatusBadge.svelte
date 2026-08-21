<script lang="ts">
  let {
    variant = 'neutral',
    label,
    size = 'normal',
  }: {
    variant?: 'amber' | 'green' | 'blue' | 'danger' | 'neutral' | 'dim';
    label: string;
    size?: 'small' | 'normal';
  } = $props();
</script>

<span class="status-badge {variant} {size}">
  {label}
</span>

<style>
  .status-badge {
    display: inline-flex;
    align-items: center;
    border-radius: 999px;
    padding: 2px 8px;
    font-size: 11px;
    font-weight: 600;
    line-height: 1.2;
    white-space: nowrap;
  }
  .status-badge.small {
    padding: 1px 6px;
    font-size: 10px;
  }
  .amber {
    background: var(--amber-wash);
    color: var(--amber);
    border: 1px solid var(--amber-line);
  }
  .green {
    background: var(--green-wash);
    color: var(--green);
    border: 1px solid rgba(77, 179, 128, 0.3);
  }
  .blue {
    background: var(--blue-wash);
    color: var(--blue);
    border: 1px solid rgba(112, 163, 220, 0.3);
  }
  .danger {
    background: rgba(224, 91, 80, 0.12);
    color: var(--danger);
    border: 1px solid rgba(224, 91, 80, 0.3);
  }
  .neutral {
    background: var(--surface-3);
    color: var(--text);
    border: 1px solid var(--line);
  }
  .dim {
    background: var(--surface-2);
    color: var(--muted);
    border: 1px solid var(--line);
  }
</style>
