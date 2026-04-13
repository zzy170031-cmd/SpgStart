use regex::Regex;
use serde_json::{Number, Value};
use sha1::{Digest, Sha1};
use std::collections::{HashMap, HashSet};
use time::{
    format_description, format_description::well_known::Rfc3339, Date, OffsetDateTime,
    PrimitiveDateTime, Time, UtcOffset,
};
use unicode_normalization::UnicodeNormalization;
use url::Url;

const TRACKING_PARAM_PREFIXES: &[&str] = &["utm_"];
const TRACKING_PARAMS: &[&str] = &[
    "spm",
    "spm_id_from",
    "from",
    "from_source",
    "fromsharecode",
    "fromuid",
    "share_source",
    "share_medium",
    "share_app_id",
    "share_iid",
    "scene",
];

pub fn utc_now_iso() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

pub fn new_run_id(seed: &str) -> String {
    let now = OffsetDateTime::now_utc().unix_timestamp_nanos();
    format!("run_{}_{}", now, fingerprint(&[seed], 8))
}

pub fn normalize_whitespace(text: &str) -> String {
    Regex::new(r"\s+")
        .expect("whitespace regex")
        .replace_all(text, " ")
        .trim()
        .to_string()
}

pub fn normalize_text(text: &str) -> String {
    let normalized: String = text
        .nfkc()
        .collect::<String>()
        .replace('\u{200b}', " ")
        .replace('\u{feff}', " ")
        .replace('\u{3000}', " ");
    normalize_whitespace(&normalized)
}

pub fn normalize_author(author: &str) -> String {
    normalize_text(author).to_lowercase()
}

pub fn normalize_platform(platform: &str) -> String {
    match normalize_text(platform).to_lowercase().as_str() {
        "b站" | "bilibili" | "哔哩哔哩" => "bilibili".to_string(),
        "微博" | "weibo" => "weibo".to_string(),
        "抖音" | "douyin" => "douyin".to_string(),
        "taptap" | "taptap社区" => "taptap".to_string(),
        "小红书" | "xiaohongshu" => "xiaohongshu".to_string(),
        "official" | "官方" | "官方源" => "official".to_string(),
        value if !value.is_empty() => value.to_string(),
        _ => "unknown".to_string(),
    }
}

pub fn normalize_source_type(source_type: &str) -> String {
    match normalize_text(source_type).to_lowercase().as_str() {
        "official" | "official_source" | "官号" | "官方" => "official".to_string(),
        "media" | "媒体" => "media".to_string(),
        "community" | "社区" => "community".to_string(),
        "content_platform" | "内容平台" => "content_platform".to_string(),
        value if !value.is_empty() => value.to_string(),
        _ => "unknown".to_string(),
    }
}

pub fn normalize_content_type(content_type: &str) -> String {
    match normalize_text(content_type).to_lowercase().as_str() {
        "announcement" | "公告" => "announcement".to_string(),
        "post" | "帖子" => "post".to_string(),
        "video" | "视频" => "video".to_string(),
        "article" | "文章" => "article".to_string(),
        value if !value.is_empty() => value.to_string(),
        _ => "unknown".to_string(),
    }
}

pub fn infer_source_layer(source_type: &str, platform: &str) -> String {
    if source_type == "official" || platform == "official" {
        return "official".to_string();
    }
    if source_type == "media" {
        return "media".to_string();
    }
    if source_type == "community" || platform == "taptap" {
        return "community".to_string();
    }
    "content_platform".to_string()
}

pub fn normalize_url(input: &str) -> String {
    let normalized = normalize_text(input);
    let Ok(mut url) = Url::parse(&normalized) else {
        return normalized;
    };

    let mut kept_pairs = url
        .query_pairs()
        .filter_map(|(key, value)| {
            let lowered = key.to_lowercase();
            if TRACKING_PARAMS.contains(&lowered.as_str())
                || TRACKING_PARAM_PREFIXES
                    .iter()
                    .any(|prefix| lowered.starts_with(prefix))
            {
                return None;
            }
            Some((key.to_string(), value.to_string()))
        })
        .collect::<Vec<_>>();
    kept_pairs.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));

    url.set_fragment(None);
    if kept_pairs.is_empty() {
        url.set_query(None);
    } else {
        {
            let mut serializer = url.query_pairs_mut();
            serializer.clear();
            for (key, value) in &kept_pairs {
                serializer.append_pair(key, value);
            }
        }
    }

    url.to_string().trim_end_matches('/').to_string()
}

pub fn extract_reference_urls(text: &str) -> Vec<String> {
    let pattern = Regex::new(r#"https?://[^\s<>"'）)]+"#).expect("url extract regex");
    let mut results = Vec::new();
    let mut seen = HashSet::new();
    for matched in pattern.find_iter(text) {
        let normalized = normalize_url(matched.as_str());
        if !normalized.is_empty() && seen.insert(normalized.clone()) {
            results.push(normalized);
        }
    }
    results
}

pub fn normalize_timestamp(input: &str) -> Option<String> {
    let normalized = normalize_text(input);
    if normalized.is_empty() {
        return None;
    }

    if let Ok(parsed) = OffsetDateTime::parse(&normalized, &Rfc3339) {
        return Some(format_utc(parsed));
    }

    let default_offset = UtcOffset::from_hms(8, 0, 0).ok()?;
    for pattern in [
        "[year]-[month]-[day] [hour]:[minute]:[second]",
        "[year]-[month]-[day] [hour]:[minute]",
        "[year]/[month]/[day] [hour]:[minute]:[second]",
        "[year]/[month]/[day] [hour]:[minute]",
    ] {
        let Ok(description) = format_description::parse(pattern) else {
            continue;
        };
        if let Ok(parsed) = PrimitiveDateTime::parse(&normalized, &description) {
            return Some(format_utc(parsed.assume_offset(default_offset)));
        }
    }

    for pattern in ["[year]-[month]-[day]", "[year]/[month]/[day]"] {
        let Ok(description) = format_description::parse(pattern) else {
            continue;
        };
        if let Ok(parsed) = Date::parse(&normalized, &description) {
            let midnight =
                PrimitiveDateTime::new(parsed, Time::from_hms(0, 0, 0).unwrap_or(Time::MIDNIGHT));
            return Some(format_utc(midnight.assume_offset(default_offset)));
        }
    }

    None
}

pub fn normalize_metrics(metrics: &HashMap<String, Value>) -> HashMap<String, Value> {
    metrics
        .iter()
        .map(|(key, value)| {
            let normalized = match value {
                Value::String(text) => {
                    let candidate = text.trim().replace(',', "");
                    if let Ok(parsed) = candidate.parse::<i64>() {
                        Value::Number(Number::from(parsed))
                    } else if let Ok(parsed) = candidate.parse::<f64>() {
                        Number::from_f64(parsed)
                            .map(Value::Number)
                            .unwrap_or_else(|| Value::String(text.trim().to_string()))
                    } else {
                        Value::String(text.trim().to_string())
                    }
                }
                other => other.clone(),
            };
            (key.clone(), normalized)
        })
        .collect()
}

pub fn tokenize(text: &str) -> Vec<String> {
    let lowered = normalize_text(text).to_lowercase();
    Regex::new(r"[a-z0-9_]+|[\u4e00-\u9fff]{2,}")
        .expect("token regex")
        .find_iter(&lowered)
        .map(|item| item.as_str().to_string())
        .filter(|token| {
            !matches!(
                token.as_str(),
                "的" | "了" | "和" | "是" | "在" | "一个" | "这次" | "还是" | "我们" | "他们"
            )
        })
        .collect()
}

pub fn fingerprint(parts: &[&str], length: usize) -> String {
    let joined = parts
        .iter()
        .map(|part| normalize_text(part).to_lowercase())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let mut hasher = Sha1::new();
    hasher.update(joined.as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    digest.chars().take(length).collect()
}

pub fn semantic_basis(title: &str, text: &str) -> String {
    let normalized_title = normalize_text(title).to_lowercase();
    let normalized_text = normalize_text(text).to_lowercase();
    let combined = format!("{normalized_title} {normalized_text}");
    let marker_regex = Regex::new(r"[a-z]*\d+(?:\.\d+)?[a-z]*").expect("marker regex");

    let mut anchors = Vec::new();
    let mut seen = HashSet::new();

    for token in tokenize(&normalized_title) {
        if should_keep_semantic_token(&token) && seen.insert(token.clone()) {
            anchors.push(token);
        }
    }

    for marker in marker_regex.find_iter(&combined) {
        let value = marker.as_str().to_string();
        if seen.insert(value.clone()) {
            anchors.push(value);
        }
    }

    for token in tokenize(&normalized_text) {
        if should_keep_semantic_token(&token) && seen.insert(token.clone()) {
            anchors.push(token);
        }
        if anchors.len() >= 12 {
            break;
        }
    }

    if anchors.is_empty() {
        return normalize_whitespace(&combined);
    }

    anchors.truncate(12);
    anchors.join(" ")
}

pub fn json_ratio(numerator: usize, denominator: usize) -> Value {
    if denominator == 0 {
        return Value::Number(Number::from(0));
    }
    let value = numerator as f64 / denominator as f64;
    Number::from_f64((value * 10000.0).round() / 10000.0)
        .map(Value::Number)
        .unwrap_or_else(|| Value::Number(Number::from(0)))
}

fn format_utc(value: OffsetDateTime) -> String {
    value
        .to_offset(UtcOffset::UTC)
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

fn should_keep_semantic_token(token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    if matches!(
        token,
        "公告" | "解读" | "体验" | "分享" | "发布" | "介绍" | "真的" | "一下" | "我们"
    ) {
        return false;
    }
    token.chars().count() > 1 || token.chars().all(|ch| ch.is_ascii_digit())
}
