// SPDX-License-Identifier: MIT
import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { readFile, realpath, stat } from 'node:fs/promises';
import { isAbsolute, relative, resolve, sep } from 'node:path';
import { pathToFileURL } from 'node:url';

const required = ['wine', 'proton', 'umu', 'steam-runtime'];

export async function validateRuntimeBundle(directory) {
  const base = await realpath(directory);
  const manifest = JSON.parse(await readFile(resolve(base, 'manifest.json'), 'utf8'));
  if (manifest.format_version !== 1 || manifest.architecture !== 'x86_64') {
    throw new Error('Expected runtime manifest format 1 for x86_64');
  }
  if (!Array.isArray(manifest.components)) throw new Error('Missing components array');
  const seen = new Set();
  for (const component of manifest.components) {
    if (typeof component.id !== 'string' || !/^[a-z][a-z0-9-]*$/.test(component.id) || seen.has(component.id)) {
      throw new Error('Invalid or duplicate runtime component id');
    }
    seen.add(component.id);
    if (typeof component.version !== 'string' || !component.version.trim() || /latest|REPLACE|VERSION|TODO/i.test(component.version)) {
      throw new Error(`${component.id}: provide a pinned version`);
    }
    if (typeof component.upstream_url !== 'string' || !component.upstream_url.startsWith('https://')) {
      throw new Error(`${component.id}: provide the upstream HTTPS release URL`);
    }
    for (const field of ['archive', 'license', 'source_archive']) {
      const name = component[field];
      const digest = component[`${field}_sha256`];
      if (typeof name !== 'string' || !name || isAbsolute(name)) {
        throw new Error(`${component.id}: missing relative ${field} path`);
      }
      if (typeof digest !== 'string' || !/^[a-f0-9]{64}$/.test(digest)) {
        throw new Error(`${component.id}: invalid ${field} SHA-256`);
      }
      const target = await realpath(resolve(base, name));
      const location = relative(base, target);
      if (location === '..' || location.startsWith(`..${sep}`) || isAbsolute(location)) {
        throw new Error(`${component.id}: ${field} escapes the runtime bundle`);
      }
      if (!(await stat(target)).isFile()) throw new Error(`${component.id}: ${field} must be a file`);
      const hash = createHash('sha256');
      for await (const chunk of createReadStream(target)) hash.update(chunk);
      if (hash.digest('hex') !== digest) throw new Error(`${component.id}: ${field} checksum mismatch`);
    }
  }
  for (const id of required) {
    if (!seen.has(id)) throw new Error(`Full installer is missing required component: ${id}`);
  }
  return manifest;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try {
    const manifest = await validateRuntimeBundle(process.argv[2] ?? 'build/runtime-bundle');
    console.log(`Respawn+ Full: verified ${manifest.components.length} runtime records and their binary/license/source checksums.`);
  } catch (error) {
    console.error(`Cannot build Respawn+ Full: ${error.message}`);
    console.error('Prepare build/runtime-bundle using docs/distribution.md, or build Lite with npm run bundle:lite.');
    process.exitCode = 1;
  }
}
