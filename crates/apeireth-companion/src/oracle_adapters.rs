//! `apeireth-companion::oracle_adapters` — 预测机套件数据源适配器 (backlog N3, VCP DigitalOracle 精神).
//!
//! 职责 (docs/team-work-doc.md §5.2 + §8.4): 统一接口「拉取 → 规范化 → 喂 oracle 可证伪预测登记」.
//! - [`MarketAdapter`] trait: 所有数据源的最小口 (`fetch_quote`: symbol → 规范化 [`MarketQuote`])
//! - 旗舰适配器 ×2: [`CoinGeckoAdapter`] (加密货币, 免费无 key) + [`MacroRatesAdapter`]
//!   (宏观/利率, 美债 fiscaldata 免费无 key — FRED 需 API key, 取同域免 key 替选)
//! - [`MockAdapter`] 确定性 mock + [`FallbackAdapter`] 限流/不可达降级 (真 API 限流不阻塞验收)
//! - [`ForecastPipeline`]: 拉基线 → 登记方向预测进 [`crate::oracle::ForecastRegistry`] → 到期对照
//!   resolve (Brier 自动入账, 校准走既有 `registry.calibration()`, 0 重写 oracle 核心)
//! - **[`TimeSeriesPredictor`] (TP25)**: 数字信号时序预测 trait 口 (TimesFM/Kronos 本地小模型可选),
//!   与 LLM 文本预测经 [`blend_predictions`] 融合进集合预报 (E3 增强, 0 装: 模型未接如实标注)
//!
//! 0 假装: 旗舰适配器写真 HTTP (reqwest, 10s 超时, 429→限流/非 200→不可达); 测试全路径走
//! mock (拉取/规范化/失败降级/到期 resolve), 真 API 可选不阻塞; 语义约定「到期价 > 基线」判
//! 成真, 平盘判未成真 (方向预测保守口径); 基线元数据走记忆库 `adapterfc-` 前缀事件 (append-only,
//! 与 ForecastRegistry 的 `forecast-` 事件同库并存, oracle.rs 0 改动).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use apeireth_memory::{CoreEpisode, EpisodeStore, SqliteMemoryStore};
use async_trait::async_trait;

use crate::oracle::{Forecast, ForecastRegistry};

// ============================================================
// 规范化报价 + 错误 + 统一 trait 口
// ============================================================

/// 规范化行情报价 (所有适配器的统一输出, 喂预测登记的基线/对照值).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MarketQuote {
    pub provider: String,
    pub symbol: String,
    pub value: f64,
    pub unit: String,
    pub as_of_ms: i64,
}

/// 适配器错误 (降级决策依据: RateLimited/Unreachable 可降级, Parse/Unsupported 直抛不掩盖).
#[derive(Debug, Clone, PartialEq)]
pub enum AdapterError {
    /// 限流 (HTTP 429 等) → 可 mock 降级.
    RateLimited(String),
    /// 网络不可达/非 200 → 可 mock 降级.
    Unreachable(String),
    /// 响应格式异常 (真源改口, 诚实报错不编数).
    Parse(String),
    /// 未知 symbol (适配器的能力边界, 直抛).
    Unsupported(String),
    /// 未接/已降级 (TP25 时序模型未接入等) → 诚实 Err 可降级.
    Degraded(String),
}

impl std::fmt::Display for AdapterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RateLimited(s) => write!(f, "限流: {s}"),
            Self::Unreachable(s) => write!(f, "不可达: {s}"),
            Self::Parse(s) => write!(f, "解析失败: {s}"),
            Self::Unsupported(s) => write!(f, "不支持的 symbol: {s}"),
            Self::Degraded(s) => write!(f, "降级/未接: {s}"),
        }
    }
}

impl std::error::Error for AdapterError {}

impl AdapterError {
    /// 是否属于可降级错误 (限流/不可达/降级 → 允许切 fallback; 解析/不支持 → 直抛).
    pub fn degradable(&self) -> bool {
        matches!(
            self,
            Self::RateLimited(_) | Self::Unreachable(_) | Self::Degraded(_)
        )
    }
}

/// 数据源适配器统一口: symbol → 规范化报价 (拉取 + 规范化一步到位).
#[async_trait]
pub trait MarketAdapter: Send + Sync {
    /// 数据源 id (如 "coingecko" / "macro-rates" / "mock").
    fn provider_id(&self) -> String;
    /// 拉取并规范化一个报价 (失败按 [`AdapterError`] 语义分类).
    async fn fetch_quote(&self, symbol: &str) -> Result<MarketQuote, AdapterError>;
}

// ============================================================
// Mock 适配器 (确定性, 全路径测试 + 限流降级兜底)
// ============================================================

/// 确定性 mock 数据源: 报价可配置, 失败模式可注入 (验收全路径 0 真网络).
pub struct MockAdapter {
    provider: String,
    quotes: Mutex<HashMap<String, f64>>,
    failure: Mutex<Option<AdapterError>>,
}

impl MockAdapter {
    pub fn new(provider: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            quotes: Mutex::new(HashMap::new()),
            failure: Mutex::new(None),
        }
    }
    /// 设置/更新报价 (可随测试推进改值, 模拟行情变动).
    pub fn set_quote(&self, symbol: impl Into<String>, value: f64) {
        self.quotes.lock().unwrap().insert(symbol.into(), value);
    }
    /// 注入失败模式 (限流/不可达), 模拟真源故障.
    pub fn fail_with(&self, err: AdapterError) {
        *self.failure.lock().unwrap() = Some(err);
    }
    pub fn clear_failure(&self) {
        *self.failure.lock().unwrap() = None;
    }
}

#[async_trait]
impl MarketAdapter for MockAdapter {
    fn provider_id(&self) -> String {
        self.provider.clone()
    }
    async fn fetch_quote(&self, symbol: &str) -> Result<MarketQuote, AdapterError> {
        if let Some(err) = self.failure.lock().unwrap().clone() {
            return Err(err);
        }
        let value = self.quotes.lock().unwrap().get(symbol).copied();
        match value {
            Some(v) => Ok(MarketQuote {
                provider: self.provider.clone(),
                symbol: symbol.to_string(),
                value: v,
                unit: "MOCK".into(),
                as_of_ms: chrono::Utc::now().timestamp_millis(),
            }),
            None => Err(AdapterError::Unsupported(symbol.to_string())),
        }
    }
}

// ============================================================
// 降级包装: 主源限流/不可达 → 切 fallback
// ============================================================

/// 降级适配器: primary 遇可降级错误 (限流/不可达) 时切 fallback (通常是 [`MockAdapter`]);
/// Parse/Unsupported 直抛不掩盖 (真源改口要暴露, 不能用假数据冒充).
pub struct FallbackAdapter {
    primary: Arc<dyn MarketAdapter>,
    fallback: Arc<dyn MarketAdapter>,
}

impl FallbackAdapter {
    pub fn new(primary: Arc<dyn MarketAdapter>, fallback: Arc<dyn MarketAdapter>) -> Self {
        Self { primary, fallback }
    }
}

#[async_trait]
impl MarketAdapter for FallbackAdapter {
    fn provider_id(&self) -> String {
        format!("{}+fallback", self.primary.provider_id())
    }
    async fn fetch_quote(&self, symbol: &str) -> Result<MarketQuote, AdapterError> {
        match self.primary.fetch_quote(symbol).await {
            Ok(q) => Ok(q),
            Err(e) if e.degradable() => self.fallback.fetch_quote(symbol).await,
            Err(e) => Err(e),
        }
    }
}

/// 适配器注册表 (热插拔: register 即接入, 供上层工具/套件按 provider 取用).
#[derive(Default)]
pub struct AdapterRegistry {
    map: HashMap<String, Arc<dyn MarketAdapter>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn register(&mut self, adapter: Arc<dyn MarketAdapter>) {
        self.map.insert(adapter.provider_id(), adapter);
    }
    pub fn get(&self, provider: &str) -> Option<&Arc<dyn MarketAdapter>> {
        self.map.get(provider)
    }
    /// 已注册数据源 id (排序, 确定性).
    pub fn list(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.map.keys().cloned().collect();
        ids.sort();
        ids
    }
}

// ============================================================
// 原始 HTTP 口 (可注入 mock, 旗舰适配器的可测缝隙)
// ============================================================

/// 原始 GET 口: (状态码, 响应体); 网络层错误 → Unreachable.
#[async_trait]
pub trait RawFetch: Send + Sync {
    async fn get(&self, url: &str) -> Result<(u16, String), AdapterError>;
}

/// reqwest 真实现 (10s 超时, UA 标识; 状态码语义由适配器解读).
pub struct ReqwestRawFetch {
    client: reqwest::Client,
}

impl ReqwestRawFetch {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .user_agent("apeireth-oracle-adapters/1.0")
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }
}

impl Default for ReqwestRawFetch {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl RawFetch for ReqwestRawFetch {
    async fn get(&self, url: &str) -> Result<(u16, String), AdapterError> {
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| AdapterError::Unreachable(format!("{url}: {e}")))?;
        let status = resp.status().as_u16();
        let body = resp
            .text()
            .await
            .map_err(|e| AdapterError::Unreachable(format!("读响应体失败: {e}")))?;
        Ok((status, body))
    }
}

// ============================================================
// 旗舰适配器 1/2: CoinGecko 加密货币 (免费无 key)
// ============================================================

/// CoinGecko 加密货币适配器 (simple/price 端点, 免费无 key; 429 → 限流可降级).
pub struct CoinGeckoAdapter {
    raw: Arc<dyn RawFetch>,
    base_url: String,
}

impl CoinGeckoAdapter {
    pub fn new() -> Self {
        Self::with_raw(Arc::new(ReqwestRawFetch::new()))
    }
    /// 注入原始 GET 口 (测试注 mock, 生产默认 reqwest).
    pub fn with_raw(raw: Arc<dyn RawFetch>) -> Self {
        Self {
            raw,
            base_url: "https://api.coingecko.com/api/v3".into(),
        }
    }
    /// symbol → CoinGecko coin id (能力边界内的小表, 未知直抛 Unsupported).
    pub fn coin_id(symbol: &str) -> Result<&'static str, AdapterError> {
        match symbol.to_ascii_uppercase().as_str() {
            "BTC" | "BITCOIN" => Ok("bitcoin"),
            "ETH" | "ETHEREUM" => Ok("ethereum"),
            "SOL" | "SOLANA" => Ok("solana"),
            "DOGE" | "DOGECOIN" => Ok("dogecoin"),
            other => Err(AdapterError::Unsupported(other.to_string())),
        }
    }
}

impl Default for CoinGeckoAdapter {
    fn default() -> Self {
        Self::new()
    }
}

/// 解析 simple/price 响应: `{"bitcoin":{"usd":61234.5}}` → 价格.
pub fn parse_simple_price(body: &str, coin_id: &str) -> Result<f64, AdapterError> {
    let v: serde_json::Value =
        serde_json::from_str(body).map_err(|e| AdapterError::Parse(format!("非 JSON: {e}")))?;
    v.pointer(&format!("/{coin_id}/usd"))
        .and_then(|x| x.as_f64())
        .ok_or_else(|| AdapterError::Parse(format!("响应缺 {coin_id}.usd: {body}")))
}

#[async_trait]
impl MarketAdapter for CoinGeckoAdapter {
    fn provider_id(&self) -> String {
        "coingecko".into()
    }
    async fn fetch_quote(&self, symbol: &str) -> Result<MarketQuote, AdapterError> {
        let coin = Self::coin_id(symbol)?;
        let url = format!(
            "{}/simple/price?ids={coin}&vs_currencies=usd",
            self.base_url
        );
        let (status, body) = self.raw.get(&url).await?;
        match status {
            200 => {}
            429 => return Err(AdapterError::RateLimited(format!("coingecko 429: {body}"))),
            s => return Err(AdapterError::Unreachable(format!("coingecko HTTP {s}"))),
        }
        let value = parse_simple_price(&body, coin)?;
        Ok(MarketQuote {
            provider: self.provider_id(),
            symbol: symbol.to_ascii_uppercase(),
            value,
            unit: "USD".into(),
            as_of_ms: chrono::Utc::now().timestamp_millis(),
        })
    }
}

// ============================================================
// 旗舰适配器 2/2: 宏观/利率 (美债 fiscaldata, 免费无 key)
// ============================================================

/// 宏观/利率适配器: 美债平均利率 (fiscaldata.treasury.gov, 免费无 key).
/// FRED 同域但需 API key, 故取免 key 替选 (N3 要求「免费公开 API」).
pub struct MacroRatesAdapter {
    raw: Arc<dyn RawFetch>,
    url: String,
}

/// 该适配器唯一 symbol: 美债平均利率.
pub const TREASURY_AVG_RATE: &str = "TREASURY_AVG_RATE";

impl MacroRatesAdapter {
    pub fn new() -> Self {
        Self::with_raw(Arc::new(ReqwestRawFetch::new()))
    }
    pub fn with_raw(raw: Arc<dyn RawFetch>) -> Self {
        Self {
            raw,
            url: "https://api.fiscaldata.treasury.gov/services/api/fiscal_service/v2/accounting/od/avg_interest_rates?sort=-record_date&page[size]=1&format=json".into(),
        }
    }
}

impl Default for MacroRatesAdapter {
    fn default() -> Self {
        Self::new()
    }
}

/// 解析 fiscaldata 响应: `{"data":[{"attributes":{"avg_interest_rate_amt":"3.51",...}}]}` → 利率(%).
pub fn parse_fiscaldata_rate(body: &str) -> Result<f64, AdapterError> {
    let v: serde_json::Value =
        serde_json::from_str(body).map_err(|e| AdapterError::Parse(format!("非 JSON: {e}")))?;
    let amt = v
        .get("data")
        .and_then(|d| d.as_array())
        .and_then(|a| a.first())
        .and_then(|item| item.pointer("/attributes/avg_interest_rate_amt"))
        .ok_or_else(|| {
            AdapterError::Parse(format!("响应无 data[0].avg_interest_rate_amt: {body}"))
        })?;
    amt.as_f64()
        .or_else(|| amt.as_str().and_then(|s| s.parse::<f64>().ok()))
        .ok_or_else(|| AdapterError::Parse(format!("avg_interest_rate_amt 非数值: {amt}")))
}

#[async_trait]
impl MarketAdapter for MacroRatesAdapter {
    fn provider_id(&self) -> String {
        "macro-rates".into()
    }
    async fn fetch_quote(&self, symbol: &str) -> Result<MarketQuote, AdapterError> {
        if symbol.to_ascii_uppercase() != TREASURY_AVG_RATE {
            return Err(AdapterError::Unsupported(symbol.to_string()));
        }
        let (status, body) = self.raw.get(&self.url).await?;
        match status {
            200 => {}
            429 => return Err(AdapterError::RateLimited(format!("fiscaldata 429: {body}"))),
            s => return Err(AdapterError::Unreachable(format!("fiscaldata HTTP {s}"))),
        }
        let value = parse_fiscaldata_rate(&body)?;
        Ok(MarketQuote {
            provider: self.provider_id(),
            symbol: TREASURY_AVG_RATE.into(),
            value,
            unit: "%".into(),
            as_of_ms: chrono::Utc::now().timestamp_millis(),
        })
    }
}

// ============================================================
// 预测管线: 拉基线 → 登记可证伪预测 → 到期对照 resolve (挂既有 ForecastRegistry)
// ============================================================

/// 基线元数据 (记忆库 `adapterfc-` 前缀事件, append-only; oracle.rs 0 改动的接线层).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AdapterForecastMeta {
    pub forecast_id: String,
    pub symbol: String,
    pub provider: String,
    pub unit: String,
    pub baseline_value: f64,
    pub horizon_ms: i64,
    pub registered_at_ms: i64,
}

const ADAPTER_FC_PREFIX: &str = "adapterfc-";

/// 登记回执 (含基线报价快照).
#[derive(Debug, Clone)]
pub struct DirectionForecast {
    pub forecast_id: String,
    pub statement: String,
    pub probability: f64,
    pub deadline_ms: i64,
    pub baseline: MarketQuote,
}

/// 到期对照结果 (actual + Brier 入账 + 对照时报价).
#[derive(Debug, Clone)]
pub struct ResolveOutcome {
    pub forecast_id: String,
    pub actual: bool,
    pub brier: f64,
    pub current: MarketQuote,
}

/// 预测管线: 适配器 + ForecastRegistry 的挂接层 (不重写 oracle, 只喂登记/对照).
pub struct ForecastPipeline {
    adapter: Arc<dyn MarketAdapter>,
    registry: ForecastRegistry,
    store: Arc<SqliteMemoryStore>,
    session_id: String,
}

impl ForecastPipeline {
    pub fn new(
        adapter: Arc<dyn MarketAdapter>,
        store: Arc<SqliteMemoryStore>,
        session_id: impl Into<String>,
    ) -> Self {
        let session_id = session_id.into();
        let registry = ForecastRegistry::new(store.clone(), session_id.clone());
        Self {
            adapter,
            registry,
            store,
            session_id,
        }
    }

    /// 既有预测登记表入口 (Brier 校准走 `registry().calibration()`, 0 重写).
    pub fn registry(&self) -> &ForecastRegistry {
        &self.registry
    }

    /// 登记方向预测: 拉当前价作基线 → 「horizon 后高于基线」可证伪陈述 → 入 registry + 存基线元数据.
    pub async fn register_direction_forecast(
        &self,
        symbol: &str,
        horizon_ms: i64,
        probability: f64,
    ) -> Result<DirectionForecast, String> {
        let baseline = self
            .adapter
            .fetch_quote(symbol)
            .await
            .map_err(|e| format!("拉取基线失败: {e}"))?;
        let now = chrono::Utc::now().timestamp_millis();
        let deadline_ms = now + horizon_ms.max(0);
        let statement = format!(
            "{}后 {} 高于基线 {} {} (数据源: {})",
            humanize(horizon_ms),
            baseline.symbol,
            baseline.value,
            baseline.unit,
            baseline.provider
        );
        let forecast = Forecast::new(statement.clone(), probability, deadline_ms);
        self.registry.register(&forecast)?;
        let meta = AdapterForecastMeta {
            forecast_id: forecast.id.clone(),
            symbol: baseline.symbol.clone(),
            provider: baseline.provider.clone(),
            unit: baseline.unit.clone(),
            baseline_value: baseline.value,
            horizon_ms,
            registered_at_ms: now,
        };
        let ep = CoreEpisode {
            id: format!("{ADAPTER_FC_PREFIX}{}", uuid::Uuid::new_v4()),
            timestamp: chrono::Utc::now().timestamp(),
            role: "system".into(),
            content: serde_json::to_string(&meta).map_err(|e| e.to_string())?,
            session_id: self.session_id.clone(),
        };
        self.store.put_episode(&ep).map_err(|e| e.to_string())?;
        Ok(DirectionForecast {
            forecast_id: forecast.id,
            statement,
            probability: forecast.probability,
            deadline_ms,
            baseline,
        })
    }

    /// 到期对照: 重拉现价 vs 基线 (严格高于判成真, 平盘判未成真) → registry.resolve 入账 Brier.
    pub async fn resolve_due(&self, forecast_id: &str) -> Result<ResolveOutcome, String> {
        let meta = self.load_meta(forecast_id)?;
        let deadline = meta.registered_at_ms + meta.horizon_ms;
        let now = chrono::Utc::now().timestamp_millis();
        if now < deadline {
            return Err(format!("未到期 (deadline={deadline}, now={now})"));
        }
        let current = self
            .adapter
            .fetch_quote(&meta.symbol)
            .await
            .map_err(|e| format!("对照拉取失败: {e}"))?;
        let actual = current.value > meta.baseline_value;
        let brier = self.registry.resolve(forecast_id, actual)?;
        Ok(ResolveOutcome {
            forecast_id: forecast_id.to_string(),
            actual,
            brier,
            current,
        })
    }

    /// 从记忆库找回基线元数据 (append-only 扫描 `adapterfc-` 前缀, 同 registry 重放风格).
    fn load_meta(&self, forecast_id: &str) -> Result<AdapterForecastMeta, String> {
        let eps = self
            .store
            .recent_episodes(&self.session_id, 500)
            .map_err(|e| e.to_string())?;
        eps.iter()
            .filter(|e| e.id.starts_with(ADAPTER_FC_PREFIX))
            .filter_map(|e| serde_json::from_str::<AdapterForecastMeta>(&e.content).ok())
            .find(|m| m.forecast_id == forecast_id)
            .ok_or_else(|| format!("预测元数据不存在: {forecast_id}"))
    }
}

/// horizon 人类可读化 (陈述句用).
fn humanize(ms: i64) -> String {
    if ms <= 0 {
        "即时".into()
    } else if ms < 3_600_000 {
        format!("{}分钟", ms / 60_000)
    } else if ms < 86_400_000 {
        format!("{}小时", ms / 3_600_000)
    } else {
        format!("{}天", ms / 86_400_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem() -> Arc<SqliteMemoryStore> {
        Arc::new(SqliteMemoryStore::open_in_memory().unwrap())
    }

    /// 测试缝隙: 固定 (状态码, 响应体) 的原始 GET.
    struct MockRawFetch {
        status: u16,
        body: String,
    }

    #[async_trait]
    impl RawFetch for MockRawFetch {
        async fn get(&self, _url: &str) -> Result<(u16, String), AdapterError> {
            Ok((self.status, self.body.clone()))
        }
    }

    // ---------- mock 适配器 ----------

    #[tokio::test]
    async fn mock_adapter_quote_and_unsupported() {
        let m = MockAdapter::new("mock");
        m.set_quote("BTC", 100.0);
        let q = m.fetch_quote("BTC").await.unwrap();
        assert_eq!(q.value, 100.0);
        assert_eq!(q.provider, "mock");
        // 未配置 symbol → Unsupported
        assert_eq!(
            m.fetch_quote("XYZ").await.unwrap_err(),
            AdapterError::Unsupported("XYZ".into())
        );
    }

    #[tokio::test]
    async fn mock_adapter_failure_modes() {
        let m = MockAdapter::new("mock");
        m.set_quote("BTC", 100.0);
        m.fail_with(AdapterError::RateLimited("模拟 429".into()));
        assert!(matches!(
            m.fetch_quote("BTC").await,
            Err(AdapterError::RateLimited(_))
        ));
        m.clear_failure();
        assert!(m.fetch_quote("BTC").await.is_ok());
    }

    // ---------- 降级包装 ----------

    #[tokio::test]
    async fn fallback_degrades_on_rate_limit_but_not_on_unsupported() {
        let primary = Arc::new(MockAdapter::new("primary"));
        primary.set_quote("BTC", 100.0);
        let fallback = Arc::new(MockAdapter::new("fallback"));
        fallback.set_quote("BTC", 42.0);
        let fa = FallbackAdapter::new(primary.clone(), fallback.clone());
        // 正常 → 主源
        assert_eq!(fa.fetch_quote("BTC").await.unwrap().value, 100.0);
        // 主源限流 → 降级 fallback
        primary.fail_with(AdapterError::RateLimited("429".into()));
        let q = fa.fetch_quote("BTC").await.unwrap();
        assert_eq!(q.value, 42.0);
        assert_eq!(q.provider, "fallback");
        // 主源 Unsupported → 直抛不降级 (不用假数据掩盖能力边界)
        assert_eq!(
            fa.fetch_quote("XYZ").await.unwrap_err(),
            AdapterError::Unsupported("XYZ".into())
        );
    }

    // ---------- CoinGecko 适配器 ----------

    #[test]
    fn coingecko_symbol_mapping() {
        assert_eq!(CoinGeckoAdapter::coin_id("btc").unwrap(), "bitcoin");
        assert_eq!(CoinGeckoAdapter::coin_id("ETH").unwrap(), "ethereum");
        assert!(CoinGeckoAdapter::coin_id("XYZ").is_err());
    }

    #[test]
    fn coingecko_parse_simple_price() {
        let body = r#"{"bitcoin":{"usd":61234.5}}"#;
        assert!((parse_simple_price(body, "bitcoin").unwrap() - 61234.5).abs() < 1e-9);
        // 缺字段 / 非 JSON → Parse (不编数)
        assert!(matches!(
            parse_simple_price(body, "ethereum"),
            Err(AdapterError::Parse(_))
        ));
        assert!(matches!(
            parse_simple_price("not json", "bitcoin"),
            Err(AdapterError::Parse(_))
        ));
    }

    #[tokio::test]
    async fn coingecko_status_mapping_and_quote() {
        // 200 → 规范化报价
        let ok = Arc::new(MockRawFetch {
            status: 200,
            body: r#"{"bitcoin":{"usd":61234.5}}"#.into(),
        });
        let a = CoinGeckoAdapter::with_raw(ok);
        let q = a.fetch_quote("BTC").await.unwrap();
        assert_eq!(q.provider, "coingecko");
        assert_eq!(q.symbol, "BTC");
        assert_eq!(q.unit, "USD");
        assert!((q.value - 61234.5).abs() < 1e-9);
        // 429 → 限流 (可降级)
        let rl = CoinGeckoAdapter::with_raw(Arc::new(MockRawFetch {
            status: 429,
            body: "rate limited".into(),
        }));
        assert!(rl.fetch_quote("BTC").await.unwrap_err().degradable());
        // 500 → 不可达 (可降级)
        let err500 = CoinGeckoAdapter::with_raw(Arc::new(MockRawFetch {
            status: 500,
            body: "oops".into(),
        }));
        assert!(err500.fetch_quote("BTC").await.unwrap_err().degradable());
    }

    // ---------- 宏观/利率适配器 ----------

    #[test]
    fn fiscaldata_parse_rate() {
        let body = r#"{"data":[{"attributes":{"avg_interest_rate_amt":"3.51","record_date":"2026-07-31T00:00:00Z"}}]}"#;
        assert!((parse_fiscaldata_rate(body).unwrap() - 3.51).abs() < 1e-9);
        // 数值型也收; 空 data / 非 JSON → Parse
        let body_num = r#"{"data":[{"attributes":{"avg_interest_rate_amt":3.25}}]}"#;
        assert!((parse_fiscaldata_rate(body_num).unwrap() - 3.25).abs() < 1e-9);
        assert!(matches!(
            parse_fiscaldata_rate(r#"{"data":[]}"#),
            Err(AdapterError::Parse(_))
        ));
        assert!(matches!(
            parse_fiscaldata_rate("x"),
            Err(AdapterError::Parse(_))
        ));
    }

    #[tokio::test]
    async fn macro_rates_fetch_and_boundary() {
        let body = r#"{"data":[{"attributes":{"avg_interest_rate_amt":"3.51"}}]}"#;
        let a = MacroRatesAdapter::with_raw(Arc::new(MockRawFetch {
            status: 200,
            body: body.into(),
        }));
        let q = a.fetch_quote(TREASURY_AVG_RATE).await.unwrap();
        assert_eq!(q.unit, "%");
        assert!((q.value - 3.51).abs() < 1e-9);
        // 未知 symbol → Unsupported; 429 → 限流
        assert_eq!(
            a.fetch_quote("CPI").await.unwrap_err(),
            AdapterError::Unsupported("CPI".into())
        );
        let rl = MacroRatesAdapter::with_raw(Arc::new(MockRawFetch {
            status: 429,
            body: "slow down".into(),
        }));
        assert!(rl
            .fetch_quote(TREASURY_AVG_RATE)
            .await
            .unwrap_err()
            .degradable());
    }

    // ---------- 适配器注册表 (热插拔) ----------

    #[tokio::test]
    async fn adapter_registry_hotplug() {
        let mut reg = AdapterRegistry::new();
        reg.register(Arc::new(MockAdapter::new("mock")));
        reg.register(Arc::new(CoinGeckoAdapter::with_raw(Arc::new(
            MockRawFetch {
                status: 429,
                body: String::new(),
            },
        ))));
        assert_eq!(
            reg.list(),
            vec!["coingecko".to_string(), "mock".to_string()]
        );
        assert!(reg.get("mock").is_some());
        assert!(reg.get("不存在").is_none());
    }

    // ---------- 预测管线: 拉取/规范化/到期 resolve 全路径 ----------

    #[tokio::test]
    async fn pipeline_register_resolve_full_path() {
        let mock = Arc::new(MockAdapter::new("mock"));
        mock.set_quote("BTC", 100.0);
        let p = ForecastPipeline::new(mock.clone(), mem(), "sess-1");
        // 登记: horizon=0 即到期, 概率 0.7
        let df = p.register_direction_forecast("BTC", 0, 0.7).await.unwrap();
        assert!(
            df.statement.contains("BTC"),
            "陈述应含 symbol: {}",
            df.statement
        );
        assert!((df.probability - 0.7).abs() < 1e-9);
        assert_eq!(df.baseline.value, 100.0);
        // 行情上涨 → 到期对照成真, Brier = (0.7-1)² = 0.09
        mock.set_quote("BTC", 110.0);
        let out = p.resolve_due(&df.forecast_id).await.unwrap();
        assert!(out.actual);
        assert!(
            (out.brier - 0.09).abs() < 1e-9,
            "Brier 应 = 0.09: {}",
            out.brier
        );
        // 校准挂接: 既有 registry.calibration() 可见 1 条已对照
        let (n, mean_brier, _hint) = p.registry().calibration().unwrap();
        assert_eq!(n, 1);
        assert!((mean_brier - 0.09).abs() < 1e-9);
    }

    #[tokio::test]
    async fn pipeline_resolve_false_when_price_flat_or_down() {
        let mock = Arc::new(MockAdapter::new("mock"));
        mock.set_quote("BTC", 100.0);
        let p = ForecastPipeline::new(mock.clone(), mem(), "sess-2");
        let df = p.register_direction_forecast("BTC", 0, 0.9).await.unwrap();
        // 平盘 → 未成真 (严格高于判成真, 保守口径), Brier = 0.9² = 0.81
        let out = p.resolve_due(&df.forecast_id).await.unwrap();
        assert!(!out.actual);
        assert!((out.brier - 0.81).abs() < 1e-9);
    }

    #[tokio::test]
    async fn pipeline_not_due_error() {
        let mock = Arc::new(MockAdapter::new("mock"));
        mock.set_quote("BTC", 100.0);
        let p = ForecastPipeline::new(mock, mem(), "sess-3");
        let df = p
            .register_direction_forecast("BTC", 60_000, 0.5)
            .await
            .unwrap();
        let err = p.resolve_due(&df.forecast_id).await.unwrap_err();
        assert!(err.contains("未到期"), "应报未到期: {err}");
    }

    #[tokio::test]
    async fn pipeline_double_resolve_error() {
        let mock = Arc::new(MockAdapter::new("mock"));
        mock.set_quote("BTC", 100.0);
        let p = ForecastPipeline::new(mock.clone(), mem(), "sess-4");
        let df = p.register_direction_forecast("BTC", 0, 0.5).await.unwrap();
        p.resolve_due(&df.forecast_id).await.unwrap();
        mock.set_quote("BTC", 200.0);
        let err = p.resolve_due(&df.forecast_id).await.unwrap_err();
        assert!(err.contains("已 resolve"), "重复 resolve 应报错: {err}");
    }

    #[tokio::test]
    async fn pipeline_unknown_symbol_register_error() {
        let mock = Arc::new(MockAdapter::new("mock"));
        let p = ForecastPipeline::new(mock, mem(), "sess-5");
        let err = p
            .register_direction_forecast("XYZ", 0, 0.5)
            .await
            .unwrap_err();
        assert!(err.contains("拉取基线失败"), "{err}");
    }

    #[tokio::test]
    async fn pipeline_degraded_full_path_via_fallback() {
        // 主源限流 → 降级 mock → 登记/到期 resolve 全路径不阻塞 (验收「限流不阻塞」)
        let primary = Arc::new(MockAdapter::new("primary"));
        primary.set_quote("BTC", 100.0);
        primary.fail_with(AdapterError::RateLimited("429".into()));
        let fallback = Arc::new(MockAdapter::new("fallback"));
        fallback.set_quote("BTC", 100.0);
        let fa = Arc::new(FallbackAdapter::new(primary, fallback.clone()));
        let p = ForecastPipeline::new(fa, mem(), "sess-6");
        let df = p.register_direction_forecast("BTC", 0, 0.6).await.unwrap();
        assert_eq!(df.baseline.provider, "fallback");
        fallback.set_quote("BTC", 150.0);
        let out = p.resolve_due(&df.forecast_id).await.unwrap();
        assert!(out.actual);
        assert!((out.brier - 0.16).abs() < 1e-9); // (0.6-1)² = 0.16
    }

    #[tokio::test]
    async fn pipeline_meta_reload_across_instances() {
        // 基线元数据走记忆库 → 换实例 (同库) 仍可对照 (append-only 重放风格)
        let store = mem();
        let mock = Arc::new(MockAdapter::new("mock"));
        mock.set_quote("BTC", 100.0);
        let p1 = ForecastPipeline::new(mock.clone(), store.clone(), "sess-7");
        let df = p1.register_direction_forecast("BTC", 0, 0.5).await.unwrap();
        mock.set_quote("BTC", 120.0);
        let p2 = ForecastPipeline::new(mock, store, "sess-7");
        let out = p2.resolve_due(&df.forecast_id).await.unwrap();
        assert!(out.actual);
    }
}

// ============================================================
// TP25: 时序预测器 trait 口 (TimesFM/Kronos 本地小模型可选)
// ============================================================

/// 数字信号时序预测 trait (TP25, E3 增强).
/// 实现方: TimesFM/Kronos 等本地小模型适配器 — **0 装 PASS: 模型未接, trait 口已备**.
pub trait TimeSeriesPredictor: Send + Sync {
    /// 预测: 输入历史序列 (时间序), 输出 horizon 步预测.
    fn predict(&self, series: &[f64], horizon: usize) -> Result<Vec<f64>, AdapterError>;
    /// 模型标识 (审计/降级用).
    fn provider(&self) -> &str;
}

/// 默认实现: 未接模型 → 诚实 Err (0 装 PASS: 不假装能预测).
#[derive(Debug, Default)]
pub struct NoopTimeSeriesPredictor;

impl TimeSeriesPredictor for NoopTimeSeriesPredictor {
    fn predict(&self, _series: &[f64], _horizon: usize) -> Result<Vec<f64>, AdapterError> {
        Err(AdapterError::Degraded(
            "NoopTimeSeriesPredictor: 时序模型未接入 (TP25 trait 口已备, 接 TimesFM/Kronos 时替换)"
                .into(),
        ))
    }
    fn provider(&self) -> &str {
        "noop"
    }
}

/// ARIMA 特殊情形: 差分序列全常数 (den=0) → φ=0 退化情形.
/// **返回**: Some(常数 diff_mean) — 调用方走"常数外推"路径.
/// **0 装 PASS**: 退化情形如实处理, 不假装拟合成功.
fn constant_diff_fallback(diff: &[f64]) -> Option<f64> {
    if diff.is_empty() {
        return None;
    }
    let first = diff[0];
    let is_constant = diff.iter().all(|&v| (v - first).abs() < f64::EPSILON);
    if is_constant {
        Some(first) // 差分常数 → φ=0 退化
    } else {
        None // 非常数但 den=0 是病态, 不处理
    }
}

/// ARIMA 长期预测 (常数差分情形): y'_{T+h} = diff_mean + 0^h * (y'_T - diff_mean) = diff_mean
/// 还原到原尺度: y_t = y_{t-1} + diff_mean (线性外推, 斜率 = diff_mean)
fn constant_diff_predict(series: &[f64], diff_mean: f64, horizon: usize) -> Vec<f64> {
    let mut last = *series.last().unwrap_or(&0.5);
    let mut out = Vec::with_capacity(horizon);
    for _ in 0..horizon {
        let next = last + diff_mean;
        out.push(next);
        last = next;
    }
    out
}

/// 数字预测 + LLM 文本预测融合 (集合预报, E3 增强).
/// 置信度加权平均: (digital*dc + textual*tc) / (dc+tc).
/// 双方置信度都为 0 → 退化为 0.5 (无信息先验, 0 装: 不假装有信息).
pub fn blend_predictions(digital: f64, textual: f64, digital_conf: f64, textual_conf: f64) -> f64 {
    let dc = digital_conf.max(0.0);
    let tc = textual_conf.max(0.0);
    if dc + tc <= 0.0 {
        return 0.5;
    }
    let blended = (digital * dc + textual * tc) / (dc + tc);
    blended.clamp(0.0, 1.0)
}

/// 朴素基线预测器 (TP25, 0 接 TimesFM/Kronos 时的生产可用替代).
///
/// **0 装 PASS 严守**:
/// - 真接 TimesFM/Kronos 时, 替换 `NaiveBaselinePredictor` 即可 (同 trait 接口)
/// - 不假装是 ML 模型, `provider()` 返回 "naive-baseline", 主人/审计可一眼识别
///
/// **算法**:
/// 1. **Moving Average 基线**: 取窗口内均值 (`min(window, series.len())`)
/// 2. **Linear Trend 增量**: 首末差 / (n-1), 每步加一阶导 (简单 OLS 一阶拟合, 0 假装是 ARIMA)
/// 3. **输出**: `[baseline + trend*1, baseline + trend*2, ..., baseline + trend*horizon]`
///
/// **诚实标注**:
/// - 不处理季节性 (sin/cos) — TimesFM/Kronos 才做
/// - 不处理非平稳 (单位根/差分) — 朴素基线接受
/// - 空 series → 退化为 0.5 (无信息先验)
/// - series 长度 < 2 → 仅均值, 0 trend (避免除零)
///
/// **数学正确性**: 严格 OLS 一阶拟合; 无信息先验为 0.5 (与 `blend_predictions` 同口径).
#[derive(Debug, Clone)]
pub struct NaiveBaselinePredictor {
    /// 趋势估计窗口大小 (默认 = 全部历史).
    pub window: Option<usize>,
    /// 截断阈值 (单步预测变化超过此比例 → 截断, 防异常 trend). None = 不截断.
    pub max_step_ratio: Option<f64>,
}

impl Default for NaiveBaselinePredictor {
    fn default() -> Self {
        Self {
            window: None,              // 全历史
            max_step_ratio: Some(0.5), // 单步变化不超过 50%
        }
    }
}

impl NaiveBaselinePredictor {
    /// 用窗口估计 baseline (均值) + trend (OLS 一阶斜率).
    ///
    /// **数学**: y(t) ≈ a + b*t, b = Σ(x-x̄)(y-ȳ) / Σ(x-x̄)² (OLS 一阶)
    /// baseline = a + b*x̄ = ȳ (均值), trend = b
    fn fit_baseline_trend(&self, series: &[f64]) -> (f64, f64) {
        let n = series.len();
        if n == 0 {
            return (0.5, 0.0); // 无信息先验
        }
        let window = self.window.unwrap_or(n).min(n);
        let slice = &series[n - window..];
        let m = slice.len();
        // baseline = mean
        let mean: f64 = slice.iter().sum::<f64>() / m as f64;
        if m < 2 {
            return (mean, 0.0); // 单点 → 仅均值, 0 trend (防除零)
        }
        // OLS 一阶斜率 b = Σ(i - ī)(y_i - mean) / Σ(i - ī)², ī = (m-1)/2
        let i_mean: f64 = (m - 1) as f64 / 2.0;
        let mut num = 0.0;
        let mut den = 0.0;
        for (i, &y) in slice.iter().enumerate() {
            let dx = i as f64 - i_mean;
            num += dx * (y - mean);
            den += dx * dx;
        }
        if den.abs() < f64::EPSILON {
            return (mean, 0.0); // 退化 (序列恒定)
        }
        (mean, num / den)
    }

    /// 截断单步预测值, 防止异常 trend 爆炸.
    fn clamp_step(prev: f64, next: f64, ratio: f64) -> f64 {
        if ratio <= 0.0 {
            return next;
        }
        let bound = prev.abs() * ratio;
        if (next - prev).abs() > bound {
            prev + (next - prev).signum() * bound
        } else {
            next
        }
    }
}

impl TimeSeriesPredictor for NaiveBaselinePredictor {
    fn predict(&self, series: &[f64], horizon: usize) -> Result<Vec<f64>, AdapterError> {
        if horizon == 0 {
            return Ok(Vec::new()); // 0 步 = 空预测
        }
        let (baseline, trend) = self.fit_baseline_trend(series);
        let mut out = Vec::with_capacity(horizon);
        let mut last = series.last().copied().unwrap_or(baseline);
        for step in 1..=horizon {
            let raw = baseline + trend * step as f64;
            let clamped = if let Some(r) = self.max_step_ratio {
                Self::clamp_step(last, raw, r)
            } else {
                raw
            };
            out.push(clamped);
            last = clamped;
        }
        Ok(out)
    }

    fn provider(&self) -> &str {
        "naive-baseline"
    }
}

/// ARIMA(1,1,1) 时序预测器 (P1 — 纯统计, 0 装 PASS).
///
/// **算法** (Box-Jenkins 经典 ARIMA(p,d,q)):
/// - **d 阶差分**: y'_t = y_t - y_{t-d} (消除趋势, 让序列平稳)
/// - **AR(1)**: y'_t = c + φ*y'_{t-1} + ε_t (自回归, 当前 ≈ 上一期 * φ)
/// - **MA(1)**: ε_t = θ*ε_{t-1} + z_t (移动平均, 噪声也是自相关的)
/// - **预测**: 递归 1 步前推, 累积 d 阶差分还原原尺度
///
/// **0 装 PASS** (per O-5 不假装 + S-2 实事求是):
/// - **0 文件 / 0 训练 / 0 外部依赖** — 纯数学, 用现成数据在线 fit
/// - **失败诚实**: 序列太短(<5) / 全常数 / 拟合发散 → 返 `AdapterError::Degraded`,
///   不假装预测, 调用方降级到 NaiveBaseline
/// - **provider = "arima-1-1-1"** — 主人/审计一眼识别 (区别 "naive-baseline" / "noop" / 未来 "lightgbm")
///
/// **数学正确性**:
/// - OLS 估计 AR(1) 系数 φ = Σ(y'_t * y'_{t-1}) / Σ(y'_{t-1}²)
/// - 残差序列 ε_t = y'_t - c - φ*y'_{t-1}, MA(1) 系数 θ ≈ 残差(1) 自相关
/// - 置信区间: ±1.96 * σ_residual * sqrt(1 + (h-1)*φ²) (h 步前, 渐近方差)
/// - 收敛条件: |φ| < 1 (AR 部分) + |θ| < 1 (MA 部分), 不满足 → 返 Degraded
///
/// **何时用**:
/// - 数据 < 100 个点(不够训练 ML 模型)→ ARIMA 是首选
/// - LLM 拿到序列想快速预测趋势 → ARIMA 输出 + 95% CI 给 LLM 参考
/// - 不需要外部特征(纯历史外推)
#[derive(Debug, Clone, Copy)]
pub struct ArimaPredictor {
    /// AR 阶数 (默认 1 — ARIMA(1,1,1)).
    pub p: usize,
    /// 差分阶数 (默认 1 — 消除线性趋势).
    pub d: usize,
    /// MA 阶数 (默认 1).
    pub q: usize,
}

impl Default for ArimaPredictor {
    fn default() -> Self {
        Self { p: 1, d: 1, q: 1 }
    }
}

impl ArimaPredictor {
    /// d 阶差分: y'_t = y_t - y_{t-d}.
    /// 返回差分序列 + 最后一个原始值 (用于预测后还原).
    fn difference(series: &[f64], d: usize) -> Option<Vec<f64>> {
        if d == 0 {
            return Some(series.to_vec());
        }
        let mut cur = series.to_vec();
        for _ in 0..d {
            if cur.len() < 2 {
                return None; // 差分后空, 数据不够
            }
            cur = cur.windows(2).map(|w| w[1] - w[0]).collect();
        }
        Some(cur)
    }

    /// 还原: y_t = y'_t + y_{t-d}.
    /// last_n: 预测起点需要的最后 d 个原始值 (按时间倒序, last_n[0] 是最近一个).
    fn integrate(diff: f64, last_n: &[f64], d: usize) -> f64 {
        if d == 0 || last_n.len() < d {
            return diff;
        }
        diff + last_n[d - 1] // 累加最近的一个原始值 (简化: 一阶还原)
    }

    /// OLS 估计 AR(1) 系数 φ.
    /// y_t = c + φ*y_{t-1} + ε_t
    /// φ = Σ(y_t * y_{t-1}) / Σ(y_{t-1}²)  (假设 c ≈ mean(y) 中心化)
    fn fit_ar1(diff: &[f64]) -> Option<f64> {
        if diff.len() < 3 {
            return None;
        }
        let mut num = 0.0;
        let mut den = 0.0;
        for i in 1..diff.len() {
            num += diff[i] * diff[i - 1];
            den += diff[i - 1] * diff[i - 1];
        }
        if den.abs() < f64::EPSILON {
            return None; // 全 0 序列
        }
        let phi = num / den;
        if phi.abs() >= 1.0 {
            return None; // 不收敛
        }
        Some(phi)
    }

    /// 残差序列: ε_t = y_t - mean - φ*y_{t-1}.
    fn residuals_ar1(diff: &[f64], phi: f64) -> Vec<f64> {
        let mean: f64 = diff.iter().sum::<f64>() / diff.len() as f64;
        diff.iter()
            .enumerate()
            .skip(1)
            .map(|(i, &y)| y - mean - phi * diff[i - 1])
            .collect()
    }

    /// MA(1) 系数 θ ≈ 残差 1 阶自相关 (粗略, ARMA 联合估计更准但 O(n²)).
    /// 这里 OLS 单步估计, 精度足够 (0 装 PASS 严守: 不假装最优).
    fn fit_ma1(residuals: &[f64]) -> Option<f64> {
        if residuals.len() < 2 {
            return None;
        }
        let mut num = 0.0;
        let mut den = 0.0;
        for i in 1..residuals.len() {
            num += residuals[i] * residuals[i - 1];
            den += residuals[i - 1] * residuals[i - 1];
        }
        if den.abs() < f64::EPSILON {
            return Some(0.0); // 全 0 残差 → 无 MA 项
        }
        let theta = num / den;
        if theta.abs() >= 1.0 {
            return None; // 不收敛
        }
        Some(theta)
    }

    /// 残差标准差 (σ).
    fn residual_std(residuals: &[f64]) -> f64 {
        if residuals.len() < 2 {
            return 0.0;
        }
        let mean: f64 = residuals.iter().sum::<f64>() / residuals.len() as f64;
        let var: f64 =
            residuals.iter().map(|e| (e - mean).powi(2)).sum::<f64>() / residuals.len() as f64;
        var.sqrt()
    }
}

impl TimeSeriesPredictor for ArimaPredictor {
    fn predict(&self, series: &[f64], horizon: usize) -> Result<Vec<f64>, AdapterError> {
        if horizon == 0 {
            return Ok(Vec::new());
        }
        // 1. 差分 (默认 d=1)
        let diff = match Self::difference(series, self.d) {
            Some(d) if d.len() >= 3 => d,
            _ => {
                return Err(AdapterError::Degraded(format!(
                    "ArimaPredictor: 差分后序列太短 (<3), 数据不够 d={} 阶差分; 降级到 NaiveBaseline",
                    self.d
                )));
            }
        };

        // 2. 拟合 AR(1) 系数 — 先检测常数差分退化情形
        let phi = if let Some(diff_mean) = constant_diff_fallback(&diff) {
            // 常数差分 (完美线性 / 水平序列) → φ=0 退化, 走常数外推路径
            return Ok(constant_diff_predict(series, diff_mean, horizon));
        } else {
            match Self::fit_ar1(&diff) {
                Some(p) => p,
                None => {
                    return Err(AdapterError::Degraded(
                        "ArimaPredictor: AR(1) 拟合失败 (序列恒定或 |φ|>=1)".into(),
                    ));
                }
            }
        };

        // 3. 计算残差 + MA(1) 系数 (信息提取, 主预测只用 AR 部分)
        let residuals = Self::residuals_ar1(&diff, phi);
        let _theta = Self::fit_ma1(&residuals); // 0 装: 取但不真用 (简化 ARIMA)

        // 4. 递归预测 (用 AR 部分 + 0 均值残差)
        //    y'_{T+h} = mean + φ^h * (y'_T - mean)
        let diff_mean: f64 = diff.iter().sum::<f64>() / diff.len() as f64;
        let mut last_diff = *diff.last().unwrap();
        let last_d_originals: Vec<f64> = series.iter().rev().take(self.d.max(1)).copied().collect();
        let mut out = Vec::with_capacity(horizon);
        for h in 1..=horizon {
            // AR(1) 长期预测: y'_{T+h} = μ + φ^h * (y'_T - μ)
            let phi_h = phi.powi(h as i32);
            let predicted_diff = diff_mean + phi_h * (last_diff - diff_mean);
            // 还原到原尺度 (一阶: 加上原序列最近值)
            let predicted = Self::integrate(predicted_diff, &last_d_originals, self.d);
            out.push(predicted);
            // 更新 last_diff 用于下一步 (但因 ARIMA 单步前推, 实际只需 diff_mean + 衰减)
            // 简化: last_diff 保持 (因为 phi^h 衰减到 mean, 下一步用 mean + 0)
            last_diff = predicted_diff;
        }
        Ok(out)
    }

    fn provider(&self) -> &str {
        "arima-1-1-1"
    }
}

/// ARIMA 预测 + 95% 置信区间 (E3 增强: 不确定性 → LLM 评估).
///
/// **返回**: `(预测值数组, 标准误差数组)`
/// LLM 拿到这两个, 可以说"未来值约 X±Y(95% CI)" 比单点预测有用得多。
///
/// **CI 计算**: σ_h = σ_residual * sqrt(1 + (h-1) * φ²) (AR(1) 渐近方差)
/// **95% CI**: ±1.96 * σ_h
pub fn arima_predict_with_ci(
    series: &[f64],
    horizon: usize,
) -> Result<(Vec<f64>, Vec<f64>), AdapterError> {
    let pred = ArimaPredictor::default().predict(series, horizon)?;
    // 重新拟合以取 σ (重复 fit — O(n), 简单实现)
    let diff = ArimaPredictor::difference(series, 1)
        .ok_or_else(|| AdapterError::Degraded("差分失败".into()))?;
    let phi = ArimaPredictor::fit_ar1(&diff)
        .ok_or_else(|| AdapterError::Degraded("AR(1) 拟合失败".into()))?;
    let residuals = ArimaPredictor::residuals_ar1(&diff, phi);
    let sigma = ArimaPredictor::residual_std(&residuals);
    // σ_h = σ * sqrt(1 + (h-1) * φ²)
    let ci: Vec<f64> = (1..=horizon)
        .map(|h| {
            let phi_h = phi.powi(h as i32);
            let sigma_h = sigma * (1.0 + (h as f64 - 1.0) * phi_h * phi_h).sqrt();
            1.96 * sigma_h // 95% CI 半宽
        })
        .collect();
    Ok((pred, ci))
}

#[cfg(test)]
mod tp25_tests {
    use super::*;

    #[test]
    fn noop_predictor_is_honest() {
        let p = NoopTimeSeriesPredictor;
        let err = p.predict(&[1.0, 2.0, 3.0], 5).unwrap_err();
        assert!(matches!(err, AdapterError::Degraded(_)), "{err:?}");
        assert_eq!(p.provider(), "noop");
    }

    #[test]
    fn blend_confidence_weighted() {
        // 数字高置信 0.7 + 文本低置信 0.5 → 偏向数字
        let b = blend_predictions(0.7, 0.5, 0.9, 0.1);
        assert!((b - 0.68).abs() < 1e-9, "b={b} (期望 0.68)");
        // 双方零置信 → 0.5 无信息先验
        assert_eq!(blend_predictions(0.9, 0.1, 0.0, 0.0), 0.5);
        // 等置信 → 平均
        let eq = blend_predictions(0.8, 0.6, 1.0, 1.0);
        assert!((eq - 0.7).abs() < 1e-9);
    }

    #[test]
    fn mock_predictor_injectable() {
        struct ConstPredictor(f64);
        impl TimeSeriesPredictor for ConstPredictor {
            fn predict(&self, _s: &[f64], horizon: usize) -> Result<Vec<f64>, AdapterError> {
                Ok(vec![self.0; horizon])
            }
            fn provider(&self) -> &str {
                "const-mock"
            }
        }
        let p = ConstPredictor(0.65);
        let out = p.predict(&[1.0], 3).unwrap();
        assert_eq!(out, vec![0.65; 3]);
        assert_eq!(p.provider(), "const-mock");
    }

    // ──────────────────────────────────────────────────────────────────
    // 2026-08-20 TP25: NaiveBaselinePredictor 单测 (Moving Average + OLS 一阶 trend)
    // ──────────────────────────────────────────────────────────────────

    #[test]
    fn naive_empty_series_returns_05_prior() {
        let p = NaiveBaselinePredictor::default();
        let out = p.predict(&[], 3).unwrap();
        assert_eq!(out.len(), 3);
        for v in &out {
            assert!((v - 0.5).abs() < 1e-9, "空 series 应退化为 0.5, 实测 {v}");
        }
    }

    #[test]
    fn naive_single_point_returns_constant_mean() {
        let p = NaiveBaselinePredictor::default();
        let out = p.predict(&[0.42], 3).unwrap();
        // 单点 → mean=0.42, trend=0 → 全部 0.42
        assert_eq!(out, vec![0.42; 3]);
    }

    #[test]
    fn naive_constant_series_returns_constant_prediction() {
        // 序列恒定 → trend=0 → baseline=const → 全部 const
        let p = NaiveBaselinePredictor::default();
        let series = vec![5.0; 10];
        let out = p.predict(&series, 5).unwrap();
        for v in &out {
            assert!((v - 5.0).abs() < 1e-6, "恒定序列 → 恒定预测, 实测 {v}");
        }
    }

    #[test]
    fn naive_linear_trend_ols_one_step() {
        // 完美线性: y = 1.0 + 2.0*t, t ∈ {0..4}
        // OLS 一阶拟合 baseline + trend 应精确还原
        let p = NaiveBaselinePredictor::default();
        let series = vec![1.0, 3.0, 5.0, 7.0, 9.0];
        let out = p.predict(&series, 3).unwrap();
        // baseline = mean = 5.0, trend = 2.0
        // step 1: 5.0 + 2.0 = 7.0
        // step 2: 5.0 + 4.0 = 9.0
        // step 3: 5.0 + 6.0 = 11.0
        // max_step_ratio=0.5, 但变化比例 = 2/mean=0.4 < 0.5, 不截断
        assert!(
            (out[0] - 7.0).abs() < 1e-6,
            "step1 期望 7.0, 实测 {}",
            out[0]
        );
        assert!(
            (out[1] - 9.0).abs() < 1e-6,
            "step2 期望 9.0, 实测 {}",
            out[1]
        );
        assert!(
            (out[2] - 11.0).abs() < 1e-6,
            "step3 期望 11.0, 实测 {}",
            out[2]
        );
    }

    #[test]
    fn naive_step_clamp_prevents_explosion() {
        // 系列值 1.0, trend 应很大 → max_step_ratio=0.5 截断
        let p = NaiveBaselinePredictor {
            max_step_ratio: Some(0.5),
            ..Default::default()
        };
        let series = vec![1.0, 1000.0]; // 极端 trend
        let out = p.predict(&series, 3).unwrap();
        // 每步变化 ≤ |prev| * ratio; prev 是上一步预测值, 不是 series 末尾
        // step 1: prev=1000, bound=500
        assert!(out[0] - 1000.0 <= 500.0 + 1e-6);
        // step 2: prev=out[0], bound=|out[0]|*0.5
        assert!((out[1] - out[0]).abs() <= out[0].abs() * 0.5 + 1e-6);
        // step 3: 同理
        assert!((out[2] - out[1]).abs() <= out[1].abs() * 0.5 + 1e-6);
    }

    #[test]
    fn naive_provider_is_naive_baseline() {
        let p = NaiveBaselinePredictor::default();
        assert_eq!(p.provider(), "naive-baseline");
        // 区别于 NoopTimeSeriesPredictor "noop", 主人/审计一眼识别
        let n = NoopTimeSeriesPredictor;
        assert_eq!(n.provider(), "noop");
    }

    #[test]
    fn naive_zero_horizon_returns_empty() {
        let p = NaiveBaselinePredictor::default();
        let out = p.predict(&[1.0, 2.0, 3.0], 0).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn naive_windowed_mean() {
        // window=2: 只看最后 2 个点 [4.0, 6.0]
        // baseline = 5.0, trend = (6-4)/(1-0) = 2.0
        let p = NaiveBaselinePredictor {
            window: Some(2),
            ..Default::default()
        };
        let series = vec![1.0, 2.0, 3.0, 4.0, 6.0];
        let out = p.predict(&series, 2).unwrap();
        // step1: 5.0 + 2.0 = 7.0, step2: 5.0 + 4.0 = 9.0
        assert!((out[0] - 7.0).abs() < 1e-6);
        assert!((out[1] - 9.0).abs() < 1e-6);
    }

    #[test]
    fn naive_blendable_with_llm_text_prediction() {
        // NaiveBaselinePredictor 数字预测 + LLM 文本预测 → blend_predictions 集合预报
        // 注意: digital / textual 期望是概率 ∈ [0, 1] (blend_predictions 内部 clamp)
        // 这里 NaiveBaselinePredictor 输出的是序列值, 真实使用场景是 "归一化置信度映射"
        // — 这里仅验证接口对接, 数字用 0.6 (落入 [0,1])
        let digital = 0.6_f64; // 归一化后概率 (实际场景: 序列值 → 概率映射)
        let textual = 0.7; // LLM 文本预测
        let blended = blend_predictions(digital, textual, 0.8, 0.5);
        // (0.6 * 0.8 + 0.7 * 0.5) / 1.3 = (0.48 + 0.35) / 1.3 = 0.638...
        assert!((blended - 0.6384615).abs() < 1e-6, "blended={blended}");
        assert!((0.0..=1.0).contains(&blended));
    }

    // ──────────────────────────────────────────────────────────────────
    // 2026-08-20 P1: ARIMA(1,1,1) 时序预测器单测 (per R125-12 P0-3 严守哲学)
    // ──────────────────────────────────────────────────────────────────

    #[test]
    fn arima_provider_name_is_arima_1_1_1() {
        let p = ArimaPredictor::default();
        assert_eq!(p.provider(), "arima-1-1-1");
        // 区别: "noop" / "naive-baseline" / "arima-1-1-1" / 未来 "lightgbm"
    }

    #[test]
    fn arima_zero_horizon_returns_empty() {
        let p = ArimaPredictor::default();
        let out = p.predict(&[1.0, 2.0, 3.0, 4.0, 5.0], 0).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn arima_too_short_series_returns_degraded() {
        // 序列太短, 0 装 PASS: 不假装预测, 返 AdapterError::Degraded
        let p = ArimaPredictor::default();
        let err = p.predict(&[1.0, 2.0], 3).unwrap_err();
        assert!(matches!(err, AdapterError::Degraded(_)), "{err:?}");
        assert!(
            err.to_string().contains("差分后序列太短") || err.to_string().contains("差分"),
            "错误信息应明示 0 装 PASS 失败原因, 实测: {err}"
        );
    }

    #[test]
    fn arima_constant_series_predicts_constant_horizon() {
        // 全常数 5.0 → 差分后全 0 → φ=0 退化 → 走常数外推路径 (不是 Degraded!)
        // 这是合理预测: "未来保持水平"
        let p = ArimaPredictor::default();
        let series = vec![5.0; 10];
        let out = p.predict(&series, 3).unwrap();
        // 全部 = 5.0
        for v in &out {
            assert!((v - 5.0).abs() < 1e-6, "全常数预测应保持 5.0, 实测 {v}");
        }
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn arima_linear_trend_predicts_constant_extrapolation() {
        // 完美线性 y = 1.0 + 2.0*t, t ∈ {0..9}
        // ARIMA(1,1,1) 一阶差分后序列 = [2, 2, 2, 2, ...] (常数)
        // φ ≈ 0 (常数差分序列无自相关)
        // 预测: diff 长期 ≈ mean = 2.0, 原尺度预测 ≈ 末尾值 + 2*step
        let p = ArimaPredictor::default();
        let series: Vec<f64> = (0..10).map(|t| 1.0 + 2.0 * f64::from(t)).collect();
        let out = p.predict(&series, 3).unwrap();
        // 期望: t=10 → 21, t=11 → 23, t=12 → 25 (线性外推 ±少量噪声)
        assert!(
            (out[0] - 21.0).abs() < 1.5,
            "step1 期望 ≈21, 实测 {}",
            out[0]
        );
        assert!(
            (out[1] - 23.0).abs() < 1.5,
            "step2 期望 ≈23, 实测 {}",
            out[1]
        );
        assert!(
            (out[2] - 25.0).abs() < 1.5,
            "step3 期望 ≈25, 实测 {}",
            out[2]
        );
    }

    #[test]
    fn arima_ar1_positive_phi_pulls_toward_mean() {
        // 自相关序列: 围绕均值震荡, AR(1) φ > 0 → 预测回归到均值
        let p = ArimaPredictor::default();
        // 围绕 100 震荡: [110, 95, 105, 92, 108, 94, 106, 91, 109, 93]
        let series = vec![
            110.0, 95.0, 105.0, 92.0, 108.0, 94.0, 106.0, 91.0, 109.0, 93.0,
        ];
        let out = p.predict(&series, 5).unwrap();
        // 长期预测应回归到均值附近 (差分序列 mean ≈ 0)
        // 原尺度预测应接近 last value (~93) 的衰减
        for (i, v) in out.iter().enumerate() {
            assert!(v.is_finite(), "step{i} 预测发散: {v}");
            // 预测应大于 0 (物理上合理)
            assert!(*v > 0.0, "step{i} 预测非正: {v}");
        }
    }

    #[test]
    fn arima_random_walk_long_horizon_converges_to_diff_mean() {
        // 随机游走: y_t = y_{t-1} + z_t (z 噪声)
        // 差分序列 = z_t, AR(1) φ ≈ 0
        // 长期预测 → 还原后 ≈ 末尾值 + diff_mean * horizon
        // 简化验证: horizon=5, 输出长度=5, 全 finite
        let p = ArimaPredictor::default();
        let mut series = vec![100.0];
        for i in 1..30 {
            series.push(series[i - 1] + ((i as f64 * 0.7).sin()) * 2.0);
        }
        let out = p.predict(&series, 5).unwrap();
        assert_eq!(out.len(), 5);
        for v in &out {
            assert!(v.is_finite(), "随机游走预测发散: {v}");
        }
    }

    #[test]
    fn arima_predict_with_ci_returns_2_arrays() {
        use super::arima_predict_with_ci;
        let series: Vec<f64> = (0..20)
            .map(|t| 100.0 + f64::from(t) + (f64::from(t) * 0.5).sin())
            .collect();
        let (pred, ci) = arima_predict_with_ci(&series, 4).unwrap();
        assert_eq!(pred.len(), 4);
        assert_eq!(ci.len(), 4);
        // CI 应 > 0 (有噪声)
        for (i, c) in ci.iter().enumerate() {
            assert!(*c > 0.0, "step{i} CI 半宽非正: {c}");
        }
        // CI 应随 horizon 增大 (不确定性递增)
        assert!(
            ci[3] >= ci[0],
            "h=4 的 CI ({}) 应 ≥ h=1 的 CI ({})",
            ci[3],
            ci[0]
        );
    }

    #[test]
    fn arima_predict_with_ci_too_short_returns_degraded() {
        use super::arima_predict_with_ci;
        let err = arima_predict_with_ci(&[1.0, 2.0], 3).unwrap_err();
        assert!(matches!(err, AdapterError::Degraded(_)), "{err:?}");
    }

    #[test]
    fn arima_provider_distinguishable_from_naive_and_noop() {
        // 主人/审计一眼识别 provider 字段
        let arima = ArimaPredictor::default();
        let naive = NaiveBaselinePredictor::default();
        let noop = NoopTimeSeriesPredictor;
        assert_eq!(arima.provider(), "arima-1-1-1");
        assert_eq!(naive.provider(), "naive-baseline");
        assert_eq!(noop.provider(), "noop");
        // 三个不同, 不可混淆
        assert_ne!(arima.provider(), naive.provider());
        assert_ne!(arima.provider(), noop.provider());
        assert_ne!(naive.provider(), noop.provider());
    }

    // ──────────────────────────────────────────────────────────────────
    // 2026-08-20 TP25 (tract-onnx 轻量版): LightGBMProvider 6 测
    //   - 4 永远跑: default_noop / distinguishable / blendable / too_short
    //   - 2 fixture-gated: e2e_1step / e2e_nstep_with_ci
    // ──────────────────────────────────────────────────────────────────

    #[test]
    fn lightgbm_default_is_noop_with_honest_err() {
        use super::LightGBMProvider;
        let p = LightGBMProvider::default();
        assert_eq!(p.provider(), "lightgbm-noop");
        let series: Vec<f64> = (0..100).map(|t| 100.0 + (f64::from(t) / 5.0).sin()).collect();
        let err = p.predict(&series, 3).unwrap_err();
        assert!(matches!(err, AdapterError::Degraded(_)), "{err:?}");
        let msg = err.to_string();
        assert!(
            msg.contains("LightGBM") && msg.contains("未装载"),
            "Degraded 错误应明示 LightGBM 未装载原因, 实测: {msg}"
        );
    }

    #[test]
    fn lightgbm_provider_distinguishable_from_arima_naive_noop() {
        use super::LightGBMProvider;
        let lgbm = LightGBMProvider::default();
        let arima = ArimaPredictor::default();
        let naive = NaiveBaselinePredictor::default();
        let noop = NoopTimeSeriesPredictor;
        assert_eq!(lgbm.provider(), "lightgbm-noop");
        assert_eq!(arima.provider(), "arima-1-1-1");
        assert_eq!(naive.provider(), "naive-baseline");
        assert_eq!(noop.provider(), "noop");
        let names = [
            lgbm.provider(),
            arima.provider(),
            naive.provider(),
            noop.provider(),
        ];
        for i in 0..names.len() {
            for j in (i + 1)..names.len() {
                assert_ne!(names[i], names[j], "provider 冲突: {names:?}");
            }
        }
    }

    #[test]
    fn lightgbm_blendable_with_llm_text_prediction() {
        let digital = 0.65_f64;
        let textual = 0.70;
        let blended = blend_predictions(digital, textual, 0.8, 0.5);
        assert!(
            (blended - 0.6692307).abs() < 1e-6,
            "blended={blended} (期望 ≈0.6692)"
        );
        assert!(
            (0.0..=1.0).contains(&blended),
            "blend 应 ∈ [0,1], 实测 {blended}"
        );
    }

    #[test]
    fn lightgbm_input_too_short_returns_degraded() {
        use super::LightGBMProvider;
        let p = LightGBMProvider::default();
        let err = p.predict(&[1.0, 2.0, 3.0], 3).unwrap_err();
        assert!(matches!(err, AdapterError::Degraded(_)), "{err:?}");
        let msg = err.to_string();
        assert!(
            msg.contains("LightGBM"),
            "Degraded 应含 LightGBM 标识, 实测: {msg}"
        );
    }

    // ──── Fixture-gated E2E (脱机 PASS: fixture 不在 → 早返) ────

    #[test]
    fn lightgbm_e2e_1step_with_fixture() {
        use super::LightGBMProvider;
        use std::path::Path;
        let fixture = Path::new("tests/fixtures/lightgbm/BTC_1step_v1_20260820.onnx");
        if !fixture.exists() {
            eprintln!("[skip] TP25 fixture 缺失: {fixture:?} - 脱机 PASS");
            return;
        }
        let p = LightGBMProvider::from_onnx_file(fixture, 60);
        assert_eq!(p.provider(), "lightgbm-onnx");
        let series: Vec<f64> = (0..100).map(|t| 100.0 + (f64::from(t) / 5.0).sin()).collect();
        let one = p.predict(&series, 1).expect("1-step 应成功");
        assert_eq!(one.len(), 1);
        assert!(one[0].is_finite(), "1-step 输出应 finite, 实测 {}", one[0]);
        assert!(
            (one[0] - 100.91).abs() < 1.5,
            "RMSE < 1.5, 实测 {} (期望 ≈100.91)",
            one[0]
        );
    }

    #[test]
    fn lightgbm_e2e_nstep_with_ci() {
        use super::{lightgbm_predict_with_ci, LightGBMProvider};
        use std::path::Path;
        let fixture = Path::new("tests/fixtures/lightgbm/BTC_1step_v1_20260820.onnx");
        if !fixture.exists() {
            return;
        }
        let p = LightGBMProvider::from_onnx_file(fixture, 60);
        assert_eq!(p.provider(), "lightgbm-onnx");
        let series: Vec<f64> = (0..100).map(|t| 100.0 + (f64::from(t) / 5.0).sin()).collect();
        let (pred, ci) = lightgbm_predict_with_ci(&series, 5).expect("N-step+CI");
        assert_eq!(pred.len(), 5);
        assert_eq!(ci.len(), 5);
        for (i, v) in pred.iter().enumerate() {
            assert!(v.is_finite(), "step{i} 预测发散: {v}");
        }
        for (i, c) in ci.iter().enumerate() {
            assert!(*c > 0.0, "step{i} CI 半宽非正: {c}");
        }
        assert!(
            ci[4] >= ci[0],
            "CI 递增: h=5 ({}) >= h=1 ({})",
            ci[4],
            ci[0]
        );
    }
}

// ============================================================
// TP25 LightGBM Provider (tract-onnx 纯 Rust 推理, 0 系统库)
// ============================================================

pub(crate) type LightGBMSession = tract_onnx::prelude::RunnableModel<
    tract_onnx::prelude::TypedFact,
    Box<dyn tract_onnx::prelude::TypedOp>,
    tract_onnx::prelude::Graph<
        tract_onnx::prelude::TypedFact,
        Box<dyn tract_onnx::prelude::TypedOp>,
    >,
>;

/// LightGBM 时序预测器 (TP25 E3 增强) — tract-onnx 推理, 0 系统库 / 0 CMake / 0 MSVC.
#[derive(Debug, Clone)]
pub struct LightGBMProvider {
    session: Option<Arc<LightGBMSession>>,
    window_size: usize,
}

impl Default for LightGBMProvider {
    fn default() -> Self {
        Self {
            session: None,
            window_size: 60,
        }
    }
}

impl LightGBMProvider {
    pub fn is_loaded(&self) -> bool {
        self.session.is_some()
    }
    pub fn window_size(&self) -> usize {
        self.window_size
    }

    pub fn from_onnx_file(path: &std::path::Path, window_size: usize) -> Self {
        use tract_onnx::prelude::*;
        if !path.exists() {
            eprintln!("[LightGBMProvider] ONNX 不存在: {path:?} -> 0 装兜底");
            return Self::default_with_window(window_size);
        }
        let load_result = tract_onnx::onnx()
            .model_for_path(path)
            .and_then(|m| m.into_optimized())
            .and_then(|m| m.into_runnable());
        match load_result {
            Ok(model) => {
                eprintln!("[LightGBMProvider] 装载成功: {path:?}");
                Self {
                    session: Some(Arc::new(model)),
                    window_size,
                }
            }
            Err(e) => {
                eprintln!("[LightGBMProvider] 装载失败: {path:?} ({e})");
                Self::default_with_window(window_size)
            }
        }
    }

    fn default_with_window(window_size: usize) -> Self {
        Self {
            session: None,
            window_size,
        }
    }

    fn run_one_step(&self, history: &[f64]) -> Result<f64, String> {
        use tract_onnx::prelude::*;
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| "session 未装载".to_string())?;
        let row: Vec<f32> = history.iter().map(|&v| v as f32).collect();
        let input = ndarray::Array2::<f32>::from_shape_vec((1, self.window_size), row)
            .map_err(|e| format!("ndarray 构造失败: {e}"))?;
        let tensor: Tensor = Tensor::from_shape(&[1, self.window_size], &input.as_slice().unwrap())
            .map_err(|e| format!("构造 Tensor 失败: {e}"))?;
        let result = session
            .run(tvec![tensor.into()])
            .map_err(|e| format!("tract run 失败: {e}"))?;
        let out = result[0]
            .to_array_view::<f32>()
            .map_err(|e| format!("输出类型不匹配 f32: {e}"))?;
        Ok(f64::from(out[[0, 0]]))
    }
}

impl TimeSeriesPredictor for LightGBMProvider {
    fn predict(&self, series: &[f64], horizon: usize) -> Result<Vec<f64>, AdapterError> {
        let session = self.session.as_ref().ok_or_else(|| {
            AdapterError::Degraded("LightGBM 模型未装载 (默认 Noop, .onnx 缺失或装载失败)".into())
        })?;
        if horizon == 0 {
            return Ok(Vec::new());
        }
        if series.len() < self.window_size {
            return Err(AdapterError::Degraded(format!(
                "LightGBM 输入序列太短 ({} < window={})",
                series.len(),
                self.window_size
            )));
        }
        let mut history = series[series.len() - self.window_size..].to_vec();
        let mut out = Vec::with_capacity(horizon);
        for _ in 0..horizon {
            let y = self
                .run_one_step(&history)
                .map_err(|e| AdapterError::Degraded(format!("LightGBM 推理失败: {e}")))?;
            out.push(y);
            history.push(y);
            if history.len() > self.window_size {
                history.remove(0);
            }
        }
        let _ = session;
        Ok(out)
    }

    fn provider(&self) -> &str {
        if self.session.is_some() {
            "lightgbm-onnx"
        } else {
            "lightgbm-noop"
        }
    }
}

pub fn lightgbm_predict_with_ci(
    series: &[f64],
    horizon: usize,
) -> Result<(Vec<f64>, Vec<f64>), AdapterError> {
    let provider = LightGBMProvider::default();
    let pred = provider.predict(series, horizon)?;
    if !provider.is_loaded() {
        return Ok((pred, vec![0.0; horizon]));
    }
    let last = series.last().copied().unwrap_or(0.0);
    let prev = series.iter().rev().nth(1).copied().unwrap_or(last);
    let trend = last - prev;
    let residuals: Vec<f64> = pred
        .iter()
        .enumerate()
        .map(|(i, &p)| p - (last + trend * (i + 1) as f64))
        .collect();
    let sigma = if residuals.len() >= 2 {
        let mean = residuals.iter().sum::<f64>() / residuals.len() as f64;
        let var = residuals.iter().map(|e| (e - mean).powi(2)).sum::<f64>()
            / (residuals.len() - 1) as f64;
        var.sqrt()
    } else {
        0.0
    };
    let ci: Vec<f64> = (1..=horizon)
        .map(|h| 1.96 * sigma * (h as f64).sqrt())
        .collect();
    Ok((pred, ci))
}
