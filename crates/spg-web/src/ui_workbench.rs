use crate::models::WorkbenchPageView;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;

const ASSET_VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "-workbench-v1");

pub fn render_workbench_page(page: WorkbenchPageView) -> String {
    let state = STANDARD.encode(serde_json::to_string(&page).expect("serialize page state"));
    let content = format!(
        r##"
        <div id="workbench-root" data-state="{state}" hidden></div>
        {shell_backdrop}
        <main class="app-shell app-shell-workbench">
          <section class="top-ribbon">
            <div>
              <p class="eyebrow">SPG DESKTOP COMMAND CENTER</p>
              <h1>03 SLG 选题工作台</h1>
              <p class="hero-copy">通过 Doubao、Serper、Exa、Tavily、Firecrawl 的协同调度，完成抖音 SLG 内容的发现、拆解、排名与选题沉淀。</p>
            </div>
            <div class="status-cluster">
              <div class="status-chip">
                <span>当前用户</span>
                <strong>{display_name}</strong>
              </div>
              <div class="status-chip">
                <span>必需 API</span>
                <strong>{required_ready}/{required_total}</strong>
              </div>
              <div class="status-chip">
                <span>选题池</span>
                <strong>{topic_pool_count}</strong>
              </div>
            </div>
          </section>

          <section class="step-banner">
            <div class="step-banner-copy">
              <p class="section-kicker">WORKBENCH</p>
              <h2>热度发现、结构拆解、选题沉淀一体化</h2>
              <p>这一页会优先展示最近一次有效快照。你也可以在右侧策略区调整 watchlist、时间窗、调度频率和 Doubao endpoint/model。</p>
            </div>
            <div class="step-pill-row">
              <span class="step-pill done">01 登录已确认</span>
              <span class="step-pill done">02 API 已解锁</span>
              <span class="step-pill active">03 主系统运行中</span>
            </div>
          </section>

          <section class="galaxy-toolbar">
            <button id="run-now-btn" class="primary-action" type="button">立即执行本轮分析</button>
            <a class="mini-link" href="/settings/providers">Provider 设置</a>
            <button id="logout-btn" class="ghost-action" type="button">退出登录</button>
          </section>

          <section class="workbench-layout">
            <aside class="workbench-column rankings-column">
              <div class="workbench-card">
                <div class="workbench-card-head">
                  <div>
                    <p class="section-kicker">RANKINGS</p>
                    <h2>排行榜</h2>
                  </div>
                  <div class="tab-strip">
                    <button class="tab-button active" data-kind="content" type="button">内容榜</button>
                    <button class="tab-button" data-kind="game" type="button">游戏榜</button>
                    <button class="tab-button" data-kind="event" type="button">事件榜</button>
                  </div>
                </div>
                <label class="field field-wide">
                  <span>查找</span>
                  <input id="ranking-search" class="text-input" type="text" placeholder="游戏名、事件词、标题关键词"/>
                </label>
                <div id="ranking-list" class="list-panel workbench-list"></div>
              </div>
            </aside>

            <section class="workbench-column detail-column">
              <div class="workbench-card hero-workbench-card">
                <div class="workbench-card-head">
                  <div>
                    <p class="section-kicker">BREAKDOWN</p>
                    <h2 id="detail-title">等待选择内容</h2>
                    <p id="detail-subtitle" class="focus-subtitle">点击左侧任意内容榜条目后，这里会展开 Doubao 拆解卡与热度细节。</p>
                  </div>
                  <div id="latest-run-pill" class="status-pill">未执行</div>
                </div>
                <div class="mini-stat-grid" id="detail-metrics"></div>
                <div id="detail-breakdown" class="analysis-list"></div>
                <div class="topic-pool-form">
                  <label class="field field-wide">
                    <span>选题备注</span>
                    <textarea id="topic-note" class="text-input textarea-input" rows="3" placeholder="为这条内容补充落地备注"></textarea>
                  </label>
                  <div class="provider-actions">
                    <button id="topic-add-btn" class="secondary-action" type="button" disabled>加入选题池</button>
                    <a id="source-link" class="mini-link" href="#" target="_blank" rel="noreferrer noopener">打开来源</a>
                  </div>
                </div>
              </div>
            </section>

            <aside class="workbench-column side-column">
              <div class="workbench-card">
                <div class="workbench-card-head">
                  <div>
                    <p class="section-kicker">TOPIC POOL</p>
                    <h2>选题池</h2>
                  </div>
                  <a class="mini-link" href="/api/workbench/topic-pool/export">导出 CSV</a>
                </div>
                <div id="topic-pool-list" class="analysis-list"></div>
              </div>

              <div class="workbench-card">
                <div class="workbench-card-head">
                  <div>
                    <p class="section-kicker">STRATEGY</p>
                    <h2>策略设置</h2>
                  </div>
                </div>
                <form id="settings-form" class="settings-form-grid">
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
                    <input name="doubao_model" class="text-input" type="text" placeholder="例如你的 endpoint id"/>
                  </label>
                  <button class="primary-action" type="submit">保存策略</button>
                </form>
              </div>

              <div class="workbench-card">
                <div class="workbench-card-head">
                  <div>
                    <p class="section-kicker">BUDGET</p>
                    <h2>API 配额状态</h2>
                  </div>
                </div>
                <div id="budget-grid" class="analysis-list"></div>
              </div>
            </aside>
          </section>
        </main>
        "##,
        state = state,
        shell_backdrop = shell_backdrop(),
        display_name = escape_html(&page.local_user.display_name),
        required_ready = page.unlock_state.ready_count,
        required_total = page.unlock_state.total_count,
        topic_pool_count = page.overview.topic_pool_count,
    );

    html_document(
        "SPG Desktop | Workbench",
        "page-workbench",
        &content,
        "/assets/shell.css",
        "/assets/workbench.js",
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

fn shell_backdrop() -> &'static str {
    r#"
    <div class="space-backdrop">
      <div class="space-orbit space-orbit-one"></div>
      <div class="space-orbit space-orbit-two"></div>
      <div class="space-haze"></div>
      <div class="space-horizon"></div>
      <div class="space-grid"></div>
    </div>
    "#
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
