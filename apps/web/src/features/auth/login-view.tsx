"use client";

import React, { useState } from "react";
import {
  useLogin,
  useDevLogin,
  useChangePassword,
  useLogout,
  OFFLINE_STUDIO_ENABLED,
} from "@/hooks/use-auth";
import { ApiError } from "@/lib/api-client";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ThemeToggle } from "@/components/ui/theme-toggle";
import type { UserProfile } from "@pace/shared-types";
import {
  AlertCircle,
  Lock,
  KeyRound,
  Film,
  Layers,
  LogOut,
  AudioLines,
  ArrowRight,
} from "lucide-react";

export function LoginView() {
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  const login = useLogin();
  const devLogin = useDevLogin();

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setErrorMsg(null);
    if (!email.trim() || !password) {
      setErrorMsg("Enter both email and password.");
      return;
    }

    try {
      await login.mutateAsync({ email: email.trim(), password });
    } catch (err) {
      if (err instanceof ApiError) {
        if (err.code === "ACCOUNT_LOCKED") {
          setErrorMsg(
            "Account temporarily locked after repeated failed attempts. Wait 15 minutes before retrying."
          );
        } else {
          setErrorMsg(err.message);
        }
      } else {
        setErrorMsg(
          "Cannot reach PACE API server. Check that pace-api is running or use Offline Studio below."
        );
      }
    }
  };

  return (
    <div className="min-h-screen bg-background flex flex-col">
      {/* Minimal Top Bar */}
      <header className="flex items-center justify-between px-5 sm:px-8 h-14 border-b border-border/60 bg-background/80 backdrop-blur-xl">
        <div className="flex items-center gap-2">
          {/* eslint-disable-next-line @next/next/no-img-element */}
          <img
            src="/pace-logo-transparent.png"
            alt="PACE"
            className="h-7 w-auto block dark:hidden"
          />
          {/* eslint-disable-next-line @next/next/no-img-element */}
          <img
            src="/pace-logo-white-transparent.png"
            alt="PACE"
            className="h-7 w-auto hidden dark:block"
          />
        </div>
        <ThemeToggle variant="compact" />
      </header>

      {/* Main Split Workspace Entry */}
      <div className="flex-1 grid grid-cols-1 lg:grid-cols-12">
        {/* Left Brand Canvas */}
        <div className="hidden lg:flex lg:col-span-6 xl:col-span-7 relative overflow-hidden flex-col justify-between bg-[#22074A] text-white p-12 xl:p-16">
          {/* Ambient soft studio glow */}
          <div
            aria-hidden="true"
            className="pointer-events-none absolute -top-24 -left-24 h-[420px] w-[420px] rounded-full opacity-45 blur-3xl"
            style={{ background: "radial-gradient(circle, #A970FF 0%, transparent 70%)" }}
          />
          <div
            aria-hidden="true"
            className="pointer-events-none absolute -bottom-32 right-0 h-[460px] w-[460px] rounded-full opacity-35 blur-3xl"
            style={{ background: "radial-gradient(circle, #6E2CF2 0%, transparent 70%)" }}
          />

          <div className="relative z-10 max-w-lg space-y-6">
            <span className="inline-flex items-center gap-2 rounded-[8px] bg-white/10 px-3 py-1.5 text-xs font-medium text-[#E5D6FF] backdrop-blur-sm">
              <span className="h-1.5 w-1.5 rounded-sm bg-[#A970FF]" />
              Simple. Smart. Focused.
            </span>

            <h1 className="text-4xl xl:text-[2.75rem] font-bold tracking-tight text-white leading-[1.12]">
              Script to frame-exact video,{" "}
              <span className="text-[#C79EFF]">without the friction.</span>
            </h1>

            <p className="text-base text-[#D7CBEF] leading-relaxed max-w-md">
              Turn your script, visuals, and narration into a tightly synchronized timeline with
              instant browser preview and broadcast-ready MP4 export.
            </p>
          </div>

          <div className="relative z-10 grid grid-cols-3 gap-4 pt-10 max-w-xl">
            <div className="rounded-xl bg-white/[0.07] p-4 backdrop-blur-sm space-y-2 border border-white/10">
              <Layers className="h-4 w-4 text-[#C79EFF]" />
              <div className="text-xs font-semibold text-white">Scene Flow</div>
              <div className="text-[11px] text-[#C5B7E3]">16:9 &amp; 9:16 Canvases</div>
            </div>
            <div className="rounded-xl bg-white/[0.07] p-4 backdrop-blur-sm space-y-2 border border-white/10">
              <AudioLines className="h-4 w-4 text-[#C79EFF]" />
              <div className="text-xs font-semibold text-white">Narration Sync</div>
              <div className="text-[11px] text-[#C5B7E3]">Pause-Aware Cuts</div>
            </div>
            <div className="rounded-xl bg-white/[0.07] p-4 backdrop-blur-sm space-y-2 border border-white/10">
              <Film className="h-4 w-4 text-[#C79EFF]" />
              <div className="text-xs font-semibold text-white">Frame Parity</div>
              <div className="text-[11px] text-[#C5B7E3]">30 FPS Exact Export</div>
            </div>
          </div>
        </div>

        {/* Right Sign In Panel */}
        <div className="lg:col-span-6 xl:col-span-5 flex items-center justify-center p-6 sm:p-10">
          <div className="w-full max-w-[400px] rounded-2xl border border-border/70 bg-card p-7 sm:p-8 studio-card space-y-6">
            <div className="space-y-1.5">
              <h2 className="text-xl font-bold tracking-tight text-foreground">
                Welcome back
              </h2>
              <p className="text-sm text-muted-foreground">
                Sign in to open your studio projects.
              </p>
            </div>

            {errorMsg && (
              <div
                role="alert"
                className="flex items-start gap-2.5 rounded-[10px] border border-red-500/30 bg-red-500/10 p-3.5 text-xs text-red-700 dark:text-red-300"
              >
                <AlertCircle className="h-4 w-4 shrink-0 mt-0.5" />
                <span>{errorMsg}</span>
              </div>
            )}

            <form onSubmit={handleSubmit} className="space-y-4">
              <div className="space-y-1.5">
                <label className="block text-xs font-medium text-foreground">
                  Email address
                </label>
                <Input
                  type="email"
                  autoComplete="email"
                  placeholder="name@studio.local"
                  value={email}
                  onChange={(e) => setEmail(e.target.value)}
                  disabled={login.isPending}
                  required
                />
              </div>

              <div className="space-y-1.5">
                <label className="block text-xs font-medium text-foreground">
                  Password
                </label>
                <Input
                  type="password"
                  autoComplete="current-password"
                  placeholder="Enter password"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  disabled={login.isPending}
                  required
                />
              </div>

              <Button
                type="submit"
                size="lg"
                className="w-full mt-1"
                isLoading={login.isPending}
              >
                <Lock className="h-4 w-4" />
                Sign in
              </Button>
            </form>

            {OFFLINE_STUDIO_ENABLED && (
              <>
                <div className="relative py-1">
                  <div className="absolute inset-0 flex items-center">
                    <span className="w-full border-t border-border/70" />
                  </div>
                  <div className="relative flex justify-center text-[11px] font-medium text-muted-foreground">
                    <span className="bg-card px-2.5">Local Studio</span>
                  </div>
                </div>

                <Button
                  type="button"
                  variant="secondary"
                  size="lg"
                  className="w-full justify-between"
                  onClick={() => devLogin.mutate()}
                  isLoading={devLogin.isPending}
                >
                  <span>Continue in Offline Studio</span>
                  <ArrowRight className="h-4 w-4 opacity-70" />
                </Button>
              </>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

/**
 * Full-screen mandatory password update gate rendered when an authenticated account
 * carries `must_change_password = true` (AUTH-03).
 */
export function ForcePasswordChangeView({ user }: { user: UserProfile }) {
  const [currentPassword, setCurrentPassword] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  const changePassword = useChangePassword();
  const logout = useLogout();

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setErrorMsg(null);

    if (!currentPassword) {
      setErrorMsg("Enter your temporary current password.");
      return;
    }
    if (newPassword.length < 10) {
      setErrorMsg("New password must be at least 10 characters.");
      return;
    }
    if (newPassword === currentPassword) {
      setErrorMsg("New password must be different from your temporary password.");
      return;
    }
    if (newPassword !== confirmPassword) {
      setErrorMsg("Passwords do not match.");
      return;
    }

    try {
      await changePassword.mutateAsync({
        current_password: currentPassword,
        new_password: newPassword,
      });
    } catch (err) {
      if (err instanceof ApiError) {
        setErrorMsg(err.message);
      } else {
        setErrorMsg("Could not update password. Please verify connection and retry.");
      }
    }
  };

  return (
    <div className="min-h-screen bg-background flex flex-col">
      <div className="flex items-center justify-between px-5 sm:px-8 h-14 border-b border-border/60 bg-background/80 backdrop-blur-xl">
        <div className="flex items-center gap-2">
          {/* eslint-disable-next-line @next/next/no-img-element */}
          <img
            src="/pace-logo-transparent.png"
            alt="PACE"
            className="h-7 w-auto block dark:hidden"
          />
          {/* eslint-disable-next-line @next/next/no-img-element */}
          <img
            src="/pace-logo-white-transparent.png"
            alt="PACE"
            className="h-7 w-auto hidden dark:block"
          />
        </div>
        <div className="flex items-center gap-2">
          <ThemeToggle variant="compact" />
          <Button
            variant="ghost"
            size="sm"
            onClick={() => logout.mutate()}
            isLoading={logout.isPending}
          >
            <LogOut className="h-3.5 w-3.5" />
            Sign out
          </Button>
        </div>
      </div>

      <div className="flex-1 flex items-center justify-center p-5">
        <div className="w-full max-w-md rounded-2xl border border-border/70 bg-card p-7 sm:p-8 space-y-5 studio-card">
          <div className="space-y-2">
            <div className="inline-flex items-center gap-1.5 rounded-[6px] bg-amber-500/12 px-2.5 py-1 text-[11px] font-medium text-amber-700 dark:text-amber-300">
              <KeyRound className="h-3.5 w-3.5" />
              Security Action Required
            </div>
            <h1 className="text-xl font-bold text-foreground">
              Update temporary password
            </h1>
            <p className="text-sm text-muted-foreground leading-relaxed">
              {user.email ? `Account ${user.email} is` : "Your account is"} using a temporary
              password. Choose a new password (10+ characters) before accessing studio projects.
            </p>
          </div>

          {errorMsg && (
            <div
              role="alert"
              className="flex items-start gap-2 rounded-[10px] border border-red-500/30 bg-red-500/10 p-3.5 text-xs text-red-700 dark:text-red-300"
            >
              <AlertCircle className="h-4 w-4 shrink-0 mt-0.5" />
              <span>{errorMsg}</span>
            </div>
          )}

          <form onSubmit={handleSubmit} className="space-y-4">
            <div className="space-y-1.5">
              <label className="text-xs font-medium text-foreground">
                Current temporary password
              </label>
              <Input
                type="password"
                autoComplete="current-password"
                value={currentPassword}
                onChange={(e) => setCurrentPassword(e.target.value)}
                required
              />
            </div>

            <div className="space-y-1.5">
              <label className="text-xs font-medium text-foreground">
                New password (min 10 characters)
              </label>
              <Input
                type="password"
                autoComplete="new-password"
                value={newPassword}
                onChange={(e) => setNewPassword(e.target.value)}
                required
              />
            </div>

            <div className="space-y-1.5">
              <label className="text-xs font-medium text-foreground">
                Confirm new password
              </label>
              <Input
                type="password"
                autoComplete="new-password"
                value={confirmPassword}
                onChange={(e) => setConfirmPassword(e.target.value)}
                required
              />
            </div>

            <Button
              type="submit"
              size="lg"
              className="w-full mt-2"
              isLoading={changePassword.isPending}
            >
              Save Password &amp; Continue
            </Button>
          </form>
        </div>
      </div>
    </div>
  );
}
