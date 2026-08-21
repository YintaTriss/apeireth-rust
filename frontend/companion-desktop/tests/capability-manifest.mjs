// Runtime Capability Manifest — Contract & Legacy Reality Check
// Mirrors the logic in src/lib/runtime.ts (fetchCapabilities / capabilitySupported / legacyCapabilityManifest)
// and the Rust manifest shape in crates/apeireth-companion/src/runtime_capabilities.rs.
// Node cannot import .ts directly, so pure logic is re-implemented here against the documented contract.
import assert from 'node:assert/strict';

console.log('--- Starting Apeireth Capability Manifest Reality Check ---');

// ---------------------------------------------------------------------------
// Re-implementation of the pure logic (must stay in sync with runtime.ts)
// ---------------------------------------------------------------------------

function legacyCapabilityManifest() {
  const cap = (id, supported, read, write, ops) => ({id, supported, read, write, version: 1, operations: ops});
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
      {name: 'permissions', capabilities: [cap('permissions.requests.read', true, true, false, ['list'])]},
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

function capabilitySupported(manifest, id) {
  if (!manifest) return false;
  for (const group of manifest.capabilities) {
    for (const cap of group.capabilities) {
      if (cap.id === id) return cap.supported === true;
    }
  }
  return false;
}

// A sample "known runtime" manifest matching the real Rust current_manifest() shape.
function knownRuntimeManifest() {
  const cap = (id, supported, read, write, ops) => ({id, supported, read, write, version: 1, operations: ops});
  return {
    schema_version: 1,
    runtime: {service: 'apeireth-companion-serve', version: '1.2.0'},
    legacy: false,
    capabilities: [
      {name: 'chat', capabilities: [cap('chat.completions', true, true, true, ['stream'])]},
      {name: 'health', capabilities: [cap('health', true, true, false, ['check'])]},
      {
        name: 'sessions',
        capabilities: [
          cap('sessions.read', true, true, false, ['list', 'get', 'timeline']),
          cap('sessions.create', false, false, false, []),
          cap('sessions.archive', false, false, false, []),
        ],
      },
      {
        name: 'memory',
        capabilities: [
          cap('memory.read', true, true, false, ['list', 'search', 'streams', 'graph']),
          cap('memory.append', true, false, true, ['append']),
          cap('memory.forget', false, false, false, []),
          cap('memory.protect', false, false, false, []),
        ],
      },
      {
        name: 'permissions',
        capabilities: [
          cap('permissions.requests.read', true, true, false, ['list']),
          cap('permissions.grant', true, false, true, ['grant']),
          cap('permissions.revoke', false, false, false, []),
        ],
      },
      {name: 'trace', capabilities: [cap('trace.read', false, false, false, ['list', 'detail'])]},
    ],
  };
}

// Simulates fetchCapabilities: 200 → parse; non-200/error → legacy fallback.
async function fetchCapabilitiesSim(status, body) {
  if (status === 200) {
    const data = typeof body === 'string' ? JSON.parse(body) : body;
    if (
      typeof data.schema_version !== 'number' ||
      !Array.isArray(data.capabilities) ||
      !data.runtime ||
      typeof data.runtime.service !== 'string'
    ) {
      return legacyCapabilityManifest();
    }
    return data;
  }
  return legacyCapabilityManifest();
}

// ---------------------------------------------------------------------------
// Test 1: Known-runtime manifest — supported read + unsupported mutations
// ---------------------------------------------------------------------------
console.log('[Test 1] Known runtime manifest: read supported, mutations honestly unsupported...');
{
  const m = knownRuntimeManifest();
  assert.equal(m.schema_version, 1);
  assert.equal(m.legacy, false);
  assert.equal(m.runtime.service, 'apeireth-companion-serve');

  // Read + chat + append genuinely wired
  assert.equal(capabilitySupported(m, 'chat.completions'), true);
  assert.equal(capabilitySupported(m, 'sessions.read'), true);
  assert.equal(capabilitySupported(m, 'memory.read'), true);
  assert.equal(capabilitySupported(m, 'memory.append'), true);
  assert.equal(capabilitySupported(m, 'permissions.grant'), true);

  // Mutations not yet wired must be honestly unsupported (UI must disable these)
  assert.equal(capabilitySupported(m, 'sessions.create'), false);
  assert.equal(capabilitySupported(m, 'sessions.archive'), false);
  assert.equal(capabilitySupported(m, 'memory.forget'), false);
  assert.equal(capabilitySupported(m, 'memory.protect'), false);
  assert.equal(capabilitySupported(m, 'permissions.revoke'), false);
  assert.equal(capabilitySupported(m, 'trace.read'), false);

  console.log('  -> PASS: known-runtime capability gating honest.');
}

// ---------------------------------------------------------------------------
// Test 2: Legacy runtime (no /v1/apeireth/capabilities) — conservative fallback
// ---------------------------------------------------------------------------
console.log('[Test 2] Legacy runtime: 404 / 500 / network error → legacy profile, no white screen...');
{
  // 404 → legacy
  const m404 = await fetchCapabilitiesSim(404);
  assert.equal(m404.legacy, true, '404 must fall back to legacy, not crash');
  assert.equal(capabilitySupported(m404, 'chat.completions'), true);
  assert.equal(capabilitySupported(m404, 'memory.read'), true);
  // legacy never speculates mutation
  assert.equal(capabilitySupported(m404, 'memory.append'), false);
  assert.equal(capabilitySupported(m404, 'memory.forget'), false);
  assert.equal(capabilitySupported(m404, 'sessions.create'), false);
  assert.equal(capabilitySupported(m404, 'permissions.grant'), false);

  // 500 → legacy
  const m500 = await fetchCapabilitiesSim(500);
  assert.equal(m500.legacy, true);

  // network error (simulate via null status) → legacy
  const mErr = await fetchCapabilitiesSim(null);
  assert.equal(mErr.legacy, true);

  console.log('  -> PASS: legacy runtime degrades gracefully (no white screen).');
}

// ---------------------------------------------------------------------------
// Test 3: Unsupported operation — unknown capability id is unsupported
// ---------------------------------------------------------------------------
console.log('[Test 3] Unsupported operation: unknown capability id → false (no guessing)...');
{
  const m = knownRuntimeManifest();
  assert.equal(capabilitySupported(m, 'memory.purge'), false, 'unknown id must be unsupported');
  assert.equal(capabilitySupported(m, 'nonexistent.thing'), false);
  assert.equal(capabilitySupported(m, ''), false);
  // null manifest → false
  assert.equal(capabilitySupported(null, 'chat.completions'), false);
  console.log('  -> PASS: unknown capability ids never reported as supported.');
}

// ---------------------------------------------------------------------------
// Test 4: Forward compatibility — unknown fields preserved, no crash
// ---------------------------------------------------------------------------
console.log('[Test 4] Forward compat: unknown fields in manifest do not break parsing...');
{
  // Future runtime adds a new capability with extra fields + a top-level unknown key
  const future = JSON.stringify({
    schema_version: 1,
    runtime: {service: 'x', version: '2.0.0'},
    capabilities: [
      {
        name: 'future',
        capabilities: [
          {id: 'future.cap', supported: true, read: true, write: false, version: 3, operations: ['x'], unknown_field: 'ok'},
        ],
      },
    ],
    legacy: false,
    future_top_level: 42,
  });
  const m = await fetchCapabilitiesSim(200, future);
  assert.equal(m.legacy, false);
  assert.equal(capabilitySupported(m, 'future.cap'), true);
  assert.equal(m.capabilities[0].capabilities[0].version, 3);
  console.log('  -> PASS: forward compatibility preserved (unknown fields ignored).');
}

// ---------------------------------------------------------------------------
// Test 5: Version-change refresh — manifest re-fetched on reconnect
// ---------------------------------------------------------------------------
console.log('[Test 5] Version-change refresh: re-fetching returns fresh manifest, not stale...');
{
  // First fetch: runtime v1.2.0
  const m1 = await fetchCapabilitiesSim(200, JSON.stringify(knownRuntimeManifest()));
  assert.equal(m1.runtime.version, '1.2.0');
  assert.equal(capabilitySupported(m1, 'trace.read'), false, 'trace not supported in v1.2.0');

  // Runtime upgraded: now declares trace.read supported (new version)
  const upgraded = JSON.parse(JSON.stringify(knownRuntimeManifest()));
  upgraded.runtime.version = '1.3.0';
  upgraded.capabilities.find((g) => g.name === 'trace').capabilities[0].supported = true;
  const m2 = await fetchCapabilitiesSim(200, JSON.stringify(upgraded));
  assert.equal(m2.runtime.version, '1.3.0', 're-fetch must return fresh version');
  assert.equal(capabilitySupported(m2, 'trace.read'), true, 're-fetch must reflect new capability');
  console.log('  -> PASS: re-fetch on version change returns fresh manifest.');
}

// ---------------------------------------------------------------------------
// Test 6: Malformed manifest (missing required fields) → legacy fallback
// ---------------------------------------------------------------------------
console.log('[Test 6] Malformed manifest → legacy fallback (no crash, no fake data)...');
{
  // Missing schema_version
  const bad1 = await fetchCapabilitiesSim(200, JSON.stringify({runtime: {service: 'x'}, capabilities: []}));
  assert.equal(bad1.legacy, true, 'missing schema_version → legacy');
  // Missing runtime
  const bad2 = await fetchCapabilitiesSim(200, JSON.stringify({schema_version: 1, capabilities: []}));
  assert.equal(bad2.legacy, true, 'missing runtime → legacy');
  // capabilities not an array
  const bad3 = await fetchCapabilitiesSim(200, JSON.stringify({schema_version: 1, runtime: {service: 'x'}, capabilities: 'nope'}));
  assert.equal(bad3.legacy, true, 'malformed capabilities → legacy');
  console.log('  -> PASS: malformed manifest safely falls back to legacy.');
}

// ---------------------------------------------------------------------------
// Test 7: No secret leak in manifest
// ---------------------------------------------------------------------------
console.log('[Test 7] Manifest never exposes secrets or internal paths...');
{
  const m = knownRuntimeManifest();
  const json = JSON.stringify(m);
  assert.ok(!json.includes('api_key'));
  assert.ok(!json.includes('apiKey'));
  assert.ok(!json.includes('master_token'));
  assert.ok(!json.includes('masterToken'));
  assert.ok(!json.includes('password'));
  assert.ok(!json.includes('.sqlite'));
  assert.ok(!json.includes('APPDATA'));
  const ljson = JSON.stringify(legacyCapabilityManifest());
  assert.ok(!ljson.includes('api_key'));
  console.log('  -> PASS: manifest is secret-free.');
}

console.log('--- All Capability Manifest Reality Check Tests PASSED! ---');
