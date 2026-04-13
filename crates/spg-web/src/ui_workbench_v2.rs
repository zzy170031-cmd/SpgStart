use crate::models::WorkbenchPageView;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;

const ASSET_VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "-workbench-v2");

pub fn render_workbench_page(page: WorkbenchPageView) -> String {
    let state = STANDARD.encode(serde_json::to_string(&page).expect("serialize page state"));
    let latest_run = page.overview.latest_run.as_ref();
    let latest_run_status = latest_run
        .map(|run| escape_html(&run.status))
        .unwrap_or_else(|| "待执行".to_string());
    let latest_run_time = latest_run
        .and_then(|run| run.finished_at.as_ref().or(Some(&run.started_at)))
        .map(|value| escape_html(value))
        .unwrap_or_else(|| "尚未产生工作流结果".to_string());
    let watchlist_html = if page.overview.settings.watchlist_games.is_empty() {
        r#"<span class="watchlist-chip muted">等待配置监控游戏</span>"#.to_string()
    } else {
        page.overview
            .settings
            .watchlist_games
            .iter()
            .map(|item| {
                format!(
                    r#"<span class="watchlist-chip">{}</span>"#,
                    escape_html(item)
                )
            })
            .collect::<Vec<_>>()
            .join("")
    };

    let content = format!(
        r##"
        <div id="workbench-root" data-state="{state}" hidden></div>
        <main class="dashboard-shell dashboard-shell-workbench">
          <header class="dashboard-topbar dashboard-topbar-workbench">
            <div class="dashboard-brand">
              <p class="dashboard-kicker">SPG / MAIN SYSTEM</p>
              <h1>SLG 选题工作台</h1>
              <p class="dashboard-copy">基于 Doubao 与搜索抓取组合，持续产出抖音 SLG 热度榜、视频拆解与选题池。</p>
            </div>

            <div class="dashboard-topbar-side">
              <div class="dashboard-metrics">
                <div class="dashboard-total">
                  <span>当前用户</span>
                  <strong>{display_name}</strong>
                </div>
                <div class="dashboard-total">
                  <span>必需 API</span>
                  <strong>{required_ready}/{required_total}</strong>
                </div>
                <div class="dashboard-total">
                  <span>选题池</span>
                  <strong id="top-topic-count">{topic_pool_count}</strong>
                </div>
                <div class="dashboard-total">
                  <span>最近运行</span>
                  <strong id="top-run-status">{latest_run_status}</strong>
                </div>
              </div>

              <div class="dashboard-actions">
                <button id="run-now-btn" class="primary-action" type="button">立即执行本轮分析</button>
                <a class="ghost-action" href="/settings/providers">Provider 设置</a>
                <button id="logout-btn" class="ghost-action" type="button">退出登录</button>
              </div>
            </div>
          </header>

          <section class="dashboard-grid workbench-dashboard-grid">
            <aside class="dashboard-panel panel-watchlist">
              <div class="panel-header panel-header-stack">
                <div>
                  <p class="panel-kicker">watch list</p>
                  <h2>游戏观察区</h2>
                  <p class="panel-copy">这里承接备份布局里的左侧观察栏，用于筛选当前关注的 SLG 游戏，并联动内容榜与事件榜。</p>
                </div>
              </div>

              <label class="search-field" for="workbench-search">
                <span class="panel-kicker">search</span>
                <input
                  id="workbench-search"
                  class="search-input"
                  type="search"
                  placeholder="搜索游戏名、事件词或内容标题"
                  autocomplete="off"
                  spellcheck="false"
                />
              </label>

              <div class="filter-strip filter-strip-workbench">
                <button id="clear-game-filter" class="filter-chip active" type="button">全部游戏</button>
                <button id="clear-event-filter" class="filter-chip" type="button">全部事件</button>
              </div>

              <div class="watchlist-strip">
                {watchlist_html}
              </div>

              <div id="game-meta" class="search-meta">显示全部监控游戏</div>
              <div id="game-ranking-list" class="list-panel dashboard-list"></div>

              <div class="run-brief-card">
                <p class="panel-kicker">latest run</p>
                <h3 id="run-status-title">{latest_run_status}</h3>
                <p id="run-status-copy" class="panel-copy">{latest_run_time}</p>
              </div>
            </aside>

            <section class="dashboard-panel panel-content-rank">
              <div class="panel-header">
                <div>
                  <p class="panel-kicker">content ranking</p>
                  <h2>热视频榜单</h2>
                  <p class="panel-copy">中间主区沿用备份版的排行榜思路，优先展示当前最值得拆解与改编的抖音内容。</p>
                </div>
                <div class="heat-window-note">
                  <span class="panel-kicker">window</span>
                  <strong>{time_window_hours}H</strong>
                </div>
              </div>

              <div class="ranking-focus-card workbench-focus-card">
                <div class="ranking-focus-copy">
                  <p class="panel-kicker">selected focus</p>
                  <h3 id="focus-title">等待选择内容</h3>
                  <p id="focus-subtitle" class="focus-subtitle">点击榜单、游戏或事件后，这里会同步显示当前聚焦对象的摘要和关键分数。</p>
                </div>
                <div id="focus-metrics" class="focus-metrics workbench-focus-metrics"></div>
              </div>

              <div id="content-ranking-list" class="list-panel dashboard-list"></div>
            </section>

            <aside class="dashboard-panel panel-events">
              <div class="panel-header panel-header-stack">
                <div>
                  <p class="panel-kicker">event ranking</p>
                  <h2>热点事件</h2>
                  <p class="panel-copy">右上角承接事件观察位，查看当前游戏之下最值得追踪的赛季、活动与版本事件。</p>
                </div>
              </div>
              <div id="event-meta" class="search-meta">显示全部热点事件</div>
              <div id="event-ranking-list" class="list-panel dashboard-list"></div>
            </aside>

            <section class="dashboard-panel panel-breakdown">
              <div class="panel-header panel-header-stack">
                <div>
                  <p class="panel-kicker">doubao breakdown</p>
                  <h2 id="detail-title">结构化拆解</h2>
                  <p id="detail-subtitle" class="panel-copy">选中一条内容后，这里会展示 Doubao 的拆解结果，包括摘要、爆点、改编角度与标题方向。</p>
                </div>
              </div>

              <div id="detail-breakdown" class="workbench-breakdown-grid"></div>

              <div class="topic-pool-form compact-topic-form">
                <label class="field field-wide">
                  <span>选题备注</span>
                  <textarea id="topic-note" class="text-input textarea-input" rows="3" placeholder="补充这条内容为什么值得进入选题池"></textarea>
                </label>
                <div class="provider-actions">
                  <button id="topic-add-btn" class="secondary-action" type="button" disabled>加入选题池</button>
                  <a id="source-link" class="mini-link" href="#" target="_blank" rel="noreferrer noopener">打开来源</a>
                </div>
              </div>
            </section>

            <section class="dashboard-panel panel-topic-pool">
              <div class="panel-header">
                <div>
                  <p class="panel-kicker">topic pool</p>
                  <h2>选题池</h2>
                  <p class="panel-copy">可直接沉淀准备二创的题材，保留来源、热度和备注，支持随时导出 CSV。</p>
                </div>
                <a class="mini-link" href="/api/workbench/topic-pool/export">导出 CSV</a>
              </div>
              <div id="topic-pool-list" class="analysis-list"></div>
            </section>

            <section class="dashboard-panel panel-strategy">
              <div class="panel-header panel-header-stack">
                <div>
                  <p class="panel-kicker">strategy</p>
                  <h2>策略与配额</h2>
                  <p class="panel-copy">这里保留调度策略和 API 配额，作为主系统右下角的控制区，不干扰主榜单浏览。</p>
                </div>
              </div>

              <form id="settings-form" class="settings-form-grid workbench-settings-grid">
                <label class="field field-wide">
                  <span>Watchlist Games</span>
                  <textarea name="watchlist_games" class="text-input textarea-input" rows="3" placeholder="每行一个游戏名"></textarea>
                </label>
                <label class="field field-wide">
                  <span>Keyword Templates</span>
                  <textarea name="keyword_templates" class="text-input textarea-input" rows="4" placeholder="每行一个查询模板"></textarea>
                </label>
                <label class="field">
                  <span>时间窗（小时）</span>
                  <input name="time_window_hours" class="text-input" type="number" min="6" max="168"/>
                </label>
                <label class="field">
                  <span>调度间隔（小时）</span>
                  <input name="schedule_interval_hours" class="text-input" type="number" min="1" max="24"/>
                </label>
                <label class="field">
                  <span>每轮候选上限</span>
                  <input name="max_candidates_per_run" class="text-input" type="number" min="10" max="50"/>
                </label>
                <label class="field field-wide">
                  <span>Doubao Endpoint / Model</span>
                  <input name="doubao_model" class="text-input" type="text" placeholder="填写实际可调用的 endpoint 或 model id"/>
                </label>
                <button class="primary-action" type="submit">保存策略</button>
              </form>

              <div class="panel-divider"></div>

              <div class="panel-header panel-header-inline">
                <div>
                  <p class="panel-kicker">api budget</p>
                  <h3>本轮配额状态</h3>
                </div>
              </div>
              <div id="budget-grid" class="analysis-list compact-budget-list"></div>
            </section>
          </section>
        </main>
        "##,
        state = state,
        display_name = escape_html(&page.local_user.display_name),
        required_ready = page.unlock_state.ready_count,
        required_total = page.unlock_state.total_count,
        topic_pool_count = page.overview.topic_pool_count,
        latest_run_status = latest_run_status,
        latest_run_time = latest_run_time,
        watchlist_html = watchlist_html,
        time_window_hours = page.overview.settings.time_window_hours,
    );

    html_document(
        "SPG Desktop | Workbench",
        "page-workbench",
        &content,
        "/assets/shell.css",
        "/assets/workbench_v2.js",
    )
}

fn html_document(
    title: &str,
    body_class: &str,
    content: &str,
    stylesheet: &str,
    script: &str,
) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="zh-CN">
  <head>
    <meta charset="utf-8"/>
    <meta name="viewport" content="width=device-width, initial-scale=1.0"/>
    <title>{title}</title>
    <link rel="stylesheet" href="{stylesheet}?v={version}"/>
  </head>
  <body class="{body_class}">
    {content}
    <script type="module" src="{script}?v={version}"></script>
  </body>
</html>"#,
        title = title,
        stylesheet = stylesheet,
        version = ASSET_VERSION,
        body_class = body_class,
        content = content,
        script = script,
    )
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
