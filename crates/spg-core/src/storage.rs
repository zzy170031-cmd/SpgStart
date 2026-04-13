use crate::models::{AssetBundle, PipelineRunResult};
use crate::text_utils::fingerprint;
use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::Serialize;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct SQLiteStorage {
    db_path: PathBuf,
}

impl SQLiteStorage {
    pub fn new(path: &Path) -> Self {
        Self {
            db_path: path.to_path_buf(),
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
            CREATE TABLE IF NOT EXISTS pipeline_runs (
                run_id TEXT PRIMARY KEY,
                asset_version TEXT NOT NULL,
                rule_version TEXT NOT NULL,
                input_count INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                is_current INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS raw_items (
                source_id TEXT PRIMARY KEY,
                platform TEXT NOT NULL,
                source_type TEXT NOT NULL,
                content_type TEXT NOT NULL,
                title TEXT,
                text TEXT,
                author TEXT,
                published_at TEXT,
                url TEXT,
                raw_metrics_json TEXT NOT NULL,
                fetched_at TEXT,
                metadata_json TEXT NOT NULL,
                run_id TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS clean_items (
                clean_id TEXT PRIMARY KEY,
                source_id TEXT NOT NULL,
                platform TEXT NOT NULL,
                source_type TEXT NOT NULL,
                content_type TEXT NOT NULL,
                source_layer TEXT NOT NULL,
                normalized_title TEXT,
                normalized_text TEXT,
                normalized_author TEXT,
                normalized_url TEXT,
                reference_urls_json TEXT NOT NULL,
                normalized_published_at TEXT,
                normalized_metrics_json TEXT NOT NULL,
                title_tokens_json TEXT NOT NULL,
                text_tokens_json TEXT NOT NULL,
                title_fingerprint TEXT NOT NULL,
                text_fingerprint TEXT NOT NULL,
                url_fingerprint TEXT,
                reference_url_fingerprints_json TEXT NOT NULL,
                content_fingerprint TEXT NOT NULL,
                metadata_json TEXT NOT NULL,
                run_id TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS resolved_signals (
                signal_id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL,
                asset_version TEXT NOT NULL,
                rule_version TEXT NOT NULL,
                raw_source_ids_json TEXT NOT NULL,
                platform TEXT NOT NULL,
                source_layer TEXT NOT NULL,
                source_type TEXT NOT NULL,
                content_type TEXT NOT NULL,
                title TEXT,
                text TEXT,
                author TEXT,
                published_at TEXT,
                normalized_published_at TEXT,
                url TEXT,
                scope TEXT NOT NULL,
                track_id TEXT,
                track_candidates_json TEXT NOT NULL,
                dedupe_key TEXT NOT NULL,
                information_key TEXT NOT NULL,
                dup_group_id TEXT,
                canonical_signal_id TEXT NOT NULL,
                dup_rank INTEGER NOT NULL,
                duplicate_type TEXT NOT NULL,
                game_candidates_json TEXT NOT NULL,
                resolved_game_id TEXT,
                entity_resolution_method TEXT NOT NULL,
                entity_resolution_confidence REAL NOT NULL,
                event_type TEXT,
                attribution_keywords_json TEXT NOT NULL,
                unresolved_reason TEXT,
                state TEXT NOT NULL,
                metrics_json TEXT NOT NULL,
                trace_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS dup_groups (
                dup_group_id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL,
                canonical_signal_id TEXT NOT NULL,
                duplicate_type TEXT NOT NULL,
                dedupe_key TEXT NOT NULL,
                member_signal_ids_json TEXT NOT NULL,
                member_source_ids_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS entity_aliases (
                alias TEXT NOT NULL,
                game_id TEXT NOT NULL,
                track TEXT NOT NULL,
                strength TEXT NOT NULL,
                source TEXT NOT NULL,
                asset_version TEXT NOT NULL,
                PRIMARY KEY(alias, game_id, source)
            );

            CREATE TABLE IF NOT EXISTS resolution_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                run_id TEXT NOT NULL,
                asset_version TEXT NOT NULL,
                rule_version TEXT NOT NULL,
                signal_id TEXT NOT NULL,
                source_id TEXT NOT NULL,
                stage TEXT NOT NULL,
                decision TEXT NOT NULL,
                detail_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS unresolved_pool (
                unresolved_id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL,
                asset_version TEXT NOT NULL,
                rule_version TEXT NOT NULL,
                source_id TEXT NOT NULL,
                title TEXT,
                platform TEXT NOT NULL,
                track_id TEXT,
                track_candidates_json TEXT NOT NULL,
                reason TEXT NOT NULL,
                candidate_game_ids_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            -- Signal query indexes
            CREATE INDEX IF NOT EXISTS idx_signals_game ON resolved_signals(resolved_game_id);
            CREATE INDEX IF NOT EXISTS idx_signals_state ON resolved_signals(state);
            CREATE INDEX IF NOT EXISTS idx_signals_run ON resolved_signals(run_id);
            CREATE INDEX IF NOT EXISTS idx_signals_platform ON resolved_signals(platform);
            CREATE INDEX IF NOT EXISTS idx_signals_dedupe ON resolved_signals(dedupe_key);

            -- Fingerprint indexes (for future cross-batch dedup)
            CREATE INDEX IF NOT EXISTS idx_clean_content_fp ON clean_items(content_fingerprint);
            CREATE INDEX IF NOT EXISTS idx_clean_url_fp ON clean_items(url_fingerprint);

            -- Audit log indexes
            CREATE INDEX IF NOT EXISTS idx_logs_signal ON resolution_logs(signal_id);
            CREATE INDEX IF NOT EXISTS idx_logs_run ON resolution_logs(run_id);

            -- Unresolved pool indexes
            CREATE INDEX IF NOT EXISTS idx_unresolved_run ON unresolved_pool(run_id);
            CREATE INDEX IF NOT EXISTS idx_unresolved_platform ON unresolved_pool(platform);

            -- Pipeline runs
            CREATE INDEX IF NOT EXISTS idx_runs_current ON pipeline_runs(is_current);
            ",
        )?;
        Ok(())
    }

    pub fn seed_aliases(&self, bundle: &AssetBundle) -> Result<()> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        let asset_version = asset_version_for_bundle(bundle);
        tx.execute("DELETE FROM entity_aliases", [])?;
        for game in &bundle.games {
            tx.execute(
                "
                INSERT OR REPLACE INTO entity_aliases(alias, game_id, track, strength, source, asset_version)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                ",
                params![
                    game.standard_name,
                    game.game_id,
                    game.track,
                    "strong",
                    "standard_name",
                    asset_version
                ],
            )?;
            for alias in &game.aliases {
                tx.execute(
                    "
                    INSERT OR REPLACE INTO entity_aliases(alias, game_id, track, strength, source, asset_version)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                    ",
                    params![
                        alias.value,
                        game.game_id,
                        game.track,
                        alias.strength,
                        alias.source,
                        asset_version
                    ],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn persist_result(&self, result: &PipelineRunResult) -> Result<()> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;

        tx.execute("UPDATE pipeline_runs SET is_current = 0", [])?;
        tx.execute(
            "
            INSERT INTO pipeline_runs(run_id, asset_version, rule_version, input_count, created_at, is_current)
            VALUES (?1, ?2, ?3, ?4, ?5, 1)
            ",
            params![
                result.run_id,
                result.asset_version,
                result.rule_version,
                result.raw_items.len() as i64,
                result.run_created_at,
            ],
        )?;

        for table in [
            "raw_items",
            "clean_items",
            "resolved_signals",
            "dup_groups",
            "resolution_logs",
            "unresolved_pool",
        ] {
            tx.execute(&format!("DELETE FROM {table}"), [])?;
        }

        for item in &result.raw_items {
            tx.execute(
                "
                INSERT OR REPLACE INTO raw_items(
                    source_id, platform, source_type, content_type, title, text, author,
                    published_at, url, raw_metrics_json, fetched_at, metadata_json, run_id
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                ",
                params![
                    item.source_id,
                    item.platform,
                    item.source_type,
                    item.content_type,
                    item.title,
                    item.text,
                    item.author,
                    item.published_at,
                    item.url,
                    to_json(&item.raw_metrics)?,
                    item.fetched_at,
                    to_json(&item.metadata)?,
                    result.run_id,
                ],
            )?;
        }

        for item in &result.clean_items {
            tx.execute(
                "
                INSERT OR REPLACE INTO clean_items(
                    clean_id, source_id, platform, source_type, content_type, source_layer,
                    normalized_title, normalized_text, normalized_author, normalized_url,
                    reference_urls_json, normalized_published_at, normalized_metrics_json,
                    title_tokens_json, text_tokens_json, title_fingerprint, text_fingerprint,
                    url_fingerprint, reference_url_fingerprints_json, content_fingerprint,
                    metadata_json, run_id
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)
                ",
                params![
                    item.clean_id,
                    item.source_id,
                    item.platform,
                    item.source_type,
                    item.content_type,
                    item.source_layer,
                    item.normalized_title,
                    item.normalized_text,
                    item.normalized_author,
                    item.normalized_url,
                    to_json(&item.reference_urls)?,
                    item.normalized_published_at,
                    to_json(&item.normalized_metrics)?,
                    to_json(&item.title_tokens)?,
                    to_json(&item.text_tokens)?,
                    item.title_fingerprint,
                    item.text_fingerprint,
                    item.url_fingerprint,
                    to_json(&item.reference_url_fingerprints)?,
                    item.content_fingerprint,
                    to_json(&item.metadata)?,
                    result.run_id,
                ],
            )?;
        }

        for signal in &result.signals {
            tx.execute(
                "
                INSERT OR REPLACE INTO resolved_signals(
                    signal_id, run_id, asset_version, rule_version, raw_source_ids_json,
                    platform, source_layer, source_type, content_type, title, text, author,
                    published_at, normalized_published_at, url, scope, track_id,
                    track_candidates_json, dedupe_key, information_key, dup_group_id,
                    canonical_signal_id, dup_rank, duplicate_type, game_candidates_json,
                    resolved_game_id, entity_resolution_method, entity_resolution_confidence,
                    event_type, attribution_keywords_json, unresolved_reason, state,
                    metrics_json, trace_json, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                          ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29,
                          ?30, ?31, ?32, ?33, ?34, ?35)
                ",
                params![
                    signal.signal_id,
                    signal.run_id,
                    signal.asset_version,
                    signal.rule_version,
                    to_json(&signal.raw_source_ids)?,
                    signal.platform,
                    signal.source_layer,
                    signal.source_type,
                    signal.content_type,
                    signal.title,
                    signal.text,
                    signal.author,
                    signal.published_at,
                    signal.normalized_published_at,
                    signal.url,
                    signal.scope,
                    signal.track_id,
                    to_json(&signal.track_candidates)?,
                    signal.dedupe_key,
                    signal.information_key,
                    signal.dup_group_id,
                    signal.canonical_signal_id,
                    signal.dup_rank,
                    signal.duplicate_type,
                    to_json(&signal.game_candidates)?,
                    signal.resolved_game_id,
                    signal.entity_resolution_method,
                    signal.entity_resolution_confidence,
                    signal.event_type,
                    to_json(&signal.attribution_keywords)?,
                    signal.unresolved_reason,
                    signal.state,
                    to_json(&signal.metrics)?,
                    to_json(&signal.trace)?,
                    signal.created_at,
                ],
            )?;
        }

        for group in &result.dedupe_groups {
            tx.execute(
                "
                INSERT OR REPLACE INTO dup_groups(
                    dup_group_id, run_id, canonical_signal_id, duplicate_type, dedupe_key,
                    member_signal_ids_json, member_source_ids_json, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                ",
                params![
                    group.dup_group_id,
                    group.run_id,
                    group.canonical_signal_id,
                    group.duplicate_type,
                    group.dedupe_key,
                    to_json(&group.member_signal_ids)?,
                    to_json(&group.member_source_ids)?,
                    group.created_at,
                ],
            )?;
        }

        for log in &result.resolution_logs {
            tx.execute(
                "
                INSERT INTO resolution_logs(
                    run_id, asset_version, rule_version, signal_id, source_id, stage,
                    decision, detail_json, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                ",
                params![
                    log.run_id,
                    log.asset_version,
                    log.rule_version,
                    log.signal_id,
                    log.source_id,
                    log.stage,
                    log.decision,
                    to_json(&log.detail)?,
                    log.created_at,
                ],
            )?;
        }

        for unresolved in &result.unresolved_records {
            tx.execute(
                "
                INSERT OR REPLACE INTO unresolved_pool(
                    unresolved_id, run_id, asset_version, rule_version, source_id, title,
                    platform, track_id, track_candidates_json, reason,
                    candidate_game_ids_json, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                ",
                params![
                    unresolved.unresolved_id,
                    unresolved.run_id,
                    unresolved.asset_version,
                    unresolved.rule_version,
                    unresolved.source_id,
                    unresolved.title,
                    unresolved.platform,
                    unresolved.track_id,
                    to_json(&unresolved.track_candidates)?,
                    unresolved.reason,
                    to_json(&unresolved.candidate_game_ids)?,
                    unresolved.created_at,
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    pub fn row_counts(&self) -> Result<Value> {
        let conn = self.connect()?;
        let tables = [
            "pipeline_runs",
            "raw_items",
            "clean_items",
            "resolved_signals",
            "dup_groups",
            "entity_aliases",
            "resolution_logs",
            "unresolved_pool",
        ];
        let mut payload = serde_json::Map::new();
        for table in tables {
            let count: i64 =
                conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })?;
            payload.insert(table.to_string(), json!(count));
        }
        Ok(Value::Object(payload))
    }

    fn connect(&self) -> Result<Connection> {
        let conn = Connection::open(&self.db_path)
            .with_context(|| format!("failed to open {}", self.db_path.display()))?;
        conn.execute_batch(
            "
            PRAGMA foreign_keys = ON;
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA temp_store = MEMORY;
            ",
        )?;
        Ok(conn)
    }
}

fn to_json<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).context("failed to serialize JSON payload")
}

fn asset_version_for_bundle(bundle: &AssetBundle) -> String {
    let serialized = serde_json::to_string(bundle).unwrap_or_else(|_| "asset_bundle".to_string());
    format!("entity_assets_{}", fingerprint(&[&serialized], 12))
}
