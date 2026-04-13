use crate::assets::{AliasLookup, AssetIndex};
use crate::models::{
    CleanItem, DedupeGroup, MatchEvidence, PipelineRunResult, RawSourceItem, ResolutionLog,
    SourceSignal, TraceEntry, UnresolvedRecord,
};
use crate::text_utils::{
    extract_reference_urls, fingerprint, infer_source_layer, json_ratio, new_run_id,
    normalize_author, normalize_content_type, normalize_metrics, normalize_platform,
    normalize_source_type, normalize_text, normalize_timestamp, normalize_url, semantic_basis,
    tokenize, utc_now_iso,
};
use serde_json::json;
use std::collections::{BTreeSet, HashMap};
use url::Url;

const RULE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone)]
struct CandidateScore {
    game_id: String,
    score: f64,
    evidences: Vec<MatchEvidence>,
}

#[derive(Debug, Clone)]
struct ResolutionOutcome {
    candidates: Vec<String>,
    track_candidates: Vec<String>,
    resolved_game_id: Option<String>,
    resolved_track_id: Option<String>,
    method: String,
    confidence: f64,
    scope: String,
    unresolved_reason: Option<String>,
    candidate_trace: TraceEntry,
    resolution_trace: TraceEntry,
}

#[derive(Debug, Clone)]
struct EventOutcome {
    event_type: Option<String>,
    keywords: Vec<String>,
    trace: TraceEntry,
}

#[derive(Debug, Clone)]
pub struct SPGInputPipeline {
    assets: AssetIndex,
}

impl SPGInputPipeline {
    pub fn new(assets: AssetIndex) -> Self {
        Self { assets }
    }

    pub fn process(&self, items: Vec<RawSourceItem>) -> PipelineRunResult {
        let run_seed = items
            .iter()
            .map(|item| item.source_id.as_str())
            .collect::<Vec<_>>()
            .join("|");
        let run_id = new_run_id(&run_seed);
        let run_created_at = utc_now_iso();
        let asset_version = self.assets.asset_version.clone();
        let rule_version = format!("v{RULE_VERSION}");

        let mut clean_items = Vec::new();
        let mut signals = Vec::new();
        let mut dedupe_groups: HashMap<String, DedupeGroup> = HashMap::new();
        let mut resolution_logs = Vec::new();
        let mut unresolved_records = Vec::new();

        let mut content_index: HashMap<String, String> = HashMap::new();
        let mut information_index: HashMap<String, String> = HashMap::new();
        let mut canonical_sources: HashMap<String, String> = HashMap::new();

        for raw in &items {
            let clean = self.clean_item(raw);
            let clean_trace = TraceEntry {
                stage: "clean".to_string(),
                decision: "normalized".to_string(),
                detail: json!({
                    "platform": clean.platform,
                    "source_type": clean.source_type,
                    "source_layer": clean.source_layer,
                    "normalized_url": clean.normalized_url,
                    "reference_urls": clean.reference_urls,
                    "normalized_published_at": clean.normalized_published_at,
                    "url_fingerprint": clean.url_fingerprint,
                    "reference_url_fingerprints": clean.reference_url_fingerprints,
                    "content_fingerprint": clean.content_fingerprint,
                }),
            };

            let resolution = self.resolve_candidates(&clean);
            let event = self.attribute_event(&clean);
            let signal_id = format!(
                "sig_{}",
                fingerprint(&[&run_id, &raw.source_id, &clean.content_fingerprint], 20)
            );

            let primary_content_key = clean
                .url_fingerprint
                .as_ref()
                .map(|fingerprint| format!("url::{fingerprint}"))
                .unwrap_or_else(|| format!("content::{}", clean.content_fingerprint));
            let mut content_keys = Vec::new();
            if let Some(url_fingerprint) = &clean.url_fingerprint {
                content_keys.push(format!("url::{url_fingerprint}"));
            }
            for reference_url_fingerprint in &clean.reference_url_fingerprints {
                content_keys.push(format!("url::{reference_url_fingerprint}"));
            }
            content_keys.push(format!("content::{}", clean.content_fingerprint));

            let info_basis = if let Some(game_id) = &resolution.resolved_game_id {
                format!("game::{game_id}")
            } else if let Some(track_id) = &resolution.resolved_track_id {
                format!("track::{track_id}")
            } else {
                format!("source::{}", raw.source_id)
            };
            let event_key = event
                .event_type
                .clone()
                .unwrap_or_else(|| "general".to_string());
            let information_key = format!(
                "info::{}::{}::{}",
                info_basis,
                event_key,
                fingerprint(
                    &[&semantic_basis(
                        &clean.normalized_title,
                        &clean.normalized_text
                    )],
                    20,
                )
            );

            let mut duplicate_type = "none".to_string();
            let mut canonical_signal_id = signal_id.clone();
            let mut dup_group_id = String::new();
            let mut dup_rank = 0_i64;
            let mut matched_content_key: Option<String> = None;

            if let Some((matched_key, existing)) = content_keys.iter().find_map(|key| {
                content_index
                    .get(key)
                    .cloned()
                    .map(|existing| (key.clone(), existing))
            }) {
                duplicate_type = "content_near_duplicate".to_string();
                canonical_signal_id = existing;
                matched_content_key = Some(matched_key);
            } else if let Some(existing) = information_index.get(&information_key) {
                duplicate_type = "information_duplicate".to_string();
                canonical_signal_id = existing.clone();
            } else {
                for key in &content_keys {
                    content_index.insert(key.clone(), signal_id.clone());
                }
                information_index.insert(information_key.clone(), signal_id.clone());
                canonical_sources.insert(signal_id.clone(), clean.source_id.clone());
            }

            let effective_dedupe_key = matched_content_key
                .clone()
                .unwrap_or_else(|| primary_content_key.clone());

            if duplicate_type != "none" {
                let group_key = if duplicate_type == "content_near_duplicate" {
                    effective_dedupe_key.clone()
                } else {
                    information_key.clone()
                };
                dup_group_id = format!("dup_{}", fingerprint(&[&group_key], 16));
                let group = dedupe_groups
                    .entry(dup_group_id.clone())
                    .or_insert_with(|| {
                        let canonical_source = canonical_sources
                            .get(&canonical_signal_id)
                            .cloned()
                            .unwrap_or_default();
                        DedupeGroup {
                            dup_group_id: dup_group_id.clone(),
                            run_id: run_id.clone(),
                            canonical_signal_id: canonical_signal_id.clone(),
                            duplicate_type: duplicate_type.clone(),
                            dedupe_key: group_key.clone(),
                            member_signal_ids: vec![canonical_signal_id.clone()],
                            member_source_ids: if canonical_source.is_empty() {
                                Vec::new()
                            } else {
                                vec![canonical_source]
                            },
                            created_at: utc_now_iso(),
                        }
                    });
                if !group.member_signal_ids.contains(&signal_id) {
                    group.member_signal_ids.push(signal_id.clone());
                    group.member_source_ids.push(clean.source_id.clone());
                }
                dup_rank = group.member_signal_ids.len().saturating_sub(1) as i64;
            }

            let state = if duplicate_type != "none" {
                "folded".to_string()
            } else if resolution.resolved_game_id.is_some() {
                "canonical".to_string()
            } else if resolution.resolved_track_id.is_some() {
                "track_level".to_string()
            } else {
                "unresolved".to_string()
            };

            let dedupe_trace = TraceEntry {
                stage: "dedupe".to_string(),
                decision: if duplicate_type == "none" {
                    "canonical".to_string()
                } else {
                    duplicate_type.clone()
                },
                detail: json!({
                    "canonical_signal_id": canonical_signal_id,
                    "dup_group_id": dup_group_id,
                    "primary_content_key": primary_content_key,
                    "matched_content_key": matched_content_key,
                    "content_keys": content_keys,
                    "information_key": information_key,
                }),
            };

            let trace = vec![
                clean_trace.clone(),
                resolution.candidate_trace.clone(),
                resolution.resolution_trace.clone(),
                event.trace.clone(),
                dedupe_trace.clone(),
            ];

            let signal = SourceSignal {
                signal_id: signal_id.clone(),
                run_id: run_id.clone(),
                asset_version: asset_version.clone(),
                rule_version: rule_version.clone(),
                raw_source_ids: vec![raw.source_id.clone()],
                platform: clean.platform.clone(),
                source_layer: clean.source_layer.clone(),
                source_type: clean.source_type.clone(),
                content_type: clean.content_type.clone(),
                title: clean.title.clone(),
                text: clean.text.clone(),
                author: clean.author.clone(),
                published_at: clean.published_at.clone(),
                normalized_published_at: clean.normalized_published_at.clone(),
                url: clean.url.clone(),
                scope: resolution.scope.clone(),
                track_id: resolution.resolved_track_id.clone(),
                track_candidates: resolution.track_candidates.clone(),
                dedupe_key: effective_dedupe_key,
                information_key,
                dup_group_id: dup_group_id.clone(),
                canonical_signal_id: canonical_signal_id.clone(),
                dup_rank,
                duplicate_type: duplicate_type.clone(),
                game_candidates: resolution.candidates.clone(),
                resolved_game_id: resolution.resolved_game_id.clone(),
                entity_resolution_method: resolution.method.clone(),
                entity_resolution_confidence: resolution.confidence,
                event_type: event.event_type.clone(),
                attribution_keywords: event.keywords.clone(),
                unresolved_reason: resolution.unresolved_reason.clone(),
                state,
                metrics: json!({
                    "raw_metrics": raw.raw_metrics,
                    "normalized_metrics": clean.normalized_metrics,
                }),
                trace: trace.clone(),
                created_at: utc_now_iso(),
            };

            if let Some(reason) = &resolution.unresolved_reason {
                unresolved_records.push(UnresolvedRecord {
                    unresolved_id: format!("unresolved_{}", signal_id),
                    run_id: run_id.clone(),
                    asset_version: asset_version.clone(),
                    rule_version: rule_version.clone(),
                    source_id: raw.source_id.clone(),
                    title: raw.title.clone(),
                    platform: clean.platform.clone(),
                    track_id: resolution.resolved_track_id.clone(),
                    track_candidates: resolution.track_candidates.clone(),
                    reason: reason.clone(),
                    candidate_game_ids: resolution.candidates.clone(),
                    created_at: utc_now_iso(),
                });
            }

            for entry in &trace {
                resolution_logs.push(ResolutionLog {
                    run_id: run_id.clone(),
                    asset_version: asset_version.clone(),
                    rule_version: rule_version.clone(),
                    signal_id: signal_id.clone(),
                    source_id: raw.source_id.clone(),
                    stage: entry.stage.clone(),
                    decision: entry.decision.clone(),
                    detail: entry.detail.clone(),
                    created_at: utc_now_iso(),
                });
            }

            clean_items.push(clean);
            signals.push(signal);
        }

        let metrics = self.build_metrics(&signals);
        PipelineRunResult {
            run_id,
            asset_version,
            rule_version,
            run_created_at,
            raw_items: items,
            clean_items,
            signals,
            dedupe_groups: dedupe_groups.into_values().collect(),
            resolution_logs,
            unresolved_records,
            metrics,
        }
    }

    fn clean_item(&self, raw: &RawSourceItem) -> CleanItem {
        let platform = normalize_platform(&raw.platform);
        let source_type = normalize_source_type(&raw.source_type);
        let content_type = normalize_content_type(&raw.content_type);
        let source_layer = infer_source_layer(&source_type, &platform);
        let normalized_title = normalize_text(&raw.title);
        let normalized_text = normalize_text(&raw.text);
        let normalized_author = normalize_author(&raw.author);
        let normalized_url = normalize_url(&raw.url);
        let mut reference_urls = extract_reference_urls(&format!("{} {}", raw.title, raw.text));
        if let Some(original_url) = raw
            .metadata
            .get("original_url")
            .and_then(|value| value.as_str())
        {
            let normalized_original_url = normalize_url(original_url);
            if !normalized_original_url.is_empty()
                && !reference_urls.contains(&normalized_original_url)
            {
                reference_urls.push(normalized_original_url);
            }
        }
        let normalized_metrics = normalize_metrics(&raw.raw_metrics);
        let normalized_published_at = normalize_timestamp(&raw.published_at);
        let title_tokens = tokenize(&normalized_title);
        let text_tokens = tokenize(&normalized_text);
        let title_fingerprint = fingerprint(&[&normalized_title], 24);
        let text_fingerprint = fingerprint(&[&normalized_text], 24);
        let url_fingerprint = if normalized_url.is_empty() {
            None
        } else {
            Some(fingerprint(&[&normalized_url], 24))
        };
        let reference_url_fingerprints = reference_urls
            .iter()
            .map(|url| fingerprint(&[url], 24))
            .collect::<Vec<_>>();
        let content_fingerprint = fingerprint(&[&normalized_title, &normalized_text], 24);

        CleanItem {
            clean_id: format!("clean_{}", raw.source_id),
            source_id: raw.source_id.clone(),
            platform,
            source_type,
            content_type,
            source_layer,
            title: raw.title.clone(),
            text: raw.text.clone(),
            author: raw.author.clone(),
            published_at: raw.published_at.clone(),
            normalized_published_at,
            url: raw.url.clone(),
            normalized_title,
            normalized_text,
            normalized_author,
            normalized_url,
            reference_urls,
            normalized_metrics,
            title_tokens,
            text_tokens,
            title_fingerprint,
            text_fingerprint,
            url_fingerprint,
            reference_url_fingerprints,
            content_fingerprint,
            metadata: raw.metadata.clone(),
        }
    }

    fn resolve_candidates(&self, clean: &CleanItem) -> ResolutionOutcome {
        let mut evidence_map: HashMap<String, Vec<MatchEvidence>> = HashMap::new();
        let mut add_match =
            |game_id: &str, alias: &str, strength: &str, source: &str, score: f64| {
                evidence_map
                    .entry(game_id.to_string())
                    .or_default()
                    .push(MatchEvidence {
                        game_id: game_id.to_string(),
                        alias: alias.to_string(),
                        strength: strength.to_string(),
                        source: source.to_string(),
                        score,
                    });
            };

        if let Some(game_id) = self
            .assets
            .official_account_index
            .get(&clean.normalized_author)
        {
            add_match(
                game_id,
                &clean.normalized_author,
                "strong",
                "official_account",
                0.99,
            );
        }

        for (seed, game_id) in &self.assets.official_url_seed_index {
            if !clean.normalized_url.is_empty() && url_seed_matches(&clean.normalized_url, seed) {
                add_match(game_id, seed, "strong", "official_url_seed", 0.98);
            }
        }

        let title = clean.normalized_title.to_lowercase();
        let body = clean.normalized_text.to_lowercase();
        for (alias, lookups) in &self.assets.alias_index {
            if alias.is_empty() || self.assets.negative_aliases.contains(alias) {
                continue;
            }
            if title.contains(alias) {
                for lookup in lookups {
                    add_match_for_lookup(&mut add_match, lookup, alias, true);
                }
            } else if body.contains(alias) {
                for lookup in lookups {
                    add_match_for_lookup(&mut add_match, lookup, alias, false);
                }
            }
        }

        let mut ranked = evidence_map
            .into_iter()
            .map(|(game_id, evidences)| {
                let top_score = evidences
                    .iter()
                    .map(|evidence| evidence.score)
                    .fold(0.0_f64, f64::max);
                let bonus = ((evidences.len().saturating_sub(1)) as f64 * 0.03).min(0.12);
                CandidateScore {
                    game_id,
                    score: (top_score + bonus).min(0.99),
                    evidences,
                }
            })
            .collect::<Vec<_>>();
        ranked.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(left.game_id.cmp(&right.game_id))
        });

        let candidates = ranked
            .iter()
            .map(|candidate| candidate.game_id.clone())
            .collect::<Vec<_>>();
        let track_candidates = unique_tracks(&self.assets, &candidates);
        let candidate_trace = TraceEntry {
            stage: "candidate_identification".to_string(),
            decision: "scored_candidates".to_string(),
            detail: json!({
                "track_candidates": track_candidates,
                "candidates": ranked
                    .iter()
                    .map(|candidate| json!({
                        "game_id": candidate.game_id,
                        "track_id": self.assets.track_for_game(&candidate.game_id),
                        "score": round_score(candidate.score),
                        "evidences": candidate.evidences,
                    }))
                    .collect::<Vec<_>>()
            }),
        };

        if ranked.is_empty() {
            return ResolutionOutcome {
                candidates,
                track_candidates,
                resolved_game_id: None,
                resolved_track_id: None,
                method: "unresolved".to_string(),
                confidence: 0.0,
                scope: "unresolved".to_string(),
                unresolved_reason: Some("no_candidate".to_string()),
                candidate_trace,
                resolution_trace: TraceEntry {
                    stage: "entity_resolution".to_string(),
                    decision: "unresolved".to_string(),
                    detail: json!({
                        "reason": "no_candidate",
                        "scope": "unresolved",
                    }),
                },
            };
        }

        let top = &ranked[0];
        let second_score = ranked
            .get(1)
            .map(|candidate| candidate.score)
            .unwrap_or(0.0);
        let gap = top.score - second_score;
        let top_track_id = self.assets.track_for_game(&top.game_id);
        let has_official_account = top
            .evidences
            .iter()
            .any(|evidence| evidence.source == "official_account");
        let has_official_url_seed = top
            .evidences
            .iter()
            .any(|evidence| evidence.source == "official_url_seed");
        let has_standard_name = top
            .evidences
            .iter()
            .any(|evidence| evidence.source == "standard_name");

        let (method, resolved_game_id, resolved_track_id, confidence, scope, unresolved_reason) =
            if has_official_account {
                (
                    "official_account_match".to_string(),
                    Some(top.game_id.clone()),
                    top_track_id.clone(),
                    0.99,
                    "game".to_string(),
                    None,
                )
            } else if has_official_url_seed {
                (
                    "official_url_seed_match".to_string(),
                    Some(top.game_id.clone()),
                    top_track_id.clone(),
                    0.98,
                    "game".to_string(),
                    None,
                )
            } else if ranked.len() == 1 && has_standard_name {
                (
                    "standard_name_match".to_string(),
                    Some(top.game_id.clone()),
                    top_track_id.clone(),
                    round_score(top.score.max(0.9)),
                    "game".to_string(),
                    None,
                )
            } else if top.score >= 0.86 && (ranked.len() == 1 || gap >= 0.10) {
                (
                    "alias_match".to_string(),
                    Some(top.game_id.clone()),
                    top_track_id.clone(),
                    round_score(top.score),
                    "game".to_string(),
                    None,
                )
            } else if ranked.len() == 1 && top.score >= 0.72 {
                (
                    "weak_alias_match".to_string(),
                    Some(top.game_id.clone()),
                    top_track_id.clone(),
                    round_score(top.score),
                    "game".to_string(),
                    None,
                )
            } else if matches!(clean.source_layer.as_str(), "official" | "media")
                && top.score >= 0.68
                && gap >= 0.15
            {
                (
                    "context_resolution".to_string(),
                    Some(top.game_id.clone()),
                    top_track_id.clone(),
                    round_score(top.score),
                    "game".to_string(),
                    None,
                )
            } else if track_candidates.len() == 1 && top.score >= 0.55 {
                (
                    "track_fallback".to_string(),
                    None,
                    track_candidates.first().cloned(),
                    round_score(top.score),
                    "track".to_string(),
                    None,
                )
            } else {
                (
                    "unresolved".to_string(),
                    None,
                    None,
                    round_score(top.score),
                    "unresolved".to_string(),
                    Some("ambiguous_candidate".to_string()),
                )
            };

        let resolution_trace = TraceEntry {
            stage: "entity_resolution".to_string(),
            decision: method.clone(),
            detail: json!({
                "resolved_game_id": resolved_game_id,
                "resolved_track_id": resolved_track_id,
                "track_candidates": track_candidates,
                "confidence": round_score(confidence),
                "gap": round_score(gap.max(0.0)),
                "scope": scope,
                "reason": unresolved_reason,
            }),
        };

        ResolutionOutcome {
            candidates,
            track_candidates,
            resolved_game_id,
            resolved_track_id,
            method,
            confidence,
            scope,
            unresolved_reason,
            candidate_trace,
            resolution_trace,
        }
    }

    fn attribute_event(&self, clean: &CleanItem) -> EventOutcome {
        let haystack = format!(
            "{} {}",
            clean.normalized_title.to_lowercase(),
            clean.normalized_text.to_lowercase()
        );
        let mut best_event: Option<(String, Vec<String>)> = None;
        for (event_type, keywords) in &self.assets.event_rules {
            let mut hits = keywords
                .iter()
                .filter(|keyword| haystack.contains(keyword.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            hits.sort();
            hits.dedup();
            if hits.is_empty() {
                continue;
            }
            let should_replace = best_event
                .as_ref()
                .map(|(current_event, current_hits)| {
                    hits.len() > current_hits.len()
                        || (hits.len() == current_hits.len()
                            && event_priority(event_type) < event_priority(current_event))
                        || (hits.len() == current_hits.len()
                            && event_priority(event_type) == event_priority(current_event)
                            && event_type < current_event)
                })
                .unwrap_or(true);
            if should_replace {
                best_event = Some((event_type.clone(), hits));
            }
        }

        let (event_type, keywords) = if let Some((event_type, hits)) = best_event {
            (Some(event_type), hits)
        } else {
            (None, Vec::new())
        };

        EventOutcome {
            trace: TraceEntry {
                stage: "event_attribution".to_string(),
                decision: event_type.clone().unwrap_or_else(|| "no_event".to_string()),
                detail: json!({
                    "event_type": event_type,
                    "keywords": keywords,
                }),
            },
            event_type,
            keywords,
        }
    }

    fn build_metrics(&self, signals: &[SourceSignal]) -> serde_json::Value {
        let total = signals.len();
        let unresolved_count = signals
            .iter()
            .filter(|signal| signal.state == "unresolved")
            .count();
        let duplicate_count = signals
            .iter()
            .filter(|signal| signal.duplicate_type != "none")
            .count();

        let mut high = 0_usize;
        let mut medium = 0_usize;
        let mut low = 0_usize;
        let mut signals_per_game: HashMap<String, usize> = HashMap::new();
        let mut signals_per_track: HashMap<String, usize> = HashMap::new();
        let mut platform_counts: HashMap<String, usize> = HashMap::new();

        for signal in signals {
            if signal.entity_resolution_confidence >= 0.85 {
                high += 1;
            } else if signal.entity_resolution_confidence >= 0.65 {
                medium += 1;
            } else {
                low += 1;
            }
            if let Some(game_id) = &signal.resolved_game_id {
                *signals_per_game.entry(game_id.clone()).or_insert(0) += 1;
            }
            if let Some(track_id) = &signal.track_id {
                *signals_per_track.entry(track_id.clone()).or_insert(0) += 1;
            }
            *platform_counts.entry(signal.platform.clone()).or_insert(0) += 1;
        }

        let platform_contribution_ratio = platform_counts
            .iter()
            .map(|(platform, count)| (platform.clone(), json_ratio(*count, total)))
            .collect::<serde_json::Map<_, _>>();

        json!({
            "processed_items": total,
            "unresolved_ratio": json_ratio(unresolved_count, total),
            "duplicate_ratio": json_ratio(duplicate_count, total),
            "resolution_confidence_distribution": {
                "high": high,
                "medium": medium,
                "low": low,
            },
            "signals_per_game": signals_per_game,
            "signals_per_track": signals_per_track,
            "platform_contribution_ratio": platform_contribution_ratio,
        })
    }
}

fn add_match_for_lookup<F>(add_match: &mut F, lookup: &AliasLookup, alias: &str, in_title: bool)
where
    F: FnMut(&str, &str, &str, &str, f64),
{
    let mut score: f64 = match (lookup.strength.as_str(), in_title) {
        ("strong", true) => 0.88_f64,
        ("strong", false) => 0.78_f64,
        (_, true) => 0.70_f64,
        (_, false) => 0.58_f64,
    };
    if lookup.source == "standard_name" {
        score += 0.05;
    }
    add_match(
        &lookup.game_id,
        alias,
        &lookup.strength,
        &lookup.source,
        score.min(0.99),
    );
}

/// Domain-aware URL seed matching.  Replaces the old `contains(seed)` approach
/// that would match "com" against every `.com` URL.
///
/// Matching rules:
/// 1. Exact host match:  seed == host
/// 2. Sub-domain match:  host ends with ".{seed}"
/// 3. Host+path match:   "{host}{path}" contains seed  (for path-style seeds like "eggy-party")
fn url_seed_matches(normalized_url: &str, seed: &str) -> bool {
    let seed_lower = seed.to_lowercase();
    let Ok(parsed) = Url::parse(normalized_url) else {
        return false;
    };
    let Some(host) = parsed.host_str() else {
        return false;
    };
    let host_lower = host.to_lowercase();

    // 1. Exact host match
    if host_lower == seed_lower {
        return true;
    }
    // 2. Sub-domain match: "sub.pvp.qq.com" matches seed "pvp.qq.com"
    if host_lower.ends_with(&format!(".{seed_lower}")) {
        return true;
    }
    // 3. Host+path match for non-domain/path-style seeds (e.g. "eggy-party")
    if seed_lower.contains('.') {
        return false;
    }
    let full = format!("{}{}", host_lower, parsed.path());
    full.contains(&seed_lower)
}

fn unique_tracks(assets: &AssetIndex, candidates: &[String]) -> Vec<String> {
    let mut tracks = BTreeSet::new();
    for game_id in candidates {
        if let Some(track_id) = assets.track_for_game(game_id) {
            tracks.insert(track_id);
        }
    }
    tracks.into_iter().collect()
}

fn event_priority(event_type: &str) -> usize {
    match event_type {
        "version" => 0,
        "season" => 1,
        "launch" => 2,
        "test" => 3,
        "collab" => 4,
        "activity" => 5,
        "controversy" => 6,
        _ => 9,
    }
}

fn round_score(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}
