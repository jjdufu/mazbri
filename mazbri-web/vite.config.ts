import fs from "node:fs";
import path from "node:path";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig, loadEnv } from "vite";
import viteCompression from "vite-plugin-compression";

function readWorkspaceVersion(): string {
  try {
    const raw = fs.readFileSync(path.resolve(__dirname, "../VERSION"), "utf-8");
    const parsed = raw.split("#")[0]?.trim();
    return parsed || "0.0.0";
  } catch {
    return "0.0.0";
  }
}

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd());
  const apiProxyTarget =
    env.VITE_API_PROXY_URL ||
    process.env.VITE_API_PROXY_URL ||
    "http://127.0.0.1:8085";
  const baseUrl = env.VITE_BASE_URL || "/";
  const port = Number(env.VITE_PORT) || 5173;
  const appVersion = readWorkspaceVersion();

  return {
    base: baseUrl,
    define: {
      __APP_VERSION__: JSON.stringify(appVersion),
    },
    plugins: [
      react(),
      tailwindcss(),
      viteCompression({
        verbose: false,
        disable: false,
        algorithm: "gzip",
        ext: ".gz",
        threshold: 10240,
        deleteOriginFile: false,
      }),
    ],
    resolve: {
      alias: {
        "@": path.resolve(__dirname, "./src"),
      },
    },
    server: {
      port,
      host: true,
      proxy: {
        "/api": {
          target: apiProxyTarget,
          changeOrigin: true,
        },
        "/health": {
          target: apiProxyTarget,
          changeOrigin: true,
        },
      },
    },
    preview: {
      port: 4173,
      proxy: {
        "/api": {
          target: apiProxyTarget,
          changeOrigin: true,
        },
        "/health": {
          target: apiProxyTarget,
          changeOrigin: true,
        },
      },
    },
    build: {
      target: "es2020",
      outDir: "dist",
      chunkSizeWarningLimit: 2000,
    },
  };
});
