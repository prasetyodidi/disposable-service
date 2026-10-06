import { defineConfig } from 'astro/config';
import react from '@astrojs/react';
import tailwindcss from '@tailwindcss/vite';

// https://astro.build/config
export default defineConfig({
  integrations: [react()],
  vite: {
    plugins: [tailwindcss()],
  },
  output: 'static',
  build: {
    format: 'file',
  },
  server: {
    port: 3000,
    host: true,
  },
});
