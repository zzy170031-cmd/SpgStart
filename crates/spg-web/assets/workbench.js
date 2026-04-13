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

function renderRankingItem(item) {
  const contentId = item.content_id ? `data-content-id="${escapeHtml(item.content_id)}"` : "";
  return `
    <button class="list-item ranking-item" type="button" data-kind="${escapeHtml(item.kind)}" ${contentId}>
      <div class="list-item-top">
        <strong>#${item.rank} ${escapeHtml(item.title)}</strong>
        <span class="status-pill">${formatScore(item.score)}</span>
      </div>
      <p>${escapeHtml(item.subtitle)}</p>
    </button>
  `;
}

function renderMetricCard(label, value) {
  return `
    <div class="mini-stat">
      <span>${escapeHtml(label)}</span>
      <strong>${escapeHtml(value)}</strong>
    </div>
  `;
}

function renderBreakdownItem(title, items) {
  const body = Array.isArray(items)
    ? `<ul class="narrative-list">${items
        .map((item) => `<li>${escapeHtml(item)}</li>`)
        .join("")}</ul>`
    : `<p>${escapeHtml(items)}</p>`;
  return `
    <article class="analysis-item">
      <h4>${escapeHtml(title)}</h4>
      ${body}
    </article>
  `;
}

function renderTopicPoolItem(item) {
  return `
    <article class="analysis-item">
      <div class="list-item-top">
        <strong>${escapeHtml(item.title)}</strong>
        <button class="ghost-action compact-action topic-remove-btn" data-topic-id="${escapeHtml(
          item.topic_id
        )}" type="button">移除</button>
      </div>
      <p>${escapeHtml(item.game_name || "未归因游戏")} | 热度 ${formatScore(item.score)}</p>
      <p>${escapeHtml(item.note || "暂无备注")}</p>
    </article>
  `;
}

function renderBudgetItem(item) {
  return `
    <article class="analysis-item">
      <h4>${escapeHtml(item.provider)}</h4>
      <p>${item.enabled ? "已启用" : "已停用"}，本月 ${item.used_monthly}/${item.monthly_limit}，今日 ${item.used_today}/${item.daily_soft_limit}</p>
      <p>保留池 ${item.reserve_pool}，本轮可用 ${item.allowed_this_run}</p>
    </article>
  `;
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

function renderOverview() {
  const rankings = state.overview[`${state.activeKind}_rankings`] || [];
  const rankingList = document.getElementById("ranking-list");
  if (rankingList) {
    rankingList.innerHTML = rankings.map(renderRankingItem).join("") || `<p class="status-copy">当前还没有 ${state.activeKind} 榜快照。</p>`;
  }

  const topicPoolList = document.getElementById("topic-pool-list");
  if (topicPoolList) {
    topicPoolList.innerHTML = state.topicPool.map(renderTopicPoolItem).join("") || `<p class="status-copy">选题池暂时为空。</p>`;
  }

  const budgetGrid = document.getElementById("budget-grid");
  if (budgetGrid) {
    budgetGrid.innerHTML = (state.overview.budgets || [])
      .map(renderBudgetItem)
      .join("");
  }

  const latestRun = state.overview.latest_run;
  const latestRunPill = document.getElementById("latest-run-pill");
  if (latestRunPill) {
    latestRunPill.textContent = latestRun
      ? `${latestRun.status} · ${latestRun.finished_at || latestRun.started_at}`
      : "未执行";
  }

  const topicPoolCountChip = document.querySelectorAll(".status-chip strong")[2];
  if (topicPoolCountChip) {
    topicPoolCountChip.textContent = String(state.topicPool.length);
  }

  applySettingsForm(state.overview.settings);
}

function renderDetail() {
  const detailTitle = document.getElementById("detail-title");
  const detailSubtitle = document.getElementById("detail-subtitle");
  const detailMetrics = document.getElementById("detail-metrics");
  const detailBreakdown = document.getElementById("detail-breakdown");
  const topicButton = document.getElementById("topic-add-btn");
  const sourceLink = document.getElementById("source-link");

  if (!state.selectedContent) {
    if (detailTitle) detailTitle.textContent = "等待选择内容";
    if (detailSubtitle) detailSubtitle.textContent = "点击左侧内容榜条目后，这里会展开结构化拆解。";
    if (detailMetrics) detailMetrics.innerHTML = "";
    if (detailBreakdown) detailBreakdown.innerHTML = "";
    if (topicButton) topicButton.setAttribute("disabled", "true");
    if (sourceLink) sourceLink.setAttribute("href", "#");
    return;
  }

  const { candidate, breakdown } = state.selectedContent;
  if (detailTitle) detailTitle.textContent = candidate.title;
  if (detailSubtitle) {
    detailSubtitle.textContent = `${candidate.game_name || "未归因游戏"} | ${
      candidate.event_type || "general"
    } | ${candidate.published_at}`;
  }
  if (detailMetrics) {
    detailMetrics.innerHTML = [
      renderMetricCard("热度", formatScore(candidate.hotness_score)),
      renderMetricCard("相关性", formatScore(candidate.doubao_relevance_score)),
      renderMetricCard("交叉验证", formatScore(candidate.cross_source_score)),
      renderMetricCard("来源", candidate.source_domain),
    ].join("");
  }
  if (detailBreakdown) {
    detailBreakdown.innerHTML = [
      renderBreakdownItem("内容摘要", breakdown.content_summary),
      renderBreakdownItem("爆点 / 钩子", breakdown.hook_points),
      renderBreakdownItem("核心冲突或价值", breakdown.core_conflict_or_value),
      renderBreakdownItem("适合人群", breakdown.audience_fit),
      renderBreakdownItem("改编角度", breakdown.adaptation_angles),
      renderBreakdownItem("标题方向", breakdown.title_directions),
      renderBreakdownItem("选题池理由", breakdown.topic_pool_reason),
    ].join("");
  }
  if (topicButton) topicButton.removeAttribute("disabled");
  if (sourceLink) sourceLink.setAttribute("href", candidate.url);
}

async function refreshOverview(search = "") {
  const [overview, topicPool] = await Promise.all([
    fetchJson(
      `/api/workbench/overview${search ? `?query=${encodeURIComponent(search)}` : ""}`
    ),
    fetchJson("/api/workbench/topic-pool"),
  ]);
  state.overview = overview;
  state.topicPool = topicPool;
  renderOverview();
  if (!state.selectedContent && overview.content_rankings.length) {
    const first = overview.content_rankings.find((item) => item.content_id);
    if (first?.content_id) {
      await selectContent(first.content_id);
    }
  } else {
    renderDetail();
  }
}

async function selectContent(contentId) {
  const detail = await fetchJson(`/api/workbench/content/${encodeURIComponent(contentId)}`);
  state.selectedContent = detail;
  renderDetail();
}

async function runNow(button) {
  button?.setAttribute("disabled", "true");
  try {
    const result = await fetchJson("/api/workbench/runs/run-now", { method: "POST" });
    await refreshOverview(document.getElementById("ranking-search")?.value?.trim() || "");
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
  await refreshOverview(document.getElementById("ranking-search")?.value?.trim() || "");
}

async function removeTopicPool(topicId) {
  await fetchJson(`/api/workbench/topic-pool/${encodeURIComponent(topicId)}`, {
    method: "DELETE",
  });
  await refreshOverview(document.getElementById("ranking-search")?.value?.trim() || "");
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

let state = decodeState(root?.dataset.state || "") || {
  activeKind: "content",
  overview: {
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
};
state.activeKind = "content";

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

  const tabButton = target.closest(".tab-button");
  if (tabButton instanceof HTMLElement) {
    document.querySelectorAll(".tab-button").forEach((button) => {
      button.classList.toggle("active", button === tabButton);
    });
    state.activeKind = tabButton.dataset.kind || "content";
    renderOverview();
    return;
  }

  const rankingItem = target.closest(".ranking-item");
  if (rankingItem instanceof HTMLElement && rankingItem.dataset.contentId) {
    await selectContent(rankingItem.dataset.contentId);
    return;
  }

  if (target.id === "topic-add-btn") {
    await addTopicPool();
    return;
  }

  if (target.classList.contains("topic-remove-btn")) {
    await removeTopicPool(target.dataset.topicId);
  }
});

document.getElementById("ranking-search")?.addEventListener("input", async (event) => {
  const value = event.target.value.trim();
  const endpoint = `/api/workbench/rankings?kind=${encodeURIComponent(
    state.activeKind
  )}&query=${encodeURIComponent(value)}`;
  const rankings = await fetchJson(endpoint);
  state.overview[`${state.activeKind}_rankings`] = rankings;
  renderOverview();
});

document.getElementById("settings-form")?.addEventListener("submit", async (event) => {
  event.preventDefault();
  await saveSettings(event.currentTarget);
});

refreshOverview().catch((error) => {
  console.error(error);
  window.alert(`工作台初始化失败：${error.message || error}`);
});
