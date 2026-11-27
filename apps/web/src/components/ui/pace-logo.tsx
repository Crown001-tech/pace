"use client";

import React from "react";
import { cn } from "@/lib/utils";

/**
 * Crisp vector squircle app mark matching the PACE brand identity.
 */
export function PaceMark({ className = "h-8 w-8" }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 64 64"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      className={className}
      aria-label="PACE"
    >
      <defs>
        <linearGradient id="pace-mark-bg" x1="8" y1="6" x2="56" y2="58" gradientUnits="userSpaceOnUse">
          <stop offset="0%" stopColor="#4A1096" />
          <stop offset="100%" stopColor="#2B055E" />
        </linearGradient>
      </defs>
      <rect x="2" y="2" width="60" height="60" rx="16" fill="url(#pace-mark-bg)" />
      <rect
        x="2.75"
        y="2.75"
        width="58.5"
        height="58.5"
        rx="15.25"
        stroke="rgba(169, 112, 255, 0.35)"
        strokeWidth="1.5"
      />
      {/* Signature rounded 'a' with forward pace tail */}
      <path
        d="M32 19C24.8203 19 19 24.8203 19 32C19 39.1797 24.8203 45 32 45H43.5C45.433 45 47 43.433 47 41.5C47 39.567 45.433 38 43.5 38H42.2C43.95 36.22 45 34.22 45 32C45 24.8203 39.1797 19 32 19ZM32 25.5C35.5899 25.5 38.5 28.4101 38.5 32C38.5 35.5899 35.5899 38.5 32 38.5C28.4101 38.5 25.5 35.5899 25.5 32C25.5 28.4101 28.4101 25.5 32 25.5Z"
        fill="#A970FF"
      />
    </svg>
  );
}

/**
 * Crisp vector PACE wordmark with the signature lavender 'a' mark.
 */
export function PaceLogo({
  className,
  showMark = true,
  size = "md",
}: {
  className?: string;
  showMark?: boolean;
  size?: "sm" | "md" | "lg";
}) {
  const sizes = {
    sm: { mark: "h-6 w-6", text: "text-lg" },
    md: { mark: "h-7 w-7", text: "text-xl" },
    lg: { mark: "h-9 w-9", text: "text-2xl" },
  }[size];

  return (
    <span className={cn("inline-flex items-center gap-2.5 select-none", className)}>
      {showMark && <PaceMark className={cn(sizes.mark, "shrink-0")} />}
      <span
        className={cn(
          "font-extrabold tracking-[-0.04em] leading-none text-brand-violet dark:text-white",
          sizes.text
        )}
      >
        p<span className="text-brand-lavender">a</span>ce
      </span>
    </span>
  );
}
