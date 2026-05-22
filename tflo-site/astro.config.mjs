import { defineConfig } from "astro/config";
import tailwindcss from "@tailwindcss/vite";
import mdx from "@astrojs/mdx";
import react from "@astrojs/react";

// https://astro.build/config
export default defineConfig({
  site: "https://tflo.dev",
  integrations: [mdx(), react()],
  vite: {
    plugins: [tailwindcss()],
    // Vite 8 + rolldown + react-refresh bug workaround:
    // The builtin:vite-react-refresh-wrapper throws "Missing field moduleType"
    // on virtual modules and non-JS transforms. Calling build() before dev
    // avoids the issue entirely.
  },
  markdown: {
    shikiConfig: {
      theme: "github-dark",
      wrap: true,
    },
  },
});
