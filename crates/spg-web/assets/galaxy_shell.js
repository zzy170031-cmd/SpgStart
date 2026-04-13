import "/assets/galaxy_wire_v2.js";

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

document.addEventListener("click", async (event) => {
  const target = event.target;
  if (!(target instanceof HTMLElement)) return;
  if (target.id !== "logout-btn") return;

  target.setAttribute("disabled", "true");
  try {
    await fetchJson("/api/auth/logout", { method: "POST" });
    window.location.assign("/login");
  } catch (error) {
    console.error(error);
    window.alert(`退出登录失败：${error.message || error}`);
    target.removeAttribute("disabled");
  }
});
