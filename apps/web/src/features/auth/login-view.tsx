"use client";

import React, { useEffect, useRef, useState } from "react";
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
import { PaceLogo, PaceMark } from "@/components/ui/pace-logo";
import { ThemeToggle } from "@/components/ui/theme-toggle";
import type { UserProfile } from "@pace/shared-types";
import {
  AlertCircle,
  ArrowRight,
  AudioLines,
  Eye,
  EyeOff,
  Film,
  KeyRound,
  Lock,
  LogOut,
  Play,
  User,
} from "lucide-react";

const SHOWCASE_CARDS = [
  {
    src: "/showcase/scene-1.jpg",
    title: "Tokyo Midnight Rain",
    badge: "16:9 · 30 FPS",
    cue: "Voiceover locked to scene cuts",
  },
  {
    src: "/showcase/scene-2.jpg",
    title: "Deep Ocean Cavern",
    badge: "Zoom 1 · Ducking",
    cue: "Music ducks smoothly under speech",
  },
  {
    src: "/showcase/scene-3.jpg",
    title: "Above Twilight Clouds",
    badge: "9:16 · Captions",
    cue: "Timed subtitles ready for export",
  },
];

export function LoginView() {
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [rememberMe, setRememberMe] = useState(true);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const [activeIndex, setActiveIndex] = useState(0);

  const containerRef = useRef<HTMLDivElement | null>(null);
  const [parallax, setParallax] = useState({ x: 0, y: 0 });

  const login = useLogin();
  const devLogin = useDevLogin();

  useEffect(() => {
    const timer = window.setInterval(() => {
      setActiveIndex((prev) => (prev + 1) % SHOWCASE_CARDS.length);
    }, 4800);
    return () => window.clearInterval(timer);
  }, []);

  const handleMouseMove = (e: React.MouseEvent<HTMLDivElement>) => {
    const el = containerRef.current;
    if (!el) return;
    const rect = el.getBoundingClientRect();
    const nx = ((e.clientX - rect.left) / rect.width - 0.5) * 2;
    const ny = ((e.clientY - rect.top) / rect.height - 0.5) * 2;
    setParallax({ x: nx, y: ny });
  };

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

  const current = SHOWCASE_CARDS[activeIndex];
  const nextCard = SHOWCASE_CARDS[(activeIndex + 1) % SHOWCASE_CARDS.length];

  return (
    <div
      ref={containerRef}
      onMouseMove={handleMouseMove}
      onMouseLeave={() => setParallax({ x: 0, y: 0 })}
      className="pace-sculpted-canvas relative min-h-screen w-full overflow-hidden flex items-center justify-center px-4 py-10 sm:px-8 lg:px-14"
    >
      {/* Full-Bleed Sculpted 3D Architectural Background Shapes */}
      <div aria-hidden="true" className="pointer-events-none fixed inset-0 overflow-hidden">
        {/* Sweeping Studio Brush Wave SVG */}
        <svg
          viewBox="0 0 1440 900"
          fill="none"
          preserveAspectRatio="none"
          className="absolute inset-0 h-full w-full opacity-80 dark:opacity-45"
        >
          <path
            d="M-120 740 C 260 560, 480 840, 880 540 C 1180 315, 1340 120, 1580 -60 L 1580 960 L -120 960 Z"
            className="fill-[#E4DCFC]/75 dark:fill-[#1B1F38]/85"
          />
          <path
            d="M-160 180 C 180 340, 460 40, 820 220 C 1140 380, 1320 260, 1600 390"
            stroke="currentColor"
            strokeWidth="2"
            className="text-white/80 dark:text-[#A970FF]/15"
          />
        </svg>

        {/* Top-Left Sculpted Concentric 3D Ring */}
        <div
          className="pace-3d-ring absolute -top-32 -left-28 h-[380px] w-[380px] sm:h-[460px] sm:w-[460px] rounded-full flex items-center justify-center transition-transform duration-300 ease-out"
          style={{
            transform: `translate3d(${parallax.x * -14}px, ${parallax.y * -14}px, 0)`,
          }}
        >
          <div className="h-[250px] w-[250px] sm:h-[310px] sm:w-[310px] rounded-full border-[18px] border-[#8B5CF6]/20 dark:border-[#8B5CF6]/25 shadow-inner" />
        </div>

        {/* Right Floating 3D Soft-Clay Sphere */}
        <div
          className="pace-3d-orb animate-float-slow absolute top-[22%] -right-10 sm:right-[4%] h-28 w-28 sm:h-36 sm:w-36 rounded-full"
        />

        {/* Bottom-Right Sculpted Concentric Ring */}
        <div
          className="pace-3d-ring absolute -bottom-44 -right-28 h-[380px] w-[380px] sm:h-[440px] sm:w-[440px] rounded-full flex items-center justify-center transition-transform duration-300 ease-out"
          style={{
            transform: `translate3d(${parallax.x * 12}px, ${parallax.y * 12}px, 0)`,
          }}
        >
          <div className="h-[240px] w-[240px] rounded-full border-[14px] border-[#A970FF]/20 dark:border-[#A970FF]/20" />
        </div>

        {/* Small Left-Bottom Floating 3D Orb */}
        <div
          className="pace-3d-orb hidden lg:block absolute bottom-[12%] left-[8%] h-16 w-16 rounded-full opacity-90 transition-transform duration-300 ease-out"
          style={{
            transform: `translate3d(${parallax.x * 18}px, ${parallax.y * -12}px, 0)`,
          }}
        />
      </div>

      {/* Top-Right Sculpted 3D Theme Toggle Button */}
      <div className="fixed top-5 right-5 sm:top-7 sm:right-7 z-30 pace-3d-medallion rounded-2xl p-1">
        <ThemeToggle variant="compact" />
      </div>

      {/* Main Open-Canvas Layout: Left Open Showcase + Right Tactile 3D Login Card */}
      <div className="relative z-10 w-full max-w-[1160px] grid grid-cols-1 lg:grid-cols-12 gap-10 lg:gap-12 items-center">
        {/* Left: Open-Canvas Dimensional Studio Stage (Desktop) */}
        <section className="hidden lg:flex lg:col-span-7 flex-col justify-center space-y-8 pr-4">
          {/* Interactive Fanned 3D Scene Showcase Sitting Directly on the Sculpted Canvas */}
          <div className="relative h-[370px] w-full max-w-[560px] mx-auto">
            {/* Soft Violet Halo Behind Scene Stack */}
            <div
              aria-hidden="true"
              className="absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 h-[330px] w-[330px] rounded-full bg-gradient-to-br from-[#C4B5FD]/65 via-[#A970FF]/35 to-transparent dark:from-[#6D28D9]/45 dark:via-[#4C1D95]/25 blur-2xl"
            />

            {/* Back Tilted Scene Card */}
            <div
              className="absolute top-4 right-3 w-[360px] aspect-[16/10] rounded-[24px] bg-white dark:bg-[#232740] p-2 shadow-xl border border-white/80 dark:border-white/10 transition-transform duration-500 ease-out"
              style={{
                transform: `rotate(6deg) translate3d(${parallax.x * -10}px, ${parallax.y * -8}px, 0)`,
              }}
            >
              {/* eslint-disable-next-line @next/next/no-img-element */}
              <img
                src={nextCard.src}
                alt=""
                className="h-full w-full rounded-[18px] object-cover opacity-80"
              />
            </div>

            {/* Front Active Framed Cinema Card */}
            <div
              className="relative z-10 mt-6 ml-2 w-[430px] aspect-[16/10] rounded-[26px] bg-white dark:bg-[#1E2238] p-2.5 shadow-[0_28px_60px_-15px_rgba(59,10,122,0.28)] dark:shadow-[0_30px_65px_-15px_rgba(0,0,0,0.75)] border border-white/95 dark:border-white/15 transition-transform duration-300 ease-out"
              style={{
                transform: `rotate(-2deg) translate3d(${parallax.x * 12}px, ${parallax.y * 10}px, 0)`,
              }}
            >
              <div className="relative h-full w-full overflow-hidden rounded-[20px] bg-[#160B2E]">
                {/* eslint-disable-next-line @next/next/no-img-element */}
                <img
                  key={current.src}
                  src={current.src}
                  alt={current.title}
                  className="h-full w-full object-cover animate-ken-burns"
                />
                <div className="absolute inset-0 bg-gradient-to-t from-black/70 via-transparent to-black/20" />

                {/* Play Pill */}
                <div className="absolute top-3 left-3 inline-flex items-center gap-1.5 rounded-full bg-white/90 dark:bg-[#191C2E]/90 backdrop-blur-md px-3 py-1 text-[11px] font-bold text-[#3B0A7A] dark:text-[#C8A6FF] shadow-sm">
                  <Play className="h-3 w-3 fill-current" />
                  <span>{current.badge}</span>
                </div>

                {/* Scene Caption */}
                <div className="absolute bottom-3 left-3 right-3 flex items-center justify-between">
                  <div>
                    <div className="text-sm font-bold text-white drop-shadow-sm">
                      {current.title}
                    </div>
                    <div className="text-xs text-white/80">{current.cue}</div>
                  </div>
                </div>
              </div>
            </div>

            {/* Floating Tactile 3D Chip — Bottom Right */}
            <div
              className="pace-3d-medallion z-20 absolute bottom-2 right-4 rounded-2xl px-4 py-3 flex items-center gap-3 transition-transform duration-300 ease-out"
              style={{
                transform: `translate3d(${parallax.x * -14}px, ${parallax.y * 12}px, 0)`,
              }}
            >
              <div className="flex h-9 w-9 items-center justify-center rounded-xl bg-gradient-to-b from-[#8B5CF6] to-[#5B21B6] text-white shadow-md">
                <AudioLines className="h-4 w-4" />
              </div>
              <div>
                <div className="text-xs font-bold text-foreground">
                  Narration Synced
                </div>
                <div className="text-[11px] text-muted-foreground">
                  Word-accurate cuts
                </div>
              </div>
            </div>

            {/* Floating Tactile 3D Chip — Top Right */}
            <div
              className="pace-3d-medallion z-20 absolute -top-3 right-8 rounded-2xl px-3.5 py-2.5 flex items-center gap-2.5 transition-transform duration-300 ease-out"
              style={{
                transform: `translate3d(${parallax.x * 10}px, ${parallax.y * -10}px, 0)`,
              }}
            >
              <div className="flex h-8 w-8 items-center justify-center rounded-xl bg-[#EDE9FE] dark:bg-[#2B254A] text-[#5B21B6] dark:text-[#C4B5FD]">
                <Film className="h-4 w-4" />
              </div>
              <span className="text-xs font-bold text-foreground pr-1">
                16:9 &amp; 9:16 MP4
              </span>
            </div>
          </div>

          {/* Open-Canvas Typography & Slide Dots */}
          <div className="max-w-lg mx-auto text-center space-y-3">
            <h1 className="font-display text-3xl xl:text-4xl font-extrabold tracking-tight text-foreground leading-[1.12]">
              Script, voice &amp; scenes,{" "}
              <span className="text-[#6D28D9] dark:text-[#B794FF]">
                effortlessly in pace.
              </span>
            </h1>
            <p className="text-sm text-muted-foreground leading-relaxed">
              Align voiceover recordings to your script, style camera motion and captions, and
              export crisp 30 fps video.
            </p>
            <div className="pt-1 flex items-center justify-center gap-2">
              {SHOWCASE_CARDS.map((_, idx) => (
                <button
                  key={idx}
                  type="button"
                  aria-label={`Preview scene ${idx + 1}`}
                  onClick={() => setActiveIndex(idx)}
                  className={`h-2 rounded-full transition-all duration-300 ${idx === activeIndex
                    ? "w-7 bg-[#6D28D9] dark:bg-[#A970FF]"
                    : "w-2 bg-foreground/20 hover:bg-foreground/40"
                    }`}
                />
              ))}
            </div>
          </div>
        </section>

        {/* Right: Sculpted Soft-3D Login Card */}
        <section className="lg:col-span-5 flex justify-center">
          <div className="w-full max-w-[420px] rounded-[34px] bg-white/78 dark:bg-[#191C2E]/88 backdrop-blur-2xl border-[1.5px] border-white/95 dark:border-white/[0.11] shadow-[0_28px_70px_-18px_rgba(59,10,122,0.18),inset_0_2px_4px_rgba(255,255,255,0.95)] dark:shadow-[0_32px_80px_-16px_rgba(0,0,0,0.72),inset_0_1px_1px_rgba(255,255,255,0.12)] px-6 py-8 sm:px-9 sm:py-10 space-y-7">
            {/* Sculpted 3D Logo Medallion & Greeting */}
            <div className="flex flex-col items-center text-center space-y-4">
              <div className="pace-3d-medallion flex h-20 w-20 items-center justify-center rounded-[26px]">
                <PaceMark className="h-12 w-12 drop-shadow-sm" />
              </div>

              <div className="space-y-1">
                <h2 className="font-display text-[28px] font-extrabold tracking-tight text-foreground">
                  Welcome Back
                </h2>
                <p className="text-xs font-medium tracking-wider uppercase text-muted-foreground">
                  Sign in to continue
                </p>
              </div>
            </div>

            {errorMsg && (
              <div
                role="alert"
                className="flex items-start gap-2.5 rounded-2xl border border-red-500/30 bg-red-500/10 p-3.5 text-xs text-red-700 dark:text-red-300"
              >
                <AlertCircle className="h-4 w-4 shrink-0 mt-0.5" />
                <span>{errorMsg}</span>
              </div>
            )}

            <form onSubmit={handleSubmit} className="space-y-4">
              {/* Tactile Carved Email Input */}
              <div className="relative">
                <User className="pointer-events-none absolute left-4 top-1/2 -translate-y-1/2 h-[18px] w-[18px] text-muted-foreground" />
                <input
                  id="login-email"
                  type="email"
                  autoComplete="email"
                  aria-label="Email address"
                  placeholder="Email address"
                  value={email}
                  onChange={(e) => setEmail(e.target.value)}
                  disabled={login.isPending}
                  required
                  className="pace-3d-input h-13 py-3.5 w-full rounded-2xl pl-12 pr-4 text-sm font-medium text-foreground placeholder:text-muted-foreground/75 outline-none transition-all duration-150"
                />
              </div>

              {/* Tactile Carved Password Input */}
              <div className="relative">
                <Lock className="pointer-events-none absolute left-4 top-1/2 -translate-y-1/2 h-[18px] w-[18px] text-muted-foreground" />
                <input
                  id="login-password"
                  type={showPassword ? "text" : "password"}
                  autoComplete="current-password"
                  aria-label="Password"
                  placeholder="Password"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  disabled={login.isPending}
                  required
                  className="pace-3d-input h-13 py-3.5 w-full rounded-2xl pl-12 pr-12 text-sm font-medium text-foreground placeholder:text-muted-foreground/75 outline-none transition-all duration-150"
                />
                <button
                  type="button"
                  onClick={() => setShowPassword((v) => !v)}
                  aria-label={showPassword ? "Hide password" : "Show password"}
                  className="absolute right-3.5 top-1/2 -translate-y-1/2 flex h-8 w-8 items-center justify-center rounded-xl text-muted-foreground hover:text-foreground transition-colors"
                >
                  {showPassword ? (
                    <EyeOff className="h-[18px] w-[18px]" />
                  ) : (
                    <Eye className="h-[18px] w-[18px]" />
                  )}
                </button>
              </div>

              {/* Remember Me Row */}
              <div className="flex items-center px-1 pt-0.5 text-xs">
                <label className="inline-flex items-center gap-2 cursor-pointer select-none font-medium text-muted-foreground hover:text-foreground">
                  <input
                    type="checkbox"
                    checked={rememberMe}
                    onChange={(e) => setRememberMe(e.target.checked)}
                    className="h-4 w-4 rounded accent-[#6D28D9]"
                  />
                  <span>Remember Me</span>
                </label>
              </div>

              {/* Dimensional 3D Pill Primary Button */}
              <button
                type="submit"
                disabled={login.isPending}
                className="pace-3d-cta group relative flex h-13 py-3.5 w-full items-center justify-center rounded-full text-white text-sm font-bold tracking-wide transition-all duration-150 active:scale-[0.985] disabled:opacity-50"
              >
                <span>{login.isPending ? "Signing in..." : "Login"}</span>
                <span className="absolute right-2 flex h-9 w-9 items-center justify-center rounded-full bg-white/20 shadow-inner transition-transform duration-150 group-hover:translate-x-0.5">
                  <ArrowRight className="h-4 w-4" />
                </span>
              </button>
            </form>

            {OFFLINE_STUDIO_ENABLED && (
              <div className="space-y-4 pt-1">
                <div className="relative flex items-center">
                  <div className="flex-grow border-t border-border/80" />
                  <span className="shrink mx-3 text-[10px] font-bold uppercase tracking-[0.16em] text-muted-foreground">
                    Or continue with
                  </span>
                  <div className="flex-grow border-t border-border/80" />
                </div>

                <button
                  type="button"
                  onClick={() => devLogin.mutate()}
                  disabled={devLogin.isPending}
                  className="pace-3d-medallion group flex h-12 w-full items-center justify-center gap-2.5 rounded-2xl text-sm font-bold text-foreground hover:brightness-[0.99] dark:hover:brightness-110 transition-all duration-150 active:scale-[0.985] disabled:opacity-50"
                >
                  <PaceMark className="h-5 w-5" />
                  <span>
                    {devLogin.isPending
                      ? "Opening Studio..."
                      : "Offline Studio"}
                  </span>
                </button>
              </div>
            )}

            <p className="text-center text-[10px] font-semibold uppercase tracking-[0.2em] text-muted-foreground/80 pt-1">
              Script · Sync · Export
            </p>
          </div>
        </section>
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
    <div className="pace-sculpted-canvas min-h-screen flex flex-col">
      <div className="flex items-center justify-between px-5 sm:px-8 h-16 border-b border-border bg-card/80 backdrop-blur-xl">
        <PaceLogo size="md" />
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
        <div className="w-full max-w-md rounded-[28px] border border-border bg-card p-7 sm:p-8 space-y-5 studio-card">
          <div className="space-y-2">
            <div className="inline-flex items-center gap-1.5 rounded-lg bg-amber-500/12 px-2.5 py-1 text-xs font-medium text-amber-700 dark:text-amber-300">
              <KeyRound className="h-3.5 w-3.5" />
              Security Action Required
            </div>
            <h1 className="font-display text-xl font-bold text-foreground">
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
              className="flex items-start gap-2 rounded-xl border border-red-500/30 bg-red-500/10 p-3.5 text-xs text-red-700 dark:text-red-300"
            >
              <AlertCircle className="h-4 w-4 shrink-0 mt-0.5" />
              <span>{errorMsg}</span>
            </div>
          )}

          <form onSubmit={handleSubmit} className="space-y-4">
            <div className="space-y-1.5">
              <label className="text-xs font-semibold text-foreground">
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
              <label className="text-xs font-semibold text-foreground">
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
              <label className="text-xs font-semibold text-foreground">
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
              className="w-full mt-2 rounded-full"
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
