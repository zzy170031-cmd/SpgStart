use crate::models::{
    GalaxyEdge, GalaxyEventListItem, GalaxyFocusView, GalaxyGameListItem, GalaxyListResponse,
    GalaxyNode, GalaxySummaryView,
};
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

#[derive(Clone)]
pub struct GalaxyService {
    sqlite: SqliteGalaxyRepository,
    mock: MockGalaxyRepository,
}

impl GalaxyService {
    pub fn new(core_db_path: PathBuf) -> Self {
        Self {
            sqlite: SqliteGalaxyRepository::new(core_db_path),
            mock: MockGalaxyRepository::new(),
        }
    }

    pub fn summary(&self) -> Result<GalaxySummaryView> {
        if let Some(summary) = self.sqlite.summary()? {
            return Ok(summary);
        }
        Ok(self.mock.summary())
    }

    pub fn list(&self, tab: &str) -> Result<GalaxyListResponse> {
        if let Some(response) = self.sqlite.list(tab)? {
            return Ok(response);
        }
        Ok(self.mock.list(tab))
    }

    pub fn focus_game(&self, game_id: &str) -> Result<Option<GalaxyFocusView>> {
        if let Some(focus) = self.sqlite.focus_game(game_id)? {
            return Ok(Some(focus));
        }
        Ok(self.mock.focus_game(game_id))
    }

    pub fn focus_event(&self, event_id: &str) -> Result<Option<GalaxyFocusView>> {
        if let Some(focus) = self.sqlite.focus_event(event_id)? {
            return Ok(Some(focus));
        }
        Ok(self.mock.focus_event(event_id))
    }
}

#[derive(Clone)]
struct SqliteGalaxyRepository {
    db_path: PathBuf,
    asset_names: HashMap<String, String>,
}

impl SqliteGalaxyRepository {
    fn new(db_path: PathBuf) -> Self {
        let asset_names = spg_core::load_asset_bundle(&spg_core::default_asset_path())
            .map(|bundle| {
                bundle
                    .games
                    .into_iter()
                    .map(|game| (game.game_id, game.standard_name))
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();
        Self {
            db_path,
            asset_names,
        }
    }

    fn summary(&self) -> Result<Option<GalaxySummaryView>> {
        let Some(conn) = self.connect()? else {
            return Ok(None);
        };

        let counts = conn
            .query_row(
                "
                SELECT
                    COUNT(DISTINCT resolved_game_id),
                    COUNT(DISTINCT event_type),
                    COUNT(DISTINCT platform),
                    COUNT(DISTINCT track_id)
                FROM resolved_signals
                WHERE COALESCE(track_id, '') = 'slg'
                ",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .optional()?;

        let Some((game_count, event_count, source_count, track_count)) = counts else {
            return Ok(None);
        };

        if game_count == 0 && event_count == 0 {
            return Ok(None);
        }

        Ok(Some(GalaxySummaryView {
            headline: "SLG 赛道星图".to_string(),
            subheadline: "当前视图优先展示 SQLite 中已归并的 SLG 信号。".to_string(),
            game_count,
            event_count,
            source_count,
            track_count,
            last_updated: utc_now(),
        }))
    }

    fn list(&self, tab: &str) -> Result<Option<GalaxyListResponse>> {
        let Some(conn) = self.connect()? else {
            return Ok(None);
        };

        match tab {
            "events" => {
                let mut statement = conn.prepare(
                    "
                    SELECT
                        COALESCE(event_type, 'general') AS event_type,
                        COALESCE(resolved_game_id, '') AS game_id,
                        COUNT(*) AS heat,
                        COUNT(DISTINCT platform) AS source_count,
                        COALESCE(MAX(title), '未命名事件') AS title
                    FROM resolved_signals
                    WHERE COALESCE(track_id, '') = 'slg'
                    GROUP BY event_type, resolved_game_id
                    ORDER BY heat DESC, title ASC
                    ",
                )?;
                let rows = statement.query_map([], |row| {
                    let game_id: String = row.get(1)?;
                    let game_name = self
                        .asset_names
                        .get(&game_id)
                        .cloned()
                        .unwrap_or_else(|| game_id.clone());
                    Ok(GalaxyEventListItem {
                        id: format!(
                            "sqlite-event-{}-{}",
                            row.get::<_, String>(0)?,
                            if game_id.is_empty() { "none" } else { &game_id }
                        ),
                        title: row.get(4)?,
                        event_type: row.get(0)?,
                        game_id,
                        game_name,
                        heat: row.get(2)?,
                        source_count: row.get(3)?,
                        note: "来自当前 SQLite 快照".to_string(),
                    })
                })?;
                let events = rows.collect::<std::result::Result<Vec<_>, _>>()?;
                if events.is_empty() {
                    return Ok(None);
                }
                Ok(Some(GalaxyListResponse {
                    tab: "events".to_string(),
                    games: Vec::new(),
                    events,
                }))
            }
            _ => {
                let mut statement = conn.prepare(
                    "
                    SELECT
                        resolved_game_id,
                        COUNT(*) AS signal_count,
                        COALESCE(MAX(track_id), 'slg') AS track_id
                    FROM resolved_signals
                    WHERE resolved_game_id IS NOT NULL AND COALESCE(track_id, '') = 'slg'
                    GROUP BY resolved_game_id
                    ORDER BY signal_count DESC, resolved_game_id ASC
                    ",
                )?;
                let rows = statement.query_map([], |row| {
                    let game_id: String = row.get(0)?;
                    let name = self
                        .asset_names
                        .get(&game_id)
                        .cloned()
                        .unwrap_or_else(|| game_id.clone());
                    Ok(GalaxyGameListItem {
                        id: game_id,
                        name,
                        studio: "SQLite 聚合".to_string(),
                        stage: "实时快照".to_string(),
                        aliases: vec![row.get::<_, String>(2)?],
                        official_url: String::new(),
                        signal_count: row.get(1)?,
                        note: "来自当前 SQLite 快照".to_string(),
                    })
                })?;
                let games = rows.collect::<std::result::Result<Vec<_>, _>>()?;
                if games.is_empty() {
                    return Ok(None);
                }
                Ok(Some(GalaxyListResponse {
                    tab: "games".to_string(),
                    games,
                    events: Vec::new(),
                }))
            }
        }
    }

    fn focus_game(&self, game_id: &str) -> Result<Option<GalaxyFocusView>> {
        let Some(conn) = self.connect()? else {
            return Ok(None);
        };

        let signal_count: Option<i64> = conn
            .query_row(
                "
                SELECT COUNT(*)
                FROM resolved_signals
                WHERE resolved_game_id = ?1 AND COALESCE(track_id, '') = 'slg'
                ",
                [game_id],
                |row| row.get(0),
            )
            .optional()?;

        if signal_count.unwrap_or(0) == 0 {
            return Ok(None);
        }

        let name = self
            .asset_names
            .get(game_id)
            .cloned()
            .unwrap_or_else(|| game_id.to_string());

        let event_nodes = query_event_nodes(&conn, game_id)?;
        let source_nodes = query_source_nodes(&conn, game_id)?;

        let mut nodes = vec![GalaxyNode {
            id: format!("game::{game_id}"),
            label: name.clone(),
            node_type: "game".to_string(),
            orbit: 0,
            angle: 0.0,
            size: 24.0,
            accent: "#2d79ff".to_string(),
            detail: "当前聚焦游戏".to_string(),
        }];
        let mut edges = Vec::new();

        for (index, (label, count)) in event_nodes.iter().enumerate() {
            let node_id = format!("event::{game_id}::{label}");
            nodes.push(GalaxyNode {
                id: node_id.clone(),
                label: label.clone(),
                node_type: "event".to_string(),
                orbit: 1,
                angle: 60.0 * index as f64,
                size: 12.0 + (*count as f64).min(16.0),
                accent: "#5fa8ff".to_string(),
                detail: format!("{count} 条相关信号"),
            });
            edges.push(GalaxyEdge {
                source: format!("game::{game_id}"),
                target: node_id,
                edge_type: "related_event".to_string(),
                strength: (*count as f64 / 10.0).clamp(0.3, 1.0),
            });
        }

        for (index, (label, count)) in source_nodes.iter().enumerate() {
            let node_id = format!("source::{game_id}::{label}");
            nodes.push(GalaxyNode {
                id: node_id.clone(),
                label: label.clone(),
                node_type: "source".to_string(),
                orbit: 2,
                angle: 72.0 * index as f64,
                size: 10.0 + (*count as f64).min(14.0),
                accent: "#7ab8ff".to_string(),
                detail: format!("{count} 个来源节点"),
            });
            edges.push(GalaxyEdge {
                source: format!("game::{game_id}"),
                target: node_id,
                edge_type: "source_link".to_string(),
                strength: (*count as f64 / 8.0).clamp(0.25, 1.0),
            });
        }

        Ok(Some(GalaxyFocusView {
            focus_id: game_id.to_string(),
            focus_kind: "game".to_string(),
            title: name,
            subtitle: "来源、事件和信号围绕当前实体展开".to_string(),
            narrative: vec![
                "主星体表示当前聚焦的 SLG 游戏实体。".to_string(),
                "第一层轨道展示主要事件，第二层轨道展示高频来源。".to_string(),
                "当前数据来自 SQLite 快照，因此会随着新入库结果自动刷新。".to_string(),
            ],
            nodes,
            edges,
            available_layers: vec![
                "games".to_string(),
                "events".to_string(),
                "sources".to_string(),
            ],
        }))
    }

    fn focus_event(&self, _event_id: &str) -> Result<Option<GalaxyFocusView>> {
        Ok(None)
    }

    fn connect(&self) -> Result<Option<Connection>> {
        if !self.db_path.exists() {
            return Ok(None);
        }
        Ok(Some(Connection::open(&self.db_path)?))
    }
}

fn query_event_nodes(conn: &Connection, game_id: &str) -> Result<Vec<(String, i64)>> {
    let mut statement = conn.prepare(
        "
        SELECT COALESCE(event_type, 'general') AS event_type, COUNT(*) AS count
        FROM resolved_signals
        WHERE resolved_game_id = ?1 AND COALESCE(track_id, '') = 'slg'
        GROUP BY event_type
        ORDER BY count DESC, event_type ASC
        LIMIT 5
        ",
    )?;
    let rows = statement.query_map(params![game_id], |row| Ok((row.get(0)?, row.get(1)?)))?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

fn query_source_nodes(conn: &Connection, game_id: &str) -> Result<Vec<(String, i64)>> {
    let mut statement = conn.prepare(
        "
        SELECT platform, COUNT(*) AS count
        FROM resolved_signals
        WHERE resolved_game_id = ?1 AND COALESCE(track_id, '') = 'slg'
        GROUP BY platform
        ORDER BY count DESC, platform ASC
        LIMIT 6
        ",
    )?;
    let rows = statement.query_map(params![game_id], |row| Ok((row.get(0)?, row.get(1)?)))?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

#[derive(Clone)]
struct MockGalaxyRepository {
    games: Vec<MockGame>,
    events: Vec<MockEvent>,
}

impl MockGalaxyRepository {
    fn new() -> Self {
        Self {
            games: vec![
                MockGame::new(
                    "rate-land",
                    "率土之滨",
                    "网易",
                    "核心标的",
                    &["率土", "RSZ"],
                    "https://stzb.163.com",
                    128,
                    "赛季节奏长期稳定，适合作为 SLG 中枢样本。",
                ),
                MockGame::new(
                    "sango-slg",
                    "三国志·战略版",
                    "灵犀互娱",
                    "核心标的",
                    &["三战", "三国志战略版", "SZ"],
                    "https://sango.xd.com",
                    141,
                    "内容讨论密度高，适合观察版本与联盟战热点。",
                ),
                MockGame::new(
                    "rok",
                    "万国觉醒",
                    "莉莉丝",
                    "核心标的",
                    &["ROK", "万国"],
                    "https://riseofkingdoms.com",
                    97,
                    "全球化内容多，转载链路明显。",
                ),
                MockGame::new(
                    "civ",
                    "文明与征服",
                    "点点互动",
                    "跟踪标的",
                    &["文明", "CivC"],
                    "https://www.wmyzf.com",
                    66,
                    "题材特殊，适合做长尾跟踪。",
                ),
                MockGame::new(
                    "aoe",
                    "重返帝国",
                    "腾讯",
                    "跟踪标的",
                    &["重返", "AOE Mobile"],
                    "https://zfdg.qq.com",
                    72,
                    "版本和商业投放经常同步抬升。",
                ),
                MockGame::new(
                    "hongtu",
                    "鸿图之下",
                    "祖龙娱乐",
                    "跟踪标的",
                    &["鸿图"],
                    "https://htzx.qq.com",
                    51,
                    "话题集中在玩法和赛季体验。",
                ),
                MockGame::new(
                    "mouding",
                    "三国：谋定天下",
                    "灵犀互娱",
                    "观察标的",
                    &["谋定天下", "谋定"],
                    "https://sgmdtx.com",
                    43,
                    "新品观察位，适合放在外层轨道。",
                ),
            ],
            events: vec![
                MockEvent::new(
                    "evt-season-shift",
                    "赛季节奏变化",
                    "version",
                    "rate-land",
                    "率土之滨",
                    88,
                    5,
                    "围绕赛季更新、武将平衡与城战节奏。",
                ),
                MockEvent::new(
                    "evt-alliance-war",
                    "联盟战与城战热度",
                    "activity",
                    "sango-slg",
                    "三国志·战略版",
                    92,
                    6,
                    "联盟冲突与版本活动一起抬升讨论。",
                ),
                MockEvent::new(
                    "evt-creator-surge",
                    "创作者内容热度",
                    "activity",
                    "rok",
                    "万国觉醒",
                    75,
                    4,
                    "搬运、解说和攻略二创明显增多。",
                ),
                MockEvent::new(
                    "evt-commercial-push",
                    "商业投放加码",
                    "launch",
                    "aoe",
                    "重返帝国",
                    61,
                    4,
                    "版本宣传与投放在同一周期放大。",
                ),
                MockEvent::new(
                    "evt-new-product-watch",
                    "新品赛道观察",
                    "test",
                    "mouding",
                    "三国：谋定天下",
                    58,
                    3,
                    "外层观察轨道，用来承接新品信号。",
                ),
            ],
        }
    }

    fn summary(&self) -> GalaxySummaryView {
        GalaxySummaryView {
            headline: "SLG 星河总览".to_string(),
            subheadline: "蓝白轻量主界面，左侧筛选游戏与事件，右侧星系持续缓速运动。".to_string(),
            game_count: self.games.len() as i64,
            event_count: self.events.len() as i64,
            source_count: 6,
            track_count: 1,
            last_updated: utc_now(),
        }
    }

    fn list(&self, tab: &str) -> GalaxyListResponse {
        match tab {
            "events" => GalaxyListResponse {
                tab: "events".to_string(),
                games: Vec::new(),
                events: self.events.iter().map(MockEvent::to_view).collect(),
            },
            _ => GalaxyListResponse {
                tab: "games".to_string(),
                games: self.games.iter().map(MockGame::to_view).collect(),
                events: Vec::new(),
            },
        }
    }

    fn focus_game(&self, game_id: &str) -> Option<GalaxyFocusView> {
        let game = self.games.iter().find(|game| game.id == game_id)?;
        let related_events = self
            .events
            .iter()
            .filter(|event| event.game_id == game_id)
            .collect::<Vec<_>>();
        let sources = mock_sources_for(game_id);

        let mut nodes = vec![GalaxyNode {
            id: format!("game::{game_id}"),
            label: game.name.to_string(),
            node_type: "game".to_string(),
            orbit: 0,
            angle: 0.0,
            size: 26.0,
            accent: "#2d79ff".to_string(),
            detail: game.note.to_string(),
        }];
        let mut edges = Vec::new();

        for (index, event) in related_events.iter().enumerate() {
            let node_id = format!("event::{}", event.id);
            nodes.push(GalaxyNode {
                id: node_id.clone(),
                label: event.title.to_string(),
                node_type: "event".to_string(),
                orbit: 1,
                angle: 72.0 * index as f64,
                size: 13.0 + (event.heat as f64 / 20.0),
                accent: "#5b9bff".to_string(),
                detail: event.note.to_string(),
            });
            edges.push(GalaxyEdge {
                source: format!("game::{game_id}"),
                target: node_id,
                edge_type: "event_link".to_string(),
                strength: (event.heat as f64 / 100.0).clamp(0.25, 1.0),
            });
        }

        for (index, source) in sources.iter().enumerate() {
            let node_id = format!("source::{game_id}::{index}");
            nodes.push(GalaxyNode {
                id: node_id.clone(),
                label: source.name.to_string(),
                node_type: "source".to_string(),
                orbit: 2,
                angle: 58.0 * index as f64,
                size: 11.0 + source.weight,
                accent: "#8bc2ff".to_string(),
                detail: source.note.to_string(),
            });
            edges.push(GalaxyEdge {
                source: format!("game::{game_id}"),
                target: node_id,
                edge_type: "source_link".to_string(),
                strength: source.weight / 10.0,
            });
        }

        Some(GalaxyFocusView {
            focus_id: game.id.to_string(),
            focus_kind: "game".to_string(),
            title: game.name.to_string(),
            subtitle: format!("{} · {} · {}", game.stage, game.studio, game.note),
            narrative: vec![
                "核心节点承接游戏实体，周围环绕热点事件和主要来源。".to_string(),
                "拖拽可以旋转星图，滚轮可以缩放观察距离。".to_string(),
                "当前首发只组织 SLG 赛道，后续再扩多赛道总导航。".to_string(),
            ],
            nodes,
            edges,
            available_layers: vec![
                "games".to_string(),
                "events".to_string(),
                "sources".to_string(),
            ],
        })
    }

    fn focus_event(&self, event_id: &str) -> Option<GalaxyFocusView> {
        let event = self.events.iter().find(|event| event.id == event_id)?;
        let game = self.games.iter().find(|game| game.id == event.game_id)?;
        let sources = mock_sources_for(event.game_id);

        let mut nodes = vec![GalaxyNode {
            id: format!("event::{}", event.id),
            label: event.title.to_string(),
            node_type: "event".to_string(),
            orbit: 0,
            angle: 0.0,
            size: 24.0,
            accent: "#2d79ff".to_string(),
            detail: event.note.to_string(),
        }];
        let mut edges = Vec::new();

        nodes.push(GalaxyNode {
            id: format!("game::{}", game.id),
            label: game.name.to_string(),
            node_type: "game".to_string(),
            orbit: 1,
            angle: 20.0,
            size: 18.0,
            accent: "#5b9bff".to_string(),
            detail: game.note.to_string(),
        });
        edges.push(GalaxyEdge {
            source: format!("event::{}", event.id),
            target: format!("game::{}", game.id),
            edge_type: "game_link".to_string(),
            strength: (event.heat as f64 / 100.0).clamp(0.3, 1.0),
        });

        for (index, source) in sources.iter().enumerate() {
            let node_id = format!("source::{}::{}", event.id, index);
            nodes.push(GalaxyNode {
                id: node_id.clone(),
                label: source.name.to_string(),
                node_type: "source".to_string(),
                orbit: 2,
                angle: 60.0 * index as f64,
                size: 10.0 + source.weight,
                accent: "#8bc2ff".to_string(),
                detail: source.note.to_string(),
            });
            edges.push(GalaxyEdge {
                source: format!("event::{}", event.id),
                target: node_id,
                edge_type: "source_link".to_string(),
                strength: source.weight / 10.0,
            });
        }

        Some(GalaxyFocusView {
            focus_id: event.id.to_string(),
            focus_kind: "event".to_string(),
            title: event.title.to_string(),
            subtitle: format!(
                "{} · {} · 热度 {}",
                event.game_name, event.event_type, event.heat
            ),
            narrative: vec![
                "事件视图把热点放到中心，帮助你反向追到游戏和主要来源。".to_string(),
                "不同图层可以单独开关，便于只看游戏、事件或来源。".to_string(),
                "星图不是静态装饰，而是围绕焦点保持缓速运动。".to_string(),
            ],
            nodes,
            edges,
            available_layers: vec![
                "games".to_string(),
                "events".to_string(),
                "sources".to_string(),
            ],
        })
    }
}

#[derive(Clone)]
struct MockGame {
    id: &'static str,
    name: &'static str,
    studio: &'static str,
    stage: &'static str,
    aliases: &'static [&'static str],
    official_url: &'static str,
    signal_count: i64,
    note: &'static str,
}

impl MockGame {
    fn new(
        id: &'static str,
        name: &'static str,
        studio: &'static str,
        stage: &'static str,
        aliases: &'static [&'static str],
        official_url: &'static str,
        signal_count: i64,
        note: &'static str,
    ) -> Self {
        Self {
            id,
            name,
            studio,
            stage,
            aliases,
            official_url,
            signal_count,
            note,
        }
    }

    fn to_view(&self) -> GalaxyGameListItem {
        GalaxyGameListItem {
            id: self.id.to_string(),
            name: self.name.to_string(),
            studio: self.studio.to_string(),
            stage: self.stage.to_string(),
            aliases: self
                .aliases
                .iter()
                .map(|alias| (*alias).to_string())
                .collect(),
            official_url: self.official_url.to_string(),
            signal_count: self.signal_count,
            note: self.note.to_string(),
        }
    }
}

#[derive(Clone)]
struct MockEvent {
    id: &'static str,
    title: &'static str,
    event_type: &'static str,
    game_id: &'static str,
    game_name: &'static str,
    heat: i64,
    source_count: i64,
    note: &'static str,
}

impl MockEvent {
    fn new(
        id: &'static str,
        title: &'static str,
        event_type: &'static str,
        game_id: &'static str,
        game_name: &'static str,
        heat: i64,
        source_count: i64,
        note: &'static str,
    ) -> Self {
        Self {
            id,
            title,
            event_type,
            game_id,
            game_name,
            heat,
            source_count,
            note,
        }
    }

    fn to_view(&self) -> GalaxyEventListItem {
        GalaxyEventListItem {
            id: self.id.to_string(),
            title: self.title.to_string(),
            event_type: self.event_type.to_string(),
            game_id: self.game_id.to_string(),
            game_name: self.game_name.to_string(),
            heat: self.heat,
            source_count: self.source_count,
            note: self.note.to_string(),
        }
    }
}

struct MockSource {
    name: &'static str,
    weight: f64,
    note: &'static str,
}

fn mock_sources_for(game_id: &str) -> Vec<MockSource> {
    match game_id {
        "rate-land" => vec![
            MockSource {
                name: "官方公告",
                weight: 5.0,
                note: "高置信官方源",
            },
            MockSource {
                name: "Bilibili",
                weight: 4.0,
                note: "长视频攻略与战报",
            },
            MockSource {
                name: "抖音",
                weight: 3.0,
                note: "短视频高频搬运",
            },
            MockSource {
                name: "微博",
                weight: 2.5,
                note: "节点扩散与二次传播",
            },
        ],
        "sango-slg" => vec![
            MockSource {
                name: "官方账号",
                weight: 5.0,
                note: "版本与活动公告",
            },
            MockSource {
                name: "小红书",
                weight: 3.2,
                note: "阵容分享与轻攻略",
            },
            MockSource {
                name: "Bilibili",
                weight: 4.6,
                note: "深度拆解与联盟战内容",
            },
            MockSource {
                name: "TapTap",
                weight: 2.8,
                note: "用户体验反馈",
            },
        ],
        "rok" => vec![
            MockSource {
                name: "YouTube 搬运",
                weight: 3.6,
                note: "国际内容二次传播",
            },
            MockSource {
                name: "微博",
                weight: 2.6,
                note: "热点扩散",
            },
            MockSource {
                name: "Bilibili",
                weight: 3.9,
                note: "长视频战报",
            },
            MockSource {
                name: "官方公告",
                weight: 4.5,
                note: "版本源头",
            },
        ],
        _ => vec![
            MockSource {
                name: "官方",
                weight: 4.3,
                note: "官方公告与活动页",
            },
            MockSource {
                name: "Bilibili",
                weight: 3.4,
                note: "创作者内容",
            },
            MockSource {
                name: "抖音",
                weight: 2.9,
                note: "轻量热视频",
            },
        ],
    }
}

fn utc_now() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .expect("current utc time")
}

#[allow(dead_code)]
pub fn core_db_path(workspace_root: &Path) -> PathBuf {
    workspace_root.join("data").join("spg_input_layer.db")
}
