import { defineConfig } from "vitest/config";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [tailwindcss()],
  test: {
    include: ["src/**/*.spec.ts", "src/**/*.test.ts"],
  },
});
