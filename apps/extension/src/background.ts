import {
  CapturedMediaPayload,
  DEFAULT_API_URL,
  ExtensionConfig,
  ProjectSummary,
  RecentSendItem,
  RuntimeMessage,
} from "./types";

const CONFIG_KEY = "pace_ext_config";
const PROJECTS_KEY = "pace_ext_projects";
const RECENT_SENDS_KEY = "pace_ext_recent_sends";

async function getConfig(): Promise<ExtensionConfig> {
  const res = await chrome.storage.local.get(CONFIG_KEY);
  return (
    res[CONFIG_KEY] || {
      apiUrl: DEFAULT_API_URL,
      token: "",
      userEmail: "",
      selectedProjectId: "",
      selectedProjectName: "",
    }
  );
}

async function saveConfig(patch: Partial<ExtensionConfig>): Promise<ExtensionConfig> {
  const current = await getConfig();
  const next = { ...current, ...patch };
  await chrome.storage.local.set({ [CONFIG_KEY]: next });
  return next;
}

function base64ToUint8Array(b64: string): Uint8Array {
  const bin = atob(b64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) {
    bytes[i] = bin.charCodeAt(i);
  }
  return bytes;
}

async function computeSha256Hex(bytes: Uint8Array): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", bytes.buffer as ArrayBuffer);
  return Array.from(new Uint8Array(digest))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

async function putWithRetry(
  url: string,
  body: Uint8Array,
  contentType: string,
  maxAttempts = 3
): Promise<void> {
  let lastError: unknown;
  for (let attempt = 1; attempt <= maxAttempts; attempt++) {
    try {
      const resp = await fetch(url, {
        method: "PUT",
        headers: { "Content-Type": contentType },
        body: new Blob([body.buffer as ArrayBuffer], { type: contentType }),
      });
      if (!resp.ok) {
        throw new Error(`Storage upload failed (HTTP ${resp.status})`);
      }
      return;
    } catch (err) {
      lastError = err;
      if (attempt < maxAttempts) {
        await new Promise((r) => setTimeout(r, 400 * attempt));
      }
    }
  }
  throw lastError instanceof Error ? lastError : new Error("Upload failed");
}

async function fetchExtensionProjects(
  config: ExtensionConfig
): Promise<{ projects: ProjectSummary[]; last_project_id?: string | null }> {
  if (!config.token) throw new Error("Extension not paired with PACE");
  const resp = await fetch(`${config.apiUrl}/api/v1/extension/projects`, {
    headers: { Authorization: `Bearer ${config.token}` },
  });
  if (resp.status === 401) {
    throw new Error("Extension token expired or revoked. Please re-pair in PACE.");
  }
  if (!resp.ok) {
    throw new Error(`Failed loading projects (HTTP ${resp.status})`);
  }
  const data = await resp.json();
  const projects: ProjectSummary[] = data.projects || [];
  await chrome.storage.local.set({ [PROJECTS_KEY]: projects });

  if (!config.selectedProjectId && (data.last_project_id || projects.length > 0)) {
    const defaultProj =
      projects.find((p) => p.id === data.last_project_id) || projects[0];
    if (defaultProj) {
      await saveConfig({
        selectedProjectId: defaultProj.id,
        selectedProjectName: defaultProj.name,
      });
    }
  }
  return data;
}

async function handlePair(apiUrl: string, code: string) {
  const cleanUrl = apiUrl.trim().replace(/\/+$/, "") || DEFAULT_API_URL;
  const resp = await fetch(`${cleanUrl}/api/v1/extension/pair`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      code: code.trim(),
      device_name: "Chrome · Google Flow",
    }),
  });

  if (!resp.ok) {
    const errBody = await resp.json().catch(() => ({}));
    throw new Error(
      errBody?.detail || errBody?.message || `Pairing failed (HTTP ${resp.status})`
    );
  }

  const paired = await resp.json();
  const nextConfig = await saveConfig({
    apiUrl: cleanUrl,
    token: paired.token,
    userEmail: paired.user_email || "",
    selectedProjectId: "",
    selectedProjectName: "",
  });

  await fetchExtensionProjects(nextConfig);
  return { ok: true, config: await getConfig() };
}

async function handleUploadMedia(payload: CapturedMediaPayload) {
  const config = await getConfig();
  if (!config.token) {
    throw new Error("Not connected to PACE. Click the PACE extension icon to pair.");
  }
  const targetProjectId = payload.projectId || config.selectedProjectId;
  if (!targetProjectId) {
    throw new Error("No target PACE project selected.");
  }

  let rawBytes: Uint8Array;
  let resolvedMime = payload.mime;

  if (payload.bytesBase64) {
    rawBytes = base64ToUint8Array(payload.bytesBase64);
  } else if (payload.sourceUrl) {
    const mediaResp = await fetch(payload.sourceUrl, { credentials: "include" }).catch(
      () => fetch(payload.sourceUrl!)
    );
    if (!mediaResp.ok) {
      throw new Error(`Background capture failed (HTTP ${mediaResp.status})`);
    }
    const buf = await mediaResp.arrayBuffer();
    rawBytes = new Uint8Array(buf);
    const hdrMime = mediaResp.headers.get("content-type")?.split(";")[0]?.trim().toLowerCase();
    if (hdrMime && hdrMime !== "application/octet-stream") {
      resolvedMime = hdrMime;
    }
  } else {
    throw new Error("No media bytes or source URL provided.");
  }

  if (rawBytes.byteLength === 0) {
    throw new Error("Captured media file was empty.");
  }

  // Optional local download for video clips so nothing generated on Flow credits is lost
  if (payload.downloadLocally && resolvedMime.startsWith("video/")) {
    try {
      const downloadUrl =
        payload.sourceUrl && !payload.sourceUrl.startsWith("blob:")
          ? payload.sourceUrl
          : payload.bytesBase64
          ? `data:${resolvedMime};base64,${payload.bytesBase64}`
          : "";
      if (downloadUrl) {
        await chrome.downloads.download({
          url: downloadUrl,
          filename: `pace-flow/${payload.filename}`,
          conflictAction: "uniquify",
          saveAs: false,
        });
      }
    } catch (dlErr) {
      console.warn("[PACE] Local video download warning:", dlErr);
    }
  }

  const sha256 = await computeSha256Hex(rawBytes);

  // 1. Presign upload
  const presignResp = await fetch(
    `${config.apiUrl}/api/v1/extension/projects/${targetProjectId}/uploads/presign`,
    {
      method: "POST",
      headers: {
        Authorization: `Bearer ${config.token}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        filename: payload.filename,
        content_type: resolvedMime,
        bytes: rawBytes.byteLength,
      }),
    }
  );

  if (!presignResp.ok) {
    const errBody = await presignResp.json().catch(() => ({}));
    throw new Error(
      errBody?.detail || `Presign failed (HTTP ${presignResp.status})`
    );
  }

  const presign = await presignResp.json();

  // 2. Direct PUT to MinIO / S3 with 3x retry
  await putWithRetry(presign.upload_url, rawBytes, resolvedMime, 3);

  // 3. Register media with prompt + source="extension"
  const regResp = await fetch(
    `${config.apiUrl}/api/v1/extension/projects/${targetProjectId}/media`,
    {
      method: "POST",
      headers: {
        Authorization: `Bearer ${config.token}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        id: presign.media_id,
        storage_key: presign.storage_key,
        original_filename: payload.filename,
        mime: resolvedMime,
        sha256,
        width: payload.width ?? null,
        height: payload.height ?? null,
        prompt: payload.prompt ?? null,
      }),
    }
  );

  if (!regResp.ok) {
    const errBody = await regResp.json().catch(() => ({}));
    throw new Error(
      errBody?.detail || `Register media failed (HTTP ${regResp.status})`
    );
  }

  const registered = await regResp.json();

  // Save to recent sends list
  const recentRes = await chrome.storage.local.get(RECENT_SENDS_KEY);
  const existingRecent: RecentSendItem[] = recentRes[RECENT_SENDS_KEY] || [];
  const nextRecent: RecentSendItem[] = [
    {
      filename: registered.filename || payload.filename,
      projectName: config.selectedProjectName || targetProjectId,
      kind: (resolvedMime.startsWith("video/") ? "video" : "image") as
        | "image"
        | "video",
      prompt: payload.prompt,
      duplicate: Boolean(registered.duplicate),
      sentAt: new Date().toISOString(),
    },
    ...existingRecent,
  ].slice(0, 15);
  await chrome.storage.local.set({ [RECENT_SENDS_KEY]: nextRecent });

  return { ok: true, registered };
}

interface ChromeDebuggerApi {
  attach(target: { tabId: number }, requiredVersion: string): Promise<void>;
  detach(target: { tabId: number }): Promise<void>;
  sendCommand(
    target: { tabId: number },
    method: string,
    commandParams?: Record<string, unknown>
  ): Promise<unknown>;
}

function getDebuggerApi(): ChromeDebuggerApi {
  const dbg = (chrome as unknown as { debugger?: ChromeDebuggerApi }).debugger;
  if (!dbg) {
    throw new Error("chrome.debugger API is unavailable");
  }
  return dbg;
}

async function withTabDebugger<T>(
  tabId: number,
  fn: (dbg: ChromeDebuggerApi, target: { tabId: number }) => Promise<T>
): Promise<T> {
  const dbg = getDebuggerApi();
  const target = { tabId };
  await dbg.attach(target, "1.3");
  try {
    return await fn(dbg, target);
  } finally {
    await dbg.detach(target).catch(() => undefined);
  }
}

async function trustedMouseClick(tabId: number, x: number, y: number): Promise<void> {
  const rx = Math.round(x);
  const ry = Math.round(y);
  await withTabDebugger(tabId, async (dbg, target) => {
    await dbg.sendCommand(target, "Input.dispatchMouseEvent", {
      type: "mouseMoved",
      x: rx,
      y: ry,
    });
    await dbg.sendCommand(target, "Input.dispatchMouseEvent", {
      type: "mousePressed",
      x: rx,
      y: ry,
      button: "left",
      clickCount: 1,
    });
    await dbg.sendCommand(target, "Input.dispatchMouseEvent", {
      type: "mouseReleased",
      x: rx,
      y: ry,
      button: "left",
      clickCount: 1,
    });
  });
}

async function trustedTypeText(
  tabId: number,
  x: number,
  y: number,
  text: string
): Promise<void> {
  const rx = Math.round(x);
  const ry = Math.round(y);
  await withTabDebugger(tabId, async (dbg, target) => {
    await dbg.sendCommand(target, "Input.dispatchMouseEvent", {
      type: "mousePressed",
      x: rx,
      y: ry,
      button: "left",
      clickCount: 1,
    });
    await dbg.sendCommand(target, "Input.dispatchMouseEvent", {
      type: "mouseReleased",
      x: rx,
      y: ry,
      button: "left",
      clickCount: 1,
    });
    await dbg.sendCommand(target, "Input.dispatchKeyEvent", {
      type: "keyDown",
      modifiers: 2, // Ctrl
      key: "a",
      code: "KeyA",
      windowsVirtualKeyCode: 65,
    });
    await dbg.sendCommand(target, "Input.dispatchKeyEvent", {
      type: "keyUp",
      modifiers: 2,
      key: "a",
      code: "KeyA",
      windowsVirtualKeyCode: 65,
    });
    await dbg.sendCommand(target, "Input.dispatchKeyEvent", {
      type: "keyDown",
      key: "Backspace",
      code: "Backspace",
      windowsVirtualKeyCode: 8,
    });
    await dbg.sendCommand(target, "Input.dispatchKeyEvent", {
      type: "keyUp",
      key: "Backspace",
      code: "Backspace",
      windowsVirtualKeyCode: 8,
    });
    await dbg.sendCommand(target, "Input.insertText", { text });
  });
}

chrome.runtime.onMessage.addListener(
  (message: RuntimeMessage, sender, sendResponse) => {
    (async () => {
      switch (message.type) {
        case "PACE_GET_STATE": {
          const config = await getConfig();
          const stored = await chrome.storage.local.get([
            PROJECTS_KEY,
            RECENT_SENDS_KEY,
          ]);
          return {
            ok: true,
            config,
            projects: stored[PROJECTS_KEY] || [],
            recentSends: stored[RECENT_SENDS_KEY] || [],
          };
        }
        case "PACE_PAIR":
          return await handlePair(message.apiUrl, message.code);
        case "PACE_DISCONNECT":
          await chrome.storage.local.remove([CONFIG_KEY, PROJECTS_KEY]);
          return { ok: true };
        case "PACE_SELECT_PROJECT":
          await saveConfig({
            selectedProjectId: message.projectId,
            selectedProjectName: message.projectName,
          });
          return { ok: true };
        case "PACE_REFRESH_PROJECTS": {
          const config = await getConfig();
          const data = await fetchExtensionProjects(config);
          return { ok: true, projects: data.projects || [] };
        }
        case "PACE_FETCH_PROJECT_PROMPTS": {
          const config = await getConfig();
          if (!config.token) throw new Error("Extension not paired");
          const resp = await fetch(
            `${config.apiUrl}/api/v1/extension/projects/${message.projectId}/prompts`,
            { headers: { Authorization: `Bearer ${config.token}` } }
          );
          if (!resp.ok) {
            throw new Error(`Failed loading prompts (HTTP ${resp.status})`);
          }
          return { ok: true, data: await resp.json() };
        }
        case "PACE_UPLOAD_MEDIA":
          return await handleUploadMedia(message.payload);
        case "PACE_TRUSTED_CLICK": {
          const tabId = sender.tab?.id;
          if (!tabId) throw new Error("Missing sender tab ID");
          await trustedMouseClick(tabId, message.x, message.y);
          return { ok: true };
        }
        case "PACE_TRUSTED_TYPE": {
          const tabId = sender.tab?.id;
          if (!tabId) throw new Error("Missing sender tab ID");
          await trustedTypeText(tabId, message.x, message.y, message.text);
          return { ok: true };
        }
        default:
          return { ok: false, error: "Unknown message type" };
      }
    })()
      .then((res) => sendResponse(res))
      .catch((err) =>
        sendResponse({
          ok: false,
          error: err instanceof Error ? err.message : String(err),
        })
      );

    return true;
  }
);
