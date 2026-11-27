import type {
  AssignmentResponse,
  AssignSectionRequest,
  AutoMatchStockRequest,
  AutoMatchStockResponse,
  BatchUpdateBoundariesRequest,
  BoundaryDto,
  ChangePasswordRequest,
  CreateProjectRequest,
  ExportResponse,
  GenerateAiMediaRequest,
  ImportStockMediaRequest,
  JobStatusDto,
  LoginRequest,
  LoginResponse,
  MediaResponse,
  MergeSubtitleCuesRequest,
  MessageResponse,
  MusicStateResponse,
  NarrationPeaksResponse,
  NarrationStateResponse,
  PresignMusicRequest,
  PresignMusicResponse,
  PresignNarrationRequest,
  PresignNarrationResponse,
  PresignUploadRequest,
  PresignUploadResponse,
  ProblemDetails,
  ProjectResponse,
  RealignNarrationRequest,
  RegisterMediaRequest,
  RegisterMusicRequest,
  RegisterNarrationRequest,
  RegisterNarrationResponse,
  RenderSpecResponse,
  RerollEffectsResponse,
  SaveScriptRequest,
  ScriptResponse,
  SplitSubtitleCueRequest,
  StartExportRequest,
  StockSearchResponse,
  SubtitleCueDto,
  SubtitlesStateResponse,
  SubtitleStyleDto,
  SwapSectionsRequest,
  TimelineStateResponse,
  UpdateBoundaryRequest,
  UpdateMusicSettingsRequest,
  UpdateProjectRequest,
  UpdateSceneEffectRequest,
  UpdateSubtitleCueRequest,
  UserProfile,
} from "@pace/shared-types";

const isTunnelHost =
  typeof window !== "undefined" &&
  window.location.hostname.endsWith(".trycloudflare.com");

export const API_BASE_URL =
  process.env.NEXT_PUBLIC_API_URL ||
  (typeof window !== "undefined"
    ? isTunnelHost
      ? ""
      : `${window.location.protocol}//${window.location.hostname}:8080`
    : "http://localhost:8080");

export class ApiError extends Error {
  public problem: ProblemDetails;

  constructor(problem: ProblemDetails) {
    super(problem.detail || problem.title || "An unexpected error occurred");
    this.name = "ApiError";
    this.problem = problem;
  }

  get code() {
    return this.problem.code;
  }

  get status() {
    return this.problem.status;
  }
}

// In-memory + sessionStorage CSRF token cache
let cachedCsrfToken: string | null = null;
let unauthorizedHandler: (() => void) | null = null;

export function setUnauthorizedHandler(handler: (() => void) | null) {
  unauthorizedHandler = handler;
}

export function getCsrfToken(): string | null {
  if (cachedCsrfToken) return cachedCsrfToken;
  if (typeof window !== "undefined") {
    cachedCsrfToken = sessionStorage.getItem("pace_csrf_token");
  }
  return cachedCsrfToken;
}

export function setCsrfToken(token: string | null) {
  cachedCsrfToken = token;
  if (typeof window !== "undefined") {
    if (token) {
      sessionStorage.setItem("pace_csrf_token", token);
    } else {
      sessionStorage.removeItem("pace_csrf_token");
    }
  }
}

interface RequestOptions extends RequestInit {
  expectedVersion?: number;
}

async function request<T>(
  path: string,
  options: RequestOptions = {}
): Promise<T> {
  const url = `${API_BASE_URL}${path}`;
  const headers = new Headers(options.headers || {});

  if (!headers.has("Content-Type") && options.body) {
    headers.set("Content-Type", "application/json");
  }

  // Include CSRF token for state-mutating HTTP methods
  const method = (options.method || "GET").toUpperCase();
  if (["POST", "PUT", "PATCH", "DELETE"].includes(method)) {
    const csrf = getCsrfToken();
    if (csrf) {
      headers.set("X-PACE-CSRF", csrf);
    }
  }

  // Include If-Match header for optimistic concurrency control
  if (options.expectedVersion !== undefined) {
    headers.set("If-Match", `"${options.expectedVersion}"`);
  }

  const response = await fetch(url, {
    ...options,
    headers,
    credentials: "include", // Transmit HttpOnly pace_session cookie
  });

  if (response.status === 204) {
    return {} as T;
  }

  const contentType = response.headers.get("content-type") || "";

  if (!response.ok) {
    let problem: ProblemDetails;
    if (contentType.includes("application/problem+json") || contentType.includes("application/json")) {
      problem = await response.json();
    } else {
      const errorText = await response.text();
      problem = {
        type: "https://pace.video/errors/INTERNAL_SERVER_ERROR",
        title: "Request Failed",
        status: response.status,
        detail: errorText || response.statusText,
        code: "INTERNAL_SERVER_ERROR",
      };
    }

    if (response.status === 401 && path !== "/api/v1/auth/login" && path !== "/api/v1/auth/me") {
      setCsrfToken(null);
      unauthorizedHandler?.();
    }

    throw new ApiError(problem);
  }

  const csrfHeader = response.headers.get("x-pace-csrf");
  if (csrfHeader) {
    setCsrfToken(csrfHeader);
  }

  return response.json();
}

export const api = {
  auth: {
    login: async (body: LoginRequest): Promise<LoginResponse> => {
      const data = await request<LoginResponse>("/api/v1/auth/login", {
        method: "POST",
        body: JSON.stringify(body),
      });
      if (data.csrf_token) {
        setCsrfToken(data.csrf_token);
      }
      return data;
    },
    logout: async (): Promise<MessageResponse> => {
      try {
        return await request<MessageResponse>("/api/v1/auth/logout", {
          method: "POST",
        });
      } finally {
        setCsrfToken(null);
      }
    },
    me: async (): Promise<UserProfile> => {
      const data = await request<UserProfile>("/api/v1/auth/me", {
        method: "GET",
      });
      if (data.csrf_token) {
        setCsrfToken(data.csrf_token);
      }
      return data;
    },
    changePassword: async (
      body: ChangePasswordRequest
    ): Promise<MessageResponse> => {
      return request<MessageResponse>("/api/v1/auth/password", {
        method: "POST",
        body: JSON.stringify(body),
      });
    },
  },
  projects: {
    list: async (): Promise<ProjectResponse[]> => {
      return request<ProjectResponse[]>("/api/v1/projects", {
        method: "GET",
      });
    },
    get: async (id: string): Promise<ProjectResponse> => {
      return request<ProjectResponse>(`/api/v1/projects/${id}`, {
        method: "GET",
      });
    },
    create: async (body: CreateProjectRequest): Promise<ProjectResponse> => {
      return request<ProjectResponse>("/api/v1/projects", {
        method: "POST",
        body: JSON.stringify(body),
      });
    },
    update: async (
      id: string,
      body: UpdateProjectRequest,
      expectedVersion?: number
    ): Promise<ProjectResponse> => {
      return request<ProjectResponse>(`/api/v1/projects/${id}`, {
        method: "PATCH",
        body: JSON.stringify(body),
        expectedVersion,
      });
    },
    delete: async (id: string): Promise<void> => {
      return request<void>(`/api/v1/projects/${id}`, {
        method: "DELETE",
      });
    },
  },
  scripts: {
    get: async (projectId: string): Promise<ScriptResponse> => {
      return request<ScriptResponse>(`/api/v1/projects/${projectId}/script`, {
        method: "GET",
      });
    },
    save: async (
      projectId: string,
      body: SaveScriptRequest
    ): Promise<ScriptResponse> => {
      return request<ScriptResponse>(`/api/v1/projects/${projectId}/script`, {
        method: "PUT",
        body: JSON.stringify(body),
      });
    },
  },
  uploads: {
    presign: async (
      projectId: string,
      body: PresignUploadRequest
    ): Promise<PresignUploadResponse> => {
      return request<PresignUploadResponse>(
        `/api/v1/projects/${projectId}/uploads/presign`,
        {
          method: "POST",
          body: JSON.stringify(body),
        }
      );
    },
  },
  media: {
    list: async (projectId: string): Promise<MediaResponse[]> => {
      return request<MediaResponse[]>(`/api/v1/projects/${projectId}/media`, {
        method: "GET",
      });
    },
    register: async (
      projectId: string,
      body: RegisterMediaRequest
    ): Promise<MediaResponse> => {
      return request<MediaResponse>(`/api/v1/projects/${projectId}/media`, {
        method: "POST",
        body: JSON.stringify(body),
      });
    },
    delete: async (projectId: string, mediaId: string): Promise<void> => {
      return request<void>(`/api/v1/projects/${projectId}/media/${mediaId}`, {
        method: "DELETE",
      });
    },
    searchStock: async (
      projectId: string,
      q: string,
      perPage = 12
    ): Promise<StockSearchResponse> => {
      const qs = new URLSearchParams({
        q,
        per_page: String(perPage),
      });
      return request<StockSearchResponse>(
        `/api/v1/projects/${projectId}/media/stock/search?${qs.toString()}`,
        { method: "GET" }
      );
    },
    importStock: async (
      projectId: string,
      body: ImportStockMediaRequest
    ): Promise<MediaResponse> => {
      return request<MediaResponse>(
        `/api/v1/projects/${projectId}/media/stock/import`,
        {
          method: "POST",
          body: JSON.stringify(body),
        }
      );
    },
    autoMatchStock: async (
      projectId: string,
      body: AutoMatchStockRequest = {}
    ): Promise<AutoMatchStockResponse> => {
      return request<AutoMatchStockResponse>(
        `/api/v1/projects/${projectId}/media/stock/auto-match`,
        {
          method: "POST",
          body: JSON.stringify(body),
        }
      );
    },
    generateAi: async (
      projectId: string,
      body: GenerateAiMediaRequest
    ): Promise<MediaResponse> => {
      return request<MediaResponse>(
        `/api/v1/projects/${projectId}/media/ai/generate`,
        {
          method: "POST",
          body: JSON.stringify(body),
        }
      );
    },
  },
  assignments: {
    list: async (projectId: string): Promise<AssignmentResponse[]> => {
      return request<AssignmentResponse[]>(
        `/api/v1/projects/${projectId}/assignments`,
        {
          method: "GET",
        }
      );
    },
    auto: async (projectId: string): Promise<AssignmentResponse[]> => {
      return request<AssignmentResponse[]>(
        `/api/v1/projects/${projectId}/assignments/auto`,
        {
          method: "PUT",
        }
      );
    },
    swap: async (
      projectId: string,
      body: SwapSectionsRequest
    ): Promise<void> => {
      return request<void>(`/api/v1/projects/${projectId}/assignments/swap`, {
        method: "PUT",
        body: JSON.stringify(body),
      });
    },
    assign: async (
      projectId: string,
      sectionId: string,
      body: AssignSectionRequest
    ): Promise<AssignmentResponse> => {
      return request<AssignmentResponse>(
        `/api/v1/projects/${projectId}/assignments/${sectionId}`,
        {
          method: "PUT",
          body: JSON.stringify(body),
        }
      );
    },
    unassign: async (projectId: string, sectionId: string): Promise<void> => {
      return request<void>(
        `/api/v1/projects/${projectId}/assignments/${sectionId}`,
        {
          method: "DELETE",
        }
      );
    },
  },
  narration: {
    presign: async (
      projectId: string,
      body: PresignNarrationRequest
    ): Promise<PresignNarrationResponse> => {
      return request<PresignNarrationResponse>(
        `/api/v1/projects/${projectId}/narration/presign`,
        {
          method: "POST",
          body: JSON.stringify(body),
        }
      );
    },
    register: async (
      projectId: string,
      body: RegisterNarrationRequest
    ): Promise<RegisterNarrationResponse> => {
      return request<RegisterNarrationResponse>(
        `/api/v1/projects/${projectId}/narration`,
        {
          method: "POST",
          body: JSON.stringify(body),
        }
      );
    },
    get: async (projectId: string): Promise<NarrationStateResponse> => {
      return request<NarrationStateResponse>(
        `/api/v1/projects/${projectId}/narration`,
        {
          method: "GET",
        }
      );
    },
    peaks: async (projectId: string): Promise<NarrationPeaksResponse> => {
      return request<NarrationPeaksResponse>(
        `/api/v1/projects/${projectId}/narration/peaks`,
        {
          method: "GET",
        }
      );
    },
    realign: async (
      projectId: string,
      body: RealignNarrationRequest = { keep_manual_boundaries: true }
    ): Promise<RegisterNarrationResponse> => {
      return request<RegisterNarrationResponse>(
        `/api/v1/projects/${projectId}/narration/realign`,
        {
          method: "POST",
          body: JSON.stringify(body),
        }
      );
    },
    delete: async (projectId: string): Promise<void> => {
      return request<void>(`/api/v1/projects/${projectId}/narration`, {
        method: "DELETE",
      });
    },
  },
  boundaries: {
    update: async (
      projectId: string,
      index: number,
      body: UpdateBoundaryRequest
    ): Promise<BoundaryDto> => {
      return request<BoundaryDto>(
        `/api/v1/projects/${projectId}/boundaries/${index}`,
        {
          method: "PATCH",
          body: JSON.stringify(body),
        }
      );
    },
  },
  timeline: {
    get: async (projectId: string): Promise<TimelineStateResponse> => {
      return request<TimelineStateResponse>(
        `/api/v1/projects/${projectId}/timeline`,
        {
          method: "GET",
        }
      );
    },
    batchPatchBoundaries: async (
      projectId: string,
      body: BatchUpdateBoundariesRequest
    ): Promise<BoundaryDto[]> => {
      return request<BoundaryDto[]>(
        `/api/v1/projects/${projectId}/boundaries`,
        {
          method: "PATCH",
          body: JSON.stringify(body),
        }
      );
    },
  },
  effects: {
    getRenderSpec: async (projectId: string): Promise<RenderSpecResponse> => {
      return request<RenderSpecResponse>(
        `/api/v1/projects/${projectId}/render-spec`,
        {
          method: "GET",
        }
      );
    },
    updateSceneEffect: async (
      projectId: string,
      sectionId: string,
      body: UpdateSceneEffectRequest
    ): Promise<AssignmentResponse> => {
      return request<AssignmentResponse>(
        `/api/v1/projects/${projectId}/assignments/${sectionId}/effect`,
        {
          method: "PATCH",
          body: JSON.stringify(body),
        }
      );
    },
    reroll: async (projectId: string): Promise<RerollEffectsResponse> => {
      return request<RerollEffectsResponse>(
        `/api/v1/projects/${projectId}/effects/reroll`,
        {
          method: "POST",
        }
      );
    },
  },
  music: {
    presign: async (
      projectId: string,
      body: PresignMusicRequest
    ): Promise<PresignMusicResponse> => {
      return request<PresignMusicResponse>(
        `/api/v1/projects/${projectId}/music/presign`,
        {
          method: "POST",
          body: JSON.stringify(body),
        }
      );
    },
    register: async (
      projectId: string,
      body: RegisterMusicRequest
    ): Promise<MusicStateResponse> => {
      return request<MusicStateResponse>(
        `/api/v1/projects/${projectId}/music`,
        {
          method: "POST",
          body: JSON.stringify(body),
        }
      );
    },
    get: async (projectId: string): Promise<MusicStateResponse> => {
      return request<MusicStateResponse>(
        `/api/v1/projects/${projectId}/music`,
        {
          method: "GET",
        }
      );
    },
    updateSettings: async (
      projectId: string,
      body: UpdateMusicSettingsRequest
    ): Promise<MusicStateResponse> => {
      return request<MusicStateResponse>(
        `/api/v1/projects/${projectId}/music`,
        {
          method: "PATCH",
          body: JSON.stringify(body),
        }
      );
    },
    delete: async (projectId: string): Promise<void> => {
      return request<void>(`/api/v1/projects/${projectId}/music`, {
        method: "DELETE",
      });
    },
  },
  subtitles: {
    get: async (projectId: string): Promise<SubtitlesStateResponse> => {
      return request<SubtitlesStateResponse>(
        `/api/v1/projects/${projectId}/subtitles`,
        {
          method: "GET",
        }
      );
    },
    update: async (
      projectId: string,
      cueId: string,
      body: UpdateSubtitleCueRequest
    ): Promise<SubtitleCueDto> => {
      return request<SubtitleCueDto>(
        `/api/v1/projects/${projectId}/subtitles/${cueId}`,
        {
          method: "PATCH",
          body: JSON.stringify(body),
        }
      );
    },
    updateStyle: async (
      projectId: string,
      body: SubtitleStyleDto
    ): Promise<SubtitleStyleDto> => {
      return request<SubtitleStyleDto>(
        `/api/v1/projects/${projectId}/subtitles/style`,
        {
          method: "PUT",
          body: JSON.stringify(body),
        }
      );
    },
    split: async (
      projectId: string,
      cueId: string,
      body: SplitSubtitleCueRequest = {}
    ): Promise<SubtitleCueDto[]> => {
      return request<SubtitleCueDto[]>(
        `/api/v1/projects/${projectId}/subtitles/${cueId}/split`,
        {
          method: "POST",
          body: JSON.stringify(body),
        }
      );
    },
    merge: async (
      projectId: string,
      body: MergeSubtitleCuesRequest
    ): Promise<SubtitleCueDto[]> => {
      return request<SubtitleCueDto[]>(
        `/api/v1/projects/${projectId}/subtitles/merge`,
        {
          method: "POST",
          body: JSON.stringify(body),
        }
      );
    },
  },
  jobs: {
    get: async (projectId: string, jobId: string): Promise<JobStatusDto> => {
      return request<JobStatusDto>(
        `/api/v1/projects/${projectId}/jobs/${jobId}`,
        {
          method: "GET",
        }
      );
    },
  },
  exports: {
    start: async (
      projectId: string,
      body: StartExportRequest
    ): Promise<ExportResponse> => {
      return request<ExportResponse>(`/api/v1/projects/${projectId}/exports`, {
        method: "POST",
        body: JSON.stringify(body),
      });
    },
    list: async (projectId: string): Promise<ExportResponse[]> => {
      return request<ExportResponse[]>(`/api/v1/projects/${projectId}/exports`, {
        method: "GET",
      });
    },
    get: async (
      projectId: string,
      exportId: string
    ): Promise<ExportResponse> => {
      return request<ExportResponse>(
        `/api/v1/projects/${projectId}/exports/${exportId}`,
        {
          method: "GET",
        }
      );
    },
    retry: async (
      projectId: string,
      exportId: string
    ): Promise<ExportResponse> => {
      return request<ExportResponse>(
        `/api/v1/projects/${projectId}/exports/${exportId}/retry`,
        {
          method: "POST",
        }
      );
    },
    eventsUrl: (projectId: string, exportId: string): string =>
      `${API_BASE_URL}/api/v1/projects/${projectId}/exports/${exportId}/events`,
  },
  extension: {
    createPairingCode: async (): Promise<{ code: string; expires_at: string }> => {
      return request<{ code: string; expires_at: string }>(
        `/api/v1/extension/pairing-code`,
        { method: "POST" }
      );
    },
    listTokens: async (): Promise<
      {
        id: string;
        device_name: string;
        last_project_id: string | null;
        last_used_at: string;
        created_at: string;
      }[]
    > => {
      return request(`/api/v1/extension/tokens`, { method: "GET" });
    },
    revokeToken: async (id: string): Promise<void> => {
      return request<void>(`/api/v1/extension/tokens/${id}`, {
        method: "DELETE",
      });
    },
  },
};

