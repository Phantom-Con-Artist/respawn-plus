// SPDX-License-Identifier: MIT
import { defineConfig } from 'vite';

export default defineConfig({
  clearScreen: false,
  server: { port: 1420, strictPort: true, watch: { ignored: ['**/.room-lab/**', '**/.taste-skill/**', '**/src-tauri/target/**'] } },
  envPrefix: ['VITE_', 'TAURI_']
});

