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
import { LogOut, HardDrive, Puzzle, Copy, Check, Trash2, ChevronDown } from "lucide-react";
import { formatBytes } from "@/lib/utils";

interface TopNavProps {
  onHomeClick?: () => void;
}

export function TopNav({ onHomeClick }: TopNavProps) {
  const { data: user } = useUser();
  const logout = useLogout();
  const [accountOpen, setAccountOpen] = useState(false);
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
    <header className="sticky top-0 z-40 h-15 py-2 border-b border-border/60 bg-background/85 backdrop-blur-xl">
      <div className="mx-auto flex h-full max-w-[1400px] items-center justify-between px-4 sm:px-8">
        {/* Official PACE Wordmark */}
        <button
          type="button"
          onClick={onHomeClick}
          className="flex items-center rounded-xl px-1 py-1 text-left transition-opacity hover:opacity-85 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          <PaceLogo size="sm" className="sm:hidden" />
          <PaceLogo size="md" className="hidden sm:inline-flex" />
        </button>

        {/* Quiet, Mature Right Controls */}
        <div className="flex items-center gap-2 sm:gap-2.5">
          {user && (
            <button
              type="button"
              onClick={handleOpenExtModal}
              title="Connect Google Flow Chrome Extension"
              className="hidden sm:inline-flex h-9 items-center gap-1.5 rounded-xl border border-border bg-card/80 px-3 text-xs font-medium text-muted-foreground hover:text-foreground hover:border-brand-lavender/45 transition-colors"
            >
              <Puzzle className="h-3.5 w-3.5 text-[#6D28D9] dark:text-[#B794FF]" />
              <span>Flow Ext</span>
            </button>
          )}

          <div className="rounded-xl border border-border bg-card/80 p-0.5">
            <ThemeToggle variant="compact" className="h-8 w-8 rounded-lg" />
          </div>

          {user && (
            <div className="relative">
              <button
                type="button"
                onClick={() => setAccountOpen((v) => !v)}
                className="flex h-9 items-center gap-2 rounded-xl border border-border bg-card/90 pl-1.5 pr-2.5 text-left hover:border-brand-lavender/45 transition-colors"
              >
                <span className="flex h-6 w-6 items-center justify-center rounded-lg bg-[#6D28D9]/12 text-[#6D28D9] dark:bg-[#A970FF]/18 dark:text-[#C4B5FD] text-[11px] font-bold">
                  {initials}
                </span>
                <span className="hidden md:inline text-xs font-semibold text-foreground max-w-[140px] truncate">
                  {user.name}
                </span>
                <ChevronDown className="h-3.5 w-3.5 text-muted-foreground" />
              </button>

              {accountOpen && (
                <>
                  <div
                    className="fixed inset-0 z-10"
                    onClick={() => setAccountOpen(false)}
                  />
                  <div className="absolute right-0 mt-2 z-20 w-64 rounded-2xl border border-border bg-popover p-2 shadow-xl space-y-1.5">
                    <div className="px-2.5 py-2 border-b border-border/70">
                      <div className="text-xs font-semibold text-foreground truncate">
                        {user.name}
                      </div>
                      <div className="text-[11px] text-muted-foreground truncate">
                        {user.email}
                      </div>
                    </div>

                    <div className="px-2.5 py-2 space-y-1.5">
                      <div className="flex items-center justify-between text-[11px] text-muted-foreground">
                        <span className="inline-flex items-center gap-1.5">
                          <HardDrive className="h-3 w-3 text-[#6D28D9] dark:text-[#B794FF]" />
                          Storage
                        </span>
                        <span className="font-mono tabular-nums">
                          {formatBytes(user.storage_used_bytes)} ({quotaPct}%)
                        </span>
                      </div>
                      <div className="h-1.5 w-full overflow-hidden rounded-full bg-secondary">
                        <div
                          className="h-full rounded-full bg-[#6D28D9] dark:bg-[#A970FF]"
                          style={{ width: `${quotaPct}%` }}
                        />
                      </div>
                    </div>

                    <button
                      type="button"
                      onClick={() => {
                        setAccountOpen(false);
                        handleOpenExtModal();
                      }}
                      className="sm:hidden flex w-full items-center gap-2 rounded-xl px-2.5 py-2 text-left text-xs font-medium text-foreground hover:bg-secondary transition-colors"
                    >
                      <Puzzle className="h-3.5 w-3.5 text-[#6D28D9] dark:text-[#B794FF]" />
                      Flow Extension
                    </button>

                    <button
                      type="button"
                      onClick={() => {
                        setAccountOpen(false);
                        logout.mutate();
                      }}
                      className="flex w-full items-center gap-2 rounded-xl px-2.5 py-2 text-left text-xs font-medium text-red-600 dark:text-red-400 hover:bg-red-500/10 transition-colors"
                    >
                      <LogOut className="h-3.5 w-3.5" />
                      Sign out
                    </button>
                  </div>
                </>
              )}
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
