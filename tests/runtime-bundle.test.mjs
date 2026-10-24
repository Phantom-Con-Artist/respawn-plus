// SPDX-License-Identifier: MIT
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtemp, writeFile, rm, symlink } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { validateRuntimeBundle } from '../scripts/check-runtime-bundle.mjs';

async function fixture(t) {
  const base = await mkdtemp(join(tmpdir(), 'vessel-bundle-test-'));
  t.after(() => rm(base, { recursive: true, force: true }));
  const payload = 'runtime-fixture';
  const digest = createHash('sha256').update(payload).digest('hex');
  const components = [];
  for (const id of ['wine', 'proton', 'umu', 'steam-runtime']) {
    const component = { id, version: '1.2.3', upstream_url: `https://example.org/${id}/1.2.3` };
    for (const field of ['archive', 'license', 'source_archive']) {
      component[field] = `${id}-${field}.txt`;
      component[`${field}_sha256`] = digest;
      await writeFile(join(base, component[field]), payload);
    }
    components.push(component);
  }
  const manifest = { format_version: 1, architecture: 'x86_64', components };
  await writeFile(join(base, 'manifest.json'), JSON.stringify(manifest));
  return { base, manifest };
}

test('validates a complete bundle and rejects tampered runtime content', async (t) => {
  const { base, manifest } = await fixture(t);
  assert.equal((await validateRuntimeBundle(base)).components.length, 4);
  await writeFile(join(base, manifest.components[0].archive), 'modified');
  await assert.rejects(validateRuntimeBundle(base), /checksum mismatch/);
});

test('requires source materials and every full-edition runtime', async (t) => {
  const { base, manifest } = await fixture(t);
  delete manifest.components[0].source_archive;
  await writeFile(join(base, 'manifest.json'), JSON.stringify(manifest));
  await assert.rejects(validateRuntimeBundle(base), /source_archive/);
  manifest.components.shift();
  await writeFile(join(base, 'manifest.json'), JSON.stringify(manifest));
  await assert.rejects(validateRuntimeBundle(base), /missing required component: wine/);
});

test('rejects source paths and symlinks outside the staged bundle', async (t) => {
  const { base, manifest } = await fixture(t);
  // Point outside the bundle, to an existing file, to test the containment check.
  const outside = await mkdtemp(join(tmpdir(), 'vessel-outside-test-'));
  t.after(() => rm(outside, { recursive: true, force: true }));
  await writeFile(join(outside, 'source'), 'source');
  await symlink(join(outside, 'source'), join(base, 'outside-link'));
  manifest.components[0].source_archive = 'outside-link';
  await writeFile(join(base, 'manifest.json'), JSON.stringify(manifest));
  await assert.rejects(validateRuntimeBundle(base), /escapes the runtime bundle/);
});
