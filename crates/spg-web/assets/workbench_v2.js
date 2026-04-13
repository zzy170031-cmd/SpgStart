const root = document.getElementById("workbench-root");

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

function formatScore(value) {
  return Number(value || 0).toFixed(3);
}

function formatOptional(value, fallback = "未提供") {
  return value ? escapeHtml(value) : fallback;
}

function latestRunLabel(run) {
  if (!run) return "待执行";
  return `${run.status} · ${run.finished_at || run.started_at}`;
}

function rankingUrl(kind, limit) {
  const params = new URLSearchParams();
  params.set("kind", kind);
  params.set("limit", String(limit));
  if (state.query) {
    params.set("query", state.query);
  }
  if (kind === "content" && state.activeGameId) {
    params.set("game_id", state.activeGameId);
  }
  if (kind === "content" && state.activeEventType) {
    params.set("event_type", state.activeEventType);
  }
  if (kind === "event" && state.activeGameId) {
    params.set("game_id", state.activeGameId);
  }
  return `/api/workbench/rankings?${params.toString()}`;
}

function renderGameItem(item) {
  const active = state.activeGameId === item.entity_id ? " active" : "";
  return `
    <button class="list-item dashboard-list-item${active}" type="button" data-action="select-game" data-game-id="${escapeHtml(
      item.entity_id
    )}">
      <div class="list-item-top">
        <strong>${escapeHtml(item.title)}</strong>
        <span class="list-pill">#${item.rank}</span>
      </div>
      <p>${escapeHtml(item.subtitle)}</p>
    </button>
  `;
}

function renderEventItem(item) {
  const value = item.event_type || item.entity_id;
  const active = state.activeEventType === value ? " active" : "";
  return `
    <button class="list-item dashboard-list-item${active}" type="button" data-action="select-event" data-event-type="${escapeHtml(
      value
    )}">
      <div class="list-item-top">
        <strong>${escapeHtml(item.title)}</strong>
        <span class="list-pill">#${item.rank}</span>
      </div>
      <p>${escapeHtml(item.subtitle)}</p>
    </button>
  `;
}

function renderContentItem(item) {
  const active =
    state.selectedContent?.candidate?.content_id === item.content_id ? " active" : "";
  return `
    <button class="list-item ranking-item dashboard-list-item${active}" type="button" data-action="select-content" data-content-id="${escapeHtml(
      item.content_id || ""
    )}">
      <div class="list-item-top">
        <strong>#${item.rank} ${escapeHtml(item.title)}</strong>
        <span class="status-pill">${formatScore(item.score)}</span>
      </div>
      <p>${escapeHtml(item.subtitle)}</p>
    </button>
  `;
}

function renderBreakdownSection(title, body) {
  const content = Array.isArray(body)
    ? `<ul class="narrative-list">${
        body.map((item) => `<li>${escapeHtml(item)}</li>`).join("")
      }</ul>`
    : `<p>${escapeHtml(body || "暂无内容")}</p>`;
  return `
    <article class="analysis-item workbench-breakdown-card">
      <h4>${escapeHtml(title)}</h4>
      ${content}
    </article>
  `;
}

function renderTopicPoolItem(item) {
  return `
    <article class="analysis-item">
      <div class="list-item-top">
        <strong>${escapeHtml(item.title)}</strong>
        <button class="ghost-action compact-action topic-remove-btn" data-action="remove-topic" data-topic-id="${escapeHtml(
          item.topic_id
        )}" type="button">移除</button>
      </div>
      <p>${formatOptional(item.game_name, "未归因游戏")} · 热度 ${formatScore(item.score)}</p>
      <p>${escapeHtml(item.note || "暂无备注")}</p>
    </article>
  `;
}

function renderBudgetItem(item) {
  return `
    <article class="analysis-item budget-item">
      <div class="list-item-top">
        <strong>${escapeHtml(item.provider)}</strong>
        <span class="list-pill">${item.enabled ? "启用" : "停用"}</span>
      </div>
      <p>月度 ${item.used_monthly}/${item.monthly_limit} · 今日 ${item.used_today}/${item.daily_soft_limit}</p>
      <p>保留池 ${item.reserve_pool} · 本轮可用 ${item.allowed_this_run}</p>
    </article>
  `;
}

function renderRunSummary() {
  const run = state.overview.latest_run;
  const title = document.getElementById("run-status-title");
  const copy = document.getElementById("run-status-copy");
  const topStatus = document.getElementById("top-run-status");
  if (title) {
    title.textContent = run ? run.status : "待执行";
  }
  if (copy) {
    copy.textContent = run
      ? `完成于 ${run.finished_at || run.started_at} · 候选 ${run.candidate_count} · 入榜 ${run.content_count}`
      : "尚未产生工作流结果";
  }
  if (topStatus) {
    topStatus.textContent = run ? run.status : "待执行";
  }
}

function renderFilters() {
  const gameMeta = document.getElementById("game-meta");
  const eventMeta = document.getElementById("event-meta");
  const clearGameButton = document.getElementById("clear-game-filter");
  const clearEventButton = document.getElementById("clear-event-filter");

  if (gameMeta) {
    gameMeta.textContent = state.activeGameId
      ? `已锁定 1 个游戏 · 共 ${state.gameRankings.length} 条候选`
      : `显示全部监控游戏 · 共 ${state.gameRankings.length} 条候选`;
  }
  if (eventMeta) {
    eventMeta.textContent = state.activeEventType
      ? `已锁定事件 ${state.activeEventType}`
      : `显示全部热点事件 · 共 ${state.eventRankings.length} 条候选`;
  }
  if (clearGameButton) {
    clearGameButton.classList.toggle("active", !state.activeGameId);
  }
  if (clearEventButton) {
    clearEventButton.classList.toggle("active", !state.activeEventType);
  }
}

function renderFocus() {
  const title = document.getElementById("focus-title");
  const subtitle = document.getElementById("focus-subtitle");
  const metrics = document.getElementById("focus-metrics");
  const detailTitle = document.getElementById("detail-title");
  const detailSubtitle = document.getElementById("detail-subtitle");
  const detailBreakdown = document.getElementById("detail-breakdown");
  const topicButton = document.getElementById("topic-add-btn");
  const sourceLink = document.getElementById("source-link");

  if (!state.selectedContent) {
    if (title) title.textContent = "等待选择内容";
    if (subtitle) {
      subtitle.textContent =
        "点击榜单、游戏或事件后，这里会同步显示当前聚焦对象的摘要和关键分数。";
    }
    if (metrics) metrics.innerHTML = "";
    if (detailTitle) detailTitle.textContent = "结构化拆解";
    if (detailSubtitle) {
      detailSubtitle.textContent =
        "选中一条内容后，这里会展示 Doubao 的拆解结果，包括摘要、爆点、改编角度与标题方向。";
    }
    if (detailBreakdown) detailBreakdown.innerHTML = "";
    if (topicButton) topicButton.setAttribute("disabled", "true");
    if (sourceLink) sourceLink.setAttribute("href", "#");
    return;
  }

  const { candidate, breakdown } = state.selectedContent;
  if (title) title.textContent = candidate.title;
  if (subtitle) {
    subtitle.textContent = `${formatOptional(candidate.game_name, "未归因游戏")} · ${formatOptional(
      candidate.event_type,
      "general"
    )} · ${candidate.published_at}`;
  }
  if (metrics) {
    metrics.innerHTML = [
      ["热度", formatScore(candidate.hotness_score)],
      ["相关性", formatScore(candidate.doubao_relevance_score)],
      ["交叉验证", formatScore(candidate.cross_source_score)],
      ["来源", candidate.source_domain],
    ]
      .map(
        ([label, value]) => `
          <div class="mini-stat">
            <span>${escapeHtml(label)}</span>
            <strong>${escapeHtml(value)}</strong>
          </div>
        `
      )
      .join("");
  }
  if (detailTitle) detailTitle.textContent = candidate.title;
  if (detailSubtitle) {
    detailSubtitle.textContent = `${candidate.author || "匿名来源"} · ${candidate.url}`;
  }
  if (detailBreakdown) {
    detailBreakdown.innerHTML = [
      renderBreakdownSection("内容摘要", breakdown.content_summary),
      renderBreakdownSection("爆点 / 钩子", breakdown.hook_points),
      renderBreakdownSection("核心冲突或价值", breakdown.core_conflict_or_value),
      renderBreakdownSection("适合人群", breakdown.audience_fit),
      renderBreakdownSection("改编角度", breakdown.adaptation_angles),
      renderBreakdownSection("标题方向", breakdown.title_directions),
      renderBreakdownSection("进入选题池理由", breakdown.topic_pool_reason),
    ].join("");
  }
  if (topicButton) topicButton.removeAttribute("disabled");
  if (sourceLink) sourceLink.setAttribute("href", candidate.url);
}

function renderPanels() {
  const gameList = document.getElementById("game-ranking-list");
  const eventList = document.getElementById("event-ranking-list");
  const contentList = document.getElementById("content-ranking-list");
  const topicPoolList = document.getElementById("topic-pool-list");
  const budgetGrid = document.getElementById("budget-grid");
  const topicCount = document.getElementById("top-topic-count");

  if (gameList) {
    gameList.innerHTML = state.gameRankings.length
      ? state.gameRankings.map(renderGameItem).join("")
      : `<p class="status-copy">当前还没有游戏榜数据。</p>`;
  }
  if (eventList) {
    eventList.innerHTML = state.eventRankings.length
      ? state.eventRankings.map(renderEventItem).join("")
      : `<p class="status-copy">当前还没有事件榜数据。</p>`;
  }
  if (contentList) {
    contentList.innerHTML = state.contentRankings.length
      ? state.contentRankings.map(renderContentItem).join("")
      : `<p class="status-copy">当前筛选条件下没有内容榜数据。</p>`;
  }
  if (topicPoolList) {
    topicPoolList.innerHTML = state.topicPool.length
      ? state.topicPool.map(renderTopicPoolItem).join("")
      : `<p class="status-copy">选题池暂时为空。</p>`;
  }
  if (budgetGrid) {
    budgetGrid.innerHTML = (state.overview.budgets || [])
      .map(renderBudgetItem)
      .join("");
  }
  if (topicCount) {
    topicCount.textContent = String(state.topicPool.length);
  }

  renderFilters();
  renderRunSummary();
  renderFocus();
}

function applySettingsForm(settings) {
  const form = document.getElementById("settings-form");
  if (!form) return;
  form.elements.watchlist_games.value = (settings.watchlist_games || []).join("\n");
  form.elements.keyword_templates.value = (settings.keyword_templates || []).join("\n");
  form.elements.time_window_hours.value = settings.time_window_hours ?? 24;
  form.elements.schedule_interval_hours.value = settings.schedule_interval_hours ?? 2;
  form.elements.max_candidates_per_run.value = settings.max_candidates_per_run ?? 50;
  form.elements.doubao_model.value = settings.doubao_model || "";
}

async function syncRankings() {
  const [gameRankings, eventRankings, contentRankings] = await Promise.all([
    fetchJson(rankingUrl("game", 18)),
    fetchJson(rankingUrl("event", 18)),
    fetchJson(rankingUrl("content", 24)),
  ]);

  state.gameRankings = gameRankings;
  state.eventRankings = eventRankings;
  state.contentRankings = contentRankings;

  if (
    state.selectedContent &&
    !contentRankings.some(
      (item) => item.content_id === state.selectedContent?.candidate?.content_id
    )
  ) {
    state.selectedContent = null;
  }

  if (!state.selectedContent) {
    const first = contentRankings.find((item) => item.content_id);
    if (first?.content_id) {
      await selectContent(first.content_id);
      return;
    }
  }

  renderPanels();
}

async function refreshDashboard() {
  const [overview, topicPool] = await Promise.all([
    fetchJson("/api/workbench/overview"),
    fetchJson("/api/workbench/topic-pool"),
  ]);
  state.overview = overview;
  state.topicPool = topicPool;
  applySettingsForm(overview.settings);
  await syncRankings();
}

async function selectContent(contentId) {
  if (!contentId) return;
  state.selectedContent = await fetchJson(
    `/api/workbench/content/${encodeURIComponent(contentId)}`
  );
  renderPanels();
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
  if (!state.selectedContent) return;
  const note = document.getElementById("topic-note")?.value?.trim() || "";
  await fetchJson("/api/workbench/topic-pool", {
    method: "POST",
    body: JSON.stringify({
      content_id: state.selectedContent.candidate.content_id,
      note,
    }),
  });
  await refreshDashboard();
}

async function removeTopicPool(topicId) {
  await fetchJson(`/api/workbench/topic-pool/${encodeURIComponent(topicId)}`, {
    method: "DELETE",
  });
  await refreshDashboard();
}

async function saveSettings(form) {
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
  applySettingsForm(state.overview.settings);
}

async function logout() {
  await fetchJson("/api/auth/logout", { method: "POST" });
  window.location.assign("/login");
}

let searchTimer = null;

let state = {
  overview: decodeState(root?.dataset.state || "")?.overview || {
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
  selectedContent: null,
  gameRankings: [],
  eventRankings: [],
  contentRankings: [],
  activeGameId: null,
  activeEventType: null,
  query: "",
};

document.addEventListener("click", async (event) => {
  const target = event.target;
  if (!(target instanceof HTMLElement)) return;

  if (target.id === "run-now-btn") {
    await runNow(target);
    return;
  }

  if (target.id === "logout-btn") {
    await logout();
    return;
  }

  if (target.id === "topic-add-btn") {
    await addTopicPool();
    return;
  }

  if (target.id === "clear-game-filter") {
    state.activeGameId = null;
    state.activeEventType = null;
    await syncRankings();
    return;
  }

  if (target.id === "clear-event-filter") {
    state.activeEventType = null;
    await syncRankings();
    return;
  }

  const actionTarget = target.closest("[data-action]");
  if (!(actionTarget instanceof HTMLElement)) return;

  const action = actionTarget.dataset.action;
  if (action === "select-game") {
    const nextValue = actionTarget.dataset.gameId || null;
    state.activeGameId = state.activeGameId === nextValue ? null : nextValue;
    state.activeEventType = null;
    await syncRankings();
    return;
  }

  if (action === "select-event") {
    const nextValue = actionTarget.dataset.eventType || null;
    state.activeEventType = state.activeEventType === nextValue ? null : nextValue;
    await syncRankings();
    return;
  }

  if (action === "select-content" && actionTarget.dataset.contentId) {
    await selectContent(actionTarget.dataset.contentId);
    return;
  }

  if (action === "remove-topic" && actionTarget.dataset.topicId) {
    await removeTopicPool(actionTarget.dataset.topicId);
  }
});

document.getElementById("workbench-search")?.addEventListener("input", (event) => {
  const nextValue = event.target.value.trim();
  state.query = nextValue;
  window.clearTimeout(searchTimer);
  searchTimer = window.setTimeout(() => {
    syncRankings().catch((error) => {
      console.error(error);
      window.alert(`筛选失败：${error.message || error}`);
    });
  }, 220);
});

document.getElementById("settings-form")?.addEventListener("submit", async (event) => {
  event.preventDefault();
  try {
    await saveSettings(event.currentTarget);
    window.alert("策略已保存。");
  } catch (error) {
    console.error(error);
    window.alert(`保存策略失败：${error.message || error}`);
  }
});

refreshDashboard().catch((error) => {
  console.error(error);
  window.alert(`工作台初始化失败：${error.message || error}`);
});
