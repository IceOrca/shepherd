import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

const proxyTarget = process.env.VITE_SHEPHERD_DEV_PROXY_TARGET ?? "http://127.0.0.1:8000";
const authProxyTarget =
  process.env.VITE_SHEPHERD_AUTH_DEV_PROXY_TARGET ?? "http://127.0.0.1:9999";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    proxy: {
      "/api": {
        target: proxyTarget,
        changeOrigin: true,
      },
      "/auth": {
        target: authProxyTarget,
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/auth\/v1/, ""),
      },
    },
  },
});
