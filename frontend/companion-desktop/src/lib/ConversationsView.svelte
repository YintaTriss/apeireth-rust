<script lang="ts">
  import {onMount} from 'svelte';
  import {
    Search,
    MessageSquarePlus,
    Archive,
    Trash2,
    Pin,
    PinOff,
    Edit3,
    Clock,
    Database,
    Layers,
    RotateCcw,
    FolderKanban,
    Globe,
    Check,
    X,
    Filter,
  } from 'lucide-svelte';
  import PageHeader from '../components/PageHeader.svelte';
  import EmptyState from './components/EmptyState.svelte';
  import ErrorState from './components/ErrorState.svelte';
  import LoadingState from './components/LoadingState.svelte';
  import ConfirmDialog from './components/ConfirmDialog.svelte';
  import StatusBadge from './components/StatusBadge.svelte';
  import type {ApeirethConfig, ChatMessage, Conversation} from './types';
  import {fetchBackendSessions} from './runtime';


  let {
    conversations = [],
    activeId = '',
    config,
    onOpen,
    onCreate,
    onArchive,
    onDelete,
    onRename,
    onPin,
  }: {
    conversations: Conversation[];
    activeId: string;
    config?: ApeirethConfig;
    onOpen: (id: string) => void;
    onCreate: () => void;
    onArchive: (id: string) => void;
    onDelete: (id: string) => void;
    onRename?: (id: string, newTitle: string) => void;
    onPin?: (id: string) => void;
  } = $props();

  type TabMode = 'local' | 'backend';
  type FilterScope = 'all' | 'global' | 'project' | 'archived';
  type SortOrder = 'recent' | 'oldest' | 'messages';

  let activeTab = $state<TabMode>('local');
  let searchQuery = $state('');
  let filterScope = $state<FilterScope>('all');
  let sortOrder = $state<SortOrder>('recent');

  // Inline editing
  let editingId = $state<string | null>(null);
  let editTitleDraft = $state('');

  // Delete confirmation modal
  let deleteConfirmId = $state<string | null>(null);
  let deleteConfirmTitle = $state('');

  // Backend session store (Read-only)
  let backendSessions = $state<Array<{id: string; started_at: number; last_active_at: number; closed_at?: number; episode_count: number}>>([]);
  let backendLoading = $state(false);
  let backendError = $state('');

  async function loadBackendLedger() {
    if (!config) return;
    backendLoading = true;
    backendError = '';
    try {
      backendSessions = await fetchBackendSessions(config);
    } catch (e) {
      backendError = e instanceof Error ? e.message : String(e);
    } finally {
      backendLoading = false;
    }
  }

  function startRename(conv: Conversation) {
    editingId = conv.id;
    editTitleDraft = conv.title;
  }

  function saveRename(id: string) {
    const trimmed = editTitleDraft.trim();
    if (trimmed && onRename) {
      onRename(id, trimmed);
    }
    editingId = null;
  }

  function cancelRename() {
    editingId = null;
  }

  function promptDelete(conv: Conversation) {
    deleteConfirmId = conv.id;
    deleteConfirmTitle = conv.title;
  }

  function executeDelete() {
    if (deleteConfirmId) {
      onDelete(deleteConfirmId);
      deleteConfirmId = null;
    }
  }

  const filteredLocal = $derived.by(() => {
    let list = [...conversations];

    // Search filter
    if (searchQuery.trim()) {
      const q = searchQuery.toLowerCase();
      list = list.filter((c) =>
        c.title.toLowerCase().includes(q) ||
        c.messages.some((m: ChatMessage) => m.text?.toLowerCase().includes(q)),
      );

    }

    // Scope / Archive filter
    if (filterScope === 'archived') {
      list = list.filter((c) => !!c.archived);
    } else {
      list = list.filter((c) => !c.archived);
      if (filterScope === 'global') list = list.filter((c) => c.scope !== 'project');
      if (filterScope === 'project') list = list.filter((c) => c.scope === 'project');
    }

    // Sorting
    list.sort((a, b) => {
      // Pinned conversations always on top
      if (a.pinned && !b.pinned) return -1;
      if (!a.pinned && b.pinned) return 1;

      if (sortOrder === 'recent') return b.updatedAt - a.updatedAt;
      if (sortOrder === 'oldest') return a.createdAt - b.createdAt;
      if (sortOrder === 'messages') return b.messages.length - a.messages.length;
      return 0;
    });

    return list;
  });

  const preview = (item: Conversation) =>
    item.messages.at(-1)?.text?.replace(/\s+/g, ' ').slice(0, 80) || '尚未开始任何交谈…';

  function formatTime(ts: number): string {
    const d = new Date(ts);
    const now = new Date();
    const isToday = d.toDateString() === now.toDateString();
    if (isToday) {
      return d.toLocaleTimeString('zh-CN', {hour: '2-digit', minute: '2-digit'});
    }
    return d.toLocaleDateString('zh-CN', {month: 'numeric', day: 'numeric', hour: '2-digit', minute: '2-digit'});
  }

  onMount(() => {
    if (activeTab === 'backend') {
      void loadBackendLedger();
    }
  });
</script>

<section class="sessions-view">
  <PageHeader
    eyebrow="管理"
    title="会话管理"
    subtitle="管理本地对话上下文与后端持久账本；删除需确认，归档不丢失记录。"
  >
    <div class="header-tab-switch">
      <button
        class="tab-switch-btn"
        class:active={activeTab === 'local'}
        onclick={() => activeTab = 'local'}
      >
        <FolderKanban size={13} />
        <span>本地工作区 ({conversations.length})</span>
      </button>
      <button
        class="tab-switch-btn"
        class:active={activeTab === 'backend'}
        onclick={() => { activeTab = 'backend'; void loadBackendLedger(); }}
      >
        <Database size={13} />
        <span>后端账本 (只读)</span>
      </button>
    </div>
    <button class="primary-button" onclick={onCreate}>
      <MessageSquarePlus size={14} />
      <span>新建会话</span>
    </button>
  </PageHeader>

  {#if activeTab === 'local'}
    <!-- Toolbar -->
    <div class="sessions-toolbar">
      <div class="search-input-wrap">
        <Search size={14} class="search-icon" />
        <input
          type="text"
          placeholder="搜索会话标题或消息内容…"
          bind:value={searchQuery}
        />
        {#if searchQuery}
          <button class="clear-search-btn" onclick={() => searchQuery = ''} aria-label="清除搜索">
            <X size={12} />
          </button>
        {/if}
      </div>

      <div class="filters-group">
        <div class="scope-tabs">
          <button
            class="scope-btn"
            class:active={filterScope === 'all'}
            onclick={() => filterScope = 'all'}
          >全部</button>
          <button
            class="scope-btn"
            class:active={filterScope === 'global'}
            onclick={() => filterScope = 'global'}
          >全局</button>
          <button
            class="scope-btn"
            class:active={filterScope === 'project'}
            onclick={() => filterScope = 'project'}
          >项目</button>
          <button
            class="scope-btn"
            class:active={filterScope === 'archived'}
            onclick={() => filterScope = 'archived'}
          >已归档</button>
        </div>

        <select class="sort-select" bind:value={sortOrder} aria-label="排序方式">
          <option value="recent">最近更新</option>
          <option value="oldest">最早创建</option>
          <option value="messages">消息数量</option>
        </select>
      </div>
    </div>

    <!-- Conversation List -->
    <div class="sessions-list">
      {#if !filteredLocal.length}
        <EmptyState
          icon="💬"
          title={searchQuery ? '未搜索到匹配的会话' : filterScope === 'archived' ? '没有已归档的会话' : '暂无会话'}
          description={searchQuery ? '请尝试更换关键词搜索' : '点击右上角“新建会话”开启新的独立上下文。'}
        >
          {#if searchQuery}
            <button class="quiet-button" onclick={() => searchQuery = ''}>清除搜索</button>
          {:else}
            <button class="primary-button" onclick={onCreate}>新建会话</button>
          {/if}
        </EmptyState>
      {:else}
        {#each filteredLocal as conversation (conversation.id)}
          <article class="session-card" class:active={conversation.id === activeId} class:pinned={conversation.pinned}>
            <div class="session-main" role="button" tabindex="0" onclick={() => onOpen(conversation.id)} onkeydown={(e) => e.key === 'Enter' && onOpen(conversation.id)}>
              <div class="session-head">
                {#if editingId === conversation.id}
                  <div class="rename-box" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()} role="presentation">
                    <input
                      type="text"
                      bind:value={editTitleDraft}
                      onkeydown={(e) => {
                        if (e.key === 'Enter') saveRename(conversation.id);
                        if (e.key === 'Escape') cancelRename();
                      }}
                    />
                    <button class="rename-action-btn check" onclick={() => saveRename(conversation.id)} aria-label="保存">
                      <Check size={13} />
                    </button>
                    <button class="rename-action-btn cancel" onclick={cancelRename} aria-label="取消">
                      <X size={13} />
                    </button>
                  </div>
                {:else}
                  <div class="title-with-pin">
                    {#if conversation.pinned}
                      <span class="pin-tag" title="已置顶"><Pin size={11} /></span>
                    {/if}
                    <strong class="session-title">{conversation.title}</strong>
                  </div>
                {/if}

                <div class="session-tags">
                  <StatusBadge
                    label={conversation.scope === 'project' ? '项目' : '全局'}
                    variant={conversation.scope === 'project' ? 'amber' : 'blue'}
                    size="small"
                  />
                  {#if conversation.model}
                    <span class="model-tag">{conversation.model}</span>
                  {/if}
                  <span class="time-tag">{formatTime(conversation.updatedAt)}</span>
                </div>
              </div>

              <p class="session-preview">{preview(conversation)}</p>

              <div class="session-meta">
                <span class="msg-count">{conversation.messages.length} 条消息</span>
                {#if conversation.archived}
                  <span class="archived-tag">已归档</span>
                {/if}
              </div>
            </div>

            <!-- Actions Bar -->
            <div class="session-actions" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()} role="presentation">
              {#if onPin}
                <button
                  class="action-icon-btn"
                  title={conversation.pinned ? '取消置顶' : '置顶'}
                  aria-label={conversation.pinned ? '取消置顶' : '置顶'}
                  onclick={() => onPin && onPin(conversation.id)}
                >
                  {#if conversation.pinned}<PinOff size={13} />{:else}<Pin size={13} />{/if}
                </button>
              {/if}

              <button
                class="action-icon-btn"
                title="重命名"
                aria-label="重命名"
                onclick={() => startRename(conversation)}
              >
                <Edit3 size={13} />
              </button>

              <button
                class="action-icon-btn"
                title={conversation.archived ? '恢复会话' : '归档会话'}
                aria-label={conversation.archived ? '恢复会话' : '归档会话'}
                onclick={() => onArchive(conversation.id)}
              >
                <Archive size={13} />
              </button>

              <button
                class="action-icon-btn danger"
                title="删除会话"
                aria-label="删除会话"
                onclick={() => promptDelete(conversation)}
              >
                <Trash2 size={13} />
              </button>
            </div>
          </article>
        {/each}
      {/if}
    </div>
  {:else}
    <!-- Backend Session Store Read-only Ledger -->
    <div class="backend-ledger-wrap">
      <div class="backend-ledger-banner">
        <div class="banner-text">
          <Database size={15} class="db-icon" />
          <span>后端 SQLite 会话账本 (只读)。展示 Apeireth 核心已记录的 session 与关联 episode 计数。</span>
        </div>
        <button class="quiet-button" onclick={loadBackendLedger} disabled={backendLoading}>
          <RotateCcw size={13} class={backendLoading ? 'spin' : ''} />
          <span>刷新</span>
        </button>
      </div>

      {#if backendLoading}
        <LoadingState message="正在拉取后端会话账本…" />
      {:else if backendError}
        <ErrorState title="拉取后端会话失败" message={backendError} onRetry={loadBackendLedger} />
      {:else if !backendSessions.length}
        <EmptyState title="后端尚无持久化会话记录" description="当伙伴运行并产生记忆时，账本会自动记录。" />
      {:else}
        <div class="backend-table-wrap">
          <table class="backend-table">
            <thead>
              <tr>
                <th>会话 ID</th>
                <th>关联记录数</th>
                <th>创建时间</th>
                <th>最后活跃</th>
                <th>状态</th>
              </tr>
            </thead>
            <tbody>
              {#each backendSessions as row}
                <tr>
                  <td><code>{row.id}</code></td>
                  <td><span class="count-badge">{row.episode_count} 条</span></td>
                  <td>{row.started_at ? new Date(row.started_at > 1e11 ? row.started_at : row.started_at * 1000).toLocaleString('zh-CN') : '-'}</td>
                  <td>{row.last_active_at ? new Date(row.last_active_at > 1e11 ? row.last_active_at : row.last_active_at * 1000).toLocaleString('zh-CN') : '-'}</td>
                  <td>
                    {#if row.closed_at}
                      <StatusBadge label="已关闭" variant="neutral" size="small" />
                    {:else}
                      <StatusBadge label="活跃" variant="green" size="small" />
                    {/if}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </div>
  {/if}
</section>

<!-- Delete confirmation dialog -->
<ConfirmDialog
  open={!!deleteConfirmId}
  title="删除会话"
  message={`确定要删除会话「${deleteConfirmTitle}」吗？删除后本地对话历史将不可恢复。`}
  confirmText="确认删除"
  danger={true}
  onConfirm={executeDelete}
  onCancel={() => deleteConfirmId = null}
/>

<style>
  .sessions-view {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
  }
  .header-tab-switch {
    display: flex;
    gap: 4px;
    background: var(--surface-2);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 3px;
  }
  .tab-switch-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    border: 0;
    background: transparent;
    color: var(--muted);
    font-size: 12px;
    padding: 5px 10px;
    border-radius: 6px;
    cursor: pointer;
  }
  .tab-switch-btn:hover {
    color: var(--text);
  }
  .tab-switch-btn.active {
    background: var(--surface-3);
    color: var(--amber);
    font-weight: 500;
  }

  .sessions-toolbar {
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

  .filters-group {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .scope-tabs {
    display: flex;
    gap: 4px;
  }
  .scope-btn {
    border: 1px solid var(--line);
    background: var(--surface-2);
    color: var(--muted);
    font-size: 11px;
    padding: 5px 10px;
    border-radius: 6px;
    cursor: pointer;
  }
  .scope-btn:hover {
    color: var(--text);
    border-color: var(--line-strong);
  }
  .scope-btn.active {
    background: var(--amber-wash);
    border-color: var(--amber-line);
    color: var(--amber);
  }
  .sort-select {
    padding: 5px 8px;
    background: var(--surface-2);
    border: 1px solid var(--line);
    border-radius: 6px;
    color: var(--muted);
    font-size: 11px;
    outline: 0;
  }

  .sessions-list {
    flex: 1;
    overflow-y: auto;
    padding: 18px 32px 32px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .session-card {
    display: flex;
    align-items: stretch;
    background: var(--surface-2);
    border: 1px solid var(--line);
    border-radius: 9px;
    transition: all 0.15s ease;
    overflow: hidden;
  }
  .session-card:hover {
    border-color: var(--line-strong);
    background: var(--surface-3);
  }
  .session-card.active {
    border-color: var(--amber-line);
    background: var(--surface-3);
  }
  .session-card.pinned {
    border-left: 3px solid var(--amber);
  }
  .session-main {
    flex: 1;
    padding: 12px 16px;
    cursor: pointer;
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }
  .session-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .title-with-pin {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .pin-tag {
    color: var(--amber);
    display: grid;
    place-items: center;
  }
  .session-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .rename-box {
    display: flex;
    align-items: center;
    gap: 4px;
    flex: 1;
  }
  .rename-box input {
    padding: 4px 8px;
    background: var(--surface);
    border: 1px solid var(--amber-line);
    border-radius: 4px;
    color: var(--text);
    font-size: 13px;
    outline: 0;
    flex: 1;
  }
  .rename-action-btn {
    border: 0;
    padding: 4px;
    border-radius: 4px;
    cursor: pointer;
    display: grid;
    place-items: center;
  }
  .rename-action-btn.check {
    background: var(--green-wash);
    color: var(--green);
  }
  .rename-action-btn.cancel {
    background: var(--surface-3);
    color: var(--muted);
  }
  .session-tags {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .model-tag {
    font-family: var(--mono);
    font-size: 10px;
    color: var(--faint);
  }
  .time-tag {
    font-size: 11px;
    color: var(--faint);
    font-family: var(--mono);
  }
  .session-preview {
    margin: 0;
    font-size: 12px;
    color: var(--muted);
    line-height: 1.5;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .session-meta {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11px;
    color: var(--faint);
  }
  .archived-tag {
    color: var(--amber);
  }

  .session-actions {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 0 10px;
    border-left: 1px solid var(--line);
    background: var(--surface-2);
  }
  .action-icon-btn {
    width: 28px;
    height: 28px;
    border-radius: 6px;
    border: 0;
    background: transparent;
    color: var(--muted);
    display: grid;
    place-items: center;
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .action-icon-btn:hover {
    background: var(--surface-3);
    color: var(--text);
  }
  .action-icon-btn.danger:hover {
    background: rgba(224, 91, 80, 0.15);
    color: var(--danger);
  }

  .backend-ledger-wrap {
    flex: 1;
    overflow-y: auto;
    padding: 16px 32px 32px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .backend-ledger-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 16px;
    border-radius: 8px;
    background: var(--surface-2);
    border: 1px solid var(--line);
  }
  .banner-text {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--muted);
    font-size: 12px;
  }
  :global(.db-icon) {
    color: var(--amber);
  }
  .backend-table-wrap {
    border: 1px solid var(--line);
    border-radius: 8px;
    overflow: hidden;
    background: var(--surface-2);
  }
  .backend-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
    text-align: left;
  }
  .backend-table th {
    background: var(--surface-3);
    padding: 10px 14px;
    color: var(--faint);
    font-weight: 600;
    border-bottom: 1px solid var(--line);
  }
  .backend-table td {
    padding: 10px 14px;
    border-bottom: 1px solid var(--line);
    color: var(--muted);
  }
  .backend-table tr:last-child td {
    border-bottom: 0;
  }
  .backend-table code {
    font-family: var(--mono);
    color: var(--text);
  }
  .count-badge {
    font-family: var(--mono);
    color: var(--amber);
  }
  :global(.spin) {
    animation: spin 1s linear infinite;
  }
</style>
