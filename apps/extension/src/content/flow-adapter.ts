import { GenerationMode } from "../types";

export interface CapturedFlowMedia {
  kind: "image" | "video";
  src: string;
  width?: number;
  height?: number;
  element: HTMLElement;
}

const CREDIT_LIMIT_PATTERNS = [
  /out of credits/i,
  /insufficient credits/i,
  /no credits remaining/i,
  /credit limit/i,
  /daily limit reached/i,
  /quota exceeded/i,
  /too many requests/i,
  /rate limit/i,
  /try again later/i,
];

const GENERATION_ERROR_PATTERNS = [
  /generation failed/i,
  /unable to generate/i,
  /violates.*policy/i,
  /something went wrong/i,
];

/**
 * Finds Google Flow's prompt input box using resilient, class-agnostic selectors.
 */
export function findPromptInput(doc: Document = document): HTMLTextAreaElement | HTMLElement | null {
  const candidates = [
    'textarea[placeholder*="create" i]',
    'textarea[placeholder*="prompt" i]',
    'textarea[aria-label*="prompt" i]',
    'textarea[aria-label*="create" i]',
    'textarea[placeholder*="describe" i]',
    'textarea[placeholder*="imagine" i]',
    '[contenteditable="true"][data-placeholder*="create" i]',
    '[contenteditable="true"][aria-placeholder*="create" i]',
    '[contenteditable="true"][role="textbox"]',
    '[contenteditable="true"][aria-label*="prompt" i]',
    '[contenteditable="true"]',
    "textarea",
  ];

  for (const selector of candidates) {
    const els = Array.from(doc.querySelectorAll<HTMLElement>(selector));
    const visible = els.find((el) => {
      if (el.closest("#pace-flow-dock")) return false;
      const rect = el.getBoundingClientRect();
      return rect.width > 80 && rect.height > 16;
    });
    if (visible) return visible;
  }
  return null;
}

/**
 * Returns the nearest enclosing prompt-bar container around the Flow prompt input
 * so button searches never accidentally hit sidebar/header "Create collection" buttons.
 */
function findPromptBarContainer(
  input: HTMLElement,
  doc: Document = document
): HTMLElement | null {
  const form = input.closest("form");
  if (form) return form;

  let cur: HTMLElement | null = input.parentElement;
  let singleBtnFallback: HTMLElement | null = null;
  for (let depth = 0; depth < 7 && cur && cur !== doc.body; depth++) {
    const buttons = Array.from(
      cur.querySelectorAll<HTMLElement>("button, [role='button']")
    ).filter((b) => !b.closest("#pace-flow-dock"));
    const rect = cur.getBoundingClientRect();
    if (rect.height > 0 && rect.height < 320) {
      if (buttons.length >= 2) {
        return cur;
      }
      if (buttons.length === 1 && !singleBtnFallback) {
        singleBtnFallback = cur;
      }
    }
    cur = cur.parentElement;
  }
  return singleBtnFallback || input.parentElement;
}

function isElementDisabled(el: HTMLElement): boolean {
  if ("disabled" in el && (el as HTMLButtonElement).disabled) return true;
  if (el.getAttribute("aria-disabled") === "true") return true;
  if (el.getAttribute("data-disabled") === "true") return true;
  return false;
}

function getButtonDescriptor(btn: HTMLElement): string {
  return [
    btn.getAttribute("aria-label") || "",
    btn.getAttribute("title") || "",
    btn.getAttribute("data-testid") || "",
    btn.textContent || "",
  ]
    .join(" ")
    .replace(/\s+/g, " ")
    .trim()
    .toLowerCase();
}

/**
 * Finds Google Flow's Generate / Submit (→) button strictly inside the prompt bar.
 */
export function findGenerateButton(doc: Document = document): HTMLButtonElement | null {
  const input = findPromptInput(doc);
  if (!input) return null;

  const container = findPromptBarContainer(input, doc);
  if (!container) return null;

  const barButtons = Array.from(
    container.querySelectorAll<HTMLButtonElement>("button, [role='button']")
  ).filter((b) => {
    if (b.closest("#pace-flow-dock") || isElementDisabled(b)) return false;
    const rect = b.getBoundingClientRect();
    return rect.width > 0 && rect.height > 0;
  });

  if (barButtons.length === 0) return null;

  const isNonSubmitControl = (desc: string) =>
    desc === "add" ||
    desc === "+" ||
    desc === "close" ||
    desc === "clear" ||
    desc === "×" ||
    desc.includes("clear prompt") ||
    desc.includes("volume") ||
    desc.includes("mute") ||
    desc.includes("audio") ||
    desc.includes("sound") ||
    desc.includes("video ·") ||
    desc.includes("image ·") ||
    desc.includes("720p") ||
    desc.includes("1080p") ||
    desc.includes("aspect");

  // 1. Prefer explicit submit / generate / send / arrow button inside the prompt bar
  for (const btn of barButtons) {
    const type = (btn.getAttribute("type") || "").toLowerCase();
    const desc = getButtonDescriptor(btn);
    if (isNonSubmitControl(desc)) continue;
    if (
      type === "submit" ||
      desc.includes("generate") ||
      desc.includes("submit") ||
      desc.includes("send") ||
      desc === "arrow_forward" ||
      desc === "→" ||
      desc.includes("arrow_forward")
    ) {
      return btn;
    }
  }

  // 2. Otherwise pick the rightmost non-clear button in the bottom row of the prompt bar
  const filtered = barButtons.filter((b) => !isNonSubmitControl(getButtonDescriptor(b)));
  const pool = filtered.length > 0 ? filtered : barButtons;
  return pool.reduce((rightmost, btn) => {
    const rA = btn.getBoundingClientRect();
    const rB = rightmost.getBoundingClientRect();
    if (Math.abs(rA.bottom - rB.bottom) > 16) {
      return rA.bottom > rB.bottom ? btn : rightmost;
    }
    return rA.right > rB.right ? btn : rightmost;
  });
}

function getElementCenter(el: HTMLElement): { x: number; y: number } {
  const rect = el.getBoundingClientRect();
  return {
    x: rect.left + rect.width / 2,
    y: rect.top + rect.height / 2,
  };
}

function clickLikeHuman(el: HTMLElement): void {
  const { x, y } = getElementCenter(el);
  const opts: MouseEventInit = {
    bubbles: true,
    cancelable: true,
    view: window,
    clientX: x,
    clientY: y,
    button: 0,
    buttons: 1,
  };
  try {
    el.dispatchEvent(new PointerEvent("pointerdown", opts));
  } catch {
    // PointerEvent may not exist in test JSDOM
  }
  el.dispatchEvent(new MouseEvent("mousedown", opts));
  try {
    el.dispatchEvent(new PointerEvent("pointerup", { ...opts, buttons: 0 }));
  } catch {
    // ignore
  }
  el.dispatchEvent(new MouseEvent("mouseup", { ...opts, buttons: 0 }));
  el.click();
}

function getPromptText(el: HTMLElement): string {
  if (el instanceof HTMLTextAreaElement || el instanceof HTMLInputElement) {
    return el.value.trim();
  }
  return (el.textContent || "").trim();
}

export function describeFlowPromptBar(doc: Document = document): string {
  const input = findPromptInput(doc);
  if (!input) return "No prompt input found";
  const container = findPromptBarContainer(input, doc);
  const btns = container
    ? Array.from(container.querySelectorAll<HTMLElement>("button, [role='button']"))
        .filter((b) => !b.closest("#pace-flow-dock"))
        .map((b, i) => {
          const d = getButtonDescriptor(b) || b.tagName.toLowerCase();
          const dis = isElementDisabled(b) ? "(disabled)" : "";
          return `[${i}:${d}${dis}]`;
        })
    : [];
  const chosen = findGenerateButton(doc);
  const chosenDesc = chosen ? getButtonDescriptor(chosen) || chosen.tagName : "NONE";
  return `input=${input.tagName}(${input.getAttribute("contenteditable") || "native"}) | chosen=${chosenDesc} | btns=${btns.join(" ")}`;
}

async function waitForSubmitClear(
  inputEl: HTMLElement,
  genBtn: HTMLElement,
  beforeLen: number,
  checks = 4
): Promise<boolean> {
  for (let i = 0; i < checks; i++) {
    await new Promise((r) => setTimeout(r, 200));
    if (getPromptText(inputEl).length < beforeLen || isElementDisabled(genBtn)) {
      return true;
    }
  }
  return false;
}

/**
 * Submits the active prompt in Google Flow:
 * 1. Tries synthetic pointer/mouse click with coordinates
 * 2. Falls back to real isTrusted=true mouse click via chrome.debugger (PACE_TRUSTED_CLICK)
 * 3. If Slate's model still didn't register the text, re-types via Input.insertText and clicks again
 */
export async function submitFlowPrompt(
  inputEl: HTMLElement,
  genBtn: HTMLElement
): Promise<{ submitted: boolean; diagnostic: string }> {
  const promptText = getPromptText(inputEl);
  const beforeLen = promptText.length;

  clickLikeHuman(genBtn);
  if (await waitForSubmitClear(inputEl, genBtn, beforeLen, 3)) {
    return { submitted: true, diagnostic: describeFlowPromptBar(inputEl.ownerDocument) };
  }

  // Fallback 1: Real browser-trusted click via chrome.debugger (bypasses isTrusted checks)
  if (typeof chrome !== "undefined" && chrome.runtime?.sendMessage) {
    const btnPos = getElementCenter(genBtn);
    await chrome.runtime
      .sendMessage({ type: "PACE_TRUSTED_CLICK", x: btnPos.x, y: btnPos.y })
      .catch(() => undefined);

    if (await waitForSubmitClear(inputEl, genBtn, beforeLen, 4)) {
      return { submitted: true, diagnostic: describeFlowPromptBar(inputEl.ownerDocument) };
    }

    // Fallback 2: Real browser-trusted text insertion into Slate + trusted click
    if (promptText) {
      const inputPos = getElementCenter(inputEl);
      await chrome.runtime
        .sendMessage({
          type: "PACE_TRUSTED_TYPE",
          x: inputPos.x,
          y: inputPos.y,
          text: promptText,
        })
        .catch(() => undefined);
      await new Promise((r) => setTimeout(r, 200));
      const freshBtn = findGenerateButton(inputEl.ownerDocument) || genBtn;
      const freshPos = getElementCenter(freshBtn);
      await chrome.runtime
        .sendMessage({ type: "PACE_TRUSTED_CLICK", x: freshPos.x, y: freshPos.y })
        .catch(() => undefined);

      if (await waitForSubmitClear(inputEl, freshBtn, beforeLen, 5)) {
        return { submitted: true, diagnostic: describeFlowPromptBar(inputEl.ownerDocument) };
      }
    }
  }

  return {
    submitted: false,
    diagnostic: describeFlowPromptBar(inputEl.ownerDocument),
  };
}

/**
 * Checks/switches Google Flow's prompt-bar mode pill (Image vs Video), opening the settings menu if needed.
 */
export async function selectGenerationMode(
  mode: GenerationMode,
  doc: Document = document
): Promise<boolean> {
  const input = findPromptInput(doc);
  if (!input) return false;
  const container = findPromptBarContainer(input, doc);
  if (!container) return false;

  const targetWord = mode === "video" ? "video" : "image";
  const barButtons = Array.from(
    container.querySelectorAll<HTMLElement>('button, [role="tab"], [role="radio"], [role="combobox"]')
  ).filter((el) => !el.closest("#pace-flow-dock"));

  const isVideoModel = (label: string) =>
    label.includes("video") || label.includes("veo") || label.includes("720p") || label.includes("1080p");
  const isImageModel = (label: string) =>
    label.includes("image") || label.includes("nano banana") || label.includes("imagen");

  const alreadyActive = barButtons.some((el) => {
    const label = getButtonDescriptor(el);
    return mode === "video" ? isVideoModel(label) : isImageModel(label);
  });
  if (alreadyActive) return true;

  // Open the settings trigger pill (e.g. "settings trigger 🍌 nano banana 2.1 crop_16_9 x1")
  const modePill = barButtons.find((el) => {
    const label = getButtonDescriptor(el);
    return (
      label.includes("settings trigger") ||
      isVideoModel(label) ||
      isImageModel(label)
    );
  });
  if (modePill) {
    clickLikeHuman(modePill);
    await new Promise((r) => setTimeout(r, 250));
    const menuItems = Array.from(
      doc.querySelectorAll<HTMLElement>(
        '[role="menuitem"], [role="option"], [role="radio"], [role="tab"], [role="menu"] button, [role="listbox"] button, [role="dialog"] button'
      )
    ).filter((el) => !el.closest("#pace-flow-dock"));
    const match = menuItems.find((el) => {
      const text = getButtonDescriptor(el);
      return (
        text === targetWord ||
        text === `${targetWord}s` ||
        text.startsWith(`${targetWord} `) ||
        text.includes(`create ${targetWord}`) ||
        text.includes(`text to ${targetWord}`)
      );
    });
    if (match) {
      clickLikeHuman(match);
      await new Promise((r) => setTimeout(r, 150));
      return true;
    }
    // Close popover if we didn't find a direct mode tab inside it
    doc.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  }
  return false;
}

/**
 * Sets prompt text in a React/Slate/ProseMirror-safe way so controlled inputs trigger state updates.
 */
export function setPromptInputValue(
  el: HTMLTextAreaElement | HTMLElement,
  value: string
): void {
  el.focus();
  if (el instanceof HTMLTextAreaElement || el instanceof HTMLInputElement) {
    const proto = Object.getPrototypeOf(el);
    const descriptor = Object.getOwnPropertyDescriptor(proto, "value");
    if (descriptor?.set) {
      descriptor.set.call(el, value);
    } else {
      el.value = value;
    }
    el.dispatchEvent(new Event("input", { bubbles: true }));
    el.dispatchEvent(new Event("change", { bubbles: true }));
  } else {
    try {
      clickLikeHuman(el);
      el.focus();
      const sel = window.getSelection();
      if (sel) {
        const range = document.createRange();
        range.selectNodeContents(el);
        sel.removeAllRanges();
        sel.addRange(range);
      }
      if (typeof document.execCommand === "function") {
        document.execCommand("selectAll", false);
        document.execCommand("delete", false);
      }

      // 1. Prefer synthetic Paste event (Slate.js / Lexical / ProseMirror handle onPaste natively)
      let pasted = false;
      if (typeof DataTransfer !== "undefined" && typeof ClipboardEvent !== "undefined") {
        const dt = new DataTransfer();
        dt.setData("text/plain", value);
        const pasteEvt = new ClipboardEvent("paste", {
          bubbles: true,
          cancelable: true,
          clipboardData: dt,
        });
        el.dispatchEvent(pasteEvt);
        pasted = getPromptText(el) === value.trim();
      }

      // 2. Fallback to execCommand("insertText")
      if (!pasted && typeof document.execCommand === "function") {
        document.execCommand("insertText", false, value);
      }

      // 3. Last resort only if both paste & execCommand left the editor empty
      if (getPromptText(el).length === 0) {
        el.textContent = value;
        el.dispatchEvent(
          new InputEvent("input", {
            bubbles: true,
            inputType: "insertText",
            data: value,
          })
        );
      }
    } catch {
      el.textContent = value;
      el.dispatchEvent(new InputEvent("input", { bubbles: true, data: value }));
    }
  }
}

/**
 * Scans Flow page alerts/banners for out-of-credits or rate-limit notices.
 */
export function detectCreditOrRateLimitError(doc: Document = document): string | null {
  const alertEls = Array.from(
    doc.querySelectorAll<HTMLElement>(
      '[role="alert"], [role="status"], [aria-live], .toast, [class*="error" i], [class*="credit" i]'
    )
  );
  for (const el of alertEls) {
    if (el.closest("#pace-flow-dock")) continue;
    const text = (el.textContent || "").trim();
    if (!text || text.length > 260) continue;
    for (const pat of CREDIT_LIMIT_PATTERNS) {
      if (pat.test(text)) {
        return text;
      }
    }
  }
  return null;
}

/**
 * Scans Flow page alerts for per-prompt policy or transient generation errors.
 */
export function detectGenerationError(doc: Document = document): string | null {
  const alertEls = Array.from(
    doc.querySelectorAll<HTMLElement>('[role="alert"], [aria-live="assertive"]')
  );
  for (const el of alertEls) {
    if (el.closest("#pace-flow-dock")) continue;
    const text = (el.textContent || "").trim();
    if (!text || text.length > 260) continue;
    for (const pat of GENERATION_ERROR_PATTERNS) {
      if (pat.test(text)) {
        return text;
      }
    }
  }
  return null;
}

/**
 * Discovers generated image and video elements in Google Flow, ignoring tiny UI icons/avatars.
 */
export function findGeneratedMediaElements(
  doc: Document = document
): CapturedFlowMedia[] {
  const results: CapturedFlowMedia[] = [];
  const seenSrc = new Set<string>();

  const videos = Array.from(doc.querySelectorAll<HTMLVideoElement>("video"));
  for (const vid of videos) {
    if (vid.closest("#pace-flow-dock")) continue;
    const src = vid.currentSrc || vid.src || vid.querySelector("source")?.src || "";
    if (!src || seenSrc.has(src)) continue;
    const rect = vid.getBoundingClientRect();
    if (rect.width < 120 || rect.height < 90) continue;
    seenSrc.add(src);
    results.push({
      kind: "video",
      src,
      width: vid.videoWidth || Math.round(rect.width),
      height: vid.videoHeight || Math.round(rect.height),
      element: vid,
    });
  }

  const images = Array.from(doc.querySelectorAll<HTMLImageElement>("img"));
  for (const img of images) {
    if (img.closest("#pace-flow-dock")) continue;
    const src = img.currentSrc || img.src || "";
    if (!src || seenSrc.has(src)) continue;
    if (src.startsWith("data:image/svg")) continue;
    const rect = img.getBoundingClientRect();
    const w = img.naturalWidth || rect.width;
    const h = img.naturalHeight || rect.height;
    if (w < 180 || h < 120) continue;
    seenSrc.add(src);
    results.push({
      kind: "image",
      src,
      width: img.naturalWidth || Math.round(rect.width),
      height: img.naturalHeight || Math.round(rect.height),
      element: img,
    });
  }

  return results;
}

/**
 * Attempts to capture media bytes inside the content script (works for blob: URLs).
 * Returns null if blocked by cross-origin CORS so background.ts can fetch directly.
 */
export async function tryCaptureMediaBytesAsBase64(
  src: string,
  expectedKind: "image" | "video"
): Promise<{ bytesBase64: string; mime: string } | null> {
  try {
    let resp: Response;
    try {
      resp = await fetch(src, { credentials: "include" });
      if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
    } catch {
      resp = await fetch(src);
    }
    if (!resp.ok) return null;

    const blob = await resp.blob();
    const buf = await blob.arrayBuffer();
    const bytes = new Uint8Array(buf);
    let binary = "";
    const chunkSize = 0x8000;
    for (let i = 0; i < bytes.length; i += chunkSize) {
      binary += String.fromCharCode(...bytes.subarray(i, i + chunkSize));
    }

    let mime = blob.type?.split(";")[0]?.trim().toLowerCase() || "";
    if (!mime || mime === "application/octet-stream") {
      mime = expectedKind === "video" ? "video/mp4" : "image/png";
    }
    return { bytesBase64: btoa(binary), mime };
  } catch {
    return null;
  }
}
