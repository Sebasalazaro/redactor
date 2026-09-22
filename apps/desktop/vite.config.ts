import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vite";

// Two pages, one per window: the review popup and the dashboard.
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 1420, strictPort: true, host: "127.0.0.1" },
  build: {
    target: "safari16",
    rollupOptions: {
      input: {
        review: "review.html",
        dashboard: "dashboard.html",
      },
    },
  },
});
