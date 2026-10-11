import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// Relative base: the Rust server embeds dist/ and serves it at "/" (and could mount it elsewhere).
export default defineConfig({
  plugins: [react()],
  base: "./",
  server: { port: 5174, host: true },
  build: { target: "es2020", outDir: "dist", sourcemap: false }, // iOS Safari 14+/Android Chrome
});
