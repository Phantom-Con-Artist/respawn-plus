// SPDX-License-Identifier: MIT
// Reading order and short descriptions for the repository's docs on the site.
export const docOrder = ['library', 'companion-apps', 'test-game', 'distribution', 'environment'];
export const docBlurbs: Record<string, string> = {
  library: 'Find games, choose a runner, launch and read logs.',
  'companion-apps': 'Mod managers, script extenders and trainers in one room.',
  'test-game': 'Check your setup with a small free Windows game.',
  distribution: 'Lite and Full packages, and what each one bundles.',
  environment: 'The machine and versions Respawn+ was verified on.',
};
export const sortDocs = <T extends { id: string }>(docs: T[]) =>
  [...docs].sort((a, b) => (docOrder.indexOf(a.id) + 1 || 99) - (docOrder.indexOf(b.id) + 1 || 99));
