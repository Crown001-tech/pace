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
      "bg-brand-violet/10 text-brand-violet dark:bg-brand-lavender/15 dark:text-brand-lavender",
    secondary:
      "bg-secondary text-secondary-foreground",
    outline:
      "border border-border/80 text-muted-foreground bg-transparent",
    success:
      "bg-emerald-500/12 text-emerald-700 dark:bg-emerald-500/15 dark:text-emerald-300",
    warning:
      "bg-amber-500/12 text-amber-800 dark:bg-amber-500/15 dark:text-amber-300",
    cyan:
      "bg-brand-lavender/12 text-brand-violet dark:bg-brand-lavender/15 dark:text-brand-lavender",
  };

  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 rounded-[6px] px-2 py-0.5 text-[11px] font-medium tracking-tight leading-none transition-colors",
        variants[variant],
        className
      )}
      {...props}
    />
  );
}
