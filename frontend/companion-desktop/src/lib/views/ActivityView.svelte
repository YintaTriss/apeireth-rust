<script lang="ts">
  import {onMount, onDestroy} from 'svelte';
  import {
    Activity,
    Search,
    RotateCcw,
    Radio,
    Terminal,
    Wrench,
    Layers3,
    Sparkles,
    CheckCircle2,
    AlertTriangle,
    XCircle,
    ChevronDown,
    ChevronRight,
    Clock,
    X,
    Filter,
    Play,
    Pause,
  } from 'lucide-svelte';
  import PageHeader from '../../components/PageHeader.svelte';
  import EmptyState from '../components/EmptyState.svelte';
  import ErrorState from '../components/ErrorState.svelte';
  import LoadingState from '../components/LoadingState.svelte';
  import StatusBadge from '../components/StatusBadge.svelte';
  import type {ActivityItem, ApeirethConfig, CapabilityManifest} from '../types';
  import {fetchAuditLogs, fetchTraceDetail, capabilitySupported} from '../runtime';
  import {splitPresenceLine, type PresenceFrame} from '../presence';

  let {
    config,
    capabilities = null,
  }: {
    config: ApeirethConfig;
    capabilities: CapabilityManifest | null;
  } = $props();

  // Capability gating: trace 关联 (Phase 5).
  let canReadTrace = $derived(capabilitySupported(capabilities, 'trace.read'));

  // Trace detail modal (Phase 5): 点击带 traceId 的活动 → 打开 span 树.
  import type {TraceSpanItem} from '../runtime';
  let traceDetail = $state<{traceId: string; spans: TraceSpanItem[]; loading: boolean; error: string} | null>(null);

  async function openTrace(traceId: string): Promise<void> {
    if (!canReadTrace) return;
    traceDetail = {traceId, spans: [], loading: true, error: ''};
    const r = await fetchTraceDetail(config, traceId);
    if (Array.isArray(r)) {
      traceDetail = {traceId, spans: r, loading: false, error: ''};
    } else {
      traceDetail = {traceId, spans: [], loading: false, error: r.error};
    }
  }

  /** 把 span 列表渲染成缩进树 (按 parent_span_id 关联). */
  function spanTree(spans: TraceSpanItem[]): TraceSpanItem[] {
    // 按 started_at 升序; 根 (parent=null) 在前.
    return [...spans].sort((a, b) => a.started_at - b.started_at);
  }

  function spanIndent(spans: TraceSpanItem[], span: TraceSpanItem): number {
    let depth = 0;
    let cur = span.parent_span_id;
    const guard = new Set<string>();
    while (cur && !guard.has(cur)) {
      guard.add(cur);
      depth++;
      const parent = spans.find((s) => s.span_id === cur);
      cur = parent?.parent_span_id ?? null;
    }
    return Math.min(depth, 6);
  }

  type CategoryFilter = 'all' | 'tool' | 'agent' | 'memory' | 'workflow' | 'runtime' | 'error';
  type SeverityFilter = 'all' | 'info' | 'success' | 'warning' | 'error';

  let activities = $state<ActivityItem[]>([]);
  let loading = $state(false);
  let error = $state('');
  let searchQuery = $state('');
  let selectedCategory = $state<CategoryFilter>('all');
  let selectedSeverity = $state<SeverityFilter>('all');
  let isLive = $state(true);
  let expandedIds = $state<Record<string, boolean>>({});

  let sseEventSource: EventSource | null = null;

  const categoryIcons = {
    conversation: Radio,
    agent: Sparkles,
    tool: Wrench,
    memory: Layers3,
    workflow: Terminal,
    runtime: Activity,
    error: XCircle,
  };

  const categoryLabels = {
    all: '全部类别',
    conversation: '对话',
    agent: 'Agent',
    tool: '工具执行',
    memory: '记忆读写',
    workflow: '工作流',
    runtime: '运行时',
    error: '错误异常',
  };

  function toggleExpand(id: string) {
    expandedIds = {...expandedIds, [id]: !expandedIds[id]};
  }

  /**
   * 去重合并算法：根据 id 或 (近同时间戳 + 相同标题/工具) 防止 SSE 与持久化 Audit 重复显示
   */
  function mergeActivities(existing: ActivityItem[], incoming: ActivityItem[]): ActivityItem[] {
    const map = new Map<string, ActivityItem>();

    // Put existing
    for (const item of existing) {
      map.set(item.id, item);
    }

    // Merge incoming with soft dedup
    for (const item of incoming) {
      if (map.has(item.id)) {
        map.set(item.id, {...map.get(item.id)!, ...item});
        continue;
      }
      // Check timestamp and title collision within 1500ms
      let foundDup = false;
      for (const [_, ex] of map) {
        if (
          Math.abs(ex.timestamp - item.timestamp) < 1500 &&
          ex.title === item.title &&
          ex.summary === item.summary
        ) {
          foundDup = true;
          break;
        }
      }
      if (!foundDup) {
        map.set(item.id, item);
      }
    }

    const merged = Array.from(map.values());
    merged.sort((a, b) => b.timestamp - a.timestamp);
    return merged.slice(0, 300); // keep up to 300 events in memory
  }

  async function loadPersistedAudit() {
    loading = true;
    error = '';
    try {
      const logs = await fetchAuditLogs(config, 80);
      activities = mergeActivities(activities, logs);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  /**
   * presence 帧 → 活动条目（波次 2：契约 §8.1 分流纪律，修 G5 缺口）。
   * - initiative/held（欲言又止 = 他的内心）不进对话流，但在此可见；
   * - emotion 心跳（60s tick）与 legacy 测试行不进活动流——防刷屏，不是数据丢失；
   * - memory_recall 只带 found/keywords（redacted 恒 true，原文设计上不在 SSE）。
   */
  function presenceEventToActivity(ev: PresenceFrame): ActivityItem | null {
    const ts = 'at' in ev && typeof ev.at === 'string' ? Date.parse(ev.at) || Date.now() : Date.now();
    const id = `presence-${ev.type}-${ts}-${Math.random().toString(36).slice(2, 6)}`;
    if (ev.type === 'emotion') return null; // 心跳由场景层与状态行呈现
    if (ev.type === 'initiative') {
      if (ev.outcome === 'held') {
        return {
          id, timestamp: ts, category: 'agent', source: 'sse', severity: 'info',
          title: '他欲言又止',
          summary: `门控：${ev.gate_label || ev.gate || '未知'}`,
          detail: JSON.stringify(ev, null, 2), raw: ev,
        };
      }
      return {
        id, timestamp: ts, category: 'conversation', source: 'sse', severity: 'info',
        title: '他主动开口',
        summary: ev.action ? `动作：${ev.action}` : '完整话术见对话视图',
        detail: JSON.stringify(ev, null, 2), raw: ev,
      };
    }
    if (ev.type === 'dream') {
      return {
        id, timestamp: ts, category: 'memory', source: 'sse', severity: 'success',
        title: '做梦整合完成',
        summary: `合并 ${ev.merged_count} 条记忆${ev.summary_prefix ? ` · ${ev.summary_prefix}` : ''}`,
        detail: JSON.stringify(ev, null, 2), raw: ev,
      };
    }
    if (ev.type === 'memory_recall') {
      return {
        id, timestamp: ts, category: 'memory', source: 'sse', severity: 'info',
        title: `他想起了 ${ev.found} 段记忆`,
        summary: ev.keywords?.length ? `关键词：${ev.keywords.join(' · ')}` : '（脱敏事件，不含原文）',
        detail: JSON.stringify(ev, null, 2), raw: ev,
      };
    }
    // presence_error：序列化兜底帧，显式呈报而非静默
    return {
      id, timestamp: ts, category: 'runtime', source: 'sse', severity: 'error',
      title: 'presence 频道序列化异常',
      summary: ev.error,
      detail: JSON.stringify(ev, null, 2), raw: ev,
    };
  }

  function startSseListener() {
    if (sseEventSource) {
      sseEventSource.close();
      sseEventSource = null;
    }

    if (!isLive) return;

    try {
      const base = config.baseUrl.replace(/\/+$/, '');
      sseEventSource = new EventSource(`${base}/v1/apeireth/events`);

      sseEventSource.onmessage = (event) => {
        const raw = typeof event.data === 'string' ? event.data : '';
        // 契约 §8.1 分流：行首 { → presence JSON；否则 legacy 文本（[他说]/测试事件）
        const split = splitPresenceLine(raw);
        if (split.kind === 'legacy') {
          const text = split.text ?? '';
          if (!text.startsWith('[他说]')) return; // 测试事件行 = 链路验证，不进活动流
          const said = text.slice('[他说]'.length).trim();
          activities = mergeActivities(activities, [{
            id: `legacy-say-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
            timestamp: Date.now(),
            category: 'conversation',
            title: '他主动开口',
            summary: said.length > 96 ? `${said.slice(0, 96)}…` : said,
            source: 'sse',
            severity: 'info',
          }]);
          return;
        }
        if (split.kind === 'presence' && split.event) {
          const item = presenceEventToActivity(split.event);
          if (item) activities = mergeActivities(activities, [item]);
          return;
        }
        // 其余 JSON（未来 span 帧等）：保留既有通用解析路径
        try {
          const parsed = JSON.parse(raw) as {
            id?: string;
            type?: string;
            action?: string;
            tool?: string;
            summary?: string;
            detail?: string;
            status?: string;
            ts?: number;
            trace_id?: string;
            span_id?: string;
            kind?: string;
          };

          const newEvent: ActivityItem = {
            id: parsed.id || `sse-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
            timestamp: parsed.ts || Date.now(),
            category: (parsed.tool ? 'tool' : parsed.type === 'memory' ? 'memory' : 'agent') as ActivityItem['category'],
            title: parsed.tool ? `调用工具: ${parsed.tool}` : (parsed.summary || parsed.action || 'Agent 活动'),
            summary: parsed.summary || parsed.detail || '实时事件',
            source: 'sse',
            severity: parsed.status === 'error' || parsed.status === 'failed' ? 'error' : 'info',
            detail: JSON.stringify(parsed, null, 2),
            raw: parsed,
            traceId: parsed.trace_id,
          };

          activities = mergeActivities(activities, [newEvent]);
        } catch {
          // ignore malformed SSE
        }
      };

      sseEventSource.onerror = () => {
        // SSE disconnected or endpoint offline
      };
    } catch {
      // ignore
    }
  }

  function toggleLive() {
    isLive = !isLive;
    if (isLive) {
      startSseListener();
    } else if (sseEventSource) {
      sseEventSource.close();
      sseEventSource = null;
    }
  }

  const filteredActivities = $derived.by(() => {
    let list = [...activities];

    if (selectedCategory !== 'all') {
      list = list.filter((a) => a.category === selectedCategory);
    }

    if (selectedSeverity !== 'all') {
      list = list.filter((a) => a.severity === selectedSeverity);
    }

    if (searchQuery.trim()) {
      const q = searchQuery.toLowerCase();
      list = list.filter((a) =>
        a.title.toLowerCase().includes(q) ||
        a.summary.toLowerCase().includes(q) ||
        (a.detail && a.detail.toLowerCase().includes(q)),
      );
    }

    return list;
  });

  function formatTime(ts: number): string {
    const d = new Date(ts);
    return d.toLocaleTimeString('zh-CN', {hour: '2-digit', minute: '2-digit', second: '2-digit'});
  }

  function formatRelative(ts: number): string {
    const diffSec = Math.floor((Date.now() - ts) / 1000);
    if (diffSec < 5) return '刚刚';
    if (diffSec < 60) return `${diffSec}秒前`;
    const diffMin = Math.floor(diffSec / 60);
    if (diffMin < 60) return `${diffMin}分钟前`;
    const diffHour = Math.floor(diffMin / 60);
    if (diffHour < 24) return `${diffHour}小时前`;
    return `${Math.floor(diffHour / 24)}天前`;
  }

  onMount(() => {
    void loadPersistedAudit();
    startSseListener();
  });

  onDestroy(() => {
    if (sseEventSource) {
      sseEventSource.close();
      sseEventSource = null;
    }
  });
</script>

<section class="activity-view">
  <PageHeader
    eyebrow="观察"
    title="活动时间线"
    subtitle="Apeireth 统一事件流，聚合实时涌现事件与持久审计记录，展示底层决策与工具调用。"
  >
    <button
      class="live-toggle-btn"
      class:active={isLive}
      onclick={toggleLive}
      title={isLive ? '点击暂停实时监听' : '点击开启实时监听'}
    >
      {#if isLive}
        <Radio size={13} class="live-icon spin" />
        <span>实时流 (已连接)</span>
      {:else}
        <Pause size={13} />
        <span>已暂停</span>
      {/if}
    </button>
    <button class="quiet-button" onclick={loadPersistedAudit} disabled={loading}>
      <RotateCcw size={13} class={loading ? 'spin' : ''} />
      <span>刷新审计</span>
    </button>
  </PageHeader>

  <!-- Toolbar & Filters -->
  <div class="activity-toolbar">
    <div class="search-input-wrap">
      <Search size={14} class="search-icon" />
      <input
        type="text"
        placeholder="搜索事件标题、摘要或参数…"
        bind:value={searchQuery}
      />
      {#if searchQuery}
        <button class="clear-search-btn" onclick={() => searchQuery = ''} aria-label="清除搜索">
          <X size={12} />
        </button>
      {/if}
    </div>

    <div class="filters-wrap">
      <!-- Category Tabs -->
      <div class="category-tabs">
        <button
          class="cat-btn"
          class:active={selectedCategory === 'all'}
          onclick={() => selectedCategory = 'all'}
        >全部</button>
        <button
          class="cat-btn"
          class:active={selectedCategory === 'tool'}
          onclick={() => selectedCategory = 'tool'}
        >工具</button>
        <button
          class="cat-btn"
          class:active={selectedCategory === 'agent'}
          onclick={() => selectedCategory = 'agent'}
        >Agent</button>
        <button
          class="cat-btn"
          class:active={selectedCategory === 'memory'}
          onclick={() => selectedCategory = 'memory'}
        >记忆</button>
        <button
          class="cat-btn"
          class:active={selectedCategory === 'runtime'}
          onclick={() => selectedCategory = 'runtime'}
        >运行时</button>
      </div>

      <!-- Severity Filter -->
      <select class="severity-select" bind:value={selectedSeverity} aria-label="筛选级别">
        <option value="all">全部状态</option>
        <option value="info">正常 / 信息</option>
        <option value="success">成功</option>
        <option value="warning">警告</option>
        <option value="error">异常 / 错误</option>
      </select>
    </div>
  </div>

  <!-- Timeline Body -->
  <div class="timeline-container">
    {#if loading && !activities.length}
      <LoadingState message="正在连接并加载活动时间线…" />
    {:else if error && !activities.length}
      <ErrorState title="拉取活动记录失败" message={error} onRetry={loadPersistedAudit} />
    {:else if !filteredActivities.length}
      <EmptyState
        icon="⚡"
        title={searchQuery ? '没有匹配的活动事件' : '暂无活动记录'}
        description="当与伙伴对话、调用工具或系统反思时，事件流会实时更新。"
      />
    {:else}
      <div class="timeline-stream">
        {#each filteredActivities as item (item.id)}
          {@const CategoryIcon = categoryIcons[item.category] || Activity}
          <article class="timeline-item" class:error={item.severity === 'error'} class:expanded={expandedIds[item.id]}>
            <!-- Left Axis Dot -->
            <div class="timeline-axis">
              <div class="axis-icon-dot {item.category} {item.severity}">
                <CategoryIcon size={12} />
              </div>
              <div class="axis-line"></div>
            </div>

            <!-- Content Card -->
            <div class="timeline-card">
              <div
                class="timeline-card-head"
                role="button"
                tabindex="0"
                onclick={() => toggleExpand(item.id)}
                onkeydown={(e) => e.key === 'Enter' && toggleExpand(item.id)}
              >
                <div class="head-left">
                  <span class="source-tag {item.source}">{item.source === 'sse' ? '实时流' : '审计'}</span>
                  <strong class="item-title">{item.title}</strong>
                  <StatusBadge
                    label={categoryLabels[item.category] || item.category}
                    variant={item.category === 'tool' ? 'amber' : item.category === 'error' ? 'danger' : 'neutral'}
                    size="small"
                  />
                </div>

                <div class="head-right">
                  <span class="rel-time">{formatRelative(item.timestamp)}</span>
                  <time class="abs-time">{formatTime(item.timestamp)}</time>
                  <button class="expand-arrow-btn" aria-label={expandedIds[item.id] ? '收起详情' : '展开详情'}>
                    {#if expandedIds[item.id]}<ChevronDown size={13} />{:else}<ChevronRight size={13} />{/if}
                  </button>
                </div>
              </div>

              <p class="item-summary">{item.summary}</p>

              {#if item.traceId && canReadTrace}
                <button
                  class="trace-link-btn"
                  onclick={() => openTrace(item.traceId as string)}
                  title="查看执行轨迹 (trace 树)"
                >
                  轨迹 →
                </button>
              {/if}

              {#if expandedIds[item.id] && item.detail}
                <div class="item-detail-wrap">
                  <span class="detail-label">技术详情 / 原始参数</span>
                  <pre class="detail-pre">{item.detail}</pre>
                </div>
              {/if}
            </div>
          </article>
        {/each}
      </div>
    {/if}
  </div>
</section>

{#if traceDetail}
  <div class="modal-backdrop" onclick={() => (traceDetail = null)} role="presentation">
    <div
      class="modal-dialog"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      role="dialog"
      tabindex="-1"
      aria-modal="true"
    >
      <div class="modal-header">
        <h3>执行轨迹：{traceDetail.traceId.slice(0, 12)}…</h3>
        <button class="modal-close-btn" onclick={() => (traceDetail = null)} aria-label="关闭">×</button>
      </div>
      <div class="modal-body">
        {#if traceDetail.loading}
          <p class="trace-loading">加载轨迹中…</p>
        {:else if traceDetail.error}
          <p class="trace-error">轨迹加载失败：{traceDetail.error}</p>
        {:else if traceDetail.spans.length === 0}
          <p class="trace-empty">该轨迹无 span 记录。</p>
        {:else}
          <div class="trace-tree">
            {#each spanTree(traceDetail.spans) as span}
              <div class="trace-span" style="margin-left: {spanIndent(traceDetail.spans, span) * 16}px">
                <span class="span-kind">{span.kind}</span>
                <span class="span-actor">{span.actor}</span>
                <span class="span-status status-{span.status}">{span.status}</span>
                <span class="span-summary">{span.summary || ''}</span>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .activity-view {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
  }
  .live-toggle-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border-radius: 6px;
    background: var(--surface-2);
    border: 1px solid var(--line);
    color: var(--muted);
    font-size: 12px;
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .live-toggle-btn.active {
    border-color: var(--amber-line);
    color: var(--amber);
    background: var(--amber-wash);
  }
  :global(.live-icon) {
    color: var(--amber);
  }

  .activity-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 32px 14px;
    border-bottom: 1px solid var(--line);
    flex-wrap: wrap;
  }
  .search-input-wrap {
    flex: 1;
    min-width: 240px;
    max-width: 380px;
    position: relative;
    display: flex;
    align-items: center;
  }
  :global(.search-icon) {
    position: absolute;
    left: 10px;
    color: var(--faint);
  }
  .search-input-wrap input {
    width: 100%;
    padding: 7px 28px 7px 30px;
    background: var(--surface-2);
    border: 1px solid var(--line-strong);
    border-radius: 7px;
    color: var(--text);
    font-size: 12px;
    outline: 0;
  }
  .search-input-wrap input:focus {
    border-color: var(--amber-line);
  }
  .clear-search-btn {
    position: absolute;
    right: 8px;
    border: 0;
    background: transparent;
    color: var(--faint);
    cursor: pointer;
    display: grid;
    place-items: center;
  }

  .filters-wrap {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .category-tabs {
    display: flex;
    gap: 4px;
  }
  .cat-btn {
    border: 1px solid var(--line);
    background: var(--surface-2);
    color: var(--muted);
    font-size: 11px;
    padding: 5px 10px;
    border-radius: 6px;
    cursor: pointer;
  }
  .cat-btn:hover {
    color: var(--text);
    border-color: var(--line-strong);
  }
  .cat-btn.active {
    background: var(--amber-wash);
    border-color: var(--amber-line);
    color: var(--amber);
  }
  .severity-select {
    padding: 5px 8px;
    background: var(--surface-2);
    border: 1px solid var(--line);
    border-radius: 6px;
    color: var(--muted);
    font-size: 11px;
    outline: 0;
  }

  .timeline-container {
    flex: 1;
    overflow-y: auto;
    padding: 20px 32px 40px;
  }
  .timeline-stream {
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-width: 960px;
    margin: 0 auto;
  }
  .timeline-item {
    display: flex;
    gap: 16px;
  }
  .timeline-axis {
    display: flex;
    flex-direction: column;
    align-items: center;
    width: 28px;
    flex: none;
  }
  .axis-icon-dot {
    width: 24px;
    height: 24px;
    border-radius: 50%;
    background: var(--surface-3);
    border: 1px solid var(--line-strong);
    color: var(--muted);
    display: grid;
    place-items: center;
    z-index: 1;
  }
  .axis-icon-dot.tool {
    background: var(--amber-wash);
    color: var(--amber);
    border-color: var(--amber-line);
  }
  .axis-icon-dot.error {
    background: rgba(224, 91, 80, 0.15);
    color: var(--danger);
    border-color: rgba(224, 91, 80, 0.35);
  }
  .axis-line {
    flex: 1;
    width: 1px;
    background: var(--line);
    margin-top: 4px;
  }
  .timeline-item:last-child .axis-line {
    display: none;
  }

  .timeline-card {
    flex: 1;
    margin-bottom: 12px;
    background: var(--surface-2);
    border: 1px solid var(--line);
    border-radius: 9px;
    padding: 10px 14px;
    transition: all 0.15s ease;
  }
  .timeline-card:hover {
    border-color: var(--line-strong);
    background: var(--surface-3);
  }
  .timeline-item.error .timeline-card {
    border-color: rgba(224, 91, 80, 0.3);
  }

  .timeline-card-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    cursor: pointer;
    user-select: none;
  }
  .head-left {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .source-tag {
    font-size: 10px;
    padding: 1px 6px;
    border-radius: 4px;
    font-family: var(--mono);
  }
  .source-tag.sse {
    background: var(--amber-wash);
    color: var(--amber);
  }
  .source-tag.audit {
    background: var(--blue-wash);
    color: var(--blue);
  }
  .item-title {
    font-size: 13px;
    color: var(--text);
  }
  .head-right {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .rel-time {
    font-size: 11px;
    color: var(--faint);
  }
  .abs-time {
    font-size: 10px;
    font-family: var(--mono);
    color: var(--faint);
  }
  .expand-arrow-btn {
    border: 0;
    background: transparent;
    color: var(--muted);
    padding: 2px;
    display: grid;
    place-items: center;
  }

  .item-summary {
    margin: 6px 0 0;
    font-size: 12px;
    color: var(--muted);
    line-height: 1.5;
  }

  .item-detail-wrap {
    margin-top: 10px;
    padding-top: 8px;
    border-top: 1px solid var(--line);
  }
  .detail-label {
    display: block;
    font-size: 10px;
    color: var(--faint);
    margin-bottom: 4px;
    text-transform: uppercase;
  }
  .detail-pre {
    margin: 0;
    padding: 8px 10px;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 6px;
    color: var(--muted);
    font-family: var(--mono);
    font-size: 11px;
    white-space: pre-wrap;
    word-break: break-all;
    max-height: 240px;
    overflow-y: auto;
  }
  :global(.spin) {
    animation: spin 1s linear infinite;
  }

  .trace-link-btn {
    display: inline-block;
    margin-top: 4px;
    padding: 2px 8px;
    font-size: 11px;
    border-radius: 4px;
    background: rgba(245, 166, 35, 0.12);
    color: var(--accent, #f5a623);
    border: 1px solid rgba(245, 166, 35, 0.2);
    cursor: pointer;
  }
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.7);
    backdrop-filter: blur(4px);
    display: grid;
    place-items: center;
    z-index: 1000;
    padding: 20px;
  }
  .modal-dialog {
    background: var(--bg-card, #1a1a1a);
    border: 1px solid var(--border, rgba(255,255,255,0.1));
    border-radius: 12px;
    width: 90%;
    max-width: 640px;
    max-height: 80vh;
    overflow: auto;
  }
  .modal-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 14px 20px;
    border-bottom: 1px solid var(--border, rgba(255,255,255,0.08));
  }
  .modal-header h3 {
    font-size: 14px;
    margin: 0;
  }
  .modal-close-btn {
    background: none;
    border: none;
    color: var(--text-dim, #888);
    font-size: 20px;
    cursor: pointer;
  }
  .modal-body {
    padding: 14px 20px;
    font-family: monospace;
    font-size: 12px;
  }
  .trace-tree {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .trace-span {
    display: flex;
    gap: 6px;
    align-items: center;
    padding: 4px 8px;
    border-left: 2px solid var(--border, rgba(255,255,255,0.1));
  }
  .span-kind {
    color: var(--accent, #f5a623);
    min-width: 70px;
  }
  .span-actor {
    color: var(--text-dim, #aaa);
    min-width: 90px;
  }
  .span-status {
    font-size: 10px;
    padding: 1px 5px;
    border-radius: 3px;
  }
  .status-succeeded {
    background: rgba(34, 197, 94, 0.15);
    color: #4ade80;
  }
  .status-failed {
    background: rgba(239, 68, 68, 0.15);
    color: #f87171;
  }
  .status-running {
    background: rgba(245, 166, 35, 0.15);
    color: #f5a623;
  }
  .span-summary {
    color: var(--text, #ccc);
  }
  .trace-loading, .trace-error, .trace-empty {
    color: var(--text-dim, #888);
    text-align: center;
    padding: 20px;
  }
  .trace-error {
    color: #f87171;
  }
</style>
