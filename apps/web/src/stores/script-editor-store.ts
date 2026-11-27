import { create } from "zustand";
import { temporal } from "zundo";
import type { SectionDto } from "@pace/shared-types";

export interface SectionItem {
  id: string;
  text: string;
  wordCount: number;
  estimatedDurationSecs: number;
}

export function calculateWordCount(text: string): number {
  const words = text.trim().split(/\s+/).filter(Boolean);
  return words.length;
}

export function calculateEstimatedSeconds(wordCount: number): number {
  // 150 words per minute = 2.5 words per second -> duration = wordCount / 2.5
  return Math.round((wordCount / 150) * 60);
}

export function splitTextIntoSections(rawText: string): SectionItem[] {
  // Standardize CRLF to LF and split on 2 or more newlines (blank lines)
  const normalized = rawText.replace(/\r\n/g, "\n");
  const paragraphs = normalized
    .split(/\n\s*\n+/)
    .map((p) => p.trim())
    .filter(Boolean);

  if (paragraphs.length === 0) {
    return [
      {
        id: "section-1",
        text: "",
        wordCount: 0,
        estimatedDurationSecs: 0,
      },
    ];
  }

  return paragraphs.map((text, index) => {
    const wordCount = calculateWordCount(text);
    return {
      id: `section-${index + 1}-${Date.now()}-${Math.random().toString(36).substring(2, 7)}`,
      text,
      wordCount,
      estimatedDurationSecs: calculateEstimatedSeconds(wordCount),
    };
  });
}

interface ScriptEditorState {
  projectId: string | null;
  sections: SectionItem[];
  saveStatus: "saved" | "saving" | "error" | "unsaved";
  lastSavedAt: Date | null;

  // Actions
  resetForProject: (projectId: string) => void;
  hydrateFromServer: (projectId: string, serverSections: SectionDto[]) => void;
  applySavedSections: (
    projectId: string,
    serverSections: SectionDto[],
    sentSnapshot: SectionItem[]
  ) => void;
  setSections: (sections: SectionItem[]) => void;
  loadRawScript: (rawText: string) => void;
  updateSectionText: (id: string, newText: string) => void;
  splitSection: (id: string, cursorPosition: number) => void;
  splitSectionByParagraphs: (id: string, overrideText?: string) => void;
  mergeWithNext: (id: string) => void;
  deleteSection: (id: string) => void;
  reorderSections: (activeId: string, overId: string) => void;
  setSaveStatus: (status: "saved" | "saving" | "error" | "unsaved") => void;
  getFullScriptText: () => string;
}

export const useScriptEditorStore = create<ScriptEditorState>()(
  temporal(
    (set, get) => ({
      projectId: null,
      sections: [
        {
          id: "section-initial",
          text: "",
          wordCount: 0,
          estimatedDurationSecs: 0,
        },
      ],
      saveStatus: "saved",
      lastSavedAt: null,

      resetForProject: (projectId) =>
        set({
          projectId,
          sections: [
            {
              id: "section-initial",
              text: "",
              wordCount: 0,
              estimatedDurationSecs: 0,
            },
          ],
          saveStatus: "saved",
          lastSavedAt: null,
        }),

      hydrateFromServer: (projectId, serverSections) => {
        const items: SectionItem[] =
          serverSections.length > 0
            ? serverSections.map((s) => {
                const wordCount = calculateWordCount(s.text);
                return {
                  id: s.id,
                  text: s.text,
                  wordCount,
                  estimatedDurationSecs: calculateEstimatedSeconds(wordCount),
                };
              })
            : [
                {
                  id: "section-initial",
                  text: "",
                  wordCount: 0,
                  estimatedDurationSecs: 0,
                },
              ];

        set({
          projectId,
          sections: items,
          saveStatus: "saved",
          lastSavedAt: new Date(),
        });
      },

      applySavedSections: (projectId, serverSections, sentSnapshot) =>
        set((state) => {
          if (state.projectId !== projectId) return state;

          const idMap = new Map<string, string>();
          sentSnapshot.forEach((snap, idx) => {
            if (serverSections[idx]) {
              idMap.set(snap.id, serverSections[idx].id);
            }
          });

          const nextSections = state.sections.map((sec) => {
            const persistedId = idMap.get(sec.id);
            return persistedId ? { ...sec, id: persistedId } : sec;
          });

          const hasConcurrentChanges =
            state.sections.length !== sentSnapshot.length ||
            state.sections.some((sec, i) => sec.text !== sentSnapshot[i]?.text);

          return {
            sections: nextSections,
            saveStatus: hasConcurrentChanges ? "unsaved" : "saved",
            lastSavedAt: new Date(),
          };
        }),

      setSections: (sections) => set({ sections, saveStatus: "unsaved" }),

      loadRawScript: (rawText) => {
        const sections = splitTextIntoSections(rawText);
        set({ sections, saveStatus: "unsaved" });
      },

      updateSectionText: (id, newText) => {
        const wordCount = calculateWordCount(newText);
        const estimatedDurationSecs = calculateEstimatedSeconds(wordCount);

        set((state) => ({
          sections: state.sections.map((sec) =>
            sec.id === id
              ? { ...sec, text: newText, wordCount, estimatedDurationSecs }
              : sec
          ),
          saveStatus: "unsaved",
        }));
      },

      splitSection: (id, cursorPosition) => {
        set((state) => {
          const index = state.sections.findIndex((s) => s.id === id);
          if (index === -1) return state;

          const current = state.sections[index];
          const textBefore = current.text.slice(0, cursorPosition).trim();
          const textAfter = current.text.slice(cursorPosition).trim();

          // Avoid creating empty sections
          if (!textBefore || !textAfter) return state;

          const part1: SectionItem = {
            id: current.id,
            text: textBefore,
            wordCount: calculateWordCount(textBefore),
            estimatedDurationSecs: calculateEstimatedSeconds(
              calculateWordCount(textBefore)
            ),
          };

          const part2: SectionItem = {
            id: `section-${Date.now()}-${Math.random().toString(36).substring(2, 7)}`,
            text: textAfter,
            wordCount: calculateWordCount(textAfter),
            estimatedDurationSecs: calculateEstimatedSeconds(
              calculateWordCount(textAfter)
            ),
          };

          const newSections = [...state.sections];
          newSections.splice(index, 1, part1, part2);

          return {
            sections: newSections,
            saveStatus: "unsaved",
          };
        });
      },

      splitSectionByParagraphs: (id, overrideText) => {
        set((state) => {
          const index = state.sections.findIndex((s) => s.id === id);
          if (index === -1) return state;

          const current = state.sections[index];
          const textToSplit = overrideText !== undefined ? overrideText : current.text;
          const parsed = splitTextIntoSections(textToSplit);
          if (parsed.length <= 1) return state;

          const newSections = [...state.sections];
          newSections.splice(index, 1, ...parsed);

          return {
            sections: newSections,
            saveStatus: "unsaved",
          };
        });
      },

      mergeWithNext: (id) => {
        set((state) => {
          const index = state.sections.findIndex((s) => s.id === id);
          if (index === -1 || index >= state.sections.length - 1) return state;

          const current = state.sections[index];
          const next = state.sections[index + 1];
          const combinedText = `${current.text}\n\n${next.text}`.trim();
          const wordCount = calculateWordCount(combinedText);

          const merged: SectionItem = {
            id: current.id,
            text: combinedText,
            wordCount,
            estimatedDurationSecs: calculateEstimatedSeconds(wordCount),
          };

          const newSections = [...state.sections];
          newSections.splice(index, 2, merged);

          return {
            sections: newSections,
            saveStatus: "unsaved",
          };
        });
      },

      deleteSection: (id) => {
        set((state) => {
          if (state.sections.length <= 1) {
            // Keep at least one empty section
            return {
              sections: [
                {
                  id: "section-1",
                  text: "",
                  wordCount: 0,
                  estimatedDurationSecs: 0,
                },
              ],
              saveStatus: "unsaved",
            };
          }
          return {
            sections: state.sections.filter((s) => s.id !== id),
            saveStatus: "unsaved",
          };
        });
      },

      reorderSections: (activeId, overId) => {
        set((state) => {
          const oldIndex = state.sections.findIndex((s) => s.id === activeId);
          const newIndex = state.sections.findIndex((s) => s.id === overId);
          if (oldIndex === -1 || newIndex === -1 || oldIndex === newIndex) {
            return state;
          }

          const newSections = [...state.sections];
          const [moved] = newSections.splice(oldIndex, 1);
          newSections.splice(newIndex, 0, moved);

          return {
            sections: newSections,
            saveStatus: "unsaved",
          };
        });
      },

      setSaveStatus: (status) =>
        set({
          saveStatus: status,
          lastSavedAt: status === "saved" ? new Date() : get().lastSavedAt,
        }),

      getFullScriptText: () => {
        return get()
          .sections.map((s) => s.text.trim())
          .filter(Boolean)
          .join("\n\n");
      },
    }),
    {
      limit: 100, // 100 steps undo/redo history
    }
  )
);

