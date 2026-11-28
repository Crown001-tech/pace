"use client";

import React from "react";
import { useTheme } from "@/hooks/use-theme";
import { Sun, Moon } from "lucide-react";
import { cn } from "@/lib/utils";

interface ThemeToggleProps {
  variant?: "segmented" | "compact";
  className?: string;
}

export function ThemeToggle({
  variant = "segmented",
  className,
}: ThemeToggleProps) {
  const { theme, setThemeMode, toggleTheme, mounted } = useTheme();

  if (!mounted) {
    return (
      <div
        className={cn(
          variant === "compact"
            ? "h-8 w-8 rounded-[5px] bg-secondary"
            : "h-8 w-28 rounded-[5px] bg-secondary",
          className
        )}
        aria-hidden="true"
      />
    );
  }

  const isDark = theme === "dark";

  const getOrigin = (e: React.MouseEvent<HTMLButtonElement>) => {
    if (e.clientX !== 0 || e.clientY !== 0) {
      return { x: e.clientX, y: e.clientY };
    }
    const rect = e.currentTarget.getBoundingClientRect();
    return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
  };

  if (variant === "compact") {
    return (
      <button
        type="button"
        onClick={(e) => toggleTheme(getOrigin(e))}
        className={cn(
          "relative flex h-9 w-9 items-center justify-center rounded-xl text-foreground/80 hover:text-foreground focus:outline-none focus-visible:ring-2 focus-visible:ring-ring active:scale-95 transition-all",
          className
        )}
        title={isDark ? "Switch to light theme (T)" : "Switch to dark theme (T)"}
        aria-label={isDark ? "Switch to light theme" : "Switch to dark theme"}
      >
        <Sun
          className={cn(
            "h-3.5 w-3.5 transition-transform duration-200",
            isDark ? "rotate-90 scale-0 opacity-0 absolute" : "rotate-0 scale-100 opacity-100"
          )}
        />
        <Moon
          className={cn(
            "h-3.5 w-3.5 transition-transform duration-200",
            isDark ? "rotate-0 scale-100 opacity-100" : "-rotate-90 scale-0 opacity-0 absolute"
          )}
        />
      </button>
    );
  }

  return (
    <div
      role="radiogroup"
      aria-label="Color theme selector"
      className={cn(
        "inline-flex items-center rounded-[5px] border border-border bg-secondary/70 p-0.5 text-xs select-none",
        className
      )}
    >
      <button
        type="button"
        role="radio"
        aria-checked={!isDark}
        onClick={(e) => setThemeMode("light", getOrigin(e))}
        className={cn(
          "flex items-center gap-1.5 px-2.5 py-1 rounded-[4px] font-medium transition-all text-xs",
          !isDark
            ? "bg-card text-foreground shadow-sm font-semibold"
            : "text-muted-foreground hover:text-foreground"
        )}
        title="Light theme"
      >
        <Sun className={cn("h-3.5 w-3.5", !isDark ? "text-[#D48817]" : "text-muted-foreground")} />
        <span>Light</span>
      </button>

      <button
        type="button"
        role="radio"
        aria-checked={isDark}
        onClick={(e) => setThemeMode("dark", getOrigin(e))}
        className={cn(
          "flex items-center gap-1.5 px-2.5 py-1 rounded-[4px] font-medium transition-all text-xs",
          isDark
            ? "bg-card text-foreground shadow-sm font-semibold"
            : "text-muted-foreground hover:text-foreground"
        )}
        title="Dark theme"
      >
        <Moon
          className={cn(
            "h-3.5 w-3.5",
            isDark ? "text-brand-lavender" : "text-muted-foreground"
          )}
        />
        <span>Dark</span>
      </button>
    </div>
  );
}
