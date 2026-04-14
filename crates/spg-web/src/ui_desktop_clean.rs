use crate::models::DesktopLauncherPageView;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;

const ASSET_VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "-desktop-launcher-v2");

pub fn render_desktop_launcher_page(page: DesktopLauncherPageView) -> String {
    let state = STANDARD.encode(serde_json::to_string(&page).expect("serialize desktop launcher"));
    format!(
        r#"<!DOCTYPE html>
<html lang="zh-CN">
  <head>
    <meta charset="utf-8"/>
    <meta name="viewport" content="width=device-width, initial-scale=1.0"/>
    <title>SPG 桌面启动器</title>
    <link rel="stylesheet" href="/assets/desktop_launcher.css?v={version}"/>
  </head>
  <body>
    <div id="desktop-launcher-root" data-state="{state}" hidden></div>
    <main class="shell">
      <header class="hero">
        <div>
          <p class="eyebrow">SPG / Windows Desktop</p>
          <h1>桌面入口确认</h1>
        </div>
        <div class="hero-meta">
          <div class="metric">
            <span>当前版本</span>
            <strong id="current-version">{version_label}</strong>
          </div>
          <div class="metric">
            <span>当前阶段</span>
            <strong id="current-stage">第一页 / 用户确认</strong>
          </div>
        </div>
      </header>

      <main class="grid single-focus">
        <section class="panel panel-focus">
          <p class="panel-kicker">本机身份</p>
          <h2>当前 Windows 用户</h2>
          <div class="info-list">
            <div class="info-row"><span>用户名</span><strong id="identity-username">-</strong></div>
            <div class="info-row"><span>设备名</span><strong id="identity-machine">-</strong></div>
            <div class="info-row"><span>当前状态</span><strong id="identity-status">-</strong></div>
            <div class="info-row"><span>下一步</span><strong id="next-step">-</strong></div>
          </div>
          <div class="action-row">
            <button id="confirm-identity" class="primary" type="button">确认并进入下一步</button>
            <button id="reset-profile" class="ghost" type="button">重置本机确认</button>
          </div>
        </section>
      </main>
    </main>

    <script type="module" src="/assets/desktop_launcher_clean.js?v={version}"></script>
  </body>
</html>"#,
        state = state,
        version = ASSET_VERSION,
        version_label = escape_html(&page.app_version),
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
