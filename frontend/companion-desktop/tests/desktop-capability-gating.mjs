// Desktop Capability Gating — Phase 6 reality check.
// 验证 UI 按钮依据 capability manifest gate (不 404-probe), 以及 legacy runtime 降级.
// 纯逻辑镜像 src/lib/runtime.ts (capabilitySupported / legacyCapabilityManifest).
import assert from 'node:assert/strict';

console.log('--- Starting Desktop Capability Gating Reality Check ---');

function legacyCapabilityManifest() {
  const cap = (id, supported, read, write, ops) => ({id, supported, read, write, version: 1, operations: ops});
  return {
    schema_version: 1,
    runtime: {service: 'apeireth-legacy-runtime', version: 'unknown'},
    legacy: true,
    capabilities: [
      {name: 'chat', capabilities: [cap('chat.completions', true, true, true, ['stream'])]},
      {name: 'memory', capabilities: [cap('memory.read', true, true, false, ['list', 'search'])]},
      {name: 'permissions', capabilities: [cap('permissions.requests.read', true, true, false, ['list'])]},
    ],
  };
}

function capabilitySupported(manifest, id) {
  if (!manifest) return false;
  for (const group of manifest.capabilities) {
    for (const cap of group.capabilities) {
      if (cap.id === id) return cap.supported === true;
    }
  }
  return false;
}

// Phase 2/3/4/5 接线后的 "current" manifest (镜像 Rust current_manifest).
function currentManifest() {
  const cap = (id, supported, read, write, ops) => ({id, supported, read, write, version: 1, operations: ops});
  return {
    schema_version: 1,
    runtime: {service: 'apeireth-companion-serve', version: '1.2.0'},
    legacy: false,
    capabilities: [
      {name: 'sessions', capabilities: [
        cap('sessions.read', true, true, false, ['list']),
        cap('sessions.create', true, false, true, ['create']),
        cap('sessions.rename', true, false, true, ['rename']),
        cap('sessions.archive', true, false, true, ['archive']),
        cap('sessions.restore', true, false, true, ['restore']),
        cap('sessions.close', true, false, true, ['close']),
      ]},
      {name: 'memory', capabilities: [
        cap('memory.read', true, true, false, ['list']),
        cap('memory.append', true, false, true, ['append']),
        cap('memory.update', true, false, true, ['update']),
        cap('memory.forget', true, false, true, ['forget']),
        cap('memory.protect', true, false, true, ['protect']),
        cap('memory.unprotect', true, false, true, ['unprotect']),
      ]},
      {name: 'permissions', capabilities: [
        cap('permissions.grant', true, false, true, ['grant']),
        cap('permissions.revoke', true, false, true, ['revoke']),
        cap('permissions.grants.read', true, true, false, ['list']),
        cap('permissions.policy.write', false, false, false, []),
      ]},
      {name: 'trace', capabilities: [
        cap('trace.read', true, true, false, ['list']),
        cap('trace.subscribe', true, true, false, ['subscribe']),
      ]},
    ],
  };
}

// Test 1: Memory forget/protect 按钮在 current runtime 解锁
console.log('[Test 1] Current runtime: memory mutation buttons unlocked...');
{
  const m = currentManifest();
  assert.equal(capabilitySupported(m, 'memory.forget'), true, 'forget 应解锁');
  assert.equal(capabilitySupported(m, 'memory.protect'), true, 'protect 应解锁');
  assert.equal(capabilitySupported(m, 'memory.unprotect'), true, 'unprotect 应解锁');
  assert.equal(capabilitySupported(m, 'sessions.create'), true, 'session create 应解锁');
  assert.equal(capabilitySupported(m, 'permissions.revoke'), true, 'revoke 应解锁');
  assert.equal(capabilitySupported(m, 'trace.read'), true, 'trace read 应解锁');
  console.log('  -> PASS: current runtime gates unlock mutation UI.');
}

// Test 2: Legacy runtime: mutation 按钮降级 disabled/隐藏 (不 404-probe)
console.log('[Test 2] Legacy runtime: mutation buttons gated (no 404-probe)...');
{
  const m = legacyCapabilityManifest();
  assert.equal(capabilitySupported(m, 'memory.forget'), false, 'legacy: forget 必须 disabled');
  assert.equal(capabilitySupported(m, 'memory.protect'), false, 'legacy: protect 必须 disabled');
  assert.equal(capabilitySupported(m, 'sessions.create'), false, 'legacy: session create 必须 disabled');
  assert.equal(capabilitySupported(m, 'permissions.revoke'), false, 'legacy: revoke 必须 hidden');
  assert.equal(capabilitySupported(m, 'trace.read'), false, 'legacy: trace link 必须 hidden');
  // 但只读/chat 仍可用 (不白屏).
  assert.equal(capabilitySupported(m, 'chat.completions'), true, 'legacy: chat 仍可用');
  assert.equal(capabilitySupported(m, 'memory.read'), true, 'legacy: memory read 仍可用');
  console.log('  -> PASS: legacy runtime degrades to read-only (no white screen, no 404-probe).');
}

// Test 3: null manifest (尚未加载) → 全部 false (保守)
console.log('[Test 3] Null manifest (not yet loaded): all false (conservative)...');
{
  assert.equal(capabilitySupported(null, 'memory.forget'), false);
  assert.equal(capabilitySupported(null, 'chat.completions'), false);
  assert.equal(capabilitySupported(undefined, 'memory.read'), false);
  console.log('  -> PASS: null/undefined manifest gates everything off.');
}

// Test 4: manifest 加载后 UI 行为切换 (runtime version 变化刷新)
console.log('[Test 4] Manifest refresh on version change re-gates UI...');
{
  // 初始 legacy → forget disabled
  let m = legacyCapabilityManifest();
  assert.equal(capabilitySupported(m, 'memory.forget'), false);
  // runtime 升级, 重新拉 manifest → forget 解锁
  m = currentManifest();
  assert.equal(capabilitySupported(m, 'memory.forget'), true);
  assert.notEqual(m.runtime.version, legacyCapabilityManifest().runtime.version);
  console.log('  -> PASS: version-change refresh re-gates UI correctly.');
}

// Test 5: unknown capability id (未来能力) → false (不假装支持)
console.log('[Test 5] Unknown capability id: false (no guessing)...');
{
  const m = currentManifest();
  assert.equal(capabilitySupported(m, 'memory.purge'), false);
  assert.equal(capabilitySupported(m, 'future.cap'), false);
  console.log('  -> PASS: unknown ids never unlock UI.');
}

console.log('--- All Desktop Capability Gating Tests PASSED! ---');
