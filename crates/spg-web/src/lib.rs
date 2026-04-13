pub mod galaxy;
pub mod models;
pub mod secret_store;
pub mod store;
pub mod ui;
pub mod ui_desktop;
pub mod ui_shell;
pub mod ui_v2;
pub mod ui_workbench;
pub mod ui_workbench_v2;
pub mod workbench;

use crate::galaxy::{core_db_path, GalaxyService};
use crate::models::{
    DesktopLauncherPageView, GalaxyFocusView, GalaxyListResponse, GateStateView, LoginPageView,
    ProviderKind, ProviderUpdateRequest, SetupPageView, TopicPoolCreateRequest, WorkbenchPageView,
    WorkbenchSettings,
};
use crate::secret_store::{HybridSecretStore, SecretStore};
use crate::store::{provider_db_path, ProviderStore};
use crate::ui_desktop::render_desktop_launcher_page;
use crate::ui_shell::{render_login_page, render_setup_page};
use crate::ui_workbench_v2::render_workbench_page;
use crate::workbench::{HttpProviderGateway, RankingQuery, WorkbenchService};
use anyhow::{Context, Result};
use axum::extract::{Path as RoutePath, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::services::ServeDir;

#[derive(Clone)]
pub struct AppState {
    provider_store: ProviderStore,
    galaxy_service: GalaxyService,
    workbench_service: WorkbenchService,
}

impl AppState {
    pub fn new(
        provider_db: PathBuf,
        core_db: PathBuf,
        secret_store: Arc<dyn SecretStore>,
    ) -> Result<Self> {
        Self::new_with_gateway(
            provider_db,
            core_db,
            secret_store,
            Arc::new(HttpProviderGateway::default()),
        )
    }

    pub(crate) fn new_with_gateway(
        provider_db: PathBuf,
        core_db: PathBuf,
        secret_store: Arc<dyn SecretStore>,
        gateway: Arc<dyn crate::workbench::ProviderGateway>,
    ) -> Result<Self> {
        let provider_store = ProviderStore::new(provider_db, secret_store);
        provider_store.initialize()?;
        let workbench_service =
            WorkbenchService::new(core_db.clone(), provider_store.clone(), gateway)?;

        Ok(Self {
            galaxy_service: GalaxyService::new(core_db),
            provider_store,
            workbench_service,
        })
    }
}

pub fn build_app(state: AppState) -> Router {
    Router::new()
        .route("/", get(root_redirect))
        .route("/desktop/launcher", get(desktop_launcher_page))
        .route("/login", get(login_page))
        .route("/setup/providers", get(setup_page))
        .route("/settings/providers", get(settings_page))
        .route("/galaxy", get(galaxy_page))
        .route("/api/local-user", get(local_user))
        .route("/api/gate-state", get(gate_state))
        .route("/api/auth/confirm", post(confirm_local_user))
        .route("/api/auth/logout", post(logout))
        .route("/api/gate/advance", post(advance_api_gate))
        .route("/api/providers", get(list_providers))
        .route("/api/providers/test-all", post(test_all_providers))
        .route("/api/providers/{provider}/test", post(test_provider))
        .route("/api/providers/{provider}", put(update_provider))
        .route("/api/unlock-state", get(unlock_state))
        .route("/api/workbench/overview", get(workbench_overview))
        .route("/api/workbench/rankings", get(workbench_rankings))
        .route("/api/workbench/content/{id}", get(workbench_content))
        .route("/api/workbench/runs", get(workbench_runs))
        .route("/api/workbench/runs/run-now", post(workbench_run_now))
        .route(
            "/api/workbench/topic-pool",
            get(workbench_topic_pool).post(workbench_topic_pool_add),
        )
        .route(
            "/api/workbench/topic-pool/{id}",
            axum::routing::delete(workbench_topic_pool_remove),
        )
        .route(
            "/api/workbench/topic-pool/export",
            get(workbench_topic_pool_export),
        )
        .route(
            "/api/workbench/settings",
            get(workbench_settings).put(workbench_settings_update),
        )
        .route("/api/galaxy/list", get(galaxy_list))
        .route("/api/galaxy/focus/game/{id}", get(galaxy_focus_game))
        .route("/api/galaxy/focus/event/{id}", get(galaxy_focus_event))
        .route("/api/galaxy/summary", get(galaxy_summary))
        .nest_service("/assets", ServeDir::new(asset_dir()))
        .with_state(state)
}

pub async fn run() -> Result<()> {
    let workspace_root = workspace_root()?;
    let state = app_state_for_workspace(&workspace_root)?;

    let listener = TcpListener::bind("127.0.0.1:3000")
        .await
        .context("failed to bind to 127.0.0.1:3000")?;
    run_on(listener, state).await
}

pub async fn run_on(listener: TcpListener, state: AppState) -> Result<()> {
    println!("SPG v3 web app listening on http://127.0.0.1:3000");
    state.workbench_service.start_scheduler();
    axum::serve(listener, build_app(state))
        .await
        .context("axum server stopped unexpectedly")?;
    Ok(())
}

pub fn app_state_for_workspace(workspace_root: &Path) -> Result<AppState> {
    AppState::new(
        provider_db_path(workspace_root),
        core_db_path(workspace_root),
        Arc::new(HybridSecretStore::new(workspace_root)),
    )
}

async fn root_redirect(State(state): State<AppState>) -> Response {
    match state.provider_store.gate_state() {
        Ok(gate_state) => Redirect::temporary(root_destination(&gate_state)).into_response(),
        Err(error) => internal_error(error),
    }
}

async fn login_page(State(state): State<AppState>) -> Response {
    match build_login_page(&state) {
        Ok(page) if page.gate_state.local_user_confirmed => {
            Redirect::temporary(root_destination(&page.gate_state)).into_response()
        }
        Ok(page) => Html(render_login_page(page)).into_response(),
        Err(error) => internal_error(error),
    }
}

async fn desktop_launcher_page(State(state): State<AppState>) -> Response {
    match build_desktop_launcher_page(&state) {
        Ok(page) => Html(render_desktop_launcher_page(page)).into_response(),
        Err(error) => internal_error(error),
    }
}

async fn setup_page(State(state): State<AppState>) -> Response {
    match state.provider_store.gate_state() {
        Ok(gate_state) if !gate_state.local_user_confirmed => {
            Redirect::temporary("/login").into_response()
        }
        Ok(_) => match build_setup_page(&state, "setup").map(render_setup_page) {
            Ok(markup) => Html(markup).into_response(),
            Err(error) => internal_error(error),
        },
        Err(error) => internal_error(error),
    }
}

async fn settings_page(State(state): State<AppState>) -> Response {
    match state.provider_store.gate_state() {
        Ok(gate_state) if !gate_state.local_user_confirmed => {
            Redirect::temporary("/login").into_response()
        }
        Ok(_) => match build_setup_page(&state, "settings").map(render_setup_page) {
            Ok(markup) => Html(markup).into_response(),
            Err(error) => internal_error(error),
        },
        Err(error) => internal_error(error),
    }
}

async fn galaxy_page(State(state): State<AppState>) -> Response {
    match ensure_main_system_access(&state) {
        Ok(unlock_state) => match build_workbench_page(&state, unlock_state) {
            Ok(page) => Html(render_workbench_page(page)).into_response(),
            Err(error) => internal_error(error),
        },
        Err(response) => response,
    }
}

async fn local_user(State(state): State<AppState>) -> Response {
    Json(state.provider_store.local_user()).into_response()
}

async fn gate_state(State(state): State<AppState>) -> Response {
    match state.provider_store.gate_state() {
        Ok(result) => Json(result).into_response(),
        Err(error) => internal_error(error),
    }
}

async fn confirm_local_user(State(state): State<AppState>) -> Response {
    match state.provider_store.confirm_local_user() {
        Ok(result) => Json(result).into_response(),
        Err(error) => internal_error(error),
    }
}

async fn logout(State(state): State<AppState>) -> Response {
    match state.provider_store.logout() {
        Ok(result) => Json(result).into_response(),
        Err(error) => internal_error(error),
    }
}

async fn advance_api_gate(State(state): State<AppState>) -> Response {
    match state.provider_store.advance_api_gate() {
        Ok(result) => Json(result).into_response(),
        Err(error) => bad_request(error.to_string()),
    }
}

async fn list_providers(State(state): State<AppState>) -> Response {
    if let Err(response) = ensure_login_confirmed(&state) {
        return response;
    }
    match state.provider_store.list_provider_views() {
        Ok(providers) => Json(providers).into_response(),
        Err(error) => internal_error(error),
    }
}

async fn update_provider(
    State(state): State<AppState>,
    RoutePath(provider): RoutePath<String>,
    Json(request): Json<ProviderUpdateRequest>,
) -> Response {
    if let Err(response) = ensure_login_confirmed(&state) {
        return response;
    }
    let provider = match provider.parse::<ProviderKind>() {
        Ok(provider) => provider,
        Err(error) => return bad_request(error),
    };

    match state.provider_store.save_provider(provider, request) {
        Ok(view) => Json(view).into_response(),
        Err(error) => internal_error(error),
    }
}

async fn test_provider(
    State(state): State<AppState>,
    RoutePath(provider): RoutePath<String>,
) -> Response {
    if let Err(response) = ensure_login_confirmed(&state) {
        return response;
    }
    let provider = match provider.parse::<ProviderKind>() {
        Ok(provider) => provider,
        Err(error) => return bad_request(error),
    };

    match state.provider_store.test_provider(provider) {
        Ok(result) => Json(result).into_response(),
        Err(error) => internal_error(error),
    }
}

async fn test_all_providers(State(state): State<AppState>) -> Response {
    if let Err(response) = ensure_login_confirmed(&state) {
        return response;
    }
    match state.provider_store.test_all() {
        Ok(results) => Json(results).into_response(),
        Err(error) => internal_error(error),
    }
}

async fn unlock_state(State(state): State<AppState>) -> Response {
    if let Err(response) = ensure_login_confirmed(&state) {
        return response;
    }
    match state.provider_store.unlock_state() {
        Ok(result) => Json(result).into_response(),
        Err(error) => internal_error(error),
    }
}

async fn workbench_overview(
    State(state): State<AppState>,
    Query(_query): Query<RankingQuery>,
) -> Response {
    if let Err(response) = ensure_galaxy_unlocked(&state) {
        return response;
    }
    match state.workbench_service.overview() {
        Ok(result) => Json(result).into_response(),
        Err(error) => internal_error(error),
    }
}

async fn workbench_rankings(
    State(state): State<AppState>,
    Query(query): Query<RankingQuery>,
) -> Response {
    if let Err(response) = ensure_galaxy_unlocked(&state) {
        return response;
    }
    let kind = query.kind.clone().unwrap_or_else(|| "content".to_string());
    match state.workbench_service.rankings(&kind, &query) {
        Ok(result) => Json(result).into_response(),
        Err(error) => internal_error(error),
    }
}

async fn workbench_content(
    State(state): State<AppState>,
    RoutePath(id): RoutePath<String>,
) -> Response {
    if let Err(response) = ensure_galaxy_unlocked(&state) {
        return response;
    }
    match state.workbench_service.content(&id) {
        Ok(Some(result)) => Json(result).into_response(),
        Ok(None) => not_found("unknown workbench content"),
        Err(error) => internal_error(error),
    }
}

async fn workbench_runs(State(state): State<AppState>) -> Response {
    if let Err(response) = ensure_galaxy_unlocked(&state) {
        return response;
    }
    match state.workbench_service.list_runs() {
        Ok(result) => Json(result).into_response(),
        Err(error) => internal_error(error),
    }
}

async fn workbench_run_now(State(state): State<AppState>) -> Response {
    if let Err(response) = ensure_galaxy_unlocked(&state) {
        return response;
    }
    match state.workbench_service.run_now().await {
        Ok(result) => Json(result).into_response(),
        Err(error) => internal_error(error),
    }
}

async fn workbench_topic_pool(State(state): State<AppState>) -> Response {
    if let Err(response) = ensure_galaxy_unlocked(&state) {
        return response;
    }
    match state.workbench_service.topic_pool() {
        Ok(result) => Json(result).into_response(),
        Err(error) => internal_error(error),
    }
}

async fn workbench_topic_pool_add(
    State(state): State<AppState>,
    Json(request): Json<TopicPoolCreateRequest>,
) -> Response {
    if let Err(response) = ensure_galaxy_unlocked(&state) {
        return response;
    }
    match state.workbench_service.add_topic_pool(request) {
        Ok(result) => Json(result).into_response(),
        Err(error) => internal_error(error),
    }
}

async fn workbench_topic_pool_remove(
    State(state): State<AppState>,
    RoutePath(id): RoutePath<String>,
) -> Response {
    if let Err(response) = ensure_galaxy_unlocked(&state) {
        return response;
    }
    match state.workbench_service.remove_topic_pool(&id) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => internal_error(error),
    }
}

async fn workbench_topic_pool_export(State(state): State<AppState>) -> Response {
    if let Err(response) = ensure_galaxy_unlocked(&state) {
        return response;
    }
    match state.workbench_service.export_topic_pool_csv() {
        Ok(csv) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "text/csv; charset=utf-8")],
            csv,
        )
            .into_response(),
        Err(error) => internal_error(error),
    }
}

async fn workbench_settings(State(state): State<AppState>) -> Response {
    if let Err(response) = ensure_galaxy_unlocked(&state) {
        return response;
    }
    match state.workbench_service.settings() {
        Ok(result) => Json(result).into_response(),
        Err(error) => internal_error(error),
    }
}

async fn workbench_settings_update(
    State(state): State<AppState>,
    Json(request): Json<WorkbenchSettings>,
) -> Response {
    if let Err(response) = ensure_galaxy_unlocked(&state) {
        return response;
    }
    match state.workbench_service.update_settings(request) {
        Ok(result) => Json(result).into_response(),
        Err(error) => internal_error(error),
    }
}

#[derive(Debug, Deserialize)]
struct GalaxyListQuery {
    tab: Option<String>,
}

async fn galaxy_list(
    State(state): State<AppState>,
    Query(query): Query<GalaxyListQuery>,
) -> Response {
    if let Err(response) = ensure_galaxy_unlocked(&state) {
        return response;
    }
    let tab = query.tab.unwrap_or_else(|| "games".to_string());
    match state.galaxy_service.list(&tab) {
        Ok(result) => Json(result).into_response(),
        Err(error) => internal_error(error),
    }
}

async fn galaxy_focus_game(
    State(state): State<AppState>,
    RoutePath(id): RoutePath<String>,
) -> Response {
    if let Err(response) = ensure_galaxy_unlocked(&state) {
        return response;
    }
    match state.galaxy_service.focus_game(&id) {
        Ok(Some(result)) => Json(result).into_response(),
        Ok(None) => not_found("unknown game focus"),
        Err(error) => internal_error(error),
    }
}

async fn galaxy_focus_event(
    State(state): State<AppState>,
    RoutePath(id): RoutePath<String>,
) -> Response {
    if let Err(response) = ensure_galaxy_unlocked(&state) {
        return response;
    }
    match state.galaxy_service.focus_event(&id) {
        Ok(Some(result)) => Json(result).into_response(),
        Ok(None) => not_found("unknown event focus"),
        Err(error) => internal_error(error),
    }
}

async fn galaxy_summary(State(state): State<AppState>) -> Response {
    if let Err(response) = ensure_galaxy_unlocked(&state) {
        return response;
    }
    match state.galaxy_service.summary() {
        Ok(result) => Json(result).into_response(),
        Err(error) => internal_error(error),
    }
}

fn build_login_page(state: &AppState) -> Result<LoginPageView> {
    Ok(LoginPageView {
        local_user: state.provider_store.local_user(),
        gate_state: state.provider_store.gate_state()?,
    })
}

fn build_desktop_launcher_page(state: &AppState) -> Result<DesktopLauncherPageView> {
    Ok(DesktopLauncherPageView {
        local_user: state.provider_store.local_user(),
        gate_state: state.provider_store.gate_state()?,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

fn build_setup_page(state: &AppState, mode: &str) -> Result<SetupPageView> {
    Ok(SetupPageView {
        mode: mode.to_string(),
        local_user: state.provider_store.local_user(),
        gate_state: state.provider_store.gate_state()?,
        providers: state.provider_store.list_provider_views()?,
        unlock_state: state.provider_store.unlock_state()?,
    })
}

fn build_workbench_page(
    state: &AppState,
    unlock_state: crate::models::UnlockStateView,
) -> Result<WorkbenchPageView> {
    Ok(WorkbenchPageView {
        local_user: state.provider_store.local_user(),
        gate_state: state.provider_store.gate_state()?,
        unlock_state,
        overview: state.workbench_service.overview()?,
    })
}

#[allow(dead_code)]
fn first_game_focus(
    galaxy_service: &GalaxyService,
    list: &GalaxyListResponse,
) -> Result<Option<GalaxyFocusView>> {
    let Some(game) = list.games.first() else {
        return Ok(None);
    };
    galaxy_service.focus_game(&game.id)
}

#[allow(dead_code)]
fn empty_focus() -> GalaxyFocusView {
    GalaxyFocusView {
        focus_id: "empty".to_string(),
        focus_kind: "game".to_string(),
        title: "等待星图数据".to_string(),
        subtitle: "当前还没有可用的 SLG 焦点。".to_string(),
        narrative: vec![
            "先完成 Provider 配置并接入真实信号。".to_string(),
            "随后左侧列表会自动替换成实际的游戏与事件。".to_string(),
        ],
        nodes: Vec::new(),
        edges: Vec::new(),
        available_layers: vec![
            "games".to_string(),
            "events".to_string(),
            "sources".to_string(),
        ],
    }
}

pub fn workspace_root() -> Result<PathBuf> {
    let candidate = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    match candidate.canonicalize() {
        Ok(path) => Ok(path),
        Err(_) => Ok(candidate),
    }
}

fn asset_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets")
}

fn ensure_login_confirmed(state: &AppState) -> std::result::Result<(), Response> {
    match state.provider_store.gate_state() {
        Ok(gate_state) if gate_state.local_user_confirmed => Ok(()),
        Ok(_) => Err(forbidden("login_required")),
        Err(error) => Err(internal_error(error)),
    }
}

fn ensure_main_system_access(
    state: &AppState,
) -> std::result::Result<crate::models::UnlockStateView, Response> {
    let gate_state = match state.provider_store.gate_state() {
        Ok(gate_state) => gate_state,
        Err(error) => return Err(internal_error(error)),
    };
    let unlock_state = match state.provider_store.unlock_state() {
        Ok(unlock_state) => unlock_state,
        Err(error) => return Err(internal_error(error)),
    };

    if gate_state.local_user_confirmed && gate_state.api_gate_passed && unlock_state.unlocked {
        Ok(unlock_state)
    } else {
        Err(Redirect::temporary(locked_destination(&gate_state, &unlock_state)).into_response())
    }
}

fn ensure_galaxy_unlocked(state: &AppState) -> std::result::Result<(), Response> {
    match ensure_main_system_access(state) {
        Ok(_) => Ok(()),
        Err(response) => {
            if response.status().is_redirection() {
                Err(forbidden("setup_required"))
            } else {
                Err(response)
            }
        }
    }
}

fn root_destination(gate_state: &GateStateView) -> &'static str {
    if !gate_state.local_user_confirmed {
        "/login"
    } else if !gate_state.api_gate_passed {
        "/setup/providers"
    } else {
        "/galaxy"
    }
}

fn locked_destination(
    gate_state: &GateStateView,
    unlock_state: &crate::models::UnlockStateView,
) -> &'static str {
    if !gate_state.local_user_confirmed {
        "/login"
    } else if !gate_state.api_gate_passed || !unlock_state.unlocked {
        "/setup/providers"
    } else {
        "/galaxy"
    }
}

#[allow(dead_code)]
fn empty_focus_v2() -> GalaxyFocusView {
    GalaxyFocusView {
        focus_id: "empty".to_string(),
        focus_kind: "game".to_string(),
        title: "等待星图数据".to_string(),
        subtitle: "当前还没有可用的 SLG 焦点。".to_string(),
        narrative: vec![
            "先完成必需 API 的配置与测试，再进入主系统。".to_string(),
            "解锁后左侧列表会切换成真实的游戏与热点事件。".to_string(),
        ],
        nodes: Vec::new(),
        edges: Vec::new(),
        available_layers: vec![
            "games".to_string(),
            "events".to_string(),
            "sources".to_string(),
        ],
    }
}

fn internal_error(error: anyhow::Error) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("internal server error: {error:#}"),
    )
        .into_response()
}

fn bad_request(message: String) -> Response {
    (StatusCode::BAD_REQUEST, message).into_response()
}

fn not_found(message: &str) -> Response {
    (StatusCode::NOT_FOUND, message.to_string()).into_response()
}

fn forbidden(message: &str) -> Response {
    (StatusCode::FORBIDDEN, message.to_string()).into_response()
}
