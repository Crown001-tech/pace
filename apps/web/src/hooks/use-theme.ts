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
  theme: "light",
  mounted: false,
  setTheme: (theme) => set({ theme }),
  setMounted: (mounted) => set({ mounted }),
}));

let transitionTimer: number | null = null;

function applyClassToRoot(targetTheme: Theme) {
  const root = document.documentElement;
  if (targetTheme === "dark") {
    root.classList.add("dark");
    root.style.colorScheme = "dark";
  } else {
    root.classList.remove("dark");
    root.style.colorScheme = "light";
  }
}

export interface ThemeOrigin {
  x: number;
  y: number;
}

function applyDocumentTheme(
  targetTheme: Theme,
  animate: boolean,
  origin?: ThemeOrigin
) {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  const prefersReducedMotion =
    typeof window !== "undefined" &&
    window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;

  if (animate && !prefersReducedMotion) {
    const docWithVT = document as Document & {
      startViewTransition?: (cb: () => void) => {
        ready: Promise<void>;
        finished: Promise<void>;
      };
    };
    if (typeof docWithVT.startViewTransition === "function") {
      const x = origin?.x ?? window.innerWidth - 36;
      const y = origin?.y ?? 36;
      const endRadius = Math.hypot(
        Math.max(x, window.innerWidth - x),
        Math.max(y, window.innerHeight - y)
      );

      const transition = docWithVT.startViewTransition(() => {
        applyClassToRoot(targetTheme);
      });

      transition.ready
        .then(() => {
          document.documentElement.animate(
            {
              clipPath: [
                `circle(0px at ${x}px ${y}px)`,
                `circle(${endRadius}px at ${x}px ${y}px)`,
              ],
            },
            {
              duration: 600,
              easing: "cubic-bezier(0.22, 1, 0.36, 1)",
              pseudoElement: "::view-transition-new(root)",
            }
          );
        })
        .catch(() => {});
      return;
    }

    root.classList.add("theme-transitioning");
    if (transitionTimer !== null) {
      window.clearTimeout(transitionTimer);
    }
    transitionTimer = window.setTimeout(() => {
      root.classList.remove("theme-transitioning");
      transitionTimer = null;
    }, 480);
  }

  applyClassToRoot(targetTheme);
}

/**
 * Mounts once at the app root (`QueryProvider`) to hydrate the saved theme and attach
 * a single global `'T'` keyboard shortcut listener across all viewports.
 */
export function useGlobalThemeSync() {
  const setTheme = useThemeStore((s) => s.setTheme);
  const setMounted = useThemeStore((s) => s.setMounted);

  useEffect(() => {
    let initial: Theme = "light";
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
    (targetTheme: Theme, origin?: ThemeOrigin) => {
      setTheme(targetTheme);
      applyDocumentTheme(targetTheme, true, origin);
      try {
        localStorage.setItem("pace_theme", targetTheme);
      } catch {}
    },
    [setTheme]
  );

  const toggleTheme = useCallback(
    (origin?: ThemeOrigin) => {
      const current = useThemeStore.getState().theme;
      setThemeMode(current === "dark" ? "light" : "dark", origin);
    },
    [setThemeMode]
  );

  return { theme, setThemeMode, toggleTheme, mounted };
}
