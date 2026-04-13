use crate::models::{
    GateStateView, LocalUserView, ProviderKind, ProviderStatusView, ProviderTestResponse,
    ProviderUpdateRequest, QuotaSnapshot, UnlockStateView,
};
use crate::secret_store::SecretStore;
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

#[derive(Clone)]
pub struct ProviderStore {
    db_path: PathBuf,
    secret_store: Arc<dyn SecretStore>,
}

#[derive(Debug, Clone)]
pub struct ProviderRuntimeConfig {
    pub provider: ProviderKind,
    pub enabled: bool,
    pub api_key: Option<String>,
    pub base_url: String,
    pub monthly_limit: i64,
    pub daily_soft_limit: i64,
    pub default_role: String,
    pub priority: i64,
}

impl ProviderStore {
    pub fn new(db_path: PathBuf, secret_store: Arc<dyn SecretStore>) -> Self {
        Self {
            db_path,
            secret_store,
        }
    }

    pub fn initialize(&self) -> Result<()> {
        if let Some(parent) = self.db_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let conn = self.connect()?;
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS provider_settings (
                provider TEXT PRIMARY KEY,
                enabled INTEGER NOT NULL,
                base_url TEXT NOT NULL,
                monthly_limit INTEGER NOT NULL,
                daily_soft_limit INTEGER NOT NULL,
                default_role TEXT NOT NULL,
                priority INTEGER NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS provider_test_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                provider TEXT NOT NULL,
                ok INTEGER NOT NULL,
                status TEXT NOT NULL,
                latency_ms INTEGER NOT NULL,
                error_message TEXT,
                verified_at TEXT NOT NULL,
                detail_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS provider_quota_ledger (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                provider TEXT NOT NULL,
                used_monthly INTEGER NOT NULL,
                remaining_monthly INTEGER NOT NULL,
                daily_soft_limit INTEGER NOT NULL,
                monthly_limit INTEGER NOT NULL,
                captured_at TEXT NOT NULL,
                snapshot_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS app_gate_state (
                id INTEGER PRIMARY KEY CHECK(id = 1),
                local_user_confirmed INTEGER NOT NULL DEFAULT 0,
                confirmed_username TEXT,
                confirmed_device_name TEXT,
                api_gate_passed INTEGER NOT NULL DEFAULT 0,
                last_logout_at TEXT
            );
            ",
        )?;

        for provider in ProviderKind::ALL {
            conn.execute(
                "
                INSERT OR IGNORE INTO provider_settings(
                    provider, enabled, base_url, monthly_limit, daily_soft_limit,
                    default_role, priority, updated_at
                ) VALUES (?1, 1, ?2, ?3, ?4, ?5, ?6, ?7)
                ",
                params![
                    provider.slug(),
                    provider.default_base_url(),
                    provider.monthly_limit(),
                    provider.daily_soft_limit(),
                    provider.default_role(),
                    provider.priority(),
                    utc_now(),
                ],
            )?;
        }
        conn.execute(
            "
            INSERT OR IGNORE INTO app_gate_state(
                id, local_user_confirmed, confirmed_username, confirmed_device_name, api_gate_passed, last_logout_at
            ) VALUES (1, 0, NULL, NULL, 0, NULL)
            ",
            [],
        )?;
        Ok(())
    }

    pub fn list_provider_views(&self) -> Result<Vec<ProviderStatusView>> {
        let conn = self.connect()?;
        let mut views = Vec::new();
        for provider in ProviderKind::ALL {
            views.push(self.build_provider_view(&conn, provider)?);
        }
        Ok(views)
    }

    pub fn save_provider(
        &self,
        provider: ProviderKind,
        request: ProviderUpdateRequest,
    ) -> Result<ProviderStatusView> {
        let conn = self.connect()?;
        let previous_settings = self
            .raw_settings(&conn, provider)?
            .unwrap_or_else(|| default_settings(provider));
        let previous_secret = self.secret_store.get_secret(&secret_key(provider))?;
        let normalized_base_url = request.base_url.trim().to_string();
        let normalized_default_role = request.default_role.trim().to_string();

        conn.execute(
            "
            INSERT INTO provider_settings(
                provider, enabled, base_url, monthly_limit, daily_soft_limit,
                default_role, priority, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(provider) DO UPDATE SET
                enabled = excluded.enabled,
                base_url = excluded.base_url,
                monthly_limit = excluded.monthly_limit,
                daily_soft_limit = excluded.daily_soft_limit,
                default_role = excluded.default_role,
                priority = excluded.priority,
                updated_at = excluded.updated_at
            ",
            params![
                provider.slug(),
                bool_to_int(request.enabled),
                &normalized_base_url,
                request.monthly_limit,
                request.daily_soft_limit,
                &normalized_default_role,
                request.priority,
                utc_now(),
            ],
        )?;

        let mut secret_changed = false;
        if request.clear_api_key {
            if previous_secret
                .as_deref()
                .is_some_and(|secret| !secret.trim().is_empty())
            {
                secret_changed = true;
            }
            self.secret_store.delete_secret(&secret_key(provider))?;
        } else if let Some(api_key) = request.api_key {
            if api_key.trim().is_empty() {
                if previous_secret
                    .as_deref()
                    .is_some_and(|secret| !secret.trim().is_empty())
                {
                    secret_changed = true;
                }
                self.secret_store.delete_secret(&secret_key(provider))?;
            } else {
                let trimmed = api_key.trim();
                secret_changed = previous_secret
                    .as_deref()
                    .map(|secret| secret.trim() != trimmed)
                    .unwrap_or(true);
                self.secret_store
                    .set_secret(&secret_key(provider), trimmed)?;
            }
        }

        let settings_changed = previous_settings.enabled != request.enabled
            || previous_settings.base_url != normalized_base_url
            || previous_settings.monthly_limit != request.monthly_limit
            || previous_settings.daily_soft_limit != request.daily_soft_limit
            || previous_settings.default_role != normalized_default_role
            || previous_settings.priority != request.priority;

        if settings_changed || secret_changed {
            invalidate_provider_verification(&conn, provider)?;
            if provider.is_required() {
                invalidate_api_gate(&conn)?;
            }
        }

        self.build_provider_view(&conn, provider)
    }

    pub fn test_provider(&self, provider: ProviderKind) -> Result<ProviderTestResponse> {
        let conn = self.connect()?;
        let settings = self
            .raw_settings(&conn, provider)?
            .unwrap_or_else(|| default_settings(provider));
        let secret = self.secret_store.get_secret(&secret_key(provider))?;
        let response = simulate_test(provider, &settings, secret.as_deref());

        conn.execute(
            "
            INSERT INTO provider_test_logs(
                provider, ok, status, latency_ms, error_message, verified_at, detail_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ",
            params![
                provider.slug(),
                bool_to_int(response.ok),
                &response.status,
                response.latency_ms,
                response.error_message.as_deref(),
                &response.verified_at,
                serde_json::to_string(&response)?,
            ],
        )?;

        conn.execute(
            "
            INSERT INTO provider_quota_ledger(
                provider, used_monthly, remaining_monthly, daily_soft_limit,
                monthly_limit, captured_at, snapshot_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ",
            params![
                provider.slug(),
                response.quota_snapshot.used_monthly,
                response.quota_snapshot.remaining_monthly,
                response.quota_snapshot.daily_soft_limit,
                response.quota_snapshot.monthly_limit,
                &response.verified_at,
                serde_json::to_string(&response.quota_snapshot)?,
            ],
        )?;

        if provider.is_required() && response.status != "ready" {
            invalidate_api_gate(&conn)?;
        }

        Ok(response)
    }

    pub fn test_all(&self) -> Result<Vec<ProviderTestResponse>> {
        ProviderKind::ALL
            .into_iter()
            .map(|provider| self.test_provider(provider))
            .collect()
    }

    pub fn unlock_state(&self) -> Result<UnlockStateView> {
        let views = self.list_provider_views()?;
        Ok(compute_unlock_state_v2(&views))
    }

    pub fn local_user(&self) -> LocalUserView {
        let username = std::env::var("USERNAME").unwrap_or_else(|_| "unknown-user".to_string());
        let device_name =
            std::env::var("COMPUTERNAME").unwrap_or_else(|_| "unknown-device".to_string());
        LocalUserView {
            display_name: format!("{username} @ {device_name}"),
            username,
            device_name,
        }
    }

    pub fn gate_state(&self) -> Result<GateStateView> {
        let conn = self.connect()?;
        let stored = self.reconcile_gate_state(&conn, self.load_gate_state(&conn)?)?;
        Ok(self.build_gate_state(stored))
    }

    pub fn confirm_local_user(&self) -> Result<GateStateView> {
        let conn = self.connect()?;
        let local_user = self.local_user();
        conn.execute(
            "
            UPDATE app_gate_state
            SET local_user_confirmed = 1,
                confirmed_username = ?1,
                confirmed_device_name = ?2
            WHERE id = 1
            ",
            params![local_user.username, local_user.device_name],
        )?;
        self.gate_state()
    }

    pub fn logout(&self) -> Result<GateStateView> {
        let conn = self.connect()?;
        conn.execute(
            "
            UPDATE app_gate_state
            SET local_user_confirmed = 0,
                confirmed_username = NULL,
                confirmed_device_name = NULL,
                api_gate_passed = 0,
                last_logout_at = ?1
            WHERE id = 1
            ",
            [utc_now()],
        )?;
        self.gate_state()
    }

    pub fn advance_api_gate(&self) -> Result<GateStateView> {
        let conn = self.connect()?;
        let gate_state = self.gate_state()?;
        if !gate_state.local_user_confirmed {
            anyhow::bail!("login confirmation is required before unlocking the main system");
        }

        let unlock_state = self.unlock_state()?;
        if !unlock_state.unlocked {
            anyhow::bail!(
                "required providers must be fully verified before unlocking the main system"
            );
        }

        conn.execute(
            "UPDATE app_gate_state SET api_gate_passed = 1 WHERE id = 1",
            [],
        )?;
        self.gate_state()
    }

    pub fn secret_exists(&self, provider: ProviderKind) -> Result<bool> {
        Ok(self
            .secret_store
            .get_secret(&secret_key(provider))?
            .unwrap_or_default()
            .trim()
            .is_empty()
            .not())
    }

    pub fn runtime_config(&self, provider: ProviderKind) -> Result<ProviderRuntimeConfig> {
        let conn = self.connect()?;
        let settings = self
            .raw_settings(&conn, provider)?
            .unwrap_or_else(|| default_settings(provider));
        let api_key = self.secret_store.get_secret(&secret_key(provider))?;
        Ok(ProviderRuntimeConfig {
            provider,
            enabled: settings.enabled,
            api_key,
            base_url: settings.base_url,
            monthly_limit: settings.monthly_limit,
            daily_soft_limit: settings.daily_soft_limit,
            default_role: settings.default_role,
            priority: settings.priority,
        })
    }

    pub fn runtime_configs(&self) -> Result<Vec<ProviderRuntimeConfig>> {
        ProviderKind::ALL
            .into_iter()
            .map(|provider| self.runtime_config(provider))
            .collect()
    }

    fn build_provider_view(
        &self,
        conn: &Connection,
        provider: ProviderKind,
    ) -> Result<ProviderStatusView> {
        let settings = self
            .raw_settings(conn, provider)?
            .unwrap_or_else(|| default_settings(provider));
        let secret = self.secret_store.get_secret(&secret_key(provider))?;
        let has_api_key = secret
            .as_deref()
            .is_some_and(|secret| !secret.trim().is_empty());
        let latest = latest_test(conn, provider)?;
        let status = latest
            .as_ref()
            .map(|record| record.status.clone())
            .unwrap_or_else(|| {
                if has_api_key {
                    "configured".to_string()
                } else {
                    "awaiting_configuration".to_string()
                }
            });
        let latency_ms = latest.as_ref().map(|record| record.latency_ms);
        let last_verified_at = latest.as_ref().map(|record| record.verified_at.clone());
        let last_error = latest.and_then(|record| record.error_message);

        Ok(ProviderStatusView {
            provider: provider.slug().to_string(),
            display_name: provider.display_name().to_string(),
            description: provider_description(provider).to_string(),
            enabled: settings.enabled,
            has_api_key,
            api_key_summary: secret
                .as_deref()
                .map(mask_secret_for_ui)
                .filter(|value| !value.is_empty()),
            base_url: settings.base_url,
            monthly_limit: settings.monthly_limit,
            daily_soft_limit: settings.daily_soft_limit,
            default_role: settings.default_role,
            priority: settings.priority,
            status,
            last_verified_at,
            last_error,
            latency_ms,
            is_required: provider.is_required(),
        })
    }

    fn raw_settings(
        &self,
        conn: &Connection,
        provider: ProviderKind,
    ) -> Result<Option<StoredSettings>> {
        conn.query_row(
            "
            SELECT enabled, base_url, monthly_limit, daily_soft_limit, default_role, priority
            FROM provider_settings
            WHERE provider = ?1
            ",
            [provider.slug()],
            |row| {
                Ok(StoredSettings {
                    enabled: row.get::<_, i64>(0)? == 1,
                    base_url: row.get(1)?,
                    monthly_limit: row.get(2)?,
                    daily_soft_limit: row.get(3)?,
                    default_role: row.get(4)?,
                    priority: row.get(5)?,
                })
            },
        )
        .optional()
        .context("failed to load provider settings")
    }

    fn connect(&self) -> Result<Connection> {
        let conn = Connection::open(&self.db_path)
            .with_context(|| format!("failed to open {}", self.db_path.display()))?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        Ok(conn)
    }

    fn load_gate_state(&self, conn: &Connection) -> Result<StoredGateState> {
        conn.query_row(
            "
            SELECT local_user_confirmed, confirmed_username, confirmed_device_name, api_gate_passed, last_logout_at
            FROM app_gate_state
            WHERE id = 1
            ",
            [],
            |row| {
                Ok(StoredGateState {
                    local_user_confirmed: row.get::<_, i64>(0)? == 1,
                    confirmed_username: row.get(1)?,
                    confirmed_device_name: row.get(2)?,
                    api_gate_passed: row.get::<_, i64>(3)? == 1,
                    last_logout_at: row.get(4)?,
                })
            },
        )
        .context("failed to load gate state")
    }

    fn reconcile_gate_state(
        &self,
        conn: &Connection,
        mut stored: StoredGateState,
    ) -> Result<StoredGateState> {
        let local_user = self.local_user();
        let mut changed = false;

        if stored.local_user_confirmed && !stored_gate_identity_matches(&stored, &local_user) {
            stored.local_user_confirmed = false;
            stored.confirmed_username = None;
            stored.confirmed_device_name = None;
            stored.api_gate_passed = false;
            changed = true;
        }

        if !stored.local_user_confirmed && stored.api_gate_passed {
            stored.api_gate_passed = false;
            changed = true;
        }

        if stored.local_user_confirmed && stored.api_gate_passed && !self.unlock_state()?.unlocked {
            stored.api_gate_passed = false;
            changed = true;
        }

        if changed {
            self.persist_gate_state(conn, &stored)?;
        }

        Ok(stored)
    }

    fn persist_gate_state(&self, conn: &Connection, stored: &StoredGateState) -> Result<()> {
        conn.execute(
            "
            UPDATE app_gate_state
            SET local_user_confirmed = ?1,
                confirmed_username = ?2,
                confirmed_device_name = ?3,
                api_gate_passed = ?4,
                last_logout_at = ?5
            WHERE id = 1
            ",
            params![
                bool_to_int(stored.local_user_confirmed),
                stored.confirmed_username.clone(),
                stored.confirmed_device_name.clone(),
                bool_to_int(stored.api_gate_passed),
                stored.last_logout_at.clone(),
            ],
        )?;
        Ok(())
    }

    fn build_gate_state(&self, stored: StoredGateState) -> GateStateView {
        let current_stage = if !stored.local_user_confirmed {
            "login"
        } else if !stored.api_gate_passed {
            "setup"
        } else {
            "galaxy"
        };

        GateStateView {
            current_stage: current_stage.to_string(),
            local_user_confirmed: stored.local_user_confirmed,
            confirmed_username: stored.confirmed_username,
            confirmed_device_name: stored.confirmed_device_name,
            api_gate_passed: stored.api_gate_passed,
            required_providers: ProviderKind::REQUIRED
                .into_iter()
                .map(|provider| provider.display_name().to_string())
                .collect(),
            last_logout_at: stored.last_logout_at,
        }
    }
}

#[derive(Debug, Clone)]
struct StoredSettings {
    enabled: bool,
    base_url: String,
    monthly_limit: i64,
    daily_soft_limit: i64,
    default_role: String,
    priority: i64,
}

#[derive(Debug, Clone)]
struct StoredGateState {
    local_user_confirmed: bool,
    confirmed_username: Option<String>,
    confirmed_device_name: Option<String>,
    api_gate_passed: bool,
    last_logout_at: Option<String>,
}

#[derive(Debug, Clone)]
struct LatestTestRecord {
    status: String,
    latency_ms: i64,
    error_message: Option<String>,
    verified_at: String,
}

fn stored_gate_identity_matches(stored: &StoredGateState, local_user: &LocalUserView) -> bool {
    stored.confirmed_username.as_deref() == Some(local_user.username.as_str())
        && stored.confirmed_device_name.as_deref() == Some(local_user.device_name.as_str())
}

fn latest_test(conn: &Connection, provider: ProviderKind) -> Result<Option<LatestTestRecord>> {
    conn.query_row(
        "
        SELECT status, latency_ms, error_message, verified_at
        FROM provider_test_logs
        WHERE provider = ?1
        ORDER BY id DESC
        LIMIT 1
        ",
        [provider.slug()],
        |row| {
            Ok(LatestTestRecord {
                status: row.get(0)?,
                latency_ms: row.get(1)?,
                error_message: row.get(2)?,
                verified_at: row.get(3)?,
            })
        },
    )
    .optional()
    .context("failed to load latest provider test")
}

fn default_settings(provider: ProviderKind) -> StoredSettings {
    StoredSettings {
        enabled: true,
        base_url: provider.default_base_url().to_string(),
        monthly_limit: provider.monthly_limit(),
        daily_soft_limit: provider.daily_soft_limit(),
        default_role: provider.default_role().to_string(),
        priority: provider.priority(),
    }
}

fn invalidate_provider_verification(conn: &Connection, provider: ProviderKind) -> Result<()> {
    conn.execute(
        "DELETE FROM provider_test_logs WHERE provider = ?1",
        [provider.slug()],
    )?;
    conn.execute(
        "DELETE FROM provider_quota_ledger WHERE provider = ?1",
        [provider.slug()],
    )?;
    Ok(())
}

fn invalidate_api_gate(conn: &Connection) -> Result<()> {
    conn.execute(
        "UPDATE app_gate_state SET api_gate_passed = 0 WHERE id = 1",
        [],
    )?;
    Ok(())
}

#[allow(dead_code)]
fn compute_unlock_state(views: &[ProviderStatusView]) -> UnlockStateView {
    let ready_count = views.iter().filter(|view| is_ready(view)).count();
    let blockers = views
        .iter()
        .filter(|view| !is_ready(view))
        .map(|view| {
            if !view.enabled {
                format!("{} 已暂停，主系统保持锁定。", view.display_name)
            } else if !view.has_api_key {
                format!("{} 尚未录入 API Key。", view.display_name)
            } else if view.status != "ready" {
                format!("{} 最近一次测试状态为 {}。", view.display_name, view.status)
            } else {
                format!("{} 尚未通过解锁校验。", view.display_name)
            }
        })
        .collect::<Vec<_>>();

    UnlockStateView {
        ready_count,
        total_count: ProviderKind::ALL.len(),
        optional_ready_count: 0,
        optional_total_count: 0,
        unlocked: ready_count == ProviderKind::ALL.len(),
        blockers,
        required_providers: ProviderKind::ALL
            .into_iter()
            .map(|provider| provider.display_name().to_string())
            .collect(),
        optional_providers: Vec::new(),
    }
}

fn is_ready(view: &ProviderStatusView) -> bool {
    view.enabled && view.has_api_key && view.status == "ready"
}

fn compute_unlock_state_v2(views: &[ProviderStatusView]) -> UnlockStateView {
    let ready_count = views
        .iter()
        .filter(|view| view.is_required && is_ready(view))
        .count();
    let optional_ready_count = views
        .iter()
        .filter(|view| !view.is_required && is_ready(view))
        .count();
    let blockers = views
        .iter()
        .filter(|view| view.is_required && !is_ready(view))
        .map(|view| {
            if !view.enabled {
                format!("{} 已停用，主系统保持锁定。", view.display_name)
            } else if !view.has_api_key {
                format!("{} 尚未录入 API Key。", view.display_name)
            } else if view.status != "ready" {
                format!("{} 最近一次测试状态为 {}。", view.display_name, view.status)
            } else {
                format!("{} 尚未通过解锁校验。", view.display_name)
            }
        })
        .collect::<Vec<_>>();

    UnlockStateView {
        ready_count,
        total_count: ProviderKind::REQUIRED.len(),
        optional_ready_count,
        optional_total_count: ProviderKind::OPTIONAL.len(),
        unlocked: ready_count == ProviderKind::REQUIRED.len(),
        blockers,
        required_providers: ProviderKind::REQUIRED
            .into_iter()
            .map(|provider| provider.display_name().to_string())
            .collect(),
        optional_providers: ProviderKind::OPTIONAL
            .into_iter()
            .map(|provider| provider.display_name().to_string())
            .collect(),
    }
}

fn simulate_test(
    provider: ProviderKind,
    settings: &StoredSettings,
    secret: Option<&str>,
) -> ProviderTestResponse {
    let verified_at = utc_now();
    let api_key = secret.unwrap_or_default().trim().to_string();
    let provider_index = provider.priority();
    let base_url = settings.base_url.to_ascii_lowercase();

    let (ok, status, latency_ms, error_message) = if !settings.enabled {
        (
            false,
            "disabled".to_string(),
            0,
            Some("provider has been paused from the setup page".to_string()),
        )
    } else if api_key.is_empty() {
        (
            false,
            "missing_api_key".to_string(),
            0,
            Some("API key is required before provider testing".to_string()),
        )
    } else if api_key.contains("timeout") || base_url.contains("timeout") {
        (
            false,
            "timeout".to_string(),
            3_200,
            Some("simulated timeout path reached".to_string()),
        )
    } else if api_key.contains("fail") || base_url.contains("fail") {
        (
            false,
            "failed".to_string(),
            640,
            Some("simulated provider failure returned a non-200 response".to_string()),
        )
    } else {
        (true, "ready".to_string(), 180 + provider_index * 37, None)
    };

    let used_monthly = if ok {
        provider_index * 19 + 7
    } else {
        provider_index * 11
    };
    let remaining_monthly = (settings.monthly_limit - used_monthly).max(0);

    ProviderTestResponse {
        provider: provider.slug().to_string(),
        ok,
        status,
        latency_ms,
        quota_snapshot: QuotaSnapshot {
            used_monthly,
            remaining_monthly,
            daily_soft_limit: settings.daily_soft_limit,
            monthly_limit: settings.monthly_limit,
        },
        error_message,
        verified_at,
    }
}

#[allow(dead_code)]
fn mask_secret(secret: &str) -> String {
    let trimmed = secret.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let suffix = trimmed
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    format!("••••••{suffix}")
}

fn secret_key(provider: ProviderKind) -> String {
    format!("provider::{}", provider.slug())
}

fn provider_description(provider: ProviderKind) -> &'static str {
    match provider {
        ProviderKind::Doubao => "主分析与结构化输出中枢，负责内容拆解与总结。",
        ProviderKind::Serper => "主搜索入口，用于发现高频热点与基础候选样本。",
        ProviderKind::Firecrawl => "正文抓取器，用来提取页面正文与落地页结构。",
        ProviderKind::Tavily => "时效验证与热点补位，用来校验热度和近期变化。",
        ProviderKind::Exa => "长尾发现与相邻主题扩展，补全潜在选题空间。",
    }
}

fn mask_secret_for_ui(secret: &str) -> String {
    let trimmed = secret.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let suffix = trimmed
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    format!("******{suffix}")
}

fn bool_to_int(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}

fn utc_now() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .expect("current utc time")
}

#[allow(dead_code)]
pub fn provider_db_path(workspace_root: &Path) -> PathBuf {
    workspace_root.join("data").join("spg_web.db")
}

trait BoolExt {
    fn not(self) -> bool;
}

impl BoolExt for bool {
    fn not(self) -> bool {
        !self
    }
}
