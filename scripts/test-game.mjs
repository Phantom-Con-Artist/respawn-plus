// SPDX-License-Identifier: MIT
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { createWriteStream } from 'node:fs';
import { access, copyFile, mkdir, readFile, rename, writeFile } from 'node:fs/promises';
import { homedir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const game = JSON.parse(await readFile(join(root, 'test-games/mines.json'), 'utf8'));
const data = process.env.XDG_DATA_HOME || join(homedir(), '.local/share');
const state = process.env.XDG_STATE_HOME || join(homedir(), '.local/state');
const installation = join(data, 'respawn-plus/test-games', game.id, game.version);
const executable = join(installation, game.executable);
const prefix = resolve(process.env.RESPAWN_TEST_PREFIX || join(data, 'respawn-plus/prefixes/mines-test'));
const logs = join(state, 'respawn-plus/logs/mines');

function verify(bytes) {
  if (bytes.length !== game.size_bytes || createHash('sha256').update(bytes).digest('hex') !== game.sha256) {
    throw new Error('Mines checksum/size mismatch. The upstream download may have changed; review and update the manifest explicitly.');
  }
  if (bytes[0] !== 0x4d || bytes[1] !== 0x5a) throw new Error('Expected a Windows executable');
}

async function install() {
  let existing;
  try { existing = await readFile(executable); }
  catch (error) { if (error.code !== 'ENOENT') throw error; }
  await mkdir(installation, { recursive: true });
  if (existing) {
    verify(existing);
  } else {
    const response = await fetch(game.download_url, { signal: AbortSignal.timeout(120_000) });
    if (!response.ok || !response.url.startsWith('https://')) throw new Error(`Game download failed: ${response.status}`);
    const bytes = Buffer.from(await response.arrayBuffer());
    verify(bytes);
    const temporary = `${executable}.download-${process.pid}`;
    await writeFile(temporary, bytes, { flag: 'wx', mode: 0o644 });
    await rename(temporary, executable);
  }
  await copyFile(join(root, 'test-games/licenses/mines-LICENCE.txt'), join(installation, 'LICENCE.txt'));
  console.log(`Installed and SHA-256 verified: ${game.name} ${game.version}`);
  console.log(executable);
}

async function protonPath() {
  const supplied = process.env.RESPAWN_PROTON_PATH;
  const candidates = supplied ? [resolve(supplied)] : [
    join(data, 'Steam/compatibilitytools.d', game.tested_proton),
    join(homedir(), '.steam/root/compatibilitytools.d', game.tested_proton),
    join(homedir(), '.var/app/com.valvesoftware.Steam/data/Steam/compatibilitytools.d', game.tested_proton),
  ];
  for (const directory of candidates) {
    try { await access(join(directory, 'proton')); return directory; } catch { /* Try next installed runner. */ }
  }
  throw new Error(`No test Proton runner found. Set RESPAWN_PROTON_PATH to an installed Proton directory (tested: ${game.tested_proton}).`);
}

async function play(args) {
  verify(await readFile(executable));
  const proton = await protonPath();
  await mkdir(dirname(prefix), { recursive: true });
  await mkdir(logs, { recursive: true });
  const logfile = join(logs, `mines-${Date.now()}-${process.pid}.log`);
  const log = createWriteStream(logfile, { flags: 'wx' });
  log.on('error', (error) => console.error(`Log write failed: ${error.message}`));
  const localUmu = join(homedir(), '.local/bin/umu-run');
  let umu = process.env.RESPAWN_UMU_PATH;
  if (!umu) {
    try { await access(localUmu); umu = localUmu; } catch { umu = 'umu-run'; }
  }
  console.log(`Starting Windows Mines with ${proton}\nPrefix: ${prefix}\nLog: ${logfile}`);
  const child = spawn(umu, [executable, ...args], {
    cwd: installation,
    env: {
      ...process.env,
      WINEPREFIX: prefix,
      PROTONPATH: proton,
      GAMEID: '0',
      PROTON_LOG: '1',
      PROTON_LOG_DIR: logs,
      UMU_LOG: 'info',
    },
    stdio: ['inherit', 'pipe', 'pipe'],
  });
  child.stdout.on('data', (chunk) => { process.stdout.write(chunk); log.write(chunk); });
  child.stderr.on('data', (chunk) => { process.stderr.write(chunk); log.write(chunk); });
  const interrupt = () => child.kill('SIGINT');
  process.on('SIGINT', interrupt);
  try {
    return await new Promise((resolveExit, reject) => {
      child.once('error', reject);
      child.once('close', (code) => resolveExit(code ?? 1));
    });
  } finally {
    process.removeListener('SIGINT', interrupt);
    log.end();
  }
}

try {
  const [command, ...args] = process.argv.slice(2);
  if (command === 'install') await install();
  else if (command === 'play') process.exitCode = await play(args);
  else if (command === 'status') {
    verify(await readFile(executable));
    console.log(JSON.stringify({ ...game, installation, executable, prefix, logs, proton: await protonPath() }, null, 2));
  } else throw new Error('Usage: node scripts/test-game.mjs install|play|status [game arguments]');
} catch (error) {
  console.error(`Respawn+ test game: ${error.message}`);
  console.error('Run npm run test-game:setup first; see docs/test-game.md for setup and runtime overrides.');
  process.exitCode = 1;
}
