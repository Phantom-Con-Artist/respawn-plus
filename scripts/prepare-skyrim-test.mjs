// SPDX-License-Identifier: MIT
// Local validation setup only. Uses the user's archives; does not redistribute SKSE/SkyUI.
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { constants } from 'node:fs';
import { copyFile, lstat, mkdir, mkdtemp, readFile, readdir, realpath, writeFile } from 'node:fs/promises';
import { homedir, tmpdir } from 'node:os';
import { dirname, join, relative, resolve } from 'node:path';

const [gameArgument, prefixArgument, skseArchive, skyuiArchive] = process.argv.slice(2);
if (!gameArgument || !prefixArgument || !skseArchive || !skyuiArchive) {
  throw new Error('Usage: node scripts/prepare-skyrim-test.mjs GAME_FOLDER PREFIX SKSE_7Z SKYUI_ZIP');
}
const game = await realpath(gameArgument);
const prefix = await realpath(prefixArgument);
const versionInfo = spawnSync('7z', ['l', join(game, 'SkyrimSE.exe')], { encoding: 'utf8' }).stdout;
if (!/FileVersion: 1\.7\.104\.0/.test(versionInfo || '')) throw new Error('This validation recipe requires Skyrim 1.7.104.0');
const stage = await mkdtemp(join(tmpdir(), 'vessel-skyrim-stage-'));
const backup = join(process.env.XDG_DATA_HOME || join(homedir(), '.local/share'), 'vessel/backups', 'skyrim-test-' + Date.now());
await mkdir(backup, { recursive: true });
const manifest = { game, prefix, created_at: new Date().toISOString(), stage, archives: [], entries: [], complete: false };
const manifestPath = join(backup, 'manifest.json');
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');
const record = () => writeFile(manifestPath, JSON.stringify(manifest, null, 2) + '\n');
await record();

async function deploy(target, bytes) {
  target = resolve(target);
  if (!target.startsWith(game + '/') && !target.startsWith(prefix + '/')) throw new Error('Destination is outside the game and its test prefix');
  let previous = null;
  try {
    const kind = await lstat(target);
    if (!kind.isFile() || kind.isSymbolicLink()) throw new Error('Refusing to replace a non-regular file: ' + target);
    previous = await readFile(target);
  } catch (error) { if (error.code !== 'ENOENT') throw error; }
  const entry = { target, existed: !!previous, previous_sha256: previous ? hash(previous) : null,
    deployed_sha256: hash(bytes), backup: previous ? 'original-' + manifest.entries.length : null, state: 'pending' };
  if (previous) await copyFile(target, join(backup, entry.backup), constants.COPYFILE_EXCL);
  manifest.entries.push(entry); await record();
  await mkdir(dirname(target), { recursive: true });
  const parent = await realpath(dirname(target));
  if (!parent.startsWith(game + '/') && parent !== game && !parent.startsWith(prefix + '/')) throw new Error('Destination parent resolves outside the test environment');
  await writeFile(target, bytes);
  entry.state = 'deployed'; await record();
}

function extract(archive, output, members) {
  const result = spawnSync('7z', ['x', '-y', '-o' + output, resolve(archive), ...members], { encoding: 'utf8' });
  if (result.status !== 0) throw new Error('Archive extraction failed: ' + result.stderr + result.stdout);
}
extract(skseArchive, join(stage, 'skse'), ['skse64_2_03_01/skse64_loader.exe', 'skse64_2_03_01/skse64_1_7_104.dll', 'skse64_2_03_01/Data/Scripts/*.pex']);
extract(skyuiArchive, join(stage, 'skyui'), ['SkyUI_SE.bsa', 'SkyUI_SE.esp']);
for (const archive of [skseArchive, skyuiArchive]) manifest.archives.push({ path: resolve(archive), sha256: hash(await readFile(archive)) });
await record();
const skse = join(stage, 'skse/skse64_2_03_01');
for (const name of ['skse64_loader.exe', 'skse64_1_7_104.dll']) await deploy(join(game, name), await readFile(join(skse, name)));
for (const name of await readdir(join(skse, 'Data/Scripts'))) {
  if (name.endsWith('.pex')) await deploy(join(game, 'Data/Scripts', name), await readFile(join(skse, 'Data/Scripts', name)));
}
for (const name of ['SkyUI_SE.bsa', 'SkyUI_SE.esp']) await deploy(join(game, 'Data', name), await readFile(join(stage, 'skyui', name)));

const local = join(prefix, 'drive_c/users/steamuser/AppData/Local/Skyrim Special Edition');
await mkdir(local, { recursive: true });
const pluginName = (await readdir(local)).find((name) => name.toLowerCase() === 'plugins.txt') || 'Plugins.txt';
const pluginPath = join(local, pluginName);
let plugins = '';
try { plugins = await readFile(pluginPath, 'utf8'); } catch (error) { if (error.code !== 'ENOENT') throw error; }
plugins = plugins.split(/\r?\n/).filter((line) => !/^\*?SkyUI_SE\.esp$/i.test(line.trim())).join('\r\n').trimEnd() + '\r\n*SkyUI_SE.esp\r\n';
await deploy(pluginPath, Buffer.from(plugins));

const prefsPath = join(prefix, 'drive_c/users/steamuser/Documents/My Games/Skyrim Special Edition/SkyrimPrefs.ini');
let prefs = await readFile(prefsPath, 'utf8');
for (const [key, value] of Object.entries({ 'iSize W': '1280', 'iSize H': '720', bFullScreen: '0', 'bFull Screen': '0', bBorderless: '0', iShadowMapResolution: '1024', fShadowDistance: '3000.0000', bVolumetricLightingEnable: '0', bSAOEnable: '0' })) {
  const escaped = key.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  prefs = prefs.replace(new RegExp('^' + escaped + '=[^\\r\\n]*', 'm'), key + '=' + value);
}
await deploy(prefsPath, Buffer.from(prefs));
manifest.complete = true; await record();
console.log(JSON.stringify({ backup_manifest: manifestPath, files_deployed: manifest.entries.length, skse_loader: join(game, 'skse64_loader.exe'), plugin: 'SkyUI_SE.esp', window: '1280x720' }, null, 2));
