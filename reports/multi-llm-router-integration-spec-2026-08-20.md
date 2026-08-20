# MultiLlmRouter 真接入 companion_serve 主链路 — 实施规格

> **任务**: 把 `MultiLlmRouter` 真接入 `crates/apeireth-companion/examples/companion_serve.rs`, 让 TOML 里配的多个 provider 都生效, fallback 链真跑.
>
> **日期**: 2026-08-20
> **作者**: Mavis (spec 阶段)
> **状态**: 📝 规划稿 — 等待主人审阅
> **工作目录**: `C:\Users\31683\Apeireth-rust`

---

## 0. TL;DR

**问题诊断**: 当前 `companion_serve` 用 `build_pipeline(BASE_URL, key)` 单 provider, 启动时只取 TOML 第一个 provider 的 `base_url`. 用户配 5 个 provider 实际只走 1 个, fallback 链不生效. `docs/02-guides/custom-llm.md §6` 已经标 "**真正"按 model 选 provider"需要接入 `MultiLlmRouter` 全套 (V1.1 中期路线)**".

**核心方案**: 引入 `PipelinePool` 抽象 — 每个 provider 构造独立 `Pipeline` 实例 + `MultiLlmRouter` 维护路由策略. 启动时按 TOML 配置构造, 无 TOML 时退化单 `Pipeline`. `dispatch()` 调用前先按 `req.model` 选 Pipeline.

**关键约束**:
- 0 触碰 `apeireth-pipeline/src/**/*.rs` (Pipeline LOCKED, 5 步管线/HTTP/Keep-Alive 都不能改)
- 0 改 `apeireth-api` 公开签名 (`LlmProvider` trait / `dispatch` / `stream_forward` 全部稳定)
- 0 改 `workspace.version` (1.2.0 双轴制)
- 0 改任何 enum/const/24 LOCKED crate 入口签名
- 0 引入外部依赖

---

## 1. 现有 Code Map (哪些函数/结构体受影响)

### 1.1 `companion_serve.rs` (1993 行, **唯一改动文件**)

| 行号 | 符号 | 角色 | 改动类别 |
|---|---|---|---|
| 36-40 | `use apeireth_api::{Pipeline, ProtocolKind, build_pipeline, dispatch, ...}` | 导入 | 加 `MultiLlmRouter` + `LlmConfig` + `LlmProvider` trait |
| 77 | `const DEFAULT_BASE_URL: &str = "https://api.minimaxi.com"` | const | **不改** (保留回退) |
| 80 | `const DEFAULT_MODEL: &str = "MiniMax-M3"` | const | **不改** (保留回退) |
| 86-95 | `MODEL` thread_local + `init_model()` + `model()` | 全局 model | **不改** (env 路径) |
| 112-122 | `BASE_URL` thread_local + `init_base_url()` | 全局 base_url | **不改** (回退路径) |
| 134 | `MAX_TOOL_ROUNDS: usize = 5` | const | **不改** |
| 136-137 | `DEFAULT_MAX_TOKENS / MAX_TOKENS_CAP` | const | **不改** |
| 161 | `pub struct MiniMaxMemoryExtractor { pipeline: Arc<Pipeline> }` | 6 个 LLM 实现 | **结构改**: pipeline 字段保留, 但 trait 内的 dispatch 改走 PipelinePool |
| 191, 275 | `dispatch(&self.pipeline, ...)` | MiniMaxMemoryExtractor 2 处 | 改走 pool |
| 457 | `pub struct MiniMaxDreamSummarizer { pipeline: Arc<Pipeline> }` | LLM impl | 改 |
| 487 | `dispatch(&self.pipeline, ...)` | DreamSummarizer 1 处 | 改走 pool |
| 508 | `pub struct MiniMaxConstitutionLlm { pipeline: Arc<Pipeline> }` | LLM impl | 改 |
| 538 | `dispatch(&self.pipeline, ...)` | ConstitutionLlm 1 处 | 改走 pool |
| 557 | `pub struct TonalUtterance { pipeline: Arc<Pipeline> }` | LLM impl | 改 |
| 588 | `dispatch(&self.pipeline, ...)` | TonalUtterance 1 处 | 改走 pool |
| 615 | `pub struct MiniMaxReflector { pipeline: Arc<Pipeline> }` | LLM impl | 改 |
| 645 | `dispatch(&self.pipeline, ...)` | Reflector 1 处 | 改走 pool |
| 668 | `pub struct MiniMaxDeepRecall { pipeline: Arc<Pipeline> }` | LLM impl | 改 |
| 704 | `dispatch(&self.pipeline, ...)` | DeepRecall 1 处 | 改走 pool |
| 738 | `pub struct MiniMaxDialogSummarizer { pipeline: Arc<Pipeline> }` | LLM impl | 改 |
| 772 | `dispatch(&self.pipeline, ...)` | DialogSummarizer 1 处 | 改走 pool |
| 796 | `pub struct MiniMaxExperienceRefiner { pipeline: Arc<Pipeline> }` | LLM impl | 改 |
| 829 | `dispatch(&self.pipeline, ...)` | ExperienceRefiner 1 处 | 改走 pool |
| 402-415 | `struct AppState { ... pipeline: Arc<Pipeline> ... }` | AppState | **结构改**: `pipeline: Arc<Pipeline>` → `pool: Arc<PipelinePool>` |
| 1150 | `stream_forward(&st.pipeline, ...)` | streaming branch 1 处 | 改走 pool (按 req.model 选 Pipeline) |
| 1179 | `chat_once(&st.pipeline, &req2, rounds)` | chat_once 1 处 | 改走 pool |
| 1291-1331 | `async fn chat_once(pipeline: &Arc<Pipeline>, req: &OpenAiChatRequest, label: usize)` | 函数签名 | **签名改**: `pipeline: &Arc<Pipeline>` → `pool: &Arc<PipelinePool>` |
| 1298-1329 | `for attempt in 0..3 { ... dispatch(pipeline, ...) }` | 重试循环 | 改走 pool |
| 1343-1356 | `let toml_base = ... LlmConfig::from_file(&path) ...` | TOML 加载 | **结构改**: 现在只读 base_url, 改读完整 config |
| 1397 | `let pipeline = Arc::new(build_pipeline(base_url().to_string(), Some(key.clone()))?);` | 主 pipeline 构造 | **结构改**: 改成构造 PipelinePool (1 个或 N 个 pipeline) |
| 1402, 1458, 1462, 1465, 1468, 1488, 1502, 1533 | 各 LLM impl 注入 pipeline | 8 处 | 改注入 pool |
| 1562-1571 | `AppState { bridge, store, pipeline, ... }` | AppState 构造 | 改用 pool |
| 1810-1993 | `#[cfg(test)] mod cot_extraction_tests + llm_config_tests` | 单元测 | **加新 mod**: `multi_llm_router_tests` (6+ 测) |

### 1.2 总 dispatch 调用计数 (grep `dispatch(&self.pipeline` + `dispatch(&st.pipeline` + `dispatch(pipeline`)

**9 个 dispatch 调用点 (LLM 实现内部)**:
- `MiniMaxMemoryExtractor.extract` — 行 191
- `MiniMaxMemoryExtractor.reconcile` — 行 275
- `MiniMaxDreamSummarizer.summarize` — 行 487
- `MiniMaxConstitutionLlm.ask` — 行 537
- `TonalUtterance.utter` — 行 587
- `MiniMaxReflector.reflect` — 行 645
- `MiniMaxDeepRecall.recall` — 行 704
- `MiniMaxDialogSummarizer.summarize` — 行 772
- `MiniMaxExperienceRefiner.refine` — 行 829

**3 个 dispatch 调用点 (HTTP handler 内部)**:
- `chat_completions` streaming branch — 行 1150 (`stream_forward(&st.pipeline, ...)`)
- `chat_completions` tool loop — 行 1179 (`chat_once(&st.pipeline, &req2, rounds)`)
- `chat_once` 内层重试 — 行 1301 (`dispatch(pipeline, ProtocolKind::OpenAiChat, normalized)`)

**总计 12 处调用点**, 全部需要从 `&Arc<Pipeline>` 改成 `&Arc<PipelinePool>` (新抽象).

### 1.3 不改动的文件 (LOCKED)

| 文件 | 原因 |
|---|---|
| `crates/apeireth-pipeline/src/**/*.rs` (14 个 mod) | Pipeline LOCKED |
| `crates/apeireth-api/src/llm/{router,config,traits}.rs` | 公开签名稳定 |
| `crates/apeireth-api/src/llm/providers/*.rs` | provider 实现稳定 |
| `crates/apeireth-api/src/protocol_handlers.rs` (102-113 `build_pipeline`, 907-915 `dispatch`, 1379-1419 `stream_forward`) | 公开签名稳定 |
| `crates/apeireth-llm-iface/src/**/*.rs` | iface LOCKED |
| `Cargo.toml` (workspace.version) | workspace.version 1.2.0 不改 |
| `crates/apeireth-environment/tests/*` | 其他 AI 工作区 |
| `crates/apeireth-provider/tests/*` | 其他 AI 工作区 |
| `scripts/gh_*.ps1` (3-4 个) | 其他 AI 工作区 |

---

## 2. PipelinePool 设计 (新增抽象)

### 2.1 类型定义 (加在 `companion_serve.rs` 顶部, 不入 lib)

```rust
/// PipelinePool — 多 provider Pipeline 池 + MultiLlmRouter 策略层
///
/// **职责**:
/// - 持有 N 个独立 Pipeline 实例 (每个 provider 一个)
/// - 持有 1 个 MultiLlmRouter (fallback 策略)
/// - 按 `req.model` 字段路由到正确的 provider Pipeline
/// - 模型未知时按 MultiLlmRouter fallback_order 顺序尝试
///
/// **退化模式**: 1 个 Pipeline 时 = 单 provider 行为, 1:1 兼容旧版
/// (per custom-llm.md §6 限制 "当前路径 = 切 base_url + 切 model").
///
/// **0 触碰**: Pipeline 仍是 `Arc<Pipeline>`, 不动 Pipeline src/ 任何 .rs.
pub struct PipelinePool {
    /// provider name → Pipeline 实例
    pipelines: HashMap<String, Arc<Pipeline>>,
    /// fallback 顺序 (provider name) — 与 MultiLlmRouter.fallback_order 1:1
    fallback_order: Vec<String>,
    /// 第一个 provider 的 Pipeline (无 TOML / fallback 全失败时用)
    default_pipeline: Arc<Pipeline>,
    /// router 维护健康状态
    router: Arc<MultiLlmRouter>,
}

impl PipelinePool {
    /// 单 provider 构造 (无 TOML / 退化路径)
    pub fn single(provider_name: &str, pipeline: Arc<Pipeline>) -> Self {
        let router = MultiLlmRouter::new();
        // 注: router 仅在 multi provider 时启用, single 模式 router 是空壳
        Self {
            pipelines: HashMap::new(),
            fallback_order: vec![provider_name.to_string()],
            default_pipeline: pipeline,
            router: Arc::new(router),
        }
    }

    /// 多 provider 构造 (有 TOML / 真接 router)
    pub fn multi(
        pipelines: HashMap<String, Arc<Pipeline>>,
        fallback_order: Vec<String>,
        router: Arc<MultiLlmRouter>,
    ) -> Self {
        let default = pipelines
            .values()
            .next()
            .cloned()
            .expect("multi pool 至少 1 pipeline");
        Self {
            pipelines,
            fallback_order,
            default_pipeline: default,
            router,
        }
    }

    /// 按 model 选 Pipeline (核心路由逻辑)
    /// 1) 先在 router 里查 supports_model → 命中取第一个候选
    /// 2) router 没候选 → 退化 default_pipeline
    pub fn select_pipeline(&self, model: &str) -> Arc<Pipeline> {
        // 通过 router 选 (复用 supports_model 逻辑)
        for name in &self.fallback_order {
            if let Some(p) = self.pipelines.get(name) {
                if self.provider_supports_model(name, model) {
                    return Arc::clone(p);
                }
            }
        }
        // router 没有匹配 → 退化 default
        Arc::clone(&self.default_pipeline)
    }

    fn provider_supports_model(&self, provider_name: &str, model: &str) -> bool {
        // TODO: 实际查 LlmProvider.supports_model, 见 §3 实现细节
        // 简化: pipeline 名 == "apeireth-api" → minimaxi model; 等等
        true // v1 简化: 任何 provider 接受任何 model (跟旧 build_pipeline 行为 1:1)
    }

    /// 列出所有 provider 名字 (debug / health endpoint)
    pub fn provider_names(&self) -> Vec<String> {
        if self.pipelines.is_empty() {
            self.fallback_order.clone()
        } else {
            self.fallback_order.clone()
        }
    }
}
```

**关键不变量**:
1. **单 pipeline 模式**: `pipelines: HashMap::new()`, `default_pipeline: 唯一的 pipeline` — 与旧版 `&st.pipeline` 行为 1:1
2. **多 pipeline 模式**: `pipelines: HashMap` 至少 1 元素, `fallback_order` 非空
3. **`select_pipeline()` 不 panic**: 任何 model 都能返回 pipeline (fallback 到 default)
4. **`Arc<Pipeline>` 引用计数**: 同一 Pipeline 在 `pipelines` 和 `default_pipeline` 可能同时持有 (OK, Arc 共享)

### 2.2 启动期构造 (main() 改造)

```rust
// 旧版 (单 pipeline):
let pipeline = Arc::new(build_pipeline(base_url().to_string(), Some(key.clone()))?);

// 新版 (双模):
let pool = if let Ok(path) = std::env::var("APEIRETH_LLM_CONFIG") {
    match LlmConfig::from_file(&path) {
        Ok(cfg) if !cfg.providers.is_empty() => {
            // 多 provider 模式: 每个 provider build 独立 Pipeline
            let mut pipelines = HashMap::new();
            let mut key_for = |env_name: &str| -> Result<String, String> {
                std::env::var(env_name).map_err(|e| format!("env {env_name}: {e}"))
            };
            for (name, pcfg) in &cfg.providers {
                let base = pcfg.base_url.clone()
                    .ok_or_else(|| format!("provider {name} 缺 base_url"))?;
                let key = key_for(&pcfg.api_key_env)?;
                let pipe = Arc::new(build_pipeline(base, Some(key))?);
                pipelines.insert(name.clone(), pipe);
                println!("[llm] provider {name}: {} ({} models)", pcfg.base_url.as_deref().unwrap_or("?"), pcfg.models.len());
            }
            let router = Arc::new(cfg.build_router()?);
            let fallback = cfg.router.fallback_order.clone();
            Arc::new(PipelinePool::multi(pipelines, fallback, router))
        }
        Ok(_) | Err(_) => {
            // TOML 空或解析失败 → 退化单 pipeline
            eprintln!("[llm] TOML 退化到单 Pipeline (空配置或解析失败)");
            let pipe = Arc::new(build_pipeline(base_url().to_string(), Some(key.clone()))?);
            Arc::new(PipelinePool::single("default", pipe))
        }
    }
} else {
    // 无 TOML → 单 pipeline (旧版 1:1 行为)
    let pipe = Arc::new(build_pipeline(base_url().to_string(), Some(key.clone()))?);
    Arc::new(PipelinePool::single("default", pipe))
};
```

### 2.3 dispatch 调用改造 (模式 1: pool 取 Pipeline 后调原 dispatch)

```rust
// helper 函数 (新加, 放在 chat_once 上方)
async fn pool_dispatch(
    pool: &Arc<PipelinePool>,
    kind: ProtocolKind,
    input: NormalizedRequest,
    model: &str,
) -> Result<NormalizedResponse, String> {
    let pipe = pool.select_pipeline(model);
    dispatch(&pipe, kind, input).await
}

// 9 个 dispatch 调用点全部改成:
let resp = pool_dispatch(&pool, ProtocolKind::OpenAiChat, normalized, model()).await
    .map_err(|e| format!("提炼 LLM 调用失败: {e}"))?;
```

**最小侵入**: 12 处调用 → 12 处改一行 (把 `dispatch(&self.pipeline, ...)` 改成 `pool_dispatch(&self.pool, ..., model()).await`). LLM impl struct 内的字段从 `pipeline: Arc<Pipeline>` 改成 `pool: Arc<PipelinePool>`, 构造处改一下即可.

### 2.4 stream_forward 改造 (mode 2: pool.select_pipeline 取 + 透传)

```rust
// streaming branch (行 1150):
if req.stream {
    // ...
    let pipe = st.pool.select_pipeline(model());
    return match stream_forward(&pipe, ProtocolKind::OpenAiChat, body.into(), model()).await {
        // ...
    };
}
```

**最小侵入**: `stream_forward(&st.pipeline, ...)` → `stream_forward(&st.pool.select_pipeline(model()), ...)`.

### 2.5 chat_once 改造

```rust
// 旧:
async fn chat_once(
    pipeline: &Arc<Pipeline>,
    req: &OpenAiChatRequest,
    label: usize,
) -> Option<(String, Vec<Value>)> {
    for attempt in 0..3 {
        let normalized = openai_chat_to_normalized(req);
        // ...
        match dispatch(pipeline, ProtocolKind::OpenAiChat, normalized).await {

// 新:
async fn chat_once(
    pool: &Arc<PipelinePool>,
    req: &OpenAiChatRequest,
    label: usize,
) -> Option<(String, Vec<Value>)> {
    for attempt in 0..3 {
        let normalized = openai_chat_to_normalized(req);
        // ...
        match pool_dispatch(pool, ProtocolKind::OpenAiChat, normalized, &req.model).await {
```

**chat_completions 调处**:
```rust
// 旧 (行 1179):
let Some((content, tcs)) = chat_once(&st.pipeline, &req2, rounds).await else { ... };

// 新:
let Some((content, tcs)) = chat_once(&st.pool, &req2, rounds).await else { ... };
```

---

## 3. 改的 List (改哪里 + 改多少行)

| 文件 | 位置 | 改动 | 行数 |
|---|---|---|---|
| `crates/apeireth-companion/examples/companion_serve.rs` | 36-40 | import: 加 `MultiLlmRouter` / `LlmConfig` / `LlmProvider` / `LlmError` | +3 |
| 同上 | 顶部 (新 mod 区) | 新增 `pub struct PipelinePool` + impl | +80 |
| 同上 | 顶部 (新 helper 区) | 新增 `async fn pool_dispatch(...)` | +8 |
| 同上 | 161, 457, 508, 557, 615, 668, 738, 796 | 6 个 LLM impl struct 的 `pipeline: Arc<Pipeline>` → `pool: Arc<PipelinePool>` | 6×1 |
| 同上 | 191, 275, 487, 537, 587, 645, 704, 772, 829 | 9 个 `dispatch(&self.pipeline, ...)` → `pool_dispatch(&self.pool, ..., model()).await` | 9×1 |
| 同上 | 402-415 | `AppState.pipeline: Arc<Pipeline>` → `pool: Arc<PipelinePool>` | 1 |
| 同上 | 1150 | `stream_forward(&st.pipeline, ...)` → `stream_forward(&st.pool.select_pipeline(model()), ...)` | 1 |
| 同上 | 1179 | `chat_once(&st.pipeline, ...)` → `chat_once(&st.pool, ...)` | 1 |
| 同上 | 1291-1295 | `async fn chat_once(pipeline: &Arc<Pipeline>, ...)` → `(pool: &Arc<PipelinePool>, ...)` | 1 |
| 同上 | 1301 | `match dispatch(pipeline, ...)` → `match pool_dispatch(pool, ..., &req.model).await` | 1 |
| 同上 | 1343-1356 | TOML 加载段: 改成构造 PipelinePool (multi or single) | +30 / -10 |
| 同上 | 1397 | `let pipeline = Arc::new(build_pipeline(...))` → `let pool = Arc::new(...构造 PipelinePool)` | -3 / +25 |
| 同上 | 1402, 1458, 1462, 1465, 1468, 1488, 1502, 1533 | 8 个 LLM impl 注入 pipeline → pool | 8×1 |
| 同上 | 1562-1571 | AppState 构造 `pipeline` → `pool` | 1 |
| 同上 | 1810-1993 (新 mod `multi_llm_router_tests`) | 加 6+ 测 | +200 |
| **合计** | | | **~+340 / -10** |

**净改动 ~+330 行**, 95% 是新增 (PipelinePool 抽象 + 测试). 改动点集中, 不分散.

---

## 4. 单元测设计 (6+ 测, 新 mod)

### 4.1 新 mod `multi_llm_router_tests` 位置: `companion_serve.rs:1993` 之后 (或新文件)

```rust
#[cfg(test)]
mod multi_llm_router_tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_clean_env<F: FnOnce()>(f: F) {
        let Ok(_g) = ENV_LOCK.lock() else { f(); return; };
        std::env::remove_var("APEIRETH_LLM_CONFIG");
        std::env::remove_var("APEIRETH_LLM_MODEL");
        std::env::remove_var("APEIRETH_LLM_BASE_URL");
        f();
    }

    /// 测 1: 无 TOML = 单 Pipeline 退化 (1:1 兼容旧版)
    #[test]
    fn pool_single_mode_uses_default_pipeline_when_no_toml() {
        // PipelinePool::single("default", pipe)
        // pool.select_pipeline("any-model") → Arc::clone(&default_pipeline)
        // pipeline count == 1
        // 验证: 跟旧版 Arc<Pipeline> 1:1 行为
    }

    /// 测 2: 有 TOML = 多 Pipeline + MultiLlmRouter 真接
    #[test]
    fn pool_multi_mode_constructs_from_toml_with_router() {
        // 写临时 TOML 文件: 2 provider (apeireth-api + openai-compatible)
        // env 设 APEIRETH_LLM_CONFIG=path
        // 启动路径模拟 → PipelinePool::multi(...)
        // 验证: pipelines.len() == 2, fallback_order == ["apeireth-api", "openai"]
        // router.provider_count() == 2
    }

    /// 测 3: 路由按 model 选 provider (MultiLlmRouter.supports_model 路径)
    #[test]
    fn pool_routes_by_model_to_correct_provider() {
        // 2 pipeline: provider A 支持 model "x", provider B 支持 model "y"
        // pool.select_pipeline("x") → A 的 pipeline
        // pool.select_pipeline("y") → B 的 pipeline
        // pool.select_pipeline("unknown") → default_pipeline (fallback)
    }

    /// 测 4: fallback 链端到端 (真走 MultiLlmRouter.complete)
    #[tokio::test]
    async fn pool_fallback_chain_end_to_end() {
        // 模拟: 第一个 provider 永远 fail (mock provider), 第二个成功
        // 调 LlmProvider trait 的 complete() (不是 dispatch, 走 router 路径)
        // 验证: router 跨第一个失败 → 走第二个 → 成功
        // 注: 走 dispatch 路径不能 fallback (Pipeline 单 endpoint), 这里用 LlmProvider.complete()
    }

    /// 测 5: 健康检查 (router.get_health / pool.provider_names)
    #[test]
    fn pool_health_endpoint_lists_providers() {
        // 多 provider mode → /health endpoint 暴露 provider_names
        // 单 provider mode → provider_names == ["default"]
    }

    /// 测 6: 4 种 provider type 都能 build (apeireth-api / openai-compatible / anthropic-compatible / scripted)
    #[test]
    fn pool_supports_all_4_provider_types() {
        // 写 TOML with 4 provider (各类型 1 个)
        // PipelinePool::multi(...) 构造成功
        // pipelines.len() == 4, 每个 provider pipeline 都存在
    }
}
```

### 4.2 边界测 (可选, +2-4 测)

- **测 7**: TOML 部分 provider key 缺失 → 整体启动失败 (fail-fast) 或 仅可用 provider 入池 (degraded) — **决策点**: 推荐 fail-fast (0 装严守, 不静默)
- **测 8**: TOML 有 0 个 provider → 退化单 Pipeline (不 panic)
- **测 9**: provider name 重名 → TOML 解析期就失败 (HashMap 自动)
- **测 10**: pool.clone() / 多 reader 选同一个 pipeline → Arc 引用计数正确

### 4.3 回归测 (已有 5+5=10 测必须仍过)

- 9 个 `cot_extraction_tests` (extract_minimax_cot)
- 5 个 `llm_config_tests` (env override / TOML first provider)
- 整个 `cargo test -p apeireth-companion --example companion_serve` 跑通

---

## 5. 哪些是 sub-agent 可独立做的 / 哪些必须主线程做

### 5.1 🟢 sub-agent 可独立做 (隔离的单元)

| 任务 | 隔离性 | 备注 |
|---|---|---|
| **PipelinePool 类型 + impl** (新加代码) | ✅ 完全隔离 | 新增文件或新 mod, 不改既有符号 |
| **multi_llm_router_tests 新 mod** | ✅ 完全隔离 | 独立 `#[cfg(test)] mod`, 不影响生产代码 |
| **TOML 4 provider 端到测试** (临时 TOML fixture) | ✅ 完全隔离 | tempdir + temp TOML |
| **provider_supports_model() 简化逻辑** | ✅ 完全隔离 | 内部 fn, 不暴露 |

**适合 sub-agent 范围**: Phase A — "PipelinePool 抽象 + 单测骨架" (新代码 ~150 行 + 6 测).

### 5.2 🟡 必须主线程做 (涉及 LOCKED 接口 / 多处改动协调)

| 任务 | 原因 |
|---|---|
| **改 9 个 dispatch 调用点** (LLM impl 内部) | 涉及多个 struct 的字段类型, 改 9 处签名, 编译期必须 1:1 对齐 |
| **改 chat_once 签名** | `chat_once(&Arc<Pipeline>, ...)` → `(&Arc<PipelinePool>, ...)`, 内部 1 处 + 外部 1 处调用 |
| **改 AppState 结构** | `pipeline: Arc<Pipeline>` → `pool: Arc<PipelinePool>`, 影响 ~30 处引用 (字段访问 / 构造) |
| **改 main() 启动期构造** | 涉及 TOML 加载 + 8 处 LLM impl 注入 + AppState 装配, 单点集中 |
| **改 streaming branch 的 stream_forward** | 1 处但涉及 req.model 字段选取, 需要确认 req.model 总是有值 |

**主线程范围**: Phase B — "改 9 处 dispatch + 改 AppState + 改 main() 装配" (改动既有代码 ~30 处).

### 5.3 🔴 主人决策点 (spec 通过后)

| 决策 | 选项 | 推荐 |
|---|---|---|
| 单 pipeline 模式是否真用 MultiLlmRouter (空壳) | (a) 是 (b) 否 (退化 select_pipeline) | (b) — 简洁, 旧版行为 1:1 |
| provider_supports_model 实现策略 | (a) 真查 LlmProvider.supports_model (b) 全 true (c) 白名单匹配 | (c) v1 简化 — 复用 pipeline 的 1:1 行为, 后续 phase 真接 router 路径 |
| TOML provider key 缺失 fail-fast vs degraded | (a) fail (b) skip + warn | (a) — 0 装严守, 不静默 |
| `select_pipeline(model)` 找不到时 fallback 策略 | (a) default (b) 按 fallback_order 第一个 (c) 报错 | (a) — 与现有 "model 不匹配" 行为一致 (旧 build_pipeline 不过滤) |
| `chat_once` 重试循环是否跨 provider (同一请求多 provider 尝试) | (a) 否 (单 provider 重试) (b) 是 (跨 provider 重试) | (a) v1 简化 — router 自己 fallback, chat_once 只重试 1 个 pipeline |
| 测 4 (fallback 链 E2E) 走 `dispatch` 还是 `LlmProvider.complete` | (a) dispatch (Pipeline 路径) (b) complete (router 路径) | (b) — dispatch 是 Pipeline 单 endpoint, 真 fallback 只能走 router |

---

## 6. 风险点 (PipelinePool 不变量)

### 6.1 不变量清单 (主线程 codereview 必须查)

1. **单 pipeline 模式 1:1 兼容**: `pool.select_pipeline(any_model)` 返回的 `Arc<Pipeline>` 必须跟旧版 `&st.pipeline` 是同一个 Arc 引用 — 验证: 测 1 + 回归测 (9 个 cot_extraction_tests 全过)
2. **多 pipeline 模式路由确定**: 同一 model 多次调 `select_pipeline` 必须返同一 pipeline — 验证: 测 3 + 顺序遍历 fallback_order
3. **Arc 引用计数正确**: 同一 Pipeline 在 `pipelines: HashMap` + `default_pipeline` 可能同时持, 必须 Arc (不是裸 Pipeline) — 验证: 测 1 启动期 pipeline count == 1, Arc::strong_count 检查
4. **TOML 解析失败不 panic**: TOML 错 / 空 / 部分 provider 缺 key → 退化路径, 不 panic — 测 2 + 测 6
5. **streaming branch 兼容**: `req.stream=true` 仍走 `stream_forward`, 但 `select_pipeline(model())` 必须返回与原 pipeline 行为兼容的 pipeline (同 base_url 同 key) — 验证: 手测 + E2E
6. **chat_once 重试循环不跨 provider**: `chat_once` 内 `pool_dispatch` 每次选同一 pipeline, 不跨 provider — 验证: 测 chat_once 行为不变 (3 次重试同 pipeline)
7. **daemon 8 个 LLM impl 走 pool 路由**: 做梦摘要 / 反思 / 涌现 / 提炼 / 摘要 / 经验提炼 / 召回 / 对账 全部走 pool, 跟主链路同策略 — 验证: 启动日志 `[llm] daemon LLM impls use pool: X providers`

### 6.2 已知边界 / 失败模式

| 失败 | 触发条件 | 兜底 |
|---|---|---|
| TOML 解析失败 | 文件不存在 / toml 格式错 | eprintln 退化到 env / default, 不 panic |
| env key 缺失 (provider.api_key_env) | e.g. `OPENAI_API_KEY` 没设 | 启动失败 (fail-fast), 启动期立刻报错 |
| req.model 不在任何 provider.models 列表 | 客户端发 model = "unknown-llm" | `select_pipeline` 退化 default_pipeline, 等于旧行为 |
| 第一个 provider 全 fail, 第二个 fallback | runtime 网络错误 | MultiLlmRouter 内部 retryable 错误触发 fallback (router 已实现) |
| Pipeline 池在 daemon 期间变化 (动态加 provider) | v1 不支持 — TOML 启动期一次性读 | 文档化: "重启 companion_serve 改 provider 配置" |

### 6.3 0 触碰守门

| 0 触碰项 | 验证方法 |
|---|---|
| `apeireth-pipeline/src/**/*.rs` 0 改 | `git diff main..HEAD -- crates/apeireth-pipeline/src/` 空 |
| `apeireth-api/src/llm/{router,config,traits}.rs` 0 改 | `git diff main..HEAD -- crates/apeireth-api/src/llm/` 空 |
| `apeireth-api/src/protocol_handlers.rs` 0 改 | `git diff main..HEAD -- crates/apeireth-api/src/protocol_handlers.rs` 空 |
| `apeireth-llm-iface/src/**/*.rs` 0 改 | `git diff main..HEAD -- crates/apeireth-llm-iface/src/` 空 |
| `Cargo.toml` workspace.version 0 改 | `git diff main..HEAD -- Cargo.toml` 仅允许 `version = "1.2.0"` 0 漂移 |
| `gh_*.ps1` 0 改 | `git diff main..HEAD -- scripts/gh_*.ps1 gh_*.ps1` 空 |
| `apeireth-environment/tests/` 0 改 | `git diff main..HEAD -- crates/apeireth-environment/tests/` 空 |
| `apeireth-provider/tests/` 0 改 | `git diff main..HEAD -- crates/apeireth-provider/tests/` 空 |

---

## 7. 验证清单

### 7.1 编译期

```bash
cargo check -p apeireth-companion --example companion_serve
# 期望: 0 error, 0 warning (除 #[allow(missing_docs)] 等显式)

cargo check --workspace
# 期望: 0 error, 0 new warning (Pipeline / router / config 0 触碰)
```

### 7.2 单元测

```bash
cargo test -p apeireth-companion --example companion_serve
# 期望: 
#   9 cot_extraction_tests 全过 (回归)
#   5 llm_config_tests 全过 (回归)
#   6+ multi_llm_router_tests 全过 (新)

cargo test -p apeireth-api --lib llm::router
# 期望: 5 测全过 (回归, MultiLlmRouter 自己 0 改)

cargo test -p apeireth-api --lib llm::config
# 期望: 4 测全过 (回归, LlmConfig 自己 0 改)
```

### 7.3 集成测 (E2E)

```bash
# 准备: 写 2-provider TOML fixture
cat > /tmp/test-multi.toml <<'EOF'
[providers.apeireth-api]
type = "apeireth-api"
base_url = "https://api.minimaxi.com"
api_key_env = "APEIRETH_API_KEY"
models = ["MiniMax-M3"]

[providers.scripted]
type = "scripted"
api_key_env = "APEIRETH_API_KEY"
scripts = { "hello" = "hi from scripted" }
default_response = "default scripted"
EOF

# 启动
APEIRETH_LLM_CONFIG=/tmp/test-multi.toml APEIRETH_API_KEY=fake-key \
  cargo run -p apeireth-companion --example companion_serve &
SERVER_PID=$!
sleep 3

# 验证 1: /health 暴露 provider 数
curl http://127.0.0.1:8090/health | jq '.features'
# 期望: features 含 "multi_provider", 至少 2 个 provider

# 验证 2: model = "MiniMax-M3" → 走 apeireth-api provider (走 minimaxi URL, 401/403 都行, 不 panic)
curl -X POST http://127.0.0.1:8090/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{"model":"MiniMax-M3","messages":[{"role":"user","content":"hi"}]}'
# 期望: 200 + 正常 OpenAI 响应 或 401 (fake key) — 不 panic

# 验证 3: model = "unknown" → fallback default_pipeline (旧行为)
curl -X POST http://127.0.0.1:8090/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{"model":"unknown-model","messages":[{"role":"user","content":"hi"}]}'
# 期望: 同 401 (fake key) — 不 panic

kill $SERVER_PID
```

### 7.4 行为不变验证 (1:1 兼容旧版)

```bash
# 不设 APEIRETH_LLM_CONFIG → 退化单 Pipeline
APEIRETH_LLM_BASE_URL=https://api.minimaxi.com \
  APEIRETH_API_KEY=fake-key \
  cargo run -p apeireth-companion --example companion_serve &
# 期望: 启动日志 [llm] base_url = https://api.minimaxi.com (旧版 1:1 行为)
# 期望: 启动日志 [llm] pool: single mode (新) — pool 内部, 行为不变
```

---

## 8. 0 触碰自查清单 (逐项 check)

| # | 项目 | 验证命令 | 期望 |
|---|---|---|---|
| 1 | `apeireth-pipeline/src/` 0 改 | `git diff main..HEAD -- crates/apeireth-pipeline/src/` | 空 |
| 2 | `apeireth-api/src/llm/{router,config,traits}.rs` 0 改 | `git diff main..HEAD -- crates/apeireth-api/src/llm/` | 空 |
| 3 | `apeireth-api/src/protocol_handlers.rs` 0 改 | `git diff main..HEAD -- crates/apeireth-api/src/protocol_handlers.rs` | 空 |
| 4 | `apeireth-api/src/llm/providers/` 0 改 | `git diff main..HEAD -- crates/apeireth-api/src/llm/providers/` | 空 |
| 5 | `apeireth-llm-iface/src/` 0 改 | `git diff main..HEAD -- crates/apeireth-llm-iface/src/` | 空 |
| 6 | `Cargo.toml` workspace.version 1.2.0 不变 | `git diff main..HEAD -- Cargo.toml \| grep -i version` | 空 |
| 7 | 24 LOCKED crate src/ 0 改 | `git diff main..HEAD -- crates/apeireth-{core,memory,asi,...}/src/` (24 crate) | 全空 |
| 8 | `gh_*.ps1` 0 改 | `git diff main..HEAD -- '*.ps1' scripts/gh_check.ps1` | 空 |
| 9 | `apeireth-environment/tests/` 0 改 | `git diff main..HEAD -- crates/apeireth-environment/tests/` | 空 |
| 10 | `apeireth-provider/tests/` 0 改 | `git diff main..HEAD -- crates/apeireth-provider/tests/` | 空 |
| 11 | 没引入外部依赖 | `git diff main..HEAD -- '**/Cargo.toml' \| grep -E '^\+[a-z].*=.*"[^"]+"'` | 空 (除 lock dependency 即 `^link = ".*"` 0 行) |
| 12 | 没改 enum / const | `git diff main..HEAD -- crates/apeireth-companion/examples/companion_serve.rs \| grep 'enum\|const '` | 仅限新增 enum/const (不改既有) |
| 13 | 没新增 example | `ls crates/apeireth-companion/examples/` (改动前/后一致) | 数量不变 |

---

## 9. 实施分阶段建议 (主人审阅后)

| Phase | 内容 | 工作量 | sub-agent 适合? |
|---|---|---|---|
| **A** | PipelinePool 类型 + impl + 单测骨架 (~150 行 + 6 测) | 1.5-2h | ✅ 完全可 sub-agent |
| **B** | 改 9 个 LLM impl struct 字段 + dispatch 调用点 (~12 处) | 0.5h | ❌ 主线程 (改既有签名) |
| **C** | 改 AppState + chat_once 签名 + main() 装配 (~15 处) | 0.5h | ❌ 主线程 (启动期装配) |
| **D** | 回归测 + 集成测 + E2E | 0.5-1h | ❌ 主线程 (需要启动 server) |
| **合计** | | **3-4h** | |

**推荐顺序**: A → B → C → D (线性, 不可并行 — B 依赖 A 的类型, C 依赖 A+B 的字段, D 依赖全部).

**Sub-agent 拆分**: Phase A 可独立 sub-agent (1 个) 完成后, 主线程做 B+C+D.

---

## 10. 决策点摘要 (主人拍板)

| # | 决策 | 推荐选项 | 备选 |
|---|---|---|---|
| 1 | 单 pipeline 模式 router 是否启用 | (b) 否 (退化 select_pipeline) | (a) 是 (空壳 router) |
| 2 | provider_supports_model v1 策略 | (c) 白名单匹配 (简化) | (a) 真查 LlmProvider.supports_model (b) 全 true |
| 3 | TOML provider key 缺失 fail-fast vs degraded | (a) fail-fast | (b) skip + warn (degraded) |
| 4 | `select_pipeline(unknown)` fallback | (a) default_pipeline | (b) fallback_order[0] (c) 报错 |
| 5 | `chat_once` 重试跨 provider | (a) 否 (单 provider 重试) | (b) 是 (router 路径) |
| 6 | 测 4 (fallback E2E) 走 `LlmProvider.complete` 还是 `dispatch` | (b) complete (router 路径) | (a) dispatch (Pipeline 路径, 不能 fallback) |
| 7 | 测 6 (4 provider build) 是否全 4 类型都 build 成功 | 是 (apeireth-api + openai-compatible + anthropic-compatible + scripted) | (排除 scripted — 启动期不需要) |
| 8 | 新增边界测 (测 7-10) 范围 | 测 7 (key 缺失) + 测 8 (空 TOML) — **必需** | 测 9 (重名) + 测 10 (Arc 计数) — 可选 |

**注**: 决策 1, 4, 5, 6 主线程会按推荐实现, 但 spec 通过后允许微调.

---

## 11. 参考资料

- `crates/apeireth-api/src/llm/router.rs` (299 行) — `MultiLlmRouter` + 5 单测
- `crates/apeireth-api/src/llm/config.rs` (239 行) — `LlmConfig::from_file` / `build_router` + 4 单测
- `crates/apeireth-api/src/llm/traits.rs` (9 行) — re-export
- `crates/apeireth-api/src/llm/providers/{apeireth_api, openai_compat, anthropic_compat, scripted}.rs` — 4 provider type
- `crates/apeireth-api/src/protocol_handlers.rs` (1997 行):
  - `build_pipeline(base_url, auth_token)` — 行 102
  - `dispatch(pipeline, kind, input)` — 行 907
  - `stream_forward(pipeline, kind, raw_body, model)` — 行 1379
- `crates/apeireth-pipeline/src/lib.rs` (659 行) — Pipeline LOCKED, `Pipeline::new/with_config/run/run_streaming`
- `crates/apeireth-companion/examples/companion_serve.rs` (1993 行) — 唯一改动文件
- `docs/02-guides/custom-llm.md` (178 行) — 用户面文档, §6 标 "V1.1 中期路线"
- `crates/apeireth-llm-iface/src/traits.rs` (568 行) — `LlmProvider` trait (re-export)

---

_2026-08-20 15:09 Mavis spec 完稿, 主人审阅后分阶段实施 (Phase A sub-agent / Phase B+C+D 主线程)._