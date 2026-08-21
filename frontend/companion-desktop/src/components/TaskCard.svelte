<script lang="ts">
  import {ExternalLink, Zap} from 'lucide-svelte';
  import type {TaskCardInfo} from '../lib/types';

  let {
    card,
    onOpen,
  }: {
    card: TaskCardInfo;
    onOpen?: (taskId: string) => void;
  } = $props();

  const statusLabel: Record<string, string> = {
    scheduled: '已排程',
    queued: '排队中',
    running: '执行中',
    paused: '已暂停',
    awaiting_approval: '待审批',
    cancelled: '已取消',
    done: '已完成',
    failed: '失败',
  };
</script>

<div class="task-card" data-status={card.status}>
  <div class="task-card-head">
    <Zap size={14} />
    <strong>{card.title}</strong>
    <span
      class="badge"
      class:amber={card.status === 'awaiting_approval' || card.status === 'running'}
      class:green={card.status === 'done'}
      class:dim={card.status === 'queued'}
    >
      {statusLabel[card.status] || card.status}
    </span>
  </div>
  {#if card.detail}
    <p>{card.detail}</p>
  {/if}
  <footer>
    <small>任务 {card.taskId.slice(0, 8)}</small>
    <button type="button" class="text-action" onclick={() => onOpen?.(card.taskId)}>
      <ExternalLink size={12} />打开审查
    </button>
  </footer>
</div>
