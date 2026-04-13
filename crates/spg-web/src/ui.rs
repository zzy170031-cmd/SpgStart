use crate::models::{GalaxyPageView, ProviderStatusView, SetupPageView};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use leptos::*;

const ASSET_VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "-wireframe");

pub fn render_setup_page(page: SetupPageView) -> String {
    let state = STANDARD.encode(serde_json::to_string(&page).expect("serialize setup page"));
    let html = leptos::ssr::render_to_string(move || {
        view! {
            <html lang="zh-CN">
                <head>
                    <meta charset="utf-8"/>
                    <meta name="viewport" content="width=device-width, initial-scale=1.0"/>
                    <title>"SPG v3 | Provider Setup"</title>
                    <link rel="stylesheet" href=format!("/assets/app.css?v={ASSET_VERSION}")/>
                </head>
                <body class="page-shell page-setup">
                    <div id="setup-root" data-state=state data-mode=page.mode.clone() hidden></div>
                    <header class="hero hero-setup">
                        <div>
                            <p class="eyebrow">"SPG v3 / Provider Access"</p>
                            <h1>"先授权配置，再进入星系主系统"</h1>
                            <p class="hero-copy">
                                "当前首发只做 SLG。六个 Provider 全部保存并测试通过后，主系统才会解锁。"
                            </p>
                        </div>
                        <div class="hero-metrics">
                            <div class="metric-card">
                                <span class="metric-label">"Ready"</span>
                                <strong>{format!("{}/{}", page.unlock_state.ready_count, page.unlock_state.total_count)}</strong>
                            </div>
                            <div class="metric-card">
                                <span class="metric-label">"Unlock"</span>
                                <strong>{if page.unlock_state.unlocked { "已解锁" } else { "锁定中" }}</strong>
                            </div>
                        </div>
                    </header>

                    <main class="setup-grid">
                        <aside class="setup-steps">
                            <p class="section-label">"Launch Flow"</p>
                            <ol class="step-list">
                                <li class="step-item active">
                                    <span class="step-index">"01"</span>
                                    <div>
                                        <strong>"录入 Provider"</strong>
                                        <p>"填写 API Key、基地址、配额和角色优先级。"</p>
                                    </div>
                                </li>
                                <li class="step-item active">
                                    <span class="step-index">"02"</span>
                                    <div>
                                        <strong>"逐个测试"</strong>
                                        <p>"单 Provider 可独立验证，也支持一键全测。"</p>
                                    </div>
                                </li>
                                <li class="step-item">
                                    <span class="step-index">"03"</span>
                                    <div>
                                        <strong>"解锁星系"</strong>
                                        <p>"六个 Provider 全部 Ready 后进入 `/galaxy`。"</p>
                                    </div>
                                </li>
                            </ol>
                        </aside>

                        <section class="setup-center">
                            <div class="section-heading">
                                <div>
                                    <p class="section-label">"Providers"</p>
                                    <h2>"统一授权与调度配置"</h2>
                                </div>
                                <div class="section-actions">
                                    <a class="ghost-link" href="/settings/providers">"设置页"</a>
                                    <button id="test-all-btn" class="primary-button" type="button">"全部测试"</button>
                                </div>
                            </div>

                            <div id="provider-cards" class="provider-card-grid">
                                {page.providers.iter().cloned().map(render_provider_card).collect::<Vec<_>>()}
                            </div>
                        </section>

                        <aside class="setup-status">
                            <div id="unlock-panel" class="status-panel">
                                <p class="section-label">"Unlock State"</p>
                                <h3>{format!("{} / {} Ready", page.unlock_state.ready_count, page.unlock_state.total_count)}</h3>
                                <p class="status-copy">
                                    {if page.unlock_state.unlocked {
                                        "全部 Provider 已通过测试，你现在可以进入主系统。"
                                    } else {
                                        "仍有 Provider 未完成授权或最近测试失败，主系统保持锁定。"
                                    }}
                                </p>
                                <ul class="blocker-list">
                                    {page.unlock_state.blockers.iter().map(|blocker| view! { <li>{blocker.clone()}</li> }).collect::<Vec<_>>()}
                                </ul>
                                <div class="unlock-actions">
                                    <a class="secondary-button" href="/setup/providers">"刷新当前页"</a>
                                    <a class=if page.unlock_state.unlocked { "primary-button inline-link" } else { "primary-button inline-link disabled-link" } href=if page.unlock_state.unlocked { "/galaxy" } else { "#" }>"进入主系统"</a>
                                </div>
                            </div>
                        </aside>
                    </main>

                    <script type="module" src=format!("/assets/setup.js?v={ASSET_VERSION}")></script>
                </body>
            </html>
        }
    });
    format!("<!DOCTYPE html>{html}")
}

pub fn render_galaxy_page(page: GalaxyPageView) -> String {
    let state = STANDARD.encode(serde_json::to_string(&page).expect("serialize galaxy page"));
    let html = leptos::ssr::render_to_string(move || {
        view! {
            <html lang="zh-CN">
                <head>
                    <meta charset="utf-8"/>
                    <meta name="viewport" content="width=device-width, initial-scale=1.0"/>
                    <title>"SPG v3 | Galaxy"</title>
                    <link rel="stylesheet" href=format!("/assets/app.css?v={ASSET_VERSION}")/>
                </head>
                <body class="page-shell page-galaxy">
                    <div id="galaxy-root" data-state=state hidden></div>
                    <header class="hero hero-galaxy">
                        <div>
                            <p class="eyebrow">"SPG v3 / SLG Galaxy"</p>
                            <h1>{page.summary.headline.clone()}</h1>
                            <p class="hero-copy">{page.summary.subheadline.clone()}</p>
                        </div>
                        <div class="hero-actions">
                            <div class="summary-chip">
                                <span>"游戏"</span>
                                <strong>{page.summary.game_count}</strong>
                            </div>
                            <div class="summary-chip">
                                <span>"事件"</span>
                                <strong>{page.summary.event_count}</strong>
                            </div>
                            <div class="summary-chip">
                                <span>"来源"</span>
                                <strong>{page.summary.source_count}</strong>
                            </div>
                            <a class="secondary-button inline-link" href="/settings/providers">"Provider 设置"</a>
                        </div>
                    </header>

                    <main class="galaxy-layout">
                        <aside class="galaxy-sidebar">
                            <div class="sidebar-header">
                                <p class="section-label">"Explorer"</p>
                                <h2>"左列表 · 右星系"</h2>
                            </div>

                            <div class="tab-strip">
                                <button class="tab-button active" data-tab="games" type="button">"游戏实体"</button>
                                <button class="tab-button" data-tab="events" type="button">"热点事件"</button>
                            </div>

                            <div id="galaxy-list" class="list-panel">
                                {page.games.iter().cloned().map(render_game_list_item).collect::<Vec<_>>()}
                            </div>
                        </aside>

                        <section class="galaxy-main">
                            <div class="galaxy-card">
                                <div class="galaxy-card-header">
                                    <div>
                                        <p class="section-label">"Galaxy View"</p>
                                        <h2 id="focus-title">{page.initial_focus.title.clone()}</h2>
                                        <p id="focus-subtitle" class="focus-subtitle">{page.initial_focus.subtitle.clone()}</p>
                                    </div>
                                    <div id="layer-filters" class="filter-strip">
                                        <button class="filter-chip active" data-layer="games" type="button">"游戏"</button>
                                        <button class="filter-chip active" data-layer="events" type="button">"事件"</button>
                                        <button class="filter-chip active" data-layer="sources" type="button">"来源"</button>
                                    </div>
                                </div>

                                <div class="galaxy-canvas-shell">
                                    <svg id="galaxy-svg" viewBox="0 0 960 620" role="img" aria-label="SLG galaxy visualization"></svg>
                                </div>
                            </div>

                            <div class="insight-grid">
                                <div class="insight-card">
                                    <p class="section-label">"Narrative"</p>
                                    <ul id="focus-narrative" class="narrative-list">
                                        {page.initial_focus.narrative.iter().map(|item| view! { <li>{item.clone()}</li> }).collect::<Vec<_>>()}
                                    </ul>
                                </div>
                                <div class="insight-card">
                                    <p class="section-label">"System Status"</p>
                                    <div class="status-stack">
                                        <div class="mini-stat">
                                            <span>"已解锁"</span>
                                            <strong>{if page.unlock_state.unlocked { "是" } else { "否" }}</strong>
                                        </div>
                                        <div class="mini-stat">
                                            <span>"Provider Ready"</span>
                                            <strong>{format!("{}/{}", page.unlock_state.ready_count, page.unlock_state.total_count)}</strong>
                                        </div>
                                        <div class="mini-stat">
                                            <span>"最近更新"</span>
                                            <strong>{page.summary.last_updated.clone()}</strong>
                                        </div>
                                    </div>
                                </div>
                            </div>
                        </section>
                    </main>

                    <script type="module" src=format!("/assets/galaxy_wire.js?v={ASSET_VERSION}")></script>
                </body>
            </html>
        }
    });
    format!("<!DOCTYPE html>{html}")
}

fn render_provider_card(provider: ProviderStatusView) -> impl IntoView {
    let status_class = format!("status-pill status-{}", provider.status.replace('_', "-"));
    let latency = provider
        .latency_ms
        .map(|value| format!("{value} ms"))
        .unwrap_or_else(|| "未测试".to_string());
    let verified = provider
        .last_verified_at
        .clone()
        .unwrap_or_else(|| "尚未验证".to_string());
    let last_error = provider
        .last_error
        .clone()
        .unwrap_or_else(|| "暂无错误".to_string());
    let saved_key_copy = if provider.api_key_summary.is_none() {
        "未保存任何 Key".to_string()
    } else {
        format!(
            "已保存：{}。输入新值后保存才会覆盖当前 Key。",
            provider.api_key_summary.clone().unwrap_or_default()
        )
    };

    view! {
        <article
            class="provider-card"
            data-provider=provider.provider.clone()
            data-has-api-key=if provider.has_api_key { "true" } else { "false" }
        >
            <div class="provider-card-top">
                <div>
                    <p class="provider-name">{provider.display_name.clone()}</p>
                    <p class="provider-desc">{provider.description.clone()}</p>
                </div>
                <span class=status_class>{provider.status.clone()}</span>
            </div>

            <div class="form-grid">
                <label class="field">
                    <span>"启用"</span>
                    <input class="toggle-input" name="enabled" type="checkbox" checked=provider.enabled/>
                </label>
                <label class="field field-wide">
                    <span>"API Key"</span>
                    <input
                        class="text-input"
                        name="api_key"
                        type="password"
                        value=""
                        placeholder="输入新的 API Key"
                        autocomplete="new-password"
                        spellcheck="false"
                        data-lpignore="true"
                        data-1p-ignore="true"
                    />
                    <small class="field-hint">{saved_key_copy}</small>
                </label>
                <label class="field">
                    <span>"清除 Key"</span>
                    <input class="toggle-input" name="clear_api_key" type="checkbox"/>
                </label>
                <label class="field field-wide">
                    <span>"Base URL"</span>
                    <input class="text-input" name="base_url" type="text" value=provider.base_url.clone()/>
                </label>
                <label class="field">
                    <span>"Monthly Limit"</span>
                    <input class="text-input" name="monthly_limit" type="number" value=provider.monthly_limit.to_string()/>
                </label>
                <label class="field">
                    <span>"Daily Soft Limit"</span>
                    <input class="text-input" name="daily_soft_limit" type="number" value=provider.daily_soft_limit.to_string()/>
                </label>
                <label class="field field-wide">
                    <span>"Default Role"</span>
                    <input class="text-input" name="default_role" type="text" value=provider.default_role.clone()/>
                </label>
                <label class="field">
                    <span>"Priority"</span>
                    <input class="text-input" name="priority" type="number" value=provider.priority.to_string()/>
                </label>
            </div>

            <div class="provider-meta">
                <div>
                    <span class="meta-label">"最近验证"</span>
                    <strong>{verified}</strong>
                </div>
                <div>
                    <span class="meta-label">"延迟"</span>
                    <strong>{latency}</strong>
                </div>
                <div class="provider-error">
                    <span class="meta-label">"最近错误"</span>
                    <strong>{last_error}</strong>
                </div>
            </div>

            <div class="provider-actions">
                <button class="secondary-button action-button" data-action="save" type="button">"保存配置"</button>
                <button class="primary-button action-button" data-action="test" type="button">"测试 Provider"</button>
            </div>
        </article>
    }
}

fn render_game_list_item(game: crate::models::GalaxyGameListItem) -> impl IntoView {
    let aliases = if game.aliases.is_empty() {
        "暂无别名".to_string()
    } else {
        game.aliases.join(" / ")
    };

    view! {
        <button class="list-item" data-kind="game" data-id=game.id.clone() type="button">
            <div class="list-item-top">
                <strong>{game.name.clone()}</strong>
                <span class="list-pill">{game.stage.clone()}</span>
            </div>
            <p>{game.note.clone()}</p>
            <div class="list-item-meta">
                <span>{game.studio.clone()}</span>
                <span>{aliases}</span>
                <span>{format!("{} signals", game.signal_count)}</span>
            </div>
        </button>
    }
}
