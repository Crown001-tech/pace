import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, ApiError, setCsrfToken } from "@/lib/api-client";
import type {
  ChangePasswordRequest,
  LoginRequest,
  UserProfile,
} from "@pace/shared-types";

const OFFLINE_FLAG_KEY = "pace_offline_studio";
const LEGACY_DEMO_USER_KEY = "pace_demo_user";

export const OFFLINE_STUDIO_ENABLED =
  process.env.NODE_ENV !== "production" ||
  process.env.NEXT_PUBLIC_ENABLE_OFFLINE_STUDIO === "true";

export const DEMO_ADMIN_USER: UserProfile = {
  id: "00000000-0000-0000-0000-000000000001",
  email: "offline@pace.local",
  name: "Offline Studio",
  role: "creator",
  storage_quota_bytes: 107374182400,
  storage_used_bytes: 1258291200,
  must_change_password: false,
};

function clearLocalAuthArtifacts() {
  setCsrfToken(null);
  if (typeof window !== "undefined") {
    localStorage.removeItem(OFFLINE_FLAG_KEY);
    localStorage.removeItem(LEGACY_DEMO_USER_KEY);
  }
}

export function isOfflineStudioActive(): boolean {
  if (!OFFLINE_STUDIO_ENABLED || typeof window === "undefined") return false;
  return localStorage.getItem(OFFLINE_FLAG_KEY) === "1";
}

export function useUser() {
  return useQuery<UserProfile | null, ApiError>({
    queryKey: ["auth", "user"],
    queryFn: async () => {
      // If user is currently running in Offline Studio mode, maintain demo identity
      if (isOfflineStudioActive()) {
        return DEMO_ADMIN_USER;
      }

      try {
        const me = await api.auth.me();
        if (typeof window !== "undefined") {
          localStorage.removeItem(OFFLINE_FLAG_KEY);
          localStorage.removeItem(LEGACY_DEMO_USER_KEY);
        }
        return me;
      } catch (err) {
        if (err instanceof ApiError) {
          if (err.code === "PASSWORD_CHANGE_REQUIRED") {
            return {
              id: "pending-password-change",
              email: "",
              name: "Password Update Required",
              role: "creator",
              must_change_password: true,
              storage_quota_bytes: 0,
              storage_used_bytes: 0,
            };
          }
          clearLocalAuthArtifacts();
          return null;
        }

        if (isOfflineStudioActive()) {
          return DEMO_ADMIN_USER;
        }

        return null;
      }
    },
    retry: false,
    staleTime: 1000 * 60 * 5,
  });
}

export function useLogin() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (data: LoginRequest) => {
      const res = await api.auth.login({
        email: data.email.trim(),
        password: data.password,
      });
      if (typeof window !== "undefined") {
        localStorage.removeItem(OFFLINE_FLAG_KEY);
        localStorage.removeItem(LEGACY_DEMO_USER_KEY);
      }
      return res;
    },
    onSuccess: (res) => {
      queryClient.setQueryData(["auth", "user"], res.user);
    },
  });
}

export function useDevLogin() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async () => {
      if (!OFFLINE_STUDIO_ENABLED) {
        throw new Error("Offline Studio is disabled in this environment.");
      }
      setCsrfToken(null);
      if (typeof window !== "undefined") {
        localStorage.setItem(OFFLINE_FLAG_KEY, "1");
        localStorage.removeItem(LEGACY_DEMO_USER_KEY);
      }
      return DEMO_ADMIN_USER;
    },
    onSuccess: (user) => {
      queryClient.setQueryData(["auth", "user"], user);
    },
  });
}

export function useLogout() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async () => {
      const wasOffline = isOfflineStudioActive();
      clearLocalAuthArtifacts();
      if (typeof window !== "undefined") {
        localStorage.removeItem("pace_active_project_id");
      }
      if (wasOffline) {
        return { status: "ok", message: "Exited offline studio" };
      }
      try {
        return await api.auth.logout();
      } catch {
        return { status: "ok", message: "Signed out locally" };
      }
    },
    onSuccess: () => {
      queryClient.setQueryData(["auth", "user"], null);
      queryClient.clear();
    },
  });
}

export function useChangePassword() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (data: ChangePasswordRequest) => api.auth.changePassword(data),
    onSuccess: async () => {
      try {
        const fresh = await api.auth.me();
        queryClient.setQueryData(["auth", "user"], fresh);
      } catch {
        queryClient.setQueryData<UserProfile | null>(["auth", "user"], (prev) =>
          prev ? { ...prev, must_change_password: false } : prev
        );
      }
      queryClient.invalidateQueries({ queryKey: ["auth", "user"] });
    },
  });
}
