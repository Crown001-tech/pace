"use client";

import React, { useState } from "react";
import { Loader2, Search, Sparkles, Wand2, Plus, Check } from "lucide-react";
import type { MediaResponse, StockMediaItem } from "@pace/shared-types";
import { api } from "@/lib/api-client";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import type { MediaItem, SectionRef } from "./media-store";

interface MediaSourcesPanelProps {
  mode: "stock" | "ai";
  projectId: string;
  projectAspectRatio: "16:9" | "9:16";
  targetSectionId: string | null;
  emptySections: SectionRef[];
  onMediaImported: (media: MediaResponse, assignedSectionId?: string) => void;
  onBatchComplete: () => Promise<void>;
}

const AI_STYLES = [
  { id: "cinematic documentary photography, rich lighting, ultra detailed", label: "Documentary" },
  { id: "35mm editorial film photography, natural depth of field", label: "35mm Film" },
  { id: "moody studio macro photography, dramatic rim light", label: "Macro Studio" },
  { id: "minimalist architectural digital art, crisp clean composition", label: "Clean Art" },
];

export function toStoreMediaItem(
  m: MediaResponse,
  fallbackAspect: "16:9" | "9:16"
): MediaItem {
  const w = m.width ?? (fallbackAspect === "9:16" ? 9 : 16);
  const h = m.height ?? (fallbackAspect === "9:16" ? 16 : 9);
  return {
    id: m.id,
    filename: m.filename,
    bytes: m.bytes,
    sha256: m.sha256,
    mime: m.mime,
    aspectRatio: w >= h ? "16:9" : "9:16",
    width: m.width ?? undefined,
    height: m.height ?? undefined,
    storageKey: "",
    thumbUrl: m.thumb_url || m.download_url,
    previewUrl: m.preview_url || m.download_url,
  };
}

export function MediaSourcesPanel({
  mode,
  projectId,
  projectAspectRatio,
  targetSectionId,
  emptySections,
  onMediaImported,
  onBatchComplete,
}: MediaSourcesPanelProps) {
  // Stock state
  const [stockQuery, setStockQuery] = useState("");
  const [stockProvider, setStockProvider] = useState<string | null>(null);
  const [stockResults, setStockResults] = useState<StockMediaItem[]>([]);
  const [isSearchingStock, setIsSearchingStock] = useState(false);
  const [importingStockId, setImportingStockId] = useState<string | null>(null);
  const [importedIds, setImportedIds] = useState<Record<string, boolean>>({});
  const [isAutoMatching, setIsAutoMatching] = useState(false);

  // AI Gen state
  const [aiPrompt, setAiPrompt] = useState("");
  const [aiStyle, setAiStyle] = useState(AI_STYLES[0].id);
  const [isGeneratingAi, setIsGeneratingAi] = useState(false);
  const [batchAiStatus, setBatchAiStatus] = useState<string | null>(null);

  const [statusMessage, setStatusMessage] = useState<string | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  const handleStockSearch = async (e?: React.FormEvent) => {
    if (e) e.preventDefault();
    const q = stockQuery.trim();
    if (!q) return;
    setIsSearchingStock(true);
    setErrorMessage(null);
    setStatusMessage(null);
    try {
      const res = await api.media.searchStock(projectId, q, 12);
      setStockProvider(res.provider);
      setStockResults(res.items);
      if (res.items.length === 0) {
        setStatusMessage(`No stock photos found for "${q}". Try broader keywords.`);
      }
    } catch (err) {
      setErrorMessage(err instanceof Error ? err.message : "Stock search failed");
    } finally {
      setIsSearchingStock(false);
    }
  };

  const handleImportStockItem = async (item: StockMediaItem) => {
    setImportingStockId(item.id);
    setErrorMessage(null);
    try {
      const saved = await api.media.importStock(projectId, {
        image_url: item.image_url,
        title: item.title,
        provider: item.provider,
        section_id: targetSectionId ?? undefined,
      });
      setImportedIds((prev) => ({ ...prev, [item.id]: true }));
      onMediaImported(saved, targetSectionId ?? undefined);
      setStatusMessage(
        targetSectionId
          ? `Imported & assigned "${saved.filename}"`
          : `Added "${saved.filename}" to Asset Bin`
      );
    } catch (err) {
      setErrorMessage(err instanceof Error ? err.message : "Failed importing stock image");
    } finally {
      setImportingStockId(null);
    }
  };

  const handleAutoMatchAllEmpty = async () => {
    if (emptySections.length === 0) return;
    setIsAutoMatching(true);
    setErrorMessage(null);
    setStatusMessage(`Matching ${emptySections.length} empty scene(s) with free stock...`);
    try {
      const res = await api.media.autoMatchStock(projectId, {
        section_ids: emptySections.map((s) => s.id),
      });
      for (const m of res.matched) {
        onMediaImported(m.media, m.section_id);
      }
      await onBatchComplete();
      setStatusMessage(
        `Auto-matched ${res.matched_count} scene(s)${
          res.skipped_count > 0 ? ` (${res.skipped_count} skipped)` : ""
        }.`
      );
    } catch (err) {
      setErrorMessage(err instanceof Error ? err.message : "Auto-match failed");
    } finally {
      setIsAutoMatching(false);
    }
  };

  const handleGenerateSingleAi = async (e: React.FormEvent) => {
    e.preventDefault();
    const prompt = aiPrompt.trim();
    if (!prompt) return;
    setIsGeneratingAi(true);
    setErrorMessage(null);
    setStatusMessage("Generating AI image on server...");
    try {
      const saved = await api.media.generateAi(projectId, {
        prompt,
        style: aiStyle,
        section_id: targetSectionId ?? undefined,
      });
      onMediaImported(saved, targetSectionId ?? undefined);
      setStatusMessage(
        targetSectionId
          ? `Generated & assigned "${saved.filename}"`
          : `Generated "${saved.filename}" into Asset Bin`
      );
      setAiPrompt("");
    } catch (err) {
      setStatusMessage(null);
      setErrorMessage(err instanceof Error ? err.message : "AI generation failed");
    } finally {
      setIsGeneratingAi(false);
    }
  };

  const handleGenerateAllEmpty = async () => {
    if (emptySections.length === 0) return;
    setIsGeneratingAi(true);
    setErrorMessage(null);
    setStatusMessage(null);
    let generated = 0;
    try {
      for (let i = 0; i < emptySections.length; i++) {
        const sec = emptySections[i];
        setBatchAiStatus(`Generating Scene ${sec.position + 1} (${i + 1}/${emptySections.length})...`);
        const snippet = sec.text.trim().slice(0, 220);
        const saved = await api.media.generateAi(projectId, {
          prompt: snippet,
          style: aiStyle,
          section_id: sec.id,
        });
        onMediaImported(saved, sec.id);
        generated++;
      }
      await onBatchComplete();
      setStatusMessage(`Generated and assigned ${generated} AI scene image(s).`);
    } catch (err) {
      setErrorMessage(
        err instanceof Error
          ? `${err.message} (${generated} completed before error)`
          : "Batch AI generation failed"
      );
    } finally {
      setBatchAiStatus(null);
      setIsGeneratingAi(false);
    }
  };

  return (
    <div className="space-y-3">
      {errorMessage && (
        <div className="rounded-md border border-red-500/35 bg-red-500/10 px-3 py-2 text-xs text-red-600 dark:text-red-300">
          {errorMessage}
        </div>
      )}
      {statusMessage && (
        <div className="rounded-md border border-emerald-500/35 bg-emerald-500/10 px-3 py-2 text-xs text-emerald-700 dark:text-emerald-300">
          {statusMessage}
        </div>
      )}

      {mode === "stock" ? (
        <div className="space-y-3">
          <div className="flex flex-col gap-2">
            <Button
              type="button"
              variant="secondary"
              size="sm"
              disabled={isAutoMatching || emptySections.length === 0}
              onClick={handleAutoMatchAllEmpty}
              className="w-full min-h-[40px] touch-manipulation"
            >
              {isAutoMatching ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <Wand2 className="h-3.5 w-3.5" />
              )}
              <span>
                {isAutoMatching
                  ? "Matching Empty Scenes..."
                  : `Auto-Match ${emptySections.length} Empty Scene(s) ($0)`}
              </span>
            </Button>

            <form onSubmit={handleStockSearch} className="flex gap-1.5">
              <Input
                value={stockQuery}
                onChange={(e) => setStockQuery(e.target.value)}
                placeholder="Search free stock (e.g. brain sleep forest)"
                className="h-10 text-xs"
              />
              <Button
                type="submit"
                size="sm"
                disabled={isSearchingStock || !stockQuery.trim()}
                className="h-10 px-3 shrink-0 touch-manipulation"
              >
                {isSearchingStock ? (
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                ) : (
                  <Search className="h-3.5 w-3.5" />
                )}
              </Button>
            </form>
            {stockProvider && (
              <p className="text-[10px] font-mono text-muted-foreground">
                Source: {stockProvider} · {projectAspectRatio} friendly
              </p>
            )}
          </div>

          {stockResults.length > 0 && (
            <div className="grid grid-cols-2 gap-2 max-h-[42vh] overflow-y-auto pr-1">
              {stockResults.map((item) => {
                const isImporting = importingStockId === item.id;
                const isDone = Boolean(importedIds[item.id]);
                return (
                  <div
                    key={item.id}
                    className="group relative overflow-hidden rounded-lg border border-border bg-secondary/40"
                  >
                    <img
                      src={item.thumb_url}
                      alt={item.title}
                      loading="lazy"
                      className="h-24 w-full object-cover"
                    />
                    <div className="p-1.5 flex items-center justify-between gap-1 bg-card/95">
                      <span className="truncate text-[10px] text-muted-foreground">
                        {item.author}
                      </span>
                      <button
                        type="button"
                        disabled={isImporting}
                        onClick={() => handleImportStockItem(item)}
                        className="inline-flex min-h-[28px] shrink-0 items-center gap-1 rounded bg-brand-violet px-2 py-0.5 text-[10px] font-semibold text-white hover:opacity-90 disabled:opacity-50 touch-manipulation"
                      >
                        {isImporting ? (
                          <Loader2 className="h-3 w-3 animate-spin" />
                        ) : isDone ? (
                          <Check className="h-3 w-3" />
                        ) : (
                          <Plus className="h-3 w-3" />
                        )}
                        {targetSectionId ? "Use" : isDone ? "Added" : "Add"}
                      </button>
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      ) : (
        <div className="space-y-3">
          <Button
            type="button"
            variant="secondary"
            size="sm"
            disabled={isGeneratingAi || emptySections.length === 0}
            onClick={handleGenerateAllEmpty}
            className="w-full min-h-[40px] touch-manipulation"
          >
            {isGeneratingAi && batchAiStatus ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Sparkles className="h-3.5 w-3.5" />
            )}
            <span>
              {batchAiStatus ||
                `AI Generate ${emptySections.length} Empty Scene(s) ($0)`}
            </span>
          </Button>

          <form onSubmit={handleGenerateSingleAi} className="space-y-2">
            <Textarea
              value={aiPrompt}
              onChange={(e) => setAiPrompt(e.target.value)}
              placeholder={
                targetSectionId
                  ? "Describe image for the selected scene..."
                  : "Describe an image to generate & add to Asset Bin..."
              }
              rows={3}
              className="text-xs resize-none"
            />
            <div className="grid grid-cols-2 gap-1.5">
              {AI_STYLES.map((st) => (
                <button
                  key={st.id}
                  type="button"
                  onClick={() => setAiStyle(st.id)}
                  className={`min-h-[32px] rounded-md border px-2 py-1 text-[11px] font-medium transition-colors touch-manipulation ${
                    aiStyle === st.id
                      ? "border-brand-lavender bg-brand-lavender/15 text-brand-violet dark:text-brand-lavender font-semibold"
                      : "border-border bg-card text-muted-foreground hover:text-foreground"
                  }`}
                >
                  {st.label}
                </button>
              ))}
            </div>
            <Button
              type="submit"
              size="sm"
              disabled={isGeneratingAi || !aiPrompt.trim()}
              className="w-full min-h-[40px] touch-manipulation"
            >
              {isGeneratingAi && !batchAiStatus ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <Sparkles className="h-3.5 w-3.5" />
              )}
              <span>
                {targetSectionId
                  ? "Generate & Assign to Scene ($0)"
                  : "Generate Image ($0)"}
              </span>
            </Button>
          </form>
        </div>
      )}
    </div>
  );
}
