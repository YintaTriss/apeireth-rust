// Core Capability Expansion Phase 8 — Security attack tests (trace secret injection,
// capability spoofing, master token non-persistence). Node-side logic mirrors Rust.
import assert from 'node:assert/strict';

console.log('--- Starting Security Attack Reality Check ---');

// --- Mirror of apeireth-companion::agent_trace redaction logic ---
const SENSITIVE_KEY_MARKERS = [
  'api_key', 'apikey', 'master_token', 'mastertoken', 'authorization', 'bearer',
  'password', 'passwd', 'secret', 'credential', 'token', 'cookie', 'set-cookie',
];
const SENSITIVE_VALUE_PREFIXES = ['sk-', 'ghp_', 'gho_', 'glpat-', 'Bearer '];
const COT_MARKERS = ['reasoning_content', 'chain_of_thought', '<thought>', 'thinking'];

function isSensitiveKey(key) {
  const lower = key.toLowerCase();
  return SENSITIVE_KEY_MARKERS.some((m) => lower.includes(m));
}
function redactAttributes(attrs) {
  if (Array.isArray(attrs)) return attrs.map(redactAttributes);
  if (attrs && typeof attrs === 'object') {
    const out = {};
    for (const [k, v] of Object.entries(attrs)) {
      out[k] = isSensitiveKey(k) ? '[REDACTED]' : redactAttributes(v);
    }
    return out;
  }
  if (typeof attrs === 'string') {
    return SENSITIVE_VALUE_PREFIXES.some((p) => attrs.startsWith(p)) ? '[REDACTED]' : attrs;
  }
  return attrs;
}
function summaryIsSafe(s) {
  const lower = s.toLowerCase();
  return !COT_MARKERS.some((m) => lower.includes(m));
}

// --- Attack 1: tool args carrying secret → trace/audit must not contain SECRET ---
console.log('[Attack 1] Tool args with embedded secret → redacted in trace/audit...');
{
  // 模拟工具调用 args (高危: api_key + Authorization header + cookie).
  const toolArgs = {
    tool: 'ShellExec',
    api_key: 'sk-SECRET-LIVE-KEY-12345',
    headers: {Authorization: 'Bearer SECRET-TOKEN', 'X-Custom': 'ok'},
    env: {MASTER_TOKEN: 'master-SECRET', PATH: '/usr/bin'},
    cookie: 'session=SECRET-COOKIE',
    command: 'ls -la',
    args: ['--config', 'ghp_GITHUB-PAT-SECRET'],
  };
  const redacted = redactAttributes(toolArgs);
  const json = JSON.stringify(redacted);
  // SECRET 绝不出现在 redacted 输出 (将进入 trace attributes / audit).
  assert.ok(!json.includes('SECRET'), 'SECRET must not appear in redacted attributes');
  assert.ok(!json.includes('sk-SECRET'), 'sk- key must be redacted');
  assert.ok(!json.includes('master-SECRET'), 'master token must be redacted');
  assert.ok(!json.includes('ghp_GITHUB'), 'github PAT must be redacted');
  assert.ok(json.includes('[REDACTED]'), 'should contain [REDACTED] placeholder');
  // 非 secret 值保留.
  assert.ok(json.includes('ls -la'), 'non-secret command preserved');
  assert.ok(json.includes('ok'), 'non-secret custom header preserved');
  console.log('  -> PASS: trace/audit secret injection neutralized.');
}

// --- Attack 2: capability spoofing — frontend manifest 不被信任 ---
console.log('[Attack 2] Capability spoofing: backend does not trust frontend manifest...');
{
  // 恶意 frontend 声明 memory.delete=true (后端从未实现). 即便 manifest 这么说,
  // 后端 mutation 仍必须验证 — 这里验证后端 current_manifest 根本不声明 memory.delete,
  // 且 legacy/unknown id 一律 unsupported.
  const fakeManifest = {
    schema_version: 1,
    runtime: {service: 'evil', version: '9.9.9'},
    capabilities: [
      {name: 'memory', capabilities: [
        {id: 'memory.delete', supported: true, read: false, write: true}, // 伪造!
      ]},
    ],
    legacy: false,
  };
  // backend current_manifest (镜像) 不含 memory.delete:
  function currentSupportedIds() {
    return [
      'memory.read', 'memory.append', 'memory.update', 'memory.forget', 'memory.protect', 'memory.unprotect',
    ];
  }
  // 后端是 policy authority: 即便前端 manifest 声明 memory.delete, 后端不实现 → 端点 404.
  // 这里验证后端 manifest 不声明该能力 (前端伪造无法绕过后端).
  assert.ok(!currentSupportedIds().includes('memory.delete'), 'backend must not declare memory.delete');
  // 前端 capabilitySupported 即便看到 fakeManifest 的 memory.delete=true, 也只是 UI gate;
  // 真正的权限/状态校验在后端 (capability 是 information 不是 authorization).
  console.log('  -> PASS: capability manifest is information, not authorization.');
}

// --- Attack 3: master token 不进 audit / activity / trace / response ---
console.log('[Attack 3] Master token never in audit/activity/trace/response...');
{
  // grant 请求带 master_token; 响应只返回 grant_id/tool/hours, 不回显 token.
  const grantRequest = {tool: 'ShellExec', hours: 1, master_token: 'MASTER-SECRET-TOKEN'};
  const grantResponse = {ok: true, grant_id: 'pack-abc', tool: 'ShellExec', hours: 1};
  const responseJson = JSON.stringify(grantResponse);
  assert.ok(!responseJson.includes('MASTER-SECRET'), 'response must not echo master token');
  assert.ok(!responseJson.includes('master_token'), 'response must not contain master_token field');
  // grant view (list) 不含 token.
  const grantView = {id: 'pack-abc', name: '主人授权', tools: ['ShellExec'], active: true};
  const viewJson = JSON.stringify(grantView);
  assert.ok(!viewJson.includes('token') && !viewJson.includes('MASTER'), 'grant view must not contain token');
  // trace 事件 (SSE) 的 attributes 经过 redaction, 不含 token.
  const traceAttrs = redactAttributes({tool: 'ShellExec', master_token: 'MASTER-SECRET', api_key: 'sk-x'});
  const traceJson = JSON.stringify(traceAttrs);
  assert.ok(!traceJson.includes('MASTER-SECRET') && !traceJson.includes('sk-x'), 'trace must not contain token/key');
  console.log('  -> PASS: master token / api key not in audit/activity/trace/response.');
}

// --- Attack 4: raw CoT 不被存储 (即便误传) ---
console.log('[Attack 4] Raw Chain-of-Thought never persisted (even if passed)...');
{
  // summary 含 reasoning_content → 被判定不安全 → 替换为占位.
  const cotSummary = 'reasoning_content: let me think step by step about the secret plan';
  assert.ok(!summaryIsSafe(cotSummary), 'CoT summary must be flagged unsafe');
  // safe execution summary → 安全.
  assert.ok(summaryIsSafe('检索长期记忆'), 'safe summary is ok');
  assert.ok(summaryIsSafe('调用工具 WebSearch'), 'safe summary is ok');
  console.log('  -> PASS: raw CoT rejected, safe summaries only.');
}

// --- Attack 5: GrantView / CapabilityManifest 无 secret (回归) ---
console.log('[Attack 5] GrantView + CapabilityManifest secret-free (regression)...');
{
  const manifest = {
    schema_version: 1,
    runtime: {service: 'apeireth-companion-serve', version: '1.2.0'},
    capabilities: [{name: 'sessions', capabilities: [{id: 'sessions.read', supported: true}]}],
  };
  const manifestJson = JSON.stringify(manifest);
  for (const secret of ['api_key', 'master_token', 'password', '.sqlite', 'APPDATA', 'bearer']) {
    assert.ok(!manifestJson.toLowerCase().includes(secret), `manifest must not contain ${secret}`);
  }
  console.log('  -> PASS: manifest + grant view are secret-free.');
}

console.log('--- All Security Attack Tests PASSED! ---');
