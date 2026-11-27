"use client";

import { useEffect, useCallback } from "react";
import { create } from "zustand";

export type Theme = "light" | "dark";

interface ThemeStoreState {
  theme: Theme;
  mounted: boolean;
  setTheme: (theme: Theme) => void;
  setMounted: (mounted: boolean) => void;
}

const useThemeStore = create<ThemeStoreState>((set) => ({
  theme: "dark",
  mounted: false,
  setTheme: (theme) => set({ theme }),
  setMounted: (mounted) => set({ mounted }),
}));

let transitionTimer: number | null = null;

function applyDocumentTheme(targetTheme: Theme, animate: boolean) {
  if (typeof document === "undefined") return;
  const root = document.documentElement;

  if (animate) {
    root.classList.add("theme-transitioning");
    if (transitionTimer !== null) {
      window.clearTimeout(transitionTimer);
    }
    transitionTimer = window.setTimeout(() => {
      root.classList.remove("theme-transitioning");
      transitionTimer = null;
    }, 260);
  }

  if (targetTheme === "dark") {
    root.classList.add("dark");
    root.style.colorScheme = "dark";
  } else {
    root.classList.remove("dark");
    root.style.colorScheme = "light";
  }
}

/**
 * Mounts once at the app root (`QueryProvider`) to hydrate the saved theme and attach
 * a single global `'T'` keyboard shortcut listener across all viewports.
 */
export function useGlobalThemeSync() {
  const setTheme = useThemeStore((s) => s.setTheme);
  const setMounted = useThemeStore((s) => s.setMounted);

  useEffect(() => {
    let initial: Theme = "dark";
    try {
      const saved = localStorage.getItem("pace_theme");
      if (saved === "light" || saved === "dark") {
        initial = saved;
      }
    } catch {}

    setTheme(initial);
    setMounted(true);
    applyDocumentTheme(initial, false);

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.ctrlKey || e.metaKey || e.altKey) return;
      const active = document.activeElement;
      const tag = active?.tagName.toLowerCase();
      const isInput =
        tag === "input" ||
        tag === "textarea" ||
        tag === "select" ||
        active?.getAttribute("contenteditable") === "true";

      if (!isInput && (e.key === "t" || e.key === "T")) {
        e.preventDefault();
        const current = useThemeStore.getState().theme;
        const next: Theme = current === "dark" ? "light" : "dark";
        useThemeStore.getState().setTheme(next);
        applyDocumentTheme(next, true);
        try {
          localStorage.setItem("pace_theme", next);
        } catch {}
      }
    };

    const handleStorage = (e: StorageEvent) => {
      if (e.key === "pace_theme" && (e.newValue === "light" || e.newValue === "dark")) {
        setTheme(e.newValue);
        applyDocumentTheme(e.newValue, false);
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    window.addEventListener("storage", handleStorage);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("storage", handleStorage);
    };
  }, [setTheme, setMounted]);
}

export function useTheme() {
  const theme = useThemeStore((s) => s.theme);
  const mounted = useThemeStore((s) => s.mounted);
  const setTheme = useThemeStore((s) => s.setTheme);

  const setThemeMode = useCallback(
    (targetTheme: Theme) => {
      setTheme(targetTheme);
      applyDocumentTheme(targetTheme, true);
      try {
        localStorage.setItem("pace_theme", targetTheme);
      } catch {}
    },
    [setTheme]
  );

  const toggleTheme = useCallback(() => {
    const current = useThemeStore.getState().theme;
    setThemeMode(current === "dark" ? "light" : "dark");
  }, [setThemeMode]);

  return { theme, setThemeMode, toggleTheme, mounted };
}
