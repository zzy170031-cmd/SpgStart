const root = document.getElementById("workbench-root");
const MAP_WIDTH = 760;
const MAP_HEIGHT = 440;
const MAP_CENTER_X = MAP_WIDTH / 2;
const MAP_CENTER_Y = MAP_HEIGHT / 2;

function decodeState(encoded) {
  if (!encoded) return null;
  const binary = atob(encoded);
  const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));
  return JSON.parse(new TextDecoder().decode(bytes));
}

function escapeHtml(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

async function fetchJson(url, options = {}) {
  const response = await fetch(url, {
    headers: { "Content-Type": "application/json" },
    ...options,
  });

  if (!response.ok) {
    const message = await response.text();
    if (response.status === 403 && message === "setup_required") {
      window.location.replace("/setup/providers");
      throw new Error("setup_required");
    }
    if (response.status === 403 && message === "login_required") {
      window.location.replace("/login");
      throw new Error("login_required");
    }
    throw new Error(message || "request_failed");
  }

  return response.json();
}

async function fetchOptionalJson(url) {
  const response = await fetch(url);
  if (response.status === 404) {
    return null;
  }
  if (!response.ok) {
    throw new Error(await response.text());
  }
  return response.json();
}

function formatScore(value) {
  return Number(value || 0).toFixed(3);
}

function seeded(seed) {
  const value = Math.sin(seed * 91.37 + seed * 0.73) * 43758.5453;
  return value - Math.floor(value);
}

function buildRankingUrl(kind, limit, extra = {}) {
  const params = new URLSearchParams();
  params.set("kind", kind);
  params.set("limit", String(limit));

  const query = extra.query ?? state.searchQuery;
  if (query) {
    params.set("query", query);
  }
  if (extra.gameId) {
    params.set("game_id", extra.gameId);
  }
  if (extra.eventType) {
    params.set("event_type", extra.eventType);
  }

  return `/api/workbench/rankings?${params.toString()}`;
}

function currentStrategySummary() {
  const settings = state.overview.settings || {};
  return `${settings.time_window_hours || 24}H 热度窗 · 每 ${settings.schedule_interval_hours || 2}H 调度 · 每轮 ${settings.max_candidates_per_run || 50} 条`;
}

function latestRun() {
  return state.overview.latest_run || null;
}

function formatDisplayTime(value) {
  const text = String(value || "").trim();
  if (!text) return "未记录";
  return text.replace("T", " ").replace("Z", "");
}

function renderInlineStatusCard(label, detail, tone = "cached") {
  return `
    <article class="control-inline-status-card is-${escapeHtml(tone)}">
      <strong>${escapeHtml(label)}</strong>
      <p>${escapeHtml(detail)}</p>
    </article>
  `;
}

function buildContentModuleState() {
  const run = latestRun();
  const count = state.contentRankings.length;

  if (count > 0) {
    if (run?.status === "succeeded") {
      return {
        tone: "live",
        label: "实时数据",
        detail: `当前展示最近一次成功抓取的 ${count} 条结果，更新时间：${formatDisplayTime(
          run.finished_at || run.started_at
        )}`,
      };
    }
    if (run?.status === "failed") {
      return {
        tone: "cached",
        label: "缓存数据",
        detail: `本轮抓取失败，当前保留上一次成功快照。失败原因：${clipText(
          run.error_message || "未返回错误详情",
          96
        )}`,
      };
    }
    if (run?.status === "running") {
      return {
        tone: "cached",
        label: "缓存数据",
        detail: "后台正在拉取新内容，当前先展示上一次可用快照。",
      };
    }
    return {
      tone: "cached",
      label: "缓存数据",
      detail: `当前展示最近一次可用快照，共 ${count} 条。`,
    };
  }

  if (run?.status === "failed") {
    return {
      tone: "failed",
      label: "抓取失败",
      detail: `最近一次抓取失败：${clipText(run.error_message || "未返回错误详情", 96)}`,
    };
  }

  return {
    tone: "empty",
    label: "暂无数据",
    detail: "当前筛选条件下还没有可展示的真实视频内容。",
  };
}

function buildTrustLabel(trust) {
  if (!trust) {
    return "来源线索未补齐";
  }
  if (trust.source_platform_match) {
    return trust.fallback_non_douyin ? "抖音命中，含跨平台补位" : "抖音直连候选";
  }
  if (trust.fallback_non_douyin) {
    return "跨平台补位候选";
  }
  return "待进一步校验";
}

function buildVideoTrustSummary(candidate) {
  if (!candidate) {
    return {
      tone: "empty",
      label: "等待选择视频",
      detail: "从中间长横条列表选择一条视频后，这里会显示来源与抓取说明。",
    };
  }

  const moduleState = buildContentModuleState();
  const trust = candidate.trust || null;
  const notes = [
    `数据状态：${moduleState.label}`,
    `来源校验：${buildTrustLabel(trust)}`,
    `发布时间：${formatDisplayTime(candidate.published_at)}`,
    `收录时间：${formatDisplayTime(trust?.captured_at)}`,
  ];

  if (trust?.verification_note) {
    notes.push(`二次核验：${trust.verification_note}`);
  }

  return {
    tone: moduleState.tone,
    label: candidate.source_domain || candidate.platform || "来源未标记",
    detail: notes.join(" | "),
  };
}

function providerItems() {
  return Array.isArray(state.providers) ? state.providers : [];
}

function providerName(item) {
  return item.display_name || item.provider || "Provider";
}

function providerStatusLabel(item) {
  if (!item.enabled) return "Disabled";
  switch (item.status) {
    case "ready":
      return "Verified";
    case "configured":
      return "Configured";
    case "awaiting_configuration":
      return "Missing config";
    default:
      return item.status || "Connected";
  }
}

function providerTone(item) {
  if (item.last_error) return "danger";
  if (item.status === "ready") return "success";
  if (item.status === "configured") return "info";
  if (item.status === "awaiting_configuration" || item.has_api_key === false) return "warning";
  return "neutral";
}

function toneClassName(tone) {
  switch (tone) {
    case "success":
      return "is-success";
    case "warning":
      return "is-warning";
    case "danger":
      return "is-danger";
    case "info":
      return "is-info";
    default:
      return "is-neutral";
  }
}

function configuredProviderCount() {
  const providers = providerItems();
  if (!providers.length) {
    return (state.overview.budgets || []).filter((item) => item.enabled).length;
  }
  return providers.filter((item) => item.enabled && item.has_api_key).length;
}

function readyProviderCount() {
  return providerItems().filter((item) => item.status === "ready").length;
}

function configuredProviderSummary() {
  const providers = providerItems();
  if (!providers.length) {
    const budgets = state.overview.budgets || [];
    return `${budgets.filter((item) => item.enabled).length}/${budgets.length || 0}`;
  }
  return `${configuredProviderCount()}/${providers.length}`;
}

function joinProviderNames(items) {
  const names = items.map(providerName).filter(Boolean);
  if (names.length <= 3) {
    return names.join(", ");
  }
  return `${names.slice(0, 3).join(", ")} and ${names.length - 3} more`;
}

function clipText(value, max = 120) {
  const text = String(value || "").trim();
  if (!text) return "";
  return text.length > max ? `${text.slice(0, max - 1)}...` : text;
}

function looksLikeConfigIssue(message) {
  return /api key is missing|is disabled|not configured|missing runtime config/i.test(
    String(message || "")
  );
}

function selectedFilterSummary() {
  const filters = [];
  if (state.selectedGameId) {
    filters.push(`game: ${selectedGameTitle()}`);
  }
  if (state.selectedEventType) {
    filters.push(`event: ${state.selectedEventType}`);
  }
  if (state.searchQuery) {
    filters.push(`search: ${state.searchQuery}`);
  }
  return filters.join(" / ");
}

function summarizeRunStatus(status) {
  switch (status) {
    case "running":
      return "Running";
    case "failed":
      return "Failed";
    case "succeeded":
      return "Completed";
    default:
      return status || "Not started";
  }
}

function buildFeedbackActions({ includeSetupLink = false, includeClearEvent = false, includeResetSearch = false } = {}) {
  const actions = [];
  if (includeSetupLink) {
    actions.push({ href: "/setup/providers", label: "Check page 2 config" });
  }
  if (includeClearEvent && state.selectedEventType) {
    actions.push({ action: "clear-event-filter", label: "Clear event filter" });
  }
  if (includeResetSearch && state.searchQuery) {
    actions.push({ action: "reset-search", label: "Clear search" });
  }
  return actions;
}

function renderFeedbackAction(action) {
  if (action.href) {
    return `<a class="ghost-action compact-action" href="${escapeHtml(action.href)}">${escapeHtml(
      action.label
    )}</a>`;
  }
  return `<button class="ghost-action compact-action" type="button" data-action="${escapeHtml(
    action.action
  )}">${escapeHtml(action.label)}</button>`;
}

function renderFeedbackBlock(feedback, extraClass = "") {
  if (!feedback) return "";
  const actions = (feedback.actions || []).length
    ? `<div class="control-run-feedback-actions">${feedback.actions
        .map(renderFeedbackAction)
        .join("")}</div>`
    : "";
  return `
    <section class="control-run-feedback ${toneClassName(feedback.tone)} ${extraClass}">
      <p class="control-run-feedback-eyebrow">${escapeHtml(feedback.eyebrow || "Run feedback")}</p>
      <h4>${escapeHtml(feedback.title)}</h4>
      <p>${escapeHtml(feedback.body)}</p>
      ${feedback.detail ? `<p class="control-run-feedback-detail">${escapeHtml(feedback.detail)}</p>` : ""}
      ${actions}
    </section>
  `;
}

function buildRunFeedback() {
  const run = latestRun();
  const providers = providerItems();
  const configuredSummary = configuredProviderSummary();
  const readyCount = readyProviderCount();
  const pendingProviders = providers.filter(
    (item) => item.enabled && (!item.has_api_key || item.status === "awaiting_configuration")
  );

  if (!run) {
    return {
      tone: configuredProviderCount() > 0 ? "info" : "warning",
      eyebrow: "Config state",
      title: configuredProviderCount() > 0 ? "Page 2 API config is loaded" : "No usable API config yet",
      body:
        configuredProviderCount() > 0
          ? `The workbench already loaded ${configuredSummary} providers. It just has not executed a run yet.`
          : "The workbench does not currently see a usable provider setup.",
      detail:
        pendingProviders.length > 0
          ? `Still missing: ${joinProviderNames(pendingProviders)}.`
          : readyCount > 0
          ? `${readyCount} providers have already passed verification.`
          : "Run one provider test on page 2 first if you want higher confidence.",
      actions: buildFeedbackActions({
        includeSetupLink: configuredProviderCount() === 0 || pendingProviders.length > 0,
      }),
    };
  }

  if (run.status === "running") {
    return {
      tone: "info",
      eyebrow: "Run state",
      title: "Analysis is running",
      body: "The workbench is currently collecting and filtering content.",
      detail: `Started at ${run.started_at}.`,
      actions: [],
    };
  }

  if (run.status === "failed") {
    const error = clipText(run.error_message || "Unknown error");
    const configIssue = looksLikeConfigIssue(error);
    return {
      tone: "danger",
      eyebrow: "Run state",
      title: "The latest run failed",
      body: error || "The system did not produce a valid runtime result.",
      detail: configIssue
        ? "This looks more like missing or invalid config than a page-to-page handoff failure."
        : "This looks more like a runtime failure with no usable result returned.",
      actions: buildFeedbackActions({ includeSetupLink: configIssue }),
    };
  }

  if ((run.content_count || 0) === 0) {
    const hasCandidates = Number(run.candidate_count || 0) > 0;
    return {
      tone: "warning",
      eyebrow: "Run state",
      title: hasCandidates ? "Candidates were found but no final content survived" : "The run completed with no candidates",
      body: hasCandidates
        ? `This run found ${run.candidate_count} candidates, but 0 items reached the content list.`
        : "The run completed without producing any displayable candidates.",
      detail: selectedFilterSummary()
        ? `Active filters: ${selectedFilterSummary()}. Try clearing filters or widening keywords.`
        : "This looks more like a low-result runtime path than a config handoff problem.",
      actions: buildFeedbackActions({
        includeClearEvent: true,
        includeResetSearch: true,
      }),
    };
  }

  return {
    tone: "success",
    eyebrow: "Run state",
    title: "The latest run completed normally",
    body: `Produced ${run.content_count} content items from ${run.candidate_count} candidates.`,
    detail: `Loaded ${configuredSummary} providers, with ${readyCount} already verified.`,
    actions: [],
  };
}

function buildGameEmptyFeedback() {
  if (state.searchQuery) {
    return {
      tone: "warning",
      eyebrow: "Search feedback",
      title: "No games matched the current search",
      body: `The search "${state.searchQuery}" did not match the current heat list.`,
      detail: "Clear search first, then confirm whether the latest run produced game rankings.",
      actions: buildFeedbackActions({ includeResetSearch: true }),
    };
  }
  const feedback = buildRunFeedback();
  return {
    tone: feedback.tone,
    eyebrow: "Heat list feedback",
    title: "There is no game heat list yet",
    body: feedback.body,
    detail: feedback.detail,
    actions: feedback.actions,
  };
}

function buildVideoEmptyFeedback() {
  const run = latestRun();
  if (run && run.status === "succeeded" && Number(run.content_count || 0) > 0) {
    return {
      tone: "warning",
      eyebrow: "Filter feedback",
      title: "No videos match the current filters",
      body: selectedFilterSummary()
        ? `${selectedFilterSummary()} currently returns no visible items.`
        : "The current filters return no visible items.",
      detail: "Clear the event filter or search term first.",
      actions: buildFeedbackActions({ includeClearEvent: true, includeResetSearch: true }),
    };
  }
  const feedback = buildRunFeedback();
  return {
    tone: feedback.tone,
    eyebrow: "Content feedback",
    title: "There is no video breakdown content yet",
    body: feedback.body,
    detail: feedback.detail,
    actions: feedback.actions,
  };
}

function buildDetailEmptyFeedback() {
  if (state.contentRankings.length > 0) {
    return {
      tone: "info",
      eyebrow: "Interaction hint",
      title: "Select a video from the middle list first",
      body: "The breakdown and creative guide appear only after one item is selected.",
      detail: "If the middle list is also empty, check the run feedback on the left first.",
      actions: [],
    };
  }
  return buildVideoEmptyFeedback();
}

function selectedGameTitle() {
  if (state.selectedGameId) {
    const match = state.gameRankings.find((item) => item.entity_id === state.selectedGameId);
    if (match) return match.title;
  }

  if (state.selectedContent?.candidate?.game_name) {
    return state.selectedContent.candidate.game_name;
  }

  if (state.galaxyFocus?.title) {
    return state.galaxyFocus.title;
  }

  return "未锁定";
}

function selectedEventLabel() {
  if (!state.selectedEventType) {
    return "未锁定事件";
  }
  return `事件筛选：${state.selectedEventType}`;
}

function providerUsageMap() {
  const usage = state.overview.latest_run?.provider_usage?.length
    ? state.overview.latest_run.provider_usage
    : state.overview.budgets || [];
  return new Map(usage.map((item) => [item.provider, item]));
}

function applyDrawerState() {
  const overlay = document.getElementById("drawer-overlay");
  const topicDrawer = document.getElementById("topic-pool-drawer");
  const strategyDrawer = document.getElementById("strategy-drawer");
  const overlayVisible = state.topicPoolDrawerOpen || state.strategyDrawerOpen;

  if (overlay) overlay.hidden = !overlayVisible;
  if (topicDrawer) topicDrawer.hidden = !state.topicPoolDrawerOpen;
  if (strategyDrawer) strategyDrawer.hidden = !state.strategyDrawerOpen;
}

function openDrawer(drawer) {
  state.topicPoolDrawerOpen = drawer === "topic";
  state.strategyDrawerOpen = drawer === "strategy";
  applyDrawerState();
}

function closeDrawers() {
  state.topicPoolDrawerOpen = false;
  state.strategyDrawerOpen = false;
  applyDrawerState();
}

function renderGameSearchItem(item) {
  const active = state.selectedGameId === item.entity_id ? " is-active" : "";
  return `
    <button class="control-game-item${active}" type="button" data-action="select-game" data-game-id="${escapeHtml(
      item.entity_id
    )}">
      <div class="control-game-item-top">
        <strong>${escapeHtml(item.title)}</strong>
        <span>#${item.rank}</span>
      </div>
      <p>${escapeHtml(item.subtitle)}</p>
    </button>
  `;
}

function renderApiStatusItem(item) {
  const usage = providerUsageMap().get(item.provider) || item;
  const statusLabel = item.enabled ? "已接入" : "已停用";
  return `
    <article class="control-api-item">
      <div class="control-api-item-top">
        <strong>${escapeHtml(item.provider)}</strong>
        <span>${statusLabel}</span>
      </div>
      <p>月度 ${usage.used_monthly}/${usage.monthly_limit} · 今日 ${usage.used_today}/${usage.daily_soft_limit}</p>
      <p>本轮可用 ${usage.allowed_this_run} · 保留池 ${usage.reserve_pool}</p>
    </article>
  `;
}

function renderHeatStripItem(item) {
  const active = state.selectedGameId === item.entity_id ? " is-active" : "";
  return `
    <button class="control-heat-strip-item${active}" type="button" data-action="select-game" data-game-id="${escapeHtml(
      item.entity_id
    )}">
      <span class="control-heat-strip-rank">#${item.rank}</span>
      <strong>${escapeHtml(item.title)}</strong>
      <span class="control-heat-strip-score">${formatScore(item.score)}</span>
    </button>
  `;
}

function renderProviderStatusItem(item) {
  const name = providerName(item);
  const usage = providerUsageMap().get(name) || providerUsageMap().get(item.provider) || item;
  const statusLabel = providerStatusLabel(item);
  const verificationNote = item.last_error
    ? `最近异常：${clipText(item.last_error, 72)}`
    : item.last_verified_at
    ? `最近验证：${item.last_verified_at}`
    : item.has_api_key === false
    ? "API Key 未保存"
    : "尚未验证";
  const quotaSummary =
    typeof usage.allowed_this_run === "number"
      ? `本轮可用 ${usage.allowed_this_run} · 今日 ${usage.used_today}/${usage.daily_soft_limit}`
      : "暂无预算数据";
  return `
    <article class="control-api-item">
      <div class="control-api-item-top">
        <strong>${escapeHtml(name)}</strong>
        <span class="control-status-chip ${toneClassName(providerTone(item))}">${escapeHtml(
          statusLabel
        )}</span>
      </div>
      <p class="control-api-item-meta">${escapeHtml(
        item.has_api_key === false ? "Key 缺失" : "Key 已载入"
      )} · ${escapeHtml(quotaSummary)}</p>
      <p class="control-api-item-note">${escapeHtml(verificationNote)}</p>
    </article>
  `;
}

function renderGameHeatItem(item) {
  const active = state.selectedGameId === item.entity_id ? " is-active" : "";
  return `
    <button class="control-heat-item${active}" type="button" data-action="select-game" data-game-id="${escapeHtml(
      item.entity_id
    )}">
      <div class="control-heat-item-top">
        <strong>${escapeHtml(item.title)}</strong>
        <span>${formatScore(item.score)}</span>
      </div>
      <p>${escapeHtml(item.subtitle)}</p>
    </button>
  `;
}

function renderEventFilterChip(item) {
  const eventType = item.event_type || item.entity_id;
  const active = state.selectedEventType === eventType ? " is-active" : "";
  return `
    <button class="control-event-chip${active}" type="button" data-action="select-event" data-event-type="${escapeHtml(
      eventType
    )}">
      ${escapeHtml(item.title)}
    </button>
  `;
}

function renderVideoListItem(item) {
  const active = state.selectedContentId === item.content_id ? " is-active" : "";
  return `
    <button class="control-video-item${active}" type="button" data-action="select-content" data-content-id="${escapeHtml(
      item.content_id || ""
    )}">
      <div class="control-video-item-top">
        <strong>${escapeHtml(item.title)}</strong>
        <span>${formatScore(item.score)}</span>
      </div>
      <div class="control-video-item-meta">
        <span>${escapeHtml(item.game_id || "未归因游戏")}</span>
        <span>${escapeHtml(item.event_type || "general")}</span>
        <span>${escapeHtml(item.subtitle)}</span>
      </div>
    </button>
  `;
}

function renderMetric(label, value) {
  return `
    <div class="control-metric-card">
      <span>${escapeHtml(label)}</span>
      <strong>${escapeHtml(value)}</strong>
    </div>
  `;
}

function renderInsightBlock(title, content) {
  const body = Array.isArray(content)
    ? `<ul>${content.map((item) => `<li>${escapeHtml(item)}</li>`).join("")}</ul>`
    : `<p>${escapeHtml(content || "暂无内容")}</p>`;

  return `
    <article class="control-insight-item">
      <h4>${escapeHtml(title)}</h4>
      ${body}
    </article>
  `;
}

function renderTopicPoolItem(item) {
  return `
    <article class="control-drawer-item">
      <div class="control-drawer-item-top">
        <strong>${escapeHtml(item.title)}</strong>
        <button class="ghost-action compact-action" type="button" data-action="remove-topic" data-topic-id="${escapeHtml(
          item.topic_id
        )}">移除</button>
      </div>
      <p>${escapeHtml(item.game_name || "未归因游戏")} · 热度 ${formatScore(item.score)}</p>
      <p>${escapeHtml(item.note || "暂无备注")}</p>
    </article>
  `;
}

function renderWatchlistChip(game) {
  return `<button class="control-watchlist-chip" type="button" data-action="seed-watch-game" data-watch-game="${escapeHtml(
    game
  )}">${escapeHtml(game)}</button>`;
}

function orbitSpec(level) {
  if (level === 1) return { rx: 150, ry: 102, tilt: -10 };
  if (level === 2) return { rx: 222, ry: 142, tilt: 8 };
  return { rx: 290, ry: 188, tilt: -6 };
}

function computeNodePositions(nodes) {
  const positions = new Map();
  nodes.forEach((node, index) => {
    if (node.orbit === 0) {
      positions.set(node.id, { x: MAP_CENTER_X, y: MAP_CENTER_Y });
      return;
    }

    const orbit = orbitSpec(node.orbit);
    const angle = ((node.angle || index * 40) * Math.PI) / 180;
    const tilt = (orbit.tilt * Math.PI) / 180;
    positions.set(node.id, {
      x: MAP_CENTER_X + Math.cos(angle + tilt) * orbit.rx,
      y: MAP_CENTER_Y + Math.sin(angle) * orbit.ry,
    });
  });
  return positions;
}

function nodeIsHot(node) {
  if (!node) return false;
  if (state.selectedEventType && node.label === state.selectedEventType) return true;
  if (node.node_type === "event") {
    return Number(node.size || 0) >= 14;
  }
  return node.node_type === "source" && Number(node.size || 0) >= 16;
}

function renderConstellation() {
  const svg = document.getElementById("constellation-map");
  if (!svg) return;

  if (!state.galaxyFocus?.nodes?.length) {
    svg.innerHTML = `
      <g>
        <rect x="0" y="0" width="${MAP_WIDTH}" height="${MAP_HEIGHT}" rx="28" fill="rgba(247,247,243,0.94)"></rect>
        <text x="${MAP_WIDTH / 2}" y="${MAP_HEIGHT / 2 - 8}" text-anchor="middle" fill="#111111" font-size="28" font-weight="700">等待星图数据</text>
        <text x="${MAP_WIDTH / 2}" y="${MAP_HEIGHT / 2 + 26}" text-anchor="middle" fill="rgba(17,17,17,0.62)" font-size="14">先选择一个游戏，或者等待本轮工作流生成可视化节点。</text>
      </g>
    `;
    return;
  }

  const nodes = state.galaxyFocus.nodes;
  const edges = state.galaxyFocus.edges || [];
  const positions = computeNodePositions(nodes);
  const dust = Array.from({ length: 36 }, (_, index) => {
    const x = seeded(index + 17) * MAP_WIDTH;
    const y = seeded(index + 91) * MAP_HEIGHT;
    const radius = 0.8 + seeded(index + 121) * 1.6;
    return `<circle cx="${x}" cy="${y}" r="${radius}" fill="rgba(17,17,17,0.08)"></circle>`;
  }).join("");

  const orbitMarkup = [1, 2, 3]
    .map((level) => {
      const orbit = orbitSpec(level);
      return `
        <ellipse
          cx="${MAP_CENTER_X}"
          cy="${MAP_CENTER_Y}"
          rx="${orbit.rx}"
          ry="${orbit.ry}"
          transform="rotate(${orbit.tilt} ${MAP_CENTER_X} ${MAP_CENTER_Y})"
          fill="none"
          stroke="rgba(17,17,17,0.14)"
          stroke-width="1"
          stroke-dasharray="6 10"
        ></ellipse>
      `;
    })
    .join("");

  const edgeMarkup = edges
    .map((edge) => {
      const source = positions.get(edge.source);
      const target = positions.get(edge.target);
      if (!source || !target) return "";
      const mx = (source.x + target.x) / 2;
      const my = (source.y + target.y) / 2;
      const dx = target.x - source.x;
      const dy = target.y - source.y;
      const bend = 0.08 + (Number(edge.strength || 0.4) * 0.06);
      return `
        <path
          d="M ${source.x} ${source.y} Q ${mx - dy * bend} ${my + dx * bend} ${target.x} ${target.y}"
          fill="none"
          stroke="rgba(17,17,17,0.16)"
          stroke-width="${Math.max(1, Number(edge.strength || 0.4) * 1.8)}"
        ></path>
      `;
    })
    .join("");

  const nodeMarkup = nodes
    .map((node) => {
      const point = positions.get(node.id);
      if (!point) return "";
      const hot = nodeIsHot(node);
      const selected =
        node.node_type === "game" &&
        node.id.replace("game::", "") === state.selectedGameId;
      const radius = Math.max(8, Math.min(24, Number(node.size || 12)));
      const labelY = point.y + radius + 18;
      const kind = node.node_type === "game" ? "game" : node.node_type === "event" ? "event" : "source";
      const refId =
        kind === "game"
          ? node.id.replace("game::", "")
          : kind === "event"
          ? node.label
          : node.id;

      return `
        <g class="constellation-node" data-action="select-map-node" data-node-kind="${kind}" data-node-ref="${escapeHtml(
          refId
        )}">
          ${
            hot
              ? `<circle class="constellation-node-pulse" cx="${point.x}" cy="${point.y}" r="${
                  radius + 7
                }"></circle>`
              : ""
          }
          <circle
            cx="${point.x}"
            cy="${point.y}"
            r="${radius}"
            class="constellation-node-core${hot ? " is-hot" : ""}${selected ? " is-selected" : ""}"
          ></circle>
          <text x="${point.x}" y="${labelY}" text-anchor="middle" class="constellation-node-label">${escapeHtml(
            node.label
          )}</text>
        </g>
      `;
    })
    .join("");

  svg.innerHTML = `
    <defs>
      <radialGradient id="constellationGlow" cx="50%" cy="50%" r="50%">
        <stop offset="0%" stop-color="rgba(255,255,255,0.95)"></stop>
        <stop offset="100%" stop-color="rgba(244,244,240,0.0)"></stop>
      </radialGradient>
    </defs>
    <rect x="0" y="0" width="${MAP_WIDTH}" height="${MAP_HEIGHT}" rx="28" fill="url(#constellationGlow)"></rect>
    ${dust}
    ${orbitMarkup}
    ${edgeMarkup}
    ${nodeMarkup}
  `;
}

function renderSummaryStrip() {
  const selectedGame = document.getElementById("summary-selected-game");
  const topicCount = document.getElementById("summary-topic-count");
  const strategy = document.getElementById("summary-strategy");
  const runStatus = document.getElementById("summary-run-status");

  if (selectedGame) selectedGame.textContent = selectedGameTitle();
  if (topicCount) topicCount.textContent = String(state.topicPool.length);
  if (strategy) strategy.textContent = currentStrategySummary();
  if (runStatus) {
    runStatus.textContent = state.overview.latest_run
      ? `${state.overview.latest_run.status} · ${state.overview.latest_run.finished_at || state.overview.latest_run.started_at}`
      : "尚未执行";
  }
}

function renderGameSearchPanel() {
  const meta = document.getElementById("game-list-meta");
  const list = document.getElementById("game-search-list");
  const watchlist = document.getElementById("watchlist-strip");

  if (watchlist) {
    const chips = (state.overview.settings.watchlist_games || []).length
      ? state.overview.settings.watchlist_games
          .map(renderWatchlistChip)
          .join("")
      : `<span class="control-watchlist-chip is-muted">等待配置监控游戏</span>`;
    watchlist.innerHTML = chips;
  }

  if (meta) {
    meta.textContent = state.searchQuery
      ? `当前关键词：${state.searchQuery} · 共匹配 ${state.gameRankings.length} 个游戏`
      : `显示全部监控游戏 · 共 ${state.gameRankings.length} 个游戏`;
  }

  if (list) {
    list.innerHTML = state.gameRankings.length
      ? state.gameRankings.map(renderGameSearchItem).join("")
      : `<p class="control-empty-state">没有匹配到游戏结果。</p>`;
  }
}

function latestRunErrorMessage() {
  const run = state.overview.latest_run;
  if (!run || run.status !== "failed") {
    return "";
  }
  return String(run.error_message || "").trim();
}

function emptyStateMessage(defaultMessage) {
  const failure = latestRunErrorMessage();
  if (!failure) {
    return defaultMessage;
  }
  return `当前暂无内容，最近一次运行失败：${failure}`;
}

function renderApiPanel() {
  const summary = document.getElementById("api-summary-card");
  const list = document.getElementById("api-status-list");
  const dropdown = document.getElementById("api-status-dropdown");
  const toggle = document.getElementById("toggle-api-panel-btn");
  const run = state.overview.latest_run;

  if (summary) {
    summary.innerHTML = `
      <div class="control-api-summary-row">
        <span>最近一次运行</span>
        <strong>${escapeHtml(run ? run.status : "尚未执行")}</strong>
      </div>
      <div class="control-api-summary-row">
        <span>本轮候选 / 入榜</span>
        <strong>${run ? `${run.candidate_count} / ${run.content_count}` : "0 / 0"}</strong>
      </div>
    `;
    const failure = latestRunErrorMessage();
    if (failure) {
      summary.insertAdjacentHTML(
        "beforeend",
        `
          <div class="control-api-error">
            <span>最近一次失败原因</span>
            <strong>${escapeHtml(failure)}</strong>
          </div>
        `
      );
    }
  }

  if (list) {
    list.innerHTML = (state.overview.budgets || []).length
      ? state.overview.budgets.map(renderApiStatusItem).join("")
      : `<p class="control-empty-state">当前没有 API 状态数据。</p>`;
  }

  if (dropdown) {
    dropdown.hidden = !state.apiPanelExpanded;
  }

  if (toggle) {
    toggle.textContent = state.apiPanelExpanded ? "收起详情" : "展开详情";
  }
}

function renderTopHeatStrip() {
  const count = document.getElementById("heat-strip-count");
  const strip = document.getElementById("game-heat-strip");

  if (count) {
    count.textContent = `${state.gameRankings.length} 条`;
  }

  if (strip) {
    strip.innerHTML = state.gameRankings.length
      ? state.gameRankings.map(renderHeatStripItem).join("")
      : `<p class="control-empty-state">当前没有游戏热度榜数据。</p>`;
  }
}

function renderStagePanel() {
  const eventPill = document.getElementById("active-event-pill");

  if (eventPill) {
    eventPill.textContent = selectedEventLabel();
    eventPill.classList.toggle("is-active", Boolean(state.selectedEventType));
  }

  renderConstellation();
}

function renderVideoPanel() {
  const eventRow = document.getElementById("event-filter-row");
  const videoList = document.getElementById("video-breakdown-list");

  if (eventRow) {
    const chips = [`<button class="control-event-chip${state.selectedEventType ? "" : " is-active"}" type="button" data-action="clear-event-filter">全部事件</button>`]
      .concat(state.eventRankings.slice(0, 6).map(renderEventFilterChip));
    eventRow.innerHTML = chips.join("");
  }

  if (videoList) {
    videoList.innerHTML = state.contentRankings.length
      ? state.contentRankings.slice(0, 10).map(renderVideoListItem).join("")
      : `<p class="control-empty-state">当前筛选条件下没有抖音视频拆解条目。</p>`;
  }
}

function renderRightPanels() {
  const meta = document.getElementById("selected-video-meta");
  const breakdown = document.getElementById("selected-video-breakdown");
  const guide = document.getElementById("selected-video-guide");
  const addTopicButton = document.getElementById("add-topic-btn");
  const sourceLink = document.getElementById("open-source-link");

  if (!state.selectedContent) {
    if (meta) meta.innerHTML = "";
    if (breakdown) breakdown.innerHTML = `<p class="control-empty-state">从中间下方选择一条视频后，这里会展示热门视频拆解。</p>`;
    if (guide) guide.innerHTML = `<p class="control-empty-state">创作指导会跟随选中视频更新。</p>`;
    if (addTopicButton) addTopicButton.setAttribute("disabled", "true");
    if (sourceLink) sourceLink.setAttribute("href", "#");
    return;
  }

  const { candidate, breakdown: detail } = state.selectedContent;
  if (meta) {
    meta.innerHTML = [
      renderMetric("热度", formatScore(candidate.hotness_score)),
      renderMetric("相关性", formatScore(candidate.doubao_relevance_score)),
      renderMetric("交叉验证", formatScore(candidate.cross_source_score)),
      renderMetric("来源", candidate.source_domain),
    ].join("");
  }
  if (breakdown) {
    breakdown.innerHTML = [
      renderInsightBlock("内容摘要", detail.content_summary),
      renderInsightBlock("爆点 / 钩子", detail.hook_points),
      renderInsightBlock("核心冲突 / 价值", detail.core_conflict_or_value),
      renderInsightBlock("适合人群", detail.audience_fit),
    ].join("");
  }
  if (guide) {
    guide.innerHTML = [
      renderInsightBlock("改编角度", detail.adaptation_angles),
      renderInsightBlock("标题方向", detail.title_directions),
      renderInsightBlock("创作建议", detail.topic_pool_reason),
    ].join("");
  }
  if (addTopicButton) addTopicButton.removeAttribute("disabled");
  if (sourceLink) sourceLink.setAttribute("href", candidate.url);
}

function renderTopicPoolDrawer() {
  const list = document.getElementById("topic-pool-drawer-list");
  if (!list) return;
  list.innerHTML = state.topicPool.length
    ? state.topicPool.map(renderTopicPoolItem).join("")
    : `<p class="control-empty-state">当前选题池为空。</p>`;
}

function applyStrategyForm(settings) {
  const form = document.getElementById("strategy-form");
  if (!form) return;
  form.elements.watchlist_games.value = (settings.watchlist_games || []).join("\n");
  form.elements.keyword_templates.value = (settings.keyword_templates || []).join("\n");
  form.elements.time_window_hours.value = settings.time_window_hours ?? 24;
  form.elements.schedule_interval_hours.value = settings.schedule_interval_hours ?? 2;
  form.elements.max_candidates_per_run.value = settings.max_candidates_per_run ?? 50;
  form.elements.doubao_model.value = settings.doubao_model || "";
}

function renderSummaryStrip() {
  const selectedGame = document.getElementById("summary-selected-game");
  const topicCount = document.getElementById("summary-topic-count");
  const strategy = document.getElementById("summary-strategy");
  const runStatus = document.getElementById("summary-run-status");
  const run = latestRun();

  if (selectedGame) selectedGame.textContent = selectedGameTitle();
  if (topicCount) topicCount.textContent = String(state.topicPool.length);
  if (strategy) strategy.textContent = currentStrategySummary();
  if (runStatus) {
    runStatus.textContent = run
      ? `${summarizeRunStatus(run.status)} | ${run.finished_at || run.started_at}`
      : "Not started";
  }
}

function renderGameSearchPanel() {
  const meta = document.getElementById("game-list-meta");
  const list = document.getElementById("game-search-list");
  const watchlist = document.getElementById("watchlist-strip");

  if (watchlist) {
    const chips = (state.overview.settings.watchlist_games || []).length
      ? state.overview.settings.watchlist_games.map(renderWatchlistChip).join("")
      : `<span class="control-watchlist-chip is-muted">No watchlist games yet</span>`;
    watchlist.innerHTML = chips;
  }

  if (meta) {
    meta.textContent = state.searchQuery
      ? `Search: ${state.searchQuery} | ${state.gameRankings.length} games`
      : `Showing all watched games | ${state.gameRankings.length} games`;
  }

  if (list) {
    list.innerHTML = state.gameRankings.length
      ? state.gameRankings.map(renderGameSearchItem).join("")
      : renderFeedbackBlock(buildGameEmptyFeedback(), "is-inline");
  }
}

function renderApiPanel() {
  const summary = document.getElementById("api-summary-card");
  const list = document.getElementById("api-status-list");
  const dropdown = document.getElementById("api-status-dropdown");
  const toggle = document.getElementById("toggle-api-panel-btn");
  const run = latestRun();
  const feedback = buildRunFeedback();
  const providers = providerItems();
  const items = providers.length ? providers : state.overview.budgets || [];

  if (summary) {
    summary.innerHTML = `
      <div class="control-api-summary-row">
        <span>Latest run</span>
        <strong>${escapeHtml(run ? summarizeRunStatus(run.status) : "Not started")}</strong>
      </div>
      <div class="control-api-summary-row">
        <span>Candidates / outputs</span>
        <strong>${run ? `${run.candidate_count} / ${run.content_count}` : "0 / 0"}</strong>
      </div>
      <div class="control-api-summary-row">
        <span>Loaded page-2 config</span>
        <strong>${escapeHtml(configuredProviderSummary())}</strong>
      </div>
      ${renderFeedbackBlock(feedback)}
    `;
  }

  if (list) {
    list.innerHTML = items.length
      ? items.map(renderProviderStatusItem).join("")
      : `<p class="control-empty-state">No API status data yet.</p>`;
  }

  if (dropdown) {
    dropdown.hidden = !state.apiPanelExpanded;
  }

  if (toggle) {
    toggle.textContent = state.apiPanelExpanded ? "Hide details" : "Show details";
  }

  document.body?.classList.toggle("api-panel-expanded", state.apiPanelExpanded);
}

function renderTopHeatStrip() {
  const count = document.getElementById("heat-strip-count");
  const strip = document.getElementById("game-heat-strip");

  if (count) {
    count.textContent = `${state.gameRankings.length} items`;
  }

  if (strip) {
    strip.innerHTML = state.gameRankings.length
      ? state.gameRankings.map(renderHeatStripItem).join("")
      : renderFeedbackBlock(buildGameEmptyFeedback(), "is-inline");
  }
}

function renderVideoPanel() {
  const eventRow = document.getElementById("event-filter-row");
  const videoList = document.getElementById("video-breakdown-list");

  if (eventRow) {
    const chips = [
      `<button class="control-event-chip${state.selectedEventType ? "" : " is-active"}" type="button" data-action="clear-event-filter">All events</button>`,
    ].concat(state.eventRankings.slice(0, 6).map(renderEventFilterChip));
    eventRow.innerHTML = chips.join("");
  }

  if (videoList) {
    videoList.innerHTML = state.contentRankings.length
      ? state.contentRankings.slice(0, 10).map(renderVideoListItem).join("")
      : renderFeedbackBlock(buildVideoEmptyFeedback(), "is-panel");
  }
}

function renderRightPanels() {
  const meta = document.getElementById("selected-video-meta");
  const breakdown = document.getElementById("selected-video-breakdown");
  const guide = document.getElementById("selected-video-guide");
  const addTopicButton = document.getElementById("add-topic-btn");
  const sourceLink = document.getElementById("open-source-link");

  if (!state.selectedContent) {
    if (meta) meta.innerHTML = "";
    if (breakdown) breakdown.innerHTML = renderFeedbackBlock(buildDetailEmptyFeedback(), "is-panel");
    if (guide) {
      guide.innerHTML =
        state.contentRankings.length > 0
          ? `<p class="control-passive-note">Select one item from the middle list to load the creative guide.</p>`
          : `<p class="control-passive-note">The creative guide appears automatically after the first valid content result.</p>`;
    }
    if (addTopicButton) addTopicButton.setAttribute("disabled", "true");
    if (sourceLink) sourceLink.setAttribute("href", "#");
    return;
  }

  const { candidate, breakdown: detail } = state.selectedContent;
  if (meta) {
    meta.innerHTML = [
      renderMetric("Hotness", formatScore(candidate.hotness_score)),
      renderMetric("Relevance", formatScore(candidate.doubao_relevance_score)),
      renderMetric("Cross-check", formatScore(candidate.cross_source_score)),
      renderMetric("Source", candidate.source_domain),
    ].join("");
  }
  if (breakdown) {
    breakdown.innerHTML = [
      renderInsightBlock("Summary", detail.content_summary),
      renderInsightBlock("Hooks", detail.hook_points),
      renderInsightBlock("Core value", detail.core_conflict_or_value),
      renderInsightBlock("Audience", detail.audience_fit),
    ].join("");
  }
  if (guide) {
    guide.innerHTML = [
      renderInsightBlock("Angles", detail.adaptation_angles),
      renderInsightBlock("Title directions", detail.title_directions),
      renderInsightBlock("Creative note", detail.topic_pool_reason),
    ].join("");
  }
  if (addTopicButton) addTopicButton.removeAttribute("disabled");
  if (sourceLink) sourceLink.setAttribute("href", candidate.url);
}

function renderAll() {
  renderSummaryStrip();
  renderTopHeatStrip();
  renderGameSearchPanel();
  renderApiPanel();
  renderStagePanel();
  renderVideoPanel();
  renderRightPanels();
  renderTopicPoolDrawer();
  applyStrategyForm(state.overview.settings || {});
  applyDrawerState();
}

async function loadGalaxyFocus() {
  if (!state.selectedGameId) {
    state.galaxyFocus = null;
    return;
  }
  state.galaxyFocus = await fetchOptionalJson(
    `/api/galaxy/focus/game/${encodeURIComponent(state.selectedGameId)}`
  );
}

async function loadSelectedContent(contentId) {
  if (!contentId) {
    state.selectedContent = null;
    state.selectedContentId = null;
    renderRightPanels();
    return;
  }

  state.selectedContent = await fetchJson(
    `/api/workbench/content/${encodeURIComponent(contentId)}`
  );
  state.selectedContentId = contentId;
  renderRightPanels();
}

async function syncRankings() {
  const [gameRankings, eventRankings] = await Promise.all([
    fetchJson(buildRankingUrl("game", 12, { query: state.searchQuery })),
    fetchJson(buildRankingUrl("event", 6, { query: state.searchQuery })),
  ]);

  state.gameRankings = gameRankings;
  state.eventRankings = eventRankings;

  if (
    !state.selectedGameId ||
    !state.gameRankings.some((item) => item.entity_id === state.selectedGameId)
  ) {
    state.selectedGameId = state.gameRankings[0]?.entity_id || null;
  }

  const contentRankings = await fetchJson(
    buildRankingUrl("content", 10, {
      query: state.searchQuery,
      gameId: state.selectedGameId,
      eventType: state.selectedEventType,
    })
  );
  state.contentRankings = contentRankings;

  if (
    state.selectedContentId &&
    !state.contentRankings.some((item) => item.content_id === state.selectedContentId)
  ) {
    state.selectedContent = null;
    state.selectedContentId = null;
  }

  await loadGalaxyFocus();

  if (!state.selectedContentId && state.contentRankings[0]?.content_id) {
    await loadSelectedContent(state.contentRankings[0].content_id);
    return;
  }

  renderAll();
}

async function refreshDashboard() {
  const [overview, topicPool] = await Promise.all([
    fetchJson("/api/workbench/overview"),
    fetchJson("/api/workbench/topic-pool"),
  ]);
  state.overview = overview;
  state.topicPool = topicPool;
  await syncRankings();
}

async function refreshDashboard() {
  const [overview, topicPool, providers] = await Promise.all([
    fetchJson("/api/workbench/overview"),
    fetchJson("/api/workbench/topic-pool"),
    fetchJson("/api/providers"),
  ]);
  state.overview = overview;
  state.topicPool = topicPool;
  state.providers = providers;
  await syncRankings();
}

async function submitSearch() {
  const input = document.getElementById("game-search-input");
  if (input instanceof HTMLInputElement) {
    state.searchQuery = input.value.trim();
  }
  await syncRankings();
}

async function runNow(button) {
  button?.setAttribute("disabled", "true");
  try {
    const result = await fetchJson("/api/workbench/runs/run-now", { method: "POST" });
    await refreshDashboard();
    window.alert(`本轮执行完成，状态：${result.status}`);
  } catch (error) {
    console.error(error);
    window.alert(`执行失败：${error.message || error}`);
  } finally {
    button?.removeAttribute("disabled");
  }
}

async function addTopicPool() {
  if (!state.selectedContentId) return;

  const note = document.getElementById("topic-note-input")?.value?.trim() || "";
  await fetchJson("/api/workbench/topic-pool", {
    method: "POST",
    body: JSON.stringify({
      content_id: state.selectedContentId,
      note,
    }),
  });
  await refreshDashboard();
  openDrawer("topic");
}

async function removeTopicPool(topicId) {
  await fetchJson(`/api/workbench/topic-pool/${encodeURIComponent(topicId)}`, {
    method: "DELETE",
  });
  await refreshDashboard();
}

async function saveStrategy(form) {
  const payload = {
    platform: "douyin",
    watchlist_games: form.elements.watchlist_games.value
      .split(/\r?\n/)
      .map((item) => item.trim())
      .filter(Boolean),
    keyword_templates: form.elements.keyword_templates.value
      .split(/\r?\n/)
      .map((item) => item.trim())
      .filter(Boolean),
    time_window_hours: Number(form.elements.time_window_hours.value || 24),
    schedule_interval_hours: Number(form.elements.schedule_interval_hours.value || 2),
    max_candidates_per_run: Number(form.elements.max_candidates_per_run.value || 50),
    doubao_model: form.elements.doubao_model.value.trim(),
  };

  state.overview.settings = await fetchJson("/api/workbench/settings", {
    method: "PUT",
    body: JSON.stringify(payload),
  });
  renderSummaryStrip();
  applyStrategyForm(state.overview.settings);
}

async function logout() {
  await fetchJson("/api/auth/logout", { method: "POST" });
  window.location.assign("/login");
}

function renderVideoListItem(item) {
  const active = state.selectedContentId === item.content_id ? " is-active" : "";
  const trustLabel = buildTrustLabel(item.trust);
  return `
    <button class="control-video-item${active}" type="button" data-action="select-content" data-content-id="${escapeHtml(
      item.content_id || ""
    )}">
      <div class="control-video-item-top">
        <strong>${escapeHtml(item.title)}</strong>
        <span>${formatScore(item.score)}</span>
      </div>
      <div class="control-video-item-meta">
        <span>${escapeHtml(item.game_id || "未归因游戏")}</span>
        <span>${escapeHtml(item.platform || "平台未标记")}</span>
        <span>${escapeHtml(formatDisplayTime(item.published_at))}</span>
        <span>${escapeHtml(item.source_domain || item.subtitle || "来源未标记")}</span>
        <span>${escapeHtml(trustLabel)}</span>
      </div>
    </button>
  `;
}

function renderVideoPanel() {
  const eventRow = document.getElementById("event-filter-row");
  const videoStatus = document.getElementById("video-panel-status");
  const videoList = document.getElementById("video-breakdown-list");
  const moduleState = buildContentModuleState();

  if (eventRow) {
    const chips = [
      `<button class="control-event-chip${state.selectedEventType ? "" : " is-active"}" type="button" data-action="clear-event-filter">全部事件</button>`,
    ].concat(state.eventRankings.slice(0, 6).map(renderEventFilterChip));
    eventRow.innerHTML = chips.join("");
  }

  if (videoStatus) {
    videoStatus.innerHTML = renderInlineStatusCard(
      moduleState.label,
      moduleState.detail,
      moduleState.tone
    );
  }

  if (videoList) {
    videoList.innerHTML = state.contentRankings.length
      ? state.contentRankings.slice(0, 10).map(renderVideoListItem).join("")
      : renderFeedbackBlock(buildVideoEmptyFeedback(), "is-panel");
  }
}

function renderRightPanels() {
  const meta = document.getElementById("selected-video-meta");
  const trust = document.getElementById("selected-video-trust");
  const breakdown = document.getElementById("selected-video-breakdown");
  const guide = document.getElementById("selected-video-guide");
  const addTopicButton = document.getElementById("add-topic-btn");
  const sourceLink = document.getElementById("open-source-link");

  if (!state.selectedContent) {
    if (meta) meta.innerHTML = "";
    if (trust) {
      trust.innerHTML = renderInlineStatusCard(
        "等待选择视频",
        "从中间长横条列表选择一条视频后，这里会显示来源校验、发布时间和抓取状态。",
        "empty"
      );
    }
    if (breakdown) breakdown.innerHTML = renderFeedbackBlock(buildDetailEmptyFeedback(), "is-panel");
    if (guide) {
      guide.innerHTML =
        state.contentRankings.length > 0
          ? `<p class="control-passive-note">请选择一条视频，右侧会同步显示拆解与创作指导。</p>`
          : `<p class="control-passive-note">当前还没有可用视频内容，完成一次有效抓取后这里会自动更新。</p>`;
    }
    if (addTopicButton) addTopicButton.setAttribute("disabled", "true");
    if (sourceLink) sourceLink.setAttribute("href", "#");
    return;
  }

  const { candidate, breakdown: detail } = state.selectedContent;
  const trustSummary = buildVideoTrustSummary(candidate);

  if (meta) {
    meta.innerHTML = [
      renderMetric("热度", formatScore(candidate.hotness_score)),
      renderMetric("相关度", formatScore(candidate.doubao_relevance_score)),
      renderMetric("交叉校验", formatScore(candidate.cross_source_score)),
      renderMetric("来源域名", candidate.source_domain || "未记录"),
      renderMetric("平台", candidate.platform || "未记录"),
      renderMetric("发布时间", formatDisplayTime(candidate.published_at)),
    ].join("");
  }

  if (trust) {
    trust.innerHTML = renderInlineStatusCard(
      trustSummary.label,
      trustSummary.detail,
      trustSummary.tone
    );
  }

  if (breakdown) {
    breakdown.innerHTML = [
      renderInsightBlock("内容摘要", detail.content_summary),
      renderInsightBlock("开头钩子", detail.hook_points),
      renderInsightBlock("核心冲突 / 价值点", detail.core_conflict_or_value),
      renderInsightBlock("受众适配", detail.audience_fit),
    ].join("");
  }

  if (guide) {
    guide.innerHTML = [
      renderInsightBlock("二创角度", detail.adaptation_angles),
      renderInsightBlock("标题方向", detail.title_directions),
      renderInsightBlock("创作建议", detail.topic_pool_reason),
    ].join("");
  }

  if (addTopicButton) addTopicButton.removeAttribute("disabled");
  if (sourceLink) sourceLink.setAttribute("href", candidate.url);
}

let searchTimer = null;

const bootstrap = decodeState(root?.dataset.state || "") || {};

const state = {
  overview: bootstrap.overview || {
    latest_run: null,
    content_rankings: [],
    game_rankings: [],
    event_rankings: [],
    topic_pool_count: 0,
    budgets: [],
    settings: {
      platform: "douyin",
      watchlist_games: [],
      keyword_templates: [],
      time_window_hours: 24,
      schedule_interval_hours: 2,
      max_candidates_per_run: 50,
      doubao_model: "",
    },
  },
  topicPool: [],
  gameRankings: [],
  eventRankings: [],
  contentRankings: [],
  selectedContent: null,
  galaxyFocus: null,
  selectedGameId: null,
  selectedEventType: null,
  selectedContentId: null,
  topicPoolDrawerOpen: false,
  strategyDrawerOpen: false,
  apiPanelExpanded: false,
  apiStatusSummary: [],
  searchQuery: "",
};

state.providers = Array.isArray(bootstrap.providers) ? bootstrap.providers : [];

document.addEventListener("click", async (event) => {
  const target = event.target;
  if (!(target instanceof HTMLElement)) return;

  if (target.id === "summary-topic-pool-btn") {
    openDrawer("topic");
    return;
  }

  if (target.id === "summary-strategy-btn") {
    openDrawer("strategy");
    return;
  }

  if (target.id === "drawer-overlay") {
    closeDrawers();
    return;
  }

  if (target.id === "run-now-btn") {
    await runNow(target);
    return;
  }

  if (target.id === "toggle-api-panel-btn") {
    state.apiPanelExpanded = !state.apiPanelExpanded;
    renderApiPanel();
    return;
  }

  if (target.id === "add-topic-btn") {
    await addTopicPool();
    return;
  }

  if (target.id === "game-search-submit") {
    await submitSearch();
    return;
  }

  const actionTarget = target.closest("[data-action], [data-close-drawer]");
  if (!(actionTarget instanceof HTMLElement)) return;

  if (actionTarget.dataset.closeDrawer) {
    closeDrawers();
    return;
  }

  const action = actionTarget.dataset.action;
  if (!action) return;

  if (action === "select-game") {
    state.selectedGameId = actionTarget.dataset.gameId || null;
    state.selectedEventType = null;
    await syncRankings();
    return;
  }

  if (action === "select-event") {
    state.selectedEventType = actionTarget.dataset.eventType || null;
    await syncRankings();
    return;
  }

  if (action === "clear-event-filter") {
    state.selectedEventType = null;
    await syncRankings();
    return;
  }

  if (action === "select-content") {
    await loadSelectedContent(actionTarget.dataset.contentId);
    return;
  }

  if (action === "remove-topic") {
    await removeTopicPool(actionTarget.dataset.topicId);
    return;
  }

  if (action === "seed-watch-game") {
    const watchGame = actionTarget.dataset.watchGame || "";
    state.searchQuery = watchGame;
    const input = document.getElementById("game-search-input");
    if (input instanceof HTMLInputElement) {
      input.value = watchGame;
    }
    await syncRankings();
    return;
  }

  if (action === "reset-search") {
    state.searchQuery = "";
    const input = document.getElementById("game-search-input");
    if (input instanceof HTMLInputElement) {
      input.value = "";
    }
    await syncRankings();
    return;
  }

  if (action === "select-map-node") {
    const kind = actionTarget.dataset.nodeKind;
    const ref = actionTarget.dataset.nodeRef;
    if (kind === "game") {
      state.selectedGameId = ref || null;
      state.selectedEventType = null;
      await syncRankings();
    } else if (kind === "event") {
      state.selectedEventType = ref || null;
      await syncRankings();
    }
    return;
  }
});

document.getElementById("game-search-input")?.addEventListener("input", (event) => {
  state.searchQuery = event.target.value.trim();
  window.clearTimeout(searchTimer);
  searchTimer = window.setTimeout(() => {
    syncRankings().catch((error) => {
      console.error(error);
      window.alert(`搜索失败：${error.message || error}`);
    });
  }, 180);
});

document.getElementById("game-search-input")?.addEventListener("keydown", (event) => {
  if (event.key !== "Enter") return;
  event.preventDefault();
  window.clearTimeout(searchTimer);
  submitSearch().catch((error) => {
    console.error(error);
    window.alert(`搜索失败：${error.message || error}`);
  });
});

document.getElementById("strategy-form")?.addEventListener("submit", async (event) => {
  event.preventDefault();
  try {
    await saveStrategy(event.currentTarget);
    window.alert("策略已保存。");
  } catch (error) {
    console.error(error);
    window.alert(`保存策略失败：${error.message || error}`);
  }
});

refreshDashboard().catch((error) => {
  console.error(error);
  window.alert(`主系统初始化失败：${error.message || error}`);
});
