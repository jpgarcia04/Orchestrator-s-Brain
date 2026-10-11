import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";

// TypeScript dentro de los componentes: `<script lang="ts">`.
export default {
  preprocess: vitePreprocess(),
};
