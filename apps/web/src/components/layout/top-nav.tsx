"use client";

import React, { useState } from "react";
import { useUser, useLogout } from "@/hooks/use-auth";
import { api } from "@/lib/api-client";
import { Button } from "@/components/ui/button";
import { PaceLogo } from "@/components/ui/pace-logo";
import { ThemeToggle } from "@/components/ui/theme-toggle";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog";
import { LogOut, HardDrive, Puzzle, Copy, Check, Trash2 } from "lucide-react";
import { formatBytes } from "@/lib/utils";

interface TopNavProps {
  onHomeClick?: () => void;
}

export function TopNav({ onHomeClick }: TopNavProps) {
  const { data: user } = useUser();
  const logout = useLogout();
  const [extOpen, setExtOpen] = useState(false);
  const [pairingCode, setPairingCode] = useState<string | null>(null);
  const [copiedCode, setCopiedCode] = useState(false);
  const [extLoading, setExtLoading] = useState(false);
  const [extError, setExtError] = useState<string | null>(null);
  const [extTokens, setExtTokens] = useState<
    { id: string; device_name: string; created_at: string }[]
  >([]);

  const loadExtensionTokens = async () => {
    try {
      const list = await api.extension.listTokens();
      setExtTokens(list);
    } catch {}
  };

  const handleOpenExtModal = () => {
    setExtOpen(true);
    setExtError(null);
    void loadExtensionTokens();
  };

  const handleGenerateCode = async () => {
    setExtLoading(true);
    setExtError(null);
    setCopiedCode(false);
    try {
      const res = await api.extension.createPairingCode();
      setPairingCode(res.code);
    } catch (err) {
      setExtError(
        err instanceof Error ? err.message : "Failed generating pairing code"
      );
    } finally {
      setExtLoading(false);
    }
  };

  const handleRevokeToken = async (id: string) => {
    try {
      await api.extension.revokeToken(id);
      setExtTokens((prev) => prev.filter((t) => t.id !== id));
    } catch {}
  };

  const quotaPct = user
    ? Math.min(100, Math.round((user.storage_used_bytes / user.storage_quota_bytes) * 100))
    : 0;

  const initials = user?.name
    ? user.name
        .split(/\s+/)
        .map((p) => p[0])
        .slice(0, 2)
        .join("")
        .toUpperCase()
    : "P";

  return (
    <header className="sticky top-0 z-40 h-14 border-b border-border/60 bg-background/80 backdrop-blur-xl">
      <div className="mx-auto flex h-full max-w-[1600px] items-center justify-between px-4 sm:px-6">
        {/* Brand */}
        <button
          type="button"
          onClick={onHomeClick}
          className="flex items-center rounded-[10px] px-1 py-1 text-left transition-opacity hover:opacity-85 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-lavender"
        >
          <PaceLogo size="sm" className="sm:hidden" />
          <PaceLogo size="md" className="hidden sm:inline-flex" />
        </button>

        {/* Right Controls */}
        <div className="flex items-center gap-2.5 sm:gap-3.5">
          {user && (
            <div
              className="hidden lg:flex items-center gap-2.5 rounded-[10px] bg-secondary/70 px-3 py-1.5 text-[11px] font-mono tabular-nums text-muted-foreground"
              title={`${formatBytes(user.storage_used_bytes)} of ${formatBytes(user.storage_quota_bytes)} used`}
            >
              <HardDrive className="h-3.5 w-3.5 text-brand-violet dark:text-brand-lavender shrink-0" />
              <span>{formatBytes(user.storage_used_bytes)}</span>
              <div className="h-1.5 w-14 overflow-hidden rounded-sm bg-background/70">
                <div
                  className="h-full bg-brand-violet dark:bg-brand-lavender transition-all duration-300"
                  style={{ width: `${quotaPct}%` }}
                />
              </div>
              <span className="text-muted-foreground/80">{quotaPct}%</span>
            </div>
          )}

          <div className="hidden md:block">
            <ThemeToggle variant="segmented" />
          </div>
          <div className="md:hidden">
            <ThemeToggle variant="compact" />
          </div>

          {user && (
            <div className="flex items-center gap-2.5 pl-2.5 border-l border-border/60">
              <Button
                variant="outline"
                size="sm"
                onClick={handleOpenExtModal}
                title="Connect Google Flow Chrome Extension"
                className="h-8 gap-1.5 px-2.5 text-xs border-border/70"
              >
                <Puzzle className="h-3.5 w-3.5 text-brand-violet dark:text-brand-lavender" />
                <span className="hidden lg:inline">Flow Ext</span>
              </Button>

              <div className="flex h-8 w-8 items-center justify-center rounded-[10px] bg-brand-violet/10 text-brand-violet dark:bg-brand-lavender/15 dark:text-brand-lavender text-xs font-semibold">
                {initials}
              </div>
              <div className="hidden sm:flex flex-col items-start leading-none">
                <span className="text-xs font-semibold text-foreground">
                  {user.name}
                </span>
                <span className="mt-0.5 text-[11px] text-muted-foreground">
                  {user.email}
                </span>
              </div>

              <Button
                variant="ghost"
                size="sm"
                onClick={() => logout.mutate()}
                title="Sign out"
                aria-label="Sign out"
                className="h-8 px-2.5 text-muted-foreground hover:text-foreground"
              >
                <LogOut className="h-3.5 w-3.5" />
                <span className="hidden md:inline text-xs">Sign out</span>
              </Button>
            </div>
          )}
        </div>
      </div>

      {/* Connect Google Flow Chrome Extension Modal */}
      <Dialog open={extOpen} onOpenChange={setExtOpen}>
        <DialogContent onClose={() => setExtOpen(false)}>
          <DialogHeader>
            <DialogTitle>Connect Google Flow Extension</DialogTitle>
            <DialogDescription>
              Pair the PACE Chrome Extension (`apps/extension`) to batch-run Image & Video prompts in Google Flow and send assets straight to your projects.
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4">
            <div className="rounded-xl border border-border/70 bg-secondary/35 p-3.5 space-y-2.5">
              <div className="flex items-center justify-between gap-2">
                <span className="text-xs font-semibold text-foreground">
                  10-Minute One-Time Pairing Code
                </span>
                <Button
                  type="button"
                  size="sm"
                  onClick={() => void handleGenerateCode()}
                  isLoading={extLoading}
                >
                  {pairingCode ? "New Code" : "Generate Code"}
                </Button>
              </div>

              {pairingCode && (
                <div className="flex items-center justify-between gap-2 rounded-lg border border-brand-lavender/45 bg-background px-3 py-2">
                  <span className="font-mono text-sm font-bold tracking-wider text-brand-violet dark:text-brand-lavender">
                    {pairingCode}
                  </span>
                  <Button
                    type="button"
                    size="sm"
                    variant="ghost"
                    onClick={() => {
                      navigator.clipboard.writeText(pairingCode);
                      setCopiedCode(true);
                    }}
                    className="h-7 gap-1 px-2 text-xs"
                  >
                    {copiedCode ? (
                      <>
                        <Check className="h-3.5 w-3.5 text-emerald-500" />
                        Copied
                      </>
                    ) : (
                      <>
                        <Copy className="h-3.5 w-3.5" />
                        Copy
                      </>
                    )}
                  </Button>
                </div>
              )}

              {extError && (
                <p className="text-xs text-destructive">{extError}</p>
              )}

              <p className="text-[11px] text-muted-foreground">
                Load unpacked from <code className="font-mono">apps/extension</code> in Chrome (<code className="font-mono">chrome://extensions</code>), then paste this code in the extension popup.
              </p>
            </div>

            {extTokens.length > 0 && (
              <div className="space-y-2">
                <span className="text-xs font-semibold text-foreground">
                  Connected Extensions ({extTokens.length})
                </span>
                <div className="space-y-1.5 max-h-36 overflow-y-auto">
                  {extTokens.map((t) => (
                    <div
                      key={t.id}
                      className="flex items-center justify-between rounded-lg border border-border/60 bg-secondary/25 px-3 py-2 text-xs"
                    >
                      <span className="font-medium text-foreground truncate">
                        {t.device_name}
                      </span>
                      <button
                        type="button"
                        onClick={() => void handleRevokeToken(t.id)}
                        title="Revoke extension access"
                        className="rounded p-1 text-red-400 hover:bg-red-500/15"
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                      </button>
                    </div>
                  ))}
                </div>
              </div>
            )}

            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                onClick={() => setExtOpen(false)}
              >
                Close
              </Button>
            </DialogFooter>
          </div>
        </DialogContent>
      </Dialog>
    </header>
  );
}
