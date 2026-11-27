export type GenerationMode = "image" | "video";

export interface ExtensionConfig {
  apiUrl: string;
  token: string;
  userEmail: string;
  selectedProjectId: string;
  selectedProjectName: string;
}

export interface ProjectSummary {
  id: string;
  name: string;
  aspect_ratio: "16:9" | "9:16";
  updated_at: string;
}

export interface PromptQueueItem {
  id: string;
  label: string; // e.g. "S01" or "P01"
  prompt: string;
  mode: GenerationMode;
  status: "pending" | "running" | "done" | "error" | "skipped";
  error?: string;
}

export interface QueueState {
  status: "idle" | "running" | "paused_credits" | "paused_user" | "completed";
  mode: GenerationMode;
  autoSendToPace: boolean;
  autoDownloadVideos: boolean;
  cooldownSec: number;
  timeoutSec: number;
  currentIndex: number;
  items: PromptQueueItem[];
  lastMessage: string;
}

export interface RecentSendItem {
  filename: string;
  projectName: string;
  kind: "image" | "video";
  prompt?: string;
  duplicate?: boolean;
  sentAt: string;
}

export interface CapturedMediaPayload {
  filename: string;
  mime: string;
  bytesBase64?: string;
  sourceUrl?: string;
  expectedKind?: "image" | "video";
  width?: number;
  height?: number;
  prompt?: string;
  projectId?: string;
  downloadLocally?: boolean;
}

export type RuntimeMessage =
  | { type: "PACE_GET_STATE" }
  | { type: "PACE_PAIR"; apiUrl: string; code: string }
  | { type: "PACE_DISCONNECT" }
  | { type: "PACE_SELECT_PROJECT"; projectId: string; projectName: string }
  | { type: "PACE_REFRESH_PROJECTS" }
  | { type: "PACE_FETCH_PROJECT_PROMPTS"; projectId: string }
  | { type: "PACE_UPLOAD_MEDIA"; payload: CapturedMediaPayload }
  | { type: "PACE_TRUSTED_CLICK"; x: number; y: number }
  | { type: "PACE_TRUSTED_TYPE"; x: number; y: number; text: string };

export const DEFAULT_API_URL = "http://localhost:8080";

export const DEFAULT_QUEUE_STATE: QueueState = {
  status: "idle",
  mode: "image",
  autoSendToPace: true,
  autoDownloadVideos: true,
  cooldownSec: 4,
  timeoutSec: 90,
  currentIndex: 0,
  items: [],
  lastMessage: "Ready. Load prompts from PACE or paste your own.",
};
