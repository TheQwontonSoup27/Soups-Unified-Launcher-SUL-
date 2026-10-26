import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Port must match `devUrl` in src-tauri/tauri.conf.json.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
});
