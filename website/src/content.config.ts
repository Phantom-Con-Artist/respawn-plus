// SPDX-License-Identifier: MIT
// The documentation is the repository's own docs/ folder, so the site never drifts from the code.
import { defineCollection } from 'astro:content';
import { glob } from 'astro/loaders';

const docs = defineCollection({
  loader: glob({ pattern: '*.md', base: '../docs' }),
});

export const collections = { docs };
