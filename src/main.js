// SPDX-License-Identifier: MIT
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { getCurrentWindow } from '@tauri-apps/api/window';
import '@fontsource-variable/geist';
import '@fontsource-variable/geist-mono';
import './styles.css';
import { voxelMark } from './brand.js';

const $ = (selector) => document.querySelector(selector);
for (const slot of document.querySelectorAll('[data-voxel-mark]')) slot.innerHTML = voxelMark;

// Theme: an explicit choice is remembered per machine; otherwise follow the system.
const systemDark = matchMedia('(prefers-color-scheme: dark)');
function storedTheme() { try { return localStorage.getItem('respawn-theme'); } catch { return null; } }
function applyTheme(theme) {
  document.documentElement.dataset.theme = theme;
  $('#theme-toggle').setAttribute('aria-label', 'Switch to ' + (theme === 'dark' ? 'light' : 'dark') + ' theme');
}
applyTheme(storedTheme() || (systemDark.matches ? 'dark' : 'light'));
systemDark.addEventListener('change', (event) => { if (!storedTheme()) applyTheme(event.matches ? 'dark' : 'light'); });
$('#theme-toggle').addEventListener('click', () => {
  const next = document.documentElement.dataset.theme === 'dark' ? 'light' : 'dark';
  try { localStorage.setItem('respawn-theme', next); } catch { /* theme still applies for this session */ }
  applyTheme(next);
});

// Window controls live in the app's own title bar (decorations are off).
const appWindow = (() => { try { return getCurrentWindow(); } catch { return null; } })();
async function syncMaximized() {
  const maximized = await appWindow?.isMaximized().catch(() => false);
  document.documentElement.dataset.maximized = String(!!maximized);
  $('#win-max').setAttribute('aria-label', maximized ? 'Restore' : 'Maximize');
}
$('#win-min').addEventListener('click', () => appWindow?.minimize());
$('#win-max').addEventListener('click', () => appWindow?.toggleMaximize());
$('#win-close').addEventListener('click', () => appWindow?.close());
appWindow?.onResized(syncMaximized).catch(() => {});
syncMaximized();

let games = [], runtimes = [], filter = 'all', dirty = false;
let view = { name: 'home', id: null };
let selectedFormId = null, busy = false, refreshing = false, ready = false;
let detailTab = 'overview', editingToolId = null, toolGameId = null, removingToolId = null, logTarget = null;

// ---------- Formatting ----------
function message(text, error = false) {
  $('#message').textContent = text;
  $('#message').hidden = !text;
  $('#message').dataset.error = String(error);
}
// SQLite CURRENT_TIMESTAMP values are UTC without a zone marker.
function parseTime(value) { return value ? new Date(value.replace(' ', 'T') + 'Z') : null; }
function duration(seconds) {
  if (seconds == null) return '—';
  if (seconds < 60) return '<1 m';
  const hours = Math.floor(seconds / 3600), minutes = Math.round((seconds % 3600) / 60);
  return hours ? hours + ' h' + (minutes ? ' ' + minutes + ' m' : '') : minutes + ' m';
}
function relative(value) {
  const date = parseTime(value);
  if (!date) return 'Never';
  const minutes = Math.round((Date.now() - date) / 60000);
  if (minutes < 1) return 'Just now';
  if (minutes < 60) return minutes + ' min ago';
  if (minutes < 24 * 60) return Math.round(minutes / 60) + ' h ago';
  if (minutes < 48 * 60) return 'Yesterday';
  return date.toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: date.getFullYear() === new Date().getFullYear() ? undefined : 'numeric' });
}
function clock(value) { return parseTime(value)?.toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' }) ?? ''; }
const MODE_NAMES = { default: 'Game default', fullscreen: 'Fullscreen', borderless: 'Borderless', windowed: 'Windowed' };
function displayLabel(game) {
  if (!game.display_supported) return 'In-game setting';
  return MODE_NAMES[game.display_mode] + (game.display_mode !== 'default' && game.display_width ? ' · ' + game.display_width + '×' + game.display_height : '');
}

// ---------- Game state ----------
function current() { return view.name === 'game' ? games.find((g) => g.id === view.id) : undefined; }
function running(item) { return item.launch?.state === 'running'; }
function gameSessionRunning(game) { return running(game) || (game.tools || []).some((tool) => tool.kind === 'script_extender' && running(tool)); }
function environmentBusy(game) { return running(game) || (game.tools || []).some(running); }
function canPlay(game) { return !busy && !dirty && !gameSessionRunning(game) && game.available && !!game.runtime_path; }
function gameStatus(game) {
  if (!game.available) return 'Executable missing';
  if (gameSessionRunning(game)) return 'Running';
  if ((game.tools || []).some(running)) return 'Companion app open';
  if (game.launch?.state === 'failed') return 'Launch failed';
  if (!game.runtime_path) return 'Choose a runtime';
  if (game.launch?.state === 'unknown') return 'Previous session ended';
  return 'Ready to play';
}
function element(tag, text, className) {
  const node = document.createElement(tag);
  if (text !== undefined) node.textContent = text;
  if (className) node.className = className;
  return node;
}
function artwork(game) {
  const executable = game.executable.toLowerCase();
  if (executable.endsWith('/skyrimse.exe')) return { src: 'https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/489830/library_hero.jpg', kind: 'landscape', credit: 'Bethesda · Steam' };
  if (executable.endsWith('/mines.exe')) return { src: '/artwork/mines.png', kind: 'screenshot', credit: 'Simon Tatham’s Portable Puzzles' };
  return null;
}
// Artwork with a voxel fallback that shows the game's name if the image is missing or fails.
function artNode(game, className) {
  const art = artwork(game);
  const frame = element('div', undefined, className);
  const fallback = element('span', game.name, 'cover-fallback');
  if (art) {
    const image = element('img');
    image.src = art.src; image.alt = ''; image.className = art.kind; image.referrerPolicy = 'no-referrer';
    image.addEventListener('error', () => { image.remove(); fallback.hidden = false; });
    fallback.hidden = true; frame.append(image);
    frame.title = 'Artwork: ' + art.credit;
  }
  frame.append(fallback);
  return frame;
}
function playButton(game, className = 'row-play') {
  const play = element('button', gameSessionRunning(game) ? 'Running' : 'Play', className);
  play.setAttribute('aria-label', (gameSessionRunning(game) ? game.name + ' is running' : 'Play ' + game.name));
  play.disabled = !canPlay(game);
  play.addEventListener('click', (event) => { event.stopPropagation(); playGame(game.id); });
  return play;
}
function gameCard(game, index) {
  const card = element('article', undefined, 'game-card');
  card.style.setProperty('--i', index);
  card.dataset.featured = String(artwork(game)?.kind === 'landscape');
  card.dataset.running = String(environmentBusy(game));
  const open = element('a', undefined, 'game-open');
  open.href = '#/game/' + game.id;
  open.setAttribute('aria-label', game.name + ' details');
  const info = element('div', undefined, 'game-info');
  info.append(element('strong', game.name), element('span', gameStatus(game), environmentBusy(game) ? 'status running' : 'status'));
  open.append(artNode(game, 'game-art'), info);
  const footer = element('div', undefined, 'game-card-footer');
  footer.append(element('span', game.play_seconds ? duration(game.play_seconds) + ' played' : 'Not played yet', 'card-meta'), playButton(game));
  card.append(open, footer);
  return card;
}

// ---------- Rendering ----------
function renderShell() {
  const game = current();
  $('#crumbs').textContent = ['RESPAWN+', view.name === 'game' ? 'LIBRARY' : view.name.toUpperCase(), game?.name.toUpperCase()].filter(Boolean).join(' // ');
  for (const link of document.querySelectorAll('[data-nav]')) {
    const active = link.dataset.nav === view.name || (view.name === 'game' && link.dataset.nav === 'library');
    if (active) link.setAttribute('aria-current', 'page'); else link.removeAttribute('aria-current');
  }
  $('#nav-count').textContent = games.length || '';
  const list = $('#running-list'); list.replaceChildren();
  const active = games.filter(environmentBusy);
  if (!active.length) list.append(element('li', 'Nothing running', 'muted'));
  for (const game of active) {
    const item = element('li'); const link = element('a', game.name); link.href = '#/game/' + game.id;
    const apps = (game.tools || []).filter(running).map((t) => t.name);
    item.append(link, element('span', apps.length ? apps.join(' · ') : 'Game', 'running-apps')); list.append(item);
  }
  for (const name of ['home', 'library', 'game']) $('#view-' + name).hidden = view.name !== name;
}
function sessionItem(session, game) {
  const item = element('li', undefined, 'session');
  const live = session.state === 'running' || session.state === 'starting';
  item.dataset.live = String(live);
  const head = element('div', undefined, 'session-head');
  if (game) { const link = element('a', game.name, 'session-game'); link.href = '#/game/' + game.id; head.append(link); }
  head.append(element('span', live ? 'Playing now' : session.seconds == null ? 'Duration unknown' : duration(session.seconds), 'session-duration'));
  item.append(head, element('span', clock(session.started_at) + (session.via ? ' · ' + session.via : ''), 'session-when'));
  return item;
}
function renderHome() {
  const played = games.filter((g) => g.last_played_at).sort((a, b) => b.last_played_at.localeCompare(a.last_played_at));
  const hero = games.find(gameSessionRunning) || played[0] || games[0];
  const art = $('#continue-art'); art.replaceChildren();
  $('#continue').dataset.empty = String(!hero);
  if (hero) {
    art.append(artNode(hero, 'continue-image'));
    $('#continue-eyebrow').textContent = gameSessionRunning(hero) ? 'Playing now' : hero.last_played_at ? 'Continue playing' : 'Ready when you are';
    $('#home-title').textContent = hero.name;
    const last = hero.sessions?.[0];
    $('#continue-meta').textContent = [hero.last_played_at ? 'Last played ' + relative(hero.last_played_at).toLowerCase() : 'Not played yet',
      last?.seconds != null ? 'last session ' + duration(last.seconds) : null, hero.play_seconds ? duration(hero.play_seconds) + ' total' : null].filter(Boolean).join(' · ');
    $('#continue-play').textContent = gameSessionRunning(hero) ? 'Running' : 'Play';
    $('#continue-play').disabled = !canPlay(hero);
    $('#continue-play').dataset.gameId = hero.id;
    $('#continue-details').textContent = 'Game details'; $('#continue-details').href = '#/game/' + hero.id;
  } else {
    $('#continue-eyebrow').textContent = 'Welcome';
    $('#home-title').textContent = 'Build your library.';
    $('#continue-meta').textContent = 'Find installed Windows games or add an executable. Each game gets its own environment on this machine.';
    $('#continue-play').disabled = true; $('#continue-details').textContent = 'Open library'; $('#continue-details').href = '#/library';
  }
  $('#stat-playtime').textContent = duration(games.reduce((sum, g) => sum + (g.play_seconds || 0), 0));
  $('#stat-games').textContent = String(games.length);
  $('#stat-running').textContent = String(games.filter(environmentBusy).length);
  $('#stat-tools').textContent = String(games.reduce((sum, g) => sum + (g.tools || []).length, 0));
  const recent = $('#recent-list'); recent.replaceChildren();
  const row = (played.length ? played : games).slice(0, 4);
  if (!row.length) recent.append(element('p', 'Games you play show up here.', 'empty'));
  row.forEach((game, i) => recent.append(gameCard(game, i)));
  const sessions = games.flatMap((g) => (g.sessions || []).map((s) => [s, g])).sort((a, b) => b[0].started_at.localeCompare(a[0].started_at)).slice(0, 7);
  const timeline = $('#session-list'); timeline.replaceChildren();
  if (!sessions.length) timeline.append(element('li', 'No sessions yet. Press Play on any game.', 'empty'));
  for (const [session, game] of sessions) timeline.append(sessionItem(session, game));
}
function renderLibrary() {
  const query = $('#search').value.toLocaleLowerCase();
  const shown = games.filter((g) => (filter !== 'running' || environmentBusy(g)) && g.name.toLocaleLowerCase().includes(query));
  $('#library-summary').textContent = games.length + (games.length === 1 ? ' game' : ' games') + ' in your library';
  $('#shown-count').textContent = shown.length + ' shown';
  $('#filter-all').setAttribute('aria-pressed', String(filter === 'all'));
  $('#filter-running').setAttribute('aria-pressed', String(filter === 'running'));
  const list = $('#game-list'); list.replaceChildren();
  if (!shown.length) {
    list.append(element('p', games.length ? 'No games match this view.' : 'Your library is empty. Find games or add a Windows executable.', 'empty'));
    return;
  }
  shown.forEach((game, i) => list.append(gameCard(game, i)));
}
function renderRuntimes() {
  $('#runtime-count').textContent = '(' + runtimes.length + ')';
  const list = $('#runtimes'); list.replaceChildren();
  for (const runtime of runtimes) {
    const row = element('li');
    row.append(element('strong', runtime.name), element('span', runtime.path, 'runtime-path'));
    list.append(row);
  }
  if (!runtimes.length) list.append(element('li', 'No installed runtimes found. Add a custom runner in launch settings.'));
}
function screenSize() { return [Math.round(screen.width * devicePixelRatio), Math.round(screen.height * devicePixelRatio)]; }
function populateSettings(game) {
  selectedFormId = game.id;
  $('#game-name').value = game.name;
  const select = $('#runtime-choice'); select.replaceChildren();
  select.append(new Option('Choose a runtime', ''));
  for (const runtime of runtimes.filter((r) => r.kind !== 'umu')) select.append(new Option(runtime.name + ' · ' + runtime.kind, runtime.kind + '|' + runtime.path));
  select.append(new Option('Custom runtime…', 'custom'));
  const value = game.runtime_kind + '|' + game.runtime_path;
  select.value = [...select.options].some((o) => o.value === value) ? value : (game.runtime_path ? 'custom' : '');
  $('#custom-runtime-kind').value = game.runtime_kind;
  $('#custom-runtime-path').value = game.runtime_path;
  $('#custom-runtime').hidden = select.value !== 'custom';
  $('#argument-lines').value = game.arguments.join('\n');
  for (const radio of document.querySelectorAll('[name="display-mode"]')) radio.checked = radio.value === game.display_mode;
  const [width, height] = screenSize();
  $('#display-width').value = game.display_width ?? width;
  $('#display-height').value = game.display_height ?? height;
}
function displayMode() { return document.querySelector('[name="display-mode"]:checked')?.value || 'default'; }
function renderGame() {
  const game = current();
  if (!game) return;
  const art = $('#game-hero-art'); art.replaceChildren(artNode(game, 'hero-image'));
  $('#detail-title').textContent = game.name;
  $('#game-kicker').textContent = 'Windows game · ' + (game.runtime_kind === 'proton' ? 'Proton' : 'Wine');
  $('#game-state').textContent = gameStatus(game);
  $('#game-state').classList.toggle('running', environmentBusy(game));
  $('#play').textContent = gameSessionRunning(game) ? 'Running' : 'Play';
  $('#play').disabled = !canPlay(game);
  $('#game-playtime').textContent = duration(game.play_seconds);
  $('#last-played').textContent = relative(game.last_played_at);
  $('#last-played').title = clock(game.last_played_at);
  const last = (game.sessions || []).find((s) => s.seconds != null);
  $('#game-last-session').textContent = last ? duration(last.seconds) : '—';
  $('#game-display').textContent = displayLabel(game);
  for (const tab of ['overview', 'game', 'display', 'tools']) $('#detail-' + tab).setAttribute('aria-pressed', String(detailTab === tab));
  $('#overview-panel').hidden = detailTab !== 'overview';
  $('#settings-form').hidden = detailTab !== 'game' && detailTab !== 'display';
  $('#game-launch-panel').hidden = detailTab !== 'game';
  $('#display-panel').hidden = detailTab !== 'display';
  $('#tools-panel').hidden = detailTab !== 'tools';
  $('#executable-path').textContent = game.executable;
  $('#prefix-path').textContent = game.prefix;
  $('#runtime-path').textContent = game.runtime_path || 'Not selected';
  $('#view-log').disabled = !game.launch;
  const sessions = $('#game-session-list'); sessions.replaceChildren();
  if (!(game.sessions || []).length) sessions.append(element('li', 'No sessions yet.', 'empty'));
  for (const session of game.sessions || []) sessions.append(sessionItem(session));
  if (selectedFormId !== game.id || !dirty) populateSettings(game);
  const locked = environmentBusy(game);
  $('#save-settings').disabled = !dirty || busy || locked;
  $('#discard-settings').hidden = !dirty;
  $('#settings-note').textContent = locked ? 'Close the game and its companion apps before changing settings.' :
    dirty ? 'Save these changes before launching or leaving this game.' : detailTab === 'display' ? 'Applied the next time the game starts.' : 'Arguments are passed directly to the game.';
  for (const input of $('#settings-form').querySelectorAll('input, select, textarea')) input.disabled = locked;
  const displayLocked = locked || !game.display_supported;
  for (const input of $('#display-panel').querySelectorAll('input')) input.disabled = displayLocked;
  $('#use-screen').disabled = displayLocked;
  const custom = displayMode() !== 'default';
  $('#display-width').disabled = $('#display-height').disabled = displayLocked || !custom;
  $('#display-note').textContent = game.display_supported ? 'Respawn+ sets the window mode in the game’s own settings just before it starts.' :
    'This game keeps its window mode in its own options menu. Respawn+ can set it for Skyrim SE and Fallout 4.';
  renderTools(game);
}
function renderTools(game) {
  $('#add-tool').disabled = busy || dirty;
  const list = $('#tool-list'); list.replaceChildren();
  if (!(game.tools || []).length) list.append(element('p', 'No companion apps added for this game.', 'empty'));
  for (const tool of game.tools || []) {
    const row = element('article', undefined, 'tool-row');
    const type = { mod_manager: 'Mod manager', script_extender: 'Script extender', trainer: 'Trainer', utility: 'Utility' }[tool.kind] || 'Mod manager';
    const status = type + ' · ' + (!tool.available ? 'Executable missing' : running(tool) ? 'Running' : tool.launch?.state === 'failed' ? 'Launch failed' : 'Ready to open');
    row.append(element('strong', tool.name), element('span', status, running(tool) ? 'status running' : 'status'), element('p', tool.executable, 'tool-path'));
    const actions = element('div', undefined, 'tool-actions');
    for (const [label, handler, disabled] of [
      [running(tool) ? 'Running' : 'Open', () => openTool(tool.id), busy || dirty || running(tool) || !tool.available || !game.runtime_path],
      ['Edit', () => editTool(tool), busy || running(tool)],
      ['Log', () => showLog({ type: 'tool', id: tool.id, name: tool.name }), !tool.launch],
      ['Remove', () => removeToolDialog(tool), busy || running(tool)],
    ]) {
      const button = element('button', label, label === 'Open' || label === 'Running' ? '' : 'text-button');
      button.disabled = disabled; button.addEventListener('click', handler); actions.append(button);
    }
    row.append(actions); list.append(row);
  }
}
function render() {
  renderShell();
  if (view.name === 'home') renderHome();
  if (view.name === 'library') renderLibrary();
  if (view.name === 'game') renderGame();
}

// ---------- Navigation ----------
function parseRoute() {
  const [, name, id] = location.hash.match(/^#\/(home|library|game)(?:\/(\d+))?/) || [];
  return { name: name || 'home', id: id ? Number(id) : null };
}
function route() {
  const next = parseRoute();
  const leavingGame = view.name === 'game' && (next.name !== 'game' || next.id !== view.id);
  if (dirty && leavingGame) {
    history.replaceState(null, '', '#/game/' + view.id);
    message('Save or discard your settings before leaving this game.', true);
    return;
  }
  if (next.name === 'game' && ready && !games.some((g) => g.id === next.id)) { location.hash = '#/library'; return; }
  const changed = next.name !== view.name || next.id !== view.id;
  if (changed && next.name === 'game') detailTab = 'overview';
  view = next;
  render();
  if (changed) {
    // Entrance motion plays when a view opens, not on every background refresh.
    const section = $('#view-' + view.name);
    section.dataset.entering = 'true';
    setTimeout(() => { delete section.dataset.entering; }, 900);
    $('.main').scrollTo({ top: 0 });
  }
}
window.addEventListener('hashchange', route);

// ---------- Actions ----------
function setBusy(value) {
  busy = value;
  for (const id of ['scan', 'scan-folder', 'add-game', 'enter-path']) $('#' + id).disabled = value || !ready;
  render();
}
async function loadGames() {
  if (refreshing) return;
  refreshing = true;
  try {
    const previous = games;
    games = await invoke('list_games');
    for (const game of games) {
      const wasRunning = previous.some((old) => old.id === game.id && running(old));
      if (wasRunning && !running(game) && $('#message').textContent.startsWith('Starting ' + game.name + '.')) {
        message(game.launch?.state === 'finished' ? game.name + ' closed successfully.' :
          game.launch?.error || game.name + ' ended. Check the launch log for details.', game.launch?.state === 'failed');
      }
      const previousTools = previous.find((old) => old.id === game.id)?.tools || [];
      for (const tool of game.tools || []) {
        if (previousTools.some((old) => old.id === tool.id && running(old)) && !running(tool) && $('#message').textContent.startsWith('Starting ' + tool.name + '.')) {
          message(tool.launch?.state === 'finished' ? tool.name + ' closed successfully.' : tool.launch?.error || tool.name + ' ended. Check its log.', tool.launch?.state === 'failed');
        }
      }
    }
    if (view.name === 'game' && !games.some((g) => g.id === view.id)) location.hash = '#/library';
    render();
  } finally { refreshing = false; }
}
async function scan(directory = null) {
  setBusy(true); message('Looking for Windows games…');
  try {
    const result = await invoke('scan_games', { directory });
    await loadGames();
    message('Added ' + result.added + (result.added === 1 ? ' game' : ' games') + ' from ' + result.roots.length + ' folders.' +
      (result.limited ? ' Scan limit reached; scan a more specific folder for deeper results.' : '') +
      (result.skipped_directories ? ' Some folders could not be read.' : ''));
  } catch (error) { message(String(error), true); }
  finally { setBusy(false); }
}
async function playGame(id) {
  if (dirty) { message('Save your settings before playing.', true); return; }
  setBusy(true);
  try {
    const game = games.find((g) => g.id === id);
    const launch = await invoke('launch_game', { gameId: id });
    message('Starting ' + game.name + '. Launch log: ' + launch.log_path);
    await loadGames();
  } catch (error) { message(String(error), true); await loadGames().catch(() => {}); }
  finally { setBusy(false); }
}
async function importPath(path, name = null) {
  if (dirty) throw new Error('Save your current settings before importing another game.');
  setBusy(true);
  try {
    const game = await invoke('import_game', { path, name });
    selectedFormId = null;
    await loadGames(); message(game.name + ' added to your library.');
    location.hash = '#/game/' + game.id;
  } finally { setBusy(false); }
}
async function refreshEnvironment() {
  const environment = await invoke('get_environment_status');
  runtimes = environment.runtimes;
  $('#environment-state').textContent = runtimes.length + ' runtimes';
  renderRuntimes(); render();
}
async function showLog(target = logTarget) {
  if (!target) return;
  logTarget = target;
  $('#log-title').textContent = target.type === 'tool' ? target.name + ' log' : 'Game launch log';
  $('#log-content').textContent = 'Loading launch log…';
  if (!$('#log-dialog').open) $('#log-dialog').showModal();
  try { $('#log-content').textContent = await invoke(target.type === 'tool' ? 'tool_log' : 'game_log', target.type === 'tool' ? { toolId: target.id } : { gameId: target.id }); }
  catch (error) { $('#log-content').textContent = String(error); }
}
function editTool(tool = null) {
  if (!current()) return;
  toolGameId = view.id; editingToolId = tool?.id ?? null;
  $('#tool-dialog-title').textContent = tool ? 'Edit companion app' : 'Add companion app';
  $('#tool-kind').value = tool?.kind || 'mod_manager';
  $('#tool-name').value = tool?.name || '';
  $('#tool-executable').value = tool?.executable || '';
  $('#tool-working-directory').value = tool?.working_directory || '';
  $('#tool-arguments').value = (tool?.arguments || []).join('\n');
  $('#tool-error').hidden = true;
  $('#tool-dialog').showModal();
}
async function openTool(id) {
  if (dirty) { message('Save your settings before opening a companion app.', true); return; }
  setBusy(true);
  try {
    const tool = current()?.tools.find((t) => t.id === id);
    const launch = await invoke('launch_tool', { toolId: id });
    message('Starting ' + tool.name + '. Launch log: ' + launch.log_path);
    await loadGames();
  } catch (error) { message(String(error), true); await loadGames().catch(() => {}); }
  finally { setBusy(false); }
}
function removeToolDialog(tool) {
  removingToolId = tool.id;
  $('#remove-tool-description').textContent = 'Remove “' + tool.name + '” from ' + current().name + '?';
  $('#remove-tool-error').hidden = true;
  $('#remove-tool-dialog').showModal();
}
function markDirty() {
  dirty = true;
  $('#custom-runtime').hidden = $('#runtime-choice').value !== 'custom';
  render();
}

// ---------- Events ----------
$('#scan').addEventListener('click', () => scan());
$('#scan-folder').addEventListener('click', async () => {
  try { const folder = await open({ directory: true, multiple: false, title: 'Choose a folder to scan for games' }); if (folder) await scan(folder); }
  catch (error) { message(String(error), true); }
});
$('#add-game').addEventListener('click', async () => {
  try { const path = await open({ multiple: false, filters: [{ name: 'Windows game', extensions: ['exe', 'EXE'] }], title: 'Choose a game executable' }); if (path) await importPath(path); }
  catch (error) { message(String(error), true); }
});
$('#enter-path').addEventListener('click', () => { $('#import-error').hidden = true; $('#import-dialog').showModal(); });
$('#import-form').addEventListener('submit', async (event) => {
  event.preventDefault();
  try { await importPath($('#import-path').value, $('#import-name').value || null); $('#import-dialog').close(); event.target.reset(); }
  catch (error) { $('#import-error').textContent = String(error); $('#import-error').hidden = false; }
});
$('#settings-form').addEventListener('input', markDirty);
$('#discard-settings').addEventListener('click', () => { dirty = false; selectedFormId = null; render(); message('Unsaved changes discarded.'); });
$('#runtime-choice').addEventListener('change', markDirty);
$('#use-screen').addEventListener('click', () => { const [w, h] = screenSize(); $('#display-width').value = w; $('#display-height').value = h; markDirty(); });
$('#settings-form').addEventListener('submit', async (event) => {
  event.preventDefault();
  const choice = $('#runtime-choice').value;
  if (!choice) { message('Choose an installed runtime or a custom runner.', true); detailTab = 'game'; render(); return; }
  const separator = choice.indexOf('|');
  const kind = choice === 'custom' ? $('#custom-runtime-kind').value : choice.slice(0, separator);
  const path = choice === 'custom' ? $('#custom-runtime-path').value.trim() : choice.slice(separator + 1);
  const mode = displayMode();
  setBusy(true);
  try {
    await invoke('save_game', { settings: { id: view.id, name: $('#game-name').value, runtime_kind: kind, runtime_path: path,
      arguments: $('#argument-lines').value.split('\n').filter((line) => line.length > 0),
      display_mode: mode,
      display_width: mode === 'default' ? null : Number($('#display-width').value),
      display_height: mode === 'default' ? null : Number($('#display-height').value) } });
    dirty = false; await loadGames(); message('Settings saved.');
  } catch (error) { message(String(error), true); }
  finally { setBusy(false); }
});
$('#browse-runtime').addEventListener('click', async () => {
  try { const path = await open({ multiple: false, title: 'Choose the proton script or Wine executable' }); if (path) { $('#custom-runtime-path').value = path; markDirty(); } }
  catch (error) { message(String(error), true); }
});
$('#play').addEventListener('click', () => { if (current()) playGame(view.id); });
$('#continue-play').addEventListener('click', (event) => { const id = Number(event.currentTarget.dataset.gameId); if (id) playGame(id); });
$('#search').addEventListener('input', () => { if (view.name !== 'library') location.hash = '#/library'; else renderLibrary(); });
for (const mode of ['all', 'running']) $('#filter-' + mode).addEventListener('click', () => { filter = mode; renderLibrary(); });
$('#refresh').addEventListener('click', () => refreshEnvironment().catch((error) => message(String(error), true)));
$('#view-log').addEventListener('click', () => showLog({ type: 'game', id: view.id }));
$('#refresh-log').addEventListener('click', () => showLog());
for (const tab of ['overview', 'game', 'display', 'tools']) $('#detail-' + tab).addEventListener('click', () => { detailTab = tab; renderGame(); });
$('#add-tool').addEventListener('click', () => editTool());
$('#browse-tool').addEventListener('click', async () => {
  try {
    const path = await open({ multiple: false, title: 'Choose a Windows companion app', filters: [{ name: 'Windows executable', extensions: ['exe', 'EXE'] }] });
    if (path) { $('#tool-executable').value = path; if (!$('#tool-name').value) $('#tool-name').value = path.split('/').pop().replace(/\.exe$/i, ''); }
  } catch (error) { $('#tool-error').textContent = String(error); $('#tool-error').hidden = false; }
});
$('#browse-tool-folder').addEventListener('click', async () => {
  try { const path = await open({ directory: true, multiple: false, title: 'Choose the mod manager working folder' }); if (path) $('#tool-working-directory').value = path; }
  catch (error) { $('#tool-error').textContent = String(error); $('#tool-error').hidden = false; }
});
$('#tool-form').addEventListener('submit', async (event) => {
  event.preventDefault(); $('#save-tool').disabled = true;
  try {
    await invoke('save_tool', { settings: { id: editingToolId, game_id: toolGameId, kind: $('#tool-kind').value, name: $('#tool-name').value, executable: $('#tool-executable').value,
      working_directory: $('#tool-working-directory').value, arguments: $('#tool-arguments').value.split('\n').filter((line) => line.length > 0) } });
    $('#tool-dialog').close(); await loadGames(); message('Companion app profile saved.');
  } catch (error) { $('#tool-error').textContent = String(error); $('#tool-error').hidden = false; }
  finally { $('#save-tool').disabled = false; }
});
$('#remove-tool-form').addEventListener('submit', async (event) => {
  event.preventDefault();
  try { await invoke('remove_tool', { toolId: removingToolId }); $('#remove-tool-dialog').close(); await loadGames(); message('Companion app profile removed. Files were kept on disk.'); }
  catch (error) { $('#remove-tool-error').textContent = String(error); $('#remove-tool-error').hidden = false; }
});
for (const button of document.querySelectorAll('[data-close]')) button.addEventListener('click', () => $('#' + button.dataset.close).close());

async function initialize() {
  route();
  try {
    await refreshEnvironment(); ready = true; await loadGames(); setBusy(false); route();
    if (!games.length) await scan();
  } catch (error) {
    $('#environment-state').textContent = 'Backend unavailable';
    $('#library-summary').textContent = 'Open Respawn+ as a desktop app to load your games.';
    $('#game-list').replaceChildren(element('p', 'The local game library requires the Respawn+ desktop backend.', 'empty'));
    message(String(error), true); setBusy(false);
  }
}
initialize();
setInterval(() => { if (ready && !busy) loadGames().catch((error) => message('Could not refresh library: ' + error, true)); }, 2500);
