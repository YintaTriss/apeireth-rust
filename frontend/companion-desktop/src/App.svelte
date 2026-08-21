<script lang="ts">
  import {onMount, tick} from 'svelte';
  import {
    MessageCircleMore,
    Settings,
    MessagesSquare,
    Layers3,
    Activity,
    Wrench,
    Plus,
    ArrowUp,
    Square,
    Loader2,
    Plug,
    ChevronDown,
    Sparkles,
    AlertCircle,
    Info,
    RotateCcw,
  } from 'lucide-svelte';
  import MessageContent from './lib/MessageContent.svelte';
  import StatusDot from './components/StatusDot.svelte';
  import RuntimeModal from './lib/components/RuntimeModal.svelte';
  import EmptyState from './lib/components/EmptyState.svelte';
  import ConversationsView from './lib/ConversationsView.svelte';
  import ActivityView from './lib/views/ActivityView.svelte';
  import ToolsView from './lib/views/ToolsView.svelte';
  import MemoryView from './lib/MemoryView.svelte';
  import SettingsView from './lib/views/SettingsView.svelte';

  import type {
    ApeirethConfig,
    ChatMessage,
    Conversation,
    HealthState,
    RuntimeHealthReport,
    ToolCallDetails,
    ViewId,
  } from './lib/types';
  import {
    checkHealthDetailed,
    createAgentRuntime,
    fetchApprovalRequests,
    loadConfig,
    loadConversations,
    saveConfig,
    saveConversations,
    fetchCapabilities,
    capabilitySupported,
    subscribeCompanionEvents,
    type CompanionPresentationState,
  } from './lib/runtime';
  import type {CapabilityManifest} from './lib/types';

  // 6 大一级导航
  const nav = [
    {id: 'chat', label: '对话', icon: MessageCircleMore},
    {id: 'conversations', label: '会话', icon: MessagesSquare},
    {id: 'activity', label: '活动', icon: Activity},
    {id: 'tools', label: '工具', icon: Wrench},
    {id: 'memory', label: '记忆', icon: Layers3},
    {id: 'settings', label: '设置', icon: Settings},
  ] as const;

  let activeView = $state<ViewId>('chat');
  let config = $state<ApeirethConfig>(loadConfig());
  let conversations = $state<Conversation[]>(loadConversations());
  let activeId = $state<string | null>(null);
  let draft = $state('');
  let busy = $state(false);
  let error = $state('');
  let pendingApprovals = $state<import('./lib/types').ApprovalRequestItem[]>([]);
  let isReasoning = $state(false);
  let isExecutingTool = $state(false);
  let proactiveGreeting = $state('');
  let agentRuntime = $state(createAgentRuntime(loadConfig()));

  // 深度运行时报告与健康状态
  let healthState = $state<HealthState>('connecting');
  let healthReport = $state<RuntimeHealthReport>({
    overall: 'connecting',
    baseUrl: loadConfig().baseUrl,
    subsystems: [],
    model: loadConfig().model,
  });
  let showRuntimeModal = $state(false);
  let isRefreshingHealth = $state(false);

  // Runtime Capability Manifest — gate UI 按钮的依据 (不再 404-probing).
  let capabilities = $state<CapabilityManifest | null>(null);

  // 智能滚动状态管理
  let messagesContainer = $state<HTMLElement | null>(null);
  let isNearBottom = $state(true);
  let showScrollBottomBtn = $state(false);

  // 后端信号驱动的伴随体表现态 (严禁前端造假). Reconciled from master.
  const companionPresentation = $derived.by<CompanionPresentationState>(() => {
    if (pendingApprovals.length > 0) return 'concerned';
    if (isExecutingTool) return 'working';
    if (isReasoning) return 'thinking';
    if (busy) return 'speaking';
    return 'idle';
  });

  const activeConversation = $derived(
    conversations.find((item) => item.id === activeId) || null,
  );

  const activeMessages = $derived(activeConversation?.messages || []);

  const healthLabel: Record<HealthState, string> = {
    connecting: '连接中…',
    online: '后端已连接',
    ready: '后端已连接',
    degraded: '降级运行',
    generating: '正在生成…',
    error: '运行异常',
    offline: '后端离线',
  };

  const quickPrompts = [
    '聊聊今天',
    '查看我的记忆',
    '帮我处理一件事',
    '检查系统状态',
  ];

  function ensureConversation(): Conversation {
    if (activeConversation) return activeConversation;
    const now = Date.now();
    const conversation: Conversation = {
      id: crypto.randomUUID(),
      title: '新对话',
      createdAt: now,
      updatedAt: now,
      messages: [],
      scope: 'global',
      model: config.model,
    };
    conversations = [conversation, ...conversations];
    activeId = conversation.id;
    persist();
    return conversation;
  }

  function persist(): void {
    saveConversations(conversations);
  }

  function updateConversation(id: string, patch: Partial<Conversation>): void {
    conversations = conversations.map((item) =>
      item.id === id ? {...item, ...patch, updatedAt: Date.now()} : item,
    );
    persist();
  }

  function updateMessage(id: string, messageId: string, patch: Partial<ChatMessage>): void {
    conversations = conversations.map((item) => {
      if (item.id !== id) return item;
      return {
        ...item,
        updatedAt: Date.now(),
        messages: item.messages.map((m) => (m.id === messageId ? {...m, ...patch} : m)),
      };
    });
    persist();
  }

  function pushMessage(conversationId: string, message: ChatMessage): void {
    conversations = conversations.map((item) => {
      if (item.id !== conversationId) return item;
      return {...item, updatedAt: Date.now(), messages: [...item.messages, message]};
    });
    persist();
  }

  /** 按 id 原子拼接流式文本 delta. */
  function appendDelta(conversationId: string, messageId: string, delta: string): void {
    conversations = conversations.map((item) => {
      if (item.id !== conversationId) return item;
      return {
        ...item,
        updatedAt: Date.now(),
        messages: item.messages.map((m) =>
          m.id === messageId ? {...m, text: m.text + delta} : m,
        ),
      };
    });
    persist();
  }

  /** 按 id 原子拼接推理思考 delta. Reconciled from master. */
  function appendReasoningDelta(conversationId: string, messageId: string, delta: string): void {
    conversations = conversations.map((item) => {
      if (item.id !== conversationId) return item;
      return {
        ...item,
        updatedAt: Date.now(),
        messages: item.messages.map((m) => (m.id === messageId ? {...m, reasoning: (m.reasoning || '') + delta} : m)),
      };
    });
    persist();
  }

  function updateMessageToolCall(
    conversationId: string,
    messageId: string,
    toolCall: ToolCallDetails,
  ): void {
    conversations = conversations.map((item) => {
      if (item.id !== conversationId) return item;
      return {
        ...item,
        updatedAt: Date.now(),
        messages: item.messages.map((m) => {
          if (m.id !== messageId) return m;
          const list = m.toolCalls ? [...m.toolCalls] : [];
          const idx = list.findIndex((t) => t.id === toolCall.id);
          if (idx >= 0) {
            list[idx] = toolCall;
          } else {
            list.push(toolCall);
          }
          return {...m, toolCalls: list};
        }),
      };
    });
    persist();
  }

  // 滚动位置监听与控制
  function handleScroll() {
    if (!messagesContainer) return;
    const {scrollTop, scrollHeight, clientHeight} = messagesContainer;
    const distanceToBottom = scrollHeight - scrollTop - clientHeight;
    isNearBottom = distanceToBottom < 80;
    showScrollBottomBtn = distanceToBottom > 150;
  }

  function scrollToBottom(smooth = false) {
    if (!messagesContainer) return;
    if (smooth) {
      messagesContainer.scrollTo({
        top: messagesContainer.scrollHeight,
        behavior: 'smooth',
      });
    } else {
      messagesContainer.scrollTop = messagesContainer.scrollHeight;
    }
    isNearBottom = true;
    showScrollBottomBtn = false;
  }

  async function triggerAutoScroll() {
    if (isNearBottom) {
      await tick();
      scrollToBottom(false);
    }
  }

  async function refreshConnection(): Promise<void> {
    isRefreshingHealth = true;
    try {
      const report = await checkHealthDetailed(config.baseUrl, config.apiKey);
      healthReport = report;
      if (!busy) {
        healthState = report.overall;
      }
      // health 之后拉取 capability manifest (runtime version 变化/重连时刷新).
      // 不每次 render 重复 fetch — 仅在 refreshConnection (节拍/手动) 时.
      if (report.overall !== 'offline') {
        const prevVersion = capabilities?.runtime.version;
        const fresh = await fetchCapabilities(config);
        // 仅在 version 变化或首次加载时更新 (避免节拍无谓刷新覆盖).
        if (!capabilities || fresh.runtime.version !== prevVersion || fresh.legacy !== capabilities.legacy) {
          capabilities = fresh;
        }
        // 同步待审批请求 (后端权限洋葱).
        pendingApprovals = await fetchApprovalRequests(config).catch(() => []);
      } else {
        pendingApprovals = [];
      }
    } finally {
      isRefreshingHealth = false;
    }
  }

  async function send(customText?: string): Promise<void> {
    const text = (customText ?? draft).trim();
    if (!text || busy) return;
    const conversation = ensureConversation();
    const conversationId = conversation.id;
    const history = conversation.messages
      .filter((m) => m.role === 'user' || m.role === 'assistant')
      .map((m) => ({role: m.role, content: m.text}));

    if (!customText) draft = '';
    busy = true;
    isReasoning = false;
    isExecutingTool = false;
    healthState = 'generating';
    error = '';

    const userMessage: ChatMessage = {
      id: crypto.randomUUID(),
      role: 'user',
      text,
      time: new Date().toLocaleTimeString('zh-CN', {hour: '2-digit', minute: '2-digit'}),
      timestamp: Date.now(),
    };
    const assistantMessage: ChatMessage = {
      id: crypto.randomUUID(),
      role: 'assistant',
      text: '',
      time: new Date().toLocaleTimeString('zh-CN', {hour: '2-digit', minute: '2-digit'}),
      timestamp: Date.now(),
      streaming: true,
      toolCalls: [],
      reasoning: '',
      modelInfo: {id: config.model, provider: 'apeireth'},
    };

    pushMessage(conversationId, userMessage);
    pushMessage(conversationId, assistantMessage);

    if (conversation.messages.length <= 2) {
      updateConversation(conversationId, {title: text.slice(0, 24)});
    }

    await tick();
    scrollToBottom(true);

    try {
      const full = await agentRuntime.run(
        {
          messages: [...history, {role: 'user', content: text}],
          model: {id: config.model, provider: 'apeireth'},
          sessionId: conversationId,
          context: {user: '主人'},
        },
        (event) => {
          if (event.type === 'text-delta') {
            isReasoning = false;
            appendDelta(conversationId, assistantMessage.id, event.text);
            void triggerAutoScroll();
          } else if (event.type === 'reasoning-delta') {
            isReasoning = true;
            appendReasoningDelta(conversationId, assistantMessage.id, event.text);
          } else if (event.type === 'tool-call') {
            isExecutingTool = true;
            updateMessageToolCall(conversationId, assistantMessage.id, event.toolCall);
            void triggerAutoScroll();
          } else if (event.type === 'tool-result') {
            isExecutingTool = false;
            void triggerAutoScroll();
          }
        },
      );
      updateMessage(conversationId, assistantMessage.id, {
        text: full || '(空响应)',
        streaming: false,
      });
    } catch (caught) {
      const isAborted = caught instanceof Error && caught.name === 'AbortError';
      const msg = caught instanceof Error ? caught.message : String(caught);
      if (isAborted) {
        updateMessage(conversationId, assistantMessage.id, {streaming: false, aborted: true});
      } else {
        error = msg;
        updateMessage(conversationId, assistantMessage.id, {
          text: '',
          streaming: false,
          error: msg,
        });
        healthState = 'error';
      }
    } finally {
      busy = false;
      isReasoning = false;
      isExecutingTool = false;
      // 生成结束: 恢复真实 health (backend 可能已离线)
      await refreshConnection();
      await tick();
      void triggerAutoScroll();
    }
  }

  function stop(): void {
    agentRuntime.abort();
  }

  /** 重试一条失败/中止的 assistant 消息: 找到其上一条 user 文本重新发送. Reconciled from master. */
  function retry(messageId: string): void {
    if (busy || !activeConversation) return;
    const msgs = activeConversation.messages;
    const idx = msgs.findIndex((m) => m.id === messageId);
    if (idx < 0) return;
    let userText = '';
    for (let i = idx - 1; i >= 0; i--) {
      if (msgs[i].role === 'user') {
        userText = msgs[i].text;
        break;
      }
    }
    const filtered = msgs.filter((m) => m.id !== messageId);
    updateConversation(activeConversation.id, {messages: filtered});
    if (userText) {
      void send(userText);
    }
  }

  function newConversation(): void {
    const now = Date.now();
    const conversation: Conversation = {
      id: crypto.randomUUID(),
      title: '新对话',
      createdAt: now,
      updatedAt: now,
      messages: [],
      scope: 'global',
      model: config.model,
    };
    conversations = [conversation, ...conversations];
    activeId = conversation.id;
    activeView = 'chat';
    persist();
  }

  function openConversation(id: string): void {
    activeId = id;
    activeView = 'chat';
  }

  function archiveConversation(id: string): void {
    const conv = conversations.find((item) => item.id === id);
    if (conv) updateConversation(id, {archived: !conv.archived});
  }

  function deleteConversation(id: string): void {
    conversations = conversations.filter((item) => item.id !== id);
    if (activeId === id) activeId = null;
    persist();
  }

  function applyQuickPrompt(promptText: string) {
    draft = promptText;
  }

  onMount(() => {
    if (!activeId && conversations.length) activeId = conversations[0].id;
    void refreshConnection();

    // 订阅 SSE 伴随体事件通道 (主动涌现与反思通知). Reconciled from master.
    const unsubscribeEvents = subscribeCompanionEvents(config, (event) => {
      proactiveGreeting = event.text;
      window.setTimeout(() => {
        if (proactiveGreeting === event.text) proactiveGreeting = '';
      }, 12000);
    });

    // 后台健康轮询与审批请求同步 (真实 HTTP /health + capability manifest).
    const timer = window.setInterval(() => {
      void refreshConnection();
    }, 15000);

    return () => {
      window.clearInterval(timer);
      unsubscribeEvents();
    };
  });
</script>

<div class="shell">
  <!-- Sidebar -->
  <aside class="sidebar">
    <div class="sidebar-brand">
      <span class="logo-mark">A</span>
      <span class="brand-name">Apeireth 伙伴</span>
      <button class="status-indicator-btn" onclick={() => showRuntimeModal = true} title="查看运行时详情">
        <StatusDot
          size="small"
          off={healthState === 'offline'}
          active={healthState === 'generating'}
        />
      </button>
    </div>

    <nav class="nav" aria-label="主要导航">
      {#each nav as item}
        <button
          class:active={activeView === item.id}
          onclick={() => activeView = item.id}
          aria-label={item.label}
        >
          <item.icon size={16} />
          <span>{item.label}</span>
        </button>
      {/each}
    </nav>

    {#if proactiveGreeting}
      <div class="proactive-hint" role="status">
        <Sparkles size={12} />
        <span>{proactiveGreeting}</span>
      </div>
    {/if}

    <div class="sidebar-footer">
      <button class="quiet-button wide new-chat-btn" onclick={newConversation}>
        <Plus size={14} />
        <span>新对话</span>
      </button>

      <button
        class="conn-hint"
        class:offline={healthState === 'offline'}
        class:degraded={healthState === 'degraded'}
        class:error={healthState === 'error'}
        onclick={() => showRuntimeModal = true}
        title="点击查看各子系统诊断"
      >
        <Plug size={12} />
        <span>{healthLabel[healthState]}</span>
      </button>
    </div>
  </aside>

  <!-- Main View Area -->
  <main class="main">
    {#if activeView === 'chat'}
      <section class="chat-view">
        <!-- Chat Header -->
        <header class="chat-header">
          <div class="chat-header-info">
            <h1 class="chat-title">{activeConversation?.title || '新对话'}</h1>
            <div class="chat-subtitle">
              <span class="model-badge">{config.model}</span>
              <span class="dot-sep">·</span>
              <button class="conn-text-btn" onclick={() => showRuntimeModal = true}>
                <span class="conn-status-text {healthState}">
                  {healthState === 'online' ? '已连接' : healthLabel[healthState]}
                </span>
              </button>
            </div>
          </div>
          <div class="chat-header-actions">
            {#if busy}
              <button class="text-action stop-action" onclick={stop}>
                <Square size={13} />
                <span>停止生成</span>
              </button>
            {/if}
            <button class="icon-action" onclick={newConversation} title="新建对话" aria-label="新建对话">
              <Plus size={15} />
            </button>
          </div>
        </header>

        <!-- Messages Stream -->
        <div
          class="messages"
          bind:this={messagesContainer}
          onscroll={handleScroll}
        >
          {#if !activeMessages.length}
            <div class="chat-empty-container">
              <EmptyState
                icon="⌁"
                title="开启新对话"
                description="与阿佩瑞斯智能伙伴交谈。记忆提取、工具执行与安全审查均由底层运行时驱动。"
              >
                <div class="quick-prompts-grid">
                  {#each quickPrompts as prompt}
                    <button class="quick-prompt-btn" onclick={() => applyQuickPrompt(prompt)}>
                      <Sparkles size={12} class="sparkle-icon" />
                      <span>{prompt}</span>
                    </button>
                  {/each}
                </div>
              </EmptyState>
            </div>
          {:else}
            {#each activeMessages as message (message.id)}
              <article
                class="message-row"
                class:user={message.role === 'user'}
                class:assistant={message.role === 'assistant'}
                class:system={message.role === 'system'}
              >
                <div class="message-avatar">
                  {message.role === 'user' ? '主' : message.role === 'system' ? '系' : 'A'}
                </div>
                <div class="message-body">
                  <MessageContent
                    {message}
                    onRetry={(t) => { draft = t; void send(); }}
                  />
                </div>
              </article>
            {/each}
          {/if}

          {#if error}
            <div class="error-banner" role="alert">
              <AlertCircle size={14} />
              <span>{error}</span>
            </div>
          {/if}
        </div>

        <!-- Scroll to bottom float button -->
        {#if showScrollBottomBtn}
          <button class="scroll-bottom-btn" onclick={() => scrollToBottom(true)} aria-label="回到底部">
            <ChevronDown size={16} />
            <span>回到底部</span>
          </button>
        {/if}

        <!-- Composer Footer -->
        <footer class="composer-wrap">
          <div class="composer">
            <textarea
              bind:value={draft}
              rows="3"
              placeholder="给阿佩瑞斯留言…… (Enter 发送, Shift+Enter 换行, 输入 / 查看指令)"
              disabled={busy}
              onkeydown={(event) => {
                if (event.key === 'Enter' && !event.shiftKey) {
                  event.preventDefault();
                  void send();
                }
              }}
            ></textarea>

            <div class="composer-toolbar">
              <div class="composer-hints">
                <span class="hint-text">Enter 发送</span>
              </div>
              <div class="composer-actions">
                {#if busy}
                  <button class="primary-button stop-btn" onclick={stop} aria-label="停止生成" title="停止生成">
                    <Square size={14} />
                  </button>
                {:else}
                  <button
                    class="primary-button send-btn"
                    onclick={() => send()}
                    disabled={!draft.trim() || healthState === 'offline'}
                    aria-label="发送消息"
                  >
                    <ArrowUp size={16} />
                  </button>
                {/if}
              </div>
            </div>
          </div>
        </footer>
      </section>
    {:else if activeView === 'conversations'}
      <ConversationsView
        {conversations}
        activeId={activeId || ''}
        {config}
        onOpen={openConversation}
        onCreate={newConversation}
        onArchive={archiveConversation}
        onDelete={deleteConversation}
        onRename={(id, title) => updateConversation(id, {title})}
        onPin={(id) => {
          const conv = conversations.find((item) => item.id === id);
          if (conv) updateConversation(id, {pinned: !conv.pinned});
        }}
      />
    {:else if activeView === 'activity'}
      <ActivityView {config} {capabilities} />
    {:else if activeView === 'tools'}
      <ToolsView {config} {capabilities} />
    {:else if activeView === 'memory'}
      <MemoryView {config} {capabilities} />
    {:else if activeView === 'settings'}
      <SettingsView
        {config}
        onSave={(newCfg) => {
          config = newCfg;
          saveConfig(newCfg);
          agentRuntime = createAgentRuntime(newCfg);
          void refreshConnection();
        }}
        onClearLocalData={() => {
          conversations = [];
          activeId = null;
          persist();
        }}
      />
    {/if}

  </main>
</div>

<!-- Runtime Diagnostics Modal -->
<RuntimeModal
  open={showRuntimeModal}
  report={healthReport}
  {capabilities}
  isRefreshing={isRefreshingHealth}
  onClose={() => showRuntimeModal = false}
  onRefresh={refreshConnection}
/>

<style>
  .shell {
    height: 100vh;
    display: flex;
    background: var(--surface);
  }
  .sidebar {
    width: 220px;
    flex: none;
    display: flex;
    flex-direction: column;
    border-right: 1px solid var(--line);
    background: var(--surface);
  }
  .sidebar-brand {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 16px 18px;
    border-bottom: 1px solid var(--line);
  }
  .logo-mark {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: 8px;
    background: var(--amber-wash);
    color: var(--amber);
    font: 700 15px var(--sans);
  }
  .brand-name {
    font-size: 13px;
    font-weight: 600;
    color: var(--text);
    flex: 1;
  }
  .status-indicator-btn {
    border: 0;
    background: transparent;
    padding: 2px;
    cursor: pointer;
    display: grid;
    place-items: center;
  }
  .nav {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 12px 10px;
  }
  .nav button {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: var(--muted);
    padding: 9px 12px;
    text-align: left;
    font-size: 13px;
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .nav button:hover {
    background: var(--surface-2);
    color: var(--text);
  }
  .nav button.active {
    background: var(--amber-wash);
    color: var(--amber);
    font-weight: 500;
  }
  .proactive-hint {
    margin: 8px 10px;
    padding: 8px 10px;
    border-radius: 7px;
    background: var(--amber-wash);
    border: 1px solid var(--amber-line);
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--amber);
    font-size: 11px;
    line-height: 1.4;
  }
  .sidebar-footer {
    padding: 12px 10px;
    border-top: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .new-chat-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    width: 100%;
    padding: 8px;
    border-radius: 6px;
    background: var(--surface-2);
    border: 1px solid var(--line-strong);
    color: var(--text);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
  }
  .new-chat-btn:hover {
    border-color: var(--amber-line);
    color: var(--amber);
  }
  .conn-hint {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--faint);
    font-size: 11px;
    padding: 4px 6px;
    border: 0;
    background: transparent;
    cursor: pointer;
    border-radius: 4px;
    text-align: left;
  }
  .conn-hint:hover {
    background: var(--surface-2);
    color: var(--muted);
  }
  .conn-hint.offline { color: var(--danger); }
  .conn-hint.degraded { color: var(--amber); }
  .conn-hint.error { color: var(--danger); }

  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    min-height: 0;
    background: var(--surface);
    position: relative;
  }
  .chat-view {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .chat-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 28px;
    border-bottom: 1px solid var(--line);
    background: var(--surface);
    user-select: none;
  }
  .chat-title {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
    color: var(--text);
  }
  .chat-subtitle {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 2px;
  }
  .model-badge {
    font-family: var(--mono);
    font-size: 11px;
    color: var(--muted);
  }
  .dot-sep {
    color: var(--faint);
    font-size: 11px;
  }
  .conn-text-btn {
    border: 0;
    background: transparent;
    padding: 0;
    cursor: pointer;
  }
  .conn-status-text {
    font-size: 11px;
    color: var(--green);
  }
  .conn-status-text.offline { color: var(--danger); }
  .conn-status-text.degraded { color: var(--amber); }
  .conn-status-text.generating { color: var(--amber); }
  .conn-status-text.error { color: var(--danger); }

  .chat-header-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .stop-action {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 4px 8px;
    border-radius: 6px;
    border: 1px solid rgba(224, 91, 80, 0.4);
    background: rgba(224, 91, 80, 0.1);
    color: var(--danger);
    font-size: 11px;
    cursor: pointer;
  }
  .icon-action {
    width: 28px;
    height: 28px;
    border-radius: 6px;
    border: 1px solid var(--line);
    background: var(--surface-2);
    color: var(--muted);
    display: grid;
    place-items: center;
    cursor: pointer;
  }
  .icon-action:hover {
    border-color: var(--amber-line);
    color: var(--amber);
  }

  .messages {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 20px max(24px, calc((100% - 880px) / 2));
    display: flex;
    flex-direction: column;
    gap: 20px;
  }
  .chat-empty-container {
    margin: auto 0;
  }
  .quick-prompts-grid {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    justify-content: center;
    max-width: 600px;
    margin-top: 14px;
  }
  .quick-prompt-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border-radius: 999px;
    background: var(--surface-2);
    border: 1px solid var(--line-strong);
    color: var(--muted);
    font-size: 12px;
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .quick-prompt-btn:hover {
    border-color: var(--amber-line);
    color: var(--amber);
    background: var(--surface-3);
  }
  :global(.sparkle-icon) {
    color: var(--amber);
  }

  .message-row {
    display: flex;
    gap: 12px;
    max-width: 85%;
  }
  .message-row.user {
    align-self: flex-end;
    flex-direction: row-reverse;
  }
  .message-avatar {
    flex: none;
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: 50%;
    background: var(--surface-3);
    color: var(--muted);
    font-size: 11px;
    font-weight: 700;
  }
  .message-row.user .message-avatar {
    background: var(--amber-wash);
    color: var(--amber);
  }
  .message-body {
    min-width: 0;
    flex: 1;
  }
  .error-banner {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 28px;
    padding: 10px 14px;
    background: rgba(224, 91, 80, 0.15);
    border: 1px solid rgba(224, 91, 80, 0.35);
    color: var(--danger);
    border-radius: 6px;
    font-size: 12px;
  }

  .scroll-bottom-btn {
    position: absolute;
    bottom: 120px;
    left: 50%;
    transform: translateX(-50%);
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 14px;
    border-radius: 999px;
    background: var(--surface-3);
    border: 1px solid var(--line-strong);
    color: var(--text);
    font-size: 12px;
    box-shadow: var(--shadow);
    cursor: pointer;
    z-index: 10;
    transition: all 0.15s ease;
  }
  .scroll-bottom-btn:hover {
    border-color: var(--amber-line);
    color: var(--amber);
  }

  .composer-wrap {
    padding: 8px max(24px, calc((100% - 880px) / 2)) 16px;
  }
  .composer {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--line-strong);
    background: var(--surface-2);
    border-radius: 10px;
    padding: 10px 14px;
    transition: border-color 0.15s ease, box-shadow 0.15s ease;
  }
  .composer:focus-within {
    border-color: var(--amber-line);
    box-shadow: 0 0 0 3px var(--amber-wash);
  }
  .composer textarea {
    width: 100%;
    resize: none;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--text);
    line-height: 1.55;
    font-size: 13px;
  }
  .composer-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-top: 6px;
    padding-top: 4px;
    border-top: 1px solid rgba(255, 255, 255, 0.04);
  }
  .hint-text {
    font-size: 11px;
    color: var(--faint);
  }
  .send-btn, .stop-btn {
    width: 32px;
    height: 32px;
    border-radius: 7px;
    border: 0;
    display: grid;
    place-items: center;
    cursor: pointer;
  }
  .send-btn {
    background: var(--amber);
    color: #19120a;
  }
  .send-btn:hover:not(:disabled) {
    background: var(--amber-hi);
  }
  .send-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .stop-btn {
    background: var(--danger);
    color: #fff;
  }
  .placeholder-view {
    flex: 1;
    display: grid;
    place-items: center;
  }
</style>
