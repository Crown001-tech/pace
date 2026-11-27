import * as React from "react";
import { cn } from "@/lib/utils";

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "primary" | "secondary" | "outline" | "ghost" | "destructive";
  size?: "sm" | "md" | "lg" | "icon";
  isLoading?: boolean;
}

export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  (
    {
      className,
      variant = "primary",
      size = "md",
      isLoading = false,
      disabled,
      children,
      ...props
    },
    ref
  ) => {
    const baseStyles =
      "inline-flex items-center justify-center rounded-[10px] font-medium touch-manipulation select-none transition-all duration-150 ease-out focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-lavender/60 focus-visible:ring-offset-2 focus-visible:ring-offset-background disabled:opacity-45 disabled:pointer-events-none active:scale-[0.98]";

    const variants = {
      primary:
        "bg-brand-violet text-white hover:bg-brand-dark shadow-[0_1px_2px_rgba(59,10,122,0.22),inset_0_1px_0_rgba(255,255,255,0.16)] dark:bg-brand-lavender dark:text-[#140B29] dark:font-semibold dark:hover:bg-brand-hover dark:shadow-[0_2px_12px_-2px_rgba(169,112,255,0.35),inset_0_1px_0_rgba(255,255,255,0.3)]",
      secondary:
        "bg-secondary text-secondary-foreground hover:bg-secondary/75",
      outline:
        "border border-border bg-card/85 text-foreground hover:bg-secondary/80 hover:border-brand-lavender/40 shadow-[0_1px_2px_rgba(0,0,0,0.03)]",
      ghost:
        "bg-transparent text-muted-foreground hover:bg-secondary/70 hover:text-foreground",
      destructive:
        "bg-red-600 text-white hover:bg-red-700 shadow-sm dark:bg-red-500/15 dark:text-red-300 dark:hover:bg-red-500/25",
    };

    const sizes = {
      sm: "h-8 px-3 text-xs gap-1.5",
      md: "h-9 px-4 text-sm gap-2",
      lg: "h-11 px-5 text-sm gap-2",
      icon: "h-9 w-9 p-0",
    };

    return (
      <button
        ref={ref}
        className={cn(baseStyles, variants[variant], sizes[size], className)}
        disabled={disabled || isLoading}
        {...props}
      >
        {isLoading && (
          <svg
            className="-ml-0.5 mr-1 h-3.5 w-3.5 animate-spin text-current"
            xmlns="http://www.w3.org/2000/svg"
            fill="none"
            viewBox="0 0 24 24"
          >
            <circle
              className="opacity-25"
              cx="12"
              cy="12"
              r="10"
              stroke="currentColor"
              strokeWidth="4"
            />
            <path
              className="opacity-75"
              fill="currentColor"
              d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
            />
          </svg>
        )}
        {children}
      </button>
    );
  }
);
Button.displayName = "Button";
