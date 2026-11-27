"use client";

import React, { useEffect, useState } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { setUnauthorizedHandler } from "@/lib/api-client";
import { useGlobalThemeSync } from "@/hooks/use-theme";

function GlobalRuntimeBridge({ queryClient }: { queryClient: QueryClient }) {
  useGlobalThemeSync();

  useEffect(() => {
    setUnauthorizedHandler(() => {
      if (typeof window !== "undefined") {
        localStorage.removeItem("pace_offline_studio");
      }
      queryClient.setQueryData(["auth", "user"], null);
    });
    return () => setUnauthorizedHandler(null);
  }, [queryClient]);

  return null;
}

export function QueryProvider({ children }: { children: React.ReactNode }) {
  const [queryClient] = useState(
    () =>
      new QueryClient({
        defaultOptions: {
          queries: {
            staleTime: 1000 * 60,
            retry: false,
            refetchOnWindowFocus: false,
          },
        },
      })
  );

  return (
    <QueryClientProvider client={queryClient}>
      <GlobalRuntimeBridge queryClient={queryClient} />
      {children}
    </QueryClientProvider>
  );
}
