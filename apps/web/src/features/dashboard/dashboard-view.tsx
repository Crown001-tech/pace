"use client";

import React, { useState, useMemo } from "react";
import {
  useProjects,
  useCreateProject,
  useDeleteProject,
  useUpdateProject,
} from "@/hooks/use-projects";
import type { ProjectResponse } from "@pace/shared-types";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { ApiError } from "@/lib/api-client";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog";
import {
  Plus,
  MoreHorizontal,
  ArrowUpRight,
  Trash2,
  Edit3,
  Clock,
  Search,
  Monitor,
  Smartphone,
  Film,
} from "lucide-react";

interface DashboardViewProps {
  onOpenProject: (id: string) => void;
}

export function DashboardView({ onOpenProject }: DashboardViewProps) {
  const { data: projects, isLoading } = useProjects();
  const createProject = useCreateProject();
  const deleteProject = useDeleteProject();
  const updateProject = useUpdateProject();

  const [searchQuery, setSearchQuery] = useState("");
  const [showCreate, setShowCreate] = useState(false);
  const [newName, setNewName] = useState("");
  const [newAspect, setNewAspect] = useState<"16:9" | "9:16">("16:9");
  const [createError, setCreateError] = useState<string | null>(null);

  const [editingProject, setEditingProject] = useState<ProjectResponse | null>(null);
  const [renameName, setRenameName] = useState("");
  const [deletingProject, setDeletingProject] = useState<ProjectResponse | null>(null);

  const filteredProjects = useMemo(() => {
    if (!projects) return [];
    const q = searchQuery.trim().toLowerCase();
    if (!q) return projects;
    return projects.filter((p) => p.name.toLowerCase().includes(q));
  }, [projects, searchQuery]);

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newName.trim()) return;
    setCreateError(null);
    try {
      const created = await createProject.mutateAsync({
        name: newName.trim(),
        aspect_ratio: newAspect,
      });
      setShowCreate(false);
      setNewName("");
      onOpenProject(created.id);
    } catch (err) {
      const msg = err instanceof ApiError ? err.message : "Failed to create project.";
      setCreateError(msg);
    }
  };

  const handleRename = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!editingProject || !renameName.trim()) return;
    try {
      await updateProject.mutateAsync({
        id: editingProject.id,
        data: { name: renameName.trim() },
        expectedVersion: editingProject.version,
      });
      setEditingProject(null);
      setRenameName("");
    } catch {}
  };

  const handleDelete = async () => {
    if (!deletingProject) return;
    try {
      await deleteProject.mutateAsync(deletingProject.id);
      setDeletingProject(null);
    } catch {}
  };

  return (
    <div className="mx-auto max-w-[1440px] px-4 sm:px-8 py-8 sm:py-10 pb-24 sm:pb-14 space-y-8">
      {/* Studio Library Header */}
      <div className="flex flex-col gap-5 sm:flex-row sm:items-end sm:justify-between">
        <div className="space-y-1.5">
          <div className="flex items-center gap-3">
            <h1 className="text-2xl sm:text-3xl font-bold tracking-tight text-foreground">
              Studio Library
            </h1>
            {projects && (
              <span className="inline-flex items-center rounded-[8px] bg-secondary px-2.5 py-0.5 text-xs font-mono font-medium text-muted-foreground tabular-nums">
                {projects.length}
              </span>
            )}
          </div>
          <p className="text-sm text-muted-foreground">
            Create a new narrated video or pick up right where you left off.
          </p>
        </div>

        <div className="flex items-center gap-3">
          <div className="relative flex-1 sm:w-72">
            <Search className="pointer-events-none absolute left-3.5 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground/70" />
            <Input
              type="search"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="Search projects..."
              className="pl-10 h-10"
            />
          </div>

          <Button
            onClick={() => setShowCreate(true)}
            size="lg"
            className="hidden sm:inline-flex shrink-0 h-10"
          >
            <Plus className="h-4 w-4" />
            New Project
          </Button>
        </div>
      </div>

      {/* Quick Start Format Strip */}
      <div className="grid grid-cols-1 sm:grid-cols-2 gap-3.5">
        <button
          type="button"
          onClick={() => {
            setNewAspect("16:9");
            setShowCreate(true);
          }}
          className="group flex items-center justify-between rounded-2xl border border-border/60 bg-card/80 p-4 text-left studio-card transition-all duration-150 hover:border-brand-lavender/50 hover:bg-card"
        >
          <div className="flex items-center gap-3.5">
            <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-brand-violet/10 text-brand-violet dark:bg-brand-lavender/15 dark:text-brand-lavender">
              <Monitor className="h-5 w-5" />
            </div>
            <div>
              <div className="text-sm font-semibold text-foreground">New Widescreen Video</div>
              <div className="text-xs text-muted-foreground">16:9 landscape · YouTube &amp; essays</div>
            </div>
          </div>
          <span className="inline-flex h-8 w-8 items-center justify-center rounded-[8px] bg-secondary text-muted-foreground group-hover:bg-brand-violet group-hover:text-white dark:group-hover:bg-brand-lavender dark:group-hover:text-[#140B29] transition-colors">
            <Plus className="h-4 w-4" />
          </span>
        </button>

        <button
          type="button"
          onClick={() => {
            setNewAspect("9:16");
            setShowCreate(true);
          }}
          className="group flex items-center justify-between rounded-2xl border border-border/60 bg-card/80 p-4 text-left studio-card transition-all duration-150 hover:border-brand-lavender/50 hover:bg-card"
        >
          <div className="flex items-center gap-3.5">
            <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-brand-violet/10 text-brand-violet dark:bg-brand-lavender/15 dark:text-brand-lavender">
              <Smartphone className="h-5 w-5" />
            </div>
            <div>
              <div className="text-sm font-semibold text-foreground">New Vertical Short</div>
              <div className="text-xs text-muted-foreground">9:16 portrait · Reels, Shorts &amp; TikTok</div>
            </div>
          </div>
          <span className="inline-flex h-8 w-8 items-center justify-center rounded-[8px] bg-secondary text-muted-foreground group-hover:bg-brand-violet group-hover:text-white dark:group-hover:bg-brand-lavender dark:group-hover:text-[#140B29] transition-colors">
            <Plus className="h-4 w-4" />
          </span>
        </button>
      </div>

      {/* Content */}
      {isLoading ? (
        <div className="grid grid-cols-1 gap-5 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
          {[1, 2, 3, 4].map((i) => (
            <div
              key={i}
              className="h-56 rounded-2xl bg-card/70 animate-pulse studio-card"
            />
          ))}
        </div>
      ) : filteredProjects.length === 0 ? (
        <div className="flex flex-col items-center justify-center rounded-2xl border border-dashed border-border/80 bg-card/50 py-20 px-4 text-center">
          <div className="mb-4 flex h-12 w-12 items-center justify-center rounded-xl bg-brand-violet/10 text-brand-violet dark:bg-brand-lavender/15 dark:text-brand-lavender">
            <Film className="h-5 w-5" />
          </div>
          <h3 className="text-base font-semibold text-foreground">
            {searchQuery ? "No matching projects" : "No projects yet"}
          </h3>
          <p className="mt-1.5 max-w-sm text-sm text-muted-foreground leading-relaxed">
            {searchQuery
              ? "Try another title keyword or clear your search filter."
              : "Create your first video project to organize scenes, visuals, and narration."}
          </p>
          {!searchQuery && (
            <Button onClick={() => setShowCreate(true)} size="lg" className="mt-5">
              <Plus className="h-4 w-4" />
              New Project
            </Button>
          )}
        </div>
      ) : (
        <div className="grid grid-cols-1 gap-5 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
          {filteredProjects.map((project) => (
            <ProjectCard
              key={project.id}
              project={project}
              onOpen={() => onOpenProject(project.id)}
              onRename={() => {
                setEditingProject(project);
                setRenameName(project.name);
              }}
              onDelete={() => setDeletingProject(project)}
            />
          ))}
        </div>
      )}

      {/* Mobile Bottom New-Project Dock */}
      <div className="fixed bottom-0 inset-x-0 z-30 border-t border-border/60 bg-background/90 backdrop-blur-xl p-3.5 pb-safe sm:hidden">
        <Button
          size="lg"
          onClick={() => setShowCreate(true)}
          className="w-full"
        >
          <Plus className="h-4 w-4" />
          New Project
        </Button>
      </div>

      {/* Create Project Dialog */}
      <Dialog open={showCreate} onOpenChange={setShowCreate}>
        <DialogContent onClose={() => setShowCreate(false)}>
          <DialogHeader>
            <DialogTitle>New Studio Project</DialogTitle>
            <DialogDescription>
              Set the project title and target frame aspect ratio.
            </DialogDescription>
          </DialogHeader>

          <form onSubmit={handleCreate} className="space-y-5">
            <div className="space-y-2">
              <label className="text-xs font-semibold text-foreground">
                Project Title
              </label>
              <Input
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                placeholder="e.g., The Deep Ocean"
                autoFocus
                required
              />
            </div>

            <div className="space-y-2">
              <label className="text-xs font-semibold text-foreground">
                Canvas Aspect Ratio
              </label>
              <div className="grid grid-cols-2 gap-3">
                <button
                  type="button"
                  onClick={() => setNewAspect("16:9")}
                  className={`flex flex-col items-start rounded-xl border p-4 text-left transition-all duration-150 ${
                    newAspect === "16:9"
                      ? "border-brand-lavender bg-brand-lavender/10 text-foreground ring-1 ring-brand-lavender/40"
                      : "border-border/80 bg-secondary/40 text-muted-foreground hover:bg-secondary/70"
                  }`}
                >
                  <div className="flex items-center justify-between w-full mb-2.5">
                    <Monitor className="h-4 w-4 text-brand-violet dark:text-brand-lavender" />
                    <span className="text-xs font-mono font-semibold">16:9</span>
                  </div>
                  <div className="text-sm font-semibold text-foreground">Widescreen</div>
                  <div className="text-xs text-muted-foreground mt-0.5 font-mono">1920 × 1080</div>
                </button>

                <button
                  type="button"
                  onClick={() => setNewAspect("9:16")}
                  className={`flex flex-col items-start rounded-xl border p-4 text-left transition-all duration-150 ${
                    newAspect === "9:16"
                      ? "border-brand-lavender bg-brand-lavender/10 text-foreground ring-1 ring-brand-lavender/40"
                      : "border-border/80 bg-secondary/40 text-muted-foreground hover:bg-secondary/70"
                  }`}
                >
                  <div className="flex items-center justify-between w-full mb-2.5">
                    <Smartphone className="h-4 w-4 text-brand-violet dark:text-brand-lavender" />
                    <span className="text-xs font-mono font-semibold">9:16</span>
                  </div>
                  <div className="text-sm font-semibold text-foreground">Vertical</div>
                  <div className="text-xs text-muted-foreground mt-0.5 font-mono">1080 × 1920</div>
                </button>
              </div>
            </div>

            {createError && (
              <div className="rounded-[10px] border border-red-500/30 bg-red-500/10 px-3.5 py-2.5 text-xs text-red-700 dark:text-red-300 flex items-center gap-2">
                <span>{createError}</span>
              </div>
            )}

            <DialogFooter>
              <Button
                type="button"
                variant="ghost"
                onClick={() => setShowCreate(false)}
              >
                Cancel
              </Button>
              <Button
                type="submit"
                isLoading={createProject.isPending}
                disabled={!newName.trim()}
              >
                Create Project
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>

      {/* Rename Project Dialog */}
      <Dialog
        open={Boolean(editingProject)}
        onOpenChange={(open) => !open && setEditingProject(null)}
      >
        <DialogContent onClose={() => setEditingProject(null)}>
          <DialogHeader>
            <DialogTitle>Rename Project</DialogTitle>
          </DialogHeader>
          <form onSubmit={handleRename} className="space-y-4">
            <Input
              value={renameName}
              onChange={(e) => setRenameName(e.target.value)}
              autoFocus
              required
            />

            <DialogFooter>
              <Button
                type="button"
                variant="ghost"
                onClick={() => setEditingProject(null)}
              >
                Cancel
              </Button>
              <Button type="submit" isLoading={updateProject.isPending}>
                Save changes
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>

      {/* Delete Project Confirmation */}
      <Dialog
        open={Boolean(deletingProject)}
        onOpenChange={(open) => !open && setDeletingProject(null)}
      >
        <DialogContent onClose={() => setDeletingProject(null)}>
          <DialogHeader>
            <DialogTitle>Delete Project</DialogTitle>
            <DialogDescription>
              Delete <strong className="text-foreground">{deletingProject?.name}</strong>? All
              script sections, media assignments, and timeline cuts will be removed.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              type="button"
              variant="ghost"
              onClick={() => setDeletingProject(null)}
            >
              Cancel
            </Button>
            <Button
              type="button"
              variant="destructive"
              onClick={handleDelete}
              isLoading={deleteProject.isPending}
            >
              Delete Project
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

function ProjectCard({
  project,
  onOpen,
  onRename,
  onDelete,
}: {
  project: ProjectResponse;
  onOpen: () => void;
  onRename: () => void;
  onDelete: () => void;
}) {
  const [menuOpen, setMenuOpen] = useState(false);

  const formattedDate = useMemo(() => {
    try {
      return new Intl.DateTimeFormat("en-US", {
        month: "short",
        day: "numeric",
        hour: "2-digit",
        minute: "2-digit",
      }).format(new Date(project.updated_at));
    } catch {
      return "Recently";
    }
  }, [project.updated_at]);

  const isVertical = project.aspect_ratio === "9:16";

  return (
    <div
      onClick={onOpen}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onOpen();
        }
      }}
      className="group relative flex flex-col justify-between rounded-2xl border border-border/60 bg-card overflow-hidden cursor-pointer studio-card transition-all duration-200 hover:-translate-y-0.5 hover:border-brand-lavender/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-lavender"
    >
      {/* Visual Monitor Frame Header */}
      <div className="relative h-32 bg-gradient-to-br from-brand-violet/[0.06] via-secondary/60 to-brand-lavender/[0.08] dark:from-[#23183A] dark:via-[#181425] dark:to-[#1E1633] flex items-center justify-center p-4">
        <div
          className={`flex flex-col items-center justify-center gap-1 rounded-xl border border-border/70 bg-background/85 backdrop-blur-md shadow-sm transition-transform duration-200 group-hover:scale-[1.03] ${
            isVertical ? "h-[86px] w-14" : "h-[70px] w-32"
          }`}
        >
          <Film className="h-4 w-4 text-brand-violet/70 dark:text-brand-lavender/70" />
          <span className="text-[10px] font-mono font-semibold text-muted-foreground">
            {project.aspect_ratio}
          </span>
        </div>

        {/* Status Tag */}
        <div className="absolute top-3 left-3">
          <Badge variant={project.status === "ready" ? "success" : "secondary"}>
            {project.status}
          </Badge>
        </div>

        {/* Context Menu Button */}
        <div
          className="absolute top-2.5 right-2.5"
          onClick={(e) => e.stopPropagation()}
        >
          <button
            type="button"
            onClick={() => setMenuOpen(!menuOpen)}
            aria-label="Project options"
            className="flex h-8 w-8 items-center justify-center rounded-[8px] text-muted-foreground hover:bg-card/90 hover:text-foreground transition-colors"
          >
            <MoreHorizontal className="h-4 w-4" />
          </button>

          {menuOpen && (
            <>
              <div
                className="fixed inset-0 z-10"
                onClick={() => setMenuOpen(false)}
              />
              <div className="absolute right-0 mt-1.5 z-20 w-36 rounded-xl border border-border/80 bg-popover p-1.5 shadow-xl">
                <button
                  type="button"
                  onClick={() => {
                    setMenuOpen(false);
                    onRename();
                  }}
                  className="flex w-full items-center gap-2 rounded-[8px] px-2.5 py-2 text-left text-xs font-medium text-foreground hover:bg-secondary transition-colors"
                >
                  <Edit3 className="h-3.5 w-3.5 text-muted-foreground" />
                  Rename
                </button>
                <button
                  type="button"
                  onClick={() => {
                    setMenuOpen(false);
                    onDelete();
                  }}
                  className="flex w-full items-center gap-2 rounded-[8px] px-2.5 py-2 text-left text-xs font-medium text-red-500 hover:bg-red-500/10 transition-colors"
                >
                  <Trash2 className="h-3.5 w-3.5" />
                  Delete
                </button>
              </div>
            </>
          )}
        </div>
      </div>

      {/* Body */}
      <div className="p-4 space-y-3.5 flex-1 flex flex-col justify-between">
        <div>
          <h3 className="text-[15px] font-semibold text-foreground line-clamp-1 group-hover:text-brand-violet dark:group-hover:text-brand-lavender transition-colors">
            {project.name}
          </h3>
          <div className="mt-1.5 flex items-center gap-2 text-xs text-muted-foreground">
            <span className="font-mono">{project.fps} FPS</span>
            <span>·</span>
            <span className="flex items-center gap-1">
              <Clock className="h-3 w-3 opacity-75" />
              {formattedDate}
            </span>
          </div>
        </div>

        <div className="flex items-center justify-between pt-3 border-t border-border/50">
          <span className="text-xs text-muted-foreground">
            {isVertical ? "Vertical" : "Widescreen"}
          </span>
          <span className="inline-flex items-center gap-1 text-xs font-semibold text-brand-violet dark:text-brand-lavender">
            Open
            <ArrowUpRight className="h-3.5 w-3.5 transition-transform duration-150 group-hover:translate-x-0.5 group-hover:-translate-y-0.5" />
          </span>
        </div>
      </div>
    </div>
  );
}
