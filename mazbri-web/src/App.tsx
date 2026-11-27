import { useEffect } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "react-router/dom";
import { ToastHost } from "@/components/ui";
import { useThemeStore } from "@/stores/theme";
import { router } from "./router";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: 1,
      refetchOnWindowFocus: false,
      staleTime: 15_000,
    },
  },
});

export function App() {
  useEffect(() => {
    useThemeStore.getState().applyThemeToDom();
  }, []);

  return (
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
      <ToastHost />
    </QueryClientProvider>
  );
}

export default App;
