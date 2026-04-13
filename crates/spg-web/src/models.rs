use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Doubao,
    Serper,
    Firecrawl,
    Tavily,
    Exa,
}

impl ProviderKind {
    pub const ALL: [Self; 5] = [
        Self::Doubao,
        Self::Serper,
        Self::Firecrawl,
        Self::Tavily,
        Self::Exa,
    ];

    pub const REQUIRED: [Self; 5] = [
        Self::Doubao,
        Self::Serper,
        Self::Firecrawl,
        Self::Tavily,
        Self::Exa,
    ];

    pub const OPTIONAL: [Self; 0] = [];

    pub fn slug(self) -> &'static str {
        match self {
            Self::Doubao => "doubao",
            Self::Serper => "serper",
            Self::Firecrawl => "firecrawl",
            Self::Tavily => "tavily",
            Self::Exa => "exa",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Doubao => "Doubao",
            Self::Serper => "Serper",
            Self::Firecrawl => "Firecrawl",
            Self::Tavily => "Tavily",
            Self::Exa => "Exa",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Doubao => "日常主解析器，承担清洗、归因和结构化输出。",
            Self::Serper => "默认搜索入口，负责高频发现与召回。",
            Self::Firecrawl => "正文抽取器，只用于值得抓取的页面。",
            Self::Tavily => "时效验证补位，适合新闻与热点核验。",
            Self::Exa => "长尾语义扩展，做深挖和相邻发现。",
        }
    }

    pub fn default_base_url(self) -> &'static str {
        match self {
            Self::Doubao => "https://ark.cn-beijing.volces.com",
            Self::Serper => "https://google.serper.dev",
            Self::Firecrawl => "https://api.firecrawl.dev",
            Self::Tavily => "https://api.tavily.com",
            Self::Exa => "https://api.exa.ai",
        }
    }

    pub fn monthly_limit(self) -> i64 {
        match self {
            Self::Doubao => 99_999,
            Self::Serper => 2_500,
            Self::Firecrawl => 500,
            Self::Tavily => 1_000,
            Self::Exa => 1_000,
        }
    }

    pub fn daily_soft_limit(self) -> i64 {
        match self {
            Self::Doubao => 5_000,
            Self::Serper => 90,
            Self::Firecrawl => 16,
            Self::Tavily => 33,
            Self::Exa => 33,
        }
    }

    pub fn default_role(self) -> &'static str {
        match self {
            Self::Doubao => "primary_reasoner",
            Self::Serper => "primary_search",
            Self::Firecrawl => "content_fetch",
            Self::Tavily => "freshness_verify",
            Self::Exa => "long_tail_discovery",
        }
    }

    pub fn priority(self) -> i64 {
        match self {
            Self::Doubao => 1,
            Self::Serper => 2,
            Self::Firecrawl => 3,
            Self::Tavily => 4,
            Self::Exa => 5,
        }
    }

    pub fn is_required(self) -> bool {
        true
    }
}

impl std::fmt::Display for ProviderKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.slug())
    }
}

impl std::str::FromStr for ProviderKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "doubao" => Ok(Self::Doubao),
            "serper" => Ok(Self::Serper),
            "firecrawl" => Ok(Self::Firecrawl),
            "tavily" => Ok(Self::Tavily),
            "exa" => Ok(Self::Exa),
            other => Err(format!("unknown provider: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderStatusView {
    pub provider: String,
    pub display_name: String,
    pub description: String,
    pub enabled: bool,
    pub has_api_key: bool,
    pub api_key_summary: Option<String>,
    pub base_url: String,
    pub monthly_limit: i64,
    pub daily_soft_limit: i64,
    pub default_role: String,
    pub priority: i64,
    pub status: String,
    pub last_verified_at: Option<String>,
    pub last_error: Option<String>,
    pub latency_ms: Option<i64>,
    pub is_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnlockStateView {
    pub ready_count: usize,
    pub total_count: usize,
    pub optional_ready_count: usize,
    pub optional_total_count: usize,
    pub unlocked: bool,
    pub blockers: Vec<String>,
    pub required_providers: Vec<String>,
    pub optional_providers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalUserView {
    pub username: String,
    pub device_name: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateStateView {
    pub current_stage: String,
    pub local_user_confirmed: bool,
    pub confirmed_username: Option<String>,
    pub confirmed_device_name: Option<String>,
    pub api_gate_passed: bool,
    pub required_providers: Vec<String>,
    pub last_logout_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderUpdateRequest {
    pub enabled: bool,
    pub api_key: Option<String>,
    #[serde(default)]
    pub clear_api_key: bool,
    pub base_url: String,
    pub monthly_limit: i64,
    pub daily_soft_limit: i64,
    pub default_role: String,
    pub priority: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaSnapshot {
    pub used_monthly: i64,
    pub remaining_monthly: i64,
    pub daily_soft_limit: i64,
    pub monthly_limit: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderTestResponse {
    pub provider: String,
    pub ok: bool,
    pub status: String,
    pub latency_ms: i64,
    pub quota_snapshot: QuotaSnapshot,
    pub error_message: Option<String>,
    pub verified_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginPageView {
    pub local_user: LocalUserView,
    pub gate_state: GateStateView,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopLauncherPageView {
    pub local_user: LocalUserView,
    pub gate_state: GateStateView,
    pub app_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GalaxyGameListItem {
    pub id: String,
    pub name: String,
    pub studio: String,
    pub stage: String,
    pub aliases: Vec<String>,
    pub official_url: String,
    pub signal_count: i64,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GalaxyEventListItem {
    pub id: String,
    pub title: String,
    pub event_type: String,
    pub game_id: String,
    pub game_name: String,
    pub heat: i64,
    pub source_count: i64,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GalaxyNode {
    pub id: String,
    pub label: String,
    pub node_type: String,
    pub orbit: i64,
    pub angle: f64,
    pub size: f64,
    pub accent: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GalaxyEdge {
    pub source: String,
    pub target: String,
    pub edge_type: String,
    pub strength: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GalaxyFocusView {
    pub focus_id: String,
    pub focus_kind: String,
    pub title: String,
    pub subtitle: String,
    pub narrative: Vec<String>,
    pub nodes: Vec<GalaxyNode>,
    pub edges: Vec<GalaxyEdge>,
    pub available_layers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GalaxySummaryView {
    pub headline: String,
    pub subheadline: String,
    pub game_count: i64,
    pub event_count: i64,
    pub source_count: i64,
    pub track_count: i64,
    pub last_updated: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GalaxyListResponse {
    pub tab: String,
    pub games: Vec<GalaxyGameListItem>,
    pub events: Vec<GalaxyEventListItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupPageView {
    pub mode: String,
    pub local_user: LocalUserView,
    pub gate_state: GateStateView,
    pub providers: Vec<ProviderStatusView>,
    pub unlock_state: UnlockStateView,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GalaxyPageView {
    pub local_user: LocalUserView,
    pub gate_state: GateStateView,
    pub summary: GalaxySummaryView,
    pub default_tab: String,
    pub games: Vec<GalaxyGameListItem>,
    pub initial_focus: GalaxyFocusView,
    pub unlock_state: UnlockStateView,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkbenchSettings {
    pub platform: String,
    pub watchlist_games: Vec<String>,
    pub keyword_templates: Vec<String>,
    pub time_window_hours: i64,
    pub schedule_interval_hours: i64,
    pub max_candidates_per_run: i64,
    pub doubao_model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiBudgetUsageView {
    pub provider: String,
    pub enabled: bool,
    pub monthly_limit: i64,
    pub daily_soft_limit: i64,
    pub reserve_pool: i64,
    pub per_run_limit: i64,
    pub used_monthly: i64,
    pub used_today: i64,
    pub remaining_monthly: i64,
    pub remaining_today: i64,
    pub allowed_this_run: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisRun {
    pub run_id: String,
    pub trigger: String,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub candidate_count: i64,
    pub shortlisted_count: i64,
    pub content_count: i64,
    pub error_message: Option<String>,
    pub provider_usage: Vec<ApiBudgetUsageView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredCandidate {
    pub content_id: String,
    pub source_id: String,
    pub title: String,
    pub summary: String,
    pub url: String,
    pub platform: String,
    pub author: String,
    pub published_at: String,
    pub source_domain: String,
    pub discovery_query: String,
    pub tags: Vec<String>,
    pub game_id: Option<String>,
    pub game_name: Option<String>,
    pub event_type: Option<String>,
    pub topic_tags: Vec<String>,
    pub engagement_score: f64,
    pub freshness_score: f64,
    pub cross_source_score: f64,
    pub doubao_relevance_score: f64,
    pub hotness_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BreakdownCard {
    pub content_id: String,
    pub content_summary: String,
    pub hook_points: Vec<String>,
    pub core_conflict_or_value: String,
    pub audience_fit: String,
    pub adaptation_angles: Vec<String>,
    pub title_directions: Vec<String>,
    pub topic_pool_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankingSnapshot {
    pub kind: String,
    pub rank: i64,
    pub entity_id: String,
    pub content_id: Option<String>,
    pub title: String,
    pub subtitle: String,
    pub score: f64,
    pub game_id: Option<String>,
    pub event_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicPoolItem {
    pub topic_id: String,
    pub content_id: String,
    pub title: String,
    pub note: String,
    pub score: f64,
    pub source_url: String,
    pub game_name: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkbenchOverview {
    pub latest_run: Option<AnalysisRun>,
    pub content_rankings: Vec<RankingSnapshot>,
    pub game_rankings: Vec<RankingSnapshot>,
    pub event_rankings: Vec<RankingSnapshot>,
    pub topic_pool_count: usize,
    pub budgets: Vec<ApiBudgetUsageView>,
    pub settings: WorkbenchSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkbenchContentView {
    pub candidate: DiscoveredCandidate,
    pub breakdown: BreakdownCard,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkbenchPageView {
    pub local_user: LocalUserView,
    pub gate_state: GateStateView,
    pub unlock_state: UnlockStateView,
    pub overview: WorkbenchOverview,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicPoolCreateRequest {
    pub content_id: String,
    #[serde(default)]
    pub note: String,
}
