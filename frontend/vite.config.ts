import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// The dev server proxies /api to the Actix backend so the browser can use
// same-origin relative URLs during development. Override the backend location
// with VITE_PROXY_TARGET if it isn't on the default port 8080.
const proxyTarget = process.env.VITE_PROXY_TARGET ?? "http://127.0.0.1:8080";

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      "/api": {
        target: proxyTarget,
        changeOrigin: true,
      },
    },
  },
});
