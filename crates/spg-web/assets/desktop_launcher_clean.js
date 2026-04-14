const root = document.getElementById("desktop-launcher-root");

function parseState() {
  if (!root) return null;
  const raw = root.dataset.state;
  if (!raw) return null;
  return JSON.parse(atob(raw));
}

function $(id) {
  return document.getElementById(id);
}

async function fetchJson(url, options = {}) {
  const response = await fetch(url, {
    headers: { "Content-Type": "application/json" },
    credentials: "same-origin",
    ...options,
  });
  if (!response.ok) {
    const text = await response.text();
    throw new Error(text || `HTTP ${response.status}`);
  }
  return response.json();
}

function nextRoute(gateState) {
  if (!gateState.local_user_confirmed) return "/setup/providers";
  return gateState.api_gate_passed ? "/galaxy" : "/setup/providers";
}

function render(page, gateState = page.gate_state) {
  $("current-version").textContent = page.app_version || "-";
  $("identity-username").textContent = page.local_user.username || "-";
  $("identity-machine").textContent = page.local_user.device_name || "-";
  $("identity-status").textContent = gateState.local_user_confirmed ? "已确认" : "未确认";
  $("current-stage").textContent = gateState.local_user_confirmed
    ? gateState.api_gate_passed
      ? "第三页 / 主系统"
      : "第二页 / Provider 配置"
    : "第一页 / 用户确认";
  $("next-step").textContent = gateState.local_user_confirmed
    ? gateState.api_gate_passed
      ? "进入主系统"
      : "进入 Provider 配置"
    : "确认当前 Windows 用户";

  const confirmButton = $("confirm-identity");
  confirmButton.textContent = gateState.local_user_confirmed
    ? gateState.api_gate_passed
      ? "继续进入主系统"
      : "继续进入 Provider 配置"
    : "确认并进入下一步";
}

async function run() {
  const page = parseState();
  if (!page) return;
  render(page);

  $("confirm-identity").addEventListener("click", async () => {
    const button = $("confirm-identity");
    button.setAttribute("disabled", "true");
    try {
      const gateState = await fetchJson("/api/auth/confirm", { method: "POST" });
      render(page, gateState);
      window.location.replace(nextRoute(gateState));
    } catch (error) {
      window.alert(`确认失败：${error.message || error}`);
    } finally {
      button.removeAttribute("disabled");
    }
  });

  $("reset-profile").addEventListener("click", async () => {
    try {
      const gateState = await fetchJson("/api/auth/logout", { method: "POST" });
      render(page, gateState);
      window.alert("已清空当前机器的确认状态。你可以重新确认本机身份。");
    } catch (error) {
      window.alert(`重置失败：${error.message || error}`);
    }
  });
}

run().catch((error) => {
  window.alert(`初始化失败：${String(error)}`);
});
