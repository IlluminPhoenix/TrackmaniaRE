import { defineConfig } from "vitest/config";

export default defineConfig({
  // Relative asset URLs so a build works from any GitHub Pages subpath.
  base: "./",
  build: {
    target: "es2022",
    sourcemap: false,
    reportCompressedSize: false,
  },
  server: {
    port: 5173,
  },
  test: {
    environment: "node",
    include: ["tests/**/*.test.ts"],
  },
});
