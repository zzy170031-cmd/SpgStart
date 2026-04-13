const root = document.getElementById("setup-root");

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

function statusClass(status) {
  return `status-pill status-${String(status).replaceAll("_", "-")}`;
}

function providerTier(provider) {
  return provider.is_required
    ? '<span class="tier-pill required">必需</span>'
    : '<span class="tier-pill optional">可选</span>';
}

function renderProviderCard(provider) {
  const verified = provider.last_verified_at || "尚未验证";
  const keyHint = provider.api_key_summary ? `已保存 ${provider.api_key_summary}` : "未保存 Key";
  const healthHint = provider.last_error
    ? `最近错误：${provider.last_error}`
    : `最近验证：${verified}`;

  return `
    <article class="provider-row" data-provider="${escapeHtml(provider.provider)}">
      <div class="provider-row-main">
        <div class="provider-row-intro">
          <div class="provider-title-row provider-title-row-compact">
            <p class="provider-name">${escapeHtml(provider.display_name)}</p>
            ${providerTier(provider)}
            <span class="${statusClass(provider.status)}">${escapeHtml(provider.status)}</span>
          </div>
          <p class="provider-row-note">${escapeHtml(keyHint)} · ${escapeHtml(healthHint)}</p>
        </div>

        <div class="provider-row-fields">
          <label class="field field-inline provider-inline-flag">
            <span>启用</span>
            <input class="toggle-input" name="enabled" type="checkbox" ${provider.enabled ? "checked" : ""}/>
          </label>

          <label class="field provider-inline-field provider-inline-field-key">
            <span>API Key</span>
            <input class="text-input" name="api_key" type="password" value="" placeholder="输入新的 API Key" autocomplete="new-password" spellcheck="false"/>
          </label>
        </div>

        <div class="provider-row-actions">
          <button class="ghost-action compact-action provider-row-toggle" data-action="toggle-advanced" type="button" aria-expanded="false">地址</button>
          <button class="secondary-action compact-action" data-action="save" type="button">保存配置</button>
          <button class="primary-action compact-action" data-action="test" type="button">测试 API</button>
        </div>
      </div>

      <div class="provider-row-advanced" hidden>
        <label class="field field-inline provider-inline-flag">
          <span>清除 Key</span>
          <input class="toggle-input" name="clear_api_key" type="checkbox"/>
        </label>
        <label class="field provider-inline-field provider-inline-field-url">
          <span>Base URL</span>
          <input class="text-input" name="base_url" type="text" value="${escapeHtml(provider.base_url)}"/>
        </label>
      </div>
    </article>
  `;
}

function renderUnlockPanel(unlockState) {
  const blockers = unlockState.blockers.length
    ? unlockState.blockers.map((item) => `<li>${escapeHtml(item)}</li>`).join("")
    : "<li>五个必需 API 全部就绪。</li>";

  return `
    <div class="unlock-metrics unlock-metrics-compact">
      <div class="mini-stat">
        <span>必需 Ready</span>
        <strong>${unlockState.ready_count}/${unlockState.total_count}</strong>
      </div>
      <div class="mini-stat">
        <span>当前状态</span>
        <strong>${unlockState.unlocked ? "已解锁" : "待完成"}</strong>
      </div>
    </div>
    <p class="status-copy compact-copy">
      ${
        unlockState.unlocked
          ? "所有必需 API 已验证通过，现在可以把通过态写入本机并进入主系统。"
          : "仍有必需 API 未完成保存或测试，主系统保持锁定。"
      }
    </p>
    <ul class="blocker-list blocker-list-compact">${blockers}</ul>
  `;
}

async function fetchJson(url, options = {}) {
  const response = await fetch(url, {
    headers: { "Content-Type": "application/json" },
    ...options,
  });
  if (!response.ok) {
    const message = await response.text();
    if (response.status === 403 && message === "login_required") {
      window.location.replace("/login");
      throw new Error("login_required");
    }
    throw new Error(message || "request_failed");
  }
  return response.json();
}

function findProvider(providerName) {
  return state.providers.find((item) => item.provider === providerName);
}

function readCardPayload(provider, card) {
  const apiKeyInput = card.querySelector('[name="api_key"]');
  const apiKey = apiKeyInput.value.trim();
  const clearApiKey = card.querySelector('[name="clear_api_key"]').checked;

  return {
    enabled: card.querySelector('[name="enabled"]').checked,
    api_key: apiKey.length > 0 ? apiKey : null,
    clear_api_key: clearApiKey && apiKey.length === 0,
    base_url: card.querySelector('[name="base_url"]').value.trim(),
    monthly_limit: provider.monthly_limit,
    daily_soft_limit: provider.daily_soft_limit,
    default_role: provider.default_role,
    priority: provider.priority,
  };
}

function syncPageState() {
  const requiredChip = document.getElementById("required-progress");
  const unlockChip = document.getElementById("unlock-state-label");
  if (requiredChip) {
    requiredChip.textContent = `${state.unlock_state.ready_count}/${state.unlock_state.total_count}`;
  }
  if (unlockChip) {
    unlockChip.textContent = state.unlock_state.unlocked ? "已解锁" : "待完成";
  }

  const advanceButton = document.getElementById("advance-gate-btn");
  if (advanceButton) {
    advanceButton.toggleAttribute("disabled", !state.unlock_state.unlocked);
    advanceButton.classList.toggle("disabled", !state.unlock_state.unlocked);
  }
}

async function refreshState() {
  const [providers, unlockState, gateState] = await Promise.all([
    fetchJson("/api/providers"),
    fetchJson("/api/unlock-state"),
    fetchJson("/api/gate-state"),
  ]);
  state.providers = providers;
  state.unlock_state = unlockState;
  state.gate_state = gateState;
  render();
}

async function saveProvider(provider, card) {
  const providerView = findProvider(provider);
  if (!providerView) {
    throw new Error(`unknown_provider:${provider}`);
  }
  const payload = readCardPayload(providerView, card);
  await fetchJson(`/api/providers/${provider}`, {
    method: "PUT",
    body: JSON.stringify(payload),
  });
  await refreshState();
}

async function testProvider(provider) {
  await fetchJson(`/api/providers/${provider}/test`, {
    method: "POST",
  });
  await refreshState();
}

async function testAll() {
  await fetchJson("/api/providers/test-all", {
    method: "POST",
  });
  await refreshState();
}

async function advanceGate() {
  await fetchJson("/api/gate/advance", {
    method: "POST",
  });
  window.location.assign("/galaxy");
}

async function logout() {
  await fetchJson("/api/auth/logout", {
    method: "POST",
  });
  window.location.assign("/login");
}

function render() {
  const cards = document.getElementById("provider-cards");
  const unlockPanel = document.getElementById("unlock-panel");
  if (cards) {
    cards.innerHTML = state.providers.map(renderProviderCard).join("");
  }
  if (unlockPanel) {
    unlockPanel.innerHTML = renderUnlockPanel(state.unlock_state);
  }
  syncPageState();
}

let state = decodeState(root?.dataset.state || "") || {
  mode: "setup",
  gate_state: null,
  providers: [],
  unlock_state: {
    ready_count: 0,
    total_count: 5,
    optional_ready_count: 0,
    optional_total_count: 0,
    unlocked: false,
    blockers: [],
  },
};

document.addEventListener("click", async (event) => {
  const target = event.target;
  if (!(target instanceof HTMLElement)) return;

  if (target.id === "test-all-btn") {
    target.setAttribute("disabled", "true");
    try {
      await testAll();
    } catch (error) {
      console.error(error);
      window.alert(`批量测试失败：${error.message || error}`);
    } finally {
      target.removeAttribute("disabled");
    }
    return;
  }

  if (target.id === "advance-gate-btn") {
    target.setAttribute("disabled", "true");
    try {
      await advanceGate();
    } catch (error) {
      console.error(error);
      window.alert(`主系统解锁失败：${error.message || error}`);
    } finally {
      target.removeAttribute("disabled");
    }
    return;
  }

  if (target.id === "logout-btn") {
    target.setAttribute("disabled", "true");
    try {
      await logout();
    } catch (error) {
      console.error(error);
      window.alert(`退出登录失败：${error.message || error}`);
      target.removeAttribute("disabled");
    }
    return;
  }

  const actionButton = target.closest("[data-action]");
  if (!(actionButton instanceof HTMLElement)) return;
  const card = actionButton.closest(".provider-card");
  const providerRow = actionButton.closest(".provider-row");
  const cardHost = providerRow instanceof HTMLElement ? providerRow : card;
  if (!(cardHost instanceof HTMLElement)) return;
  const provider =
    card instanceof HTMLElement ? card.dataset.provider : null;
  const providerName =
    providerRow instanceof HTMLElement ? providerRow.dataset.provider : provider;
  if (!providerName) return;

  if (actionButton.dataset.action === "toggle-advanced") {
    const advanced = cardHost.querySelector(".provider-row-advanced");
    if (!(advanced instanceof HTMLElement)) return;
    const expanded = !advanced.hasAttribute("hidden");
    if (expanded) {
      advanced.setAttribute("hidden", "true");
      actionButton.setAttribute("aria-expanded", "false");
      actionButton.textContent = "地址";
    } else {
      advanced.removeAttribute("hidden");
      actionButton.setAttribute("aria-expanded", "true");
      actionButton.textContent = "收起地址";
    }
    return;
  }

  actionButton.setAttribute("disabled", "true");
  try {
    if (actionButton.dataset.action === "save") {
      await saveProvider(providerName, cardHost);
    } else if (actionButton.dataset.action === "test") {
      await testProvider(providerName);
    }
  } catch (error) {
    console.error(error);
    window.alert(`操作失败：${error.message || error}`);
  } finally {
    actionButton.removeAttribute("disabled");
  }
});

render();
