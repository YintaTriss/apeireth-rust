<script lang="ts">
  import {onMount, tick, untrack} from 'svelte';
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
    Plug,
    ChevronDown,
    Sparkles,
    AlertCircle,
    Sofa,
    Gauge,
    Eclipse,
  } from 'lucide-svelte';
  import MessageContent from './lib/MessageContent.svelte';
  import StatusDot from './components/StatusDot.svelte';
  import RuntimeModal from './lib/components/RuntimeModal.svelte';
  import EmptyState from './lib/components/EmptyState.svelte';
  import SceneLayer from './lib/scene/SceneLayer.svelte';
  import PlanetLayer from './lib/scene/PlanetLayer.svelte';
  import BridgeLayer from './lib/bridge/BridgeLayer.svelte';
  import DeepCabinLayer from './lib/cabin/DeepCabinLayer.svelte';
  import IntroLayer from './lib/intro/IntroLayer.svelte';
  import {localClockHour} from './lib/scene/timeline';
  import ConversationsView from './lib/ConversationsView.svelte';
  import ActivityView from './lib/views/ActivityView.svelte';
  import ToolsView from './lib/views/ToolsView.svelte';
  import MemoryView from './lib/MemoryView.svelte';
  import SettingsView from './lib/views/SettingsView.svelte';

  import type {
    ApeirethConfig,
    ApprovalRequestItem,
    CapabilityManifest,
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
    subscribeCompanionEvents,
    type CompanionPresentationState,
  } from './lib/runtime';
  import {presenceStore, subscribePresence} from './lib/presence';

  // 6 大一级导航（信息架构不变；视觉改为左侧细竖条，金色 = 当前项，规范 §2.1 金色纪律）
  const nav = [
    {id: 'chat', label: '对话', icon: MessageCircleMore},
    {id: 'conversations', label: '会话', icon: MessagesSquare},
    {id: 'activity', label: '活动', icon: Activity},
    {id: 'tools', label: '工具', icon: Wrench},
    {id: 'memory', label: '记忆', icon: Layers3},
    {id: 'settings', label: '设置', icon: Settings},
  ] as const;

  // ---------- 波次 4：三模式骨架 ----------
  // companion=陪伴（舰桥+对话，默认）｜engineering=工程（深舱+页面层）｜focus=专注（临渊机位+chrome 淡出）
  type ModeId = 'companion' | 'engineering' | 'focus';
  // 开发覆写 ?mode=focus|engineering（与 ?hour= 同纪律：初始模式按参数设定，供无头截图验证）
  const modeQuery =
    typeof window !== 'undefined' ? new URLSearchParams(window.location.search).get('mode') : null;
  const initialMode: ModeId =
    modeQuery === 'engineering' || modeQuery === 'focus' ? modeQuery : 'companion';
  let mode = $state<ModeId>(initialMode);

  // ---------- 开场动画（火之文明史序章）门禁 ----------
  // 【2026-08-22 封存】v1 审美验收未过（主人评：一言难尽），默认关闭不再自动播放，
  // 保留全部引擎代码待额度充足后重启打磨（详见 docs/design/intro-animation.md）。
  // 重看方式：?intro=1 强制重播；?it=<秒> 冻结开场时钟供无头截图（永不自然播完）；
  // prefers-reduced-motion → 强制参数也不播，直接进产品。
  // 播放期间 SceneLayer/PlanetLayer/BridgeLayer 全程挂载绝不卸载（无缝接缝的物理基础），
  // 全部 chrome 以 .intro-playing class 隐藏；落幅 1.5s IntroLayer 淡出，活舰桥显形。
  const introQuery =
    typeof window !== 'undefined' ? new URLSearchParams(window.location.search) : null;
  const introForced = introQuery?.get('intro') === '1' || (introQuery?.has('it') ?? false);
  const introReduceMotion =
    typeof window !== 'undefined' &&
    typeof window.matchMedia === 'function' &&
    window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  // 封存期：仅显式强制参数才播（reduced-motion 仍有最终否决权）
  let introPlaying = $state(introForced && !introReduceMotion);

  function handleIntroComplete(): void {
    introPlaying = false;
    try {
      localStorage.setItem('ap-intro-seen', '1');
    } catch {
      /* 存储不可用时静默跳过 */
    }
  }

  // 初始视图与初始模式对齐：工程直接落「活动」页（深舱是页面层的家），其余落对话
  let activeView = $state<ViewId>(initialMode === 'engineering' ? 'activity' : 'chat');

  // 模式切换器（右缘三段胶囊）：舰桥 / 深舱 / 临渊
  const modes = [
    {id: 'companion', label: '陪伴 · 舰桥', icon: Sofa},
    {id: 'engineering', label: '工程 · 深舱', icon: Gauge},
    {id: 'focus', label: '专注 · 临渊', icon: Eclipse},
  ] as const;

  // 场景受控机位：专注=临渊(1)，陪伴=远眺(0)，工程=null（深舱不透明盖住场景，引擎自管理）
  const sceneCamera = $derived(mode === 'focus' ? 1 : mode === 'engineering' ? null : 0);

  function setMode(next: ModeId): void {
    if (next === mode) return;
    mode = next;
    if (next === 'focus') {
      activeView = 'chat'; // 进专注时若正开着页面层视图，退回 chat
    } else if (next === 'engineering') {
      if (activeView === 'chat') activeView = 'activity'; // 工程模式落在页面层（活动），左 rail 照常工作
    } else {
      activeView = 'chat'; // 切回陪伴：回对话（相机由 sceneCamera 带回远眺）
    }
  }

  // 点黑洞 = 进入专注模式（§4.2 临渊机位由引擎承担，此处管模式）；
  // 工程模式下深舱盖住黑洞，忽略穿透到场景的点击
  function handleBlackholeClick(): void {
    if (mode === 'engineering') return;
    setMode('focus');
  }

  // Esc 退出专注回陪伴
  function handleModeKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape' && mode === 'focus') setMode('companion');
  }

  // 舰内时刻（规范 §3 时间线照明）：默认跟随本地时钟，30s 心跳刷新；
  // 开发调试覆写 ?hour=22 强制指定时刻（截图验证各时间档用），覆写时不走时钟。
  const hourQuery =
    typeof window !== 'undefined' ? new URLSearchParams(window.location.search).get('hour') : null;
  const hourOverride =
    hourQuery !== null && hourQuery.trim() !== '' && Number.isFinite(Number(hourQuery))
      ? Number(hourQuery)
      : null;
  let timelineHour = $state(hourOverride ?? localClockHour());
  let config = $state<ApeirethConfig>(loadConfig());
  let conversations = $state<Conversation[]>(loadConversations());
  let activeId = $state<string | null>(null);
  let draft = $state('');
  let busy = $state(false);
  let error = $state('');
  let pendingApprovals = $state<ApprovalRequestItem[]>([]);
  let isReasoning = $state(false);
  let isExecutingTool = $state(false);
  let legacyToast = $state('');
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

  // 星尘条（规范 §5.3：memory_recall → 对话流中的「他想起了 N 段记忆」，脱敏，不含原文）。
  // 会话内瞬态：不持久化——星尘是「此刻」的痕迹，刷新即散。按会话 id 分桶。
  interface Stardust {
    id: string;
    found: number;
    keywords: string[];
    ts: number;
  }
  let stardusts = $state<Record<string, Stardust[]>>({});
  let lastDustAt = 0; // 非响应式记账：已消费到的 memory_recall receivedAt

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

  // 对话流 = 消息 + 星尘条，按时间戳归并（同刻消息优先于星尘）
  type FlowItem =
    | {kind: 'msg'; id: string; ts: number; message: ChatMessage}
    | {kind: 'dust'; id: string; ts: number; dust: Stardust};

  const flowItems = $derived.by<FlowItem[]>(() => {
    const items: FlowItem[] = activeMessages.map((m) => ({
      kind: 'msg',
      id: m.id,
      ts: m.timestamp ?? 0,
      message: m,
    }));
    const dusts = (activeId ? stardusts[activeId] : undefined) ?? [];
    for (const d of dusts) items.push({kind: 'dust', id: d.id, ts: d.ts, dust: d});
    items.sort((a, b) => a.ts - b.ts || (a.kind === b.kind ? 0 : a.kind === 'msg' ? -1 : 1));
    return items;
  });

  // 他的卡片左缘光晕强度：由真实 presence 状态驱动（规范 §5.3 光晕随 bright 呼吸）；
  // 无数据时取静息微光 —— 金线本身不消失，消失的只是呼吸。
  const presenceGlow = $derived.by(() => {
    const cur = $presenceStore.current;
    if (!cur) return 0.14;
    const base = cur.mode === 'speaking' ? 0.5 : cur.mode === 'thinking' ? 0.32 : 0.16;
    return Math.min(0.65, base + Math.max(0, cur.p) * 0.12);
  });

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

  /**
   * 他主动开口（legacy `[他说] …` 行，契约 §5.1；initiative/spoke 的完整话术由此送达）：
   * 按规范 §5.3 走与「他的消息」相同的卡片语言进入对话流。
   */
  function appendProactiveMessage(text: string): void {
    const conversation = ensureConversation();
    pushMessage(conversation.id, {
      id: crypto.randomUUID(),
      role: 'assistant',
      text,
      time: new Date().toLocaleTimeString('zh-CN', {hour: '2-digit', minute: '2-digit'}),
      timestamp: Date.now(),
      proactive: 'initiative',
    });
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
    // presence 遗留整合点 2：对话请求开始 → thinking（等首字节）；首段文本到达 → speaking
    presenceStore.setChatActive(true);

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
            presenceStore.setSpeaking(true); // 流式输出进行中 = 他在说话
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
      presenceStore.setSpeaking(false);
      presenceStore.setChatActive(false);
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

  // 星尘条：监听 presenceStore.recentEvents，新 memory_recall 事件落进当前会话流。
  // recentEvents 已由 store 按 (type, at) 去重；此处按 receivedAt 水位线消费，幂等。
  $effect(() => {
    const records = $presenceStore.recentEvents;
    const fresh: Stardust[] = [];
    let maxAt = lastDustAt;
    for (const r of records) {
      if (r.event.type !== 'memory_recall') continue;
      if (r.receivedAt <= lastDustAt) continue;
      maxAt = Math.max(maxAt, r.receivedAt);
      fresh.push({
        id: `dust-${r.receivedAt}`,
        found: r.event.found,
        keywords: Array.isArray(r.event.keywords) ? r.event.keywords : [],
        ts: r.receivedAt,
      });
    }
    if (!fresh.length) return;
    lastDustAt = maxAt;
    untrack(() => {
      const convId = activeId;
      if (!convId) return; // 无活动会话时不落（边缘：事件发生在对话外）
      const list = stardusts[convId] ?? [];
      stardusts = {...stardusts, [convId]: [...list, ...fresh]};
      void triggerAutoScroll();
    });
  });

  onMount(() => {
    if (!activeId && conversations.length) activeId = conversations[0].id;
    void refreshConnection();

    // 舰内时刻心跳：无 ?hour= 覆写时每 30s 对齐本地时钟（照明过渡由 CSS/rAF 慢性子承担）
    const hourTimer =
      hourOverride === null
        ? window.setInterval(() => {
            timelineHour = localClockHour();
          }, 30000)
        : null;

    // presence 频道主订阅（波次 2 壳层整合点）：EventSource + 指数退避 + SIM 纪律。
    // 与下方 legacy 订阅并存是设计内行为——store 按 (type, at) 去重（presence.ts dedupKey）。
    const unsubscribePresence = subscribePresence(config.baseUrl);

    // 订阅 SSE 伴随体事件通道 (主动涌现与反思通知). Reconciled from master.
    // G5 修复: 频道现为 legacy 文本行 + presence JSON 行共流 (契约 §5.1/§8.1) —
    // 先经 presence 分流: JSON 行进 presenceStore, 仅 legacy 文本行继续下行。
    // 波次 2：`[他说]` 行 = 他主动开口 → 进入对话流（规范 §5.3）；
    // 其余 legacy 行（如测试事件）→ 轻量 toast，不进对话。
    const unsubscribeEvents = subscribeCompanionEvents(config, (event) => {
      if (presenceStore.ingestLine(event.text) !== 'legacy') return;
      const text = event.text.trim();
      if (text.startsWith('[他说]')) {
        const said = text.slice('[他说]'.length).trim();
        if (said) {
          appendProactiveMessage(said);
          void triggerAutoScroll();
        }
        return;
      }
      legacyToast = text;
      window.setTimeout(() => {
        if (legacyToast === text) legacyToast = '';
      }, 12000);
    });

    // 后台健康轮询与审批请求同步 (真实 HTTP /health + capability manifest).
    const timer = window.setInterval(() => {
      void refreshConnection();
    }, 15000);

    return () => {
      window.clearInterval(timer);
      if (hourTimer !== null) window.clearInterval(hourTimer);
      unsubscribeEvents();
      unsubscribePresence();
    };
  });
</script>

<svelte:window onkeydown={handleModeKeydown} />

<div
  class="shell"
  class:mode-focus={mode === 'focus'}
  class:mode-engineering={mode === 'engineering'}
  class:intro-playing={introPlaying}
>
  <!-- 场景层（z 最低，规范 §5.1）：黑洞星空铺底，用户从未离开舰桥。
       页面层面板打开或运行时弹窗时关掉鼠标视差（interactive=false）。
       hour 与 BridgeLayer 共用同一舰内时刻（含 ?hour= 开发覆写），两层照明保持同步。
       波次 4：cameraIndex 受控机位（专注=临渊/陪伴=远眺/工程=自管理）；
       点黑洞 = 进入专注模式。 -->
  <SceneLayer
    presence={$presenceStore.current}
    hour={timelineHour}
    interactive={activeView === 'chat' && !showRuntimeModal}
    cameraIndex={sceneCamera}
    onBlackholeClick={handleBlackholeClick}
  />

  <!-- 窗外巨行星层（波次 3b）：DOM 序在场景层之后、舰桥内装之前——行星 physically
       在舷窗外，舰桥窗框会正确压住它；与场景层共用同一舰内时刻（含 ?hour= 覆写）。
       波次 4 补丁：专注模式下随舰桥一起淡出（只留黑洞+星空），保持挂载不断状态。 -->
  <div class="planet-xfade" class:layer-off={mode === 'focus'}>
    <PlanetLayer hour={timelineHour} />
  </div>

  <!-- 舰桥/深舱内装交叉淡（波次 4）：两层常驻 DOM 按模式切 opacity（工程交叉淡 0.8s、
       专注淡出 0.6s），黑洞场景与行星层保持挂载不断状态；深舱整幅不透明，工程模式下
       盖住下层场景。舰桥在工程与专注模式都淡出——专注时全屏只剩黑洞+星空。 -->
  <div class="layer-xfade" class:layer-off={mode !== 'companion'}>
    <BridgeLayer hour={timelineHour} />
  </div>
  <div class="layer-xfade" class:layer-off={mode !== 'engineering'}>
    <DeepCabinLayer hour={timelineHour} />
  </div>

  <div class="bridge-ui">
    <!-- 左侧细竖条导航：金色高亮当前项（§2.1 金色纪律 = 他的存在色兼作激活态） -->
    <nav class="rail" aria-label="主要导航">
      <div class="rail-brand" title="Apeireth 舰桥">A</div>

      <div class="rail-nav">
        {#each nav as item (item.id)}
          <button
            class="rail-btn"
            class:active={activeView === item.id}
            onclick={() => (activeView = item.id)}
            title={item.label}
            aria-label={item.label}
            aria-current={activeView === item.id ? 'page' : undefined}
          >
            <item.icon size={17} />
          </button>
        {/each}
      </div>

      <div class="rail-foot">
        <button class="rail-btn" onclick={newConversation} title="新对话" aria-label="新对话">
          <Plus size={17} />
        </button>
        <button
          class="rail-status"
          class:offline={healthState === 'offline'}
          class:degraded={healthState === 'degraded'}
          class:error={healthState === 'error'}
          onclick={() => (showRuntimeModal = true)}
          title="{healthLabel[healthState]} — 点击查看运行时详情"
          aria-label="运行时状态"
        >
          <StatusDot
            size="small"
            off={healthState === 'offline'}
            active={healthState === 'generating'}
          />
        </button>
      </div>
    </nav>

    <!-- Main View Area -->
    <main class="main">
      {#if activeView === 'chat'}
        <section class="chat-view">
          <!-- Chat Header：透明浮于场景上，状态行含 SIM 标记（§5.4） -->
          <header class="chat-header">
            <div class="chat-header-info">
              <h1 class="chat-title">{activeConversation?.title || '新对话'}</h1>
              <div class="chat-statusline">
                <span class="model-badge">{config.model}</span>
                <span class="dot-sep">·</span>
                <button class="conn-text-btn" onclick={() => (showRuntimeModal = true)}>
                  <span class="conn-status-text {healthState}">
                    {healthState === 'online' ? '已连接' : healthLabel[healthState]}
                  </span>
                </button>
                {#if $presenceStore.simulated}
                  <span
                    class="sim-badge"
                    title="presence 频道断连超过 30 秒：当前呈现为本机中性默认值（模拟态标注，设计规范 §5.4）"
                  >SIM</span>
                {/if}
              </div>
            </div>
            <div class="chat-header-actions">
              {#if busy}
                <button class="text-action stop-action" onclick={stop}>
                  <Square size={13} />
                  <span>停止生成</span>
                </button>
              {/if}
            </div>
          </header>

          <!-- Messages Stream：容器让出空区指针事件给场景（点黑洞=临渊机位），
               消息行整行带宽恢复事件以保滚动。--presence-glow 由真实 presence 驱动。 -->
          <div
            class="messages"
            bind:this={messagesContainer}
            onscroll={handleScroll}
            style:--presence-glow={presenceGlow.toFixed(3)}
          >
            {#if !flowItems.length}
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
              {#each flowItems as item (item.id)}
                {#if item.kind === 'dust'}
                  <!-- 星尘条（§5.3）：他想起了 N 段记忆 —— 脱敏，不含原文 -->
                  <div class="stardust" role="status">
                    <span class="stardust-line" aria-hidden="true"></span>
                    <span class="stardust-text">他想起了 {item.dust.found} 段记忆</span>
                    {#if item.dust.keywords.length}
                      <span class="stardust-keys">{item.dust.keywords.slice(0, 4).join(' · ')}</span>
                    {/if}
                    <span class="stardust-line" aria-hidden="true"></span>
                  </div>
                {:else}
                  <article
                    class="msg-row"
                    class:user={item.message.role === 'user'}
                    class:assistant={item.message.role === 'assistant'}
                    class:system={item.message.role === 'system'}
                  >
                    <div class="msg-card" class:ap-clip-corner={item.message.role === 'user'}>
                      <MessageContent
                        message={item.message}
                        onRetry={(t) => { draft = t; void send(); }}
                      />
                    </div>
                  </article>
                {/if}
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

          <!-- Composer：底部居中细长输入条，近黑面板 + 金边聚焦态，发送按钮金色 -->
          <footer class="composer-wrap">
            <div class="composer-bar">
              <textarea
                bind:value={draft}
                rows="1"
                placeholder="给阿佩瑞斯留言…… (Enter 发送, Shift+Enter 换行)"
                disabled={busy}
                onkeydown={(event) => {
                  if (event.key === 'Enter' && !event.shiftKey) {
                    event.preventDefault();
                    void send();
                  }
                }}
              ></textarea>

              {#if busy}
                <button class="composer-btn stop" onclick={stop} aria-label="停止生成" title="停止生成">
                  <Square size={14} />
                </button>
              {:else}
                <button
                  class="composer-btn send"
                  onclick={() => send()}
                  disabled={!draft.trim() || healthState === 'offline'}
                  aria-label="发送消息"
                  title="发送"
                >
                  <ArrowUp size={16} />
                </button>
              {/if}
            </div>
            <p class="composer-hint">Enter 发送 · Shift+Enter 换行</p>
          </footer>
        </section>
      {:else}
        <!-- 页面层（§5.1）：近黑半透明面板浮在虚化场景上，括号角标（§5.2）。场景透见，不离开舰桥。 -->
        <div class="page-layer ap-panel ap-bracket">
          {#if activeView === 'conversations'}
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
        </div>
      {/if}
    </main>
  </div>

  <!-- 模式切换器（波次 4）：右缘中部竖向三段胶囊，与左 rail 视觉平衡——
       金色当前态 + 左缘刻度线（沿用 rail 语言），title 中文标签。
       专注模式下随 chrome 一起淡出（退出靠下方「返回舰桥」胶囊 + Esc）。 -->
  <nav class="mode-switch" aria-label="模式切换">
    {#each modes as item (item.id)}
      <button
        class="mode-btn"
        class:active={mode === item.id}
        onclick={() => setMode(item.id)}
        title={item.label}
        aria-label={item.label}
        aria-current={mode === item.id ? 'page' : undefined}
      >
        <item.icon size={16} />
      </button>
    {/each}
  </nav>

  <!-- 专注模式退出胶囊：底部居中极简，金色描边半透明黑底，hover 增亮 -->
  <button class="focus-exit" onclick={() => setMode('companion')}>返回舰桥</button>

  <!-- 非「他说」legacy 行（测试事件等）：轻量 toast，不进对话流 -->
  {#if legacyToast}
    <div class="legacy-toast" role="status">
      <Sparkles size={12} />
      <span>{legacyToast}</span>
    </div>
  {/if}

  <!-- 开场动画「火之文明史」：全屏覆盖一切（z 最高），播放期 chrome 以 .intro-playing
       隐藏；SceneLayer/PlanetLayer/BridgeLayer 全程挂载绝不卸载——落幅 1.5s IntroLayer
       整体淡出，底下活舰桥显形完成无缝接缝；播完卸载（引擎 loseContext 释放 GL） -->
  {#if introPlaying}
    <IntroLayer onComplete={handleIntroComplete} />
  {/if}
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
  /* ============================================================
     壳层：场景铺底 + 全息 UI 浮层（规范 §5.1 三层架构）
     ============================================================ */
  .shell {
    position: relative;
    height: 100vh;
    overflow: hidden;
    background: var(--ap-space-void, #07070c);
  }
  /* UI 浮层整体不截获指针——空区点击穿透到场景（§4.2 点黑洞 = 临渊机位）；
     各交互件自行恢复 pointer-events。 */
  .bridge-ui {
    position: absolute;
    inset: 0;
    z-index: 2;
    display: flex;
    pointer-events: none;
  }
  .rail,
  .chat-header,
  .composer-wrap,
  .scroll-bottom-btn,
  .page-layer,
  .legacy-toast {
    pointer-events: auto;
  }

  /* ---------- 左侧细竖条导航 ---------- */
  .rail {
    width: 60px;
    flex: none;
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 14px 0 12px;
    /* 极轻的可读性渐变，非面板——竖条浮在深空上 */
    background: linear-gradient(90deg, rgba(7, 7, 12, 0.62), rgba(7, 7, 12, 0.2) 72%, transparent);
    user-select: none;
  }
  .rail-brand {
    width: 40px;
    height: 40px;
    display: grid;
    place-items: center;
    margin-bottom: 10px;
    font-family: var(--ap-font-voice);
    font-size: 19px;
    color: var(--ap-gold);
    text-shadow: 0 0 14px rgba(255, 210, 122, 0.45);
  }
  .rail-nav {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    width: 100%;
  }
  .rail-btn {
    position: relative;
    width: 40px;
    height: 40px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: rgba(232, 224, 204, 0.42);
    display: grid;
    place-items: center;
    padding: 0;
    transition: color 0.25s ease, background 0.25s ease;
  }
  .rail-btn:hover {
    color: rgba(232, 224, 204, 0.85);
    background: rgba(232, 224, 204, 0.05);
  }
  .rail-btn.active {
    color: var(--ap-gold);
    background: rgba(255, 210, 122, 0.07);
  }
  /* 当前项左缘金色刻度线 */
  .rail-btn.active::before {
    content: "";
    position: absolute;
    left: -10px;
    top: 11px;
    bottom: 11px;
    width: 2px;
    border-radius: 2px;
    background: var(--ap-gold);
    box-shadow: 0 0 8px rgba(255, 210, 122, 0.6);
  }
  .rail-foot {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
  }
  .rail-status {
    width: 40px;
    height: 32px;
    border: 0;
    background: transparent;
    display: grid;
    place-items: center;
    padding: 0;
    border-radius: 8px;
  }
  .rail-status:hover {
    background: rgba(232, 224, 204, 0.05);
  }

  /* ---------- 主区 ---------- */
  .main {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    position: relative;
    pointer-events: none;
  }
  .chat-view {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  /* ---------- 对话头：透明，状态行等宽小字（§6.2 数据文字） ---------- */
  .chat-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 15px 30px 11px;
    background: linear-gradient(180deg, rgba(7, 7, 12, 0.55), transparent);
    user-select: none;
  }
  .chat-title {
    margin: 0;
    font-size: 15px;
    font-weight: 500;
    letter-spacing: 0.04em;
    color: var(--ap-bone);
  }
  .chat-statusline {
    display: flex;
    align-items: center;
    gap: 7px;
    margin-top: 3px;
  }
  .model-badge {
    font-family: var(--ap-font-mono);
    font-size: 10px;
    letter-spacing: 0.12em;
    color: rgba(232, 224, 204, 0.4);
  }
  .dot-sep {
    color: rgba(232, 224, 204, 0.25);
    font-size: 10px;
  }
  .conn-text-btn {
    border: 0;
    background: transparent;
    padding: 0;
    cursor: pointer;
  }
  .conn-status-text {
    font-size: 10px;
    letter-spacing: 0.08em;
    color: var(--ap-semantic-success);
  }
  .conn-status-text.offline { color: var(--ap-semantic-danger); }
  .conn-status-text.degraded { color: var(--ap-semantic-warning); }
  .conn-status-text.generating { color: var(--ap-gold); }
  .conn-status-text.error { color: var(--ap-semantic-danger); }
  .conn-status-text.connecting { color: rgba(232, 224, 204, 0.5); }

  /* SIM 标记（§5.4 模拟态标注）：极小等宽字，状态行内可识别 */
  .sim-badge {
    font-family: var(--ap-font-mono);
    font-size: 9px;
    letter-spacing: 0.28em;
    padding: 1px 4px 1px 7px;
    border: 1px solid rgba(232, 217, 176, 0.4);
    border-radius: 2px;
    color: var(--ap-gold-ui);
    opacity: 0.85;
  }

  .chat-header-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .stop-action {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 4px 10px;
    border-radius: 6px;
    border: 1px solid rgba(192, 88, 78, 0.45);
    background: rgba(192, 88, 78, 0.12);
    color: var(--ap-semantic-danger);
    font-size: 11px;
    cursor: pointer;
  }

  /* ---------- 消息流 ---------- */
  .messages {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 10px max(26px, calc((100% - 880px) / 2)) 26px;
    display: flex;
    flex-direction: column;
    gap: 18px;
    pointer-events: none; /* 空区穿透到场景；子行恢复 */
    scrollbar-width: thin;
    scrollbar-color: rgba(232, 224, 204, 0.18) transparent;
  }
  .chat-empty-container {
    margin: auto 0;
    pointer-events: auto;
    /* 空态文字压得住亮盘的可读性底晕（非面板，径向渐隐） */
    background: radial-gradient(closest-side, rgba(7, 7, 12, 0.62), rgba(7, 7, 12, 0.28) 58%, transparent);
    border-radius: 24px;
    padding: 24px 0;
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
    padding: 6px 13px;
    border-radius: 999px;
    background: rgba(11, 13, 18, 0.55);
    border: 1px solid var(--ap-line);
    color: rgba(232, 224, 204, 0.68);
    font-size: 12px;
    cursor: pointer;
    backdrop-filter: blur(8px);
    -webkit-backdrop-filter: blur(8px);
    transition: border-color 0.25s ease, color 0.25s ease;
  }
  .quick-prompt-btn:hover {
    border-color: rgba(255, 210, 122, 0.5);
    color: var(--ap-gold);
  }
  :global(.sparkle-icon) {
    color: var(--ap-gold);
  }

  /* 消息行：整行带宽恢复指针事件（保滚动），卡片对齐左右 */
  .msg-row {
    display: flex;
    width: 100%;
    pointer-events: auto;
  }
  .msg-row.assistant { justify-content: flex-start; }
  .msg-row.user { justify-content: flex-end; }
  .msg-row.system { justify-content: center; }

  /* 他的卡片（§5.3 BAKER 映射）：近黑半透明 + 左缘金色存在线 + 光晕随 presence 呼吸。
     衬线引语排版由 MessageContent 的 .ap-voice 承担。 */
  .msg-row.assistant .msg-card {
    position: relative;
    width: fit-content;
    min-width: 150px;
    max-width: min(76%, 680px);
    padding: 13px 22px 9px 24px;
    background: rgba(11, 13, 18, 0.74);
    border: 1px solid var(--ap-line);
    border-left: 0;
    border-radius: 2px 7px 7px 2px;
    backdrop-filter: blur(10px);
    -webkit-backdrop-filter: blur(10px);
    box-shadow: -12px 0 30px -14px rgba(255, 210, 122, var(--presence-glow, 0.18));
    transition: box-shadow 1.2s ease;
  }
  /* 存在线：白热 → 金 → 琥珀（§2.1 存在色梯度），透明度随 presence 光晕呼吸 */
  .msg-row.assistant .msg-card::before {
    content: "";
    position: absolute;
    left: -1px;
    top: 10px;
    bottom: 10px;
    width: 2px;
    border-radius: 2px;
    background: linear-gradient(180deg, var(--ap-gold-white-hot), var(--ap-gold) 45%, var(--ap-gold-amber));
    opacity: calc(0.45 + var(--presence-glow, 0.18) * 0.8);
    box-shadow: 0 0 10px rgba(255, 210, 122, var(--presence-glow, 0.18));
    transition: opacity 1.2s ease, box-shadow 1.2s ease;
  }
  /* 工具/任务卡在他的卡内保持可读宽度 */
  .msg-row.assistant .msg-card :global(.tool-calls-container) {
    min-width: min(340px, 52vw);
  }

  /* 用户卡片（§5.3）：右侧哑白实体卡（.ap-card 配方），无晕无金无衬线，斜切角 */
  .msg-row.user .msg-card {
    max-width: min(72%, 560px);
    background: var(--ap-card);
    color: var(--ap-register-archive-ink);
    padding: 0;
  }
  .msg-row.user .msg-card :global(.user-text) {
    background: transparent;
    border: 0;
    border-radius: 0;
    padding: 10px 16px;
    color: inherit;
    font-family: var(--ap-font-ui);
    font-size: 13.5px;
    line-height: 1.75;
  }
  /* 复制钮收进卡内右下角（斜切角切的是右上/左下，避开） */
  .msg-row.user .msg-card :global(.user-copy-btn) {
    top: auto;
    bottom: 3px;
    right: 8px;
    color: rgba(38, 38, 42, 0.4);
  }
  .msg-row.user .msg-card :global(.user-copy-btn:hover) {
    color: rgba(38, 38, 42, 0.85);
  }
  .msg-row.user .msg-card :global(.user-bubble) {
    display: block;
  }

  /* 系统消息：无卡片，居中胶囊由 MessageContent 自带样式承担 */
  .msg-row.system .msg-card {
    background: transparent;
    border: 0;
    padding: 0;
  }

  /* ---------- 星尘条（§5.3）：细长居中，金色微光 ---------- */
  .stardust {
    align-self: center;
    display: flex;
    align-items: center;
    gap: 14px;
    width: min(600px, 88%);
    pointer-events: auto;
    user-select: none;
  }
  .stardust-line {
    flex: 1;
    height: 1px;
    background: linear-gradient(90deg, transparent, rgba(255, 210, 122, 0.38), transparent);
  }
  .stardust-text {
    font-family: var(--ap-font-voice);
    font-size: 11px;
    letter-spacing: 0.38em;
    white-space: nowrap;
    color: rgba(255, 210, 122, 0.8);
    text-shadow: 0 0 12px rgba(255, 210, 122, 0.35);
  }
  .stardust-keys {
    font-family: var(--ap-font-mono);
    font-size: 10px;
    letter-spacing: 0.16em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 34%;
    color: rgba(232, 224, 204, 0.38);
  }

  .error-banner {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    background: rgba(192, 88, 78, 0.14);
    border: 1px solid rgba(192, 88, 78, 0.38);
    color: var(--ap-semantic-danger);
    border-radius: 6px;
    font-size: 12px;
    pointer-events: auto;
  }

  .scroll-bottom-btn {
    position: absolute;
    bottom: 118px;
    left: 50%;
    transform: translateX(-50%);
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 14px;
    border-radius: 999px;
    background: var(--ap-panel);
    border: 1px solid var(--ap-line);
    color: var(--ap-bone);
    font-size: 12px;
    backdrop-filter: blur(12px);
    -webkit-backdrop-filter: blur(12px);
    cursor: pointer;
    z-index: 10;
    transition: border-color 0.25s ease, color 0.25s ease;
  }
  .scroll-bottom-btn:hover {
    border-color: rgba(255, 210, 122, 0.5);
    color: var(--ap-gold);
  }

  /* ---------- Composer：底部居中细长输入条，近黑面板 + 金边聚焦 ---------- */
  .composer-wrap {
    padding: 6px max(26px, calc((100% - 880px) / 2)) 16px;
  }
  .composer-bar {
    display: flex;
    align-items: flex-end;
    gap: 10px;
    background: var(--ap-panel);
    border: 1px solid var(--ap-line);
    border-radius: 9px;
    padding: 9px 9px 9px 16px;
    backdrop-filter: blur(14px);
    -webkit-backdrop-filter: blur(14px);
    transition: border-color 0.3s ease, box-shadow 0.3s ease;
  }
  .composer-bar:focus-within {
    border-color: rgba(255, 210, 122, 0.55);
    box-shadow: 0 0 0 1px rgba(255, 210, 122, 0.2), 0 8px 32px -12px rgba(255, 210, 122, 0.2);
  }
  .composer-bar textarea {
    flex: 1;
    resize: none;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--ap-bone);
    font-size: 13.5px;
    line-height: 1.6;
    min-height: 22px;
    max-height: 132px;
    field-sizing: content; /* Chromium ≥123：随内容伸长；不支持时退化为固定单行可滚 */
    padding: 2px 0;
  }
  .composer-bar textarea::placeholder {
    color: rgba(232, 224, 204, 0.3);
  }
  .composer-btn {
    width: 34px;
    height: 34px;
    border-radius: 8px;
    border: 0;
    display: grid;
    place-items: center;
    flex: none;
    cursor: pointer;
    padding: 0;
    transition: background 0.25s ease, opacity 0.25s ease;
  }
  .composer-btn.send {
    background: var(--ap-gold);
    color: #19120a;
  }
  .composer-btn.send:hover:not(:disabled) {
    background: var(--ap-gold-white-hot);
  }
  .composer-btn.send:disabled {
    opacity: 0.32;
    cursor: default;
  }
  .composer-btn.stop {
    background: transparent;
    border: 1px solid rgba(192, 88, 78, 0.5);
    color: var(--ap-semantic-danger);
  }
  .composer-hint {
    margin: 6px 6px 0;
    font-family: var(--ap-font-mono);
    font-size: 10px;
    letter-spacing: 0.14em;
    color: rgba(232, 224, 204, 0.26);
    text-align: right;
    user-select: none;
  }

  /* ---------- 页面层（§5.1）：非对话视图浮在虚化场景上的近黑面板 ---------- */
  .page-layer {
    flex: 1;
    min-height: 0;
    min-width: 0;
    display: flex;
    margin: 14px 20px 20px 14px;
    border-radius: 3px;
    overflow: hidden;
  }
  .page-layer > :global(*) {
    flex: 1;
    min-height: 0;
    min-width: 0;
  }

  /* ---------- legacy 轻量 toast（非「他说」行，如测试事件） ---------- */
  .legacy-toast {
    position: absolute;
    left: 50%;
    bottom: 98px;
    transform: translateX(-50%);
    z-index: 4;
    display: flex;
    align-items: center;
    gap: 8px;
    max-width: min(560px, 80vw);
    padding: 7px 16px;
    border-radius: 999px;
    background: var(--ap-panel);
    border: 1px solid var(--ap-line);
    backdrop-filter: blur(12px);
    -webkit-backdrop-filter: blur(12px);
    color: rgba(232, 224, 204, 0.75);
    font-size: 11px;
    letter-spacing: 0.06em;
  }
  .legacy-toast :global(svg) {
    color: var(--ap-gold);
    flex: none;
  }

  /* ============================================================
     波次 4：三模式骨架（陪伴/工程/专注）
     ============================================================ */

  /* ---------- 舰桥/深舱/行星交叉淡：常驻 DOM，按模式切 opacity（工程交叉淡 0.8s） ---------- */
  .layer-xfade {
    position: fixed;
    inset: 0;
    z-index: 1;
    pointer-events: none;
    transition: opacity 0.8s ease;
  }
  .layer-xfade.layer-off {
    opacity: 0;
  }
  /* 专注模式：舰桥+行星随 chrome 同节奏淡出（0.6s），全屏只剩黑洞+星空 */
  .shell.mode-focus .layer-xfade {
    transition-duration: 0.6s;
  }
  /* 行星层专用淡出容器（波次 4b 回归修复）：不许带 position/z-index——任何定位祖先
     都会成为 stacking context，把 .planet 的 screen 混合隔离在空背景上，黑底显形为
     黑箱盖死黑洞（PlanetLayer 文件头纪律）。static 祖先 + 静止态 opacity:1 = 无上下文、
     混合穿透；淡出中途 opacity<1 的短暂隔离面纱可接受（0.6s 内淡没）。 */
  .planet-xfade {
    pointer-events: none;
    transition: opacity 0.8s ease;
  }
  .planet-xfade.layer-off {
    opacity: 0;
  }
  .shell.mode-focus .planet-xfade {
    transition-duration: 0.6s;
  }

  /* ---------- 工程模式：页面层收成「主控台主屏」 ----------
     max-width 1150px，水平居中；高度收 40vh、顶 5vh——底缘落在视口 45% 处，
     恰好让开深舱椅背顶沿（新深舱图椅背起于 ~45.3%），整把椅子+舱底圆盘全露出来；
     圆角、近黑半透明、边框/角标语言不变；面板内部视图滚动不受影响。陪伴模式不动。 */
  .shell.mode-engineering .page-layer {
    flex: none;
    width: 100%;
    max-width: 1150px;
    height: 100%;
    max-height: 40vh;
    margin: 5vh auto auto;
  }

  /* ---------- chrome 淡出（专注模式） ----------
     左 rail、chat-header、composer、模式切换器、消息流本体 0.6s ease；
     opacity+visibility 联动（隐藏后不可聚焦、不截获指针），场景本身继续活。
     波次 4b：主人"只留黑洞"——消息流/快捷按钮/回底按钮一并淡出，
     专注模式的对话形态留给后续波次设计。 */
  .rail,
  .chat-header,
  .composer-wrap,
  .mode-switch,
  .messages,
  .scroll-bottom-btn,
  .focus-exit {
    transition: opacity 0.6s ease, visibility 0s linear 0s;
  }
  .shell.mode-focus .rail,
  .shell.mode-focus .chat-header,
  .shell.mode-focus .composer-wrap,
  .shell.mode-focus .mode-switch,
  .shell.mode-focus .messages,
  .shell.mode-focus .scroll-bottom-btn {
    opacity: 0;
    visibility: hidden;
    pointer-events: none;
    transition: opacity 0.6s ease, visibility 0s linear 0.6s;
  }
  /* 工程模式：composer 隐藏（左 rail 照常工作——它就是工程导航的家） */
  .shell.mode-engineering .composer-wrap {
    display: none;
  }
  /* 开场动画播放期：全部 chrome 隐藏（沿用专注模式的 opacity+visibility 联动纪律）；
     播放结束 .intro-playing 移除 → chrome 0.6s 淡入，与落幅后活舰桥接管同节奏 */
  .shell.intro-playing .rail,
  .shell.intro-playing .chat-header,
  .shell.intro-playing .composer-wrap,
  .shell.intro-playing .mode-switch,
  .shell.intro-playing .messages,
  .shell.intro-playing .scroll-bottom-btn,
  .shell.intro-playing .focus-exit {
    opacity: 0;
    visibility: hidden;
    pointer-events: none;
    transition: opacity 0.6s ease, visibility 0s linear 0.6s;
  }
  /* reduced-motion：模式/chrome 过渡瞬切 */
  @media (prefers-reduced-motion: reduce) {
    .rail,
    .chat-header,
    .composer-wrap,
    .mode-switch,
    .messages,
    .scroll-bottom-btn,
    .layer-xfade,
    .planet-xfade,
    .focus-exit {
      transition-duration: 0.01s !important;
      transition-delay: 0s !important;
    }
  }

  /* ---------- 模式切换器：右缘中部竖向三段胶囊（左 rail 的镜像语言） ---------- */
  .mode-switch {
    position: absolute;
    right: 12px;
    top: 50%;
    transform: translateY(-50%);
    z-index: 3;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 3px;
    padding: 5px;
    border-radius: 999px;
    background: rgba(7, 7, 12, 0.5);
    border: 1px solid var(--ap-line);
    backdrop-filter: blur(10px);
    -webkit-backdrop-filter: blur(10px);
    user-select: none;
  }
  .mode-btn {
    position: relative;
    width: 32px;
    height: 32px;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: rgba(232, 224, 204, 0.42);
    display: grid;
    place-items: center;
    padding: 0;
    cursor: pointer;
    transition: color 0.25s ease, background 0.25s ease;
  }
  .mode-btn:hover {
    color: rgba(232, 224, 204, 0.85);
    background: rgba(232, 224, 204, 0.05);
  }
  .mode-btn.active {
    color: var(--ap-gold);
    background: rgba(255, 210, 122, 0.07);
  }
  /* 当前段左缘金色刻度线（沿用 rail 语言，贴胶囊左缘） */
  .mode-btn.active::before {
    content: "";
    position: absolute;
    left: -6px;
    top: 8px;
    bottom: 8px;
    width: 2px;
    border-radius: 2px;
    background: var(--ap-gold);
    box-shadow: 0 0 8px rgba(255, 210, 122, 0.6);
  }

  /* ---------- 专注模式退出胶囊：底部居中极简，金色描边半透明黑底 ---------- */
  .focus-exit {
    position: absolute;
    left: 50%;
    bottom: 26px;
    transform: translateX(-50%);
    z-index: 3;
    padding: 8px 22px;
    border-radius: 999px;
    border: 1px solid rgba(255, 210, 122, 0.55);
    background: rgba(7, 7, 12, 0.55);
    backdrop-filter: blur(10px);
    -webkit-backdrop-filter: blur(10px);
    color: var(--ap-gold);
    font-size: 12px;
    letter-spacing: 0.22em;
    cursor: pointer;
    opacity: 0;
    visibility: hidden;
    pointer-events: none;
    transition:
      opacity 0.6s ease,
      visibility 0s linear 0.6s,
      border-color 0.25s ease,
      background 0.25s ease,
      box-shadow 0.25s ease;
  }
  .shell.mode-focus .focus-exit {
    opacity: 1;
    visibility: visible;
    pointer-events: auto;
    transition:
      opacity 0.6s ease,
      visibility 0s linear 0s,
      border-color 0.25s ease,
      background 0.25s ease,
      box-shadow 0.25s ease;
  }
  .focus-exit:hover {
    border-color: var(--ap-gold);
    background: rgba(20, 16, 8, 0.72);
    box-shadow: 0 0 18px rgba(255, 210, 122, 0.18);
  }
</style>
