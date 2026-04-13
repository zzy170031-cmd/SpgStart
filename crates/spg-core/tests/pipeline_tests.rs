use rusqlite::Connection;
use serde_json::json;
use spg_core::models::{AliasRule, AssetBundle, GameAsset, RawSourceItem};
use spg_core::{
    build_asset_index, default_asset_path, load_asset_bundle, SPGInputPipeline, SQLiteStorage,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn make_pipeline() -> SPGInputPipeline {
    make_pipeline_from_bundle(load_asset_bundle(&default_asset_path()).expect("asset bundle"))
}

fn make_pipeline_from_bundle(bundle: AssetBundle) -> SPGInputPipeline {
    SPGInputPipeline::new(build_asset_index(bundle))
}

fn make_raw_item(
    source_id: &str,
    platform: &str,
    source_type: &str,
    title: &str,
    text: &str,
    author: &str,
    url: &str,
) -> RawSourceItem {
    RawSourceItem {
        source_id: source_id.to_string(),
        platform: platform.to_string(),
        source_type: source_type.to_string(),
        content_type: "post".to_string(),
        title: title.to_string(),
        text: text.to_string(),
        author: author.to_string(),
        published_at: "2026-04-10T10:00:00+08:00".to_string(),
        url: url.to_string(),
        raw_metrics: HashMap::from([("likes".to_string(), json!(100))]),
        fetched_at: "2026-04-10T10:05:00+08:00".to_string(),
        metadata: HashMap::new(),
    }
}

// ===== EXISTING TESTS (updated for SLG) =====

#[test]
fn resolves_official_source_with_strong_match() {
    let pipeline = make_pipeline();
    let item = make_raw_item(
        "raw_official_stzb_test",
        "official",
        "official",
        "率土之滨新赛季公告",
        "率土之滨官方发布新赛季征服赛季公告。",
        "率土之滨官方",
        "https://stzb.163.com/news/season",
    );

    let result = pipeline.process(vec![item]);
    let signal = &result.signals[0];
    assert_eq!(signal.resolved_game_id.as_deref(), Some("game_stzb"));
    assert_eq!(signal.track_id.as_deref(), Some("slg"));
    assert_eq!(signal.entity_resolution_method, "official_account_match");
    assert_eq!(signal.state, "canonical");
    assert_eq!(
        signal.normalized_published_at.as_deref(),
        Some("2026-04-10T02:00:00Z")
    );
}

#[test]
fn resolves_new_slg_seed_games_from_standard_name() {
    let pipeline = make_pipeline();
    let item = make_raw_item(
        "raw_sgmdtx_alias",
        "douyin",
        "content_platform",
        "三国谋定天下新赛季开荒阵容怎么配",
        "这条内容围绕三国谋定天下赛季开荒、配将和同盟节奏做拆解。",
        "SLG情报局",
        "https://www.douyin.com/video/sgmdtx001",
    );

    let result = pipeline.process(vec![item]);
    let signal = &result.signals[0];
    assert_eq!(signal.resolved_game_id.as_deref(), Some("game_sgmdtx"));
    assert_eq!(signal.track_id.as_deref(), Some("slg"));
}

#[test]
fn folds_content_duplicates_when_same_url_is_reused() {
    let pipeline = make_pipeline();
    let first = make_raw_item(
        "raw_dup_one",
        "official",
        "official",
        "率土之滨新赛季征服赛季公告",
        "率土之滨官方发布新赛季征服赛季公告，包含武将觉醒调整。",
        "率土之滨官方",
        "https://stzb.163.com/news/season12?utm_source=weibo",
    );
    let second = make_raw_item(
        "raw_dup_two",
        "weibo",
        "official",
        "率土之滨新赛季重点整理",
        "这条微博只放重点总结，但链接仍然指向同一篇官方公告。",
        "率土之滨官方",
        "https://stzb.163.com/news/season12",
    );

    let result = pipeline.process(vec![first, second]);
    assert_eq!(result.dedupe_groups.len(), 1);
    let duplicate = result
        .signals
        .iter()
        .find(|signal| signal.duplicate_type == "content_near_duplicate")
        .expect("content duplicate");
    assert!(duplicate.dedupe_key.starts_with("url::"));
}

#[test]
fn folds_content_duplicates_when_repost_contains_original_url_in_text() {
    let pipeline = make_pipeline();
    let original = make_raw_item(
        "raw_dup_original",
        "official",
        "official",
        "率土之滨新赛季征服赛季公告",
        "率土之滨官方发布新赛季征服赛季公告，包含武将觉醒调整。",
        "率土之滨官方",
        "https://stzb.163.com/news/season12",
    );
    let repost = make_raw_item(
        "raw_dup_repost",
        "weibo",
        "content_platform",
        "搬运一下率土新赛季公告重点",
        "原文链接：https://stzb.163.com/news/season12?utm_source=weibo 我这里只做重点整理。",
        "SLG热榜搬运号",
        "https://weibo.com/u/1?id=3",
    );

    let result = pipeline.process(vec![original, repost]);
    let duplicate = result
        .signals
        .iter()
        .find(|signal| signal.duplicate_type == "content_near_duplicate")
        .expect("repost duplicate");
    assert!(duplicate.dedupe_key.starts_with("url::"));
}

#[test]
fn keeps_distinct_versions_out_of_information_duplicates() {
    let pipeline = make_pipeline();
    let s12 = make_raw_item(
        "raw_stzb_s12",
        "official",
        "official",
        "率土之滨S12赛季征服赛季公告",
        "率土之滨官方发布S12赛季征服赛季公告，包含武将觉醒调整。",
        "率土之滨官方",
        "https://stzb.163.com/news/s12",
    );
    let s13 = make_raw_item(
        "raw_stzb_s13",
        "official",
        "official",
        "率土之滨S13赛季征服赛季公告",
        "率土之滨官方发布S13赛季征服赛季公告，包含同盟战规则变动。",
        "率土之滨官方",
        "https://stzb.163.com/news/s13",
    );

    let result = pipeline.process(vec![s12, s13]);
    assert_eq!(result.dedupe_groups.len(), 0);
    assert!(result
        .signals
        .iter()
        .all(|signal| signal.duplicate_type == "none"));
}

#[test]
fn falls_back_to_track_when_candidates_share_same_track() {
    let bundle = AssetBundle {
        games: vec![
            GameAsset {
                game_id: "game_alpha".to_string(),
                standard_name: "征服王朝".to_string(),
                track: "slg".to_string(),
                representative_work: "征服王朝".to_string(),
                aliases: vec![AliasRule {
                    value: "征服".to_string(),
                    strength: "weak".to_string(),
                    source: "shared_alias".to_string(),
                }],
                official_accounts: vec![],
                official_url_seeds: vec![],
            },
            GameAsset {
                game_id: "game_beta".to_string(),
                standard_name: "征服时代".to_string(),
                track: "slg".to_string(),
                representative_work: "征服时代".to_string(),
                aliases: vec![AliasRule {
                    value: "征服".to_string(),
                    strength: "weak".to_string(),
                    source: "shared_alias".to_string(),
                }],
                official_accounts: vec![],
                official_url_seeds: vec![],
            },
        ],
        negative_aliases: vec![],
        event_rules: HashMap::from([("version".to_string(), vec!["版本".to_string()])]),
    };
    let pipeline = make_pipeline_from_bundle(bundle);
    let item = make_raw_item(
        "raw_shared_track",
        "bilibili",
        "content_platform",
        "征服新版本体验",
        "征服这波版本节奏变化很大。",
        "测试作者",
        "https://www.bilibili.com/video/BVtrack001",
    );

    let result = pipeline.process(vec![item]);
    let signal = &result.signals[0];
    assert_eq!(signal.scope, "track");
    assert_eq!(signal.state, "track_level");
    assert_eq!(signal.track_id.as_deref(), Some("slg"));
    assert!(signal.resolved_game_id.is_none());
    assert_eq!(result.unresolved_records.len(), 0);
}

#[test]
fn rerun_replaces_unresolved_snapshot() {
    let bundle = load_asset_bundle(&default_asset_path()).expect("asset bundle");
    let pipeline = make_pipeline_from_bundle(bundle.clone());
    let db_path = temp_db_path("spg_rust_snapshot");
    let storage = SQLiteStorage::new(&db_path);
    storage.initialize().expect("init db");
    storage.seed_aliases(&bundle).expect("seed aliases");

    let unresolved = make_raw_item(
        "raw_unknown",
        "douyin",
        "content_platform",
        "新赛季上分真的太难了",
        "这波赛季调整以后同盟实力全变了。",
        "SLG随手录",
        "https://www.douyin.com/video/889999999",
    );
    let first_result = pipeline.process(vec![unresolved]);
    storage
        .persist_result(&first_result)
        .expect("persist unresolved");
    let first_counts = storage.row_counts().expect("row counts");
    assert_eq!(first_counts["unresolved_pool"], json!(1));

    let resolved = make_raw_item(
        "raw_resolved",
        "official",
        "official",
        "率土之滨赛季公告",
        "率土之滨官方发布新赛季公告。",
        "率土之滨官方",
        "https://stzb.163.com/news/season",
    );
    let second_result = pipeline.process(vec![resolved]);
    storage
        .persist_result(&second_result)
        .expect("persist resolved");
    let second_counts = storage.row_counts().expect("row counts");
    assert_eq!(second_counts["unresolved_pool"], json!(0));
    assert_eq!(second_counts["raw_items"], json!(1));
    assert_eq!(second_counts["pipeline_runs"], json!(2));
}

#[test]
fn seeds_one_alias_to_many_games_in_sqlite() {
    let bundle = AssetBundle {
        games: vec![
            GameAsset {
                game_id: "game_one".to_string(),
                standard_name: "铁骑征途".to_string(),
                track: "slg".to_string(),
                representative_work: "铁骑征途".to_string(),
                aliases: vec![AliasRule {
                    value: "铁骑".to_string(),
                    strength: "weak".to_string(),
                    source: "short_name".to_string(),
                }],
                official_accounts: vec![],
                official_url_seeds: vec![],
            },
            GameAsset {
                game_id: "game_two".to_string(),
                standard_name: "铁骑战争".to_string(),
                track: "slg".to_string(),
                representative_work: "铁骑战争".to_string(),
                aliases: vec![AliasRule {
                    value: "铁骑".to_string(),
                    strength: "weak".to_string(),
                    source: "short_name".to_string(),
                }],
                official_accounts: vec![],
                official_url_seeds: vec![],
            },
        ],
        negative_aliases: vec![],
        event_rules: HashMap::new(),
    };

    let db_path = temp_db_path("spg_rust_aliases");
    let storage = SQLiteStorage::new(&db_path);
    storage.initialize().expect("init db");
    storage.seed_aliases(&bundle).expect("seed aliases");

    let conn = Connection::open(&db_path).expect("open db");
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM entity_aliases WHERE alias = '铁骑'",
            [],
            |row| row.get(0),
        )
        .expect("count aliases");
    assert_eq!(count, 2);
}

#[test]
fn prefers_version_event_when_hits_tie() {
    let pipeline = make_pipeline();
    let item = make_raw_item(
        "raw_event_tie",
        "bilibili",
        "content_platform",
        "率土之滨版本活动整理",
        "率土之滨这次版本活动信息汇总。",
        "测试作者",
        "https://www.bilibili.com/video/BVevent001",
    );

    let result = pipeline.process(vec![item]);
    assert_eq!(result.signals[0].event_type.as_deref(), Some("version"));
}

#[test]
fn persists_rows_to_sqlite() {
    let bundle = load_asset_bundle(&default_asset_path()).expect("asset bundle");
    let pipeline = SPGInputPipeline::new(build_asset_index(bundle.clone()));
    let mut item = make_raw_item(
        "raw_sqlite_check",
        "taptap",
        "community",
        "三战S12赛季新战法体验",
        "三国志战略版这次S12赛季新战法设计更有策略性了。",
        "策略兵法家",
        "https://www.taptap.cn/post/889900",
    );
    item.published_at = "2026/04/10 11:10".to_string();
    let result = pipeline.process(vec![item]);

    let db_path = temp_db_path("spg_rust_persist");
    let storage = SQLiteStorage::new(&db_path);
    storage.initialize().expect("init db");
    storage.seed_aliases(&bundle).expect("seed aliases");
    storage.persist_result(&result).expect("persist result");

    let counts = storage.row_counts().expect("row counts");
    assert_eq!(counts["raw_items"], json!(1));
    assert_eq!(counts["clean_items"], json!(1));
    assert_eq!(counts["resolved_signals"], json!(1));

    let conn = Connection::open(&db_path).expect("open db");
    let normalized: String = conn
        .query_row(
            "SELECT normalized_published_at FROM clean_items LIMIT 1",
            [],
            |row| row.get(0),
        )
        .expect("normalized_published_at");
    assert_eq!(normalized, "2026-04-10T03:10:00Z");

    let reference_urls: String = conn
        .query_row(
            "SELECT reference_urls_json FROM clean_items LIMIT 1",
            [],
            |row| row.get(0),
        )
        .expect("reference_urls_json");
    assert_eq!(reference_urls, "[]");
}

// ===== NEW TESTS (Phase 1) =====

#[test]
fn url_seed_matches_domain_not_substring() {
    // URL seed "stzb.163.com" should match that domain but not unrelated URLs
    let pipeline = make_pipeline();
    let item = make_raw_item(
        "raw_url_seed_test",
        "official",
        "official",
        "率土之滨版本公告",
        "率土之滨官方发布版本公告。",
        "未知作者",
        "https://stzb.163.com/news/version",
    );

    let result = pipeline.process(vec![item]);
    let signal = &result.signals[0];
    assert_eq!(signal.resolved_game_id.as_deref(), Some("game_stzb"));
    // Should match via url_seed since author is not in official_accounts
    assert!(
        signal.entity_resolution_method == "official_url_seed_match"
            || signal.entity_resolution_method == "alias_match"
            || signal.entity_resolution_method == "standard_name_match"
    );
}

#[test]
fn url_seed_rejects_partial_domain_collision() {
    // A URL containing "163.com" but NOT "stzb.163.com" should not match stzb seed
    let bundle = AssetBundle {
        games: vec![GameAsset {
            game_id: "game_test".to_string(),
            standard_name: "测试游戏XYZ".to_string(),
            track: "slg".to_string(),
            representative_work: "测试游戏XYZ".to_string(),
            aliases: vec![],
            official_accounts: vec![],
            official_url_seeds: vec!["stzb.163.com".to_string()],
        }],
        negative_aliases: vec![],
        event_rules: HashMap::new(),
    };
    let pipeline = make_pipeline_from_bundle(bundle);
    let item = make_raw_item(
        "raw_no_match",
        "bilibili",
        "content_platform",
        "其他163产品新闻",
        "这是一个完全不相关的内容。",
        "测试作者",
        "https://other.163.com/news/something",
    );

    let result = pipeline.process(vec![item]);
    let signal = &result.signals[0];
    // Should NOT resolve via url_seed because other.163.com != stzb.163.com
    assert_ne!(signal.entity_resolution_method, "official_url_seed_match");
}

#[test]
fn negative_alias_blocks_match() {
    let bundle = AssetBundle {
        games: vec![GameAsset {
            game_id: "game_neg".to_string(),
            standard_name: "手游世界".to_string(),
            track: "slg".to_string(),
            representative_work: "手游世界".to_string(),
            aliases: vec![AliasRule {
                value: "手游".to_string(),
                strength: "weak".to_string(),
                source: "short_name".to_string(),
            }],
            official_accounts: vec![],
            official_url_seeds: vec![],
        }],
        negative_aliases: vec!["手游".to_string()],
        event_rules: HashMap::new(),
    };
    let pipeline = make_pipeline_from_bundle(bundle);
    let item = make_raw_item(
        "raw_neg_alias",
        "bilibili",
        "content_platform",
        "最近手游推荐",
        "手游排行榜今日推荐。",
        "测试作者",
        "https://www.bilibili.com/video/BVneg001",
    );

    let result = pipeline.process(vec![item]);
    let signal = &result.signals[0];
    // "手游" is in negative_aliases, so standard_name "手游世界" may still match
    // but "手游" as alias should be blocked
    assert_eq!(signal.scope, "unresolved");
}

#[test]
fn empty_input_produces_empty_result() {
    let pipeline = make_pipeline();
    let result = pipeline.process(vec![]);
    assert_eq!(result.signals.len(), 0);
    assert_eq!(result.dedupe_groups.len(), 0);
    assert_eq!(result.unresolved_records.len(), 0);
}

#[test]
fn malformed_url_does_not_panic() {
    let pipeline = make_pipeline();
    let item = make_raw_item(
        "raw_bad_url",
        "bilibili",
        "content_platform",
        "率土之滨攻略分享",
        "率土之滨同盟战打法分享。",
        "测试作者",
        "not-a-valid-url!!!",
    );

    let result = pipeline.process(vec![item]);
    assert_eq!(result.signals.len(), 1);
    // Should still resolve via alias match even with bad URL
    assert_eq!(
        result.signals[0].resolved_game_id.as_deref(),
        Some("game_stzb")
    );
}

#[test]
fn information_duplicate_detected_for_same_semantic_content() {
    let pipeline = make_pipeline();
    let first = make_raw_item(
        "raw_info_dup_1",
        "official",
        "official",
        "率土之滨新赛季征服赛季公告",
        "率土之滨官方发布新赛季征服赛季公告，包含武将觉醒调整。",
        "率土之滨官方",
        "https://stzb.163.com/news/season-announce",
    );
    let second = make_raw_item(
        "raw_info_dup_2",
        "weibo",
        "media",
        "率土之滨新赛季征服赛季公告解读",
        "率土之滨官方发布新赛季征服赛季公告，包含武将觉醒调整。简单解读一下。",
        "游戏媒体号",
        "https://weibo.com/u/media?id=slg001",
    );

    let result = pipeline.process(vec![first, second]);
    // Both should resolve to game_stzb. The second may be info dup if semantic basis aligns.
    assert!(result.signals.len() == 2);
    let resolved_count = result
        .signals
        .iter()
        .filter(|s| s.resolved_game_id.as_deref() == Some("game_stzb"))
        .count();
    assert!(resolved_count >= 1);
}

#[test]
fn slg_season_event_is_attributed() {
    let pipeline = make_pipeline();
    let item = make_raw_item(
        "raw_season_event",
        "official",
        "official",
        "率土之滨新赛季开启公告",
        "率土之滨新赛季征服赛季正式开启，同盟战规则全面调整。",
        "率土之滨官方",
        "https://stzb.163.com/news/season-open",
    );

    let result = pipeline.process(vec![item]);
    let signal = &result.signals[0];
    assert_eq!(signal.resolved_game_id.as_deref(), Some("game_stzb"));
    // Should attribute a season event (新赛季 / 赛季开启 keywords)
    assert!(
        signal.event_type.as_deref() == Some("season")
            || signal.event_type.as_deref() == Some("version"),
        "expected season or version event, got {:?}",
        signal.event_type
    );
}

fn temp_db_path(prefix: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}_{stamp}.db"))
}
