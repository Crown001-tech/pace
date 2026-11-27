import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, ApiError } from "@/lib/api-client";
import { isOfflineStudioActive } from "@/hooks/use-auth";
import type {
  CreateProjectRequest,
  ProjectResponse,
  UpdateProjectRequest,
} from "@pace/shared-types";

const INITIAL_DEMO_PROJECTS: ProjectResponse[] = [
  {
    id: "10000000-0000-0000-0000-000000000001",
    owner_id: "00000000-0000-0000-0000-000000000001",
    name: "The Deep Ocean Documentary",
    aspect_ratio: "16:9",
    fps: 30,
    language: "en",
    effect_seed: 42,
    settings: {},
    status: "draft",
    version: 1,
    created_at: new Date(Date.now() - 3600000 * 24).toISOString(),
    updated_at: new Date(Date.now() - 3600000 * 2).toISOString(),
  },
  {
    id: "10000000-0000-0000-0000-000000000002",
    owner_id: "00000000-0000-0000-0000-000000000001",
    name: "Quick Facts: Neutron Stars",
    aspect_ratio: "9:16",
    fps: 30,
    language: "en",
    effect_seed: 88,
    settings: {},
    status: "draft",
    version: 1,
    created_at: new Date(Date.now() - 3600000 * 48).toISOString(),
    updated_at: new Date(Date.now() - 3600000 * 5).toISOString(),
  },
];

function getLocalProjects(): ProjectResponse[] {
  if (typeof window === "undefined") return INITIAL_DEMO_PROJECTS;
  const stored = localStorage.getItem("pace_demo_projects");
  if (!stored) {
    localStorage.setItem("pace_demo_projects", JSON.stringify(INITIAL_DEMO_PROJECTS));
    return INITIAL_DEMO_PROJECTS;
  }
  try {
    return JSON.parse(stored);
  } catch {
    return INITIAL_DEMO_PROJECTS;
  }
}

function saveLocalProjects(projects: ProjectResponse[]) {
  if (typeof window !== "undefined") {
    localStorage.setItem("pace_demo_projects", JSON.stringify(projects));
  }
}

export function useProjects() {
  return useQuery<ProjectResponse[], ApiError>({
    queryKey: ["projects"],
    queryFn: async () => {
      if (isOfflineStudioActive()) {
        return getLocalProjects();
      }
      try {
        return await api.projects.list();
      } catch (err) {
        if (err instanceof ApiError) throw err;
        return getLocalProjects();
      }
    },
  });
}

export function useProject(id: string | null) {
  return useQuery<ProjectResponse, ApiError>({
    queryKey: ["projects", id],
    queryFn: async () => {
      if (!id) throw new Error("No id");
      if (isOfflineStudioActive()) {
        const local = getLocalProjects().find((p) => p.id === id);
        if (local) return local;
        throw new Error("Project not found");
      }
      try {
        return await api.projects.get(id);
      } catch (err) {
        if (err instanceof ApiError) throw err;
        const local = getLocalProjects().find((p) => p.id === id);
        if (local) return local;
        throw err;
      }
    },
    enabled: Boolean(id),
  });
}

export function useCreateProject() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (data: CreateProjectRequest) => {
      if (!isOfflineStudioActive()) {
        try {
          return await api.projects.create(data);
        } catch (err) {
          if (err instanceof ApiError) throw err;
        }
      }
      const newProj: ProjectResponse = {
        id:
          typeof crypto !== "undefined" && crypto.randomUUID
            ? crypto.randomUUID()
            : `local-${Date.now()}`,
        owner_id: "00000000-0000-0000-0000-000000000001",
        name: data.name,
        aspect_ratio: data.aspect_ratio || "16:9",
        fps: data.fps || 30,
        language: data.language || "en",
        effect_seed: data.effect_seed || 1,
        settings: (data.settings as Record<string, unknown>) || {},
        status: "draft",
        version: 1,
        created_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
      };
      const projects = [newProj, ...getLocalProjects()];
      saveLocalProjects(projects);
      return newProj;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["projects"] });
    },
  });
}

export function useUpdateProject() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async ({
      id,
      data,
      expectedVersion,
    }: {
      id: string;
      data: UpdateProjectRequest;
      expectedVersion?: number;
    }) => {
      if (!isOfflineStudioActive()) {
        try {
          return await api.projects.update(id, data, expectedVersion);
        } catch (err) {
          if (err instanceof ApiError) throw err;
        }
      }
      const projects = getLocalProjects();
      const index = projects.findIndex((p) => p.id === id);
      if (index === -1) throw new Error("Project not found");
      const prev = projects[index];
      const updated: ProjectResponse = {
        ...prev,
        name: data.name ?? prev.name,
        aspect_ratio: data.aspect_ratio ?? prev.aspect_ratio,
        settings: data.settings ? { ...prev.settings, ...data.settings } : prev.settings,
        status: data.status ?? prev.status,
        version: (prev.version || 1) + 1,
        updated_at: new Date().toISOString(),
      };
      projects[index] = updated;
      saveLocalProjects(projects);
      return updated;
    },
    onSuccess: (updated) => {
      queryClient.setQueryData(["projects", updated.id], updated);
      queryClient.invalidateQueries({ queryKey: ["projects"] });
    },
  });
}

export function useDeleteProject() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (id: string) => {
      if (!isOfflineStudioActive()) {
        try {
          await api.projects.delete(id);
          return;
        } catch (err) {
          if (err instanceof ApiError) throw err;
        }
      }
      const filtered = getLocalProjects().filter((p) => p.id !== id);
      saveLocalProjects(filtered);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["projects"] });
    },
  });
}
