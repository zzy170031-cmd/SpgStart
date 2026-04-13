const root = document.getElementById("login-root");

function decodeState(encoded) {
  if (!encoded) return null;
  const binary = atob(encoded);
  const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));
  return JSON.parse(new TextDecoder().decode(bytes));
}

async function fetchJson(url, options = {}) {
  const response = await fetch(url, {
    headers: { "Content-Type": "application/json" },
    ...options,
  });
  if (!response.ok) {
    throw new Error(await response.text());
  }
  return response.json();
}

let state = decodeState(root?.dataset.state || "") || {
  gate_state: { current_stage: "login" },
};

async function confirmLocalUser() {
  state.gate_state = await fetchJson("/api/auth/confirm", {
    method: "POST",
  });
  window.location.assign("/setup/providers");
}

document.getElementById("confirm-login-btn")?.addEventListener("click", async (event) => {
  const button = event.currentTarget;
  if (!(button instanceof HTMLElement)) return;
  button.setAttribute("disabled", "true");
  try {
    await confirmLocalUser();
  } catch (error) {
    console.error(error);
    window.alert(`本地用户确认失败：${error.message || error}`);
    button.removeAttribute("disabled");
  }
});
