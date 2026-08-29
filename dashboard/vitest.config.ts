import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import path from "node:path";

/**
 * Unit / integration test configuration for the dashboard.
 *
 * Playwright owns `e2e/` and drives a real browser; vitest owns everything
 * else and runs in jsdom. `exclude` keeps the two from colliding — Playwright
 * specs use `test.describe` from `@playwright/test` and fail under vitest.
 */
export default defineConfig({
  plugins: [react()],
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./vitest.setup.ts"],
    include: ["{app,components,lib,hooks}/**/*.{test,spec}.{ts,tsx}"],
    exclude: ["e2e/**", "node_modules/**", ".next/**"],
    coverage: {
      reporter: ["text", "html"],
      include: ["lib/**", "components/**"],
    },
  },
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "."),
    },
  },
});
