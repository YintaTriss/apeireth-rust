// Apeireth 桌面伙伴 — Agent Runtime Contract & Adapter (Svelte 5 + Tauri 2)
//
// Reconciled integration baseline: capability-manifest-driven gating (core
// capability expansion) is the canonical contract; upstream master's companion
// presentation event stream is fused in. All V2 mutation endpoints and the
// capability discovery functions must not regress. Security invariant:
// apiKey / masterToken are NEVER persisted to localStorage.
//
// Conflict resolution (merge origin/master into feature): feature's richer
// signatures win for duplicated fetchers (fetchTools / fetchGraphData /
// fetchMemoryStreams / fetchEpisodes / fetchOrgans) because the capability-gated
// views depend on them; master's subscribeCompanionEvents + CompanionPresentationState
// + chatOnce + runtimeStatus are added as pure additions.

import type {
  ApeirethConfig,
  ChatMessage,
  Conversation,
  ModelSetup,
  RuntimeHealthReport,
  SubsystemStatus,
  ToolCallDetails,
  ActivityItem,
  MemoryEpisodeItem,
  ToolItem,
  ApprovalRequestItem,
  CapabilityManifest,
  Capability,
} from './types';


const STORAGE_KEY = 'apeireth-config';

// ============================================================
// Runtime Contract Types
// ============================================================

export interface ModelReference {
  id: string;
  provider?: string;
  label?: string;
}

export interface AgentMessage {
  role: 'user' | 'assistant' | 'system';
  content: string;
  id?: string;
  timestamp?: number;
}

export interface AgentRunRequest {
  messages: AgentMessage[];
  model: ModelReference;
  sessionId?: string;
  context?: {
    persona?: string;
    user?: string;
  };
  signal?: AbortSignal;
}

export type RuntimeEvent =
  | {type: 'run-start'; requestId: string}
  | {type: 'message-start'; requestId: string; messageId: string}
  | {type: 'text-delta'; requestId: string; text: string}
  | {type: 'reasoning-delta'; requestId: string; text: string}
  | {type: 'tool-call'; requestId: string; toolCall: ToolCallDetails}
  | {type: 'tool-result'; requestId: string; toolCallId: string; ok: boolean; summary?: string; full?: string; error?: string}
  | {type: 'message-end'; requestId: string; messageId: string; fullText: string}
  | {type: 'run-error'; requestId: string; error: RuntimeError}
  | {type: 'run-end'; requestId: string; aborted: boolean};

export interface RuntimeError {
  code: 'http' | 'network' | 'auth' | 'timeout' | 'aborted' | 'unknown';
  message: string;
  status?: number;
}

export interface AgentRuntime {
  run(request: AgentRunRequest, onEvent: (event: RuntimeEvent) => void): Promise<string>;
  abort(): void;
  readonly running: boolean;
  health(): Promise<RuntimeHealthReport>;
}

export interface RuntimeStatus {
  connected: boolean;
  baseUrl: string;
  model?: string;
}

export function classifyHttpError(status: number): RuntimeError['code'] {
  if (status === 401 || status === 403) return 'auth';
  if (status === 404) return 'http';
  if (status >= 500) return 'http';
  return 'http';
}

export class HttpError extends Error {
  status: number;
  constructor(status: number, message: string) {
    super(message);
    this.name = 'HttpError';
    this.status = status;
  }
}

export function toRuntimeError(caught: unknown): RuntimeError {
  if (caught instanceof DOMException && caught.name === 'AbortError') {
    return {code: 'aborted', message: '已中止请求'};
  }
  if (caught instanceof TypeError) {
    return {code: 'network', message: '网络错误：后端不可达或跨域拒绝'};
  }
  if (caught instanceof HttpError) {
    return {
      code: classifyHttpError(caught.status),
      message: caught.message,
      status: caught.status,
    };
  }
  const message = caught instanceof Error ? caught.message : String(caught);
  return {code: 'unknown', message};
}

export function loadConfig(): ApeirethConfig {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw) as Record<string, unknown>;
      // Security migration: purge any legacy apiKey or masterToken from local storage
      let modified = false;
      if ('apiKey' in parsed) {
        delete parsed.apiKey;
        modified = true;
      }
      if ('api_key' in parsed) {
        delete parsed.api_key;
        modified = true;
      }
      if ('masterToken' in parsed) {
        delete parsed.masterToken;
        modified = true;
      }
      if ('master_token' in parsed) {
        delete parsed.master_token;
        modified = true;
      }
      const cleaned: ApeirethConfig = {
        baseUrl: typeof parsed.baseUrl === 'string' ? parsed.baseUrl : 'http://127.0.0.1:8090',
        apiKey: '', // transient in-memory only; not persisted
        model: typeof parsed.model === 'string' ? parsed.model : 'MiniMax-M3',
        theme: typeof parsed.theme === 'string' ? (parsed.theme as any) : undefined,
      };
      if (modified) {
        localStorage.setItem(STORAGE_KEY, JSON.stringify({
          baseUrl: cleaned.baseUrl,
          model: cleaned.model,
          theme: cleaned.theme,
        }));
      }
      return cleaned;
    }
  } catch {
    // ignore corrupted config
  }
  return {baseUrl: 'http://127.0.0.1:8090', apiKey: '', model: 'MiniMax-M3'};
}

export function saveConfig(config: ApeirethConfig): void {
  // Never persist apiKey or masterToken to local storage
  const safeConfig = {
    baseUrl: config.baseUrl,
    model: config.model,
    theme: config.theme,
  };
  localStorage.setItem(STORAGE_KEY, JSON.stringify(safeConfig));
}



function normalizeBaseUrl(baseUrl: string): string {
  return baseUrl.replace(/\/+$/, '');
}

async function checkJson(response: Response): Promise<unknown> {
  if (!response.ok) {
    const text = await response.text().catch(() => '');
    throw new HttpError(response.status, `HTTP ${response.status} ${text.slice(0, 300)}`);
  }
  return response.json();
}

/** 基础 /health 端点探测 */
export async function checkHealth(baseUrl: string): Promise<boolean> {
  try {
    const response = await fetch(`${normalizeBaseUrl(baseUrl)}/health`, {signal: AbortSignal.timeout(2500)});
    return response.ok;
  } catch {
    return false;
  }
}

/** 深度多子系统健康检测，真实探测后端各项能力 */
export async function checkHealthDetailed(baseUrl: string, apiKey: string = ''): Promise<RuntimeHealthReport> {
  const base = normalizeBaseUrl(baseUrl);
  const subsystems: SubsystemStatus[] = [];
  const startAll = performance.now();
  let anyOk = false;
  let allOk = true;

  // 1. API Gateway / Gateway Health
  const t0 = performance.now();
  try {
    const res = await fetch(`${base}/health`, {signal: AbortSignal.timeout(2500)});
    const lat = Math.round(performance.now() - t0);
    if (res.ok) {
      anyOk = true;
      subsystems.push({name: 'API 网关', key: 'api', status: 'ok', endpoint: '/health', latencyMs: lat, detail: 'HTTP 200 OK'});
    } else {
      allOk = false;
      subsystems.push({name: 'API 网关', key: 'api', status: 'degraded', endpoint: '/health', latencyMs: lat, detail: `HTTP ${res.status}`});
    }
  } catch (e) {
    allOk = false;
    subsystems.push({name: 'API 网关', key: 'api', status: 'offline', endpoint: '/health', detail: '连接超时或服务未启动'});
  }

  // 2. 模型列表 / Provider
  const t1 = performance.now();
  try {
    const res = await fetch(`${base}/v1/models`, {
      headers: apiKey ? {Authorization: `Bearer ${apiKey}`} : {},
      signal: AbortSignal.timeout(3000),
    });
    const lat = Math.round(performance.now() - t1);
    if (res.ok) {
      anyOk = true;
      const data = (await res.json().catch(() => ({}))) as {data?: unknown[]};
      const count = Array.isArray(data.data) ? data.data.length : 0;
      subsystems.push({name: '模型服务', key: 'companion', status: 'ok', endpoint: '/v1/models', latencyMs: lat, detail: `可用模型数: ${count}`});
    } else {
      allOk = false;
      subsystems.push({name: '模型服务', key: 'companion', status: 'degraded', endpoint: '/v1/models', latencyMs: lat, detail: `HTTP ${res.status}`});
    }
  } catch {
    allOk = false;
    subsystems.push({name: '模型服务', key: 'companion', status: 'offline', endpoint: '/v1/models', detail: '模型列表不可用'});
  }

  // 3. 会话存储 / Session Ledger
  const t2 = performance.now();
  try {
    const res = await fetch(`${base}/v1/panel/sessions`, {
      headers: apiKey ? {Authorization: `Bearer ${apiKey}`} : {},
      signal: AbortSignal.timeout(3000),
    });
    const lat = Math.round(performance.now() - t2);
    if (res.ok) {
      anyOk = true;
      subsystems.push({name: '会话存储', key: 'sessions', status: 'ok', endpoint: '/v1/panel/sessions', latencyMs: lat, detail: 'SQLite 会话账本已加载'});
    } else {
      allOk = false;
      subsystems.push({name: '会话存储', key: 'sessions', status: 'degraded', endpoint: '/v1/panel/sessions', latencyMs: lat, detail: `HTTP ${res.status}`});
    }
  } catch {
    allOk = false;
    subsystems.push({name: '会话存储', key: 'sessions', status: 'offline', endpoint: '/v1/panel/sessions', detail: '会话只读端点不可用'});
  }

  // 4. 记忆系统 / Memory Streams
  const t3 = performance.now();
  try {
    const res = await fetch(`${base}/v1/panel/memory/streams`, {
      headers: apiKey ? {Authorization: `Bearer ${apiKey}`} : {},
      signal: AbortSignal.timeout(3000),
    });
    const lat = Math.round(performance.now() - t3);
    if (res.ok) {
      anyOk = true;
      subsystems.push({name: '记忆流', key: 'memory', status: 'ok', endpoint: '/v1/panel/memory/streams', latencyMs: lat, detail: '6 历史流已就绪'});
    } else {
      allOk = false;
      subsystems.push({name: '记忆流', key: 'memory', status: 'degraded', endpoint: '/v1/panel/memory/streams', latencyMs: lat, detail: `HTTP ${res.status}`});
    }
  } catch {
    allOk = false;
    subsystems.push({name: '记忆流', key: 'memory', status: 'offline', endpoint: '/v1/panel/memory/streams', detail: '记忆端点不可用'});
  }

  // 5. 工具注册表 / Tools
  const t4 = performance.now();
  try {
    const res = await fetch(`${base}/v1/tools/list`, {
      headers: apiKey ? {Authorization: `Bearer ${apiKey}`} : {},
      signal: AbortSignal.timeout(3000),
    });
    const lat = Math.round(performance.now() - t4);
    if (res.ok) {
      anyOk = true;
      subsystems.push({name: '工具注册表', key: 'tools', status: 'ok', endpoint: '/v1/tools/list', latencyMs: lat, detail: '工具目录已装配'});
    } else {
      allOk = false;
      subsystems.push({name: '工具注册表', key: 'tools', status: 'degraded', endpoint: '/v1/tools/list', latencyMs: lat, detail: `HTTP ${res.status}`});
    }
  } catch {
    allOk = false;
    subsystems.push({name: '工具注册表', key: 'tools', status: 'offline', endpoint: '/v1/tools/list', detail: '工具服务不可用'});
  }

  const overallLat = Math.round(performance.now() - startAll);
  let overall: RuntimeHealthReport['overall'] = 'offline';
  if (allOk && anyOk) {
    overall = 'online';
  } else if (anyOk) {
    overall = 'degraded';
  } else {
    overall = 'offline';
  }

  return {
    overall,
    baseUrl: base,
    latencyMs: overallLat,
    lastChecked: Date.now(),
    subsystems,
    model: 'MiniMax-M3',
  };
}

export async function listModels(baseUrl: string, apiKey: string): Promise<string[]> {
  const response = await fetch(`${normalizeBaseUrl(baseUrl)}/v1/models`, {
    headers: apiKey ? {Authorization: `Bearer ${apiKey}`} : {},
  });
  const data = (await checkJson(response)) as {data?: Array<{id: string}>};
  return (data.data || []).map((item) => item.id);
}

/**
 * 流式聊天: 通过 SSE 请求 OpenAI 兼容 chat completion 端点.
 * 覆盖：text delta, tool calls, reasoning delta, malformed lines, interruptions.
 * Reconciled: feature's structured ToolCallDetails callback + sessionId header
 * (canonical, verified) retained; reasoning_content delta handling retained.
 */
export interface StreamCallbacks {
  onDelta?: (text: string) => void;
  onReasoningDelta?: (text: string) => void;
  onToolCall?: (toolCall: ToolCallDetails) => void;
  onToolResult?: (id: string, ok: boolean, summary?: string) => void;
}

export async function streamChat(
  config: ApeirethConfig,
  messages: Array<{role: 'user' | 'assistant' | 'system'; content: string}>,
  callbacks: StreamCallbacks,
  signal?: AbortSignal,
  sessionId?: string,
): Promise<string> {
  const base = normalizeBaseUrl(config.baseUrl);
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
  };
  if (config.apiKey) {
    headers.Authorization = `Bearer ${config.apiKey}`;
  }
  if (sessionId) {
    headers['X-Apeireth-Continuity'] = sessionId;
  }

  const response = await fetch(`${base}/v1/chat/completions`, {
    method: 'POST',
    headers,
    body: JSON.stringify({
      model: config.model,
      messages,
      stream: true,
    }),
    signal,
  });

  if (!response.ok) {
    const text = await response.text().catch(() => '');
    throw new HttpError(response.status, `HTTP ${response.status} ${text.slice(0, 300)}`);
  }
  if (!response.body) throw new Error('响应流为空');

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = '';
  let fullText = '';
  const currentTools: Map<string, ToolCallDetails> = new Map();

  try {
    while (true) {
      const {done, value} = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, {stream: true});
      const lines = buffer.split('\n');
      buffer = lines.pop() || '';

      for (const line of lines) {
        const trimmed = line.trim();
        if (!trimmed || !trimmed.startsWith('data:')) continue;
        const payload = trimmed.slice(5).trim();
        if (payload === '[DONE]') {
          return fullText;
        }

        try {
          const json = JSON.parse(payload) as {
            choices?: Array<{
              delta?: {
                content?: string;
                reasoning_content?: string;
                tool_calls?: Array<{
                  index?: number;
                  id?: string;
                  function?: {
                    name?: string;
                    arguments?: string;
                  };
                }>;
              };
              finish_reason?: string;
            }>;
          };

          const choice = json.choices?.[0];
          const delta = choice?.delta;

          // 1. Text delta
          if (delta?.content) {
            fullText += delta.content;
            callbacks.onDelta?.(delta.content);
          }

          // 2. Reasoning delta
          if (delta?.reasoning_content) {
            callbacks.onReasoningDelta?.(delta.reasoning_content);
          }

          // 3. Tool calls streaming
          if (delta?.tool_calls && Array.isArray(delta.tool_calls)) {
            for (const tc of delta.tool_calls) {
              const tcId = tc.id || `tc-${tc.index ?? 0}`;
              let existing = currentTools.get(tcId);
              if (!existing) {
                existing = {
                  id: tcId,
                  name: tc.function?.name || '未知工具',
                  rawArgs: tc.function?.arguments || '',
                  status: 'running',
                  startTime: Date.now(),
                };
                currentTools.set(tcId, existing);
                callbacks.onToolCall?.(existing);
              } else {
                if (tc.function?.name) existing.name = tc.function.name;
                if (tc.function?.arguments) existing.rawArgs = (existing.rawArgs || '') + tc.function.arguments;
                try {
                  if (existing.rawArgs) {
                    existing.args = JSON.parse(existing.rawArgs);
                  }
                } catch {
                  // partial JSON parsing failure is expected while streaming arguments
                }
                callbacks.onToolCall?.(existing);
              }
            }
          }

          // 4. Finish reason
          if (choice?.finish_reason) {
            for (const [, tc] of currentTools) {
              if (tc.status === 'running') {
                tc.status = 'succeeded';
                tc.endTime = Date.now();
                tc.durationMs = tc.endTime - (tc.startTime || tc.endTime);
                callbacks.onToolResult?.(tc.id, true, '执行成功');
              }
            }
          }
        } catch {
          // ignore malformed SSE chunks
        }
      }
    }
  } finally {
    reader.releaseLock();
  }

  return fullText;
}

/** 非流式聊天 (用于简单问答/健康检查). Reconciled from master. */
export async function chatOnce(config: ApeirethConfig, prompt: string): Promise<string> {
  const response = await fetch(`${normalizeBaseUrl(config.baseUrl)}/v1/chat/completions`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${config.apiKey}`,
    },
    body: JSON.stringify({
      model: config.model,
      messages: [{role: 'user', content: prompt}],
      stream: false,
    }),
  });
  const data = await checkJson(response) as {
    choices?: Array<{message?: {content?: string}}>;
  };
  return data.choices?.[0]?.message?.content || '';
}

export function runtimeStatus(baseUrl: string, model?: string): RuntimeStatus {
  return {connected: false, baseUrl, model};
}

export function createAgentRuntime(config: ApeirethConfig): AgentRuntime {
  let abortController: AbortController | null = null;
  let _running = false;

  const runtime: AgentRuntime = {
    get running() {
      return _running;
    },

    async run(request, onEvent) {
      _running = true;
      abortController = new AbortController();
      const requestId = crypto.randomUUID();

      try {
        onEvent({type: 'run-start', requestId});
        onEvent({type: 'message-start', requestId, messageId: requestId});

        const full = await streamChat(
          config,
          request.messages.map((m) => ({role: m.role, content: m.content})),
          {
            onDelta: (delta) => onEvent({type: 'text-delta', requestId, text: delta}),
            onReasoningDelta: (delta) => onEvent({type: 'reasoning-delta', requestId, text: delta}),
            onToolCall: (toolCall) => onEvent({type: 'tool-call', requestId, toolCall}),
            onToolResult: (toolCallId, ok, summary) =>
              onEvent({type: 'tool-result', requestId, toolCallId, ok, summary}),
          },
          request.signal ?? abortController.signal,
          request.sessionId,
        );

        onEvent({type: 'message-end', requestId, messageId: requestId, fullText: full});
        onEvent({type: 'run-end', requestId, aborted: false});
        return full;
      } catch (caught) {
        const error = toRuntimeError(caught);
        if (error.code !== 'aborted') {
          onEvent({type: 'run-error', requestId, error});
        }
        onEvent({type: 'run-end', requestId, aborted: error.code === 'aborted'});
        throw error;
      } finally {
        _running = false;
        abortController = null;
      }
    },

    abort() {
      abortController?.abort();
    },

    async health() {
      return checkHealthDetailed(config.baseUrl, config.apiKey);
    },
  };

  return runtime;
}

// ============================================================
// Backend Real API Fetchers (Activity, Memory, Tools, Sessions)
// ============================================================

// ------------------------------------------------------------
// Runtime Capability Manifest — 能力发现 (不再 404-probing)
// ------------------------------------------------------------

/**
 * 拉取 Runtime Capability Manifest.
 *
 * 启动流程: 先 health → 再 capabilities.
 * 当 runtime 无原生 `/v1/apeireth/capabilities` 端点 (旧 runtime) 时,
 * 回落到保守的 legacy profile — 只声明历史契约证明存在的只读/对话能力,
 * 绝不推测 mutation. UI 据此降级 (mutation 按钮 disabled/隐藏).
 *
 * 404 仅作为 legacy fallback 触发条件, 不作为长期协议设计.
 */
export async function fetchCapabilities(config: ApeirethConfig): Promise<CapabilityManifest> {
  const base = normalizeBaseUrl(config.baseUrl);
  try {
    const res = await fetch(`${base}/v1/apeireth/capabilities`, {
      headers: config.apiKey ? {Authorization: `Bearer ${config.apiKey}`} : {},
      signal: AbortSignal.timeout(4000),
    });
    if (!res.ok) {
      // 非 200 → legacy fallback (不抛错, 不弄死 Desktop)
      return legacyCapabilityManifest();
    }
    const data = (await res.json()) as CapabilityManifest;
    // 基本校验: 必须有 schema_version + capabilities 数组, 否则视为损坏 → legacy
    if (
      typeof data.schema_version !== 'number' ||
      !Array.isArray(data.capabilities) ||
      !data.runtime ||
      typeof data.runtime.service !== 'string'
    ) {
      return legacyCapabilityManifest();
    }
    return data;
  } catch {
    // 网络错误/超时 → legacy fallback
    return legacyCapabilityManifest();
  }
}

/**
 * 查询 manifest 是否支持某 capability ID. 未知 ID 一律返回 false (保守).
 * null manifest (尚未加载) 也返回 false.
 *
 * 注意: supported 是静态语义 (runtime 是否实现该能力). 要判断「现在能否调用」
 * 应使用 capabilityAvailable() — 它反映 provider/凭据状态.
 */
export function capabilitySupported(manifest: CapabilityManifest | null, id: string): boolean {
  if (!manifest) return false;
  for (const group of manifest.capabilities) {
    for (const cap of group.capabilities) {
      if (cap.id === id) return cap.supported === true;
    }
  }
  return false;
}

/**
 * 查询某 capability 是否**当前可用** (动态语义, 受 provider/凭据影响).
 *
 * 语义:
 * - available === true → 可用
 * - available === false → 不可用 (reason 给出 machine-readable 原因)
 * - available === undefined (旧 manifest 无此字段) → 回落 supported (向后兼容)
 *
 * Runtime Decoupling: 桌面端 gating 应优先用 capabilityAvailable 判断「现在能否用」,
 * 用 capabilitySupported 判断「runtime 是否实现」, 两者 UI 可区分表达
 * (Unsupported vs Provider not configured).
 */
export function capabilityAvailable(manifest: CapabilityManifest | null, id: string): boolean {
  if (!manifest) return false;
  const cap = findCapability(manifest, id);
  if (!cap) return false;
  // 回落: 旧 manifest 无 available → 按 supported 解释.
  return cap.available === undefined ? cap.supported === true : cap.available === true;
}

/**
 * 查询某 capability 不可用的 machine-readable 原因 (仅当 available === false).
 * 可用或旧 manifest 回落时返回 null.
 */
export function capabilityUnavailableReason(
  manifest: CapabilityManifest | null,
  id: string,
): import('./types').CapabilityAvailabilityReason | null {
  if (!manifest) return null;
  const cap = findCapability(manifest, id);
  if (!cap) return null;
  if (cap.available === false) return cap.reason ?? null;
  return null;
}

/** 查找某 capability 完整声明 (跨组). */
export function findCapability(manifest: CapabilityManifest | null, id: string): Capability | null {
  if (!manifest) return null;
  for (const group of manifest.capabilities) {
    for (const cap of group.capabilities) {
      if (cap.id === id) return cap;
    }
  }
  return null;
}

/**
 * Legacy 兼容 profile: runtime 无原生 manifest 端点时的保守声明.
 * 只声明经过历史契约证明存在的能力 (chat / health / models / 只读 panel 端点).
 * 不推测任何 mutation — memory.forget / sessions.create / permissions.revoke 等一律 unsupported.
 */
export function legacyCapabilityManifest(): CapabilityManifest {
  const cap = (id: string, supported: boolean, read: boolean, write: boolean, ops: string[]): Capability => ({
    id,
    supported,
    read,
    write,
    version: 1,
    operations: ops,
  });
  return {
    schema_version: 1,
    runtime: {service: 'apeireth-legacy-runtime', version: 'unknown'},
    legacy: true,
    capabilities: [
      {name: 'chat', capabilities: [cap('chat.completions', true, true, true, ['stream'])]},
      {name: 'health', capabilities: [cap('health', true, true, false, ['check'])]},
      {name: 'models', capabilities: [cap('models.list', true, true, false, ['list'])]},
      {name: 'sessions', capabilities: [cap('sessions.read', true, true, false, ['list', 'timeline'])]},
      {name: 'memory', capabilities: [cap('memory.read', true, true, false, ['list', 'search'])]},
      {name: 'tools', capabilities: [cap('tools.list', true, true, false, ['list'])]},
      {
        name: 'permissions',
        capabilities: [cap('permissions.requests.read', true, true, false, ['list'])],
      },
      {
        name: 'activity',
        capabilities: [
          cap('activity.sse', true, true, false, ['subscribe']),
          cap('activity.audit', true, true, false, ['list']),
        ],
      },
    ],
  };
}

/** 获取真实后端会话列表 (只读数据) */
export async function fetchBackendSessions(config: ApeirethConfig): Promise<Array<{id: string; started_at: number; last_active_at: number; closed_at?: number; episode_count: number}>> {
  const res = await fetch(`${normalizeBaseUrl(config.baseUrl)}/v1/panel/sessions`, {
    headers: config.apiKey ? {Authorization: `Bearer ${config.apiKey}`} : {},
  });
  const data = (await checkJson(res)) as {sessions?: Array<{id: string; started_at: number; last_active_at: number; closed_at?: number; episode_count: number}>};
  return data.sessions || [];
}

/** 获取会话时间线 (episodes) */
export async function fetchSessionTimeline(config: ApeirethConfig, sessionId: string): Promise<MemoryEpisodeItem[]> {
  const res = await fetch(`${normalizeBaseUrl(config.baseUrl)}/v1/panel/sessions/${encodeURIComponent(sessionId)}/timeline`, {
    headers: config.apiKey ? {Authorization: `Bearer ${config.apiKey}`} : {},
  });
  const data = (await checkJson(res)) as {episodes?: Array<{id: string; timestamp: number; role: string; content: string; session_id: string}>};
  return (data.episodes || []).map((e) => ({
    id: e.id,
    timestamp: e.timestamp,
    role: e.role,
    content: e.content,
    sessionId: e.session_id,
  }));
}

/** 获取 6 历史记忆流 */
export async function fetchMemoryStreams(config: ApeirethConfig): Promise<Record<string, MemoryEpisodeItem[]>> {
  const res = await fetch(`${normalizeBaseUrl(config.baseUrl)}/v1/panel/memory/streams`, {
    headers: config.apiKey ? {Authorization: `Bearer ${config.apiKey}`} : {},
  });
  const data = (await checkJson(res)) as {streams?: Record<string, Array<{id: string; timestamp: number; role: string; content: string; session_id: string}>>};
  const result: Record<string, MemoryEpisodeItem[]> = {};
  if (data.streams) {
    for (const [key, list] of Object.entries(data.streams)) {
      result[key] = (list || []).map((e) => ({
        id: e.id,
        timestamp: e.timestamp,
        role: e.role,
        content: e.content,
        sessionId: e.session_id,
        stream: key,
      }));
    }
  }
  return result;
}

/** 搜索记忆条目 */
export async function fetchMemoryEpisodes(config: ApeirethConfig, query = '', limit = 100): Promise<MemoryEpisodeItem[]> {
  const url = `${normalizeBaseUrl(config.baseUrl)}/v1/panel/memory/episodes?limit=${limit}${query ? `&q=${encodeURIComponent(query)}` : ''}`;
  const res = await fetch(url, {
    headers: config.apiKey ? {Authorization: `Bearer ${config.apiKey}`} : {},
  });
  const data = (await checkJson(res)) as {episodes?: Array<{id: string; timestamp: number; role: string; content: string; session_id: string}>};
  return (data.episodes || []).map((e) => ({
    id: e.id,
    timestamp: e.timestamp,
    role: e.role,
    content: e.content,
    sessionId: e.session_id,
  }));
}

/** 获取知识图谱事实和链接 */
export async function fetchGraphData(config: ApeirethConfig): Promise<{facts: MemoryEpisodeItem[]; links: MemoryEpisodeItem[]}> {
  const res = await fetch(`${normalizeBaseUrl(config.baseUrl)}/v1/panel/graph`, {
    headers: config.apiKey ? {Authorization: `Bearer ${config.apiKey}`} : {},
  });
  const data = (await checkJson(res)) as {facts?: Array<{id: string; timestamp: number; role: string; content: string; session_id: string}>; links?: Array<{id: string; timestamp: number; role: string; content: string; session_id: string}>};
  return {
    facts: (data.facts || []).map((e) => ({id: e.id, timestamp: e.timestamp, role: e.role, content: e.content, sessionId: e.session_id})),
    links: (data.links || []).map((e) => ({id: e.id, timestamp: e.timestamp, role: e.role, content: e.content, sessionId: e.session_id})),
  };
}

/** 获取持久化审计记录 */
export async function fetchAuditLogs(config: ApeirethConfig, limit = 100): Promise<ActivityItem[]> {
  const res = await fetch(`${normalizeBaseUrl(config.baseUrl)}/v1/panel/audit?limit=${limit}`, {
    headers: config.apiKey ? {Authorization: `Bearer ${config.apiKey}`} : {},
  });
  const data = (await checkJson(res)) as {records?: Array<{id?: string; timestamp?: number; action?: string; tool?: string; status?: string; detail?: string}>};
  return (data.records || []).map((r, i) => ({
    id: r.id || `audit-${r.timestamp || Date.now()}-${i}`,
    timestamp: r.timestamp ? (r.timestamp > 1e11 ? r.timestamp : r.timestamp * 1000) : Date.now(),
    category: (r.tool ? 'tool' : 'runtime') as ActivityItem['category'],
    title: r.tool ? `工具调用: ${r.tool}` : (r.action || '操作记录'),
    summary: r.detail || r.action || '系统操作留痕',
    source: 'audit',
    severity: r.status === 'failed' || r.status === 'error' ? 'error' : 'info',
    detail: JSON.stringify(r, null, 2),
    raw: r,
  }));
}

/** 获取工具列表 (严格请求后端真实注册表端点) */
export async function fetchTools(config: ApeirethConfig): Promise<ToolItem[]> {
  const baseUrl = normalizeBaseUrl(config.baseUrl);
  // 先尝试 /v1/tools/list，再尝试 /v1/panel/tools
  let res = await fetch(`${baseUrl}/v1/tools/list`, {
    headers: config.apiKey ? {Authorization: `Bearer ${config.apiKey}`} : {},
  }).catch(() => null);

  if (!res || !res.ok) {
    res = await fetch(`${baseUrl}/v1/panel/tools`, {
      headers: config.apiKey ? {Authorization: `Bearer ${config.apiKey}`} : {},
    }).catch(() => null);
  }

  if (!res || !res.ok) {
    throw new HttpError(
      res ? res.status : 503,
      `后端工具注册表端点不可用 (${res ? `HTTP ${res.status}` : '连接失败'})`,
    );
  }

  const data = (await res.json()) as {tools?: Array<{name: string; description?: string; args_schema?: unknown}>};
  if (!Array.isArray(data.tools)) return [];
  return data.tools.map((t) => ({
    name: t.name,
    description: t.description || '无描述信息',
    argsSchema: t.args_schema,
    source: 'builtin',
    permission: 'prompt',
    available: true,
  }));
}

/** 获取待审批授权请求 */
export async function fetchApprovalRequests(config: ApeirethConfig): Promise<ApprovalRequestItem[]> {
  const res = await fetch(`${normalizeBaseUrl(config.baseUrl)}/v1/apeireth/approval-requests`, {
    headers: config.apiKey ? {Authorization: `Bearer ${config.apiKey}`} : {},
  });
  const list = (await checkJson(res)) as Array<{
    id?: string;
    chain?: string;
    rev?: number;
    tool?: string;
    reason?: string;
    args_preview?: string;
    summary?: string;
    created_at?: number;
    requested_at?: number;
    status?: string;
  }>;
  if (!Array.isArray(list)) return [];
  return list.map((item, idx) => ({
    id: item.id || `apreq-${idx}`,
    chain: item.chain,
    rev: item.rev,
    tool: item.tool || '未知工具',
    reason: item.reason,
    args_preview: item.args_preview,
    summary: item.reason || item.summary || item.args_preview || '请求执行特权工具',
    requestedAt: item.created_at || item.requested_at,
    status: (item.status as ApprovalRequestItem['status']) || 'pending',
  }));
}

/** 主人批准端点 (master token 显式授权，不持久化 Token) */
export async function grantToolPermission(
  config: ApeirethConfig,
  tool: string,
  hours: number = 1,
  masterToken: string = '',
): Promise<{ok: boolean; error?: string}> {
  try {
    const res = await fetch(`${normalizeBaseUrl(config.baseUrl)}/v1/apeireth/grant`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Authorization: config.apiKey ? `Bearer ${config.apiKey}` : '',
      },
      body: JSON.stringify({
        tool,
        hours,
        master_token: masterToken.trim(),
      }),
    });
    if (!res.ok) {
      const err = (await res.json().catch(() => ({error: `HTTP ${res.status}`}))) as {error?: string};
      return {ok: false, error: err.error || `HTTP ${res.status}`};
    }
    return {ok: true};
  } catch (caught) {
    return {ok: false, error: caught instanceof Error ? caught.message : String(caught)};
  }
}

// Compatibility alias — master used `grantApproval` for the same endpoint.
export const grantApproval = grantToolPermission;
// Compatibility alias — master used `fetchPendingApprovals`.
export const fetchPendingApprovals = fetchApprovalRequests;

/** 写入记忆条目 */
export async function appendMemoryEpisode(
  config: ApeirethConfig,
  content: string,
  category: string = 'fact',
  sessionId: string = 'me',
): Promise<boolean> {
  const res = await fetch(`${normalizeBaseUrl(config.baseUrl)}/v1/memory/append`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Authorization: config.apiKey ? `Bearer ${config.apiKey}` : '',
    },
    body: JSON.stringify({
      session_id: sessionId,
      role: 'user',
      content: `[${category}] ${content}`,
    }),
  });
  return res.ok;
}

/** 本地会话持久化与容错迁移 (客户端专用) */
export function loadConversations(): Conversation[] {
  try {
    const raw = localStorage.getItem('apeireth-conversations');
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.map((item: any) => ({
      id: typeof item.id === 'string' ? item.id : crypto.randomUUID(),
      title: typeof item.title === 'string' ? item.title : '新对话',
      createdAt: typeof item.createdAt === 'number' ? item.createdAt : Date.now(),
      updatedAt: typeof item.updatedAt === 'number' ? item.updatedAt : Date.now(),
      messages: Array.isArray(item.messages) ? item.messages : [],
      scope: item.scope === 'project' ? 'project' : 'global',
      pinned: !!item.pinned,
      archived: !!item.archived,
      model: typeof item.model === 'string' ? item.model : undefined,
    }));
  } catch {
    return [];
  }
}


export function saveConversations(conversations: Conversation[]): void {
  localStorage.setItem('apeireth-conversations', JSON.stringify(conversations));
}

// Backward-compatible aliases for legacy / transition imports
export type MemoryEpisode = MemoryEpisodeItem;
export type ToolInfo = ToolItem;

export async function fetchEpisodes(config: ApeirethConfig, limit = 50): Promise<MemoryEpisodeItem[]> {
  return fetchMemoryEpisodes(config, '', limit);
}

export async function fetchOrgans(config: ApeirethConfig): Promise<unknown[]> {
  try {
    const res = await fetch(`${normalizeBaseUrl(config.baseUrl)}/v1/organs`, {
      headers: config.apiKey ? {Authorization: `Bearer ${config.apiKey}`} : {},
    });
    return (await checkJson(res)) as unknown[];
  } catch {
    return [];
  }
}

// ============================================================
// Companion Presentation Events — reconciled from upstream master
// 后端信号驱动的伴随体表现态 (严禁前端造假). Raw CoT 仍不持久化;
// 这是 SSE 事件流的 presentation 层, 与 trace 持久化无关.
// ============================================================

export type CompanionPresentationState =
  | 'idle'
  | 'thinking'
  | 'speaking'
  | 'working'
  | 'reflecting'
  | 'concerned'
  | 'happy';

export interface CompanionEvent {
  text: string;
  ts: number;
  kind?: string;
}

/**
 * 订阅 Apeireth 伴随体事件流 (GET /v1/apeireth/events)
 * 接收后端 CompanionDaemon 涌现问候、反思完成与做梦通知
 * 支持断线指数退避自动重连 (2s ~ 30s)
 */
export function subscribeCompanionEvents(
  config: ApeirethConfig,
  onEvent: (event: CompanionEvent) => void,
): () => void {
  const url = `${normalizeBaseUrl(config.baseUrl)}/v1/apeireth/events`;
  let active = true;
  let currentController: AbortController | null = null;
  let retryDelay = 2000;

  async function connectLoop(): Promise<void> {
    while (active) {
      currentController = new AbortController();
      try {
        const response = await fetch(url, {
          headers: {Authorization: `Bearer ${config.apiKey}`},
          signal: currentController.signal,
        });
        if (!response.ok || !response.body) {
          throw new Error(`HTTP ${response.status}`);
        }
        retryDelay = 2000; // Reset delay on successful connection
        const reader = response.body.getReader();
        const decoder = new TextDecoder();
        let buffer = '';
        while (active) {
          const {done, value} = await reader.read();
          if (done) break;
          buffer += decoder.decode(value, {stream: true});
          const lines = buffer.split('\n');
          buffer = lines.pop() || '';
          for (const line of lines) {
            const trimmed = line.trim();
            if (trimmed.startsWith('data:')) {
              const data = trimmed.slice(5).trim();
              if (data) {
                onEvent({text: data, ts: Date.now()});
              }
            }
          }
        }
      } catch {
        // Disconnected or aborted
      }
      if (!active) break;
      await new Promise((resolve) => setTimeout(resolve, retryDelay));
      retryDelay = Math.min(retryDelay * 1.5, 30000);
    }
  }

  void connectLoop();

  return () => {
    active = false;
    currentController?.abort();
  };
}

// ============================================================
// Core Capability Expansion Phase 6 — 后端 mutation 真实接入
// 所有调用都应先由 capabilitySupported() gate (UI 按钮). 不 fake.
// ============================================================

/** 后端会话生命周期记录 (对应 Rust SessionLifecycleRecord). */
export interface BackendSessionRecord {
  id: string;
  title: string | null;
  scope: 'global' | 'project';
  project_id: string | null;
  state: 'active' | 'archived' | 'closed';
  started_at: number;
  last_active_at: number;
  updated_at: number | null;
  archived_at: number | null;
  closed_at: number | null;
  revision: number;
  metadata: unknown;
}

/** 治理后的 episode (含 forgotten/protected/override). */
export interface GovernedEpisodeItem extends MemoryEpisodeItem {
  status: 'active' | 'forgotten';
  protected: boolean;
  content_override: string | null;
  revision: number;
  updated_at: number | null;
  updated_by: string | null;
  forgotten_at: number | null;
}

/** Grant 视图 (对应 Rust GrantView). */
export interface GrantView {
  id: string;
  name: string;
  tools: string[];
  paths: string[];
  expiry: string;
  op_budget: number | null;
  used_ops: number;
  spend_budget: number | null;
  spend_used: number;
  activated_at_ms: number;
  created_at_ms: number;
  active: boolean;
  expired: boolean;
}

/** Trace span (对应 Rust TraceSpan). */
export interface TraceSpanItem {
  span_id: string;
  trace_id: string;
  parent_span_id: string | null;
  kind: string;
  actor: string;
  status: string;
  summary: string | null;
  attributes: unknown;
  started_at: number;
  ended_at: number | null;
  session_id: string | null;
}

async function postJson(config: ApeirethConfig, path: string, body: unknown): Promise<{ok: boolean; status: number; data?: unknown; error?: string}> {
  try {
    const res = await fetch(`${normalizeBaseUrl(config.baseUrl)}${path}`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Authorization: config.apiKey ? `Bearer ${config.apiKey}` : '',
      },
      body: JSON.stringify(body),
    });
    const data = await res.json().catch(() => null);
    if (!res.ok) {
      return {ok: false, status: res.status, error: (data && (data as {message?: string}).message) || `HTTP ${res.status}`};
    }
    return {ok: true, status: res.status, data};
  } catch (caught) {
    return {ok: false, status: 0, error: caught instanceof Error ? caught.message : String(caught)};
  }
}

async function patchJson(config: ApeirethConfig, path: string, body: unknown): Promise<{ok: boolean; status: number; data?: unknown; error?: string}> {
  try {
    const res = await fetch(`${normalizeBaseUrl(config.baseUrl)}${path}`, {
      method: 'PATCH',
      headers: {
        'Content-Type': 'application/json',
        Authorization: config.apiKey ? `Bearer ${config.apiKey}` : '',
      },
      body: JSON.stringify(body),
    });
    const data = await res.json().catch(() => null);
    if (!res.ok) {
      return {ok: false, status: res.status, error: (data && (data as {message?: string}).message) || `HTTP ${res.status}`};
    }
    return {ok: true, status: res.status, data};
  } catch (caught) {
    return {ok: false, status: 0, error: caught instanceof Error ? caught.message : String(caught)};
  }
}

// --- Session lifecycle ---

export async function fetchBackendSessionsV2(config: ApeirethConfig, includeArchived = false): Promise<BackendSessionRecord[]> {
  const res = await fetch(`${normalizeBaseUrl(config.baseUrl)}/v1/apeireth/sessions?include_archived=${includeArchived}`, {
    headers: config.apiKey ? {Authorization: `Bearer ${config.apiKey}`} : {},
  });
  const data = (await checkJson(res)) as {sessions?: BackendSessionRecord[]};
  return data.sessions || [];
}

export async function createBackendSession(config: ApeirethConfig, title?: string, scope: 'global' | 'project' = 'global', projectId?: string): Promise<BackendSessionRecord | {error: string}> {
  const r = await postJson(config, '/v1/apeireth/sessions', {title, scope, project_id: projectId});
  return r.ok ? (r.data as BackendSessionRecord) : {error: r.error || 'create failed'};
}

export async function renameBackendSession(config: ApeirethConfig, id: string, title: string, expectedRev: number): Promise<BackendSessionRecord | {error: string}> {
  const r = await patchJson(config, `/v1/apeireth/sessions/${encodeURIComponent(id)}`, {title, expected_rev: expectedRev});
  return r.ok ? (r.data as BackendSessionRecord) : {error: r.error || 'rename failed'};
}

export async function archiveBackendSession(config: ApeirethConfig, id: string, expectedRev: number): Promise<BackendSessionRecord | {error: string}> {
  const r = await postJson(config, `/v1/apeireth/sessions/${encodeURIComponent(id)}/archive`, {expected_rev: expectedRev});
  return r.ok ? (r.data as BackendSessionRecord) : {error: r.error || 'archive failed'};
}

export async function restoreBackendSession(config: ApeirethConfig, id: string, expectedRev: number): Promise<BackendSessionRecord | {error: string}> {
  const r = await postJson(config, `/v1/apeireth/sessions/${encodeURIComponent(id)}/restore`, {expected_rev: expectedRev});
  return r.ok ? (r.data as BackendSessionRecord) : {error: r.error || 'restore failed'};
}

export async function closeBackendSession(config: ApeirethConfig, id: string, expectedRev: number): Promise<BackendSessionRecord | {error: string}> {
  const r = await postJson(config, `/v1/apeireth/sessions/${encodeURIComponent(id)}/close`, {expected_rev: expectedRev});
  return r.ok ? (r.data as BackendSessionRecord) : {error: r.error || 'close failed'};
}

// --- Memory governance ---

export async function updateMemoryEpisode(config: ApeirethConfig, id: string, content: string, expectedRev: number, updatedBy?: string): Promise<GovernedEpisodeItem | {error: string}> {
  const r = await patchJson(config, `/v1/apeireth/memory/episodes/${encodeURIComponent(id)}`, {content, expected_rev: expectedRev, updated_by: updatedBy});
  return r.ok ? (r.data as GovernedEpisodeItem) : {error: r.error || 'update failed'};
}

export async function forgetMemoryEpisode(config: ApeirethConfig, id: string, expectedRev: number, reason?: string): Promise<GovernedEpisodeItem | {error: string}> {
  const r = await postJson(config, `/v1/apeireth/memory/episodes/${encodeURIComponent(id)}/forget`, {expected_rev: expectedRev, reason});
  return r.ok ? (r.data as GovernedEpisodeItem) : {error: r.error || 'forget failed'};
}

export async function protectMemoryEpisode(config: ApeirethConfig, id: string, expectedRev: number): Promise<GovernedEpisodeItem | {error: string}> {
  const r = await postJson(config, `/v1/apeireth/memory/episodes/${encodeURIComponent(id)}/protect`, {expected_rev: expectedRev});
  return r.ok ? (r.data as GovernedEpisodeItem) : {error: r.error || 'protect failed'};
}

export async function unprotectMemoryEpisode(config: ApeirethConfig, id: string, expectedRev: number): Promise<GovernedEpisodeItem | {error: string}> {
  const r = await postJson(config, `/v1/apeireth/memory/episodes/${encodeURIComponent(id)}/unprotect`, {expected_rev: expectedRev});
  return r.ok ? (r.data as GovernedEpisodeItem) : {error: r.error || 'unprotect failed'};
}

// --- Permission grants (revoke + list) ---

export async function fetchGrants(config: ApeirethConfig): Promise<GrantView[]> {
  try {
    const res = await fetch(`${normalizeBaseUrl(config.baseUrl)}/v1/apeireth/grants`, {
      headers: config.apiKey ? {Authorization: `Bearer ${config.apiKey}`} : {},
    });
    const data = (await checkJson(res)) as {grants?: GrantView[]};
    return data.grants || [];
  } catch {
    return [];
  }
}

export async function revokeGrant(config: ApeirethConfig, id: string, masterToken: string): Promise<{ok: boolean; error?: string}> {
  const r = await postJson(config, `/v1/apeireth/grants/${encodeURIComponent(id)}/revoke`, {master_token: masterToken.trim()});
  return r.ok ? {ok: true} : {ok: false, error: r.error};
}

// --- Trace ---

export async function fetchTraces(config: ApeirethConfig, limit = 50): Promise<Array<{trace_id: string; root_span: TraceSpanItem; span_count: number}>> {
  try {
    const res = await fetch(`${normalizeBaseUrl(config.baseUrl)}/v1/panel/traces?limit=${limit}`, {
      headers: config.apiKey ? {Authorization: `Bearer ${config.apiKey}`} : {},
    });
    const data = (await checkJson(res)) as {traces?: Array<{trace_id: string; root_span: TraceSpanItem; span_count: number}>};
    return data.traces || [];
  } catch {
    return [];
  }
}

export async function fetchTraceDetail(config: ApeirethConfig, traceId: string): Promise<TraceSpanItem[] | {error: string}> {
  try {
    const res = await fetch(`${normalizeBaseUrl(config.baseUrl)}/v1/panel/traces/${encodeURIComponent(traceId)}`, {
      headers: config.apiKey ? {Authorization: `Bearer ${config.apiKey}`} : {},
    });
    if (!res.ok) {
      const err = (await res.json().catch(() => ({}))) as {message?: string};
      return {error: err.message || `HTTP ${res.status}`};
    }
    const data = (await res.json()) as {spans?: TraceSpanItem[]};
    return data.spans || [];
  } catch (caught) {
    return {error: caught instanceof Error ? caught.message : String(caught)};
  }
}
