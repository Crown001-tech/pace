import * as React from "react";
import { cn } from "@/lib/utils";

export interface BadgeProps extends React.HTMLAttributes<HTMLSpanElement> {
  variant?: "default" | "secondary" | "outline" | "success" | "warning" | "cyan";
}

export function Badge({
  className,
  variant = "default",
  ...props
}: BadgeProps) {
  const variants = {
    default:
      "border border-[#8B5CF6]/35 bg-[#8B5CF6]/12 text-[#5B21B6] dark:text-[#C4B5FD]",
    secondary:
      "border border-white/60 dark:border-white/10 bg-white/90 dark:bg-[#1E2236]/90 text-foreground backdrop-blur-md",
    outline:
      "border border-border text-muted-foreground bg-transparent",
    success:
      "border border-emerald-500/35 bg-emerald-500/15 text-emerald-800 dark:text-emerald-300 backdrop-blur-md",
    warning:
      "border border-amber-500/35 bg-amber-500/15 text-amber-800 dark:text-amber-300 backdrop-blur-md",
    cyan:
      "border border-border bg-secondary text-foreground",
  };

  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded-full px-2.5 py-0.5 text-[11px] font-semibold leading-none transition-colors shadow-sm",
        variants[variant],
        className
      )}
      {...props}
    />
  );
}
