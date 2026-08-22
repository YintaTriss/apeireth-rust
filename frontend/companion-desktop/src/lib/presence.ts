// Apeireth 桌面伙伴 — Presence 频道客户端 (内心状态事件)
//
// 契约: docs/02-guides/frontend-data-contract.md §5.1 / §8.1
// GET /v1/apeireth/events 上三类 data 行共流:
//   ① legacy 纯文本行 `[他说] …` / 测试事件行
//   ② presence JSON 行 (单行, serde 内部标签 type 平铺 + at RFC3339)
//   ③ presence_error 兜底帧 {"type":"presence_error","error":…}
// 分流规则: 行首 `{` → JSON (再按 type 分发), 否则按 legacy 文本; JSON 解析失败静默跳过.
//
// SIM 纪律 (docs/design/01-DESIGN-SYSTEM.md §5.4): 频道断连持续 >30s 时
// store 标记 simulated=true, current 回落本机中性默认值 (PAD 0/0/0),
// 绝不编造情绪. 平滑插值是真实数据的中间态, 不算模拟, 不触发 SIM.
//
// 本文件刻意零运行时依赖 (不 import svelte/store): store 手写 Svelte store
// 契约 (subscribe 返回 unsubscribe), 纯函数与 store 均可被 Node 直接 import
// 测试 (tests/presence-split.mjs), DOM/EventSource/rAF 全部惰性守卫.

// ============================================================
// 事件类型 — 字段名与契约 §8.1 逐字一致 (presence.rs:145-163)
// ============================================================

export interface PresencePad {
  p: number;
  a: number;
  d: number;
}

/** emotion 事件: PAD 情绪快照 (60s tick 心跳 + 主人消息触发) */
export interface EmotionEvent {
  type: 'emotion';
  at: string;
  pad: PresencePad;
  dominant: string;
  intensity: number;
  response_style: string;
  tone?: string;
}

/** initiative 门控标签: 13 种真实门控, serde 标签即线上值 (presence.rs:84-99) */
export type InitiativeGate =
  | 'sovereignty_frozen'
  | 'user_quiet'
  | 'quiet_hours'
  | 'daily_limit'
  | 'llm_budget'
  | 'depth_low'
  | 'rhythm_unknown'
  | 'rhythm_veto'
  | 'drive_low'
  | 'emotion_low'
  | 'council_veto'
  | 'policy_inactive'
  | 'gate_block';

/** initiative 事件: 开口决策 (spoke 每推; held 按 gate 原因去抖) */
export interface InitiativeEvent {
  type: 'initiative';
  at: string;
  outcome: 'spoke' | 'held';
  gate?: InitiativeGate;
  gate_label?: string;
  action?: string;
}

/** dream 事件: 做梦整合完成 (只报 serve 启动后真库增量) */
export interface DreamEvent {
  type: 'dream';
  at: string;
  merged_count: number;
  summary_prefix: string;
}

/** memory_recall 事件: 记忆被唤起 (脱敏, redacted 设计上恒 true) */
export interface MemoryRecallEvent {
  type: 'memory_recall';
  at: string;
  found: number;
  keywords: string[];
  redacted: boolean;
}

export type PresenceEvent = EmotionEvent | InitiativeEvent | DreamEvent | MemoryRecallEvent;

/** presence_error 兜底帧: 序列化失败时的显式错误行, 解析器容忍即可 */
export interface PresenceErrorFrame {
  type: 'presence_error';
  error: string;
  at?: string;
}

export type PresenceFrame = PresenceEvent | PresenceErrorFrame;

// ============================================================
// 纯逻辑: 行分流 / mode 推导 / 平滑插值 (无 DOM, 可单测)
// ============================================================

export type PresenceLineKind = 'presence' | 'legacy' | 'skip';

export interface SplitPresenceLineResult {
  kind: PresenceLineKind;
  /** kind === 'presence' 时的解析结果 */
  event?: PresenceFrame;
  /** kind === 'legacy' 时的原始文本 (已 trim) */
  text?: string;
}

const KNOWN_FRAME_TYPES: ReadonlySet<string> = new Set([
  'emotion',
  'initiative',
  'dream',
  'memory_recall',
  'presence_error',
]);

/**
 * SSE data 行分流 (契约 §8.1「分流」纪律):
 * 行首 `{` → presence JSON (再按 type 分发); 否则 legacy 文本行.
 * JSON 解析失败静默返回 skip —— 不抛错、不降级为文本展示 (G5 纪律).
 */
export function splitPresenceLine(line: string): SplitPresenceLineResult {
  const trimmed = line.trim();
  if (!trimmed) return {kind: 'skip'};
  if (!trimmed.startsWith('{')) return {kind: 'legacy', text: trimmed};
  let parsed: unknown;
  try {
    parsed = JSON.parse(trimmed);
  } catch {
    return {kind: 'skip'};
  }
  if (parsed !== null && typeof parsed === 'object' && !Array.isArray(parsed)) {
    const type = (parsed as {type?: unknown}).type;
    if (typeof type === 'string' && KNOWN_FRAME_TYPES.has(type)) {
      return {kind: 'presence', event: parsed as PresenceFrame};
    }
  }
  return {kind: 'skip'};
}

export type PresenceMode = 'quiet' | 'thinking' | 'speaking';

/** thinking 窗口: 最近 5s 内有 initiative/spoke 视为 thinking */
export const THINKING_WINDOW_MS = 5000;

export interface PresenceModeInput {
  /** 对话流进行中 (调用方 setSpeaking 告知) */
  speaking: boolean;
  /** 对话请求进行中 (已发送未流完, 调用方 setChatActive 告知) */
  chatActive: boolean;
  /** 最近一次 initiative/spoke 的本地接收时刻 (ms epoch; 0 = 从未) */
  lastSpokeAt: number;
  now: number;
}

/**
 * mode 推导:
 *   speaking = 对话流进行中;
 *   thinking = 最近 5s 内有 initiative/spoke, 或对话请求中;
 *   否则 quiet.
 */
export function derivePresenceMode(input: PresenceModeInput): PresenceMode {
  if (input.speaking) return 'speaking';
  if (input.chatActive) return 'thinking';
  if (input.lastSpokeAt > 0 && input.now - input.lastSpokeAt <= THINKING_WINDOW_MS) return 'thinking';
  return 'quiet';
}

/** 平滑插值时间常数 (~2s 指数趋近, 契约 §8.1 频率纪律: 不要逐帧跳变) */
export const SMOOTHING_TAU_MS = 2000;

/** 指数趋近一步: dtMs 后从 current 向 target 靠近 (时间常数 tauMs) */
export function approachExponential(
  current: number,
  target: number,
  dtMs: number,
  tauMs: number = SMOOTHING_TAU_MS,
): number {
  if (dtMs <= 0) return current;
  return target + (current - target) * Math.exp(-dtMs / tauMs);
}

function clampPadValue(v: unknown): number {
  const n = typeof v === 'number' && Number.isFinite(v) ? v : 0;
  return Math.min(1, Math.max(-1, n));
}

// ============================================================
// presenceStore — Svelte store 契约 (subscribe → unsubscribe)
// ============================================================

/** recentEvents 环形缓冲容量 (供 UI 做「星尘卡片」等) */
export const RECENT_EVENTS_CAPACITY = 50;

const PAD_EPSILON = 0.001;

export interface PresenceCurrent {
  p: number;
  a: number;
  d: number;
  mode: PresenceMode;
  tone?: string;
}

export interface PresenceEventRecord {
  event: PresenceFrame;
  /** 本地接收时刻 (ms epoch) */
  receivedAt: number;
}

export interface PresenceState {
  /** emotion 事件驱动 + rAF 平滑插值; 无真实数据时为 null; simulated 时为本机中性默认 */
  current: PresenceCurrent | null;
  /** SIM 标注 (设计规范 §5.4): 断连 >30s 无真实来源时为 true */
  simulated: boolean;
  /** 频道当前是否连通 (任一路真实事件到达也会置 true) */
  connected: boolean;
  /** 最近事件环形缓冲, 最新在前, 容量 50 */
  recentEvents: PresenceEventRecord[];
}

export interface PresenceStore {
  subscribe(run: (state: PresenceState) => void): () => void;
  /** 非 Svelte 消费者/测试用快照 */
  get(): PresenceState;
  /** 分流一行 SSE data; 返回该行归类 (legacy 行不进入 store) */
  ingestLine(line: string): PresenceLineKind;
  /** 直接喂入已解析帧 */
  ingest(frame: PresenceFrame): void;
  /** 调用方告知对话流开始/结束 (mode: speaking) */
  setSpeaking(speaking: boolean): void;
  /** 调用方告知对话请求开始/结束 (mode: thinking) */
  setChatActive(active: boolean): void;
  /** 订阅层回报连通状态; 连通会清除 simulated */
  setConnected(connected: boolean): void;
  setSimulated(simulated: boolean): void;
  /** 清空全部状态 (测试用) */
  reset(): void;
}

export function createPresenceStore(nowFn: () => number = () => Date.now()): PresenceStore {
  const subscribers = new Set<(state: PresenceState) => void>();

  let targetPad: PresencePad | null = null;
  let displayedPad: PresencePad | null = null;
  let tone: string | undefined;
  let speaking = false;
  let chatActive = false;
  let lastSpokeAt = 0;
  let simulated = false;
  let connected = false;
  let recentEvents: PresenceEventRecord[] = [];
  let lastEventKey = '';

  let rafId: number | null = null;
  let lastFrameAt: number | undefined;
  let visibilityHooked = false;
  let modeTimer: ReturnType<typeof setTimeout> | null = null;

  function currentMode(): PresenceMode {
    return derivePresenceMode({speaking, chatActive, lastSpokeAt, now: nowFn()});
  }

  function snapshot(): PresenceState {
    let current: PresenceCurrent | null = null;
    if (simulated) {
      // SIM 纪律: 本机中性默认值, 不编造情绪
      current = {p: 0, a: 0, d: 0, mode: currentMode()};
    } else if (displayedPad) {
      current = {p: displayedPad.p, a: displayedPad.a, d: displayedPad.d, mode: currentMode(), tone};
    }
    return {current, simulated, connected, recentEvents};
  }

  function notify(): void {
    const state = snapshot();
    for (const run of subscribers) run(state);
  }

  function cancelLoop(): void {
    if (rafId !== null && typeof cancelAnimationFrame === 'function') {
      cancelAnimationFrame(rafId);
    }
    rafId = null;
    lastFrameAt = undefined;
  }

  function hookVisibility(): void {
    if (visibilityHooked || typeof document === 'undefined') return;
    visibilityHooked = true;
    document.addEventListener('visibilitychange', () => {
      // hidden 时停插值循环 (省电); 连接本身不断线 (见 subscribePresence)
      if (document.hidden) {
        cancelLoop();
      } else {
        ensureLoop();
      }
    });
  }

  function ensureLoop(): void {
    if (typeof requestAnimationFrame !== 'function') return;
    if (typeof document !== 'undefined' && document.hidden) return;
    if (rafId !== null || !targetPad || !displayedPad) return;
    if (
      Math.abs(displayedPad.p - targetPad.p) < PAD_EPSILON &&
      Math.abs(displayedPad.a - targetPad.a) < PAD_EPSILON &&
      Math.abs(displayedPad.d - targetPad.d) < PAD_EPSILON
    ) {
      return;
    }
    hookVisibility();
    rafId = requestAnimationFrame(tick);
  }

  function tick(frameAt: number): void {
    rafId = null;
    if (!targetPad || !displayedPad) return;
    const dt = lastFrameAt === undefined ? 16 : Math.max(0, frameAt - lastFrameAt);
    lastFrameAt = frameAt;
    displayedPad = {
      p: approachExponential(displayedPad.p, targetPad.p, dt),
      a: approachExponential(displayedPad.a, targetPad.a, dt),
      d: approachExponential(displayedPad.d, targetPad.d, dt),
    };
    if (
      Math.abs(displayedPad.p - targetPad.p) < PAD_EPSILON &&
      Math.abs(displayedPad.a - targetPad.a) < PAD_EPSILON &&
      Math.abs(displayedPad.d - targetPad.d) < PAD_EPSILON
    ) {
      displayedPad = {...targetPad};
      lastFrameAt = undefined;
      notify();
      return;
    }
    notify();
    ensureLoop();
  }

  /** spoke 窗口 (5s) 到期后把 thinking 回落为 quiet */
  function scheduleModeRecheck(): void {
    if (modeTimer !== null) clearTimeout(modeTimer);
    modeTimer = setTimeout(() => {
      modeTimer = null;
      notify();
    }, THINKING_WINDOW_MS + 50);
  }

  function dedupKey(frame: PresenceFrame): string {
    // 双订阅挂接 (App  legacy 订阅 + subscribePresence) 下同一条事件会到达两次;
    // 后端 at 为推送端 Utc::now, 同事件 key 相同, 据此去重.
    const at = 'at' in frame && typeof frame.at === 'string' ? frame.at : '';
    if (frame.type === 'presence_error') return `${frame.type}|${frame.error}`;
    return `${frame.type}|${at}`;
  }

  function ingest(frame: PresenceFrame): void {
    const key = dedupKey(frame);
    if (key === lastEventKey) return;
    lastEventKey = key;

    // 真实事件到达 = 频道有活性
    connected = true;
    simulated = false;

    recentEvents = [{event: frame, receivedAt: nowFn()}, ...recentEvents].slice(0, RECENT_EVENTS_CAPACITY);

    if (frame.type === 'emotion') {
      targetPad = {
        p: clampPadValue(frame.pad?.p),
        a: clampPadValue(frame.pad?.a),
        d: clampPadValue(frame.pad?.d),
      };
      tone = typeof frame.tone === 'string' ? frame.tone : undefined;
      if (!displayedPad) displayedPad = {...targetPad}; // 首条直接落位, 不做无中生有的滑行
      ensureLoop();
    } else if (frame.type === 'initiative' && frame.outcome === 'spoke') {
      lastSpokeAt = nowFn();
      scheduleModeRecheck();
    }
    notify();
  }

  return {
    subscribe(run) {
      subscribers.add(run);
      run(snapshot());
      return () => {
        subscribers.delete(run);
      };
    },
    get: snapshot,
    ingestLine(line) {
      const result = splitPresenceLine(line);
      if (result.kind === 'presence' && result.event) ingest(result.event);
      return result.kind;
    },
    ingest,
    setSpeaking(value) {
      if (speaking === value) return;
      speaking = value;
      notify();
    },
    setChatActive(value) {
      if (chatActive === value) return;
      chatActive = value;
      notify();
    },
    setConnected(value) {
      const nextSimulated = value ? false : simulated;
      if (connected === value && simulated === nextSimulated) return;
      connected = value;
      simulated = nextSimulated;
      notify();
    },
    setSimulated(value) {
      if (simulated === value) return;
      simulated = value;
      notify();
    },
    reset() {
      cancelLoop();
      if (modeTimer !== null) {
        clearTimeout(modeTimer);
        modeTimer = null;
      }
      targetPad = null;
      displayedPad = null;
      tone = undefined;
      speaking = false;
      chatActive = false;
      lastSpokeAt = 0;
      simulated = false;
      connected = false;
      recentEvents = [];
      lastEventKey = '';
      notify();
    },
  };
}

/** 全局单例 — 场景层组件消费 `$presenceStore.current` */
export const presenceStore: PresenceStore = createPresenceStore();

// ============================================================
// subscribePresence — EventSource 订阅 + 指数退避重连
// ============================================================

/** 断连持续超过该时长即按 SIM 纪律标记 simulated (设计规范 §5.4) */
export const SIM_AFTER_MS = 30000;

const RETRY_BASE_MS = 2000;
const RETRY_MAX_MS = 30000;

export interface SubscribePresenceOptions {
  /** 可选: 顺带把 legacy 文本行交给调用方 (默认忽略, 由既有订阅继续消费) */
  onLegacyLine?: (text: string) => void;
  /** 测试/特殊场景注入自定义 store; 默认全局 presenceStore */
  store?: PresenceStore;
}

/**
 * 订阅 GET /v1/apeireth/events 并驱动 presenceStore.
 * - EventSource 实现 (该端点无需鉴权头, baseUrl 即全部入参);
 * - 自动重连: 出错即关闭并自建指数退避 (2s ×1.5, 封顶 30s, 连通后复位),
 *   取代 EventSource 内置的固定间隔重试;
 * - 页面 hidden 不断线 — 只停插值 rAF, 不关闭连接;
 * - 断连持续 >30s → store.setSimulated(true) (SIM 纪律).
 * 注意: 服务端 broadcast 容量 64、落后即丢 — 重连后不假设能补到断线期事件.
 * 返回取消订阅函数.
 */
export function subscribePresence(baseUrl: string, options: SubscribePresenceOptions = {}): () => void {
  const store = options.store ?? presenceStore;
  const url = `${baseUrl.replace(/\/+$/, '')}/v1/apeireth/events`;

  let active = true;
  let source: EventSource | null = null;
  let retryDelay = RETRY_BASE_MS;
  let retryTimer: ReturnType<typeof setTimeout> | null = null;
  let simTimer: ReturnType<typeof setTimeout> | null = null;

  function clearTimers(): void {
    if (retryTimer !== null) {
      clearTimeout(retryTimer);
      retryTimer = null;
    }
    if (simTimer !== null) {
      clearTimeout(simTimer);
      simTimer = null;
    }
  }

  function armSimTimer(): void {
    if (simTimer !== null) return;
    simTimer = setTimeout(() => {
      simTimer = null;
      if (active) store.setSimulated(true);
    }, SIM_AFTER_MS);
  }

  function connect(): void {
    if (!active) return;
    const es = new EventSource(url);
    source = es;

    es.onopen = () => {
      retryDelay = RETRY_BASE_MS;
      if (simTimer !== null) {
        clearTimeout(simTimer);
        simTimer = null;
      }
      store.setConnected(true);
    };

    es.onmessage = (msg) => {
      const data = typeof msg.data === 'string' ? msg.data : '';
      if (!data) return;
      const kind = store.ingestLine(data);
      if (kind === 'legacy' && options.onLegacyLine) {
        const text = data.trim();
        if (text) options.onLegacyLine(text);
      }
    };

    es.onerror = () => {
      if (source === es) source = null;
      es.close();
      if (!active) return;
      store.setConnected(false);
      armSimTimer();
      retryTimer = setTimeout(connect, retryDelay);
      retryDelay = Math.min(retryDelay * 1.5, RETRY_MAX_MS);
    };
  }

  connect();

  return () => {
    active = false;
    clearTimers();
    source?.close();
    source = null;
    store.setConnected(false);
  };
}
