"use client";

import React from "react";
import {
  Activity,
  Crosshair,
  Layers,
  Maximize2,
  RotateCcw,
  Sparkles,
  Zap,
} from "lucide-react";
import type {
  BoundaryTransitionDto,
  MotionEasing,
  MotionIntensity,
  MotionPreset,
  SceneVisualEffect,
  TransitionKind,
  UpdateSceneEffectRequest,
} from "@pace/shared-types";
import type { SectionAssignment } from "@/features/media/media-store";
import {
  MOTION_PRESETS,
  formatPresetLabel,
  normalizeBoundaryTransition,
} from "./preview-engine";

interface SceneEffectsPanelProps {
  sceneIndex: number;
  assignment?: SectionAssignment;
  hasAssignedMedia: boolean;
  resolvedPreset: string;
  resolvedHasShake: boolean;
  isPickingFocal: boolean;
  outCutEditable: boolean;
  outTransition: BoundaryTransitionDto;
  isSavingEffect: boolean;
  onTogglePickFocal: () => void;
  onUpdateEffect: (patch: UpdateSceneEffectRequest) => void;
  onUpdateOutTransition: (transition: BoundaryTransitionDto) => void;
}

const QUICK_CAPCUT_CHIPS: {
  id: "__auto__" | MotionPreset;
  label: string;
  defaultEasing?: MotionEasing;
}[] = [
  { id: "__auto__", label: "Auto" },
  { id: "punch_zoom_in", label: "Zoom 1", defaultEasing: "punch_out" },
  { id: "zoom_snap", label: "Zoom 2", defaultEasing: "punch_out" },
  { id: "pendulum", label: "Pendulum", defaultEasing: "ease_in_out" },
  { id: "wobble", label: "Wobble", defaultEasing: "ease_in_out" },
  { id: "fall_left", label: "Fall L", defaultEasing: "ease_in_out" },
  { id: "fall_right", label: "Fall R", defaultEasing: "ease_in_out" },
  { id: "ken_burns_diagonal_in", label: "Diagonal", defaultEasing: "dynamic_curve" },
  { id: "handheld_drift", label: "Handheld", defaultEasing: "ease_in_out" },
];

const VISUAL_EFFECT_OPTIONS: { id: SceneVisualEffect; label: string }[] = [
  { id: "none", label: "None" },
  { id: "handheld", label: "Handheld" },
  { id: "camera_shake", label: "Camera Shake" },
  { id: "zoom_lens", label: "Zoom Lens" },
];

const INTENSITY_OPTIONS: { id: MotionIntensity; label: string }[] = [
  { id: "subtle", label: "Subtle" },
  { id: "normal", label: "Normal" },
  { id: "strong", label: "Strong" },
];

const EASING_OPTIONS: { id: MotionEasing; label: string; hint: string }[] = [
  { id: "ease_in_out", label: "Smooth", hint: "Cinematic S-curve" },
  { id: "punch_out", label: "Punch Out", hint: "Fast burst, slow glide" },
  { id: "ramp_in", label: "Ramp In", hint: "Accelerate into cut" },
  { id: "dynamic_curve", label: "S-Whip", hint: "Brisk mid-scene snap" },
  { id: "steady", label: "Linear", hint: "Constant speed" },
];

const TRANSITION_OPTIONS: { id: TransitionKind; label: string }[] = [
  { id: "cut", label: "Cut" },
  { id: "crossfade", label: "Dissolve" },
  { id: "shutter_flash", label: "Shutter Flash" },
  { id: "zoom_through", label: "Zoom Thru" },
  { id: "whip_pan_left", label: "Whip Left" },
  { id: "whip_pan_right", label: "Whip Right" },
  { id: "slide_left", label: "Left" },
  { id: "slide_right", label: "Right" },
  { id: "slide_up", label: "Up" },
  { id: "slide_down", label: "Down" },
  { id: "fade_black", label: "Fade Black" },
];

export function SceneEffectsPanel({
  sceneIndex,
  assignment,
  hasAssignedMedia,
  resolvedPreset,
  resolvedHasShake,
  isPickingFocal,
  outCutEditable,
  outTransition,
  isSavingEffect,
  onTogglePickFocal,
  onUpdateEffect,
  onUpdateOutTransition,
}: SceneEffectsPanelProps) {
  const manualPreset = assignment?.effect?.manual_preset ?? null;
  const intensity = (assignment?.effect?.intensity ?? "normal") as MotionIntensity;
  const easing = (assignment?.effect?.easing ?? "ease_in_out") as MotionEasing;
  const manualShake = assignment?.effect?.manual_shake ?? null;
  const visualEffect = (assignment?.effect?.visual_effect ?? "none") as SceneVisualEffect;
  const fitMode = assignment?.fitMode ?? "fill";
  const focalX = assignment?.focalX ?? 0.5;
  const focalY = assignment?.focalY ?? 0.5;
  const isOverridden = Boolean(assignment?.effectOverridden);

  const staticPresets = MOTION_PRESETS.filter((p) => p.category === "static");
  const punchPresets = MOTION_PRESETS.filter((p) => p.category === "punch");
  const tiltPresets = MOTION_PRESETS.filter((p) => p.category === "tilt");
  const cinematicPresets = MOTION_PRESETS.filter((p) => p.category === "cinematic");
  const zoomPresets = MOTION_PRESETS.filter((p) => p.category === "zoom");
  const panPresets = MOTION_PRESETS.filter((p) => p.category === "pan");
  const comboPresets = MOTION_PRESETS.filter((p) => p.category === "combo");

  const applyPresetSelection = (
    presetVal: string,
    suggestedEasing?: MotionEasing
  ) => {
    if (presetVal === "__auto__") {
      onUpdateEffect({ clear_manual_preset: true });
      return;
    }
    const nextPreset = presetVal as MotionPreset;
    const autoEasing: MotionEasing | undefined =
      suggestedEasing ??
      (nextPreset === "punch_zoom_in" ||
      nextPreset === "punch_zoom_out" ||
      nextPreset === "zoom_snap"
        ? "punch_out"
        : nextPreset === "ken_burns_diagonal_in" ||
            nextPreset === "ken_burns_diagonal_out"
          ? "dynamic_curve"
          : undefined);

    if (autoEasing) {
      onUpdateEffect({ manual_preset: nextPreset, easing: autoEasing });
    } else {
      onUpdateEffect({ manual_preset: nextPreset });
    }
  };

  return (
    <div className="space-y-3.5 text-xs">
      {!hasAssignedMedia && (
        <div className="rounded-md border border-amber-500/30 bg-amber-500/10 px-2.5 py-2 text-[11px] text-amber-700 dark:text-amber-300">
          Assign an image to Scene {String(sceneIndex + 1).padStart(2, "0")} in Media to unlock per-scene framing and motion effects.
        </div>
      )}

      {/* Motion Preset */}
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <label
            htmlFor="scene-motion-preset"
            className="flex items-center gap-1 text-[10px] font-mono uppercase tracking-wider text-muted-foreground"
          >
            <Sparkles className="h-3 w-3 text-brand-violet dark:text-brand-lavender" />
            Camera Motion
          </label>

          {isOverridden ? (
            <button
              type="button"
              disabled={!hasAssignedMedia || isSavingEffect}
              onClick={() => onUpdateEffect({ reset_to_auto: true })}
              className="inline-flex items-center gap-1 rounded px-2 py-1 font-mono text-[10px] text-brand-violet dark:text-brand-lavender hover:bg-secondary disabled:opacity-40 transition-colors touch-manipulation"
              title="Clear manual overrides and return to automatic variety"
            >
              <RotateCcw className="h-2.5 w-2.5" />
              Reset Auto
            </button>
          ) : (
            <span className="rounded border border-border bg-secondary px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground">
              Auto
            </span>
          )}
        </div>

        {/* 1-Tap Quick Motion Chips (Mobile & Desktop) */}
        <div className="grid grid-cols-3 gap-1.5">
          {QUICK_CAPCUT_CHIPS.map((chip) => {
            const active =
              chip.id === "__auto__"
                ? manualPreset === null
                : manualPreset === chip.id;
            return (
              <button
                key={chip.id}
                type="button"
                disabled={!hasAssignedMedia || isSavingEffect}
                onClick={() => applyPresetSelection(chip.id, chip.defaultEasing)}
                className={`min-h-[34px] rounded-md border px-2 py-1 text-[11px] font-medium transition-colors disabled:opacity-40 touch-manipulation ${
                  active
                    ? "border-brand-violet bg-brand-violet/15 text-brand-violet dark:border-brand-lavender dark:bg-brand-lavender/20 dark:text-brand-lavender font-semibold"
                    : "border-border bg-card text-muted-foreground hover:text-foreground hover:bg-secondary/50"
                }`}
              >
                {chip.label}
              </button>
            );
          })}
        </div>

        <select
          id="scene-motion-preset"
          disabled={!hasAssignedMedia || isSavingEffect}
          value={manualPreset ?? "__auto__"}
          onChange={(e) => applyPresetSelection(e.target.value)}
          className="w-full min-h-[36px] rounded-md border border-input bg-background px-2.5 py-1.5 text-xs font-medium text-foreground focus:outline-none focus:ring-2 focus:ring-ring disabled:opacity-50"
        >
          <option value="__auto__">
            Auto ({formatPresetLabel(resolvedPreset)})
          </option>
          <optgroup label="Impact / Punch">
            {punchPresets.map((p) => (
              <option key={p.id} value={p.id}>
                {p.label}
              </option>
            ))}
          </optgroup>
          <optgroup label="Combo & Tilt">
            {tiltPresets.map((p) => (
              <option key={p.id} value={p.id}>
                {p.label}
              </option>
            ))}
          </optgroup>
          <optgroup label="Cinematic & Handheld">
            {cinematicPresets.map((p) => (
              <option key={p.id} value={p.id}>
                {p.label}
              </option>
            ))}
          </optgroup>
          <optgroup label="Zoom">
            {zoomPresets.map((p) => (
              <option key={p.id} value={p.id}>
                {p.label}
              </option>
            ))}
          </optgroup>
          <optgroup label="Pan">
            {panPresets.map((p) => (
              <option key={p.id} value={p.id}>
                {p.label}
              </option>
            ))}
          </optgroup>
          <optgroup label="Zoom + Pan">
            {comboPresets.map((p) => (
              <option key={p.id} value={p.id}>
                {p.label}
              </option>
            ))}
          </optgroup>
          <optgroup label="Hold">
            {staticPresets.map((p) => (
              <option key={p.id} value={p.id}>
                {p.label}
              </option>
            ))}
          </optgroup>
        </select>
      </div>

      {/* Strength & Velocity Curve */}
      <div className="grid grid-cols-1 gap-2.5">
        <div className="space-y-1">
          <div className="text-[10px] font-mono uppercase tracking-wider text-muted-foreground">
            Strength
          </div>
          <div className="grid grid-cols-3 gap-1 rounded-md border border-border bg-secondary/40 p-0.5">
            {INTENSITY_OPTIONS.map((opt) => {
              const active = intensity === opt.id;
              return (
                <button
                  key={opt.id}
                  type="button"
                  disabled={!hasAssignedMedia || isSavingEffect}
                  onClick={() => onUpdateEffect({ intensity: opt.id })}
                  className={`min-h-[32px] rounded-[4px] py-1 text-[11px] font-medium transition-colors disabled:opacity-40 touch-manipulation ${
                    active
                      ? "bg-card text-foreground border border-border shadow-xs font-semibold"
                      : "text-muted-foreground hover:text-foreground border border-transparent"
                  }`}
                >
                  {opt.label}
                </button>
              );
            })}
          </div>
        </div>

        <div className="space-y-1">
          <div className="flex items-center justify-between text-[10px] font-mono uppercase tracking-wider text-muted-foreground">
            <span className="flex items-center gap-1">
              <Zap className="h-3 w-3" />
              Velocity Curve
            </span>
            <span className="text-[10px] normal-case font-sans text-muted-foreground">
              {EASING_OPTIONS.find((o) => o.id === easing)?.hint}
            </span>
          </div>
          <div className="grid grid-cols-3 gap-1 rounded-md border border-border bg-secondary/40 p-1">
            {EASING_OPTIONS.map((opt) => {
              const active = easing === opt.id;
              return (
                <button
                  key={opt.id}
                  type="button"
                  disabled={!hasAssignedMedia || isSavingEffect}
                  onClick={() => onUpdateEffect({ easing: opt.id })}
                  title={opt.hint}
                  className={`min-h-[30px] rounded-[4px] px-1.5 py-1 text-[11px] font-medium transition-colors disabled:opacity-40 touch-manipulation ${
                    active
                      ? "bg-card text-foreground border border-border shadow-xs font-semibold"
                      : "text-muted-foreground hover:text-foreground border border-transparent"
                  }`}
                >
                  {opt.label}
                </button>
              );
            })}
          </div>
        </div>

        {/* Visual Effects (Camera Shake, Zoom Lens, Handheld) */}
        <div className="space-y-1">
          <div className="flex items-center justify-between text-[10px] font-mono uppercase tracking-wider text-muted-foreground">
            <span className="flex items-center gap-1">
              <Sparkles className="h-3 w-3" />
              Visual Effect
            </span>
            <span className="text-[10px]">
              {VISUAL_EFFECT_OPTIONS.find((v) => v.id === visualEffect)?.label ?? "None"}
            </span>
          </div>
          <div className="grid grid-cols-2 gap-1 rounded-md border border-border bg-secondary/40 p-1">
            {VISUAL_EFFECT_OPTIONS.map((opt) => {
              const active = visualEffect === opt.id;
              return (
                <button
                  key={opt.id}
                  type="button"
                  disabled={!hasAssignedMedia || isSavingEffect}
                  onClick={() => onUpdateEffect({ visual_effect: opt.id })}
                  className={`min-h-[30px] rounded-[4px] px-1.5 py-1 text-[11px] font-medium transition-colors disabled:opacity-40 touch-manipulation ${
                    active
                      ? "bg-card text-foreground border border-border shadow-xs font-semibold"
                      : "text-muted-foreground hover:text-foreground border border-transparent"
                  }`}
                >
                  {opt.label}
                </button>
              );
            })}
          </div>
        </div>

        {/* Camera Shake */}
        <div className="space-y-1">
          <div className="flex items-center justify-between text-[10px] font-mono uppercase tracking-wider text-muted-foreground">
            <span className="flex items-center gap-1">
              <Activity className="h-3 w-3" />
              Handheld Shake
            </span>
            <span className="text-[10px]">
              {resolvedHasShake ? "Active" : "Off"}
            </span>
          </div>
          <div className="grid grid-cols-3 gap-1 rounded-md border border-border bg-secondary/40 p-0.5">
            {[
              { id: "auto", label: "Auto" },
              { id: "on", label: "On" },
              { id: "off", label: "Off" },
            ].map((opt) => {
              const active =
                (opt.id === "auto" && manualShake === null) ||
                (opt.id === "on" && manualShake === true) ||
                (opt.id === "off" && manualShake === false);
              return (
                <button
                  key={opt.id}
                  type="button"
                  disabled={!hasAssignedMedia || isSavingEffect}
                  onClick={() => {
                    if (opt.id === "auto") {
                      onUpdateEffect({ clear_manual_shake: true });
                    } else {
                      onUpdateEffect({ manual_shake: opt.id === "on" });
                    }
                  }}
                  className={`min-h-[32px] rounded-[4px] py-1 text-[11px] font-medium transition-colors disabled:opacity-40 touch-manipulation ${
                    active
                      ? "bg-card text-foreground border border-border shadow-xs font-semibold"
                      : "text-muted-foreground hover:text-foreground border border-transparent"
                  }`}
                >
                  {opt.label}
                </button>
              );
            })}
          </div>
        </div>
      </div>

      {/* Framing & Focal Point */}
      <div className="rounded-md border border-border bg-secondary/25 p-2.5 space-y-2.5">
        <div className="flex items-center justify-between">
          <span className="flex items-center gap-1 text-[10px] font-mono uppercase tracking-wider text-muted-foreground">
            <Maximize2 className="h-3 w-3" />
            Framing
          </span>
          <div className="flex items-center gap-1 rounded-md border border-border bg-card p-0.5">
            {(["fill", "fit_blur"] as const).map((mode) => (
              <button
                key={mode}
                type="button"
                disabled={!hasAssignedMedia || isSavingEffect}
                onClick={() => onUpdateEffect({ fit_mode: mode })}
                className={`min-h-[28px] rounded-[4px] px-2.5 py-0.5 text-[11px] font-medium transition-colors disabled:opacity-40 touch-manipulation ${
                  fitMode === mode
                    ? "bg-brand-lavender text-black font-semibold"
                    : "text-muted-foreground hover:text-foreground"
                }`}
              >
                {mode === "fill" ? "Fill" : "Fit Blur"}
              </button>
            ))}
          </div>
        </div>

        <div className="flex items-center justify-between gap-2 pt-1 border-t border-border/60">
          <div>
            <div className="text-[10px] font-mono uppercase text-muted-foreground">
              Focal Point
            </div>
            <div className="font-mono text-[11px] font-semibold text-foreground">
              {Math.round(focalX * 100)}%, {Math.round(focalY * 100)}%
            </div>
          </div>

          <div className="flex items-center gap-1">
            <button
              type="button"
              disabled={!hasAssignedMedia}
              onClick={onTogglePickFocal}
              className={`min-h-[30px] inline-flex items-center gap-1 rounded-md border px-2.5 py-1 text-[11px] font-medium transition-colors disabled:opacity-40 touch-manipulation ${
                isPickingFocal
                  ? "border-brand-lavender bg-brand-lavender text-[#140B29] font-semibold"
                  : "border-border bg-card text-foreground hover:bg-secondary"
              }`}
            >
              <Crosshair className="h-3 w-3" />
              {isPickingFocal ? "Tap Preview..." : "Pick"}
            </button>
            <button
              type="button"
              disabled={
                !hasAssignedMedia ||
                isSavingEffect ||
                (Math.abs(focalX - 0.5) < 0.005 && Math.abs(focalY - 0.5) < 0.005)
              }
              onClick={() => onUpdateEffect({ focal_x: 0.5, focal_y: 0.5 })}
              className="min-h-[30px] rounded-md border border-border bg-card px-2.5 py-1 text-[11px] text-muted-foreground hover:text-foreground disabled:opacity-40 transition-colors touch-manipulation"
            >
              Center
            </button>
          </div>
        </div>
      </div>

      {/* Out-Cut Transition */}
      {outCutEditable && (
        <div className="rounded-md border border-border bg-secondary/25 p-2.5 space-y-2">
          <div className="flex items-center justify-between">
            <span className="flex items-center gap-1 text-[10px] font-mono uppercase tracking-wider text-muted-foreground">
              <Layers className="h-3 w-3" />
              Out Transition
            </span>
            <span className="font-mono text-[10px] text-muted-foreground">
              Cut #{sceneIndex + 1}
            </span>
          </div>

          <div className="grid grid-cols-3 gap-1 rounded-md border border-border bg-card p-1">
            {TRANSITION_OPTIONS.map((opt) => {
              const active = outTransition.kind === opt.id;
              return (
                <button
                  key={opt.id}
                  type="button"
                  onClick={() =>
                    onUpdateOutTransition(
                      normalizeBoundaryTransition(
                        opt.id,
                        outTransition.duration_ms || 0
                      )
                    )
                  }
                  className={`min-h-[30px] rounded-[4px] px-1.5 py-1 text-[11px] font-medium transition-colors touch-manipulation ${
                    active
                      ? "bg-brand-violet text-white dark:bg-brand-lavender dark:text-zinc-950 font-semibold"
                      : "text-muted-foreground hover:text-foreground hover:bg-secondary/50"
                  }`}
                >
                  {opt.label}
                </button>
              );
            })}
          </div>

          {outTransition.kind !== "cut" && (
            <div className="space-y-1.5 pt-1">
              <div className="flex items-center justify-between font-mono text-[10px] text-muted-foreground">
                <span>Duration</span>
                <span className="font-semibold text-foreground">
                  {outTransition.duration_ms} ms
                </span>
              </div>
              <input
                type="range"
                min={100}
                max={3000}
                step={50}
                value={outTransition.duration_ms}
                onChange={(e) =>
                  onUpdateOutTransition(
                    normalizeBoundaryTransition(
                      outTransition.kind,
                      Number(e.target.value)
                    )
                  )
                }
                className="w-full accent-brand-lavender cursor-pointer"
                aria-label="Transition duration in milliseconds"
              />
              <div className="grid grid-cols-4 gap-1">
                {[300, 450, 700, 1100].map((ms) => (
                  <button
                    key={ms}
                    type="button"
                    onClick={() =>
                      onUpdateOutTransition(
                        normalizeBoundaryTransition(outTransition.kind, ms)
                      )
                    }
                    className={`min-h-[28px] rounded border py-0.5 font-mono text-[10px] transition-colors touch-manipulation ${
                      outTransition.duration_ms === ms
                        ? "border-brand-lavender bg-brand-lavender/15 text-brand-violet dark:text-brand-lavender font-semibold"
                        : "border-border bg-card text-muted-foreground hover:text-foreground"
                    }`}
                  >
                    {(ms / 1000).toFixed(2).replace(/0$/, "")}s
                  </button>
                ))}
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
