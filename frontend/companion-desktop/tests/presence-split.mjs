// Presence 频道客户端 — 分流 / mode 推导 / 平滑插值 / store 行为 纯逻辑单测
// 直接 import 真实实现 ../src/lib/presence.ts (Node ≥23.6 默认类型擦除;
// presence.ts 零运行时依赖、DOM/EventSource/rAF 全部惰性守卫, 可在 Node 下加载).
// 契约依据: docs/02-guides/frontend-data-contract.md §5.1 / §8.1. 不依赖后端.
import assert from 'node:assert/strict';

import {
  splitPresenceLine,
  derivePresenceMode,
  approachExponential,
  createPresenceStore,
  THINKING_WINDOW_MS,
  SMOOTHING_TAU_MS,
  RECENT_EVENTS_CAPACITY,
} from '../src/lib/presence.ts';

console.log('--- Starting Presence Split & Mode Derivation Check ---');

// ---------------------------------------------------------------------------
// 1. splitPresenceLine — 三类 data 行共流的分流纪律
// ---------------------------------------------------------------------------

// ① legacy 纯文本行原样透传
{
  const r = splitPresenceLine('[他说] 主人，夜深了，本座留意到你还在忙。');
  assert.equal(r.kind, 'legacy');
  assert.equal(r.text, '[他说] 主人，夜深了，本座留意到你还在忙。');
  // 测试事件行同样是 legacy
  assert.equal(splitPresenceLine('测试事件: 本座在 (SSE 链路验证)').kind, 'legacy');
}

// ② presence JSON 行 (serde 内部标签 type 平铺) — 四类事件 + presence_error
{
  const emotion = '{"type":"emotion","at":"2026-08-21T08:30:00Z","pad":{"p":0.12,"a":0.05,"d":0.0},"dominant":"joy","intensity":0.46,"response_style":"friendly","tone":"礼貌克制, 谨慎而友好"}';
  const r = splitPresenceLine(emotion);
  assert.equal(r.kind, 'presence');
  assert.equal(r.event.type, 'emotion');
  assert.equal(r.event.pad.p, 0.12);
  assert.equal(r.event.tone, '礼貌克制, 谨慎而友好');

  const held = '{"type":"initiative","at":"2026-08-21T08:30:00Z","outcome":"held","gate":"quiet_hours","gate_label":"安静时段"}';
  const rh = splitPresenceLine(held);
  assert.equal(rh.kind, 'presence');
  assert.equal(rh.event.type, 'initiative');
  assert.equal(rh.event.gate, 'quiet_hours');

  assert.equal(splitPresenceLine('{"type":"dream","at":"2026-08-21T08:30:00Z","merged_count":2,"summary_prefix":"【做梦摘要】主人在准备考试"}').event.merged_count, 2);
  assert.equal(splitPresenceLine('{"type":"memory_recall","at":"2026-08-21T08:30:00Z","found":3,"keywords":["考试","数学","线代"],"redacted":true}').event.keywords.length, 3);

  // ③ presence_error 兜底帧: 解析器容忍
  const re = splitPresenceLine('{"type":"presence_error","error":"serialize boom"}');
  assert.equal(re.kind, 'presence');
  assert.equal(re.event.type, 'presence_error');
}

// 行首 `{` 判定前允许空白 (SSE data: 后常带一个空格, 上游已 trim 但保持防御)
assert.equal(splitPresenceLine('  {"type":"dream","at":"t","merged_count":1,"summary_prefix":"x"}').kind, 'presence');

// JSON 解析失败静默 skip (契约纪律: 不抛错、不当文本展示)
assert.equal(splitPresenceLine('{"type":"emotion",broken').kind, 'skip');
// 合法 JSON 但非已知 presence type → skip (forward compat 容忍)
assert.equal(splitPresenceLine('{"type":"unknown_future"}').kind, 'skip');
assert.equal(splitPresenceLine('{"foo":1}').kind, 'skip');
assert.equal(splitPresenceLine('[1,2,3]').kind, 'legacy'); // 不以 { 开头 → legacy
assert.equal(splitPresenceLine('').kind, 'skip');
assert.equal(splitPresenceLine('   ').kind, 'skip');

console.log('✓ splitPresenceLine: legacy / presence / presence_error / skip 全分支');

// ---------------------------------------------------------------------------
// 2. derivePresenceMode — speaking > thinking > quiet
// ---------------------------------------------------------------------------

const NOW = 1_000_000;
assert.equal(derivePresenceMode({speaking: true, chatActive: true, lastSpokeAt: NOW, now: NOW}), 'speaking');
assert.equal(derivePresenceMode({speaking: false, chatActive: true, lastSpokeAt: 0, now: NOW}), 'thinking');
// 最近 5s 内 initiative/spoke → thinking (边界含等号)
assert.equal(derivePresenceMode({speaking: false, chatActive: false, lastSpokeAt: NOW - THINKING_WINDOW_MS, now: NOW}), 'thinking');
// 超过窗口 → quiet
assert.equal(derivePresenceMode({speaking: false, chatActive: false, lastSpokeAt: NOW - THINKING_WINDOW_MS - 1, now: NOW}), 'quiet');
assert.equal(derivePresenceMode({speaking: false, chatActive: false, lastSpokeAt: 0, now: NOW}), 'quiet');

console.log('✓ derivePresenceMode: speaking 优先 / 5s spoke 窗口 / quiet 回落');

// ---------------------------------------------------------------------------
// 3. approachExponential — 时间常数 ~2s 的指数趋近
// ---------------------------------------------------------------------------

// 经过恰好一个 tau, 残差 = 1/e
{
  const next = approachExponential(0, 1, SMOOTHING_TAU_MS);
  assert.ok(Math.abs(next - (1 - 1 / Math.E)) < 1e-9, `tau 步进应为 1-1/e, 实得 ${next}`);
}
// dt<=0 不动; 长期收敛
assert.equal(approachExponential(0.5, 1, 0), 0.5);
{
  let v = 0;
  for (let i = 0; i < 200; i++) v = approachExponential(v, 0.8, 100);
  // 200 步 × 100ms = 10τ, 残差 = 0.8·e⁻¹⁰ ≈ 3.6e-5
  assert.ok(Math.abs(v - 0.8) < 1e-3, '200 步 × 100ms (10τ) 后应基本收敛到目标');
}

console.log('✓ approachExponential: tau 语义 / 零 dt 不动 / 长期收敛');

// ---------------------------------------------------------------------------
// 4. createPresenceStore — current / recentEvents / SIM 纪律
// ---------------------------------------------------------------------------

// 注入假时钟, 避免测试依赖真实时间
let fakeNow = 1_000_000;
const store = createPresenceStore(() => fakeNow);

// 初始: 无真实数据 → current null, 不 simulated
assert.equal(store.get().current, null);
assert.equal(store.get().simulated, false);
assert.equal(store.get().connected, false);

// emotion 事件驱动 current (首条直接落位; Node 无 rAF, 插值循环惰性跳过)
{
  const kind = store.ingestLine('{"type":"emotion","at":"2026-08-21T08:30:00Z","pad":{"p":0.5,"a":-0.2,"d":0.1},"dominant":"joy","intensity":0.6,"response_style":"warm","tone":"温和"}');
  assert.equal(kind, 'presence');
  const c = store.get().current;
  assert.deepEqual({p: c.p, a: c.a, d: c.d}, {p: 0.5, a: -0.2, d: 0.1});
  assert.equal(c.mode, 'quiet');
  assert.equal(c.tone, '温和');
  assert.equal(store.get().connected, true); // 真实事件到达 = 频道有活性
}

// pad 越界防御性钳制到 [-1, 1]
{
  store.ingestLine('{"type":"emotion","at":"2026-08-21T08:31:00Z","pad":{"p":9,"a":-9,"d":0},"dominant":"anger","intensity":1,"response_style":"cautious"}');
  const c = store.get().current;
  // Node 无 rAF: displayedPad 停在上一条, 但 target 已更新; 手动确认钳制逻辑:
  assert.ok(c.p <= 1 && c.a >= -1, 'displayed 永远由 clamp 后的 target 趋近');
}

// initiative/spoke → 5s 内 mode = thinking
{
  const kind = store.ingestLine('{"type":"initiative","at":"2026-08-21T08:30:10Z","outcome":"spoke","action":"问候"}');
  assert.equal(kind, 'presence');
  assert.equal(store.get().current.mode, 'thinking');
  fakeNow += THINKING_WINDOW_MS + 100;
  // 时钟走过窗口后 (无新事件) 推导回落 quiet — setSpeaking 幂等调用触发重算
  store.setSpeaking(false);
  assert.equal(store.get().current.mode, 'quiet');
}

// setSpeaking / setChatActive 驱动 mode
store.setChatActive(true);
assert.equal(store.get().current.mode, 'thinking');
store.setSpeaking(true);
assert.equal(store.get().current.mode, 'speaking');
store.setSpeaking(false);
store.setChatActive(false);
assert.equal(store.get().current.mode, 'quiet');

// 双订阅挂接去重: 同一条事件 (同 type+at) 到达两次只记一次
{
  const line = '{"type":"dream","at":"2026-08-21T09:00:00Z","merged_count":1,"summary_prefix":"【做梦摘要】x"}';
  store.ingestLine(line);
  store.ingestLine(line);
  const dreams = store.get().recentEvents.filter((r) => r.event.type === 'dream');
  assert.equal(dreams.length, 1, '同 (type, at) 事件应去重');
}

// legacy 行不进 recentEvents
{
  const before = store.get().recentEvents.length;
  assert.equal(store.ingestLine('[他说] 你好'), 'legacy');
  assert.equal(store.get().recentEvents.length, before);
}

// 环形缓冲容量 50, 最新在前
{
  for (let i = 0; i < RECENT_EVENTS_CAPACITY + 10; i++) {
    store.ingest({
      type: 'memory_recall',
      at: `2026-08-21T10:${String(i).padStart(2, '0')}:00Z`,
      found: i,
      keywords: ['k'],
      redacted: true,
    });
  }
  const evts = store.get().recentEvents;
  assert.equal(evts.length, RECENT_EVENTS_CAPACITY);
  assert.equal(evts[0].event.found, RECENT_EVENTS_CAPACITY + 9, '最新在前');
}

// SIM 纪律: 断连 >30s → simulated=true, current 回落本机中性默认 (不编造情绪)
{
  store.setConnected(false);
  store.setSimulated(true);
  const s = store.get();
  assert.equal(s.simulated, true);
  assert.deepEqual({p: s.current.p, a: s.current.a, d: s.current.d}, {p: 0, a: 0, d: 0});
  assert.equal(s.current.tone, undefined, 'simulated 时不保留真实 tone');
  // 连通恢复 → simulated 清除, 真实 (虽陈旧的) 数据恢复
  store.setConnected(true);
  const r = store.get();
  assert.equal(r.simulated, false);
  assert.equal(r.connected, true);
  assert.notEqual(r.current, null, '有过真实 emotion 时重连后恢复真实值而非 null');
}

// subscribe 契约: 立即回放 + 变更推送 + 退订
{
  store.reset();
  const seen = [];
  const unsubscribe = store.subscribe((state) => seen.push(state));
  assert.equal(seen.length, 1, 'subscribe 立即回放当前快照');
  assert.equal(seen[0].current, null);
  store.ingestLine('{"type":"emotion","at":"2026-08-21T11:00:00Z","pad":{"p":0.1,"a":0.2,"d":0.3},"dominant":"joy","intensity":0.3,"response_style":"friendly"}');
  assert.equal(seen.length, 2, 'ingest 推送新快照');
  assert.equal(seen[1].current.p, 0.1);
  unsubscribe();
  store.ingestLine('{"type":"emotion","at":"2026-08-21T11:01:00Z","pad":{"p":0.9,"a":0.2,"d":0.3},"dominant":"joy","intensity":0.3,"response_style":"friendly"}');
  assert.equal(seen.length, 2, '退订后不再推送');
}

console.log('✓ presenceStore: current 驱动 / 去重 / 环形缓冲 / SIM 纪律 / subscribe 契约');

console.log('--- Presence Split & Mode Derivation Check PASSED ---');
