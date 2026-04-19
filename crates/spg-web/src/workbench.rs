use crate::models::{
    AnalysisRun, ApiBudgetUsageView, BreakdownCard, ContentTrustView, DiscoveredCandidate,
    ProviderKind, RankingSnapshot, TopicPoolCreateRequest, TopicPoolItem, WorkbenchContentView,
    WorkbenchOverview, WorkbenchSettings,
};
use crate::store::{ProviderRuntimeConfig, ProviderStore};
use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use spg_core::{
    build_asset_index, default_asset_path, load_asset_bundle, SPGInputPipeline, SQLiteStorage,
};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::Arc;
use time::format_description::well_known::Rfc3339;
use time::{Duration as TimeDuration, OffsetDateTime};
use tokio::sync::Mutex;
use tokio::time::{interval, Duration};

const SCHEDULER_TICK_SECS: u64 = 300;
const DEFAULT_DOUBAO_MODEL: &str = "doubao-seed-2-0-code-preview-260215";
const DEFAULT_AUTO_REASON_MODE: &str = "off";
const AUTO_REASON_STAGE_CANDIDATE_REFINE: &str = "candidate_refine";
const STALE_RUN_TIMEOUT_MINUTES: i64 = 30;
const STALE_RUN_RECOVERY_ERROR: &str = "stale run recovered before live scheduling";

#[derive(Debug, Clone, Copy)]
struct SeedGameProfile {
    canonical: &'static str,
    aliases: &'static [&'static str],
    focus_terms: &'static [&'static str],
}

#[derive(Debug, Clone, Deserialize)]
pub struct RankingQuery {
    pub kind: Option<String>,
    pub query: Option<String>,
    pub game_id: Option<String>,
    pub event_type: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone)]
struct BudgetPolicy {
    monthly_limit: i64,
    daily_soft_limit: i64,
    reserve_pool: i64,
    per_run_limit: i64,
    minimum_floor: i64,
}

#[derive(Debug, Clone)]
struct BudgetMeter {
    provider: ProviderKind,
    enabled: bool,
    policy: BudgetPolicy,
    used_monthly: i64,
    used_today: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct QueryPlan {
    queries: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SearchHit {
    title: String,
    url: String,
    snippet: String,
    source_domain: String,
    author: String,
    published_at: Option<String>,
    tags: Vec<String>,
    engagement_hint: f64,
}

#[derive(Debug, Clone)]
pub(crate) struct ScrapePayload {
    text: String,
    description: String,
    tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct VerificationPayload {
    score: f64,
    note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct StructuredBreakdown {
    game_id: Option<String>,
    game_name: Option<String>,
    event_type: Option<String>,
    topic_tags: Vec<String>,
    content_summary: String,
    hook_points: Vec<String>,
    core_conflict_or_value: String,
    audience_fit: String,
    adaptation_angles: Vec<String>,
    title_directions: Vec<String>,
    doubao_relevance_score: f64,
    topic_pool_reason: String,
}

#[derive(Debug, Clone)]
pub(crate) struct CandidateDraft {
    content_id: String,
    source_id: String,
    title: String,
    summary: String,
    url: String,
    platform: String,
    author: String,
    published_at: String,
    source_domain: String,
    discovery_query: String,
    tags: Vec<String>,
    text: String,
    description: String,
    search_rank: usize,
    engagement_score: f64,
    freshness_score: f64,
    cross_source_score: f64,
    doubao_relevance_score: f64,
    hotness_score: f64,
    game_id: Option<String>,
    game_name: Option<String>,
    event_type: Option<String>,
    topic_tags: Vec<String>,
    breakdown: BreakdownCard,
    metadata: Value,
}

fn trust_from_metadata_value(metadata: &Value, captured_at: String) -> ContentTrustView {
    ContentTrustView {
        source_platform_match: metadata
            .get("source_platform_match")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        fallback_non_douyin: metadata
            .get("fallback_non_douyin")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        verification_note: metadata
            .get("verification_note")
            .and_then(Value::as_str)
            .map(|value| value.to_string()),
        captured_at,
    }
}

fn trust_from_metadata_json(metadata_json: &str, captured_at: String) -> ContentTrustView {
    let metadata = serde_json::from_str::<Value>(metadata_json).unwrap_or(Value::Null);
    trust_from_metadata_value(&metadata, captured_at)
}

#[derive(Debug, Clone)]
struct RunExecution {
    discovered_count: usize,
    shortlisted_count: usize,
    candidates: Vec<DiscoveredCandidate>,
    breakdowns: Vec<BreakdownCard>,
    content_rankings: Vec<RankingSnapshot>,
    game_rankings: Vec<RankingSnapshot>,
    event_rankings: Vec<RankingSnapshot>,
    usage: Vec<ApiBudgetUsageView>,
    reasoning: AutoReasonArtifacts,
}

#[derive(Debug, Clone)]
struct CandidateVersionArtifact {
    version_id: String,
    run_id: String,
    content_id: String,
    parent_content_id: String,
    stage: String,
    variant_kind: String,
    source_id: String,
    title: String,
    summary: String,
    url: String,
    author: String,
    published_at: String,
    tags_json: String,
    topic_tags_json: String,
    metadata_json: String,
    created_at: String,
}

#[derive(Debug, Clone)]
struct ReasoningJudgementArtifact {
    decision_id: String,
    run_id: String,
    stage: String,
    content_id: String,
    candidate_a_version_id: String,
    candidate_b_version_id: String,
    candidate_ab_version_id: String,
    winner_version_id: String,
    judge_round: i64,
    judge_model: String,
    judge_labels_json: String,
    judge_ranking_json: String,
    do_nothing: i64,
    stop_reason: String,
    confidence: f64,
    latency_ms: i64,
    created_at: String,
}

#[derive(Debug, Clone)]
struct StageRuntimeMetricArtifact {
    metric_id: String,
    run_id: String,
    stage: String,
    object_id: String,
    mode: String,
    status: String,
    attempts: i64,
    latency_ms: i64,
    provider_usage_json: String,
    note: String,
    created_at: String,
}

#[derive(Debug, Clone, Default)]
struct AutoReasonArtifacts {
    candidate_versions: Vec<CandidateVersionArtifact>,
    reasoning_judgements: Vec<ReasoningJudgementArtifact>,
    stage_metrics: Vec<StageRuntimeMetricArtifact>,
}

#[derive(Debug, Clone)]
struct AutoReasonOutcome {
    drafts: Vec<CandidateDraft>,
    artifacts: AutoReasonArtifacts,
}

#[derive(Debug, Clone)]
struct ShadowCandidateVariant {
    kind: String,
    title: String,
    summary: String,
    score: f64,
    rationale: String,
}

#[async_trait]
pub(crate) trait ProviderGateway: Send + Sync {
    async fn plan_queries(
        &self,
        config: &ProviderRuntimeConfig,
        settings: &WorkbenchSettings,
        history: &[String],
    ) -> Result<QueryPlan>;

    async fn serper_search(
        &self,
        config: &ProviderRuntimeConfig,
        query: &str,
    ) -> Result<Vec<SearchHit>>;

    async fn exa_expand(
        &self,
        config: &ProviderRuntimeConfig,
        query: &str,
    ) -> Result<Vec<SearchHit>>;

    async fn tavily_verify(
        &self,
        config: &ProviderRuntimeConfig,
        title: &str,
        url: &str,
    ) -> Result<VerificationPayload>;

    async fn firecrawl_scrape(
        &self,
        config: &ProviderRuntimeConfig,
        url: &str,
    ) -> Result<ScrapePayload>;

    async fn doubao_analyze(
        &self,
        config: &ProviderRuntimeConfig,
        settings: &WorkbenchSettings,
        candidate: &CandidateDraft,
    ) -> Result<StructuredBreakdown>;
}

#[derive(Clone)]
pub(crate) struct HttpProviderGateway {
    client: Client,
}

impl Default for HttpProviderGateway {
    fn default() -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("reqwest client"),
        }
    }
}

#[test]
fn looks_like_douyin_accepts_share_domains() {
    let hit = SearchHit {
        title: "率土之滨 热门视频".to_string(),
        url: "https://v.douyin.com/abcd1234/".to_string(),
        snippet: "分享页".to_string(),
        source_domain: "v.douyin.com".to_string(),
        author: "测试作者".to_string(),
        published_at: Some(utc_now()),
        tags: vec![],
        engagement_hint: 0.5,
    };

    assert!(looks_like_douyin(&hit));
}

#[test]
fn shortlist_candidates_falls_back_to_recent_douyin_hits() {
    let settings = WorkbenchSettings {
        platform: "douyin".to_string(),
        watchlist_games: vec!["率土之滨".to_string()],
        keyword_templates: default_seed_keyword_templates(),
        time_window_hours: 24,
        schedule_interval_hours: 2,
        max_candidates_per_run: 10,
        doubao_model: DEFAULT_DOUBAO_MODEL.to_string(),
        auto_reason_mode: DEFAULT_AUTO_REASON_MODE.to_string(),
        auto_reason_stages: vec![AUTO_REASON_STAGE_CANDIDATE_REFINE.to_string()],
        auto_reason_judge_model: DEFAULT_DOUBAO_MODEL.to_string(),
        auto_reason_max_rounds: 2,
        auto_reason_timeout_ms: 12_000,
        auto_reason_shadow_sample_rate: 0.2,
        auto_reason_min_confidence: 0.6,
        auto_reason_do_nothing_margin: 0.05,
    };
    let discovery = vec![(
        "site:douyin.com/video 率土之滨".to_string(),
        SearchHit {
            title: "今日热门视频".to_string(),
            url: "https://v.douyin.com/fallback-hit/".to_string(),
            snippet: "热门分享入口".to_string(),
            source_domain: "v.douyin.com".to_string(),
            author: "测试作者".to_string(),
            published_at: Some(utc_now()),
            tags: vec![],
            engagement_hint: 0.6,
        },
    )];

    let shortlist = shortlist_candidates(discovery, &settings);
    assert_eq!(shortlist.len(), 1);
    assert_eq!(shortlist[0].1.url, "https://v.douyin.com/fallback-hit/");
}
#[async_trait]
impl ProviderGateway for HttpProviderGateway {
    async fn plan_queries(
        &self,
        config: &ProviderRuntimeConfig,
        settings: &WorkbenchSettings,
        history: &[String],
    ) -> Result<QueryPlan> {
        let model = resolve_doubao_model(config, settings)?;
        let prompt = json!({
            "task": "Generate one structured Douyin SLG query plan and return strict JSON.",
            "constraints": {
                "platform": "douyin",
                "time_window_hours": settings.time_window_hours,
                "max_queries": 8,
                "required_buckets": [
                    "game + version_or_season",
                    "game + activity_or_collab",
                    "game + gameplay_or_lineup_or_leveling",
                    "generic_slg_hot_topic"
                ]
            },
            "watchlist_games": settings.watchlist_games,
            "keyword_templates": settings.keyword_templates,
            "recent_hot_topics": history,
            "output_schema": { "queries": ["query 1", "query 2"] }
        });

        let content = self.doubao_request(config, &model, prompt).await?;
        let value = extract_json_value(&content)?;
        let queries = value
            .get("queries")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .filter(|items| !items.is_empty())
            .ok_or_else(|| anyhow!("doubao query plan did not include queries"))?;
        Ok(QueryPlan { queries })
    }

    async fn serper_search(
        &self,
        config: &ProviderRuntimeConfig,
        query: &str,
    ) -> Result<Vec<SearchHit>> {
        let api_key = required_api_key(config)?;
        let response = self
            .client
            .post(join_url(&config.base_url, "/search"))
            .header("X-API-KEY", api_key)
            .json(&json!({
                "q": query,
                "gl": "cn",
                "hl": "zh-cn",
                "num": 10
            }))
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;

        Ok(response
            .get("organic")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
            .filter_map(|(index, item)| {
                let link = item.get("link").and_then(Value::as_str)?.to_string();
                Some(SearchHit {
                    title: item
                        .get("title")
                        .and_then(Value::as_str)
                        .unwrap_or("untitled result")
                        .to_string(),
                    url: link.clone(),
                    snippet: item
                        .get("snippet")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    source_domain: domain_for_url(&link),
                    author: String::new(),
                    published_at: item
                        .get("date")
                        .and_then(Value::as_str)
                        .map(normalize_external_timestamp),
                    tags: vec!["serper".to_string()],
                    engagement_hint: (1.0 - index as f64 * 0.08).max(0.15),
                })
            })
            .collect())
    }

    async fn exa_expand(
        &self,
        config: &ProviderRuntimeConfig,
        query: &str,
    ) -> Result<Vec<SearchHit>> {
        let api_key = required_api_key(config)?;
        let response = self
            .client
            .post(join_url(&config.base_url, "/search"))
            .header("x-api-key", api_key)
            .json(&json!({
                "query": query,
                "numResults": 5,
                "contents": { "text": true, "highlights": true }
            }))
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;

        Ok(response
            .get("results")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
            .filter_map(|(index, item)| {
                let url = item.get("url").and_then(Value::as_str)?.to_string();
                let text = item
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                Some(SearchHit {
                    title: item
                        .get("title")
                        .and_then(Value::as_str)
                        .unwrap_or("Exa 鎵╁睍缁撴灉")
                        .to_string(),
                    url: url.clone(),
                    snippet: text.chars().take(240).collect(),
                    source_domain: domain_for_url(&url),
                    author: item
                        .get("author")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    published_at: item
                        .get("publishedDate")
                        .and_then(Value::as_str)
                        .map(normalize_external_timestamp),
                    tags: vec!["exa".to_string()],
                    engagement_hint: (0.72 - index as f64 * 0.09).max(0.12),
                })
            })
            .collect())
    }

    async fn tavily_verify(
        &self,
        config: &ProviderRuntimeConfig,
        title: &str,
        url: &str,
    ) -> Result<VerificationPayload> {
        let api_key = required_api_key(config)?;
        let response = self
            .client
            .post(join_url(&config.base_url, "/search"))
            .json(&json!({
                "api_key": api_key,
                "query": format!("{title} {url}"),
                "search_depth": "advanced",
                "max_results": 5,
                "topic": "news"
            }))
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;

        let results = response
            .get("results")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok(VerificationPayload {
            score: (results.len() as f64 / 5.0).clamp(0.0, 1.0),
            note: results
                .iter()
                .take(2)
                .filter_map(|item| item.get("title").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join(" | "),
        })
    }

    async fn firecrawl_scrape(
        &self,
        config: &ProviderRuntimeConfig,
        url: &str,
    ) -> Result<ScrapePayload> {
        let api_key = required_api_key(config)?;
        let response = self
            .client
            .post(join_url(&config.base_url, "/v1/scrape"))
            .header("Authorization", format!("Bearer {api_key}"))
            .json(&json!({
                "url": url,
                "formats": ["markdown"],
                "onlyMainContent": true
            }))
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;

        let metadata = response
            .get("data")
            .and_then(|data| data.get("metadata"))
            .cloned()
            .unwrap_or_else(|| json!({}));

        Ok(ScrapePayload {
            text: response
                .get("data")
                .and_then(|data| data.get("markdown"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            description: metadata
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            tags: metadata
                .get("keywords")
                .and_then(Value::as_str)
                .map(|value| split_csvish(value).collect())
                .unwrap_or_default(),
        })
    }

    async fn doubao_analyze(
        &self,
        config: &ProviderRuntimeConfig,
        settings: &WorkbenchSettings,
        candidate: &CandidateDraft,
    ) -> Result<StructuredBreakdown> {
        let model = resolve_doubao_model(config, settings)?;
        let prompt = json!({
            "task": "You analyze Douyin SLG content and must return strict JSON only.",
            "platform": "douyin",
            "watchlist_games": settings.watchlist_games,
            "output_schema": {
                "game_id": "optional",
                "game_name": "optional",
                "event_type": "optional",
                "topic_tags": ["tag"],
                "content_summary": "summary",
                "hook_points": ["hook"],
                "core_conflict_or_value": "core value or conflict",
                "audience_fit": "target audience",
                "adaptation_angles": ["adaptation angle"],
                "title_directions": ["title direction"],
                "doubao_relevance_score": 0.0,
                "topic_pool_reason": "why it belongs in the topic pool"
            },
            "candidate": {
                "title": candidate.title,
                "summary": candidate.summary,
                "text": candidate.text,
                "description": candidate.description,
                "url": candidate.url,
                "tags": candidate.tags,
                "published_at": candidate.published_at
            }
        });

        let content = self.doubao_request(config, &model, prompt).await?;
        Ok(serde_json::from_value(extract_json_value(&content)?)?)
    }
}

impl HttpProviderGateway {
    async fn doubao_request(
        &self,
        config: &ProviderRuntimeConfig,
        model: &str,
        prompt: Value,
    ) -> Result<String> {
        let api_key = required_api_key(config)?;
        let response = self
            .client
            .post(join_url(&config.base_url, "/api/v3/chat/completions"))
            .bearer_auth(api_key)
            .json(&json!({
                "model": model,
                "messages": [
                    { "role": "system", "content": "Return valid JSON only for Douyin SLG analysis tasks." },
                    { "role": "user", "content": prompt.to_string() }
                ],
                "temperature": 0.2
            }))
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;

        response
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("content"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| anyhow!("doubao response did not include content"))
    }
}

#[derive(Clone)]
pub(crate) struct WorkbenchService {
    db_path: PathBuf,
    provider_store: ProviderStore,
    gateway: Arc<dyn ProviderGateway>,
    run_lock: Arc<Mutex<()>>,
    scheduler_started: Arc<AtomicBool>,
}

impl WorkbenchService {
    pub(crate) fn new(
        db_path: PathBuf,
        provider_store: ProviderStore,
        gateway: Arc<dyn ProviderGateway>,
    ) -> Result<Self> {
        let service = Self {
            db_path,
            provider_store,
            gateway,
            run_lock: Arc::new(Mutex::new(())),
            scheduler_started: Arc::new(AtomicBool::new(false)),
        };
        service.initialize()?;
        Ok(service)
    }

    pub fn initialize(&self) -> Result<()> {
        if let Some(parent) = self.db_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        let storage = SQLiteStorage::new(&self.db_path);
        storage.initialize()?;
        let bundle = load_asset_bundle(&default_asset_path())?;
        storage.seed_aliases(&bundle)?;

        let conn = self.connect()?;
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS analysis_runs (
                run_id TEXT PRIMARY KEY,
                trigger TEXT NOT NULL,
                status TEXT NOT NULL,
                started_at TEXT NOT NULL,
                finished_at TEXT,
                candidate_count INTEGER NOT NULL DEFAULT 0,
                shortlisted_count INTEGER NOT NULL DEFAULT 0,
                content_count INTEGER NOT NULL DEFAULT 0,
                error_message TEXT,
                provider_usage_json TEXT NOT NULL DEFAULT '[]'
            );

            CREATE TABLE IF NOT EXISTS content_candidates (
                content_id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL,
                source_id TEXT NOT NULL,
                title TEXT NOT NULL,
                summary TEXT NOT NULL,
                url TEXT NOT NULL,
                platform TEXT NOT NULL,
                author TEXT NOT NULL,
                published_at TEXT NOT NULL,
                source_domain TEXT NOT NULL,
                discovery_query TEXT NOT NULL,
                tags_json TEXT NOT NULL,
                game_id TEXT,
                game_name TEXT,
                event_type TEXT,
                topic_tags_json TEXT NOT NULL,
                engagement_score REAL NOT NULL,
                freshness_score REAL NOT NULL,
                cross_source_score REAL NOT NULL,
                doubao_relevance_score REAL NOT NULL,
                hotness_score REAL NOT NULL,
                metadata_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS breakdown_cards (
                content_id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL,
                content_summary TEXT NOT NULL,
                hook_points_json TEXT NOT NULL,
                core_conflict_or_value TEXT NOT NULL,
                audience_fit TEXT NOT NULL,
                adaptation_angles_json TEXT NOT NULL,
                title_directions_json TEXT NOT NULL,
                topic_pool_reason TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS ranking_snapshots (
                snapshot_id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                entity_id TEXT NOT NULL,
                content_id TEXT,
                title TEXT NOT NULL,
                subtitle TEXT NOT NULL,
                score REAL NOT NULL,
                rank_value INTEGER NOT NULL,
                game_id TEXT,
                event_type TEXT,
                is_current INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS topic_pool (
                topic_id TEXT PRIMARY KEY,
                content_id TEXT NOT NULL,
                title TEXT NOT NULL,
                note TEXT NOT NULL,
                score REAL NOT NULL,
                source_url TEXT NOT NULL,
                game_name TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS workbench_settings (
                id INTEGER PRIMARY KEY CHECK(id = 1),
                platform TEXT NOT NULL,
                watchlist_games_json TEXT NOT NULL,
                keyword_templates_json TEXT NOT NULL,
                time_window_hours INTEGER NOT NULL,
                schedule_interval_hours INTEGER NOT NULL,
                max_candidates_per_run INTEGER NOT NULL,
                doubao_model TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS api_budget_usage (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                provider TEXT NOT NULL,
                run_id TEXT NOT NULL,
                units INTEGER NOT NULL,
                endpoint TEXT NOT NULL,
                recorded_at TEXT NOT NULL,
                note TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS candidate_versions (
                version_id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL,
                content_id TEXT NOT NULL,
                parent_content_id TEXT NOT NULL,
                stage TEXT NOT NULL,
                variant_kind TEXT NOT NULL,
                source_id TEXT NOT NULL,
                title TEXT NOT NULL,
                summary TEXT NOT NULL,
                url TEXT NOT NULL,
                author TEXT NOT NULL,
                published_at TEXT NOT NULL,
                tags_json TEXT NOT NULL,
                topic_tags_json TEXT NOT NULL,
                metadata_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS reasoning_judgements (
                decision_id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL,
                stage TEXT NOT NULL,
                content_id TEXT NOT NULL,
                candidate_a_version_id TEXT NOT NULL,
                candidate_b_version_id TEXT NOT NULL,
                candidate_ab_version_id TEXT NOT NULL,
                winner_version_id TEXT NOT NULL,
                judge_round INTEGER NOT NULL,
                judge_model TEXT NOT NULL,
                judge_labels_json TEXT NOT NULL,
                judge_ranking_json TEXT NOT NULL,
                do_nothing INTEGER NOT NULL,
                stop_reason TEXT NOT NULL,
                confidence REAL NOT NULL,
                latency_ms INTEGER NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS stage_runtime_metrics (
                metric_id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL,
                stage TEXT NOT NULL,
                object_id TEXT NOT NULL,
                mode TEXT NOT NULL,
                status TEXT NOT NULL,
                attempts INTEGER NOT NULL,
                latency_ms INTEGER NOT NULL,
                provider_usage_json TEXT NOT NULL,
                note TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_analysis_runs_started_at ON analysis_runs(started_at DESC);
            CREATE INDEX IF NOT EXISTS idx_content_candidates_run_id ON content_candidates(run_id);
            CREATE INDEX IF NOT EXISTS idx_ranking_snapshots_current ON ranking_snapshots(kind, is_current, rank_value);
            CREATE INDEX IF NOT EXISTS idx_topic_pool_updated_at ON topic_pool(updated_at DESC);
            CREATE INDEX IF NOT EXISTS idx_api_budget_usage_provider ON api_budget_usage(provider, recorded_at);
            CREATE INDEX IF NOT EXISTS idx_candidate_versions_content_stage ON candidate_versions(content_id, stage, variant_kind);
            CREATE INDEX IF NOT EXISTS idx_reasoning_judgements_run_stage ON reasoning_judgements(run_id, stage, judge_round);
            CREATE INDEX IF NOT EXISTS idx_stage_runtime_metrics_run_stage ON stage_runtime_metrics(run_id, stage, created_at DESC);
            ",
        )?;

        ensure_column(
            &conn,
            "workbench_settings",
            "auto_reason_mode",
            "TEXT NOT NULL DEFAULT 'off'",
        )?;
        ensure_column(
            &conn,
            "workbench_settings",
            "auto_reason_stages_json",
            "TEXT NOT NULL DEFAULT '[\"candidate_refine\"]'",
        )?;
        ensure_column(
            &conn,
            "workbench_settings",
            "auto_reason_judge_model",
            &format!("TEXT NOT NULL DEFAULT '{DEFAULT_DOUBAO_MODEL}'"),
        )?;
        ensure_column(
            &conn,
            "workbench_settings",
            "auto_reason_max_rounds",
            "INTEGER NOT NULL DEFAULT 2",
        )?;
        ensure_column(
            &conn,
            "workbench_settings",
            "auto_reason_timeout_ms",
            "INTEGER NOT NULL DEFAULT 12000",
        )?;
        ensure_column(
            &conn,
            "workbench_settings",
            "auto_reason_shadow_sample_rate",
            "REAL NOT NULL DEFAULT 0.2",
        )?;
        ensure_column(
            &conn,
            "workbench_settings",
            "auto_reason_min_confidence",
            "REAL NOT NULL DEFAULT 0.6",
        )?;
        ensure_column(
            &conn,
            "workbench_settings",
            "auto_reason_do_nothing_margin",
            "REAL NOT NULL DEFAULT 0.05",
        )?;

        conn.execute(
            "
            INSERT OR IGNORE INTO workbench_settings(
                id, platform, watchlist_games_json, keyword_templates_json,
                time_window_hours, schedule_interval_hours, max_candidates_per_run,
                doubao_model, updated_at
            ) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ",
            params![
                "douyin",
                serde_json::to_string(&default_watchlist_games())?,
                serde_json::to_string(&default_seed_keyword_templates())?,
                24_i64,
                2_i64,
                50_i64,
                DEFAULT_DOUBAO_MODEL,
                utc_now(),
            ],
        )?;

        conn.execute(
            "
            UPDATE workbench_settings
            SET doubao_model = ?1,
                updated_at = ?2
            WHERE id = 1 AND (TRIM(doubao_model) = '' OR TRIM(doubao_model) = 'primary_reasoner')
            ",
            params![DEFAULT_DOUBAO_MODEL, utc_now()],
        )?;

        self.recover_stale_runs(&conn)?;

        Ok(())
    }

    pub fn start_scheduler(&self) {
        if self.scheduler_started.swap(true, AtomicOrdering::SeqCst) {
            return;
        }

        let service = self.clone();
        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(SCHEDULER_TICK_SECS));
            loop {
                ticker.tick().await;
                if let Err(error) = service.run_scheduled_if_due().await {
                    eprintln!("workbench scheduler error: {error:#}");
                }
            }
        });
    }

    pub async fn run_now(&self) -> Result<AnalysisRun> {
        let _guard = self.run_lock.lock().await;
        let run_id = format!("run_{}", short_hash(&utc_now()));
        let started_at = utc_now();
        self.insert_run(&run_id, "manual", "running", &started_at)?;

        match self.execute_run(&run_id).await {
            Ok(result) => {
                self.persist_execution(&run_id, "manual", &started_at, result)?;
                self.run_by_id(&run_id)
            }
            Err(error) => {
                self.finish_run_failure(&run_id, &started_at, &error.to_string())?;
                self.run_by_id(&run_id)
            }
        }
    }

    pub fn overview(&self) -> Result<WorkbenchOverview> {
        Ok(WorkbenchOverview {
            latest_run: self.latest_run()?,
            content_rankings: self.rankings(
                "content",
                &RankingQuery {
                    kind: None,
                    query: None,
                    game_id: None,
                    event_type: None,
                    limit: Some(8),
                },
            )?,
            game_rankings: self.rankings(
                "game",
                &RankingQuery {
                    kind: None,
                    query: None,
                    game_id: None,
                    event_type: None,
                    limit: Some(6),
                },
            )?,
            event_rankings: self.rankings(
                "event",
                &RankingQuery {
                    kind: None,
                    query: None,
                    game_id: None,
                    event_type: None,
                    limit: Some(6),
                },
            )?,
            topic_pool_count: self.topic_pool()?.len(),
            budgets: self.current_budgets()?,
            settings: self.settings()?,
        })
    }

    pub fn rankings(&self, kind: &str, query: &RankingQuery) -> Result<Vec<RankingSnapshot>> {
        let conn = self.connect()?;
        let mut statement = conn.prepare(
            "
            SELECT rs.entity_id, rs.content_id, rs.title, rs.subtitle, rs.score, rs.rank_value,
                   rs.game_id, rs.event_type, cc.platform, cc.author, cc.published_at,
                   cc.source_domain, cc.url, cc.metadata_json, cc.created_at
            FROM ranking_snapshots rs
            LEFT JOIN content_candidates cc ON cc.content_id = rs.content_id
            WHERE rs.kind = ?1 AND rs.is_current = 1
            ORDER BY rank_value ASC
            ",
        )?;
        let mut rows = statement.query([kind])?;
        let needle = query.query.as_ref().map(|value| value.to_ascii_lowercase());
        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            let item = RankingSnapshot {
                kind: kind.to_string(),
                entity_id: row.get(0)?,
                content_id: row.get(1)?,
                title: row.get(2)?,
                subtitle: row.get(3)?,
                score: row.get(4)?,
                rank: row.get(5)?,
                game_id: row.get(6)?,
                event_type: row.get(7)?,
                platform: row.get(8)?,
                author: row.get(9)?,
                published_at: row.get(10)?,
                source_domain: row.get(11)?,
                source_url: row.get(12)?,
                trust: match (
                    row.get::<_, Option<String>>(13)?,
                    row.get::<_, Option<String>>(14)?,
                ) {
                    (Some(metadata_json), Some(created_at)) => {
                        Some(trust_from_metadata_json(&metadata_json, created_at))
                    }
                    _ => None,
                },
            };
            if let Some(game_id) = query.game_id.as_deref() {
                if item.game_id.as_deref() != Some(game_id) {
                    continue;
                }
            }
            if let Some(event_type) = query.event_type.as_deref() {
                if item.event_type.as_deref() != Some(event_type) {
                    continue;
                }
            }
            if let Some(needle) = &needle {
                let haystack = format!(
                    "{} {} {}",
                    item.title.to_ascii_lowercase(),
                    item.subtitle.to_ascii_lowercase(),
                    item.entity_id.to_ascii_lowercase()
                );
                if !haystack.contains(needle) {
                    continue;
                }
            }
            items.push(item);
            if items.len() >= query.limit.unwrap_or(24) {
                break;
            }
        }
        Ok(items)
    }

    pub fn content(&self, content_id: &str) -> Result<Option<WorkbenchContentView>> {
        let conn = self.connect()?;
        let candidate_row = conn
            .query_row(
                "
                SELECT source_id, title, summary, url, platform, author, published_at, source_domain,
                       discovery_query, tags_json, game_id, game_name, event_type, topic_tags_json,
                       engagement_score, freshness_score, cross_source_score, doubao_relevance_score,
                       hotness_score, metadata_json, created_at
                FROM content_candidates
                WHERE content_id = ?1
                ",
                [content_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, String>(8)?,
                        row.get::<_, String>(9)?,
                        row.get::<_, Option<String>>(10)?,
                        row.get::<_, Option<String>>(11)?,
                        row.get::<_, Option<String>>(12)?,
                        row.get::<_, String>(13)?,
                        row.get::<_, f64>(14)?,
                        row.get::<_, f64>(15)?,
                        row.get::<_, f64>(16)?,
                        row.get::<_, f64>(17)?,
                        row.get::<_, f64>(18)?,
                        row.get::<_, String>(19)?,
                        row.get::<_, String>(20)?,
                    ))
                },
            )
            .optional()?;

        let Some(candidate_row) = candidate_row else {
            return Ok(None);
        };
        let candidate = DiscoveredCandidate {
            content_id: content_id.to_string(),
            source_id: candidate_row.0,
            title: candidate_row.1,
            summary: candidate_row.2,
            url: candidate_row.3,
            platform: candidate_row.4,
            author: candidate_row.5,
            published_at: candidate_row.6,
            source_domain: candidate_row.7,
            discovery_query: candidate_row.8,
            tags: from_json_text(candidate_row.9)?,
            game_id: candidate_row.10,
            game_name: candidate_row.11,
            event_type: candidate_row.12,
            topic_tags: from_json_text(candidate_row.13)?,
            engagement_score: candidate_row.14,
            freshness_score: candidate_row.15,
            cross_source_score: candidate_row.16,
            doubao_relevance_score: candidate_row.17,
            hotness_score: candidate_row.18,
            trust: trust_from_metadata_json(&candidate_row.19, candidate_row.20),
        };

        let breakdown_row = conn.query_row(
            "
            SELECT content_summary, hook_points_json, core_conflict_or_value, audience_fit,
                   adaptation_angles_json, title_directions_json, topic_pool_reason
            FROM breakdown_cards
            WHERE content_id = ?1
            ",
            [content_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            },
        )?;
        let breakdown = BreakdownCard {
            content_id: content_id.to_string(),
            content_summary: breakdown_row.0,
            hook_points: from_json_text(breakdown_row.1)?,
            core_conflict_or_value: breakdown_row.2,
            audience_fit: breakdown_row.3,
            adaptation_angles: from_json_text(breakdown_row.4)?,
            title_directions: from_json_text(breakdown_row.5)?,
            topic_pool_reason: breakdown_row.6,
        };

        Ok(Some(WorkbenchContentView {
            candidate,
            breakdown,
        }))
    }

    pub fn list_runs(&self) -> Result<Vec<AnalysisRun>> {
        let conn = self.connect()?;
        let mut statement = conn.prepare(
            "
            SELECT run_id, trigger, status, started_at, finished_at,
                   candidate_count, shortlisted_count, content_count,
                   error_message, provider_usage_json
            FROM analysis_runs
            ORDER BY started_at DESC
            LIMIT 20
            ",
        )?;
        let rows = statement.query_map([], map_analysis_run_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn topic_pool(&self) -> Result<Vec<TopicPoolItem>> {
        let conn = self.connect()?;
        let mut statement = conn.prepare(
            "
            SELECT topic_id, content_id, title, note, score, source_url, game_name, created_at, updated_at
            FROM topic_pool
            ORDER BY updated_at DESC
            ",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(TopicPoolItem {
                topic_id: row.get(0)?,
                content_id: row.get(1)?,
                title: row.get(2)?,
                note: row.get(3)?,
                score: row.get(4)?,
                source_url: row.get(5)?,
                game_name: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn add_topic_pool(&self, request: TopicPoolCreateRequest) -> Result<TopicPoolItem> {
        let content = self
            .content(&request.content_id)?
            .ok_or_else(|| anyhow!("unknown content item"))?;
        let now = utc_now();
        let conn = self.connect()?;
        let existing: Option<String> = conn
            .query_row(
                "SELECT topic_id FROM topic_pool WHERE content_id = ?1",
                [request.content_id.as_str()],
                |row| row.get(0),
            )
            .optional()?;
        let topic_id =
            existing.unwrap_or_else(|| format!("topic_{}", short_hash(&request.content_id)));
        conn.execute(
            "
            INSERT INTO topic_pool(topic_id, content_id, title, note, score, source_url, game_name, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
            ON CONFLICT(topic_id) DO UPDATE SET
                note = excluded.note,
                score = excluded.score,
                source_url = excluded.source_url,
                game_name = excluded.game_name,
                updated_at = excluded.updated_at
            ",
            params![
                &topic_id,
                &content.candidate.content_id,
                &content.candidate.title,
                request.note.trim(),
                content.candidate.hotness_score,
                &content.candidate.url,
                content.candidate.game_name.clone(),
                &now,
            ],
        )?;
        self.topic_pool()?
            .into_iter()
            .find(|item| item.topic_id == topic_id)
            .ok_or_else(|| anyhow!("failed to fetch inserted topic"))
    }

    pub fn remove_topic_pool(&self, topic_id: &str) -> Result<()> {
        let conn = self.connect()?;
        conn.execute("DELETE FROM topic_pool WHERE topic_id = ?1", [topic_id])?;
        Ok(())
    }

    pub fn export_topic_pool_csv(&self) -> Result<String> {
        let mut lines =
            vec!["topic_id,title,game_name,score,note,source_url,created_at".to_string()];
        for item in self.topic_pool()? {
            lines.push(format!(
                "{},{},{},{:.3},{},{},{}",
                csv_escape(&item.topic_id),
                csv_escape(&item.title),
                csv_escape(item.game_name.as_deref().unwrap_or("")),
                item.score,
                csv_escape(&item.note),
                csv_escape(&item.source_url),
                csv_escape(&item.created_at),
            ));
        }
        Ok(lines.join("\n"))
    }

    pub fn settings(&self) -> Result<WorkbenchSettings> {
        let conn = self.connect()?;
        let row = conn.query_row(
            "
            SELECT platform, watchlist_games_json, keyword_templates_json, time_window_hours,
                   schedule_interval_hours, max_candidates_per_run, doubao_model,
                   auto_reason_mode, auto_reason_stages_json, auto_reason_judge_model,
                   auto_reason_max_rounds, auto_reason_timeout_ms, auto_reason_shadow_sample_rate,
                   auto_reason_min_confidence, auto_reason_do_nothing_margin
            FROM workbench_settings
            WHERE id = 1
            ",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, i64>(10)?,
                    row.get::<_, i64>(11)?,
                    row.get::<_, f64>(12)?,
                    row.get::<_, f64>(13)?,
                    row.get::<_, f64>(14)?,
                ))
            },
        )?;
        Ok(sanitize_settings(WorkbenchSettings {
            platform: row.0,
            watchlist_games: from_json_text(row.1)?,
            keyword_templates: from_json_text(row.2)?,
            time_window_hours: row.3,
            schedule_interval_hours: row.4,
            max_candidates_per_run: row.5,
            doubao_model: row.6,
            auto_reason_mode: row.7,
            auto_reason_stages: from_json_text(row.8)?,
            auto_reason_judge_model: row.9,
            auto_reason_max_rounds: row.10,
            auto_reason_timeout_ms: row.11,
            auto_reason_shadow_sample_rate: row.12,
            auto_reason_min_confidence: row.13,
            auto_reason_do_nothing_margin: row.14,
        }))
    }

    pub fn update_settings(&self, settings: WorkbenchSettings) -> Result<WorkbenchSettings> {
        let sanitized = sanitize_settings(settings);
        let conn = self.connect()?;
        conn.execute(
            "
            UPDATE workbench_settings
            SET platform = ?1,
                watchlist_games_json = ?2,
                keyword_templates_json = ?3,
                time_window_hours = ?4,
                schedule_interval_hours = ?5,
                max_candidates_per_run = ?6,
                doubao_model = ?7,
                auto_reason_mode = ?8,
                auto_reason_stages_json = ?9,
                auto_reason_judge_model = ?10,
                auto_reason_max_rounds = ?11,
                auto_reason_timeout_ms = ?12,
                auto_reason_shadow_sample_rate = ?13,
                auto_reason_min_confidence = ?14,
                auto_reason_do_nothing_margin = ?15,
                updated_at = ?16
            WHERE id = 1
            ",
            params![
                &sanitized.platform,
                serde_json::to_string(&sanitized.watchlist_games)?,
                serde_json::to_string(&sanitized.keyword_templates)?,
                sanitized.time_window_hours,
                sanitized.schedule_interval_hours,
                sanitized.max_candidates_per_run,
                sanitized.doubao_model.trim(),
                sanitized.auto_reason_mode.trim(),
                serde_json::to_string(&sanitized.auto_reason_stages)?,
                sanitized.auto_reason_judge_model.trim(),
                sanitized.auto_reason_max_rounds,
                sanitized.auto_reason_timeout_ms,
                sanitized.auto_reason_shadow_sample_rate,
                sanitized.auto_reason_min_confidence,
                sanitized.auto_reason_do_nothing_margin,
                utc_now(),
            ],
        )?;
        Ok(sanitized)
    }

    async fn run_scheduled_if_due(&self) -> Result<()> {
        let gate_state = self.provider_store.gate_state()?;
        let unlock_state = self.provider_store.unlock_state()?;
        if !gate_state.api_gate_passed || !unlock_state.unlocked {
            return Ok(());
        }

        let settings = self.settings()?;
        let doubao = self.provider_store.runtime_config(ProviderKind::Doubao)?;
        if resolve_doubao_model(&doubao, &settings).is_err() {
            return Ok(());
        }
        if !self.should_run(&settings)? {
            return Ok(());
        }

        let _guard = self.run_lock.lock().await;
        let run_id = format!("run_{}", short_hash(&format!("scheduled:{}", utc_now())));
        let started_at = utc_now();
        self.insert_run(&run_id, "schedule", "running", &started_at)?;
        match self.execute_run(&run_id).await {
            Ok(result) => self.persist_execution(&run_id, "schedule", &started_at, result)?,
            Err(error) => self.finish_run_failure(&run_id, &started_at, &error.to_string())?,
        }
        Ok(())
    }

    async fn execute_run(&self, run_id: &str) -> Result<RunExecution> {
        let settings = self.settings()?;
        let runtime = self.runtime_map()?;
        let history = self
            .rankings(
                "content",
                &RankingQuery {
                    kind: None,
                    query: None,
                    game_id: None,
                    event_type: None,
                    limit: Some(6),
                },
            )
            .unwrap_or_default()
            .into_iter()
            .map(|item| item.title)
            .collect::<Vec<_>>();

        let doubao = runtime_required(&runtime, ProviderKind::Doubao)?;
        let serper = runtime_required(&runtime, ProviderKind::Serper)?;
        let exa = runtime_required(&runtime, ProviderKind::Exa)?;
        let tavily = runtime_required(&runtime, ProviderKind::Tavily)?;
        let firecrawl = runtime_required(&runtime, ProviderKind::Firecrawl)?;

        let budget_map = self.budget_map()?;
        let serper_limit = budget_map
            .get(&ProviderKind::Serper)
            .map(BudgetMeter::allowed_this_run)
            .unwrap_or(0)
            .max(1);
        let exa_limit = budget_map
            .get(&ProviderKind::Exa)
            .map(BudgetMeter::allowed_this_run)
            .unwrap_or(0);
        let tavily_limit = budget_map
            .get(&ProviderKind::Tavily)
            .map(BudgetMeter::allowed_this_run)
            .unwrap_or(0);
        let firecrawl_limit = budget_map
            .get(&ProviderKind::Firecrawl)
            .map(BudgetMeter::allowed_this_run)
            .unwrap_or(0);
        let doubao_limit = budget_map
            .get(&ProviderKind::Doubao)
            .map(BudgetMeter::allowed_this_run)
            .unwrap_or(0)
            .max(1) as usize;

        let query_plan = match self
            .gateway
            .plan_queries(&doubao, &settings, &history)
            .await
        {
            Ok(plan) if !plan.queries.is_empty() => plan,
            _ => seeded_local_query_plan(&settings),
        };

        let mut raw_discovery: Vec<(String, SearchHit)> = Vec::new();
        let mut discovery: Vec<(String, SearchHit)> = Vec::new();
        for query in query_plan.queries.iter().take(serper_limit as usize) {
            let hits = self.gateway.serper_search(&serper, query).await?;
            self.record_budget_usage(ProviderKind::Serper, run_id, 1, "search", query)?;
            for hit in hits {
                raw_discovery.push((query.clone(), hit.clone()));
                if looks_like_douyin(&hit) {
                    discovery.push((query.clone(), hit));
                }
            }
        }

        let exa_seed_pool = if discovery.is_empty() {
            &raw_discovery
        } else {
            &discovery
        };
        for query in top_semantic_queries(exa_seed_pool, exa_limit as usize) {
            let hits = self.gateway.exa_expand(&exa, &query).await?;
            self.record_budget_usage(ProviderKind::Exa, run_id, 1, "expand", &query)?;
            for hit in hits {
                raw_discovery.push((query.clone(), hit.clone()));
                if looks_like_douyin(&hit) {
                    discovery.push((query.clone(), hit));
                }
            }
        }

        if discovery.is_empty() {
            discovery = fallback_relevant_hits(raw_discovery.clone(), &settings);
        }

        let mut shortlist = shortlist_candidates(discovery, &settings);
        if shortlist.is_empty() && !raw_discovery.is_empty() {
            shortlist = fallback_relevant_hits(raw_discovery.clone(), &settings);
        }
        let discovered_count = shortlist.len();
        let mut drafts = shortlist
            .into_iter()
            .take(doubao_limit)
            .enumerate()
            .map(|(index, (query, hit))| candidate_from_hit(&query, hit, index, &settings))
            .collect::<Vec<_>>();
        let shortlisted_count = drafts.len();

        if drafts.is_empty() {
            bail!("this run did not discover any eligible douyin candidates");
        }

        for candidate in drafts.iter_mut().take(firecrawl_limit as usize) {
            if let Ok(scrape) = self
                .gateway
                .firecrawl_scrape(&firecrawl, &candidate.url)
                .await
            {
                self.record_budget_usage(
                    ProviderKind::Firecrawl,
                    run_id,
                    1,
                    "scrape",
                    &candidate.url,
                )?;
                if !scrape.text.trim().is_empty() {
                    candidate.text = scrape.text;
                }
                if !scrape.description.trim().is_empty() {
                    candidate.description = scrape.description;
                }
                merge_tags(&mut candidate.tags, scrape.tags);
            }
        }

        for candidate in drafts.iter_mut().take(tavily_limit as usize) {
            if let Ok(verification) = self
                .gateway
                .tavily_verify(&tavily, &candidate.title, &candidate.url)
                .await
            {
                self.record_budget_usage(
                    ProviderKind::Tavily,
                    run_id,
                    1,
                    "verify",
                    &candidate.url,
                )?;
                candidate.cross_source_score = verification.score;
                candidate.metadata["verification_note"] = json!(verification.note);
            }
        }

        let auto_reason = self
            .auto_reason_candidate_refine(run_id, &settings, drafts)
            .await?;
        drafts = auto_reason.drafts;

        for candidate in &mut drafts {
            let structured = self
                .gateway
                .doubao_analyze(&doubao, &settings, candidate)
                .await?;
            self.record_budget_usage(ProviderKind::Doubao, run_id, 1, "analyze", &candidate.url)?;
            candidate.game_id = structured.game_id.clone();
            candidate.game_name = structured.game_name.clone();
            candidate.event_type = structured.event_type.clone();
            candidate.topic_tags = structured.topic_tags.clone();
            candidate.doubao_relevance_score = structured.doubao_relevance_score.clamp(0.0, 1.0);
            candidate.breakdown = BreakdownCard {
                content_id: candidate.content_id.clone(),
                content_summary: structured.content_summary,
                hook_points: structured.hook_points,
                core_conflict_or_value: structured.core_conflict_or_value,
                audience_fit: structured.audience_fit,
                adaptation_angles: structured.adaptation_angles,
                title_directions: structured.title_directions,
                topic_pool_reason: structured.topic_pool_reason,
            };
        }

        let raw_items = drafts
            .iter()
            .map(|candidate| spg_core::models::RawSourceItem {
                source_id: candidate.source_id.clone(),
                platform: candidate.platform.clone(),
                source_type: "web_search".to_string(),
                content_type: "video".to_string(),
                title: candidate.title.clone(),
                text: format!(
                    "{}\n{}\n{}",
                    candidate.summary, candidate.description, candidate.text
                ),
                author: candidate.author.clone(),
                published_at: candidate.published_at.clone(),
                url: candidate.url.clone(),
                raw_metrics: HashMap::from([
                    (
                        "engagement_score".to_string(),
                        json!(candidate.engagement_score),
                    ),
                    (
                        "freshness_score".to_string(),
                        json!(candidate.freshness_score),
                    ),
                    (
                        "cross_source_score".to_string(),
                        json!(candidate.cross_source_score),
                    ),
                    (
                        "doubao_relevance_score".to_string(),
                        json!(candidate.doubao_relevance_score),
                    ),
                ]),
                fetched_at: utc_now(),
                metadata: HashMap::from([
                    ("topic_tags".to_string(), json!(candidate.topic_tags)),
                    (
                        "discovery_query".to_string(),
                        json!(candidate.discovery_query),
                    ),
                ]),
            })
            .collect::<Vec<_>>();

        let bundle = load_asset_bundle(&default_asset_path())?;
        let pipeline = SPGInputPipeline::new(build_asset_index(bundle));
        let pipeline_result = pipeline.process(raw_items);
        let storage = SQLiteStorage::new(&self.db_path);
        storage.persist_result(&pipeline_result)?;

        let signal_map = pipeline_result.signals.iter().fold(
            HashMap::<String, (Option<String>, Option<String>, Vec<String>)>::new(),
            |mut acc, signal| {
                if let Some(source_id) = signal.raw_source_ids.first() {
                    acc.insert(
                        source_id.clone(),
                        (
                            signal.resolved_game_id.clone(),
                            signal.event_type.clone(),
                            signal.attribution_keywords.clone(),
                        ),
                    );
                }
                acc
            },
        );

        for candidate in &mut drafts {
            if let Some((game_id, event_type, tags)) = signal_map.get(&candidate.source_id) {
                if candidate.game_id.is_none() {
                    candidate.game_id = game_id.clone();
                }
                if candidate.event_type.is_none() {
                    candidate.event_type = event_type.clone();
                }
                merge_tags(&mut candidate.topic_tags, tags.clone());
            }
            candidate.hotness_score = calculate_hotness(candidate);
        }

        drafts.sort_by(|left, right| {
            right
                .hotness_score
                .partial_cmp(&left.hotness_score)
                .unwrap_or(Ordering::Equal)
                .then_with(|| left.search_rank.cmp(&right.search_rank))
        });

        let candidates = drafts
            .iter()
            .map(|candidate| DiscoveredCandidate {
                content_id: candidate.content_id.clone(),
                source_id: candidate.source_id.clone(),
                title: candidate.title.clone(),
                summary: candidate.summary.clone(),
                url: candidate.url.clone(),
                platform: candidate.platform.clone(),
                author: candidate.author.clone(),
                published_at: candidate.published_at.clone(),
                source_domain: candidate.source_domain.clone(),
                discovery_query: candidate.discovery_query.clone(),
                tags: candidate.tags.clone(),
                game_id: candidate.game_id.clone(),
                game_name: candidate.game_name.clone(),
                event_type: candidate.event_type.clone(),
                topic_tags: candidate.topic_tags.clone(),
                engagement_score: candidate.engagement_score,
                freshness_score: candidate.freshness_score,
                cross_source_score: candidate.cross_source_score,
                doubao_relevance_score: candidate.doubao_relevance_score,
                hotness_score: candidate.hotness_score,
                trust: trust_from_metadata_value(&candidate.metadata, utc_now()),
            })
            .collect::<Vec<_>>();

        let breakdowns = drafts
            .iter()
            .map(|candidate| candidate.breakdown.clone())
            .collect::<Vec<_>>();
        let usage = self.current_budgets()?;

        Ok(RunExecution {
            discovered_count,
            shortlisted_count,
            content_rankings: build_content_rankings(&candidates),
            game_rankings: build_game_rankings(&candidates),
            event_rankings: build_event_rankings(&candidates),
            candidates,
            breakdowns,
            usage,
            reasoning: auto_reason.artifacts,
        })
    }

    fn should_run(&self, settings: &WorkbenchSettings) -> Result<bool> {
        let Some(last_run) = self.latest_run()? else {
            return Ok(true);
        };
        if last_run.status == "running" {
            return Ok(false);
        }
        if last_run.status == "failed"
            && last_run.error_message.as_deref() == Some(STALE_RUN_RECOVERY_ERROR)
            && !self.has_current_snapshot()?
        {
            return Ok(true);
        }
        let anchor = last_run
            .finished_at
            .as_deref()
            .unwrap_or(&last_run.started_at);
        let finished = OffsetDateTime::parse(anchor, &Rfc3339)?;
        let due_at = finished + TimeDuration::hours(settings.schedule_interval_hours.max(1));
        Ok(OffsetDateTime::now_utc() >= due_at)
    }

    fn latest_run(&self) -> Result<Option<AnalysisRun>> {
        let conn = self.connect()?;
        conn.query_row(
            "
            SELECT run_id, trigger, status, started_at, finished_at,
                   candidate_count, shortlisted_count, content_count,
                   error_message, provider_usage_json
            FROM analysis_runs
            ORDER BY started_at DESC
            LIMIT 1
            ",
            [],
            map_analysis_run_row,
        )
        .optional()
        .context("failed to load latest workbench run")
    }

    fn run_by_id(&self, run_id: &str) -> Result<AnalysisRun> {
        let conn = self.connect()?;
        conn.query_row(
            "
            SELECT run_id, trigger, status, started_at, finished_at,
                   candidate_count, shortlisted_count, content_count,
                   error_message, provider_usage_json
            FROM analysis_runs
            WHERE run_id = ?1
            ",
            [run_id],
            map_analysis_run_row,
        )
        .context("failed to load run by id")
    }

    fn has_current_snapshot(&self) -> Result<bool> {
        let conn = self.connect()?;
        let exists = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM ranking_snapshots WHERE is_current = 1 LIMIT 1)",
            [],
            |row| row.get::<_, i64>(0),
        )?;
        Ok(exists != 0)
    }

    fn insert_run(
        &self,
        run_id: &str,
        trigger: &str,
        status: &str,
        started_at: &str,
    ) -> Result<()> {
        let conn = self.connect()?;
        conn.execute(
            "
            INSERT INTO analysis_runs(run_id, trigger, status, started_at, provider_usage_json)
            VALUES (?1, ?2, ?3, ?4, '[]')
            ",
            params![run_id, trigger, status, started_at],
        )?;
        Ok(())
    }

    fn finish_run_failure(
        &self,
        run_id: &str,
        _started_at: &str,
        error_message: &str,
    ) -> Result<()> {
        let conn = self.connect()?;
        conn.execute(
            "
            UPDATE analysis_runs
            SET status = 'failed',
                finished_at = ?2,
                error_message = ?3
            WHERE run_id = ?1
            ",
            params![run_id, utc_now(), error_message],
        )?;
        Ok(())
    }

    fn persist_execution(
        &self,
        run_id: &str,
        trigger: &str,
        _started_at: &str,
        result: RunExecution,
    ) -> Result<()> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;

        persist_auto_reason_artifacts(&tx, &result.reasoning)?;

        tx.execute(
            "UPDATE ranking_snapshots SET is_current = 0 WHERE is_current = 1",
            [],
        )?;

        for candidate in &result.candidates {
            tx.execute(
                "
                INSERT INTO content_candidates(
                    content_id, run_id, source_id, title, summary, url, platform, author,
                    published_at, source_domain, discovery_query, tags_json, game_id, game_name,
                    event_type, topic_tags_json, engagement_score, freshness_score,
                    cross_source_score, doubao_relevance_score, hotness_score, metadata_json, created_at
                ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                    ?18, ?19, ?20, ?21, ?22, ?23
                )
                ON CONFLICT(content_id) DO UPDATE SET
                    run_id = excluded.run_id,
                    source_id = excluded.source_id,
                    title = excluded.title,
                    summary = excluded.summary,
                    url = excluded.url,
                    platform = excluded.platform,
                    author = excluded.author,
                    published_at = excluded.published_at,
                    source_domain = excluded.source_domain,
                    discovery_query = excluded.discovery_query,
                    tags_json = excluded.tags_json,
                    game_id = excluded.game_id,
                    game_name = excluded.game_name,
                    event_type = excluded.event_type,
                    topic_tags_json = excluded.topic_tags_json,
                    engagement_score = excluded.engagement_score,
                    freshness_score = excluded.freshness_score,
                    cross_source_score = excluded.cross_source_score,
                    doubao_relevance_score = excluded.doubao_relevance_score,
                    hotness_score = excluded.hotness_score,
                    metadata_json = excluded.metadata_json,
                    created_at = excluded.created_at
                ",
                params![
                    &candidate.content_id,
                    run_id,
                    &candidate.source_id,
                    &candidate.title,
                    &candidate.summary,
                    &candidate.url,
                    &candidate.platform,
                    &candidate.author,
                    &candidate.published_at,
                    &candidate.source_domain,
                    &candidate.discovery_query,
                    serde_json::to_string(&candidate.tags)?,
                    candidate.game_id.clone(),
                    candidate.game_name.clone(),
                    candidate.event_type.clone(),
                    serde_json::to_string(&candidate.topic_tags)?,
                    candidate.engagement_score,
                    candidate.freshness_score,
                    candidate.cross_source_score,
                    candidate.doubao_relevance_score,
                    candidate.hotness_score,
                    serde_json::to_string(&candidate.trust)?,
                    utc_now(),
                ],
            )?;
        }

        for breakdown in &result.breakdowns {
            tx.execute(
                "
                INSERT INTO breakdown_cards(
                    content_id, run_id, content_summary, hook_points_json, core_conflict_or_value,
                    audience_fit, adaptation_angles_json, title_directions_json, topic_pool_reason,
                    created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                ON CONFLICT(content_id) DO UPDATE SET
                    run_id = excluded.run_id,
                    content_summary = excluded.content_summary,
                    hook_points_json = excluded.hook_points_json,
                    core_conflict_or_value = excluded.core_conflict_or_value,
                    audience_fit = excluded.audience_fit,
                    adaptation_angles_json = excluded.adaptation_angles_json,
                    title_directions_json = excluded.title_directions_json,
                    topic_pool_reason = excluded.topic_pool_reason,
                    created_at = excluded.created_at
                ",
                params![
                    &breakdown.content_id,
                    run_id,
                    &breakdown.content_summary,
                    serde_json::to_string(&breakdown.hook_points)?,
                    &breakdown.core_conflict_or_value,
                    &breakdown.audience_fit,
                    serde_json::to_string(&breakdown.adaptation_angles)?,
                    serde_json::to_string(&breakdown.title_directions)?,
                    &breakdown.topic_pool_reason,
                    utc_now(),
                ],
            )?;
        }

        for snapshot in result
            .content_rankings
            .iter()
            .chain(result.game_rankings.iter())
            .chain(result.event_rankings.iter())
        {
            tx.execute(
                "
                INSERT INTO ranking_snapshots(
                    snapshot_id, run_id, kind, entity_id, content_id, title, subtitle,
                    score, rank_value, game_id, event_type, is_current, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 1, ?12)
                ",
                params![
                    format!(
                        "snap_{}",
                        short_hash(&format!(
                            "{}:{}:{}",
                            run_id, snapshot.kind, snapshot.entity_id
                        ))
                    ),
                    run_id,
                    &snapshot.kind,
                    &snapshot.entity_id,
                    snapshot.content_id.clone(),
                    &snapshot.title,
                    &snapshot.subtitle,
                    snapshot.score,
                    snapshot.rank,
                    snapshot.game_id.clone(),
                    snapshot.event_type.clone(),
                    utc_now(),
                ],
            )?;
        }

        tx.execute(
            "
            UPDATE analysis_runs
            SET trigger = ?2,
                status = 'succeeded',
                finished_at = ?3,
                candidate_count = ?4,
                shortlisted_count = ?5,
                content_count = ?6,
                provider_usage_json = ?7
            WHERE run_id = ?1
            ",
            params![
                run_id,
                trigger,
                utc_now(),
                result.discovered_count as i64,
                result.shortlisted_count as i64,
                result.content_rankings.len() as i64,
                serde_json::to_string(&result.usage)?,
            ],
        )?;

        tx.commit()?;
        Ok(())
    }

    fn current_budgets(&self) -> Result<Vec<ApiBudgetUsageView>> {
        let budget_map = self.budget_map()?;
        let mut views = budget_map
            .into_values()
            .map(|meter| meter.as_view())
            .collect::<Vec<_>>();
        views.sort_by(|left, right| left.provider.cmp(&right.provider));
        Ok(views)
    }

    fn budget_map(&self) -> Result<HashMap<ProviderKind, BudgetMeter>> {
        let runtime = self.runtime_map()?;
        let conn = self.connect()?;
        let now = OffsetDateTime::now_utc();
        let day_start = now
            .date()
            .with_hms(0, 0, 0)
            .expect("day start")
            .assume_utc();
        let month_start = now
            .date()
            .replace_day(1)
            .expect("month start")
            .with_hms(0, 0, 0)
            .expect("month start time")
            .assume_utc();

        let mut map = HashMap::new();
        for provider in ProviderKind::ALL {
            let Some(config) = runtime.get(&provider).cloned() else {
                continue;
            };
            let used_today: i64 = conn.query_row(
                "SELECT COALESCE(SUM(units), 0) FROM api_budget_usage WHERE provider = ?1 AND recorded_at >= ?2",
                params![provider.slug(), day_start.format(&Rfc3339)?],
                |row| row.get(0),
            )?;
            let used_monthly: i64 = conn.query_row(
                "SELECT COALESCE(SUM(units), 0) FROM api_budget_usage WHERE provider = ?1 AND recorded_at >= ?2",
                params![provider.slug(), month_start.format(&Rfc3339)?],
                |row| row.get(0),
            )?;
            map.insert(
                provider,
                BudgetMeter {
                    provider,
                    enabled: config.enabled,
                    policy: budget_policy(provider, &config),
                    used_monthly,
                    used_today,
                },
            );
        }
        Ok(map)
    }

    fn runtime_map(&self) -> Result<HashMap<ProviderKind, ProviderRuntimeConfig>> {
        Ok(self
            .provider_store
            .runtime_configs()?
            .into_iter()
            .map(|config| (config.provider, config))
            .collect())
    }

    fn record_budget_usage(
        &self,
        provider: ProviderKind,
        run_id: &str,
        units: i64,
        endpoint: &str,
        note: &str,
    ) -> Result<()> {
        let conn = self.connect()?;
        conn.execute(
            "
            INSERT INTO api_budget_usage(provider, run_id, units, endpoint, recorded_at, note)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ",
            params![provider.slug(), run_id, units, endpoint, utc_now(), note],
        )?;
        Ok(())
    }

    fn connect(&self) -> Result<Connection> {
        let conn = Connection::open(&self.db_path)
            .with_context(|| format!("failed to open {}", self.db_path.display()))?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        Ok(conn)
    }

    fn recover_stale_runs(&self, conn: &Connection) -> Result<()> {
        let cutoff = OffsetDateTime::now_utc() - TimeDuration::minutes(STALE_RUN_TIMEOUT_MINUTES);
        let mut statement = conn.prepare(
            "
            SELECT run_id, started_at
            FROM analysis_runs
            WHERE status = 'running'
            ",
        )?;
        let stale_runs = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .filter_map(|(run_id, started_at)| {
                let started = OffsetDateTime::parse(&started_at, &Rfc3339).ok()?;
                (started <= cutoff).then_some((run_id, started_at))
            })
            .collect::<Vec<_>>();

        for (run_id, _) in stale_runs {
            conn.execute(
                "
                UPDATE analysis_runs
                SET status = 'failed',
                    finished_at = ?2,
                    error_message = ?3
                WHERE run_id = ?1
                ",
                params![run_id, utc_now(), STALE_RUN_RECOVERY_ERROR],
            )?;
        }
        Ok(())
    }
}

impl BudgetMeter {
    fn allowed_this_run(&self) -> i64 {
        if !self.enabled {
            return 0;
        }
        if self.provider == ProviderKind::Doubao {
            return self.policy.per_run_limit.max(8);
        }
        let monthly_cap = (self.policy.monthly_limit - self.policy.reserve_pool).max(0);
        let monthly_remaining = (monthly_cap - self.used_monthly).max(0);
        let daily_remaining = (self.policy.daily_soft_limit - self.used_today).max(0);
        let allowed = monthly_remaining
            .min(daily_remaining)
            .min(self.policy.per_run_limit);
        if self.provider == ProviderKind::Serper
            && allowed == 0
            && self.used_monthly < self.policy.monthly_limit
            && self.used_today < self.policy.daily_soft_limit
            && self.policy.minimum_floor > 0
        {
            self.policy.minimum_floor
        } else {
            allowed
        }
    }

    fn as_view(&self) -> ApiBudgetUsageView {
        ApiBudgetUsageView {
            provider: self.provider.display_name().to_string(),
            enabled: self.enabled,
            monthly_limit: self.policy.monthly_limit,
            daily_soft_limit: self.policy.daily_soft_limit,
            reserve_pool: self.policy.reserve_pool,
            per_run_limit: self.policy.per_run_limit,
            used_monthly: self.used_monthly,
            used_today: self.used_today,
            remaining_monthly: (self.policy.monthly_limit - self.used_monthly).max(0),
            remaining_today: (self.policy.daily_soft_limit - self.used_today).max(0),
            allowed_this_run: self.allowed_this_run(),
        }
    }
}

fn map_analysis_run_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AnalysisRun> {
    Ok(AnalysisRun {
        run_id: row.get(0)?,
        trigger: row.get(1)?,
        status: row.get(2)?,
        started_at: row.get(3)?,
        finished_at: row.get(4)?,
        candidate_count: row.get(5)?,
        shortlisted_count: row.get(6)?,
        content_count: row.get(7)?,
        error_message: row.get(8)?,
        provider_usage: serde_json::from_str(&row.get::<_, String>(9)?).unwrap_or_default(),
    })
}

fn budget_policy(provider: ProviderKind, config: &ProviderRuntimeConfig) -> BudgetPolicy {
    let plan = match provider {
        ProviderKind::Doubao => BudgetPolicy {
            monthly_limit: 99_999,
            daily_soft_limit: 5_000,
            reserve_pool: 0,
            per_run_limit: 12,
            minimum_floor: 0,
        },
        ProviderKind::Serper => BudgetPolicy {
            monthly_limit: 2_500,
            daily_soft_limit: 60,
            reserve_pool: 700,
            per_run_limit: 5,
            minimum_floor: 1,
        },
        ProviderKind::Exa => BudgetPolicy {
            monthly_limit: 1_000,
            daily_soft_limit: 24,
            reserve_pool: 280,
            per_run_limit: 2,
            minimum_floor: 0,
        },
        ProviderKind::Tavily => BudgetPolicy {
            monthly_limit: 1_000,
            daily_soft_limit: 24,
            reserve_pool: 280,
            per_run_limit: 2,
            minimum_floor: 0,
        },
        ProviderKind::Firecrawl => BudgetPolicy {
            monthly_limit: 500,
            daily_soft_limit: 12,
            reserve_pool: 140,
            per_run_limit: 1,
            minimum_floor: 0,
        },
    };
    BudgetPolicy {
        monthly_limit: plan.monthly_limit.min(config.monthly_limit.max(1)),
        daily_soft_limit: plan.daily_soft_limit.min(config.daily_soft_limit.max(1)),
        ..plan
    }
}

fn shortlist_candidates(
    discovery: Vec<(String, SearchHit)>,
    settings: &WorkbenchSettings,
) -> Vec<(String, SearchHit)> {
    let watch_terms = seed_watch_terms(&settings.watchlist_games);
    let now = OffsetDateTime::now_utc();
    let limit = settings.max_candidates_per_run.clamp(10, 50) as usize;
    let mut seen = BTreeSet::new();
    let recent_hits = discovery
        .into_iter()
        .filter(|(_, hit)| seen.insert(normalized_identity(&hit.url, &hit.title)))
        .filter(|(_, hit)| {
            within_time_window(hit.published_at.as_deref(), settings.time_window_hours, now)
        })
        .collect::<Vec<_>>();

    let strict_hits = recent_hits
        .iter()
        .filter(|(_, hit)| looks_slg_relevant(hit, &watch_terms))
        .cloned()
        .take(limit)
        .collect::<Vec<_>>();
    if !strict_hits.is_empty() {
        return strict_hits;
    }

    let watchlist_hits = recent_hits
        .iter()
        .filter(|(_, hit)| matches_watch_terms(&relevance_haystack(hit), &watch_terms))
        .cloned()
        .take(limit)
        .collect::<Vec<_>>();
    if !watchlist_hits.is_empty() {
        return watchlist_hits;
    }

    recent_hits.into_iter().take(limit).collect()
}

fn fallback_relevant_hits(
    discovery: Vec<(String, SearchHit)>,
    settings: &WorkbenchSettings,
) -> Vec<(String, SearchHit)> {
    let watch_terms = seed_watch_terms(&settings.watchlist_games);
    let now = OffsetDateTime::now_utc();
    let limit = settings.max_candidates_per_run.clamp(10, 50) as usize;
    let mut seen = BTreeSet::new();

    discovery
        .into_iter()
        .filter(|(_, hit)| seen.insert(normalized_identity(&hit.url, &hit.title)))
        .filter(|(_, hit)| {
            within_time_window(hit.published_at.as_deref(), settings.time_window_hours, now)
        })
        .filter(|(_, hit)| looks_slg_relevant(hit, &watch_terms))
        .take(limit)
        .map(|(query, mut hit)| {
            if !hit.tags.iter().any(|tag| tag == "fallback_non_douyin") {
                hit.tags.push("fallback_non_douyin".to_string());
            }
            (query, hit)
        })
        .collect()
}

fn candidate_from_hit(
    query: &str,
    hit: SearchHit,
    index: usize,
    settings: &WorkbenchSettings,
) -> CandidateDraft {
    let published_at = hit.published_at.clone().unwrap_or_else(utc_now);
    let freshness_score = freshness_score(&published_at, settings.time_window_hours);
    let source_platform_match = looks_like_douyin(&hit);
    let fallback_non_douyin = hit.tags.iter().any(|tag| tag == "fallback_non_douyin");
    let content_id = format!(
        "content_{}",
        short_hash(&format!("{}:{}", hit.url, hit.title))
    );
    CandidateDraft {
        content_id: content_id.clone(),
        source_id: format!("douyin_{}", short_hash(&hit.url)),
        title: hit.title,
        summary: hit.snippet,
        url: hit.url,
        platform: "douyin".to_string(),
        author: hit.author,
        published_at,
        source_domain: hit.source_domain,
        discovery_query: query.to_string(),
        tags: hit.tags,
        text: String::new(),
        description: String::new(),
        search_rank: index + 1,
        engagement_score: hit.engagement_hint.clamp(0.0, 1.0),
        freshness_score,
        cross_source_score: 0.2,
        doubao_relevance_score: 0.0,
        hotness_score: 0.0,
        game_id: None,
        game_name: None,
        event_type: None,
        topic_tags: Vec::new(),
        breakdown: BreakdownCard {
            content_id,
            content_summary: String::new(),
            hook_points: Vec::new(),
            core_conflict_or_value: String::new(),
            audience_fit: String::new(),
            adaptation_angles: Vec::new(),
            title_directions: Vec::new(),
            topic_pool_reason: String::new(),
        },
        metadata: json!({
            "source_platform_match": source_platform_match,
            "fallback_non_douyin": fallback_non_douyin,
        }),
    }
}

fn build_content_rankings(candidates: &[DiscoveredCandidate]) -> Vec<RankingSnapshot> {
    candidates
        .iter()
        .enumerate()
        .map(|(index, item)| RankingSnapshot {
            kind: "content".to_string(),
            rank: (index + 1) as i64,
            entity_id: item.content_id.clone(),
            content_id: Some(item.content_id.clone()),
            title: item.title.clone(),
            subtitle: format!(
                "{} | {} | {}",
                item.game_name
                    .clone()
                    .unwrap_or_else(|| "未归因游戏".to_string()),
                item.event_type
                    .clone()
                    .unwrap_or_else(|| "general".to_string()),
                item.source_domain
            ),
            score: item.hotness_score,
            game_id: item.game_id.clone(),
            event_type: item.event_type.clone(),
            platform: Some(item.platform.clone()),
            author: Some(item.author.clone()),
            published_at: Some(item.published_at.clone()),
            source_domain: Some(item.source_domain.clone()),
            source_url: Some(item.url.clone()),
            trust: Some(item.trust.clone()),
        })
        .collect()
}

fn build_game_rankings(candidates: &[DiscoveredCandidate]) -> Vec<RankingSnapshot> {
    let mut groups: BTreeMap<String, (String, f64, usize)> = BTreeMap::new();
    for candidate in candidates {
        let key = candidate
            .game_id
            .clone()
            .unwrap_or_else(|| "unresolved".to_string());
        let title = candidate
            .game_name
            .clone()
            .unwrap_or_else(|| "未归因游戏".to_string());
        let entry = groups.entry(key).or_insert((title, 0.0, 0));
        entry.1 += candidate.hotness_score;
        entry.2 += 1;
    }
    let mut items = groups.into_iter().collect::<Vec<_>>();
    items.sort_by(|left, right| {
        right
            .1
             .1
            .partial_cmp(&left.1 .1)
            .unwrap_or(Ordering::Equal)
    });
    items
        .into_iter()
        .enumerate()
        .map(
            |(index, (game_id, (title, score, count)))| RankingSnapshot {
                kind: "game".to_string(),
                rank: (index + 1) as i64,
                entity_id: game_id.clone(),
                content_id: None,
                title,
                subtitle: format!("{count} 条内容进入当前快照"),
                score,
                game_id: Some(game_id),
                event_type: None,
                platform: None,
                author: None,
                published_at: None,
                source_domain: None,
                source_url: None,
                trust: None,
            },
        )
        .collect()
}

fn build_event_rankings(candidates: &[DiscoveredCandidate]) -> Vec<RankingSnapshot> {
    let mut groups: BTreeMap<String, (String, f64, usize, Option<String>)> = BTreeMap::new();
    for candidate in candidates {
        let event_type = candidate
            .event_type
            .clone()
            .unwrap_or_else(|| "general".to_string());
        let entry = groups.entry(event_type.clone()).or_insert((
            event_type.clone(),
            0.0,
            0,
            candidate.game_id.clone(),
        ));
        entry.1 += candidate.hotness_score;
        entry.2 += 1;
    }
    let mut items = groups.into_iter().collect::<Vec<_>>();
    items.sort_by(|left, right| {
        right
            .1
             .1
            .partial_cmp(&left.1 .1)
            .unwrap_or(Ordering::Equal)
    });
    items
        .into_iter()
        .enumerate()
        .map(
            |(index, (event_type, (title, score, count, game_id)))| RankingSnapshot {
                kind: "event".to_string(),
                rank: (index + 1) as i64,
                entity_id: event_type.clone(),
                content_id: None,
                title,
                subtitle: format!("{count} 条内容触发此类事件"),
                score,
                game_id,
                event_type: Some(event_type),
                platform: None,
                author: None,
                published_at: None,
                source_domain: None,
                source_url: None,
                trust: None,
            },
        )
        .collect()
}

fn calculate_hotness(candidate: &CandidateDraft) -> f64 {
    candidate.engagement_score * 0.35
        + candidate.freshness_score * 0.25
        + candidate.cross_source_score * 0.20
        + candidate.doubao_relevance_score * 0.20
}

fn resolve_doubao_model(
    config: &ProviderRuntimeConfig,
    settings: &WorkbenchSettings,
) -> Result<String> {
    let candidate = if settings.doubao_model.trim().is_empty() {
        config.default_role.trim()
    } else {
        settings.doubao_model.trim()
    };
    if candidate.is_empty() || candidate == "primary_reasoner" {
        bail!("doubao model/endpoint is not configured in workbench settings");
    }
    Ok(candidate.to_string())
}

fn required_api_key(config: &ProviderRuntimeConfig) -> Result<&str> {
    config
        .api_key
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow!("{} api key is missing", config.provider.display_name()))
}

fn runtime_required(
    runtime: &HashMap<ProviderKind, ProviderRuntimeConfig>,
    provider: ProviderKind,
) -> Result<ProviderRuntimeConfig> {
    let config = runtime
        .get(&provider)
        .cloned()
        .ok_or_else(|| anyhow!("missing runtime config for {}", provider.display_name()))?;
    if !config.enabled {
        bail!("{} is disabled", provider.display_name());
    }
    Ok(config)
}

#[allow(dead_code)]
fn default_keyword_templates() -> Vec<String> {
    vec![
        "SLG 赛季 热点".to_string(),
        "SLG 联盟 对抗".to_string(),
        "SLG 开荒 阵容".to_string(),
        "SLG 活动 联动".to_string(),
    ]
}

fn seed_game_profiles() -> &'static [SeedGameProfile] {
    const PROFILES: &[SeedGameProfile] = &[
        SeedGameProfile {
            canonical: "率土之滨",
            aliases: &["率土"],
            focus_terms: &["赛季", "开荒", "同盟", "配将"],
        },
        SeedGameProfile {
            canonical: "三国群英传策定九州",
            aliases: &["三国群英传：策定九州", "策定九州"],
            focus_terms: &["国战", "开荒", "阵容", "九州"],
        },
        SeedGameProfile {
            canonical: "三国谋定天下",
            aliases: &["三谋", "谋定天下"],
            focus_terms: &["配将", "阵容", "赛季", "同盟"],
        },
        SeedGameProfile {
            canonical: "无尽冬日",
            aliases: &["冬日"],
            focus_terms: &["联盟", "生存", "发育", "活动"],
        },
        SeedGameProfile {
            canonical: "三国冰河时代",
            aliases: &["三国：冰河时代", "冰河时代"],
            focus_terms: &["冰雪", "SLG", "开荒", "阵容"],
        },
        SeedGameProfile {
            canonical: "天下归心",
            aliases: &["归心"],
            focus_terms: &["国战", "策略", "配队", "联盟"],
        },
        SeedGameProfile {
            canonical: "九牧之野",
            aliases: &["九牧"],
            focus_terms: &["开荒", "阵容", "热点", "赛季"],
        },
    ];
    PROFILES
}

fn default_seed_keyword_templates() -> Vec<String> {
    seed_game_profiles()
        .iter()
        .map(|profile| format!("{} {}", profile.canonical, profile.focus_terms.join(" ")))
        .collect()
}

fn default_watchlist_games() -> Vec<String> {
    seed_game_profiles()
        .iter()
        .map(|profile| profile.canonical.to_string())
        .collect()
}

fn requested_seed_profiles<'a>(watchlist: &'a [String]) -> Vec<&'static SeedGameProfile> {
    let requested = watchlist
        .iter()
        .map(|item| item.trim().to_lowercase())
        .filter(|item| !item.is_empty())
        .collect::<BTreeSet<_>>();
    let mut profiles = seed_game_profiles()
        .iter()
        .filter(|profile| {
            requested.is_empty()
                || requested.contains(&profile.canonical.to_lowercase())
                || profile
                    .aliases
                    .iter()
                    .any(|alias| requested.contains(&alias.to_lowercase()))
        })
        .collect::<Vec<_>>();
    if profiles.is_empty() {
        profiles = seed_game_profiles().iter().collect();
    }
    profiles
}

fn seed_watch_terms(watchlist: &[String]) -> Vec<String> {
    let mut terms = requested_seed_profiles(watchlist)
        .into_iter()
        .flat_map(|profile| {
            std::iter::once(profile.canonical)
                .chain(profile.aliases.iter().copied())
                .map(|value| value.to_lowercase())
                .collect::<Vec<_>>()
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    for item in watchlist {
        let trimmed = item.trim().to_lowercase();
        if !trimmed.is_empty() && !terms.iter().any(|term| term == &trimmed) {
            terms.push(trimmed);
        }
    }
    terms
}

fn ensure_column(conn: &Connection, table: &str, column: &str, definition: &str) -> Result<()> {
    let mut statement = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if columns.iter().any(|existing| existing == column) {
        return Ok(());
    }
    conn.execute(
        &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
        [],
    )?;
    Ok(())
}

fn persist_auto_reason_artifacts(
    tx: &Transaction<'_>,
    artifacts: &AutoReasonArtifacts,
) -> Result<()> {
    for version in &artifacts.candidate_versions {
        tx.execute(
            "
            INSERT INTO candidate_versions(
                version_id, run_id, content_id, parent_content_id, stage, variant_kind, source_id,
                title, summary, url, author, published_at, tags_json, topic_tags_json,
                metadata_json, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
            ",
            params![
                &version.version_id,
                &version.run_id,
                &version.content_id,
                &version.parent_content_id,
                &version.stage,
                &version.variant_kind,
                &version.source_id,
                &version.title,
                &version.summary,
                &version.url,
                &version.author,
                &version.published_at,
                &version.tags_json,
                &version.topic_tags_json,
                &version.metadata_json,
                &version.created_at,
            ],
        )?;
    }

    for judgement in &artifacts.reasoning_judgements {
        tx.execute(
            "
            INSERT INTO reasoning_judgements(
                decision_id, run_id, stage, content_id, candidate_a_version_id,
                candidate_b_version_id, candidate_ab_version_id, winner_version_id,
                judge_round, judge_model, judge_labels_json, judge_ranking_json,
                do_nothing, stop_reason, confidence, latency_ms, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
            ",
            params![
                &judgement.decision_id,
                &judgement.run_id,
                &judgement.stage,
                &judgement.content_id,
                &judgement.candidate_a_version_id,
                &judgement.candidate_b_version_id,
                &judgement.candidate_ab_version_id,
                &judgement.winner_version_id,
                judgement.judge_round,
                &judgement.judge_model,
                &judgement.judge_labels_json,
                &judgement.judge_ranking_json,
                judgement.do_nothing,
                &judgement.stop_reason,
                judgement.confidence,
                judgement.latency_ms,
                &judgement.created_at,
            ],
        )?;
    }

    for metric in &artifacts.stage_metrics {
        tx.execute(
            "
            INSERT INTO stage_runtime_metrics(
                metric_id, run_id, stage, object_id, mode, status, attempts, latency_ms,
                provider_usage_json, note, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            ",
            params![
                &metric.metric_id,
                &metric.run_id,
                &metric.stage,
                &metric.object_id,
                &metric.mode,
                &metric.status,
                metric.attempts,
                metric.latency_ms,
                &metric.provider_usage_json,
                &metric.note,
                &metric.created_at,
            ],
        )?;
    }

    Ok(())
}

fn candidate_version_artifact(
    run_id: &str,
    candidate: &CandidateDraft,
    variant: &ShadowCandidateVariant,
    mode: &str,
) -> Result<CandidateVersionArtifact> {
    let created_at = utc_now();
    Ok(CandidateVersionArtifact {
        version_id: format!(
            "cv_{}",
            short_hash(&format!(
                "{}:{}:{}:{}",
                run_id, candidate.content_id, variant.kind, candidate.title
            ))
        ),
        run_id: run_id.to_string(),
        content_id: candidate.content_id.clone(),
        parent_content_id: candidate.content_id.clone(),
        stage: AUTO_REASON_STAGE_CANDIDATE_REFINE.to_string(),
        variant_kind: variant.kind.clone(),
        source_id: candidate.source_id.clone(),
        title: variant.title.clone(),
        summary: variant.summary.clone(),
        url: candidate.url.clone(),
        author: candidate.author.clone(),
        published_at: candidate.published_at.clone(),
        tags_json: serde_json::to_string(&candidate.tags)?,
        topic_tags_json: serde_json::to_string(&candidate.topic_tags)?,
        metadata_json: serde_json::to_string(&json!({
            "auto_reason": {
                "mode": mode,
                "stage": AUTO_REASON_STAGE_CANDIDATE_REFINE,
                "variant_kind": variant.kind,
                "score": variant.score,
                "rationale": variant.rationale,
            },
            "candidate_metadata": candidate.metadata,
        }))?,
        created_at,
    })
}

fn compact_shadow_text(value: &str, limit: usize) -> String {
    let compact = value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string();
    if compact.chars().count() <= limit {
        return compact;
    }
    compact
        .chars()
        .take(limit)
        .collect::<String>()
        .trim()
        .to_string()
}

fn shadow_base_score(candidate: &CandidateDraft) -> f64 {
    let weighted = candidate.hotness_score * 0.36
        + candidate.cross_source_score * 0.22
        + candidate.freshness_score * 0.16
        + candidate.engagement_score * 0.16
        + candidate.doubao_relevance_score * 0.10;
    weighted.clamp(0.0, 1.0)
}

fn shadow_candidate_variants(candidate: &CandidateDraft) -> Vec<ShadowCandidateVariant> {
    let base_score = shadow_base_score(candidate);
    let verification_note = candidate
        .metadata
        .get("verification_note")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    let description = candidate.description.trim().to_string();
    let text_excerpt = compact_shadow_text(&candidate.text, 100);
    let tag_hint = candidate
        .tags
        .iter()
        .take(3)
        .cloned()
        .collect::<Vec<_>>()
        .join(" / ");
    let query_focus = query_word_seed(&candidate.discovery_query, &candidate.title);

    let a_summary = compact_shadow_text(&candidate.summary, 140);
    let mut b_parts = vec![a_summary.clone()];
    if !verification_note.is_empty() {
        b_parts.push(format!("外部验证：{verification_note}"));
    }
    if !description.is_empty() {
        b_parts.push(format!(
            "页面描述：{}",
            compact_shadow_text(&description, 60)
        ));
    }
    let b_summary = compact_shadow_text(&b_parts.join("；"), 180);
    let b_title = if verification_note.is_empty() {
        format!("{} | {}热点复核", candidate.title.trim(), query_focus)
    } else {
        format!("{} | 已验证热点", candidate.title.trim())
    };

    let mut ab_parts = vec![a_summary.clone()];
    if !verification_note.is_empty() {
        ab_parts.push(format!("验证线索：{verification_note}"));
    }
    if !text_excerpt.is_empty() {
        ab_parts.push(format!("正文摘要：{text_excerpt}"));
    }
    if !tag_hint.is_empty() {
        ab_parts.push(format!("标签：{tag_hint}"));
    }
    let ab_summary = compact_shadow_text(&ab_parts.join("；"), 200);
    let ab_title = if tag_hint.is_empty() {
        format!("{} | {}整合版", candidate.title.trim(), query_focus)
    } else {
        format!("{} | {}整合版", candidate.title.trim(), tag_hint)
    };

    let b_score = (base_score
        + if !verification_note.is_empty() {
            0.08
        } else {
            0.03
        }
        + if !description.is_empty() { 0.04 } else { 0.0 })
    .clamp(0.0, 1.0);
    let ab_score = (base_score
        + if !verification_note.is_empty() {
            0.06
        } else {
            0.02
        }
        + if !text_excerpt.is_empty() { 0.05 } else { 0.0 }
        + if !tag_hint.is_empty() { 0.02 } else { 0.0 })
    .clamp(0.0, 1.0);

    vec![
        ShadowCandidateVariant {
            kind: "A".to_string(),
            title: candidate.title.trim().to_string(),
            summary: a_summary,
            score: base_score,
            rationale: "保留原始候选，作为当前 incumbent 基线。".to_string(),
        },
        ShadowCandidateVariant {
            kind: "B".to_string(),
            title: compact_shadow_text(&b_title, 120),
            summary: b_summary,
            score: b_score,
            rationale: "挑战者版本强化了验证信号与页面描述信息。".to_string(),
        },
        ShadowCandidateVariant {
            kind: "AB".to_string(),
            title: compact_shadow_text(&ab_title, 120),
            summary: ab_summary,
            score: ab_score,
            rationale: "融合版本整合了原摘要、验证线索和正文摘录。".to_string(),
        },
    ]
}

fn stage_runtime_metric_artifact(
    run_id: &str,
    object_id: &str,
    mode: &str,
    status: &str,
    note: &str,
) -> StageRuntimeMetricArtifact {
    let created_at = utc_now();
    StageRuntimeMetricArtifact {
        metric_id: format!(
            "metric_{}",
            short_hash(&format!(
                "{}:{}:{}:{}:{}",
                run_id, AUTO_REASON_STAGE_CANDIDATE_REFINE, object_id, mode, created_at
            ))
        ),
        run_id: run_id.to_string(),
        stage: AUTO_REASON_STAGE_CANDIDATE_REFINE.to_string(),
        object_id: object_id.to_string(),
        mode: mode.to_string(),
        status: status.to_string(),
        attempts: 1,
        latency_ms: 0,
        provider_usage_json: "[]".to_string(),
        note: note.to_string(),
        created_at,
    }
}

fn build_shadow_auto_reason_artifacts(
    run_id: &str,
    settings: &WorkbenchSettings,
    drafts: &[CandidateDraft],
) -> Result<AutoReasonArtifacts> {
    let mode = normalize_auto_reason_mode(&settings.auto_reason_mode);
    let mut artifacts = AutoReasonArtifacts::default();

    for candidate in drafts {
        let variants = shadow_candidate_variants(candidate);
        let baseline_score = variants
            .iter()
            .find(|variant| variant.kind == "A")
            .map(|variant| variant.score)
            .unwrap_or(0.0);
        let mut ranked_variants = variants.clone();
        ranked_variants.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(Ordering::Equal)
                .then_with(|| left.kind.cmp(&right.kind))
        });
        let top_variant =
            ranked_variants
                .first()
                .cloned()
                .unwrap_or_else(|| ShadowCandidateVariant {
                    kind: "A".to_string(),
                    title: candidate.title.clone(),
                    summary: candidate.summary.clone(),
                    score: baseline_score,
                    rationale: "Fallback to incumbent baseline.".to_string(),
                });
        let retain_baseline = top_variant.kind == "A"
            || (top_variant.score - baseline_score) <= settings.auto_reason_do_nothing_margin;
        let winning_kind = if retain_baseline {
            "A".to_string()
        } else {
            top_variant.kind.clone()
        };
        let ranked_labels = ranked_variants
            .iter()
            .map(|variant| variant.kind.clone())
            .collect::<Vec<_>>();
        let mut version_records = variants
            .iter()
            .map(|variant| candidate_version_artifact(run_id, candidate, variant, &mode))
            .collect::<Result<Vec<_>>>()?;
        let winner_record = version_records
            .iter()
            .find(|record| record.variant_kind == winning_kind)
            .cloned()
            .unwrap_or_else(|| version_records[0].clone());
        let created_at = utc_now();

        artifacts.candidate_versions.append(&mut version_records);
        artifacts
            .reasoning_judgements
            .push(ReasoningJudgementArtifact {
                decision_id: format!(
                    "judge_{}",
                    short_hash(&format!("{}:{}:{}", run_id, candidate.content_id, mode))
                ),
                run_id: run_id.to_string(),
                stage: AUTO_REASON_STAGE_CANDIDATE_REFINE.to_string(),
                content_id: candidate.content_id.clone(),
                candidate_a_version_id: artifacts
                    .candidate_versions
                    .iter()
                    .rev()
                    .find(|record| {
                        record.content_id == candidate.content_id && record.variant_kind == "A"
                    })
                    .map(|record| record.version_id.clone())
                    .unwrap_or_else(|| winner_record.version_id.clone()),
                candidate_b_version_id: artifacts
                    .candidate_versions
                    .iter()
                    .rev()
                    .find(|record| {
                        record.content_id == candidate.content_id && record.variant_kind == "B"
                    })
                    .map(|record| record.version_id.clone())
                    .unwrap_or_else(|| winner_record.version_id.clone()),
                candidate_ab_version_id: artifacts
                    .candidate_versions
                    .iter()
                    .rev()
                    .find(|record| {
                        record.content_id == candidate.content_id && record.variant_kind == "AB"
                    })
                    .map(|record| record.version_id.clone())
                    .unwrap_or_else(|| winner_record.version_id.clone()),
                winner_version_id: winner_record.version_id.clone(),
                judge_round: 1,
                judge_model: settings.auto_reason_judge_model.clone(),
                judge_labels_json: serde_json::to_string(&vec!["A", "B", "AB", "do_nothing"])?,
                judge_ranking_json: serde_json::to_string(&ranked_labels)?,
                do_nothing: if retain_baseline { 1 } else { 0 },
                stop_reason: if retain_baseline {
                    "no_improvement".to_string()
                } else {
                    format!("shadow_select_{}", winning_kind.to_ascii_lowercase())
                },
                confidence: (top_variant.score - baseline_score).max(0.0),
                latency_ms: 0,
                created_at: created_at.clone(),
            });
        artifacts.stage_metrics.push(stage_runtime_metric_artifact(
            run_id,
            &candidate.content_id,
            &mode,
            "shadow_recorded",
            &format!(
                "Shadow ranked {:?}; chosen {} for analysis only; baseline candidate continues downstream.",
                ranked_labels, winning_kind
            ),
        ));
    }

    if drafts.is_empty() {
        artifacts.stage_metrics.push(stage_runtime_metric_artifact(
            run_id,
            run_id,
            &mode,
            "shadow_skipped",
            "No candidate drafts available for AutoReason shadow recording.",
        ));
    }

    Ok(artifacts)
}

fn normalize_auto_reason_mode(mode: &str) -> String {
    match mode.trim().to_ascii_lowercase().as_str() {
        "shadow" => "shadow".to_string(),
        "enabled" => "enabled".to_string(),
        _ => DEFAULT_AUTO_REASON_MODE.to_string(),
    }
}

fn sanitize_auto_reason_stages(stages: Vec<String>) -> Vec<String> {
    let mut cleaned = stages
        .into_iter()
        .map(|item| item.trim().to_ascii_lowercase())
        .filter(|item| item == AUTO_REASON_STAGE_CANDIDATE_REFINE)
        .collect::<Vec<_>>();
    cleaned.sort();
    cleaned.dedup();
    if cleaned.is_empty() {
        cleaned.push(AUTO_REASON_STAGE_CANDIDATE_REFINE.to_string());
    }
    cleaned
}

fn sanitize_settings(mut settings: WorkbenchSettings) -> WorkbenchSettings {
    settings.platform = if settings.platform.trim().is_empty() {
        "douyin".to_string()
    } else {
        settings.platform.trim().to_ascii_lowercase()
    };
    settings.watchlist_games = settings
        .watchlist_games
        .into_iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect();
    if settings.watchlist_games.is_empty() {
        settings.watchlist_games = default_watchlist_games();
    }
    settings.keyword_templates = settings
        .keyword_templates
        .into_iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect();
    if settings.keyword_templates.is_empty() {
        settings.keyword_templates = default_seed_keyword_templates();
    }
    settings.time_window_hours = settings.time_window_hours.clamp(6, 168);
    settings.schedule_interval_hours = settings.schedule_interval_hours.clamp(1, 24);
    settings.max_candidates_per_run = settings.max_candidates_per_run.clamp(10, 50);
    settings.doubao_model = if settings.doubao_model.trim().is_empty() {
        DEFAULT_DOUBAO_MODEL.to_string()
    } else {
        settings.doubao_model.trim().to_string()
    };
    settings.auto_reason_mode = normalize_auto_reason_mode(&settings.auto_reason_mode);
    settings.auto_reason_stages = sanitize_auto_reason_stages(settings.auto_reason_stages);
    settings.auto_reason_judge_model = if settings.auto_reason_judge_model.trim().is_empty() {
        DEFAULT_DOUBAO_MODEL.to_string()
    } else {
        settings.auto_reason_judge_model.trim().to_string()
    };
    settings.auto_reason_max_rounds = settings.auto_reason_max_rounds.clamp(1, 4);
    settings.auto_reason_timeout_ms = settings.auto_reason_timeout_ms.clamp(1_000, 60_000);
    settings.auto_reason_shadow_sample_rate =
        settings.auto_reason_shadow_sample_rate.clamp(0.0, 1.0);
    settings.auto_reason_min_confidence = settings.auto_reason_min_confidence.clamp(0.0, 1.0);
    settings.auto_reason_do_nothing_margin = settings.auto_reason_do_nothing_margin.clamp(0.0, 1.0);
    settings
}

impl WorkbenchService {
    async fn auto_reason_candidate_refine(
        &self,
        run_id: &str,
        settings: &WorkbenchSettings,
        drafts: Vec<CandidateDraft>,
    ) -> Result<AutoReasonOutcome> {
        let mode = normalize_auto_reason_mode(&settings.auto_reason_mode);
        if mode == DEFAULT_AUTO_REASON_MODE {
            return Ok(AutoReasonOutcome {
                drafts,
                artifacts: AutoReasonArtifacts::default(),
            });
        }
        Ok(AutoReasonOutcome {
            artifacts: build_shadow_auto_reason_artifacts(run_id, settings, &drafts)?,
            drafts,
        })
    }
}

#[allow(dead_code)]
fn local_query_plan(settings: &WorkbenchSettings) -> QueryPlan {
    let watchlist = if settings.watchlist_games.is_empty() {
        vec!["SLG".to_string()]
    } else {
        settings.watchlist_games.clone()
    };
    let mut queries = Vec::new();
    for game in watchlist.iter().take(3) {
        queries.push(format!("site:douyin.com/video {game} 版本 赛季"));
        queries.push(format!("site:douyin.com/video {game} 活动 联动"));
        queries.push(format!("site:douyin.com/video {game} 阵容 开荒 上分"));
    }
    queries.push("site:douyin.com/video SLG 热门 赛季".to_string());
    QueryPlan { queries }
}

fn seeded_local_query_plan(settings: &WorkbenchSettings) -> QueryPlan {
    let profiles = requested_seed_profiles(&settings.watchlist_games);
    let rotation = (OffsetDateTime::now_utc().unix_timestamp() as usize) % profiles.len().max(1);
    let rotated = profiles
        .iter()
        .cycle()
        .skip(rotation)
        .take(profiles.len())
        .copied()
        .collect::<Vec<_>>();
    let buckets = ["版本 赛季", "活动 联动", "阵容 开荒 上分", "同盟 联盟 国战"];
    let mut queries = vec!["site:douyin.com/video SLG 热门 赛季".to_string()];
    for (index, profile) in rotated.iter().take(7).enumerate() {
        let bucket = buckets[(rotation + index) % buckets.len()];
        let focus = profile.focus_terms[(rotation + index) % profile.focus_terms.len()];
        let alias = profile
            .aliases
            .get(index % profile.aliases.len().max(1))
            .copied()
            .unwrap_or(profile.canonical);
        queries.push(format!(
            "site:douyin.com/video {} {} {} {}",
            profile.canonical, alias, bucket, focus
        ));
    }
    QueryPlan { queries }
}

fn top_semantic_queries(discovery: &[(String, SearchHit)], limit: usize) -> Vec<String> {
    let mut counts = BTreeMap::<String, usize>::new();
    for (query, hit) in discovery.iter().take(8) {
        let key = query_word_seed(query, &hit.title);
        *counts.entry(key).or_default() += 1;
    }
    counts
        .into_iter()
        .rev()
        .map(|(key, _)| key)
        .take(limit)
        .collect()
}

fn query_word_seed(query: &str, title: &str) -> String {
    let mut words = split_csvish(query).collect::<Vec<_>>();
    words.extend(split_csvish(title));
    words.into_iter().take(4).collect::<Vec<_>>().join(" ")
}

fn looks_like_douyin(item: &SearchHit) -> bool {
    let haystack = format!(
        "{} {}",
        item.url.to_ascii_lowercase(),
        item.source_domain.to_ascii_lowercase()
    );
    ["douyin.com", "v.douyin.com", "iesdouyin.com", "douyin"]
        .iter()
        .any(|needle| haystack.contains(needle))
}

fn relevance_haystack(hit: &SearchHit) -> String {
    format!(
        "{} {} {} {} {}",
        hit.title.to_lowercase(),
        hit.snippet.to_lowercase(),
        hit.tags.join(" ").to_lowercase(),
        hit.author.to_lowercase(),
        hit.source_domain.to_lowercase()
    )
}

fn matches_watch_terms(haystack: &str, watchlist: &[String]) -> bool {
    watchlist
        .iter()
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .any(|item| haystack.contains(item))
}

fn looks_slg_relevant(hit: &SearchHit, watchlist: &[String]) -> bool {
    let haystack = relevance_haystack(hit);
    let generic_terms = [
        "slg", "赛季", "开荒", "联盟", "同盟", "配将", "阵容", "国战", "活动", "联动", "版本",
        "攻略", "配队", "养成", "抽卡", "战报", "热点",
    ];

    generic_terms.iter().any(|term| haystack.contains(term))
        || matches_watch_terms(&haystack, watchlist)
}

fn normalized_identity(url: &str, title: &str) -> String {
    format!(
        "{}::{}",
        url.split('?')
            .next()
            .unwrap_or(url)
            .trim()
            .to_ascii_lowercase(),
        title.trim().to_ascii_lowercase()
    )
}

fn within_time_window(value: Option<&str>, hours: i64, now: OffsetDateTime) -> bool {
    let Some(value) = value else {
        return true;
    };
    let parsed = OffsetDateTime::parse(value, &Rfc3339)
        .or_else(|_| OffsetDateTime::parse(&normalize_external_timestamp(value), &Rfc3339));
    match parsed {
        Ok(timestamp) => now - timestamp <= TimeDuration::hours(hours.max(1)),
        Err(_) => true,
    }
}

fn freshness_score(published_at: &str, hours: i64) -> f64 {
    let Ok(timestamp) = OffsetDateTime::parse(published_at, &Rfc3339) else {
        return 0.5;
    };
    let delta_hours = (OffsetDateTime::now_utc() - timestamp).whole_hours().max(0) as f64;
    (1.0 - delta_hours / hours.max(1) as f64).clamp(0.0, 1.0)
}

fn merge_tags(target: &mut Vec<String>, additions: Vec<String>) {
    let mut seen = target
        .iter()
        .map(|value| value.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    for item in additions {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        if seen.insert(trimmed.to_ascii_lowercase()) {
            target.push(trimmed.to_string());
        }
    }
}

fn split_csvish(value: &str) -> impl Iterator<Item = String> + '_ {
    value
        .split(|char: char| [',', '，', '|', '/', '\n'].contains(&char))
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
}

fn normalize_external_timestamp(value: &str) -> String {
    OffsetDateTime::parse(value, &Rfc3339)
        .map(|timestamp| timestamp.format(&Rfc3339).unwrap_or_else(|_| utc_now()))
        .unwrap_or_else(|_| utc_now())
}

fn csv_escape(value: &str) -> String {
    let escaped = value.replace('"', "\"\"");
    format!("\"{escaped}\"")
}

fn join_url(base: &str, path: &str) -> String {
    format!("{}{}", base.trim_end_matches('/'), path)
}

fn domain_for_url(url: &str) -> String {
    url.split('/')
        .nth(2)
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn extract_json_value(content: &str) -> Result<Value> {
    let trimmed = content.trim();
    let mut candidates = Vec::new();
    candidates.push(trimmed.to_string());
    if let Some(fenced) = extract_fenced_block(trimmed) {
        candidates.push(fenced.to_string());
    }
    candidates.extend(balanced_json_candidates(trimmed));
    if let Some(fenced) = extract_fenced_block(trimmed) {
        candidates.extend(balanced_json_candidates(fenced));
    }

    let mut seen = BTreeSet::new();
    let mut last_error = None;
    for candidate in candidates {
        let normalized = candidate.trim();
        if normalized.is_empty() || !seen.insert(normalized.to_string()) {
            continue;
        }
        match serde_json::from_str::<Value>(normalized) {
            Ok(value) => return Ok(value),
            Err(error) => last_error = Some(error),
        }
    }

    if let Some(error) = last_error {
        return Err(error).context("failed to parse json object from model output");
    }
    bail!("model output did not contain valid json")
}

fn extract_fenced_block(content: &str) -> Option<&str> {
    let start = content.find("```")?;
    let after_start = &content[start + 3..];
    let line_break = after_start.find('\n')?;
    let body = &after_start[line_break + 1..];
    let end = body.rfind("```")?;
    Some(body[..end].trim())
}

fn balanced_json_candidates(content: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    for (index, ch) in content.char_indices() {
        if ch != '{' && ch != '[' {
            continue;
        }
        if let Some(len) = balanced_json_len(&content[index..]) {
            candidates.push(content[index..index + len].trim().to_string());
        }
    }
    candidates
}

fn balanced_json_len(content: &str) -> Option<usize> {
    let mut stack = Vec::new();
    let mut in_string = false;
    let mut escape = false;

    for (index, ch) in content.char_indices() {
        if in_string {
            if escape {
                escape = false;
                continue;
            }
            match ch {
                '\\' => escape = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }

        match ch {
            '"' => in_string = true,
            '{' | '[' => stack.push(ch),
            '}' => {
                if stack.pop() != Some('{') {
                    return None;
                }
                if stack.is_empty() {
                    return Some(index + ch.len_utf8());
                }
            }
            ']' => {
                if stack.pop() != Some('[') {
                    return None;
                }
                if stack.is_empty() {
                    return Some(index + ch.len_utf8());
                }
            }
            _ => {}
        }
    }

    None
}

fn utc_now() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .expect("utc timestamp")
}

fn short_hash(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let digest = hasher.finalize();
    digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn from_json_text<T: for<'de> Deserialize<'de>>(value: String) -> Result<T> {
    serde_json::from_str(&value).context("failed to decode json text")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ProviderUpdateRequest;
    use crate::secret_store::MemorySecretStore;
    use tempfile::tempdir;

    #[derive(Clone)]
    struct MockGateway {
        invalid_json: bool,
    }

    #[async_trait]
    impl ProviderGateway for MockGateway {
        async fn plan_queries(
            &self,
            _config: &ProviderRuntimeConfig,
            _settings: &WorkbenchSettings,
            _history: &[String],
        ) -> Result<QueryPlan> {
            Ok(QueryPlan {
                queries: vec![
                    "site:douyin.com/video 涓囬緳瑙夐啋 璧涘".to_string(),
                    "site:douyin.com/video 鐜囧湡涔嬫花 娲诲姩".to_string(),
                ],
            })
        }

        async fn serper_search(
            &self,
            _config: &ProviderRuntimeConfig,
            query: &str,
        ) -> Result<Vec<SearchHit>> {
            Ok(vec![SearchHit {
                title: format!("{query} 热门视频"),
                url: format!("https://www.douyin.com/video/{}", short_hash(query)),
                snippet: "SLG 赛季开荒阵容与同盟热点".to_string(),
                source_domain: "www.douyin.com".to_string(),
                author: "测试作者".to_string(),
                published_at: Some(utc_now()),
                tags: vec!["slg".to_string(), "douyin".to_string()],
                engagement_hint: 0.82,
            }])
        }

        async fn exa_expand(
            &self,
            _config: &ProviderRuntimeConfig,
            query: &str,
        ) -> Result<Vec<SearchHit>> {
            Ok(vec![SearchHit {
                title: format!("{query} 长尾扩展"),
                url: format!("https://www.douyin.com/video/exa-{}", short_hash(query)),
                snippet: "同盟对抗与版本活动延展".to_string(),
                source_domain: "www.douyin.com".to_string(),
                author: "Exa 作者".to_string(),
                published_at: Some(utc_now()),
                tags: vec!["exa".to_string()],
                engagement_hint: 0.58,
            }])
        }

        async fn tavily_verify(
            &self,
            _config: &ProviderRuntimeConfig,
            title: &str,
            _url: &str,
        ) -> Result<VerificationPayload> {
            Ok(VerificationPayload {
                score: 0.66,
                note: format!("{title} 已通过热点验证"),
            })
        }

        async fn firecrawl_scrape(
            &self,
            _config: &ProviderRuntimeConfig,
            url: &str,
        ) -> Result<ScrapePayload> {
            Ok(ScrapePayload {
                text: format!("抓取到的正文：{url} 对应一条 SLG 阵容拆解内容"),
                description: "抓取到的描述".to_string(),
                tags: vec!["抓取正文".to_string()],
            })
        }

        async fn doubao_analyze(
            &self,
            _config: &ProviderRuntimeConfig,
            _settings: &WorkbenchSettings,
            candidate: &CandidateDraft,
        ) -> Result<StructuredBreakdown> {
            if self.invalid_json {
                bail!("invalid json from doubao");
            }
            Ok(StructuredBreakdown {
                game_id: Some("game_stzb".to_string()),
                game_name: Some("率土之滨".to_string()),
                event_type: Some("season".to_string()),
                topic_tags: vec!["赛季".to_string(), "开荒".to_string()],
                content_summary: format!("围绕 {} 的结构化总结", candidate.title),
                hook_points: vec!["首屏冲突".to_string(), "阵容悬念".to_string()],
                core_conflict_or_value: "高战与开荒效率冲突".to_string(),
                audience_fit: "SLG 中重度玩家".to_string(),
                adaptation_angles: vec!["阵容拆解".to_string(), "同盟视角".to_string()],
                title_directions: vec!["赛季开荒三分钟讲透".to_string()],
                doubao_relevance_score: 0.87,
                topic_pool_reason: "赛季节点强，适合快速改写成选题".to_string(),
            })
        }
    }

    fn build_test_service(invalid_json: bool) -> Result<WorkbenchService> {
        let temp = tempdir().expect("temp dir");
        let root = temp.path().to_path_buf();
        std::mem::forget(temp);
        let provider_store = ProviderStore::new(
            root.join("providers.db"),
            Arc::new(MemorySecretStore::new()),
        );
        provider_store.initialize()?;
        for provider in ProviderKind::REQUIRED {
            provider_store.save_provider(
                provider,
                ProviderUpdateRequest {
                    enabled: true,
                    api_key: Some(format!("{}-key", provider.slug())),
                    clear_api_key: false,
                    base_url: provider.default_base_url().to_string(),
                    monthly_limit: provider.monthly_limit(),
                    daily_soft_limit: provider.daily_soft_limit(),
                    default_role: if provider == ProviderKind::Doubao {
                        "ep-test-doubao".to_string()
                    } else {
                        provider.default_role().to_string()
                    },
                    priority: provider.priority(),
                },
            )?;
            provider_store.test_provider(provider)?;
        }
        provider_store.confirm_local_user()?;
        provider_store.advance_api_gate()?;

        let service = WorkbenchService::new(
            root.join("core.db"),
            provider_store,
            Arc::new(MockGateway { invalid_json }),
        )?;
        service.update_settings(WorkbenchSettings {
            platform: "douyin".to_string(),
            watchlist_games: default_watchlist_games(),
            keyword_templates: default_seed_keyword_templates(),
            time_window_hours: 24,
            schedule_interval_hours: 2,
            max_candidates_per_run: 12,
            doubao_model: "ep-test-doubao".to_string(),
            auto_reason_mode: DEFAULT_AUTO_REASON_MODE.to_string(),
            auto_reason_stages: vec![AUTO_REASON_STAGE_CANDIDATE_REFINE.to_string()],
            auto_reason_judge_model: "ep-test-doubao".to_string(),
            auto_reason_max_rounds: 2,
            auto_reason_timeout_ms: 12_000,
            auto_reason_shadow_sample_rate: 0.2,
            auto_reason_min_confidence: 0.6,
            auto_reason_do_nothing_margin: 0.05,
        })?;
        Ok(service)
    }

    #[tokio::test]
    async fn run_now_offline_golden_path_persists_candidate_rankings_topic_pool_and_overview() {
        let service = build_test_service(false).expect("service");
        let gate_state = service.provider_store.gate_state().expect("gate state");
        assert!(gate_state.local_user_confirmed);
        assert!(gate_state.api_gate_passed);

        let unlock_state = service.provider_store.unlock_state().expect("unlock state");
        assert!(unlock_state.unlocked);
        assert_eq!(unlock_state.ready_count, unlock_state.total_count);

        let run = service.run_now().await.expect("run");
        assert_eq!(run.status, "succeeded");
        assert!(run.candidate_count > 0);
        assert!(run.shortlisted_count > 0);
        assert!(run.content_count > 0);

        let overview = service.overview().expect("overview");
        let latest_run = overview.latest_run.as_ref().expect("latest run");
        assert_eq!(latest_run.run_id, run.run_id);
        assert_eq!(latest_run.status, "succeeded");
        assert_eq!(latest_run.candidate_count, run.candidate_count);
        assert_eq!(latest_run.shortlisted_count, run.shortlisted_count);
        assert_eq!(latest_run.content_count, run.content_count);
        assert_eq!(overview.content_rankings.len() as i64, run.content_count);
        assert!(!overview.game_rankings.is_empty());
        assert_eq!(overview.topic_pool_count, 0);

        let top_ranking = &overview.content_rankings[0];
        assert_eq!(top_ranking.rank, 1);
        let source_url = top_ranking.source_url.as_ref().expect("ranking source url");
        assert!(source_url.contains("douyin.com/video/"));
        assert!(top_ranking.trust.is_some());
        let content_id = top_ranking.content_id.clone().expect("content id");
        let content = service
            .content(&content_id)
            .expect("content")
            .expect("existing content");
        assert_eq!(content.candidate.content_id, content_id);
        assert!(!content.candidate.trust.captured_at.is_empty());
        assert!(!content.breakdown.content_summary.is_empty());
        assert!(!content.breakdown.topic_pool_reason.is_empty());

        let topic_entry = service
            .add_topic_pool(TopicPoolCreateRequest {
                content_id: content_id.clone(),
                note: "Governed golden-path entry".to_string(),
            })
            .expect("add topic pool");
        assert_eq!(topic_entry.content_id, content_id);
        assert_eq!(topic_entry.source_url, *source_url);
        assert!(topic_entry.note.contains("golden-path"));

        let topic_pool = service.topic_pool().expect("topic pool");
        let topic_pool_entry = topic_pool
            .iter()
            .find(|item| item.content_id == content_id)
            .expect("topic pool entry for ranked content");
        assert_eq!(topic_pool_entry.source_url, *source_url);
        assert!(topic_pool_entry.score > 0.0);

        let overview_after_topic_pool = service.overview().expect("overview after topic pool");
        assert_eq!(overview_after_topic_pool.topic_pool_count, topic_pool.len());

        let conn = service.connect().expect("connect");
        let persisted_content_rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM content_candidates WHERE run_id = ?1",
                params![&run.run_id],
                |row| row.get(0),
            )
            .expect("persisted content rows");
        let persisted_content_rankings: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM ranking_snapshots WHERE run_id = ?1 AND kind = 'content' AND is_current = 1",
                params![&run.run_id],
                |row| row.get(0),
            )
            .expect("persisted content rankings");
        let persisted_topic_pool_rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM topic_pool", [], |row| row.get(0))
            .expect("persisted topic pool rows");

        assert_eq!(persisted_content_rows, run.content_count);
        assert_eq!(persisted_content_rankings, run.content_count);
        assert_eq!(
            persisted_topic_pool_rows as usize,
            overview_after_topic_pool.topic_pool_count
        );
        assert!(content.breakdown.content_summary.contains("结构化总结"));
    }

    #[tokio::test]
    async fn shadow_mode_records_auto_reason_artifacts_without_changing_outputs() {
        let service = build_test_service(false).expect("service");
        let mut settings = service.settings().expect("settings");
        settings.auto_reason_mode = "shadow".to_string();
        service
            .update_settings(settings)
            .expect("update settings with shadow");

        let run = service.run_now().await.expect("shadow run");
        assert_eq!(run.status, "succeeded");

        let conn = service.connect().expect("connect");
        let candidate_versions: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM candidate_versions WHERE run_id = ?1",
                params![&run.run_id],
                |row| row.get(0),
            )
            .expect("candidate version count");
        let judgements: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM reasoning_judgements WHERE run_id = ?1",
                params![&run.run_id],
                |row| row.get(0),
            )
            .expect("judgement count");
        let metrics: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM stage_runtime_metrics WHERE run_id = ?1 AND stage = ?2",
                params![&run.run_id, AUTO_REASON_STAGE_CANDIDATE_REFINE],
                |row| row.get(0),
            )
            .expect("metric count");
        let content_rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM content_candidates WHERE run_id = ?1",
                params![&run.run_id],
                |row| row.get(0),
            )
            .expect("content count");
        let distinct_variants: i64 = conn
            .query_row(
                "SELECT COUNT(DISTINCT variant_kind) FROM candidate_versions WHERE run_id = ?1",
                params![&run.run_id],
                |row| row.get(0),
            )
            .expect("distinct variants");

        assert!(candidate_versions > 0);
        assert!(candidate_versions >= judgements * 3);
        assert!(judgements > 0);
        assert!(metrics > 0);
        assert!(content_rows > 0);
        assert!(distinct_variants >= 3);
    }

    #[test]
    fn extract_json_value_accepts_markdown_code_fence() {
        let content = "Here is the result:\n```json\n{\"queries\":[\"a\",\"b\"]}\n```";
        let value = extract_json_value(content).expect("extract fenced json");
        assert_eq!(value["queries"].as_array().expect("queries array").len(), 2);
    }

    #[test]
    fn extract_json_value_accepts_surrounding_text() {
        let content =
            "analysis first\n{\"game_name\":\"率土之滨\",\"topic_tags\":[\"赛季\"]}\nthanks";
        let value = extract_json_value(content).expect("extract surrounded json");
        assert_eq!(value["game_name"], "率土之滨");
    }

    #[test]
    fn extract_json_value_skips_non_json_braces_before_payload() {
        let content =
            "note: use {placeholder} only for explanation\n{\"topic_pool_reason\":\"valid payload\"}";
        let value = extract_json_value(content).expect("extract trailing payload");
        assert_eq!(value["topic_pool_reason"], "valid payload");
    }

    #[tokio::test]
    async fn failed_doubao_run_keeps_previous_snapshot() {
        let service = build_test_service(false).expect("service");
        let first = service.run_now().await.expect("first run");
        assert_eq!(first.status, "succeeded");
        let before = service.overview().expect("overview before");

        let failing = WorkbenchService::new(
            service.db_path.clone(),
            service.provider_store.clone(),
            Arc::new(MockGateway { invalid_json: true }),
        )
        .expect("failing service");
        failing
            .update_settings(service.settings().expect("settings"))
            .expect("sync settings");

        let second = failing.run_now().await.expect("second run");
        assert_eq!(second.status, "failed");
        let after = failing.overview().expect("overview after");
        assert_eq!(before.content_rankings.len(), after.content_rankings.len());
        assert_eq!(
            before.content_rankings[0].title,
            after.content_rankings[0].title
        );
    }

    #[tokio::test]
    async fn repeated_successful_run_upserts_existing_content_rows() {
        let service = build_test_service(false).expect("service");
        let first = service.run_now().await.expect("first run");
        assert_eq!(first.status, "succeeded");
        let second = service.run_now().await.expect("second run");
        assert_eq!(second.status, "succeeded");
        let runs = service.list_runs().expect("runs");
        assert!(runs.len() >= 2);
    }

    #[tokio::test]
    async fn workbench_budget_overview_only_lists_required_providers() {
        let service = build_test_service(false).expect("service");
        let overview = service.overview().expect("overview");
        assert_eq!(overview.budgets.len(), ProviderKind::ALL.len());
        let providers = overview
            .budgets
            .iter()
            .map(|item| item.provider.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            providers,
            BTreeSet::from(["Doubao", "Serper", "Firecrawl", "Tavily", "Exa"])
        );

        let run = service.run_now().await.expect("run");
        assert_eq!(run.status, "succeeded");
    }

    #[test]
    fn failed_run_waits_for_schedule_interval_before_retry() {
        let service = build_test_service(false).expect("service");
        let settings = service.settings().expect("settings");
        let started_at = utc_now();
        service
            .insert_run("failed_run", "schedule", "running", &started_at)
            .expect("insert run");
        service
            .finish_run_failure("failed_run", &started_at, "boom")
            .expect("finish failure");
        assert!(!service.should_run(&settings).expect("should run"));
    }

    #[test]
    fn serper_budget_keeps_minimum_discovery_floor() {
        let config = ProviderRuntimeConfig {
            provider: ProviderKind::Serper,
            enabled: true,
            api_key: Some("serper-key".to_string()),
            base_url: ProviderKind::Serper.default_base_url().to_string(),
            monthly_limit: 2_500,
            daily_soft_limit: 60,
            default_role: ProviderKind::Serper.default_role().to_string(),
            priority: 2,
        };
        let meter = BudgetMeter {
            provider: ProviderKind::Serper,
            enabled: true,
            policy: budget_policy(ProviderKind::Serper, &config),
            used_monthly: 1_700,
            used_today: 59,
        };
        assert!(meter.allowed_this_run() >= 1);
    }

    #[test]
    fn empty_settings_are_seeded_with_default_watchlist() {
        let settings = sanitize_settings(WorkbenchSettings {
            platform: String::new(),
            watchlist_games: Vec::new(),
            keyword_templates: Vec::new(),
            time_window_hours: 1,
            schedule_interval_hours: 99,
            max_candidates_per_run: 2,
            doubao_model: "  ".to_string(),
            auto_reason_mode: " ??? ".to_string(),
            auto_reason_stages: vec!["candidate_refine".to_string(), "other".to_string()],
            auto_reason_judge_model: " ".to_string(),
            auto_reason_max_rounds: 99,
            auto_reason_timeout_ms: 99,
            auto_reason_shadow_sample_rate: 2.0,
            auto_reason_min_confidence: -1.0,
            auto_reason_do_nothing_margin: 2.0,
        });
        assert_eq!(settings.platform, "douyin");
        assert_eq!(settings.watchlist_games, default_watchlist_games());
        assert_eq!(settings.keyword_templates, default_seed_keyword_templates());
        assert_eq!(settings.time_window_hours, 6);
        assert_eq!(settings.schedule_interval_hours, 24);
        assert_eq!(settings.max_candidates_per_run, 10);
        assert_eq!(settings.doubao_model, DEFAULT_DOUBAO_MODEL);
        assert_eq!(settings.auto_reason_mode, DEFAULT_AUTO_REASON_MODE);
        assert_eq!(
            settings.auto_reason_stages,
            vec![AUTO_REASON_STAGE_CANDIDATE_REFINE.to_string()]
        );
        assert_eq!(settings.auto_reason_judge_model, DEFAULT_DOUBAO_MODEL);
        assert_eq!(settings.auto_reason_max_rounds, 4);
        assert_eq!(settings.auto_reason_timeout_ms, 1_000);
        assert_eq!(settings.auto_reason_shadow_sample_rate, 1.0);
        assert_eq!(settings.auto_reason_min_confidence, 0.0);
        assert_eq!(settings.auto_reason_do_nothing_margin, 1.0);
    }

    #[test]
    fn shortlist_candidates_respects_configured_cap_above_fifteen() {
        let settings = WorkbenchSettings {
            platform: "douyin".to_string(),
            watchlist_games: vec!["率土之滨".to_string()],
            keyword_templates: default_seed_keyword_templates(),
            time_window_hours: 24,
            schedule_interval_hours: 2,
            max_candidates_per_run: 20,
            doubao_model: DEFAULT_DOUBAO_MODEL.to_string(),
            auto_reason_mode: DEFAULT_AUTO_REASON_MODE.to_string(),
            auto_reason_stages: vec![AUTO_REASON_STAGE_CANDIDATE_REFINE.to_string()],
            auto_reason_judge_model: DEFAULT_DOUBAO_MODEL.to_string(),
            auto_reason_max_rounds: 2,
            auto_reason_timeout_ms: 12_000,
            auto_reason_shadow_sample_rate: 0.2,
            auto_reason_min_confidence: 0.6,
            auto_reason_do_nothing_margin: 0.05,
        };
        let discovery = (0..24)
            .map(|index| {
                (
                    "site:douyin.com/video 率土之滨 赛季".to_string(),
                    SearchHit {
                        title: format!("率土之滨 赛季热点 {index}"),
                        url: format!("https://www.douyin.com/video/candidate-{index}"),
                        snippet: "率土之滨 赛季 开荒 同盟".to_string(),
                        source_domain: "www.douyin.com".to_string(),
                        author: "测试作者".to_string(),
                        published_at: Some(utc_now()),
                        tags: vec!["slg".to_string()],
                        engagement_hint: 0.8,
                    },
                )
            })
            .collect::<Vec<_>>();

        let shortlist = shortlist_candidates(discovery, &settings);
        assert_eq!(shortlist.len(), 20);
    }

    #[test]
    fn fallback_relevant_hits_keep_non_douyin_results_when_relevant() {
        let settings = WorkbenchSettings {
            platform: "douyin".to_string(),
            watchlist_games: vec!["率土之滨".to_string()],
            keyword_templates: default_seed_keyword_templates(),
            time_window_hours: 24,
            schedule_interval_hours: 2,
            max_candidates_per_run: 20,
            doubao_model: DEFAULT_DOUBAO_MODEL.to_string(),
            auto_reason_mode: DEFAULT_AUTO_REASON_MODE.to_string(),
            auto_reason_stages: vec![AUTO_REASON_STAGE_CANDIDATE_REFINE.to_string()],
            auto_reason_judge_model: DEFAULT_DOUBAO_MODEL.to_string(),
            auto_reason_max_rounds: 2,
            auto_reason_timeout_ms: 12_000,
            auto_reason_shadow_sample_rate: 0.2,
            auto_reason_min_confidence: 0.6,
            auto_reason_do_nothing_margin: 0.05,
        };

        let discovery = vec![(
            "率土之滨 S32赛季开荒配将 24小时热点".to_string(),
            SearchHit {
                title: "率土之滨 S32赛季开荒阵容汇总".to_string(),
                url: "https://www.gamersky.com/news/202604/123456.shtml".to_string(),
                snippet: "率土之滨 S32 赛季开荒配将与阵容热点盘点".to_string(),
                source_domain: "www.gamersky.com".to_string(),
                author: "游民星空".to_string(),
                published_at: Some(utc_now()),
                tags: vec!["serper".to_string()],
                engagement_hint: 0.72,
            },
        )];

        let hits = fallback_relevant_hits(discovery, &settings);
        assert_eq!(hits.len(), 1);
        assert!(hits[0]
            .1
            .tags
            .iter()
            .any(|tag| tag == "fallback_non_douyin"));
    }

    #[test]
    fn stale_running_run_is_recovered_on_service_init() {
        let service = build_test_service(false).expect("service");
        let started_at = (OffsetDateTime::now_utc()
            - TimeDuration::minutes(STALE_RUN_TIMEOUT_MINUTES + 5))
        .format(&Rfc3339)
        .expect("timestamp");
        service
            .insert_run("stale_run", "schedule", "running", &started_at)
            .expect("insert stale run");

        let reopened = WorkbenchService::new(
            service.db_path.clone(),
            service.provider_store.clone(),
            Arc::new(MockGateway {
                invalid_json: false,
            }),
        )
        .expect("reopen service");

        let latest = reopened
            .latest_run()
            .expect("latest run")
            .expect("stale run exists");
        assert_eq!(latest.run_id, "stale_run");
        assert_eq!(latest.status, "failed");
        assert_eq!(
            latest.error_message.as_deref(),
            Some(STALE_RUN_RECOVERY_ERROR)
        );
        assert!(reopened
            .should_run(&reopened.settings().expect("settings"))
            .expect("should run"));
    }

    #[test]
    fn seeded_local_query_plan_covers_seed_watchlist() {
        let settings = WorkbenchSettings {
            platform: "douyin".to_string(),
            watchlist_games: default_watchlist_games(),
            keyword_templates: default_seed_keyword_templates(),
            time_window_hours: 24,
            schedule_interval_hours: 2,
            max_candidates_per_run: 12,
            doubao_model: "ep-test-doubao".to_string(),
            auto_reason_mode: DEFAULT_AUTO_REASON_MODE.to_string(),
            auto_reason_stages: vec![AUTO_REASON_STAGE_CANDIDATE_REFINE.to_string()],
            auto_reason_judge_model: "ep-test-doubao".to_string(),
            auto_reason_max_rounds: 2,
            auto_reason_timeout_ms: 12_000,
            auto_reason_shadow_sample_rate: 0.2,
            auto_reason_min_confidence: 0.6,
            auto_reason_do_nothing_margin: 0.05,
        };
        let plan = seeded_local_query_plan(&settings);
        assert_eq!(plan.queries.len(), 8);
        assert!(plan.queries[0].contains("SLG"));
        for game in settings.watchlist_games {
            assert!(plan.queries.iter().any(|query| query.contains(&game)));
        }
    }

    #[test]
    fn seed_watch_terms_include_aliases_for_selected_games() {
        let terms = seed_watch_terms(&vec!["三国谋定天下".to_string(), "率土之滨".to_string()]);
        assert!(terms.iter().any(|term| term == "三谋"));
        assert!(terms.iter().any(|term| term == "率土"));
    }

    #[test]
    fn looks_slg_relevant_matches_seed_aliases() {
        let hit = SearchHit {
            title: "三谋赛季开荒阵容推荐".to_string(),
            url: "https://www.douyin.com/video/alias-hit".to_string(),
            snippet: "围绕配将和同盟节奏展开".to_string(),
            source_domain: "www.douyin.com".to_string(),
            author: "测试作者".to_string(),
            published_at: Some(utc_now()),
            tags: vec!["slg".to_string()],
            engagement_hint: 0.7,
        };
        let watch_terms = seed_watch_terms(&vec!["三国谋定天下".to_string()]);
        assert!(looks_slg_relevant(&hit, &watch_terms));
    }
}
