"use client";

import React, { useState, useEffect, useMemo, useRef } from "react";
import {
  DndContext,
  closestCenter,
  KeyboardSensor,
  PointerSensor,
  useSensor,
  useSensors,
  DragEndEvent,
} from "@dnd-kit/core";
import {
  SortableContext,
  sortableKeyboardCoordinates,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { useScriptEditorStore } from "@/stores/script-editor-store";
import { api } from "@/lib/api-client";
import { SectionCard } from "./section-card";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog";
import { formatDuration } from "@/lib/utils";
import {
  Undo2,
  Redo2,
  Plus,
  FileUp,
  ArrowRight,
  Check,
  AlertCircle,
  Loader2,
  Clock,
  FileText,
  Layers,
} from "lucide-react";

interface ScriptEditorViewProps {
  projectId: string;
  onNextStep: () => void;
}

const UUID_REGEX =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

export function ScriptEditorView({
  projectId,
  onNextStep,
}: ScriptEditorViewProps) {
  const {
    sections,
    saveStatus,
    setSections,
    loadRawScript,
    updateSectionText,
    splitSection,
    splitSectionByParagraphs,
    mergeWithNext,
    deleteSection,
    reorderSections,
    setSaveStatus,
    applySavedSections,
    getFullScriptText,
  } = useScriptEditorStore();

  const [showImport, setShowImport] = useState(false);
  const [importText, setImportText] = useState("");
  const [historyVersion, setHistoryVersion] = useState(0);

  const saveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const sensors = useSensors(
    useSensor(PointerSensor, {
      activationConstraint: { distance: 6 },
    }),
    useSensor(KeyboardSensor, {
      coordinateGetter: sortableKeyboardCoordinates,
    })
  );

  // Debounced auto-save when script changes
  useEffect(() => {
    if (saveStatus !== "unsaved") return;

    if (saveTimerRef.current) {
      clearTimeout(saveTimerRef.current);
    }

    const snapshot = sections;
    saveTimerRef.current = setTimeout(async () => {
      setSaveStatus("saving");
      try {
        const res = await api.scripts.save(projectId, {
          raw_text: getFullScriptText(),
          sections: snapshot.map((s) => ({
            id: UUID_REGEX.test(s.id) ? s.id : null,
            text: s.text,
          })),
        });
        applySavedSections(projectId, res.sections, snapshot);
      } catch {
        setSaveStatus("error");
      }
    }, 900);

    return () => {
      if (saveTimerRef.current) {
        clearTimeout(saveTimerRef.current);
      }
    };
  }, [
    sections,
    saveStatus,
    projectId,
    setSaveStatus,
    applySavedSections,
    getFullScriptText,
  ]);

  const temporal = useScriptEditorStore.temporal;
  const pastStates = temporal.getState().pastStates;
  const futureStates = temporal.getState().futureStates;

  const handleUndo = () => {
    temporal.getState().undo();
    setHistoryVersion((v) => v + 1);
  };

  const handleRedo = () => {
    temporal.getState().redo();
    setHistoryVersion((v) => v + 1);
  };

  // Trigger render check when undo/redo version increments
  void historyVersion;

  const handleAddScene = () => {
    const next = [
      ...sections,
      {
        id: `section-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
        text: "",
        wordCount: 0,
        estimatedDurationSecs: 0,
      },
    ];
    setSections(next);
  };

  const handleImportSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!importText.trim()) return;
    loadRawScript(importText);
    setImportText("");
    setShowImport(false);
  };

  const handleDragEnd = (event: DragEndEvent) => {
    const { active, over } = event;
    if (over && active.id !== over.id) {
      reorderSections(String(active.id), String(over.id));
    }
  };

  const metrics = useMemo(() => {
    const totalWords = sections.reduce((acc, s) => acc + s.wordCount, 0);
    const totalSeconds = sections.reduce(
      (acc, s) => acc + s.estimatedDurationSecs,
      0
    );
    const activeCount = sections.filter((s) => s.text.trim().length > 0).length;
    return { totalWords, totalSeconds, activeCount };
  }, [sections]);

  return (
    <div className="mx-auto w-full max-w-4xl px-4 sm:px-6 py-6 sm:py-8 space-y-5">
      {/* Top Control & Telemetry Strip */}
      <div className="flex flex-col gap-3 rounded-2xl border border-border/65 bg-card p-3.5 sm:px-5 sm:py-3.5 studio-card">
        <div className="flex flex-wrap items-center justify-between gap-2.5">
          {/* Metrics */}
          <div className="flex flex-wrap items-center gap-2.5 sm:gap-4 text-xs">
            <div className="flex items-center gap-1.5 font-mono tabular-nums text-foreground font-semibold">
              <Layers className="h-3.5 w-3.5 text-brand-violet dark:text-brand-lavender" />
              <span>{sections.length} Scenes</span>
            </div>
            <span className="h-3.5 w-px bg-border/70" />
            <div className="flex items-center gap-1.5 font-mono tabular-nums text-muted-foreground">
              <FileText className="h-3.5 w-3.5 opacity-75" />
              <span>{metrics.totalWords} words</span>
            </div>
            <span className="h-3.5 w-px bg-border/70" />
            <div className="flex items-center gap-1.5 font-mono tabular-nums text-muted-foreground">
              <Clock className="h-3.5 w-3.5 opacity-75" />
              <span>~{formatDuration(metrics.totalSeconds)}</span>
            </div>

            {/* Save State */}
            <div className="flex items-center gap-1.5 pl-1 text-xs font-medium">
              {saveStatus === "saving" && (
                <span className="inline-flex items-center gap-1 text-muted-foreground">
                  <Loader2 className="h-3 w-3 animate-spin" />
                  Saving
                </span>
              )}
              {saveStatus === "saved" && (
                <span className="inline-flex items-center gap-1 text-emerald-600 dark:text-emerald-400">
                  <Check className="h-3 w-3" />
                  Saved
                </span>
              )}
              {saveStatus === "unsaved" && (
                <span className="text-amber-600 dark:text-amber-400">
                  Unsaved
                </span>
              )}
              {saveStatus === "error" && (
                <span className="inline-flex items-center gap-1 text-red-500">
                  <AlertCircle className="h-3 w-3" />
                  Offline / Local
                </span>
              )}
            </div>
          </div>

          {/* Actions */}
          <div className="flex items-center gap-1.5 w-full sm:w-auto justify-between sm:justify-end">
            <div className="flex items-center gap-1">
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={handleUndo}
                disabled={pastStates.length === 0}
                title="Undo"
                aria-label="Undo"
                className="px-2"
              >
                <Undo2 className="h-3.5 w-3.5" />
              </Button>
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={handleRedo}
                disabled={futureStates.length === 0}
                title="Redo"
                aria-label="Redo"
                className="px-2"
              >
                <Redo2 className="h-3.5 w-3.5" />
              </Button>
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() => setShowImport(true)}
              >
                <FileUp className="h-3.5 w-3.5" />
                <span>Import</span>
              </Button>
            </div>

            <Button
              type="button"
              size="sm"
              onClick={onNextStep}
              disabled={metrics.activeCount === 0}
            >
              <span>Media</span>
              <ArrowRight className="h-3.5 w-3.5" />
            </Button>
          </div>
        </div>
      </div>

      {/* Sortable Scene Cards */}
      <DndContext
        sensors={sensors}
        collisionDetection={closestCenter}
        onDragEnd={handleDragEnd}
      >
        <SortableContext
          items={sections.map((s) => s.id)}
          strategy={verticalListSortingStrategy}
        >
          <div className="space-y-3">
            {sections.map((section, idx) => (
              <SectionCard
                key={section.id}
                section={section}
                index={idx}
                isLast={idx === sections.length - 1}
                onUpdateText={updateSectionText}
                onSplit={splitSection}
                onSplitParagraphs={splitSectionByParagraphs}
                onMergeNext={mergeWithNext}
                onDelete={deleteSection}
              />
            ))}
          </div>
        </SortableContext>
      </DndContext>

      {/* Footer Add Scene */}
      <div className="flex flex-col sm:flex-row items-stretch sm:items-center justify-between gap-2 pt-1">
        <Button
          type="button"
          variant="outline"
          onClick={handleAddScene}
          className="border-dashed"
        >
          <Plus className="h-4 w-4" />
          Add Scene
        </Button>

        <Button
          type="button"
          onClick={onNextStep}
          disabled={metrics.activeCount === 0}
          className="sm:hidden"
        >
          Continue to Media
          <ArrowRight className="h-4 w-4" />
        </Button>
      </div>

      {/* Import Raw Script Dialog */}
      <Dialog open={showImport} onOpenChange={setShowImport}>
        <DialogContent onClose={() => setShowImport(false)}>
          <DialogHeader>
            <DialogTitle>Import Full Script</DialogTitle>
            <DialogDescription>
              Paste your complete script below. Blank lines automatically separate
              paragraphs into scenes.
            </DialogDescription>
          </DialogHeader>

          <form onSubmit={handleImportSubmit} className="space-y-4">
            <Textarea
              value={importText}
              onChange={(e) => setImportText(e.target.value)}
              placeholder={"Opening narration...\n\nSecond scene narration..."}
              rows={9}
              className="font-sans text-sm"
              autoFocus
            />

            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                onClick={() => setShowImport(false)}
              >
                Cancel
              </Button>
              <Button type="submit" disabled={!importText.trim()}>
                Split into Scenes
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </div>
  );
}
