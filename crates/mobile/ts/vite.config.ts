import { defineConfig } from "vite";

export default defineConfig({
  build: {
    outDir: process.env.VITE_OUT_DIR ?? "dist",
    emptyOutDir: true,
    lib: {
      entry: "src/index.ts",
      formats: ["iife"],
      name: "__mt_note_mobileBundle",
      fileName: () => "mt-note-mobile-ts.js",
    },
    cssCodeSplit: false,
    sourcemap: false,
    minify: "esbuild",
    target: "es2020",
  },
});