import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';
import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [
    tailwindcss(),
    sveltekit({
      preprocess: vitePreprocess(),
      adapter: adapter({
        pages: 'build',
        assets: 'build',
        fallback: 'index.html',
        precompress: false,
        strict: true
      }),
      paths: { base: '', relative: false },
      prerender: { entries: [] }
    })
  ],
  build: {
    target: 'es2022',
    minify: 'oxc',
    cssCodeSplit: true,
  },
  server: {
    port: 1420,
    strictPort: true
  },
  clearScreen: false,
  envPrefix: ['TAURI_', 'VITE_']
});
