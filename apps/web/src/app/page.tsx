"use client";

import React, { useState, useEffect } from "react";
import { useUser } from "@/hooks/use-auth";
import { TopNav } from "@/components/layout/top-nav";
import { PaceMark } from "@/components/ui/pace-logo";
import { LoginView, ForcePasswordChangeView } from "@/features/auth/login-view";
import { DashboardView } from "@/features/dashboard/dashboard-view";
import { ProjectWorkspaceView } from "@/features/project/project-workspace-view";

export default function Home() {
  const { data: user, isLoading } = useUser();
  const [activeProjectId, setActiveProjectId] = useState<string | null>(null);

  useEffect(() => {
    try {
      const saved = localStorage.getItem("pace_active_project_id");
      if (saved) {
        setActiveProjectId(saved);
      }
    } catch {}
  }, []);

  // Clear active project id when user signs out or changes
  useEffect(() => {
    if (!user) {
      setActiveProjectId(null);
      try {
        localStorage.removeItem("pace_active_project_id");
      } catch {}
    }
  }, [user]);

  const handleOpenProject = (id: string | null) => {
    setActiveProjectId(id);
    try {
      if (id) {
        localStorage.setItem("pace_active_project_id", id);
      } else {
        localStorage.removeItem("pace_active_project_id");
      }
    } catch {}
  };

  if (isLoading) {
    return (
      <div className="flex min-h-screen items-center justify-center bg-background">
        <div className="flex flex-col items-center gap-4">
          <div className="relative flex items-center justify-center">
            <div className="absolute inset-0 rounded-2xl bg-brand-lavender/25 blur-xl animate-pulse" />
            <PaceMark className="relative h-12 w-12 drop-shadow-md" />
          </div>
          <div className="flex flex-col items-center gap-2">
            <div className="h-1 w-28 overflow-hidden rounded-full bg-secondary">
              <div className="h-full w-1/2 rounded-full bg-brand-violet dark:bg-brand-lavender animate-[pulse_1.2s_ease-in-out_infinite]" />
            </div>
            <span className="text-xs font-medium text-muted-foreground">
              Preparing workspace
            </span>
          </div>
        </div>
      </div>
    );
  }

  if (!user) {
    return <LoginView />;
  }

  if (user.must_change_password) {
    return <ForcePasswordChangeView user={user} />;
  }

  if (activeProjectId) {
    return (
      <div className="min-h-screen bg-background flex flex-col">
        <TopNav onHomeClick={() => handleOpenProject(null)} />
        <main className="flex-1 flex flex-col min-w-0">
          <ProjectWorkspaceView
            projectId={activeProjectId}
            onBack={() => handleOpenProject(null)}
          />
        </main>
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-background flex flex-col">
      <TopNav onHomeClick={() => handleOpenProject(null)} />
      <main className="flex-1">
        <DashboardView onOpenProject={(id) => handleOpenProject(id)} />
      </main>
    </div>
  );
}
