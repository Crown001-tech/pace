import { ExtensionConfig, GenerationMode, QueueState } from "../types";
import {
  tryCaptureMediaBytesAsBase64,
  findGeneratedMediaElements,
  findPromptInput,
} from "./flow-adapter";
import {
  FlowQueueRunner,
  hasValidExtensionContext,
  isContextInvalidatedError,
  parsePromptsText,
} from "./queue-runner";

function injectDockStyles() {
  if (document.getElementById("pace-flow-styles")) return;
  const style = document.createElement("style");
  style.id = "pace-flow-styles";
  style.textContent = `
    #pace-flow-dock {
      position: fixed;
      right: 18px;
      bottom: 18px;
      z-index: 2147483640;
      width: 440px;
      max-width: calc(100vw - 32px);
      max-height: 86vh;
      display: flex;
      flex-direction: column;
      background: #0e0c15;
      color: #f3f0fa;
      border: 1px solid rgba(169, 112, 255, 0.5);
      border-radius: 14px;
      box-shadow: 0 20px 56px rgba(0, 0, 0, 0.65);
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
      font-size: 13px;
      overflow: hidden;
      transition: width 0.15s ease;
    }
    #pace-flow-dock.expanded {
      width: 580px;
    }
    #pace-flow-dock.collapsed {
      width: auto;
    }
    .pace-dock-hdr {
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 12px 14px;
      background: #171226;
      border-bottom: 1px solid #2a2538;
      cursor: pointer;
    }
    .pace-dock-title {
      font-weight: 700;
      font-size: 14px;
      color: #f3f0fa;
      display: flex;
      align-items: center;
      gap: 8px;
    }
    .pace-pill {
      font-size: 11px;
      padding: 2px 8px;
      border-radius: 6px;
      background: rgba(169, 112, 255, 0.18);
      color: #a970ff;
      font-weight: 600;
    }
    .pace-dock-body {
      padding: 12px 14px;
      overflow-y: auto;
      display: flex;
      flex-direction: column;
      gap: 10px;
    }
    .pace-row { display: flex; gap: 8px; align-items: center; }
    .pace-btn {
      padding: 8px 12px;
      border-radius: 8px;
      border: 1px solid #2a2538;
      background: #1c182b;
      color: #f3f0fa;
      font-size: 12.5px;
      font-weight: 600;
      cursor: pointer;
    }
    .pace-btn:hover { border-color: #a970ff; }
    .pace-btn-primary {
      background: #a970ff;
      color: #140b29;
      border-color: #a970ff;
    }
    .pace-btn-warn {
      background: rgba(245, 158, 11, 0.18);
      color: #fcd34d;
      border-color: rgba(245, 158, 11, 0.4);
    }
    .pace-textarea {
      width: 100%;
      min-height: 150px;
      padding: 10px;
      border-radius: 8px;
      border: 1px solid #2a2538;
      background: #09070f;
      color: #f3f0fa;
      font-size: 12.5px;
      line-height: 1.45;
      resize: vertical;
      box-sizing: border-box;
    }
    #pace-flow-dock.expanded .pace-textarea {
      min-height: 230px;
    }
    .pace-status-box {
      padding: 9px 11px;
      border-radius: 8px;
      background: #151220;
      border: 1px solid #2a2538;
      font-size: 12px;
      line-height: 1.4;
      color: #c4b5fd;
    }
    .pace-send-btn {
      position: absolute;
      top: 8px;
      right: 8px;
      z-index: 9999;
      padding: 5px 9px;
      border-radius: 7px;
      border: 1px solid rgba(169, 112, 255, 0.65);
      background: rgba(20, 11, 41, 0.9);
      color: #f3f0fa;
      font-size: 11px;
      font-weight: 700;
      cursor: pointer;
      backdrop-filter: blur(6px);
    }
    .pace-send-btn:hover {
      background: #a970ff;
      color: #140b29;
    }
  `;
  document.head.appendChild(style);
}

function mountFlowDock() {
  if (document.getElementById("pace-flow-dock")) return;
  injectDockStyles();

  const dock = document.createElement("div");
  dock.id = "pace-flow-dock";
  dock.innerHTML = `
    <div class="pace-dock-hdr" id="pace-dock-toggle">
      <div class="pace-dock-title">
        <span>PACE × Flow</span>
        <span class="pace-pill" id="pace-dock-badge">Idle</span>
      </div>
      <div style="display:flex; align-items:center; gap:8px;">
        <button type="button" id="pace-dock-expand" title="Expand / Compact size" style="background:none; border:1px solid #2a2538; color:#c4b5fd; border-radius:6px; padding:2px 7px; font-size:11px; cursor:pointer;">⤢</button>
        <span id="pace-dock-chevron" style="color:#9b92b3; font-size:14px;">▾</span>
      </div>
    </div>
    <div class="pace-dock-body" id="pace-dock-body">
      <div id="pace-dock-project" style="font-size:11px; color:#9b92b3;">Checking PACE connection...</div>

      <div class="pace-row">
        <button class="pace-btn pace-btn-primary" id="pace-mode-image" style="flex:1;">Image Prompts</button>
        <button class="pace-btn" id="pace-mode-video" style="flex:1;">Video Prompts</button>
      </div>

      <div class="pace-row">
        <button class="pace-btn" id="pace-load-project-btn" style="flex:1;">Load Project Scenes</button>
        <button class="pace-btn" id="pace-parse-btn">Load Text</button>
      </div>

      <textarea class="pace-textarea" id="pace-prompts-input" placeholder="Paste prompts here (one per line or blank-line separated), or click 'Load Project Scenes'..."></textarea>

      <div class="pace-row" style="justify-content:space-between; font-size:11px; color:#c4b5fd;">
        <label style="display:flex; align-items:center; gap:5px; cursor:pointer;">
          <input type="checkbox" id="pace-autosend-chk" checked /> Auto-Send to PACE
        </label>
        <label style="display:flex; align-items:center; gap:4px;">
          Delay:
          <input type="number" id="pace-delay-input" value="4" min="2" max="60" style="width:44px; padding:2px 4px; background:#09070f; color:#fff; border:1px solid #2a2538; border-radius:4px;" />s
        </label>
      </div>

      <div class="pace-row">
        <button class="pace-btn pace-btn-primary" id="pace-start-btn" style="flex:1;">▶ Start Queue</button>
        <button class="pace-btn pace-btn-warn" id="pace-pause-btn">Pause</button>
        <button class="pace-btn" id="pace-skip-btn">Skip</button>
      </div>

      <div class="pace-status-box" id="pace-status-box">Ready.</div>
    </div>
  `;
  document.body.appendChild(dock);

  const toggleBtn = document.getElementById("pace-dock-toggle")!;
  const expandBtn = document.getElementById("pace-dock-expand")!;
  const bodyEl = document.getElementById("pace-dock-body")!;
  const badgeEl = document.getElementById("pace-dock-badge")!;
  const projEl = document.getElementById("pace-dock-project")!;
  const modeImgBtn = document.getElementById("pace-mode-image")!;
  const modeVidBtn = document.getElementById("pace-mode-video")!;
  const loadProjBtn = document.getElementById("pace-load-project-btn")!;
  const parseBtn = document.getElementById("pace-parse-btn")!;
  const promptsInput = document.getElementById("pace-prompts-input") as HTMLTextAreaElement;
  const autoSendChk = document.getElementById("pace-autosend-chk") as HTMLInputElement;
  const delayInput = document.getElementById("pace-delay-input") as HTMLInputElement;
  const startBtn = document.getElementById("pace-start-btn")!;
  const pauseBtn = document.getElementById("pace-pause-btn")!;
  const skipBtn = document.getElementById("pace-skip-btn")!;
  const statusBox = document.getElementById("pace-status-box")!;

  let activeConfig: ExtensionConfig | null = null;

  const renderQueue = (state: QueueState) => {
    const doneCount = state.items.filter((i) => i.status === "done").length;
    badgeEl.textContent =
      state.items.length > 0 ? `${doneCount}/${state.items.length}` : state.status;

    modeImgBtn.classList.toggle("pace-btn-primary", state.mode === "image");
    modeVidBtn.classList.toggle("pace-btn-primary", state.mode === "video");
    autoSendChk.checked = state.autoSendToPace;
    delayInput.value = String(state.cooldownSec);
    statusBox.textContent = state.lastMessage;
  };

  const runner = new FlowQueueRunner(renderQueue);
  void runner.init();

  const refreshConnectionLabel = () => {
    if (!hasValidExtensionContext()) {
      projEl.textContent = "Extension reloaded — refresh this page (Ctrl+R).";
      return;
    }
    chrome.runtime
      .sendMessage({ type: "PACE_GET_STATE" })
      .then((res) => {
        if (res?.ok && res.config?.token) {
          activeConfig = res.config;
          projEl.textContent = `Target: ${
            res.config.selectedProjectName ||
            res.config.selectedProjectId ||
            "Select project in popup"
          }`;
        } else {
          activeConfig = null;
          projEl.textContent = "Not paired — click PACE extension icon to connect.";
        }
      })
      .catch((err) => {
        if (isContextInvalidatedError(err)) {
          projEl.textContent = "Extension reloaded — refresh this page (Ctrl+R).";
        }
      });
  };

  refreshConnectionLabel();
  chrome.storage.onChanged.addListener((changes, areaName) => {
    if (areaName === "local" && changes.pace_ext_config) {
      refreshConnectionLabel();
    }
  });

  expandBtn.addEventListener("click", (e) => {
    e.stopPropagation();
    dock.classList.toggle("expanded");
  });

  toggleBtn.addEventListener("click", () => {
    const collapsed = dock.classList.toggle("collapsed");
    bodyEl.style.display = collapsed ? "none" : "flex";
  });

  modeImgBtn.addEventListener("click", () =>
    void runner.updateState({ mode: "image" as GenerationMode, timeoutSec: 90 })
  );
  modeVidBtn.addEventListener("click", () =>
    void runner.updateState({ mode: "video" as GenerationMode, timeoutSec: 240 })
  );

  autoSendChk.addEventListener("change", () =>
    void runner.updateState({ autoSendToPace: autoSendChk.checked })
  );
  delayInput.addEventListener("change", () =>
    void runner.updateState({ cooldownSec: Math.max(2, Number(delayInput.value) || 4) })
  );

  loadProjBtn.addEventListener("click", async () => {
    const stateRes = await chrome.runtime.sendMessage({ type: "PACE_GET_STATE" });
    activeConfig = stateRes?.config || activeConfig;
    if (!activeConfig?.token || !activeConfig.selectedProjectId) {
      statusBox.textContent = "Open the PACE extension popup and select a project first.";
      return;
    }
    statusBox.textContent = "Loading project scenes from PACE...";
    const res = await chrome.runtime.sendMessage({
      type: "PACE_FETCH_PROJECT_PROMPTS",
      projectId: activeConfig.selectedProjectId,
    });
    if (!res?.ok) {
      statusBox.textContent = res?.error || "Failed loading scenes.";
      return;
    }
    const prompts = (res.data?.prompts || []) as { position: number; text: string }[];
    const mode = runner.getState().mode;
    const items = prompts
      .filter((p) => p.text?.trim())
      .map((p, i) => ({
        id: `sec-${p.position}-${Date.now()}`,
        label: `S${String(i + 1).padStart(2, "0")}`,
        prompt: p.text.trim(),
        mode,
        status: "pending" as const,
      }));
    promptsInput.value = items.map((i) => i.prompt).join("\n\n");
    await runner.setPrompts(items);
  });

  parseBtn.addEventListener("click", async () => {
    const items = parsePromptsText(promptsInput.value, runner.getState().mode);
    await runner.setPrompts(items);
  });

  startBtn.addEventListener("click", () => {
    const st = runner.getState();
    if (st.items.length === 0 && promptsInput.value.trim()) {
      const items = parsePromptsText(promptsInput.value, st.mode);
      void runner.setPrompts(items).then(() => runner.startOrResume());
    } else {
      void runner.startOrResume();
    }
  });

  pauseBtn.addEventListener("click", () => void runner.pause());
  skipBtn.addEventListener("click", () => void runner.skipCurrent());
}

function attachSendButtonsToMedia() {
  const mediaEls = findGeneratedMediaElements();
  for (const item of mediaEls) {
    const parent = item.element.parentElement;
    if (!parent || parent.querySelector(".pace-send-btn")) continue;

    if (getComputedStyle(parent).position === "static") {
      parent.style.position = "relative";
    }

    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "pace-send-btn";
    btn.textContent = "Send to PACE";
    btn.addEventListener("click", async (e) => {
      e.preventDefault();
      e.stopPropagation();
      btn.textContent = "Sending...";
      btn.setAttribute("disabled", "true");
      try {
        const promptInput = findPromptInput();
        const activePrompt =
          promptInput instanceof HTMLTextAreaElement
            ? promptInput.value
            : promptInput?.textContent || "";

        const captured = await tryCaptureMediaBytesAsBase64(
          item.src,
          item.kind
        );
        const mime =
          captured?.mime || (item.kind === "video" ? "video/mp4" : "image/png");
        const ext = mime.split("/")[1] || (item.kind === "video" ? "mp4" : "png");
        const filename = `flow_${Date.now()}.${ext}`;
        const res = await chrome.runtime.sendMessage({
          type: "PACE_UPLOAD_MEDIA",
          payload: {
            filename,
            mime,
            bytesBase64: captured?.bytesBase64,
            sourceUrl: captured ? undefined : item.src,
            expectedKind: item.kind,
            width: item.width,
            height: item.height,
            prompt: activePrompt.trim() || undefined,
            downloadLocally: item.kind === "video",
          },
        });
        if (!res?.ok) throw new Error(res?.error || "Failed");
        btn.textContent = res.registered?.duplicate ? "Already in PACE ✓" : "Sent ✓";
      } catch (err) {
        btn.textContent = "Retry Send";
        btn.removeAttribute("disabled");
        alert(err instanceof Error ? err.message : String(err));
      }
    });

    parent.appendChild(btn);
  }
}

mountFlowDock();
attachSendButtonsToMedia();

const observer = new MutationObserver(() => {
  attachSendButtonsToMedia();
});
observer.observe(document.body, { childList: true, subtree: true });
