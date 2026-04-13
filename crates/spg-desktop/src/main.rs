#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use anyhow::{Context, Result};
use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};
use tao::dpi::LogicalSize;
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::window::{Icon as WindowIcon, WindowBuilder};
use tokio::runtime::Builder;
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::TrayIconBuilder;
use wry::WebViewBuilder;

const APP_URL: &str = "http://127.0.0.1:3000/desktop/launcher";
#[allow(dead_code)]
const SEEDED_GAMES: &[&str] = &[
    "率土之滨",
    "三国群英传策定九州",
    "三国谋定天下",
    "无尽冬日",
    "三国冰河时代",
    "天下归心",
    "九牧之野",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CloseBehavior {
    Exit,
    Tray,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct DesktopPreferences {
    close_behavior: Option<CloseBehavior>,
}

#[derive(Debug)]
struct LocalHttpResponse {
    status_code: u16,
    body: Vec<u8>,
}

fn main() -> Result<()> {
    let workspace_root = spg_web::workspace_root()?;
    spawn_web_server_if_needed(workspace_root.clone());
    wait_for_server_ready().context("SPG web service did not become ready in time")?;

    let prefs_path = desktop_prefs_path(&workspace_root);
    let mut prefs = load_preferences(&prefs_path);

    let event_loop = EventLoop::new();
    let menu = Menu::new();
    let open_item = MenuItem::new("打开主窗口", true, None);
    let logout_item = MenuItem::new("退出登录", true, None);
    let quit_item = MenuItem::new("退出应用", true, None);
    menu.append(&open_item)?;
    menu.append(&logout_item)?;
    menu.append(&quit_item)?;

    let (rgba, width, height) = app_icon_rgba();
    let tray_icon = tray_icon::Icon::from_rgba(rgba.clone(), width, height)
        .context("failed to create tray icon")?;
    let _tray = TrayIconBuilder::new()
        .with_tooltip("SPG Desktop")
        .with_menu(Box::new(menu))
        .with_icon(tray_icon)
        .build()
        .context("failed to build tray icon")?;

    let window = WindowBuilder::new()
        .with_title("SPG 桌面登录器")
        .with_inner_size(LogicalSize::new(1480.0, 940.0))
        .with_resizable(true)
        .with_visible(false)
        .with_window_icon(Some(
            WindowIcon::from_rgba(rgba, width, height).context("failed to create window icon")?,
        ))
        .build(&event_loop)
        .context("failed to create desktop window")?;

    let webview = WebViewBuilder::new()
        .with_url(APP_URL)
        .build(&window)
        .context("failed to build webview")?;

    let menu_events = MenuEvent::receiver();
    let mut exiting = false;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        while let Ok(menu_event) = menu_events.try_recv() {
            let menu_id = menu_event.id();
            if menu_id == open_item.id() {
                show_window(&window);
            } else if menu_id == logout_item.id() {
                match post_local("/api/auth/logout") {
                    Ok(()) => {
                        show_window(&window);
                        let _ = webview
                            .evaluate_script("window.location.replace('/desktop/launcher');");
                    }
                    Err(error) => {
                        let _ = MessageDialog::new()
                            .set_level(MessageLevel::Error)
                            .set_title("SPG Desktop")
                            .set_description(&format!("退出登录失败，请稍后重试。\n\n{error:#}"))
                            .set_buttons(MessageButtons::Ok)
                            .show();
                    }
                }
            } else if menu_id == quit_item.id() {
                exiting = true;
                *control_flow = ControlFlow::Exit;
            }
        }

        match event {
            Event::NewEvents(StartCause::Init) => {
                show_window(&window);
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => match resolve_close_action(&mut prefs, &prefs_path) {
                Some(CloseBehavior::Exit) => {
                    exiting = true;
                    *control_flow = ControlFlow::Exit;
                }
                Some(CloseBehavior::Tray) => {
                    hide_window(&window);
                }
                None => {}
            },
            Event::LoopDestroyed => {
                if !exiting {
                    let _ = save_preferences(&prefs_path, &prefs);
                }
            }
            _ => {}
        }
    });
}

fn resolve_close_action(
    prefs: &mut DesktopPreferences,
    prefs_path: &Path,
) -> Option<CloseBehavior> {
    match prefs.close_behavior.clone() {
        Some(mode) => Some(mode),
        None => {
            let choice = MessageDialog::new()
                .set_level(MessageLevel::Info)
                .set_title("SPG Desktop")
                .set_description(
                    "首次关闭时请选择行为：\n是 = 最小化到托盘\n否 = 退出应用\n取消 = 返回桌面壳",
                )
                .set_buttons(MessageButtons::YesNoCancel)
                .show();

            let resolved = match choice {
                MessageDialogResult::Yes => Some(CloseBehavior::Tray),
                MessageDialogResult::No => Some(CloseBehavior::Exit),
                _ => None,
            };

            if let Some(mode) = resolved.clone() {
                prefs.close_behavior = Some(mode.clone());
                let _ = save_preferences(prefs_path, prefs);
                Some(mode)
            } else {
                None
            }
        }
    }
}

fn load_preferences(path: &Path) -> DesktopPreferences {
    fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save_preferences(path: &Path, prefs: &DesktopPreferences) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(path, serde_json::to_vec_pretty(prefs)?)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn desktop_prefs_path(workspace_root: &Path) -> PathBuf {
    workspace_root.join("data").join("spg_desktop_state.json")
}

#[allow(dead_code)]
fn desktop_launch_html(hold_ms: u64) -> String {
    let game_badges = SEEDED_GAMES
        .iter()
        .map(|game| format!(r#"<span class="launch-badge">{game}</span>"#))
        .collect::<Vec<_>>()
        .join("");
    let app_url = serde_json::to_string(APP_URL).expect("serialize app url");

    format!(
        r#"<!DOCTYPE html>
<html lang="zh-CN">
  <head>
    <meta charset="utf-8"/>
    <meta name="viewport" content="width=device-width, initial-scale=1.0"/>
    <title>SPG Desktop Launch</title>
    <style>
      :root {{
        color-scheme: dark;
        --bg-top: #060a1b;
        --bg-bottom: #050811;
        --panel: rgba(12, 18, 42, 0.84);
        --panel-strong: rgba(18, 27, 61, 0.9);
        --line: rgba(123, 156, 255, 0.22);
        --text: #f3f7ff;
        --text-soft: rgba(222, 232, 255, 0.76);
        --text-muted: rgba(177, 194, 235, 0.58);
        --brand: #7ab7ff;
        --brand-strong: #a5d7ff;
        --shadow: 0 28px 80px rgba(4, 8, 26, 0.45);
      }}
      * {{ box-sizing: border-box; }}
      html, body {{
        margin: 0;
        min-height: 100%;
        overflow: hidden;
        background:
          radial-gradient(circle at 18% 20%, rgba(108, 144, 255, 0.18), transparent 0 24%),
          radial-gradient(circle at 78% 16%, rgba(84, 147, 255, 0.16), transparent 0 22%),
          radial-gradient(circle at 50% 0%, rgba(108, 94, 255, 0.10), transparent 0 34%),
          linear-gradient(180deg, var(--bg-top) 0%, var(--bg-bottom) 100%);
        color: var(--text);
        font-family: "Segoe UI", "PingFang SC", "Microsoft YaHei", sans-serif;
      }}
      body {{
        position: relative;
      }}
      .launch-orbit,
      .launch-grid,
      .launch-haze,
      .launch-horizon {{
        position: absolute;
        pointer-events: none;
      }}
      .launch-orbit {{
        border: 1px solid rgba(122, 168, 255, 0.14);
        border-radius: 50%;
        transform: rotate(-16deg);
        animation: floatOrbit 16s ease-in-out infinite;
      }}
      .launch-orbit.one {{
        width: 54vw;
        height: 54vw;
        top: -18vw;
        right: -10vw;
      }}
      .launch-orbit.two {{
        width: 68vw;
        height: 68vw;
        top: -30vw;
        right: 8vw;
        animation-duration: 24s;
      }}
      .launch-grid {{
        inset: auto 0 0;
        height: 40vh;
        background-image:
          linear-gradient(rgba(126, 166, 255, 0.08) 1px, transparent 1px),
          linear-gradient(90deg, rgba(126, 166, 255, 0.08) 1px, transparent 1px);
        background-size: 54px 54px;
        mask-image: linear-gradient(180deg, transparent 0%, rgba(0, 0, 0, 0.2) 26%, rgba(0, 0, 0, 0.9) 100%);
      }}
      .launch-haze {{
        inset: 0;
        background:
          radial-gradient(circle at 22% 34%, rgba(90, 144, 255, 0.2), transparent 0 22%),
          radial-gradient(circle at 78% 24%, rgba(104, 92, 255, 0.14), transparent 0 24%),
          radial-gradient(circle at 50% 62%, rgba(77, 176, 255, 0.08), transparent 0 22%);
        filter: blur(18px);
        opacity: 0.86;
      }}
      .launch-horizon {{
        left: 50%;
        bottom: -26vh;
        width: 128vw;
        height: 58vh;
        transform: translateX(-50%);
        border-radius: 50% 50% 0 0;
        background:
          radial-gradient(circle at 50% 18%, rgba(194, 222, 255, 0.84), rgba(108, 150, 255, 0.22) 24%, rgba(9, 14, 32, 0) 60%),
          linear-gradient(180deg, rgba(11, 18, 48, 0) 0%, rgba(84, 122, 255, 0.08) 46%, rgba(7, 12, 28, 0.92) 100%);
        box-shadow:
          0 -42px 120px rgba(96, 137, 255, 0.12),
          inset 0 12px 40px rgba(212, 230, 255, 0.18);
      }}
      .launch-shell {{
        position: relative;
        z-index: 1;
        display: grid;
        grid-template-rows: auto 1fr auto;
        min-height: 100vh;
        padding: 24px 28px 28px;
        gap: 18px;
      }}
      .desktop-titlebar,
      .desktop-status,
      .desktop-stage,
      .desktop-preview {{
        border: 1px solid var(--line);
        background:
          linear-gradient(180deg, rgba(18, 27, 61, 0.94), rgba(10, 16, 38, 0.82)),
          radial-gradient(circle at top right, rgba(116, 165, 255, 0.10), transparent 0 28%);
        backdrop-filter: blur(16px);
        box-shadow: var(--shadow);
      }}
      .desktop-titlebar,
      .desktop-status {{
        border-radius: 26px;
        padding: 18px 22px;
      }}
      .desktop-titlebar {{
        display: flex;
        justify-content: space-between;
        align-items: center;
        gap: 16px;
      }}
      .desktop-title {{
        display: flex;
        align-items: center;
        gap: 14px;
      }}
      .desktop-logo {{
        width: 44px;
        height: 44px;
        border-radius: 14px;
        background:
          radial-gradient(circle at 34% 30%, rgba(255, 255, 255, 0.96), rgba(132, 196, 255, 0.9) 32%, rgba(63, 124, 255, 0.42) 68%, rgba(9, 14, 32, 0) 100%);
        box-shadow: 0 0 32px rgba(114, 180, 255, 0.24);
      }}
      .desktop-title h1,
      .desktop-stage h2,
      .desktop-preview h2 {{
        margin: 0;
        font-size: clamp(28px, 3.4vw, 46px);
        letter-spacing: -0.04em;
        line-height: 1;
      }}
      .eyebrow {{
        margin: 0 0 8px;
        color: var(--brand-strong);
        letter-spacing: 0.22em;
        font-size: 11px;
        font-weight: 800;
        text-transform: uppercase;
      }}
      .title-copy,
      .stage-copy,
      .status-copy,
      .preview-copy {{
        margin: 10px 0 0;
        color: var(--text-soft);
        line-height: 1.7;
      }}
      .title-copy {{
        max-width: 760px;
      }}
      .title-badges,
      .status-row,
      .launch-games,
      .stage-steps {{
        display: flex;
        flex-wrap: wrap;
        gap: 10px;
      }}
      .title-badge,
      .launch-badge,
      .status-pill,
      .stage-pill {{
        padding: 10px 14px;
        border-radius: 999px;
        border: 1px solid rgba(132, 166, 255, 0.2);
        background: rgba(8, 13, 34, 0.42);
        color: var(--text-soft);
        font-size: 13px;
      }}
      .title-badge strong,
      .status-pill strong {{
        color: var(--text);
      }}
      .desktop-main {{
        display: grid;
        grid-template-columns: 1.2fr 0.9fr;
        gap: 18px;
      }}
      .desktop-stage,
      .desktop-preview {{
        border-radius: 30px;
        padding: 28px;
      }}
      .stage-steps {{
        display: grid;
        gap: 14px;
        margin-top: 22px;
      }}
      .stage-step {{
        display: grid;
        grid-template-columns: 48px 1fr;
        gap: 14px;
        align-items: start;
        padding: 16px;
        border-radius: 20px;
        border: 1px solid rgba(122, 162, 255, 0.14);
        background: rgba(8, 13, 34, 0.42);
      }}
      .stage-step.active {{
        border-color: rgba(138, 197, 255, 0.38);
        box-shadow: inset 0 0 0 1px rgba(138, 197, 255, 0.18);
      }}
      .step-index {{
        display: inline-flex;
        justify-content: center;
        align-items: center;
        width: 40px;
        height: 40px;
        border-radius: 14px;
        background: linear-gradient(180deg, rgba(103, 161, 255, 0.32), rgba(52, 108, 255, 0.12));
        color: var(--brand-strong);
        font-weight: 700;
      }}
      .stage-step strong,
      .preview-metric strong {{
        display: block;
        font-size: 18px;
      }}
      .stage-step p,
      .preview-metric span,
      .launch-badge,
      .status-pill,
      .preview-copy {{
        color: var(--text-soft);
      }}
      .stage-step p,
      .preview-metric span {{
        margin: 8px 0 0;
        line-height: 1.6;
      }}
      .launch-games {{
        margin-top: 22px;
      }}
      .launch-badge {{
        border-radius: 16px;
        padding: 12px 14px;
        backdrop-filter: blur(12px);
      }}
      .preview-metrics {{
        display: grid;
        grid-template-columns: repeat(2, minmax(0, 1fr));
        gap: 14px;
        margin-top: 20px;
      }}
      .preview-metric {{
        padding: 16px;
        border-radius: 18px;
        border: 1px solid rgba(122, 162, 255, 0.14);
        background: rgba(8, 13, 34, 0.42);
      }}
      .launch-actions {{
        display: flex;
        gap: 12px;
        flex-wrap: wrap;
        margin-top: 24px;
      }}
      .launch-button,
      .launch-secondary {{
        appearance: none;
        border: none;
        cursor: pointer;
        border-radius: 16px;
        padding: 14px 22px;
        font-size: 14px;
        font-weight: 700;
      }}
      .launch-button {{
        background: linear-gradient(180deg, #6caeff 0%, #4a7bff 100%);
        color: #040916;
        box-shadow: 0 12px 30px rgba(79, 132, 255, 0.28);
      }}
      .launch-secondary {{
        background: rgba(8, 13, 34, 0.42);
        color: var(--text-soft);
        border: 1px solid rgba(122, 162, 255, 0.16);
      }}
      .launch-footer {{
        display: flex;
        justify-content: space-between;
        align-items: end;
        gap: 16px;
      }}
      .launch-note {{
        color: var(--text-muted);
        font-size: 12px;
        letter-spacing: 0.08em;
        text-transform: uppercase;
      }}
      body.launching .launch-shell {{
        opacity: 0.82;
        transform: scale(0.992);
        transition: opacity 240ms ease, transform 240ms ease;
      }}
      @keyframes floatOrbit {{
        0%, 100% {{ transform: rotate(-16deg) translate3d(0, 0, 0); }}
        50% {{ transform: rotate(-10deg) translate3d(0, 12px, 0); }}
      }}
      @media (max-width: 1080px) {{
        .desktop-main {{
          grid-template-columns: 1fr;
        }}
        .preview-metrics {{
          grid-template-columns: 1fr;
        }}
      }}
    </style>
  </head>
  <body>
    <div class="launch-orbit one"></div>
    <div class="launch-orbit two"></div>
    <div class="launch-haze"></div>
    <div class="launch-horizon"></div>
    <div class="launch-grid"></div>

    <main class="launch-shell">
      <section class="desktop-titlebar">
        <div class="desktop-title">
          <div class="desktop-logo"></div>
          <div>
            <p class="eyebrow">SPG DESKTOP COMMAND CENTER</p>
            <h1>桌面启动壳层</h1>
            <p class="title-copy">先以桌面启动页承接整个产品的第一眼体验。后台服务已经准备就绪，接下来会进入三段闸门和主系统工作台。</p>
          </div>
        </div>
        <div class="title-badges">
          <span class="title-badge"><strong>Platform</strong> Douyin</span>
          <span class="title-badge"><strong>Mode</strong> Local Desktop</span>
          <span class="title-badge"><strong>Engine</strong> Rust / Wry</span>
        </div>
      </section>

      <section class="desktop-main">
        <section class="desktop-stage">
          <p class="eyebrow">ENTRY FLOW</p>
          <h2>从桌面壳进入 SLG 指挥中心</h2>
          <p class="stage-copy">这一版先把桌面外层做出来，让整体更像一个真正的本地产品，而不是直接打开浏览器页面。</p>

          <div class="stage-steps">
            <article class="stage-step active">
              <span class="step-index">01</span>
              <div>
                <strong>桌面启动页</strong>
                <p>显示品牌、目标赛道、当前监控游戏与运行状态，形成统一的桌面第一屏。</p>
              </div>
            </article>
            <article class="stage-step">
              <span class="step-index">02</span>
              <div>
                <strong>登录与 API 闸门</strong>
                <p>继续沿用三段递进逻辑，确认本地身份并校验必需 API 后再进入主系统。</p>
              </div>
            </article>
            <article class="stage-step">
              <span class="step-index">03</span>
              <div>
                <strong>SLG 工作台</strong>
                <p>进入抖音 SLG 内容排行榜、拆解和选题池，后续就直接在这里稳定产出。</p>
              </div>
            </article>
          </div>

          <div class="launch-games">{game_badges}</div>
        </section>

        <aside class="desktop-preview">
          <p class="eyebrow">SYSTEM STATUS</p>
          <h2>桌面版正在接入主系统</h2>
          <p id="launch-status-copy" class="preview-copy">后台服务已就绪。你现在看到的是桌面壳层首屏，稍后会自动进入系统，也可以手动立即进入。</p>

          <div class="preview-metrics">
            <div class="preview-metric">
              <strong>7 个种子游戏</strong>
              <span>首批监控对象已经预设进工作台默认 watchlist。</span>
            </div>
            <div class="preview-metric">
              <strong>Rust 本地运行</strong>
              <span>桌面启动器、调度和主工作流都继续走 Rust 主链路。</span>
            </div>
            <div class="preview-metric">
              <strong>三段闸门</strong>
              <span>登录、API 校验、主系统页面保持严格递进。</span>
            </div>
            <div class="preview-metric">
              <strong>无终端弹窗</strong>
              <span>点击桌面图标后默认静默启动，不显示 PowerShell 窗口。</span>
            </div>
          </div>

          <div class="launch-actions">
            <button id="enter-app-btn" class="launch-button" type="button">立即进入系统</button>
            <button id="delay-app-btn" class="launch-secondary" type="button">停留看壳层</button>
          </div>
        </aside>
      </section>

      <footer class="launch-footer">
        <div class="status-row">
          <span class="status-pill"><strong>Seed Track</strong> SLG</span>
          <span class="status-pill"><strong>Entry</strong> Desktop First</span>
          <span class="status-pill"><strong>Auto Continue</strong> <span id="launch-countdown">{hold_ms}</span> ms</span>
        </div>
        <div class="launch-note">SPG Desktop Shell Preview</div>
      </footer>
    </main>

    <script>
      const appUrl = {app_url};
      const initialDelay = {hold_ms};
      const countdownNode = document.getElementById("launch-countdown");
      const statusCopy = document.getElementById("launch-status-copy");
      const enterButton = document.getElementById("enter-app-btn");
      const delayButton = document.getElementById("delay-app-btn");
      let navigateTimer = null;
      let countdownTimer = null;
      let remaining = initialDelay;
      let paused = false;
      let launched = false;

      function syncCountdown() {{
        if (countdownNode) countdownNode.textContent = String(Math.max(remaining, 0));
      }}

      function navigateToApp() {{
        if (launched) return;
        launched = true;
        document.body.classList.add("launching");
        if (statusCopy) statusCopy.textContent = "正在切换到主系统工作台…";
        window.setTimeout(() => window.location.replace(appUrl), 280);
      }}

      function armNavigation(delay) {{
        window.clearTimeout(navigateTimer);
        window.clearInterval(countdownTimer);
        remaining = delay;
        syncCountdown();
        countdownTimer = window.setInterval(() => {{
          if (paused) return;
          remaining -= 100;
          syncCountdown();
        }}, 100);
        navigateTimer = window.setTimeout(() => {{
          window.clearInterval(countdownTimer);
          navigateToApp();
        }}, delay);
      }}

      enterButton?.addEventListener("click", navigateToApp);
      delayButton?.addEventListener("click", () => {{
        paused = !paused;
        delayButton.textContent = paused ? "继续自动进入" : "停留看壳层";
        if (statusCopy) {{
          statusCopy.textContent = paused
            ? "已暂停自动跳转，你可以先观察桌面壳层。"
            : "已恢复自动跳转，稍后会进入主系统工作台。";
        }}
      }});

      armNavigation(initialDelay);
    </script>
  </body>
</html>"#,
        game_badges = game_badges,
        app_url = app_url,
        hold_ms = hold_ms,
    )
}

fn spawn_web_server_if_needed(workspace_root: PathBuf) {
    if fetch_local_gate_state().is_ok() || port_is_occupied() {
        return;
    }

    let log_root = workspace_root.clone();
    thread::spawn(move || {
        let result = Builder::new_multi_thread()
            .enable_all()
            .build()
            .context("failed to create tokio runtime")
            .and_then(|runtime| {
                runtime.block_on(async move {
                    let state = spg_web::app_state_for_workspace(&workspace_root)?;
                    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
                        .await
                        .context("failed to bind desktop web service")?;
                    spg_web::run_on(listener, state).await
                })
            });

        if let Err(error) = result {
            let _ = append_error_log(&log_root, &format!("{error:#}"));
        }
    });
}

fn wait_for_server_ready() -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let probe_error = match fetch_local_gate_state() {
            Ok(_) => return Ok(()),
            Err(error) => error.to_string(),
        };
        if Instant::now() >= deadline {
            if port_is_occupied() {
                anyhow::bail!(
                    "127.0.0.1:3000 responded, but it was not a valid SPG service: {}",
                    probe_error
                );
            }
            anyhow::bail!("timed out waiting for SPG on 127.0.0.1:3000");
        }
        thread::sleep(Duration::from_millis(250));
    }
}

fn show_window(window: &tao::window::Window) {
    window.set_visible(true);
    let _ = window.set_focus();
    window.request_user_attention(None);
}

fn hide_window(window: &tao::window::Window) {
    window.set_visible(false);
}

fn post_local(path: &str) -> Result<()> {
    let response = send_local_request("POST", path)?;
    if !(200..400).contains(&response.status_code) {
        anyhow::bail!(
            "local request to {path} returned HTTP {}",
            response.status_code
        );
    }
    Ok(())
}

fn fetch_local_gate_state() -> Result<spg_web::models::GateStateView> {
    let response = send_local_request("GET", "/api/gate-state")?;
    if response.status_code != 200 {
        anyhow::bail!(
            "gate state probe returned unexpected HTTP status {}",
            response.status_code
        );
    }
    serde_json::from_slice(&response.body).context("failed to parse SPG gate-state response")
}

fn send_local_request(method: &str, path: &str) -> Result<LocalHttpResponse> {
    let mut stream = TcpStream::connect("127.0.0.1:3000")
        .with_context(|| format!("failed to connect to local SPG service for {method} {path}"))?;
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:3000\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .with_context(|| format!("failed to send local request to {path}"))?;

    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .with_context(|| format!("failed to read local response from {path}"))?;

    parse_local_response(&response)
}

fn parse_local_response(raw: &[u8]) -> Result<LocalHttpResponse> {
    let Some(header_end) = raw.windows(4).position(|window| window == b"\r\n\r\n") else {
        anyhow::bail!("invalid HTTP response: missing header terminator");
    };
    let headers = String::from_utf8_lossy(&raw[..header_end]);
    let status_line = headers
        .lines()
        .next()
        .context("invalid HTTP response: missing status line")?;
    let status_code = status_line
        .split_whitespace()
        .nth(1)
        .context("invalid HTTP response: missing status code")?
        .parse::<u16>()
        .context("invalid HTTP response: malformed status code")?;

    Ok(LocalHttpResponse {
        status_code,
        body: raw[header_end + 4..].to_vec(),
    })
}

fn port_is_occupied() -> bool {
    TcpStream::connect("127.0.0.1:3000").is_ok()
}

fn append_error_log(workspace_root: &Path, message: &str) -> Result<()> {
    let path = workspace_root.join("data").join("spg-desktop.err.log");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    writeln!(file, "{message}")?;
    Ok(())
}

fn app_icon_rgba() -> (Vec<u8>, u32, u32) {
    let width = 32;
    let height = 32;
    let mut rgba = vec![0_u8; (width * height * 4) as usize];

    for y in 0..height {
        for x in 0..width {
            let index = ((y * width + x) * 4) as usize;
            let dx = x as f32 - 16.0;
            let dy = y as f32 - 16.0;
            let distance = (dx * dx + dy * dy).sqrt();
            let alpha = if distance <= 14.0 { 255 } else { 0 };
            let glow = if distance <= 10.0 { 220 } else { 130 };
            rgba[index] = 64;
            rgba[index + 1] = glow;
            rgba[index + 2] = 255;
            rgba[index + 3] = alpha;
        }
    }

    (rgba, width, height)
}

#[cfg(test)]
mod tests {
    use super::{desktop_launch_html, parse_local_response, APP_URL, SEEDED_GAMES};

    #[test]
    fn parses_http_status_and_body() {
        let response = parse_local_response(
            b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"current_stage\":\"login\"}",
        )
        .expect("parse response");

        assert_eq!(response.status_code, 200);
        assert_eq!(
            String::from_utf8(response.body).expect("utf8 body"),
            "{\"current_stage\":\"login\"}"
        );
    }

    #[test]
    fn rejects_non_http_responses() {
        assert!(parse_local_response(b"not-http").is_err());
    }

    #[test]
    fn launch_shell_includes_entry_url_and_seed_games() {
        let html = desktop_launch_html(1200);
        assert!(html.contains(APP_URL));
        for game in SEEDED_GAMES {
            assert!(html.contains(game));
        }
    }
}
