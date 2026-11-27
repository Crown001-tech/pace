(async () => {
  try {
    await import(chrome.runtime.getURL("dist/content/index.js"));
  } catch (err) {
    console.warn("[PACE Extension] Failed to load content module:", err);
  }
})();
