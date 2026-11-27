import {
  DEFAULT_QUEUE_STATE,
  GenerationMode,
  PromptQueueItem,
  QueueState,
} from "../types";
import {
  tryCaptureMediaBytesAsBase64,
  detectCreditOrRateLimitError,
  detectGenerationError,
  findGeneratedMediaElements,
  findGenerateButton,
  findPromptInput,
  selectGenerationMode,
  setPromptInputValue,
  submitFlowPrompt,
} from "./flow-adapter";

const QUEUE_STORAGE_KEY = "pace_flow_queue_state";

export function isContextInvalidatedError(err: unknown): boolean {
  const msg = err instanceof Error ? err.message : String(err || "");
  return msg.toLowerCase().includes("extension context invalidated");
}

export function hasValidExtensionContext(): boolean {
  if (typeof chrome === "undefined") return false;
  try {
    return Boolean((chrome.runtime as { id?: string } | undefined)?.id);
  } catch {
    return false;
  }
}

export async function loadQueueState(): Promise<QueueState> {
  if (!hasValidExtensionContext() || !chrome.storage?.local) {
    return { ...DEFAULT_QUEUE_STATE };
  }
  try {
    const res = await chrome.storage.local.get(QUEUE_STORAGE_KEY);
    return { ...DEFAULT_QUEUE_STATE, ...(res[QUEUE_STORAGE_KEY] || {}) };
  } catch (err) {
    if (isContextInvalidatedError(err)) {
      return { ...DEFAULT_QUEUE_STATE };
    }
    throw err;
  }
}

export async function saveQueueState(state: QueueState): Promise<void> {
  if (!hasValidExtensionContext() || !chrome.storage?.local) return;
  try {
    await chrome.storage.local.set({ [QUEUE_STORAGE_KEY]: state });
  } catch (err) {
    if (!isContextInvalidatedError(err)) {
      throw err;
    }
  }
}

export function parsePromptsText(
  raw: string,
  mode: GenerationMode
): PromptQueueItem[] {
  const blocks = raw.includes("\n\n")
    ? raw.split(/\n\s*\n/)
    : raw.split(/\r?\n/);

  return blocks
    .map((line) => line.trim())
    .filter(Boolean)
    .map((prompt, idx) => ({
      id: `q-${Date.now()}-${idx}`,
      label: `S${String(idx + 1).padStart(2, "0")}`,
      prompt,
      mode,
      status: "pending" as const,
    }));
}

function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}

export class FlowQueueRunner {
  private state: QueueState = { ...DEFAULT_QUEUE_STATE };
  private abortRequested = false;
  private onStateChange: (state: QueueState) => void;

  constructor(onStateChange: (state: QueueState) => void) {
    this.onStateChange = onStateChange;
  }

  async init(): Promise<QueueState> {
    this.state = await loadQueueState();
    if (this.state.status === "running") {
      // Recover cleanly after tab reload: pause so user clicks Resume rather than double-submitting
      this.state = {
        ...this.state,
        status: "paused_user",
        lastMessage: "Tab reloaded — click Resume to continue queue.",
      };
      await saveQueueState(this.state);
    }
    this.onStateChange(this.state);
    return this.state;
  }

  getState(): QueueState {
    return this.state;
  }

  async updateState(patch: Partial<QueueState>): Promise<void> {
    this.state = { ...this.state, ...patch };
    await saveQueueState(this.state);
    this.onStateChange(this.state);
  }

  async setPrompts(items: PromptQueueItem[]): Promise<void> {
    await this.updateState({
      items,
      currentIndex: 0,
      status: "idle",
      lastMessage: `Loaded ${items.length} prompt(s) into queue.`,
    });
  }

  async pause(): Promise<void> {
    this.abortRequested = true;
    await this.updateState({
      status: "paused_user",
      lastMessage: `Paused at item ${this.state.currentIndex + 1} of ${this.state.items.length}.`,
    });
  }

  async skipCurrent(): Promise<void> {
    const items = [...this.state.items];
    if (items[this.state.currentIndex]) {
      items[this.state.currentIndex] = {
        ...items[this.state.currentIndex],
        status: "skipped",
      };
    }
    const nextIdx = Math.min(items.length, this.state.currentIndex + 1);
    await this.updateState({
      items,
      currentIndex: nextIdx,
      lastMessage: `Skipped to item ${nextIdx + 1}.`,
    });
  }

  async startOrResume(): Promise<void> {
    if (this.state.items.length === 0) {
      await this.updateState({
        lastMessage: "Add or load prompts before starting the queue.",
      });
      return;
    }

    this.abortRequested = false;
    await this.updateState({
      status: "running",
      lastMessage: "Starting Google Flow queue...",
    });

    while (!this.abortRequested && this.state.currentIndex < this.state.items.length) {
      if (!hasValidExtensionContext()) {
        this.abortRequested = true;
        this.state = {
          ...this.state,
          status: "paused_user",
          lastMessage: "Extension reloaded — refresh this page (Ctrl+R) to continue.",
        };
        this.onStateChange(this.state);
        return;
      }
      const idx = this.state.currentIndex;
      const item = this.state.items[idx];
      if (!item || item.status === "done" || item.status === "skipped") {
        await this.updateState({ currentIndex: idx + 1 });
        continue;
      }

      // 1. Check credit/rate-limit banner before running
      const preCreditErr = detectCreditOrRateLimitError();
      if (preCreditErr) {
        await this.updateState({
          status: "paused_credits",
          lastMessage: `Paused (Credits/Limit): ${preCreditErr}`,
        });
        return;
      }

      // 2. Locate Flow prompt input
      const inputEl = findPromptInput();
      if (!inputEl) {
        await this.updateState({
          status: "paused_user",
          lastMessage:
            "Could not find Google Flow prompt box. Open a Flow project canvas and click Resume.",
        });
        return;
      }

      // Mark item running
      const runningItems = [...this.state.items];
      runningItems[idx] = { ...item, status: "running", error: undefined };
      await this.updateState({
        items: runningItems,
        lastMessage: `Submitting ${item.label} (${idx + 1}/${this.state.items.length})...`,
      });

      // Snapshot existing media URLs so we only capture the newly generated asset
      const beforeSrcs = new Set(findGeneratedMediaElements().map((m) => m.src));

      await selectGenerationMode(item.mode);
      await sleep(250);
      setPromptInputValue(inputEl, item.prompt);
      await sleep(400);

      const genBtn = findGenerateButton();
      if (!genBtn) {
        await this.updateState({
          status: "paused_user",
          lastMessage:
            "Could not find Generate button (or credits may be depleted). Click Resume when ready.",
        });
        return;
      }

      const submitResult = await submitFlowPrompt(inputEl, genBtn);
      if (!submitResult.submitted) {
        await this.updateState({
          status: "paused_user",
          lastMessage: `Flow did not clear prompt on submit (${submitResult.diagnostic}). Click Resume or inspect.`,
        });
        return;
      }

      // 3. Poll for new generated media with mode-aware timeout
      const timeoutMs =
        (item.mode === "video"
          ? Math.max(120, this.state.timeoutSec)
          : this.state.timeoutSec) * 1000;
      const startedAt = Date.now();
      let capturedSrc: string | null = null;
      let capturedKind: "image" | "video" = item.mode;
      let capturedWidth: number | undefined;
      let capturedHeight: number | undefined;

      while (!this.abortRequested && Date.now() - startedAt < timeoutMs) {
        await sleep(1500);

        const creditErr = detectCreditOrRateLimitError();
        if (creditErr) {
          const pausedItems = [...this.state.items];
          pausedItems[idx] = { ...item, status: "pending", error: creditErr };
          await this.updateState({
            items: pausedItems,
            status: "paused_credits",
            lastMessage: `Paused (Credits/Limit): ${creditErr}`,
          });
          return;
        }

        const genErr = detectGenerationError();
        if (genErr) {
          const errItems = [...this.state.items];
          errItems[idx] = { ...item, status: "error", error: genErr };
          await this.updateState({
            items: errItems,
            currentIndex: idx + 1,
            lastMessage: `${item.label} failed: ${genErr}`,
          });
          break;
        }

        const nowMedia = findGeneratedMediaElements();
        const fresh = nowMedia.find(
          (m) => !beforeSrcs.has(m.src) && (item.mode === "video" ? m.kind === "video" : true)
        );
        if (fresh) {
          capturedSrc = fresh.src;
          capturedKind = fresh.kind;
          capturedWidth = fresh.width;
          capturedHeight = fresh.height;
          break;
        }

        const elapsed = Math.round((Date.now() - startedAt) / 1000);
        await this.updateState({
          lastMessage: `Waiting for ${item.label} (${item.mode})... ${elapsed}s`,
        });
      }

      if (this.abortRequested) return;

      if (!capturedSrc) {
        if (this.state.items[idx]?.status === "error") {
          await sleep(this.state.cooldownSec * 1000);
          continue;
        }
        const timedOutItems = [...this.state.items];
        timedOutItems[idx] = {
          ...item,
          status: "error",
          error: "Timed out waiting for Flow output",
        };
        await this.updateState({
          items: timedOutItems,
          status: "paused_user",
          lastMessage: `${item.label} timed out. Check Flow tab and click Retry/Resume or Skip.`,
        });
        return;
      }

      // 4. Auto-Send to PACE (and auto-download videos if enabled)
      if (this.state.autoSendToPace) {
        try {
          await this.updateState({
            lastMessage: `Uploading ${item.label} to PACE...`,
          });
          const captured = await tryCaptureMediaBytesAsBase64(
            capturedSrc,
            capturedKind
          );
          const mime =
            captured?.mime || (capturedKind === "video" ? "video/mp4" : "image/png");
          const ext = mime.split("/")[1] || (capturedKind === "video" ? "mp4" : "png");
          const filename = `flow_${String(idx + 1).padStart(3, "0")}_${item.label.toLowerCase()}.${ext}`;

          const uploadRes = await chrome.runtime.sendMessage({
            type: "PACE_UPLOAD_MEDIA",
            payload: {
              filename,
              mime,
              bytesBase64: captured?.bytesBase64,
              sourceUrl: captured ? undefined : capturedSrc,
              expectedKind: capturedKind,
              width: capturedWidth,
              height: capturedHeight,
              prompt: item.prompt,
              downloadLocally: this.state.autoDownloadVideos,
            },
          });

          if (!uploadRes?.ok) {
            throw new Error(uploadRes?.error || "Upload failed");
          }
        } catch (err) {
          const uploadErrItems = [...this.state.items];
          uploadErrItems[idx] = {
            ...item,
            status: "error",
            error: err instanceof Error ? err.message : String(err),
          };
          await this.updateState({
            items: uploadErrItems,
            status: "paused_user",
            lastMessage: `Upload error on ${item.label}: ${
              err instanceof Error ? err.message : String(err)
            }`,
          });
          return;
        }
      }

      const doneItems = [...this.state.items];
      doneItems[idx] = { ...item, status: "done" };
      await this.updateState({
        items: doneItems,
        currentIndex: idx + 1,
        lastMessage: `Completed ${item.label} (${idx + 1}/${this.state.items.length})`,
      });

      if (idx + 1 < this.state.items.length) {
        await sleep(this.state.cooldownSec * 1000);
      }
    }

    if (!this.abortRequested && this.state.currentIndex >= this.state.items.length) {
      await this.updateState({
        status: "completed",
        lastMessage: `All ${this.state.items.length} prompt(s) finished!`,
      });
    }
  }
}
