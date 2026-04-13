use crate::models::{GalaxyPageView, LoginPageView, SetupPageView};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;

const ASSET_VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "-desktop-shell-v2");

pub fn render_login_page(page: LoginPageView) -> String {
    let state = encoded_state(&page);
    let content = format!(
        r#"
        <div id="login-root" data-state="{state}" hidden></div>
        {shell_backdrop}
        <main class="app-shell app-shell-login">
          <section class="top-ribbon">
            <div>
              <p class="eyebrow">SPG DESKTOP COMMAND CENTER</p>
              <h1>01 本地用户确认</h1>
              <p class="hero-copy">先确认当前设备上的本地用户，再进入 API 校验舱，最后解锁主系统。</p>
            </div>
            <div class="status-cluster">
              <div class="status-chip">
                <span>当前阶段</span>
                <strong>登录确认</strong>
              </div>
              <div class="status-chip">
                <span>设备</span>
                <strong>{device_name}</strong>
              </div>
            </div>
          </section>

          <section class="hero-panel hero-panel-login">
            <div class="hero-panel-copy">
              <p class="section-kicker">STEP 01 / ACCESS</p>
              <h2>进入深空指挥中心</h2>
              <p>系统已自动识别当前 Windows 本地用户。确认后进入 API 配置与测试页面。</p>
              <div class="identity-grid">
                <div class="identity-card">
                  <span>本地用户名</span>
                  <strong>{username}</strong>
                </div>
                <div class="identity-card">
                  <span>设备名称</span>
                  <strong>{device_name}</strong>
                </div>
              </div>
            </div>

            <div class="command-card command-card-login">
              <div class="command-card-head">
                <p class="section-kicker">LOCAL USER</p>
                <h3>{display_name}</h3>
                <p>确认当前身份后，系统会记住本机的通过状态，直到你主动退出登录。</p>
              </div>

              <div class="step-ladder">
                <div class="step-row active">
                  <span class="step-index">01</span>
                  <div>
                    <strong>确认本地用户</strong>
                    <p>读取当前 Windows 用户名与设备名。</p>
                  </div>
                </div>
                <div class="step-row">
                  <span class="step-index">02</span>
                  <div>
                    <strong>配置并测试 API</strong>
                    <p>五个必需 API 全部通过后才能继续。</p>
                  </div>
                </div>
                <div class="step-row">
                  <span class="step-index">03</span>
                  <div>
                    <strong>进入主系统</strong>
                    <p>进入 SLG 内容筛选与拆解主界面。</p>
                  </div>
                </div>
              </div>

              <button id="confirm-login-btn" class="primary-action" type="button">确认并进入 API 配置</button>
            </div>
          </section>
        </main>
        "#,
        state = state,
        shell_backdrop = shell_backdrop(),
        username = escape_html(&page.local_user.username),
        device_name = escape_html(&page.local_user.device_name),
        display_name = escape_html(&page.local_user.display_name),
    );

    html_document(
        "SPG Desktop | Login",
        "page-login",
        &content,
        "/assets/shell.css",
        "/assets/login.js",
    )
}

pub fn render_setup_page(page: SetupPageView) -> String {
    let state = encoded_state(&page);
    let mode_label = if page.mode == "settings" {
        "Provider 设置"
    } else {
        "API 配置测试"
    };
    let content = format!(
        r#"
        <div id="setup-root" data-state="{state}" data-mode="{mode}" hidden></div>
        {shell_backdrop}
        <main class="app-shell app-shell-setup">
          <section class="top-ribbon top-ribbon-compact">
            <div>
              <p class="eyebrow">STEP 02 / API ACCESS</p>
              <h1>02 {mode_label}</h1>
              <p class="hero-copy">这一页只做 API 保存和测试。五个必需 API 全部通过后，才能进入主系统。</p>
            </div>
            <div class="status-cluster">
              <div class="status-chip">
                <span>当前用户</span>
                <strong>{display_name}</strong>
              </div>
              <div class="status-chip">
                <span>必需通过</span>
                <strong id="required-progress">{required_ready}/{required_total}</strong>
              </div>
              <div class="status-chip">
                <span>主系统</span>
                <strong id="unlock-state-label">{unlock_state_label}</strong>
              </div>
            </div>
          </section>

          <section class="setup-inline-bar">
            <div id="unlock-panel" class="unlock-panel unlock-panel-inline">
              {unlock_panel}
            </div>
            <div class="command-card-actions command-card-actions-inline">
              <button id="test-all-btn" class="secondary-action" type="button">一键测试全部 API</button>
              <button id="advance-gate-btn" class="{advance_class}" type="button" {advance_disabled}>进入主系统</button>
              <button id="logout-btn" class="ghost-action" type="button">退出登录</button>
            </div>
          </section>

          <section class="setup-layout setup-layout-compact">
            <section class="provider-stage provider-stage-compact">
              <div class="provider-stage-head provider-stage-head-compact">
                <div>
                  <p class="section-kicker">API LIST</p>
                  <h2>逐行配置与测试</h2>
                  <p class="provider-desc">默认只显示核心项，地址按需展开。</p>
                </div>
              </div>
              <div id="provider-cards" class="provider-row-list"></div>
            </section>
          </section>
        </main>
        "#,
        state = state,
        mode = escape_html(&page.mode),
        mode_label = mode_label,
        shell_backdrop = shell_backdrop(),
        display_name = escape_html(&page.local_user.display_name),
        required_ready = page.unlock_state.ready_count,
        required_total = page.unlock_state.total_count,
        unlock_state_label = if page.unlock_state.unlocked {
            "已解锁"
        } else {
            "待完成"
        },
        unlock_panel = render_unlock_panel(&page.unlock_state),
        advance_class = if page.unlock_state.unlocked {
            "primary-action"
        } else {
            "primary-action disabled"
        },
        advance_disabled = if page.unlock_state.unlocked {
            ""
        } else {
            "disabled"
        },
    );

    html_document(
        "SPG Desktop | Provider Setup",
        "page-setup",
        &content,
        "/assets/shell.css",
        "/assets/setup.js",
    )
}

pub fn render_galaxy_page(page: GalaxyPageView) -> String {
    let state = encoded_state(&page);
    let content = format!(
        r#"
        <div id="galaxy-root" data-state="{state}" hidden></div>
        {shell_backdrop}
        <main class="app-shell app-shell-galaxy">
          <section class="top-ribbon">
            <div>
              <p class="eyebrow">SPG DESKTOP COMMAND CENTER</p>
              <h1>03 主系统</h1>
              <p class="hero-copy">{subheadline}</p>
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
                <span>最近更新</span>
                <strong>{last_updated}</strong>
              </div>
            </div>
          </section>

          <section class="step-banner">
            <div class="step-banner-copy">
              <p class="section-kicker">MAIN SYSTEM</p>
              <h2>{headline}</h2>
              <p>这一轮先保留现有主系统内容逻辑，并统一到同一套深空壳层里。</p>
            </div>
            <div class="step-pill-row">
              <span class="step-pill done">01 登录已确认</span>
              <span class="step-pill done">02 API 已解锁</span>
              <span class="step-pill active">03 主系统运行中</span>
            </div>
          </section>

          <section class="galaxy-toolbar">
            <a class="mini-link" href="/settings/providers">Provider 设置</a>
            <button id="logout-btn" class="ghost-action" type="button">退出登录</button>
          </section>

          <main class="galaxy-layout">
            <aside class="galaxy-sidebar">
              <div class="sidebar-header">
                <div>
                  <p class="section-kicker">EXPLORER</p>
                  <h2>SLG 星图导航</h2>
                </div>
              </div>

              <div class="tab-strip">
                <button class="tab-button active" data-tab="games" type="button">游戏实体</button>
                <button class="tab-button" data-tab="events" type="button">热点事件</button>
              </div>

              <div id="galaxy-list" class="list-panel"></div>
            </aside>

            <section class="galaxy-main">
              <section class="heat-rank-card">
                <div class="heat-rank-header">
                  <div>
                    <p class="section-kicker">HEAT RANKING</p>
                    <h2>赛道热度排名</h2>
                  </div>
                  <p class="heat-rank-copy">滚动展示当前赛道里正在放大的游戏、事件与传播节点。</p>
                </div>
                <div class="heat-marquee">
                  <div id="heat-marquee-track" class="heat-marquee-track"></div>
                </div>
              </section>

              <div class="galaxy-card">
                <div class="galaxy-card-header">
                  <div>
                    <p class="section-kicker">GALAXY VIEW</p>
                    <h2 id="focus-title">{focus_title}</h2>
                    <p id="focus-subtitle" class="focus-subtitle">{focus_subtitle}</p>
                  </div>
                  <div id="layer-filters" class="filter-strip">
                    <button class="filter-chip active" data-layer="games" type="button">游戏</button>
                    <button class="filter-chip active" data-layer="events" type="button">事件</button>
                    <button class="filter-chip active" data-layer="sources" type="button">来源</button>
                  </div>
                </div>

                <div class="galaxy-stage">
                  <aside class="stage-rail stage-rail-left">
                    <div class="stage-rail-header">
                      <p class="section-kicker">HOT EVENTS</p>
                      <h3>爆款事件</h3>
                      <p>围绕当前焦点展示可继续追踪的事件入口。</p>
                    </div>
                    <div id="event-rail" class="rail-list"></div>
                  </aside>

                  <div class="galaxy-center">
                    <div class="galaxy-canvas-shell">
                      <svg id="galaxy-svg" viewBox="0 0 960 620" role="img" aria-label="SLG galaxy visualization"></svg>
                    </div>
                  </div>

                  <aside class="stage-rail stage-rail-right">
                    <div class="stage-rail-header">
                      <p class="section-kicker">HOT VIDEOS</p>
                      <h3>爆款视频</h3>
                      <p>按平台给出可直接打开的内容入口和观察方向。</p>
                    </div>
                    <div id="video-rail" class="rail-list"></div>
                  </aside>
                </div>
              </div>

              <div class="insight-grid">
                <div class="insight-card">
                  <p class="section-kicker">NARRATIVE</p>
                  <ul id="focus-narrative" class="narrative-list">{narrative_html}</ul>
                </div>
                <div class="insight-card">
                  <p class="section-kicker">SYSTEM STATUS</p>
                  <div class="status-stack">
                    <div class="mini-stat">
                      <span>主系统解锁</span>
                      <strong>{unlocked}</strong>
                    </div>
                    <div class="mini-stat">
                      <span>必需 API Ready</span>
                      <strong>{required_ready}/{required_total}</strong>
                    </div>
                    <div class="mini-stat">
                      <span>可选 API Ready</span>
                      <strong>{optional_ready}/{optional_total}</strong>
                    </div>
                  </div>
                </div>
              </div>

              <div class="analysis-grid">
                <div class="analysis-card">
                  <div class="analysis-header">
                    <p class="section-kicker">EVENT BREAKDOWN</p>
                    <h3>热点事件拆解</h3>
                  </div>
                  <div id="event-breakdown" class="analysis-list"></div>
                </div>
                <div class="analysis-card">
                  <div class="analysis-header">
                    <p class="section-kicker">VIDEO BREAKDOWN</p>
                    <h3>重点视频拆解</h3>
                  </div>
                  <div id="video-breakdown" class="analysis-list"></div>
                </div>
                <div class="analysis-card">
                  <div class="analysis-header">
                    <p class="section-kicker">CREATION GUIDE</p>
                    <h3>创作指导</h3>
                  </div>
                  <div id="creation-guide" class="analysis-list"></div>
                </div>
              </div>
            </section>
          </main>
        </main>
        "#,
        state = state,
        shell_backdrop = shell_backdrop(),
        subheadline = escape_html(&page.summary.subheadline),
        display_name = escape_html(&page.local_user.display_name),
        required_ready = page.unlock_state.ready_count,
        required_total = page.unlock_state.total_count,
        optional_ready = page.unlock_state.optional_ready_count,
        optional_total = page.unlock_state.optional_total_count,
        last_updated = escape_html(&page.summary.last_updated),
        headline = escape_html(&page.summary.headline),
        focus_title = escape_html(&page.initial_focus.title),
        focus_subtitle = escape_html(&page.initial_focus.subtitle),
        narrative_html = page
            .initial_focus
            .narrative
            .iter()
            .map(|item| format!("<li>{}</li>", escape_html(item)))
            .collect::<Vec<_>>()
            .join(""),
        unlocked = if page.unlock_state.unlocked {
            "是"
        } else {
            "否"
        },
    );

    html_document(
        "SPG Desktop | Galaxy",
        "page-galaxy",
        &content,
        "/assets/shell.css",
        "/assets/galaxy_shell.js",
    )
}

fn render_unlock_panel(unlock_state: &crate::models::UnlockStateView) -> String {
    let blockers = if unlock_state.blockers.is_empty() {
        "<li>五个必需 API 全部就绪。</li>".to_string()
    } else {
        unlock_state
            .blockers
            .iter()
            .map(|item| format!("<li>{}</li>", escape_html(item)))
            .collect::<Vec<_>>()
            .join("")
    };

    format!(
        r#"
        <div class="unlock-metrics unlock-metrics-compact">
          <div class="mini-stat">
            <span>必需 Ready</span>
            <strong>{required_ready}/{required_total}</strong>
          </div>
          <div class="mini-stat">
            <span>当前状态</span>
            <strong>{status_label}</strong>
          </div>
        </div>
        <p class="status-copy compact-copy">{copy}</p>
        <ul class="blocker-list blocker-list-compact">{blockers}</ul>
        "#,
        required_ready = unlock_state.ready_count,
        required_total = unlock_state.total_count,
        status_label = if unlock_state.unlocked {
            "已解锁"
        } else {
            "待完成"
        },
        copy = if unlock_state.unlocked {
            "所有必需 API 已验证通过，现在可以解锁主系统。"
        } else {
            "仍有必需 API 未完成保存或测试，主系统保持锁定。"
        },
        blockers = blockers,
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

fn encoded_state<T: serde::Serialize>(value: &T) -> String {
    STANDARD.encode(serde_json::to_string(value).expect("serialize page state"))
}

fn shell_backdrop() -> &'static str {
    r#"
    <div class="space-backdrop">
      <div class="space-haze"></div>
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
