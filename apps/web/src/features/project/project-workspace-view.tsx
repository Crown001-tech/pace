"use client";

import React, { useState, useEffect } from "react";
import { useProject } from "@/hooks/use-projects";
import { api } from "@/lib/api-client";
import { useScriptEditorStore } from "@/stores/script-editor-store";
import { ScriptEditorView } from "@/features/script-editor/script-editor-view";
import { StepMediaView } from "@/features/media/step-media-view";
import { StepNarrationView } from "@/features/narration/step-narration-view";
import { StepTimelineView } from "@/features/timeline/step-timeline-view";
import { ExportDialog } from "@/features/exports/export-dialog";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import {
  ArrowLeft,
  Download,
  FileText,
  Image as ImageIcon,
  Mic,
  SlidersHorizontal,
} from "lucide-react";

export type WorkspaceStep = "script" | "media" | "narration" | "timeline";

interface ProjectWorkspaceViewProps {
  projectId: string;
  onBack: () => void;
}

const STEPS: {
  id: WorkspaceStep;
  label: string;
  shortLabel: string;
  num: string;
  icon: React.ElementType;
}[] = [
  { id: "script", label: "Script", shortLabel: "Script", num: "01", icon: FileText },
  { id: "media", label: "Media", shortLabel: "Media", num: "02", icon: ImageIcon },
  { id: "narration", label: "Narration", shortLabel: "Audio", num: "03", icon: Mic },
  { id: "timeline", label: "Studio", shortLabel: "Studio", num: "04", icon: SlidersHorizontal },
];

const UUID_REGEX =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

export function ProjectWorkspaceView({
  projectId,
  onBack,
}: ProjectWorkspaceViewProps) {
  const { data: project, isLoading, isError } = useProject(projectId);
  const [activeStep, setActiveStep] = useState<WorkspaceStep>("script");
  const [exportOpen, setExportOpen] = useState(false);

  const hydrateFromServer = useScriptEditorStore((s) => s.hydrateFromServer);
  const resetForProject = useScriptEditorStore((s) => s.resetForProject);
  const applySavedSections = useScriptEditorStore((s) => s.applySavedSections);
  const saveStatus = useScriptEditorStore((s) => s.saveStatus);
  const setSaveStatus = useScriptEditorStore((s) => s.setSaveStatus);
  const sections = useScriptEditorStore((s) => s.sections);
  const getFullScriptText = useScriptEditorStore((s) => s.getFullScriptText);

  const flushScriptIfDirty = async () => {
    if (saveStatus !== "unsaved") return;
    const snapshot = sections;
    setSaveStatus("saving");
    try {
      const saved = await api.scripts.save(projectId, {
        raw_text: getFullScriptText(),
        sections: snapshot.map((sec) => ({
          id: UUID_REGEX.test(sec.id) ? sec.id : null,
          text: sec.text,
        })),
      });
      applySavedSections(projectId, saved.sections, snapshot);
    } catch (err) {
      console.error("Failed to flush unsaved script:", err);
      setSaveStatus("error");
    }
  };

  const handleStepChange = async (step: WorkspaceStep) => {
    if (activeStep === "script" && step !== "script") {
      await flushScriptIfDirty();
    }
    setActiveStep(step);
  };

  const handleBack = async () => {
    if (activeStep === "script") {
      await flushScriptIfDirty();
    }
    onBack();
  };

  const handleOpenExport = async () => {
    if (activeStep === "script") {
      await flushScriptIfDirty();
    }
    setExportOpen(true);
  };

  useEffect(() => {
    let active = true;
    resetForProject(projectId);

    api.scripts
      .get(projectId)
      .then((res) => {
        if (!active) return;
        const normalized = [...res.sections].sort((a, b) => a.position - b.position);
        hydrateFromServer(projectId, normalized);
      })
      .catch((err) => {
        console.warn("Could not load backend script, initializing empty state:", err);
        if (active) hydrateFromServer(projectId, []);
      });

    return () => {
      active = false;
    };
  }, [projectId, hydrateFromServer, resetForProject]);

  // Evict stale or invalid project ID if backend reports 404 or missing
  useEffect(() => {
    if (!isLoading && !project && isError) {
      try {
        localStorage.removeItem("pace_active_project_id");
      } catch {}
      onBack();
    }
  }, [isLoading, project, isError, onBack]);

  if (isLoading) {
    return (
      <div className="flex h-[calc(100vh-3.5rem)] items-center justify-center bg-background">
        <div className="flex flex-col items-center gap-3">
          <div className="h-1.5 w-24 overflow-hidden rounded-full bg-secondary">
            <div className="h-full w-1/2 rounded-full bg-brand-violet dark:bg-brand-lavender animate-pulse" />
          </div>
          <span className="text-xs font-medium text-muted-foreground">
            Opening project
          </span>
        </div>
      </div>
    );
  }

  if (!project) {
    return (
      <div className="flex flex-col items-center justify-center py-24 px-4 text-center">
        <p className="text-sm text-muted-foreground mb-4">Project could not be found or has been removed.</p>
        <Button
          variant="outline"
          onClick={() => {
            try {
              localStorage.removeItem("pace_active_project_id");
            } catch {}
            onBack();
          }}
        >
          Back to Projects
        </Button>
      </div>
    );
  }

  const projectAspect: "16:9" | "9:16" =
    project.aspect_ratio === "9:16" ? "9:16" : "16:9";

  return (
    <div className="flex flex-col flex-1 min-h-[calc(100vh-3rem)] bg-background">
      {/* Studio Workspace Sub-Bar */}
      <div className="sticky top-14 z-30 border-b border-border/60 bg-background/85 backdrop-blur-xl">
        <div className="mx-auto flex h-12 max-w-[1600px] items-center justify-between gap-3 px-4 sm:px-6">
          {/* Left: Back & Project Meta */}
          <div className="flex items-center gap-2.5 min-w-0">
            <Button
              variant="ghost"
              size="sm"
              onClick={handleBack}
              className="h-8 px-2.5 text-muted-foreground hover:text-foreground shrink-0"
              aria-label="Back to projects"
            >
              <ArrowLeft className="h-4 w-4" />
              <span className="hidden sm:inline text-xs font-medium">Projects</span>
            </Button>

            <span className="h-4 w-px bg-border/70 shrink-0" />

            <h2 className="truncate text-sm font-semibold text-foreground max-w-[180px] sm:max-w-[280px]">
              {project.name}
            </h2>

            <Badge variant="secondary" className="shrink-0 font-mono">
              {projectAspect}
            </Badge>
            <span className="hidden lg:inline text-xs font-mono text-muted-foreground shrink-0">
              {project.fps} FPS
            </span>
          </div>

          {/* Desktop Center: Segmented Workflow Tabs */}
          <nav
            aria-label="Project workflow"
            className="hidden md:flex items-center rounded-[12px] bg-secondary/75 p-1 gap-1"
          >
            {STEPS.map((step) => {
              const Icon = step.icon;
              const isActive = activeStep === step.id;
              return (
                <button
                  key={step.id}
                  type="button"
                  onClick={() => handleStepChange(step.id)}
                  className={`flex items-center gap-1.5 rounded-[8px] px-3.5 py-1.5 text-xs font-medium transition-all duration-150 ${
                    isActive
                      ? "bg-card text-foreground shadow-sm font-semibold"
                      : "text-muted-foreground hover:text-foreground"
                  }`}
                >
                  <Icon
                    className={`h-3.5 w-3.5 ${
                      isActive ? "text-brand-violet dark:text-brand-lavender" : "text-muted-foreground"
                    }`}
                  />
                  <span>{step.label}</span>
                </button>
              );
            })}
          </nav>

          {/* Export CTA */}
          <Button
            type="button"
            size="sm"
            onClick={() => void handleOpenExport()}
            className="h-8 gap-1.5 rounded-[8px] px-3.5 text-xs font-semibold shrink-0 shadow-none"
          >
            <Download className="h-3.5 w-3.5" />
            <span>Export</span>
          </Button>
        </div>
      </div>

      <ExportDialog
        open={exportOpen}
        onOpenChange={setExportOpen}
        projectId={project.id}
        projectName={project.name}
        aspectRatio={projectAspect}
        fps={project.fps}
        durationMs={Math.max(15_000, sections.length * 12_000)}
      />

      {/* Main Step Viewport */}
      <div
        className={`flex-1 flex flex-col min-w-0 ${
          activeStep === "timeline" ? "pb-16 md:pb-0" : "pb-20 md:pb-6"
        }`}
      >
        {activeStep === "script" && (
          <ScriptEditorView
            projectId={project.id}
            onNextStep={() => handleStepChange("media")}
          />
        )}
        {activeStep === "media" && (
          <StepMediaView
            projectId={project.id}
            projectAspectRatio={projectAspect}
            onNextStep={() => handleStepChange("narration")}
          />
        )}
        {activeStep === "narration" && (
          <StepNarrationView
            projectId={project.id}
            projectAspectRatio={projectAspect}
          />
        )}
        {activeStep === "timeline" && (
          <StepTimelineView
            projectId={project.id}
            projectName={project.name}
            projectAspectRatio={projectAspect}
            onOpenExport={() => void handleOpenExport()}
            onChangeStep={(step) => void handleStepChange(step)}
            onExitProject={() => void handleBack()}
          />
        )}
      </div>

      {/* Mobile Native-App Bottom Navigation Dock */}
      <nav
        aria-label="Mobile studio navigation"
        className="fixed bottom-0 inset-x-0 z-40 border-t border-border bg-card/95 backdrop-blur-md pb-safe md:hidden"
      >
        <div className="grid grid-cols-4 h-14">
          {STEPS.map((step) => {
            const Icon = step.icon;
            const isActive = activeStep === step.id;
            return (
              <button
                key={step.id}
                type="button"
                onClick={() => handleStepChange(step.id)}
                className={`relative flex flex-col items-center justify-center gap-0.5 touch-manipulation transition-colors ${
                  isActive
                    ? "text-brand-violet dark:text-brand-lavender"
                    : "text-muted-foreground hover:text-foreground"
                }`}
              >
                {isActive && (
                  <span className="absolute top-0 inset-x-4 h-0.5 bg-brand-violet dark:bg-brand-lavender" />
                )}
                <Icon className="h-4 w-4" />
                <span className="text-[10px] font-medium tracking-tight">
                  {step.shortLabel}
                </span>
              </button>
            );
          })}
        </div>
      </nav>
    </div>
  );
}
