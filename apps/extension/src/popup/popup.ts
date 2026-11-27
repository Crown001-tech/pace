import {
  DEFAULT_API_URL,
  ExtensionConfig,
  ProjectSummary,
  RecentSendItem,
} from "../types";

const connBadge = document.getElementById("conn-badge") as HTMLSpanElement;
const pairView = document.getElementById("pair-view") as HTMLDivElement;
const connectedView = document.getElementById("connected-view") as HTMLDivElement;

const apiUrlInput = document.getElementById("api-url") as HTMLInputElement;
const pairCodeInput = document.getElementById("pair-code") as HTMLInputElement;
const pairBtn = document.getElementById("pair-btn") as HTMLButtonElement;
const pairStatus = document.getElementById("pair-status") as HTMLDivElement;

const userEmailEl = document.getElementById("user-email") as HTMLElement;
const projectSelect = document.getElementById("project-select") as HTMLSelectElement;
const refreshBtn = document.getElementById("refresh-btn") as HTMLButtonElement;
const openFlowBtn = document.getElementById("open-flow-btn") as HTMLButtonElement;
const disconnectBtn = document.getElementById("disconnect-btn") as HTMLButtonElement;
const connStatus = document.getElementById("conn-status") as HTMLDivElement;
const recentList = document.getElementById("recent-list") as HTMLDivElement;

function render(
  config: ExtensionConfig,
  projects: ProjectSummary[],
  recentSends: RecentSendItem[]
) {
  const isConnected = Boolean(config.token);
  connBadge.textContent = isConnected ? "Connected" : "Disconnected";
  pairView.classList.toggle("hidden", isConnected);
  connectedView.classList.toggle("hidden", !isConnected);
  apiUrlInput.value = config.apiUrl || DEFAULT_API_URL;

  if (!isConnected) return;

  userEmailEl.textContent = config.userEmail || "PACE Creator";
  projectSelect.innerHTML = "";

  if (projects.length === 0) {
    const opt = document.createElement("option");
    opt.value = "";
    opt.textContent = "No active projects found";
    projectSelect.appendChild(opt);
  } else {
    for (const p of projects) {
      const opt = document.createElement("option");
      opt.value = p.id;
      opt.textContent = `${p.name} (${p.aspect_ratio})`;
      if (p.id === config.selectedProjectId) opt.selected = true;
      projectSelect.appendChild(opt);
    }
  }

  recentList.innerHTML = "";
  if (recentSends.length === 0) {
    recentList.innerHTML = '<div style="color:var(--muted);">No media sent yet.</div>';
  } else {
    for (const item of recentSends.slice(0, 8)) {
      const row = document.createElement("div");
      row.className = "recent-item";
      row.innerHTML = `<span>${item.filename}</span><span>${item.duplicate ? "Already saved" : "✓ Sent"}</span>`;
      recentList.appendChild(row);
    }
  }
}

async function loadState() {
  const res = await chrome.runtime.sendMessage({ type: "PACE_GET_STATE" });
  if (res?.ok) {
    render(res.config, res.projects || [], res.recentSends || []);
  }
}

pairBtn.addEventListener("click", async () => {
  pairStatus.className = "status";
  pairStatus.textContent = "Pairing with PACE...";
  pairBtn.disabled = true;
  try {
    const res = await chrome.runtime.sendMessage({
      type: "PACE_PAIR",
      apiUrl: apiUrlInput.value,
      code: pairCodeInput.value,
    });
    if (!res?.ok) throw new Error(res?.error || "Pairing failed");
    pairCodeInput.value = "";
    pairStatus.textContent = "";
    await loadState();
  } catch (err) {
    pairStatus.className = "status err";
    pairStatus.textContent = err instanceof Error ? err.message : String(err);
  } finally {
    pairBtn.disabled = false;
  }
});

projectSelect.addEventListener("change", async () => {
  const opt = projectSelect.options[projectSelect.selectedIndex];
  if (!opt?.value) return;
  await chrome.runtime.sendMessage({
    type: "PACE_SELECT_PROJECT",
    projectId: opt.value,
    projectName: opt.textContent || "",
  });
  connStatus.className = "status";
  connStatus.textContent = "Target project updated";
});

refreshBtn.addEventListener("click", async () => {
  connStatus.className = "status";
  connStatus.textContent = "Refreshing projects...";
  const res = await chrome.runtime.sendMessage({ type: "PACE_REFRESH_PROJECTS" });
  if (res?.ok) {
    connStatus.textContent = `Loaded ${res.projects.length} project(s)`;
    await loadState();
  } else {
    connStatus.className = "status err";
    connStatus.textContent = res?.error || "Failed refreshing";
  }
});

openFlowBtn.addEventListener("click", () => {
  chrome.tabs.create({ url: "https://flow.google.com/" });
});

disconnectBtn.addEventListener("click", async () => {
  await chrome.runtime.sendMessage({ type: "PACE_DISCONNECT" });
  await loadState();
});

void loadState();
