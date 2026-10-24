// SPDX-License-Identifier: MIT
import { defineConfig } from 'astro/config';

// The docs are the repository's own markdown, where pages link to each other as `library.md`.
// On the site each doc lives at /docs/<name>/, so rewrite those links at build time.
function rehypeDocLinks() {
  const visit = (node) => {
    if (node.type === 'element' && node.tagName === 'a' && typeof node.properties?.href === 'string') {
      const match = node.properties.href.match(/^(?:\.\/)?([\w-]+)\.md(#.*)?$/);
      if (match) node.properties.href = `/docs/${match[1]}/${match[2] ?? ''}`;
    }
    node.children?.forEach(visit);
  };
  return (tree) => visit(tree);
}

// Static output for Cloudflare Pages (project "vessel"); deploy with `npm run deploy -w website`.
export default defineConfig({
  site: 'https://vessel-4cp.pages.dev',
  output: 'static',
  trailingSlash: 'ignore',
  markdown: { rehypePlugins: [rehypeDocLinks] },
});
