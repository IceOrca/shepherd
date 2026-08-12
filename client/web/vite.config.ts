import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

const proxyTarget = process.env.VITE_SHEPHERD_DEV_PROXY_TARGET ?? "http://127.0.0.1:8000";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    proxy: {
      "/business": {
        target: proxyTarget,
        changeOrigin: true,
      },
      "/api": {
        target: proxyTarget,
        changeOrigin: true,
      },
      "/auth": {
        target: proxyTarget,
        changeOrigin: true,
      },
    },
  },
});
