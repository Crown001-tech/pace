"use client";

import React from "react";
import { cn } from "@/lib/utils";

/**
 * PACE brand mark image.
 */
export function PaceMark({ className = "h-8 w-8" }: { className?: string }) {
  return (
    // eslint-disable-next-line @next/next/no-img-element
    <img
      src="/pace-mark-transparent.png"
      alt="PACE"
      className={cn("object-contain select-none", className)}
    />
  );
}

/**
 * Official PACE wordmark logo (light & dark variants).
 */
export function PaceLogo({
  className,
  size = "md",
}: {
  className?: string;
  showMark?: boolean;
  size?: "sm" | "md" | "lg";
}) {
  const heightClass = {
    sm: "h-6",
    md: "h-7",
    lg: "h-9",
  }[size];

  return (
    <span className={cn("inline-flex items-center select-none", className)}>
      {/* eslint-disable-next-line @next/next/no-img-element */}
      <img
        src="/pace-logo-transparent.png"
        alt="PACE"
        className={cn(heightClass, "w-auto block dark:hidden")}
      />
      {/* eslint-disable-next-line @next/next/no-img-element */}
      <img
        src="/pace-logo-white-transparent.png"
        alt="PACE"
        className={cn(heightClass, "w-auto hidden dark:block")}
      />
    </span>
  );
}
