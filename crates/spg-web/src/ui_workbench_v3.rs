use crate::models::WorkbenchPageView;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;

const ASSET_VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "-workbench-v4-fit-2");

pub fn render_workbench_page(page: WorkbenchPageView) -> String {
    let state = STANDARD.encode(serde_json::to_string(&page).expect("serialize page state"));
    let watchlist_html = render_watchlist(&page.overview.settings.watchlist_games);
    let strategy_summary = format!(
        "{}H 热度窗 · 每 {}H 调度 · 每轮 {} 条",
        page.overview.settings.time_window_hours,
        page.overview.settings.schedule_interval_hours,
        page.overview.settings.max_candidates_per_run
    );

    let content = format!(
        r###"
        <div id="workbench-root" data-state="{state}" hidden></div>
        <main class="control-shell">
          <header class="control-header">
            <div class="control-brand">
              <p class="control-brand-mark">SPG DESKTOP</p>
              <div>
                <h1>群星璀璨</h1>
                <p>SLG 赛道热度主控台</p>
              </div>
            </div>

            <div class="control-header-rank">
              <div class="control-rank-strip-head">
                <p>游戏热度榜</p>
                <span id="heat-strip-count">0 条</span>
              </div>
              <div id="game-heat-strip" class="control-heat-strip"></div>
            </div>

            <a class="ghost-action control-back-link" href="/setup/providers">返回上一页</a>
          </header>

          <section class="control-summary-strip">
            <div class="control-summary-chip">
              <span>当前选中游戏</span>
              <strong id="summary-selected-game">等待选择</strong>
            </div>
            <button id="summary-topic-pool-btn" class="control-summary-chip control-summary-chip-action" type="button">
              <span>选题池</span>
              <strong id="summary-topic-count">{topic_pool_count}</strong>
            </button>
            <button id="summary-strategy-btn" class="control-summary-chip control-summary-chip-action" type="button">
              <span>策略摘要</span>
              <strong id="summary-strategy">{strategy_summary}</strong>
            </button>
            <div class="control-summary-chip">
              <span>最近运行</span>
              <strong id="summary-run-status">等待同步</strong>
            </div>
          </section>

          <section class="control-grid">
            <aside class="control-left-rail">
              <section class="control-panel control-panel-search">
                <div class="control-panel-head">
                  <div>
                    <p class="control-kicker">GAME SEARCH</p>
                    <h3>游戏搜索简介区</h3>
                  </div>
                </div>

                <div class="control-search-row">
                  <label class="control-search-field" for="game-search-input">
                    <span>搜索游戏</span>
                    <input
                      id="game-search-input"
                      class="text-input"
                      type="search"
                      placeholder="输入游戏名、别名或事件关键词"
                      autocomplete="off"
                      spellcheck="false"
                    />
                  </label>
                  <button id="game-search-submit" class="ghost-action compact-action" type="button">确定搜索</button>
                </div>

                <div class="control-watchlist-strip" id="watchlist-strip">
                  {watchlist_html}
                </div>

                <div class="control-list-meta" id="game-list-meta">准备加载游戏列表</div>
                <div id="game-search-list" class="control-game-list"></div>
              </section>

              <section class="control-panel control-panel-api">
                <div class="control-panel-head">
                  <div>
                    <p class="control-kicker">API STATUS</p>
                    <h3>API 调用实况区</h3>
                  </div>
                  <button id="toggle-api-panel-btn" class="ghost-action compact-action" type="button">展开详情</button>
                </div>

                <div class="control-api-summary" id="api-summary-card"></div>
                <div id="api-status-dropdown" class="control-api-dropdown" hidden>
                  <div id="api-status-list" class="control-api-list"></div>
                </div>

                <div class="control-api-actions">
                  <button id="run-now-btn" class="primary-action" type="button">立即执行本轮分析</button>
                </div>
              </section>
            </aside>

            <section class="control-main-column">
              <section class="control-panel control-stage-panel">
                <div class="control-panel-head">
                  <div>
                    <p class="control-kicker">HEAT STAGE</p>
                    <h3>赛道游戏可视化排名区</h3>
                  </div>
                  <div class="control-stage-state">
                    <span id="active-event-pill">未锁定事件</span>
                  </div>
                </div>

                <div class="control-stage-split">
                  <div class="control-map-shell control-map-shell-full">
                    <svg id="constellation-map" viewBox="0 0 760 440" role="img" aria-label="SLG constellation map"></svg>
                  </div>
                </div>
              </section>

              <section class="control-panel control-video-panel">
                <div class="control-panel-head">
                  <div>
                    <p class="control-kicker">DOUYIN BREAKDOWN</p>
                    <h3>抖音视频拆解区（10条）</h3>
                    <p class="control-copy">固定展示当前条件下最值得参考的 10 条视频，点击后右侧同步切换拆解和创作指导。</p>
                  </div>
                </div>

                <div id="event-filter-row" class="control-event-filter-row"></div>
                <div id="video-panel-status" class="control-inline-status"></div>
                <div id="video-breakdown-list" class="control-video-list"></div>
              </section>
            </section>

            <aside class="control-right-rail">
              <section class="control-panel control-result-panel">
                <div class="control-panel-head">
                  <div>
                    <p class="control-kicker">VIDEO INSIGHT</p>
                    <h3>对应选择游戏热门视频拆解区</h3>
                  </div>
                </div>

                <div id="selected-video-meta" class="control-metric-grid"></div>
                <div id="selected-video-trust" class="control-inline-status"></div>
                <div id="selected-video-breakdown" class="control-insight-list"></div>
              </section>

              <section class="control-panel control-guide-panel">
                <div class="control-panel-head">
                  <div>
                    <p class="control-kicker">CREATIVE GUIDE</p>
                    <h3>对应选择游戏热门视频创作指导区</h3>
                  </div>
                </div>

                <div id="selected-video-guide" class="control-insight-list"></div>

                <div class="control-guide-actions">
                  <label class="control-note-field" for="topic-note-input">
                    <span>选题备注</span>
                    <textarea
                      id="topic-note-input"
                      class="text-input"
                      rows="3"
                      placeholder="补充这条内容为什么值得进入选题池"
                    ></textarea>
                  </label>

                  <div class="control-guide-buttons">
                    <button id="add-topic-btn" class="secondary-action" type="button" disabled>加入选题池</button>
                    <a id="open-source-link" class="mini-link" href="#" target="_blank" rel="noreferrer noopener">打开来源</a>
                  </div>
                </div>
              </section>
            </aside>
          </section>
        </main>

        <div id="drawer-overlay" class="control-drawer-overlay" hidden></div>

        <aside id="topic-pool-drawer" class="control-drawer" hidden>
          <div class="control-drawer-head">
            <div>
              <p class="control-kicker">TOPIC POOL</p>
              <h3>选题池</h3>
              <p class="control-copy">这里保留完整的选题池内容，但不再占主页面主舞台。</p>
            </div>
            <button class="ghost-action control-drawer-close" data-close-drawer="topic-pool-drawer" type="button">关闭</button>
          </div>
          <div class="control-drawer-actions">
            <a class="mini-link" href="/api/workbench/topic-pool/export">导出 CSV</a>
          </div>
          <div id="topic-pool-drawer-list" class="control-drawer-list"></div>
        </aside>

        <aside id="strategy-drawer" class="control-drawer" hidden>
          <div class="control-drawer-head">
            <div>
              <p class="control-kicker">STRATEGY</p>
              <h3>策略设置</h3>
              <p class="control-copy">主页面只展示摘要，完整策略设置收进这里。</p>
            </div>
            <button class="ghost-action control-drawer-close" data-close-drawer="strategy-drawer" type="button">关闭</button>
          </div>
          <form id="strategy-form" class="control-strategy-form">
            <label class="field field-wide">
              <span>Watchlist Games</span>
              <textarea name="watchlist_games" class="text-input" rows="4" placeholder="每行一个游戏名"></textarea>
            </label>
            <label class="field field-wide">
              <span>Keyword Templates</span>
              <textarea name="keyword_templates" class="text-input" rows="5" placeholder="每行一个查询模板"></textarea>
            </label>
            <label class="field">
              <span>时间窗（小时）</span>
              <input name="time_window_hours" class="text-input" type="number" min="6" max="168" />
            </label>
            <label class="field">
              <span>调度间隔（小时）</span>
              <input name="schedule_interval_hours" class="text-input" type="number" min="1" max="24" />
            </label>
            <label class="field">
              <span>每轮候选上限</span>
              <input name="max_candidates_per_run" class="text-input" type="number" min="10" max="50" />
            </label>
            <label class="field field-wide">
              <span>Doubao Endpoint / Model</span>
              <input name="doubao_model" class="text-input" type="text" placeholder="填写实际可调用的 endpoint 或 model id" />
            </label>
            <button class="primary-action" type="submit">保存策略</button>
          </form>
        </aside>
        "###,
        state = state,
        topic_pool_count = page.overview.topic_pool_count,
        strategy_summary = escape_html(&strategy_summary),
        watchlist_html = watchlist_html,
    );

    html_document(
        "SPG Desktop | 群星璀璨",
        "page-workbench",
        &content,
        "/assets/shell.css",
        "/assets/workbench_v3.js",
    )
}

fn render_watchlist(watchlist_games: &[String]) -> String {
    if watchlist_games.is_empty() {
        return r#"<span class="control-watchlist-chip is-muted">等待配置监控游戏</span>"#
            .to_string();
    }

    watchlist_games
        .iter()
        .map(|game| {
            format!(
                r#"<button class="control-watchlist-chip" type="button" data-action="seed-watch-game" data-watch-game="{}">{}</button>"#,
                escape_html(game),
                escape_html(game)
            )
        })
        .collect::<Vec<_>>()
        .join("")
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
