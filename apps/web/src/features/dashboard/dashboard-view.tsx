"use client";

import React, { useState, useMemo } from "react";
import {
  useProjects,
  useCreateProject,
  useDeleteProject,
  useUpdateProject,
} from "@/hooks/use-projects";
import { useUser } from "@/hooks/use-auth";
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
  ArrowUpRight,
  Clock,
  Edit3,
  Film,
  Monitor,
  MoreHorizontal,
  Play,
  Plus,
  Search,
  Smartphone,
  Trash2,
  X,
} from "lucide-react";

interface DashboardViewProps {
  onOpenProject: (id: string) => void;
}

type AspectFilter = "all" | "16:9" | "9:16";

const COVER_IMAGES = [
  "/showcase/scene-1.jpg",
  "/showcase/scene-2.jpg",
  "/showcase/scene-3.jpg",
];

export function DashboardView({ onOpenProject }: DashboardViewProps) {
  const { data: user } = useUser();
  const { data: projects, isLoading } = useProjects();
  const createProject = useCreateProject();
  const deleteProject = useDeleteProject();
  const updateProject = useUpdateProject();

  const [searchQuery, setSearchQuery] = useState("");
  const [aspectFilter, setAspectFilter] = useState<AspectFilter>("all");
  const [showCreate, setShowCreate] = useState(false);
  const [newName, setNewName] = useState("");
  const [newAspect, setNewAspect] = useState<"16:9" | "9:16">("16:9");
  const [createError, setCreateError] = useState<string | null>(null);

  const [editingProject, setEditingProject] = useState<ProjectResponse | null>(null);
  const [renameName, setRenameName] = useState("");
  const [deletingProject, setDeletingProject] = useState<ProjectResponse | null>(null);

  const greetingHeading = useMemo(() => {
    const raw = user?.name?.trim();
    if (!raw || raw.toLowerCase().startsWith("offline")) {
      return "Your Studio";
    }
    const first = raw.split(/\s+/)[0];
    return first ? `Welcome, ${first}` : "Your Studio";
  }, [user?.name]);

  const counts = useMemo(() => {
    const list = projects ?? [];
    return {
      all: list.length,
      widescreen: list.filter((p) => p.aspect_ratio === "16:9").length,
      vertical: list.filter((p) => p.aspect_ratio === "9:16").length,
    };
  }, [projects]);

  const filteredProjects = useMemo(() => {
    if (!projects) return [];
    const q = searchQuery.trim().toLowerCase();
    return projects.filter((p) => {
      if (aspectFilter !== "all" && p.aspect_ratio !== aspectFilter) return false;
      if (q && !p.name.toLowerCase().includes(q)) return false;
      return true;
    });
  }, [projects, searchQuery, aspectFilter]);

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
    } catch { }
  };

  const handleDelete = async () => {
    if (!deletingProject) return;
    try {
      await deleteProject.mutateAsync(deletingProject.id);
      setDeletingProject(null);
    } catch { }
  };

  return (
    <div className="pace-sculpted-canvas relative min-h-[calc(100vh-3.5rem)] w-full overflow-hidden">
      {/* Sculpted Soft-3D Studio Background Relief */}
      <div aria-hidden="true" className="pointer-events-none absolute inset-0 overflow-hidden">
        <div className="pace-3d-ring -top-44 -left-32 absolute h-[380px] w-[380px] rounded-full opacity-75" />
        <div className="pace-3d-orb top-20 -right-10 absolute h-28 w-28 rounded-full opacity-70 animate-float-slow" />
        <div className="pace-3d-ring -bottom-52 right-12 absolute h-[420px] w-[420px] rounded-full opacity-60" />
      </div>

      <div className="relative z-10 mx-auto max-w-[1400px] px-4 sm:px-8 py-7 sm:py-10 pb-24 sm:pb-16 space-y-8">
        {/* Bento Creation Header */}
        <section className="grid grid-cols-1 lg:grid-cols-12 gap-5 items-stretch">
          {/* Left Studio Welcome Card */}
          <div className="lg:col-span-5 rounded-[28px] border border-white/80 dark:border-white/[0.09] bg-white/85 dark:bg-[#191C2E]/90 backdrop-blur-xl p-6 sm:p-7 studio-card flex flex-col justify-between gap-5">
            <div className="space-y-1.5">
              <span className="text-xs font-bold uppercase tracking-wider text-[#6D28D9] dark:text-[#B794FF]">
                Workspace
              </span>
              <h1 className="font-display text-2xl sm:text-3xl font-extrabold tracking-tight text-foreground leading-tight">
                {greetingHeading}
              </h1>
              <p className="text-sm text-muted-foreground leading-relaxed">
                Start a new narrated video or jump straight back into an active timeline.
              </p>
            </div>

            <div className="flex items-center justify-between gap-3 pt-1">
              <div className="flex items-center gap-5">
                <div>
                  <div className="font-display text-xl font-extrabold text-foreground tabular-nums">
                    {counts.all}
                  </div>
                  <div className="text-[11px] font-medium text-muted-foreground">Total</div>
                </div>
                <div className="h-7 w-px bg-border" />
                <div>
                  <div className="font-display text-xl font-extrabold text-foreground tabular-nums">
                    {counts.widescreen}
                  </div>
                  <div className="text-[11px] font-medium text-muted-foreground">16:9</div>
                </div>
                <div className="h-7 w-px bg-border" />
                <div>
                  <div className="font-display text-xl font-extrabold text-foreground tabular-nums">
                    {counts.vertical}
                  </div>
                  <div className="text-[11px] font-medium text-muted-foreground">9:16</div>
                </div>
              </div>

              <Button
                onClick={() => setShowCreate(true)}
                size="lg"
                className="rounded-full px-5 shrink-0"
              >
                <Plus className="h-4 w-4" />
                New Project
              </Button>
            </div>
          </div>

          {/* Right: Balanced 16:9 & True 9:16 Launchers */}
          <div className="lg:col-span-7 grid grid-cols-1 sm:grid-cols-2 gap-5">
            {/* 16:9 Widescreen Launcher */}
            <button
              type="button"
              onClick={() => {
                setNewAspect("16:9");
                setShowCreate(true);
              }}
              className="group rounded-[28px] border border-white/80 dark:border-white/[0.09] bg-white/85 dark:bg-[#191C2E]/90 backdrop-blur-xl p-2.5 text-left studio-card transition-all duration-200 hover:-translate-y-0.5 hover:border-[#8B5CF6]/55 active:scale-[0.99] flex flex-col justify-between"
            >
              <div className="relative h-32 w-full overflow-hidden rounded-[20px] bg-[#190B33]">
                {/* eslint-disable-next-line @next/next/no-img-element */}
                <img
                  src="/showcase/scene-1.jpg"
                  alt=""
                  className="h-full w-full object-cover transition-transform duration-500 ease-out group-hover:scale-105"
                />
                <div className="absolute inset-0 bg-gradient-to-t from-[#15072C]/75 via-black/10 to-transparent" />
                <div className="absolute top-2.5 left-2.5 inline-flex items-center gap-1.5 rounded-full bg-white/90 dark:bg-[#191C2E]/90 backdrop-blur-md px-3 py-1 text-[11px] font-bold text-[#151427] dark:text-white shadow-sm">
                  <Monitor className="h-3.5 w-3.5 text-[#6D28D9] dark:text-[#B794FF]" />
                  <span>16:9</span>
                </div>
              </div>

              <div className="flex items-center justify-between px-2.5 pt-3 pb-1.5">
                <div>
                  <div className="font-display text-base font-bold text-foreground">
                    Widescreen Video
                  </div>
                  <div className="text-xs text-muted-foreground">
                    1920 × 1080 · YouTube &amp; essays
                  </div>
                </div>
                <span className="pace-3d-medallion flex h-9 w-9 shrink-0 items-center justify-center rounded-xl text-foreground group-hover:text-[#6D28D9] dark:group-hover:text-[#C4B5FD] transition-colors">
                  <Plus className="h-4 w-4" />
                </span>
              </div>
            </button>

            {/* 9:16 Vertical Short Launcher — Exact 9:16 Portrait Visual Panel */}
            <button
              type="button"
              onClick={() => {
                setNewAspect("9:16");
                setShowCreate(true);
              }}
              className="group rounded-[28px] border border-white/80 dark:border-white/[0.09] bg-white/85 dark:bg-[#191C2E]/90 backdrop-blur-xl p-2.5 text-left studio-card transition-all duration-200 hover:-translate-y-0.5 hover:border-[#8B5CF6]/55 active:scale-[0.99] flex items-stretch justify-between gap-3"
            >
              <div className="flex flex-col justify-between px-2.5 py-2 min-w-0">
                <div className="inline-flex w-fit items-center gap-1.5 rounded-full bg-[#EFEAFF] dark:bg-[#25213B] px-3 py-1 text-[11px] font-bold text-[#5B21B6] dark:text-[#C4B5FD]">
                  <Smartphone className="h-3.5 w-3.5" />
                  <span>9:16</span>
                </div>

                <div className="space-y-1 my-auto py-2">
                  <div className="font-display text-base font-bold text-foreground leading-snug">
                    Vertical Short
                  </div>
                  <div className="text-xs text-muted-foreground leading-relaxed">
                    1080 × 1920
                    <br />
                    Reels, Shorts &amp; TikTok
                  </div>
                </div>

                <span className="pace-3d-medallion flex h-9 w-9 items-center justify-center rounded-xl text-foreground group-hover:text-[#6D28D9] dark:group-hover:text-[#C4B5FD] transition-colors">
                  <Plus className="h-4 w-4" />
                </span>
              </div>

              {/* True 9:16 Portrait Frame */}
              <div className="relative shrink-0 aspect-[9/16] h-[196px] overflow-hidden rounded-[20px] bg-[#190B33]">
                {/* eslint-disable-next-line @next/next/no-img-element */}
                <img
                  src="/showcase/scene-3.jpg"
                  alt=""
                  className="h-full w-full object-cover transition-transform duration-500 ease-out group-hover:scale-105"
                />
                <div className="absolute inset-0 bg-gradient-to-t from-[#15072C]/50 via-transparent to-black/15" />
              </div>
            </button>
          </div>
        </section>

        {/* Projects Section Header + Controls */}
        <div className="flex flex-col gap-3.5 sm:flex-row sm:items-center sm:justify-between">
          <div className="flex items-center gap-3 overflow-x-auto no-scrollbar">
            <h2 className="font-display text-lg font-bold text-foreground mr-1">
              Recent Projects
            </h2>
            <div
              role="tablist"
              aria-label="Filter projects"
              className="inline-flex items-center rounded-full bg-white/65 dark:bg-[#141728] border border-white/80 dark:border-white/[0.07] p-1 shadow-inner"
            >
              {(
                [
                  { id: "all", label: "All" },
                  { id: "16:9", label: "16:9" },
                  { id: "9:16", label: "9:16" },
                ] as const
              ).map((tab) => {
                const active = aspectFilter === tab.id;
                return (
                  <button
                    key={tab.id}
                    type="button"
                    role="tab"
                    aria-selected={active}
                    onClick={() => setAspectFilter(tab.id)}
                    className={`rounded-full px-3.5 py-1 text-xs font-semibold transition-all duration-150 ${active
                      ? "bg-[#6D28D9] text-white shadow-sm"
                      : "text-muted-foreground hover:text-foreground"
                      }`}
                  >
                    {tab.label}
                  </button>
                );
              })}
            </div>
          </div>

          <div className="relative w-full sm:w-72">
            <Search className="pointer-events-none absolute left-3.5 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
            <input
              type="search"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="Search projects..."
              className="pace-3d-input h-10 w-full rounded-full pl-10 pr-9 text-sm text-foreground placeholder:text-muted-foreground/70 outline-none transition-all duration-150"
            />
            {searchQuery && (
              <button
                type="button"
                onClick={() => setSearchQuery("")}
                aria-label="Clear search"
                className="absolute right-3 top-1/2 -translate-y-1/2 rounded-full p-1 text-muted-foreground hover:text-foreground"
              >
                <X className="h-3.5 w-3.5" />
              </button>
            )}
          </div>
        </div>

        {/* Project Grid */}
        {isLoading ? (
          <div className="grid grid-cols-1 gap-5 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
            {[1, 2, 3, 4].map((i) => (
              <div
                key={i}
                className="h-60 rounded-[26px] border border-border bg-card/70 animate-pulse"
              />
            ))}
          </div>
        ) : filteredProjects.length === 0 ? (
          <div className="flex flex-col items-center justify-center rounded-[28px] border border-dashed border-border bg-white/75 dark:bg-[#191C2E]/80 backdrop-blur-xl py-16 px-4 text-center studio-card">
            <div className="pace-3d-medallion mb-4 flex h-14 w-14 items-center justify-center rounded-2xl text-[#6D28D9] dark:text-[#B794FF]">
              <Film className="h-6 w-6" />
            </div>
            <h3 className="font-display text-lg font-bold text-foreground">
              {searchQuery || aspectFilter !== "all"
                ? "No matching projects"
                : "No projects yet"}
            </h3>
            <p className="mt-1 max-w-sm text-sm text-muted-foreground leading-relaxed">
              {searchQuery || aspectFilter !== "all"
                ? "Try another keyword or clear your format filter."
                : "Create your first project to organize scenes, visuals, and voiceover."}
            </p>
            {searchQuery || aspectFilter !== "all" ? (
              <Button
                variant="secondary"
                size="sm"
                onClick={() => {
                  setSearchQuery("");
                  setAspectFilter("all");
                }}
                className="mt-5 rounded-full px-4"
              >
                Reset filters
              </Button>
            ) : (
              <Button
                onClick={() => setShowCreate(true)}
                size="lg"
                className="mt-5 rounded-full px-6"
              >
                <Plus className="h-4 w-4" />
                Create Project
              </Button>
            )}
          </div>
        ) : (
          <div className="columns-1 sm:columns-2 lg:columns-3 xl:columns-4 gap-5 space-y-5">
            {filteredProjects.map((project, index) => (
              <ProjectCard
                key={project.id}
                project={project}
                coverSrc={COVER_IMAGES[index % COVER_IMAGES.length]}
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
        <div className="fixed bottom-0 inset-x-0 z-30 border-t border-border bg-card/90 backdrop-blur-xl p-3.5 pb-safe sm:hidden">
          <Button
            size="lg"
            onClick={() => setShowCreate(true)}
            className="w-full rounded-full"
          >
            <Plus className="h-4 w-4" />
            New Project
          </Button>
        </div>

        {/* Create Project Dialog */}
        <Dialog open={showCreate} onOpenChange={setShowCreate}>
          <DialogContent onClose={() => setShowCreate(false)}>
            <DialogHeader>
              <DialogTitle className="font-display">New Project</DialogTitle>
              <DialogDescription>
                Name your project and pick its frame format.
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
                  Canvas Format
                </label>
                <div className="grid grid-cols-2 gap-3">
                  <button
                    type="button"
                    onClick={() => setNewAspect("16:9")}
                    className={`flex flex-col items-start rounded-2xl border p-4 text-left transition-all duration-150 ${newAspect === "16:9"
                      ? "border-[#8B5CF6] bg-[#8B5CF6]/10 text-foreground ring-1 ring-[#8B5CF6]/40"
                      : "border-border bg-secondary/50 text-muted-foreground hover:bg-secondary"
                      }`}
                  >
                    <div className="flex items-center justify-between w-full mb-2">
                      <Monitor className="h-4 w-4 text-[#6D28D9] dark:text-[#B794FF]" />
                      <span className="text-xs font-mono font-semibold">16:9</span>
                    </div>
                    <div className="text-sm font-semibold text-foreground">Widescreen</div>
                    <div className="text-xs text-muted-foreground mt-0.5">1920 × 1080</div>
                  </button>

                  <button
                    type="button"
                    onClick={() => setNewAspect("9:16")}
                    className={`flex flex-col items-start rounded-2xl border p-4 text-left transition-all duration-150 ${newAspect === "9:16"
                      ? "border-[#8B5CF6] bg-[#8B5CF6]/10 text-foreground ring-1 ring-[#8B5CF6]/40"
                      : "border-border bg-secondary/50 text-muted-foreground hover:bg-secondary"
                      }`}
                  >
                    <div className="flex items-center justify-between w-full mb-2">
                      <Smartphone className="h-4 w-4 text-[#6D28D9] dark:text-[#B794FF]" />
                      <span className="text-xs font-mono font-semibold">9:16</span>
                    </div>
                    <div className="text-sm font-semibold text-foreground">Vertical</div>
                    <div className="text-xs text-muted-foreground mt-0.5">1080 × 1920</div>
                  </button>
                </div>
              </div>

              {createError && (
                <div className="rounded-xl border border-red-500/30 bg-red-500/10 px-3.5 py-2.5 text-xs text-red-700 dark:text-red-300 flex items-center gap-2">
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
                  className="rounded-full px-5"
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
              <DialogTitle className="font-display">Rename Project</DialogTitle>
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
                <Button type="submit" className="rounded-full px-5" isLoading={updateProject.isPending}>
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
              <DialogTitle className="font-display">Delete Project</DialogTitle>
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
                className="rounded-full px-5"
                onClick={handleDelete}
                isLoading={deleteProject.isPending}
              >
                Delete Project
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      </div>
    </div>
  );
}

function ProjectCard({
  project,
  coverSrc,
  onOpen,
  onRename,
  onDelete,
}: {
  project: ProjectResponse;
  coverSrc: string;
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
      className="break-inside-avoid group relative flex flex-col justify-between rounded-[26px] border border-white/80 dark:border-white/[0.09] bg-white/85 dark:bg-[#191C2E]/90 backdrop-blur-xl p-2.5 cursor-pointer studio-card transition-all duration-200 hover:-translate-y-1 hover:border-[#8B5CF6]/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[#8B5CF6]"
    >
      {/* Inset Rounded Visual Cover — Full-Bleed Portrait (9:16) vs Widescreen (16:9) */}
      <div
        className={`relative overflow-hidden rounded-[18px] bg-[#190B33] ${
          isVertical ? "aspect-[9/14]" : "aspect-[16/9]"
        }`}
      >
        {/* eslint-disable-next-line @next/next/no-img-element */}
        <img
          src={coverSrc}
          alt={project.name}
          className="h-full w-full object-cover transition-transform duration-500 ease-out group-hover:scale-105"
        />
        <div
          aria-hidden="true"
          className="pointer-events-none absolute inset-0 bg-gradient-to-t from-black/70 via-black/15 to-black/20"
        />

        {/* Subtle Hover Play Button */}
        <div className="pointer-events-none absolute inset-0 z-20 flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity duration-200">
          <div className="flex h-10 w-10 items-center justify-center rounded-full bg-white/95 text-[#5B21B6] shadow-lg">
            <Play className="h-4 w-4 fill-current ml-0.5" />
          </div>
        </div>

        {/* Status Tag */}
        <div className="absolute top-2.5 left-2.5 z-20">
          <Badge variant={project.status === "ready" ? "success" : "secondary"}>
            {project.status}
          </Badge>
        </div>

        {/* Format Pill */}
        <div className="absolute bottom-2.5 left-2.5 z-20 inline-flex items-center gap-1 rounded-full bg-white/90 dark:bg-[#191C2E]/90 backdrop-blur-md px-2.5 py-0.5 text-[11px] font-bold text-[#151427] dark:text-white shadow-sm">
          <span>{project.aspect_ratio}</span>
          <span className="opacity-45">·</span>
          <span>{project.fps}fps</span>
        </div>

        {/* Context Menu Button */}
        <div
          className="absolute top-2 right-2 z-20"
          onClick={(e) => e.stopPropagation()}
        >
          <button
            type="button"
            onClick={() => setMenuOpen(!menuOpen)}
            aria-label="Project options"
            className="flex h-8 w-8 items-center justify-center rounded-full bg-black/40 backdrop-blur-md text-white hover:bg-black/65 transition-colors"
          >
            <MoreHorizontal className="h-4 w-4" />
          </button>

          {menuOpen && (
            <>
              <div
                className="fixed inset-0 z-10"
                onClick={() => setMenuOpen(false)}
              />
              <div className="absolute right-0 mt-1.5 z-20 w-36 rounded-2xl border border-border bg-popover p-1.5 shadow-xl">
                <button
                  type="button"
                  onClick={() => {
                    setMenuOpen(false);
                    onRename();
                  }}
                  className="flex w-full items-center gap-2 rounded-xl px-2.5 py-2 text-left text-xs font-medium text-foreground hover:bg-secondary transition-colors"
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
                  className="flex w-full items-center gap-2 rounded-xl px-2.5 py-2 text-left text-xs font-medium text-red-500 hover:bg-red-500/10 transition-colors"
                >
                  <Trash2 className="h-3.5 w-3.5" />
                  Delete
                </button>
              </div>
            </>
          )}
        </div>
      </div>

      {/* Card Body */}
      <div className="px-2.5 pt-3.5 pb-1.5 space-y-2.5 flex-1 flex flex-col justify-between">
        <div>
          <h3 className="font-display text-[15px] font-bold text-foreground line-clamp-1 group-hover:text-[#6D28D9] dark:group-hover:text-[#B794FF] transition-colors">
            {project.name}
          </h3>
          <div className="mt-1 flex items-center gap-1.5 text-xs text-muted-foreground">
            <Clock className="h-3 w-3 opacity-75" />
            <span>{formattedDate}</span>
          </div>
        </div>

        <div className="flex items-center justify-between pt-2.5 border-t border-border/60">
          <span className="text-xs font-medium text-muted-foreground">
            {isVertical ? "Vertical" : "Widescreen"}
          </span>
          <span className="inline-flex items-center gap-1 text-xs font-bold text-[#6D28D9] dark:text-[#B794FF]">
            Open
            <ArrowUpRight className="h-3.5 w-3.5 transition-transform duration-150 group-hover:translate-x-0.5 group-hover:-translate-y-0.5" />
          </span>
        </div>
      </div>
    </div>
  );
}
