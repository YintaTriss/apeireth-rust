<script lang="ts">
  import {onMount} from 'svelte';
  import {
    Layers3,
    Search,
    RotateCcw,
    Plus,
    Tag,
    Clock,
    Database,
    Sparkles,
    FileText,
    Brain,
    Share2,
    Lock,
    ExternalLink,
    X,
    ChevronRight,
    ChevronDown,
    Info,
  } from 'lucide-svelte';
  import PageHeader from '../components/PageHeader.svelte';
  import EmptyState from './components/EmptyState.svelte';
  import ErrorState from './components/ErrorState.svelte';
  import LoadingState from './components/LoadingState.svelte';
  import StatusBadge from './components/StatusBadge.svelte';
  import type {ApeirethConfig, CapabilityManifest, MemoryCategory, MemoryEpisodeItem} from './types';
  import {
    appendMemoryEpisode,
    fetchGraphData,
    fetchMemoryEpisodes,
    fetchMemoryStreams,
    forgetMemoryEpisode,
    protectMemoryEpisode,
    unprotectMemoryEpisode,
    capabilitySupported,
  } from './runtime';
  import ConfirmDialog from './components/ConfirmDialog.svelte';

  let {
    config,
    capabilities = null,
  }: {
    config: ApeirethConfig;
    capabilities: CapabilityManifest | null;
  } = $props();

  // Capability gating — 后端不支持则按钮 disabled/隐藏, 不 404-probe.
  let canForget = $derived(capabilitySupported(capabilities, 'memory.forget'));
  let canProtect = $derived(capabilitySupported(capabilities, 'memory.protect'));
  let canUnprotect = $derived(capabilitySupported(capabilities, 'memory.unprotect'));

  // Memory mutation state (forget/protect). revision 从 0 起 (客户端无本地缓存治理态).
  let forgetTarget = $state<MemoryEpisodeItem | null>(null);
  let mutationError = $state('');
  let mutating = $state(false);

  async function handleForget(ep: MemoryEpisodeItem): Promise<void> {
    forgetTarget = null;
    mutating = true;
    mutationError = '';
    const r = await forgetMemoryEpisode(config, ep.id, 0, '用户手动遗忘');
    mutating = false;
    if ('error' in r) {
      mutationError = r.error;
      return;
    }
    // 本地从列表移除 (forgotten 不再默认检索).
    episodes = episodes.filter((item) => item.id !== ep.id);
  }

  async function handleToggleProtect(ep: MemoryEpisodeItem, currentlyProtected: boolean): Promise<void> {
    mutating = true;
    mutationError = '';
    const r = currentlyProtected
      ? await unprotectMemoryEpisode(config, ep.id, 0)
      : await protectMemoryEpisode(config, ep.id, 0);
    mutating = false;
    if ('error' in r) {
      mutationError = r.error;
    }
    // 重新拉取以反映新状态.
    await loadData();
  }

  type MemoryTab = 'all' | 'working' | 'recent' | 'long_term' | 'profile' | 'graph';

  let activeTab = $state<MemoryTab>('all');
  let searchQuery = $state('');
  let episodes = $state<MemoryEpisodeItem[]>([]);
  let graphFacts = $state<MemoryEpisodeItem[]>([]);
  let graphLinks = $state<MemoryEpisodeItem[]>([]);
  let loading = $state(false);
  let error = $state('');

  // Write new memory
  let showAppender = $state(false);
  let newContent = $state('');
  let newCategory = $state<string>('fact');
  let newSession = $state('me');
  let appending = $state(false);
  let appendSuccess = $state(false);

  // Detail modal
  let selectedEpisode = $state<MemoryEpisodeItem | null>(null);

  const tabLabels = {
    all: '全部记忆',
    working: '工作记忆',
    recent: '近期记忆',
    long_term: '长期记忆',
    profile: '用户画像',
    graph: '知识图谱',
  };

  async function loadData() {
    loading = true;
    error = '';
    try {
      if (activeTab === 'graph') {
        const graph = await fetchGraphData(config);
        graphFacts = graph.facts;
        graphLinks = graph.links;
      } else {
        episodes = await fetchMemoryEpisodes(config, searchQuery, 120);
      }
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  async function handleAppend() {
    const text = newContent.trim();
    if (!text || appending) return;
    appending = true;
    appendSuccess = false;

    const ok = await appendMemoryEpisode(
      config,
      text,
      newCategory,
      newSession || 'me',
    );

    appending = false;
    if (ok) {
      appendSuccess = true;
      newContent = '';
      setTimeout(() => {
        appendSuccess = false;
        showAppender = false;
      }, 1000);
      void loadData();
    } else {
      error = '写入记忆失败，请确认后端连接正常';
    }
  }

  const displayList = $derived.by(() => {
    let list = [...episodes];
    if (searchQuery.trim()) {
      const q = searchQuery.toLowerCase();
      list = list.filter((e) =>
        e.content.toLowerCase().includes(q) ||
        (e.sessionId && e.sessionId.toLowerCase().includes(q)),
      );
    }
    return list;
  });

  function formatTime(ts: number): string {
    const d = new Date(ts > 1e11 ? ts : ts * 1000);
    return d.toLocaleString('zh-CN', {month: 'numeric', day: 'numeric', hour: '2-digit', minute: '2-digit'});
  }

  onMount(() => {
    void loadData();
  });
</script>

<section class="memory-view">
  <PageHeader
    eyebrow="认知"
    title="记忆与知识库"
    subtitle="查看与检索持久化情节记忆 (episodes)、历史流与结构化知识图谱。"
  >
    <button
      class="quiet-button"
      onclick={() => showAppender = !showAppender}
    >
      <Plus size={14} />
      <span>写入记忆</span>
    </button>
    <button class="quiet-button" onclick={loadData} disabled={loading}>
      <RotateCcw size={13} class={loading ? 'spin' : ''} />
      <span>刷新</span>
    </button>
  </PageHeader>

  <!-- Append Memory Drawer / Form -->
  {#if showAppender}
    <div class="appender-card">
      <div class="appender-head">
        <strong>写入新记忆条目</strong>
        <button class="close-appender-btn" onclick={() => showAppender = false} aria-label="关闭">
          <X size={14} />
        </button>
      </div>
      <textarea
        bind:value={newContent}
        rows="2"
        placeholder="输入需持久记住的信息（如主人偏好、关键事实、项目约定）…"
        disabled={appending}
      ></textarea>
      <div class="appender-foot">
        <div class="field-group">
          <label for="memory-cat-select">类别</label>
          <select id="memory-cat-select" bind:value={newCategory}>
            <option value="fact">事实 (Fact)</option>
            <option value="preference">偏好 (Preference)</option>
            <option value="event">事件 (Event)</option>
            <option value="feedback">反馈 (Feedback)</option>
            <option value="reference">参考 (Reference)</option>
          </select>
        </div>
        <div class="field-group">
          <label for="memory-session-input">会话标签</label>
          <input
            id="memory-session-input"
            type="text"
            bind:value={newSession}
            placeholder="默认 me"
          />
        </div>
        <div class="appender-actions">
          {#if appendSuccess}
            <span class="ok-hint">已写入后端数据库</span>
          {/if}
          <button
            class="primary-button"
            onclick={handleAppend}
            disabled={appending || !newContent.trim()}
          >
            {appending ? '正在写入…' : '写入记忆库'}
          </button>
        </div>
      </div>
    </div>
  {/if}

  <!-- Tabs & Toolbar -->
  <div class="memory-toolbar">
    <div class="search-input-wrap">
      <Search size={14} class="search-icon" />
      <input
        type="text"
        placeholder="搜索记忆文本或会话标签…"
        bind:value={searchQuery}
        onkeydown={(e) => e.key === 'Enter' && void loadData()}
      />
      {#if searchQuery}
        <button class="clear-search-btn" onclick={() => { searchQuery = ''; void loadData(); }} aria-label="清除搜索">
          <X size={12} />
        </button>
      {/if}
    </div>

    <div class="memory-tabs-row">
      {#each Object.entries(tabLabels) as [key, label]}
        <button
          class="mem-tab-btn"
          class:active={activeTab === key}
          onclick={() => { activeTab = key as MemoryTab; void loadData(); }}
        >
          {label}
        </button>
      {/each}
    </div>
  </div>

  <!-- Content Stream -->
  <div class="memory-container">
    {#if loading && !episodes.length && !graphFacts.length}
      <LoadingState message="正在拉取记忆流与知识图谱…" />
    {:else if error && !episodes.length && !graphFacts.length}
      <ErrorState title="拉取记忆失败" message={error} onRetry={loadData} />
    {:else if activeTab === 'graph'}
      <!-- Knowledge Graph View -->
      <div class="graph-section">
        <div class="graph-box">
          <h3 class="section-title">图谱事实 (Facts: {graphFacts.length})</h3>
          {#if !graphFacts.length}
            <p class="empty-hint">暂无知识图谱事实条目</p>
          {:else}
            <div class="graph-list">
              {#each graphFacts as fact}
                <div class="graph-item">
                  <span class="fact-tag">Fact</span>
                  <p class="fact-text">{fact.content}</p>
                  {#if fact.timestamp > 0}
                    <small class="time-text">{formatTime(fact.timestamp)}</small>
                  {:else if fact.importance != null}
                    <small class="time-text">重要度 {fact.importance}</small>
                  {/if}
                </div>
              {/each}
            </div>
          {/if}
        </div>

        <div class="graph-box">
          <h3 class="section-title">图谱关联 (Links: {graphLinks.length})</h3>
          {#if !graphLinks.length}
            <p class="empty-hint">暂无知识图谱关系链接</p>
          {:else}
            <div class="graph-list">
              {#each graphLinks as link}
                <div class="graph-item">
                  <span class="link-tag">Link</span>
                  <p class="fact-text">{link.content}</p>
                  {#if link.timestamp > 0}
                    <small class="time-text">{formatTime(link.timestamp)}</small>
                  {/if}
                </div>
              {/each}
            </div>
          {/if}
        </div>
      </div>
    {:else if !displayList.length}
      <EmptyState
        icon="🧠"
        title={searchQuery ? '没有找到匹配的记忆' : '记忆库为空'}
        description="与伙伴对话或点击右上角“写入记忆”即可开始积累认知。"
      />
    {:else}
      <!-- Episodes Grid / List -->
      <div class="episodes-grid">
        {#each displayList as ep (ep.id)}
          <div
            class="episode-card"
            role="button"
            tabindex="0"
            onclick={() => selectedEpisode = ep}
            onkeydown={(e) => e.key === 'Enter' && (selectedEpisode = ep)}
          >

            <div class="ep-head">
              <div class="ep-meta">
                <StatusBadge
                  label={ep.role === 'user' ? '用户输入' : '伙伴感知'}
                  variant={ep.role === 'user' ? 'blue' : 'amber'}
                  size="small"
                />
                <span class="session-badge">{ep.sessionId || 'me'}</span>
              </div>
              <time class="time-text">{formatTime(ep.timestamp)}</time>
            </div>

            <p class="ep-content">{ep.content}</p>

            <div class="ep-foot">
              <span class="ep-id">#{ep.id.slice(0, 8)}</span>
              <div class="foot-actions">
                {#if canForget || canProtect}
                  {#if canProtect}
                    <button
                      class="foot-btn protect-btn"
                      title={ep.protected ? '解除保护' : '保护 (防自动遗忘)'}
                      disabled={mutating}
                      onclick={(e) => {
                        e.stopPropagation();
                        void handleToggleProtect(ep, !!ep.protected);
                      }}
                    >
                      {#if ep.protected}
                        <Lock size={11} />
                        <span>取消保护</span>
                      {:else}
                        <Lock size={11} />
                        <span>保护</span>
                      {/if}
                    </button>
                  {/if}
                  {#if canForget}
                    <button
                      class="foot-btn forget-btn"
                      title="遗忘此条记忆 (软删, 可审计)"
                      disabled={mutating || !!ep.protected}
                      onclick={(e) => {
                        e.stopPropagation();
                        if (ep.protected) return;
                        forgetTarget = ep;
                      }}
                    >
                      <Lock size={11} />
                      <span>遗忘</span>
                    </button>
                  {/if}
                {:else}
                  <button
                    class="disabled-action-btn"
                    title="后端能力未开放：当前仅支持读取与追加 (Backend capability unavailable)"
                    onclick={(e) => e.stopPropagation()}
                  >
                    <Lock size={11} />
                    <span>编辑/删除 (只读)</span>
                  </button>
                {/if}
              </div>
            </div>
          </div>
        {/each}

      </div>
    {/if}
  </div>
</section>

<!-- Phase 3: Forget 确认弹窗 (forget 不可逆软删, 必须 confirm). -->
<ConfirmDialog
  open={forgetTarget !== null}
  title="遗忘此条记忆？"
  message={forgetTarget ? `将软删除记忆 #${forgetTarget.id.slice(0, 8)}（从检索中隐藏，保留审计记录）。此操作可审计但不易撤销，确定继续？` : ''}
  confirmText="确认遗忘"
  danger={true}
  onConfirm={() => {
    if (forgetTarget) void handleForget(forgetTarget);
  }}
  onCancel={() => (forgetTarget = null)}
/>

{#if mutationError}
  <div class="mutation-error" role="alert">记忆操作失败：{mutationError}</div>
{/if}

<!-- Detail Modal -->
{#if selectedEpisode}
  <div class="modal-backdrop" onclick={() => selectedEpisode = null} role="presentation">
    <div
      class="modal-dialog"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      role="dialog"
      tabindex="-1"
      aria-modal="true"
      aria-labelledby="ep-dialog-title"
    >
      <div class="modal-header">
        <div class="modal-title-wrap">
          <Brain size={16} class="dialog-brain-icon" />
          <h3 id="ep-dialog-title">记忆条目详情: #{selectedEpisode.id}</h3>
        </div>
        <button class="modal-close-btn" onclick={() => selectedEpisode = null} aria-label="关闭">
          <X size={16} />
        </button>
      </div>

      <div class="modal-body">
        <div class="detail-section">
          <span class="detail-label">记忆文本内容</span>
          <p class="detail-content-text">{selectedEpisode.content}</p>
        </div>

        <div class="detail-grid">
          <div>
            <span class="detail-label">所属会话</span>
            <code>{selectedEpisode.sessionId || 'me'}</code>
          </div>
          <div>
            <span class="detail-label">记录角色</span>
            <span>{selectedEpisode.role}</span>
          </div>
          <div>
            <span class="detail-label">记录时间</span>
            <span>{new Date(selectedEpisode.timestamp > 1e11 ? selectedEpisode.timestamp : selectedEpisode.timestamp * 1000).toLocaleString('zh-CN')}</span>
          </div>
        </div>

        <div class="detail-section">
          <span class="detail-label">原始数据 (JSON)</span>
          <pre class="raw-pre">{JSON.stringify(selectedEpisode, null, 2)}</pre>
        </div>
      </div>

      <div class="modal-footer">
        <button class="primary-btn" onclick={() => selectedEpisode = null}>完成</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .memory-view {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
  }
  .appender-card {
    margin: 10px 32px;
    padding: 14px 18px;
    background: var(--surface-2);
    border: 1px solid var(--line-strong);
    border-radius: 9px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .appender-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 13px;
    color: var(--text);
  }
  .close-appender-btn {
    border: 0;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
    padding: 2px;
  }
  .appender-card textarea {
    width: 100%;
    resize: none;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--surface);
    color: var(--text);
    padding: 8px 10px;
    font-size: 13px;
    outline: 0;
  }
  .appender-card textarea:focus {
    border-color: var(--amber-line);
  }
  .appender-foot {
    display: flex;
    align-items: center;
    gap: 14px;
    flex-wrap: wrap;
  }
  .field-group {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .field-group label {
    font-size: 11px;
    color: var(--muted);
  }
  .field-group select, .field-group input {
    padding: 5px 8px;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 5px;
    color: var(--text);
    font-size: 12px;
    outline: 0;
  }
  .appender-actions {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .ok-hint {
    color: var(--green);
    font-size: 12px;
  }

  .memory-toolbar {
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

  .memory-tabs-row {
    display: flex;
    gap: 4px;
  }
  .mem-tab-btn {
    border: 1px solid var(--line);
    background: var(--surface-2);
    color: var(--muted);
    font-size: 11px;
    padding: 5px 10px;
    border-radius: 6px;
    cursor: pointer;
  }
  .mem-tab-btn:hover {
    color: var(--text);
    border-color: var(--line-strong);
  }
  .mem-tab-btn.active {
    background: var(--amber-wash);
    border-color: var(--amber-line);
    color: var(--amber);
  }

  .memory-container {
    flex: 1;
    overflow-y: auto;
    padding: 20px 32px 40px;
  }
  .episodes-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
    gap: 12px;
  }
  .episode-card {
    background: var(--surface-2);
    border: 1px solid var(--line);
    border-radius: 9px;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .episode-card:hover {
    border-color: var(--line-strong);
    background: var(--surface-3);
    transform: translateY(-1px);
  }
  .ep-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
  }
  .ep-meta {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .session-badge {
    font-size: 10px;
    font-family: var(--mono);
    color: var(--faint);
    background: var(--surface);
    padding: 1px 6px;
    border-radius: 4px;
    border: 1px solid var(--line);
  }
  .time-text {
    font-size: 11px;
    font-family: var(--mono);
    color: var(--faint);
  }
  .ep-content {
    margin: 0;
    font-size: 13px;
    color: var(--text);
    line-height: 1.6;
    display: -webkit-box;
    -webkit-line-clamp: 4;
    line-clamp: 4;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .ep-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-top: auto;
    padding-top: 8px;
    border-top: 1px solid rgba(255, 255, 255, 0.04);
  }
  .ep-id {
    font-family: var(--mono);
    font-size: 10px;
    color: var(--faint);
  }
  .disabled-action-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    border: 0;
    background: transparent;
    color: var(--faint);
    font-size: 10px;
    cursor: not-allowed;
    opacity: 0.7;
  }

  .graph-section {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 16px;
  }
  .graph-box {
    background: var(--surface-2);
    border: 1px solid var(--line);
    border-radius: 9px;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .section-title {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: var(--text);
  }
  .empty-hint {
    margin: 0;
    font-size: 12px;
    color: var(--faint);
  }
  .graph-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .graph-item {
    padding: 8px 10px;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 6px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .fact-tag {
    align-self: flex-start;
    font-size: 9px;
    padding: 1px 5px;
    background: var(--green-wash);
    color: var(--green);
    border-radius: 3px;
  }
  .link-tag {
    align-self: flex-start;
    font-size: 9px;
    padding: 1px 5px;
    background: var(--blue-wash);
    color: var(--blue);
    border-radius: 3px;
  }
  .fact-text {
    margin: 0;
    font-size: 12px;
    color: var(--text);
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
    width: 100%;
    max-width: 520px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: 12px;
    box-shadow: var(--shadow);
    overflow: hidden;
    display: flex;
    flex-direction: column;
    max-height: 85vh;
  }
  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 20px;
    border-bottom: 1px solid var(--line);
    background: var(--surface-2);
  }
  .modal-title-wrap {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .modal-title-wrap h3 {
    margin: 0;
    font-size: 14px;
    color: var(--text);
  }
  :global(.dialog-brain-icon) { color: var(--amber); }
  .modal-close-btn {
    border: 0;
    background: transparent;
    color: var(--muted);
    padding: 4px;
    border-radius: 6px;
    cursor: pointer;
    display: grid;
    place-items: center;
  }
  .modal-body {
    padding: 18px 20px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .detail-section {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .detail-label {
    font-size: 11px;
    font-weight: 600;
    color: var(--faint);
    text-transform: uppercase;
  }
  .detail-content-text {
    margin: 0;
    font-size: 13px;
    color: var(--text);
    line-height: 1.6;
  }
  .detail-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 10px;
    padding: 10px 12px;
    background: var(--surface-2);
    border-radius: 6px;
    border: 1px solid var(--line);
  }
  .detail-grid span, .detail-grid code {
    display: block;
    font-size: 11px;
    color: var(--text);
    margin-top: 2px;
  }
  .raw-pre {
    margin: 0;
    padding: 10px;
    background: var(--surface-2);
    border: 1px solid var(--line);
    border-radius: 6px;
    color: var(--muted);
    font-family: var(--mono);
    font-size: 11px;
    white-space: pre-wrap;
    word-break: break-all;
    max-height: 200px;
    overflow-y: auto;
  }
  .modal-footer {
    display: flex;
    justify-content: flex-end;
    padding: 12px 20px;
    border-top: 1px solid var(--line);
    background: var(--surface-2);
  }
  .primary-btn {
    padding: 6px 14px;
    border-radius: 6px;
    background: var(--amber);
    border: 1px solid var(--amber);
    color: #1a1408;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }
  :global(.spin) {
    animation: spin 1s linear infinite;
  }
  @media (max-width: 600px) {
    .graph-section {
      grid-template-columns: 1fr;
    }
  }

  .foot-btn {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    padding: 3px 8px;
    font-size: 11px;
    border-radius: 5px;
    cursor: pointer;
    border: 1px solid var(--border, rgba(255,255,255,0.12));
    background: transparent;
    color: var(--text-dim, #aaa);
  }
  .foot-btn:hover:not(:disabled) {
    background: rgba(255,255,255,0.05);
  }
  .foot-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .forget-btn {
    color: #ef4444;
    border-color: rgba(239, 68, 68, 0.3);
  }
  .protect-btn {
    color: var(--accent, #f5a623);
    border-color: rgba(245, 166, 35, 0.3);
  }
  .mutation-error {
    position: fixed;
    bottom: 20px;
    right: 20px;
    padding: 10px 14px;
    background: rgba(239, 68, 68, 0.15);
    border: 1px solid rgba(239, 68, 68, 0.4);
    color: #fca5a5;
    border-radius: 8px;
    font-size: 12px;
    z-index: 1100;
    max-width: 320px;
  }
</style>
