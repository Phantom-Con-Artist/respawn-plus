// SPDX-License-Identifier: MIT
import { invoke } from '@tauri-apps/api/core';
import { getVersion } from '@tauri-apps/api/app';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { open } from '@tauri-apps/plugin-dialog';
import '@fontsource-variable/inter';
import '@fontsource-variable/geist-mono';
import './styles.css';
import { appIcon } from './brand.js';

const $ = (selector) => document.querySelector(selector);
for (const slot of document.querySelectorAll('[data-app-icon]')) slot.innerHTML = appIcon;
getVersion().then((version) => { $('#version').textContent = 'v' + version; }).catch(() => {});

// Theme: an explicit choice is remembered per machine; otherwise follow the system.
const systemDark = matchMedia('(prefers-color-scheme: dark)');
function storedTheme() { try { return localStorage.getItem('vessel-theme'); } catch { return null; } }
function applyTheme(theme) {
  document.documentElement.dataset.theme = theme;
  $('#theme-toggle').textContent = theme === 'dark' ? 'Light theme' : 'Dark theme';
}
applyTheme(storedTheme() || (systemDark.matches ? 'dark' : 'light'));
systemDark.addEventListener('change', (event) => { if (!storedTheme()) applyTheme(event.matches ? 'dark' : 'light'); });
$('#theme-toggle').addEventListener('click', () => {
  const next = document.documentElement.dataset.theme === 'dark' ? 'light' : 'dark';
  try { localStorage.setItem('vessel-theme', next); } catch { /* theme still applies for this session */ }
  applyTheme(next);
});

// Window controls live in the app's own top bar (decorations are off).
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

let games = [], runtimes = [], windowsRuntimes = [], filter = 'all', dirty = false;
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
  if (seconds === 0) return '0 m';
  if (seconds < 60) return '<1 m';
  const hours = Math.floor(seconds / 3600), minutes = Math.round((seconds % 3600) / 60);
  return hours ? hours + ' h' + (minutes ? ' ' + minutes + ' m' : '') : minutes + ' m';
}
function relative(value) {
  const date = parseTime(value);
  if (!date) return 'never';
  const minutes = Math.round((Date.now() - date) / 60000);
  if (minutes < 1) return 'just now';
  if (minutes < 60) return minutes + ' min ago';
  if (minutes < 24 * 60) return Math.round(minutes / 60) + ' h ago';
  if (minutes < 48 * 60) return 'yesterday';
  return date.toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: date.getFullYear() === new Date().getFullYear() ? undefined : 'numeric' });
}
function clock(value) { return parseTime(value)?.toLocaleString(undefined, { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' }) ?? ''; }
const MODE_NAMES = { default: 'Game default', fullscreen: 'Fullscreen', borderless: 'Borderless', windowed: 'Windowed' };
function displayLabel(game) {
  if (!game.display_supported) return 'Game’s own window';
  return MODE_NAMES[game.display_mode] + (game.display_mode !== 'default' && game.display_width ? ' · ' + game.display_width + '×' + game.display_height : '');
}
function runnerName(game) {
  if (!game.runtime_path) return 'No runtime';
  const parts = game.runtime_path.split('/');
  return game.runtime_kind === 'proton' ? parts[parts.length - 2] || 'Proton' : parts[parts.length - 1];
}

// ---------- Game state ----------
function current() { return view.name === 'game' ? games.find((g) => g.id === view.id) : undefined; }
function running(item) { return item.launch?.state === 'running'; }
// Mod managers and script extenders start the game; trainers and utilities attach to it.
function startsGame(kind) { return kind === 'mod_manager' || kind === 'script_extender'; }
function gameSessionRunning(game) { return running(game) || (game.tools || []).some((tool) => (tool.kind === 'script_extender' || tool.id === game.launch_via_tool_id) && running(tool)); }
function environmentBusy(game) { return running(game) || (game.tools || []).some(running); }
function canPlay(game) { return !busy && !dirty && !gameSessionRunning(game) && game.available && !!game.runtime_path; }
function needsSetup(game) { return !game.available || !game.runtime_path; }
function gameStatus(game) {
  if (!game.available) return 'Executable missing';
  if (gameSessionRunning(game)) return 'Running';
  if ((game.tools || []).some(running)) return 'Companion app open';
  if (game.launch?.state === 'failed') return 'Last launch failed';
  if (!game.runtime_path) return 'Choose a runtime';
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
// Artwork, falling back to the game's name on a quiet field if the image is missing or fails.
function artNode(game, className) {
  const art = artwork(game);
  const frame = element('div', undefined, className + ' art');
  const fallback = element('span', game.name, 'art-fallback');
  if (art) {
    const image = element('img');
    image.src = art.src; image.alt = ''; image.className = art.kind; image.referrerPolicy = 'no-referrer';
    image.addEventListener('error', () => { image.remove(); fallback.hidden = false; });
    fallback.hidden = true; frame.append(image); frame.title = 'Artwork: ' + art.credit;
  }
  frame.append(fallback);
  return frame;
}
function playButton(game, className = 'primary small-button') {
  const play = element('button', gameSessionRunning(game) ? 'Running' : 'Play', className);
  play.setAttribute('aria-label', gameSessionRunning(game) ? game.name + ' is running' : 'Play ' + game.name);
  play.disabled = !canPlay(game);
  play.addEventListener('click', (event) => { event.preventDefault(); event.stopPropagation(); playGame(game.id); });
  return play;
}
function statusNode(game) {
  const node = element('span', gameStatus(game), 'state');
  node.dataset.state = environmentBusy(game) ? 'running' : needsSetup(game) || game.launch?.state === 'failed' ? 'attention' : 'ready';
  return node;
}
function gameCard(game) {
  const card = element('article', undefined, 'game-card');
  const link = element('a', undefined, 'game-link');
  link.href = '#/game/' + game.id;
  link.setAttribute('aria-label', game.name + ' details');
  link.append(artNode(game, 'game-art'));
  const body = element('div', undefined, 'game-body');
  const text = element('div');
  text.append(element('strong', game.name, 'game-name'), element('span', game.play_seconds ? duration(game.play_seconds) + ' played' : 'Not played yet', 'muted small num'));
  body.append(text, playButton(game));
  card.append(link, body, statusNode(game));
  return card;
}

// ---------- Rendering ----------
function renderShell() {
  for (const link of document.querySelectorAll('[data-nav]')) {
    const active = link.dataset.nav === view.name || (view.name === 'game' && link.dataset.nav === 'library');
    if (active) link.setAttribute('aria-current', 'page'); else link.removeAttribute('aria-current');
  }
  $('#nav-count').textContent = games.length ? String(games.length) : '';
  $('#back-library').hidden = view.name !== 'game';
  const list = $('#running-list'); list.replaceChildren();
  const active = games.filter(environmentBusy);
  if (!active.length) list.append(element('li', 'Nothing running', 'muted'));
  for (const game of active) {
    const item = element('li'); const link = element('a', game.name); link.href = '#/game/' + game.id;
    const apps = (game.tools || []).filter(running).map((t) => t.name);
    item.append(link, element('span', apps.length ? apps.join(', ') : 'Game', 'muted small'));
    list.append(item);
  }
  for (const name of ['home', 'library', 'game']) $('#view-' + name).hidden = view.name !== name;
}
function sessionItem(session, game) {
  const item = element('li', undefined, 'session');
  const live = session.state === 'running' || session.state === 'starting';
  item.dataset.live = String(live);
  const text = element('div');
  if (game) { const link = element('a', game.name, 'session-game'); link.href = '#/game/' + game.id; text.append(link); }
  text.append(element('span', clock(session.started_at) + (session.via ? ' · ' + session.via : ''), 'muted small'));
  item.append(text, element('span', live ? 'Playing' : session.seconds == null ? '—' : duration(session.seconds), 'num session-time'));
  if (session.seconds == null && !live) item.lastChild.title = 'This session ended while Vessel was closed, so its length is unknown';
  return item;
}
// The 14-day strip: one bar per day from daily totals, today in the data hue, a rounded scale above the maximum.
function renderPlaytime() {
  const days = [];
  const today = new Date();
  for (let i = 13; i >= 0; i--) {
    const d = new Date(Date.UTC(today.getUTCFullYear(), today.getUTCMonth(), today.getUTCDate() - i));
    days.push({ key: d.toISOString().slice(0, 10), date: d, seconds: 0 });
  }
  for (const game of games) for (const day of game.daily_play || []) {
    const slot = days.find((d) => d.key === day.date);
    if (slot) slot.seconds += day.seconds;
  }
  const total = days.reduce((sum, d) => sum + d.seconds, 0);
  $('#playtime-total').textContent = duration(total);
  const maxMinutes = Math.max(...days.map((d) => d.seconds / 60));
  const steps = [5, 10, 15, 30, 60, 120, 180, 240, 360, 480, 720];
  const top = steps.find((s) => s > maxMinutes) || Math.ceil(maxMinutes / 60) * 60 + 60;
  const W = Math.max(260, $('#playtime-chart').clientWidth || 320), H = 168, left = 36, bottom = 22, plot = H - bottom - 8, slot = (W - left) / days.length;
  const ns = 'http://www.w3.org/2000/svg';
  const svg = document.createElementNS(ns, 'svg');
  svg.setAttribute('viewBox', `0 0 ${W} ${H}`);
  svg.setAttribute('role', 'img');
  svg.setAttribute('aria-label', 'Playtime per day over the last 14 days, ' + duration(total) + ' in total');
  const add = (tag, attrs, text) => { const n = document.createElementNS(ns, tag); for (const [k, v] of Object.entries(attrs)) n.setAttribute(k, v); if (text != null) n.textContent = text; svg.append(n); return n; };
  for (const value of [0, top / 2, top]) {
    const y = 8 + plot - (value / top) * plot;
    add('line', { x1: left, x2: W, y1: y, y2: y, class: value ? 'grid-line' : 'base-line' });
    add('text', { x: left - 6, y: y + 4, 'text-anchor': 'end', class: 'tick' }, value >= 60 ? value / 60 + ' h' : value + ' m');
  }
  days.forEach((day, i) => {
    const h = (day.seconds / 60 / top) * plot;
    const x = left + i * slot + slot * 0.2;
    if (h > 0) {
      const bar = add('rect', { x, y: 8 + plot - h, width: slot * 0.6, height: Math.max(h, 1.5), class: i === 13 ? 'bar today' : 'bar' });
      const title = document.createElementNS(ns, 'title');
      title.textContent = day.date.toLocaleDateString(undefined, { weekday: 'short', day: 'numeric', month: 'short', timeZone: 'UTC' }) + ': ' + duration(day.seconds);
      bar.append(title);
    }
    if (i % 3 === 1 || i === 13) add('text', { x: x + slot * 0.3, y: H - 6, 'text-anchor': 'middle', class: 'tick' }, i === 13 ? 'Today' : day.date.toLocaleDateString(undefined, { day: 'numeric', month: 'short', timeZone: 'UTC' }));
  });
  $('#playtime-chart').replaceChildren(svg);
}
function renderHome() {
  const played = games.filter((g) => g.last_played_at).sort((a, b) => b.last_played_at.localeCompare(a.last_played_at));
  const hero = games.find(gameSessionRunning) || played[0] || games[0];
  $('#continue-art').replaceChildren(...(hero ? [artNode(hero, 'continue-image')] : []));
  if (hero) {
    const last = (hero.sessions || []).find((s) => s.seconds != null);
    $('#continue-label').textContent = gameSessionRunning(hero) ? 'Playing now' : hero.last_played_at ? 'Continue playing' : 'Ready to play';
    $('#continue-name').textContent = hero.name;
    $('#continue-meta').textContent = [hero.last_played_at ? 'Played ' + relative(hero.last_played_at) : 'Not played yet', last ? 'last session ' + duration(last.seconds) : null, runnerName(hero)].filter(Boolean).join(' · ');
    $('#continue-play').textContent = gameSessionRunning(hero) ? 'Running' : 'Play';
    $('#continue-play').disabled = !canPlay(hero);
    $('#continue-play').dataset.gameId = hero.id;
    $('#continue-details').href = '#/game/' + hero.id;
    $('#home-finding').textContent = gameSessionRunning(hero) ? hero.name + ' is running now.' :
      hero.last_played_at ? hero.name + ' was played ' + relative(hero.last_played_at) + (last ? ', for ' + duration(last.seconds) + '.' : '.') : games.length + ' games ready to play.';
  } else {
    $('#continue-label').textContent = 'Get started';
    $('#continue-name').textContent = 'Build your library';
    $('#continue-meta').textContent = 'Find installed Windows games or add an executable. Each game gets its own environment.';
    $('#continue-play').disabled = true;
    $('#continue-details').href = '#/library';
    $('#home-finding').textContent = 'No games in your library yet.';
  }
  const sessions = games.flatMap((g) => (g.sessions || []).map((s) => [s, g])).sort((a, b) => b[0].started_at.localeCompare(a[0].started_at));
  $('#stat-playtime').textContent = duration(games.reduce((sum, g) => sum + (g.play_seconds || 0), 0));
  const weekStart = new Date(Date.now() - 6 * 86400000).toISOString().slice(0, 10);
  $('#stat-week').textContent = duration(games.flatMap((g) => g.daily_play || []).filter((d) => d.date >= weekStart).reduce((sum, d) => sum + d.seconds, 0));
  $('#stat-games').textContent = String(games.length);
  $('#stat-tools').textContent = String(games.reduce((sum, g) => sum + (g.tools || []).length, 0));
  renderPlaytime();
  const recent = $('#recent-list'); recent.replaceChildren();
  const row = (played.length ? played : games).slice(0, 3);
  if (!row.length) recent.append(element('p', 'Games you play appear here.', 'empty'));
  for (const game of row) recent.append(gameCard(game));
  const list = $('#session-list'); list.replaceChildren();
  if (!sessions.length) list.append(element('li', 'No sessions yet. Press Play on any game.', 'empty'));
  for (const [session, game] of sessions.slice(0, 5)) list.append(sessionItem(session, game));
}
function matchesFilter(game, name) {
  if (name === 'running') return environmentBusy(game);
  if (name === 'ready') return !needsSetup(game) && !environmentBusy(game);
  if (name === 'setup') return needsSetup(game);
  return true;
}
function renderLibrary() {
  const query = $('#search').value.toLocaleLowerCase();
  for (const button of document.querySelectorAll('[data-filter]')) {
    button.setAttribute('aria-pressed', String(button.dataset.filter === filter));
    button.querySelector('[data-count]').textContent = String(games.filter((g) => matchesFilter(g, button.dataset.filter)).length);
  }
  const shown = games.filter((g) => matchesFilter(g, filter) && g.name.toLocaleLowerCase().includes(query));
  $('#library-summary').textContent = games.length + (games.length === 1 ? ' game' : ' games') + ' · ' + runtimes.length + ' runtimes found';
  $('#shown-count').textContent = shown.length + ' of ' + games.length + ' shown';
  const list = $('#game-list'); list.replaceChildren();
  if (!shown.length) {
    list.append(element('p', games.length ? 'No games match this filter.' : 'Your library is empty. Find games or add a Windows executable.', 'empty'));
    return;
  }
  for (const game of shown) list.append(gameCard(game));
}
function renderRuntimes() {
  $('#runtime-count').textContent = String(runtimes.length);
  const list = $('#runtimes'); list.replaceChildren();
  for (const runtime of runtimes) {
    const row = element('li');
    row.append(element('strong', runtime.name), element('span', runtime.path, 'path muted'));
    list.append(row);
  }
  if (!runtimes.length) list.append(element('li', 'No installed runtimes found. Add a custom runner in launch settings.'));
  const dependencies = $('#windows-runtimes'); dependencies.replaceChildren();
  for (const runtime of windowsRuntimes) {
    const row = element('li');
    row.textContent = runtime.manifest ? runtime.manifest.id + ' · ' + runtime.manifest.version + ' · ' + runtime.manifest.architecture : 'A runtime package needs repair: ' + runtime.error;
    dependencies.append(row);
  }
  if (!windowsRuntimes.length) dependencies.append(element('li', 'No shared Windows packages installed. Your runner’s built-in libraries remain available.', 'muted'));
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
  const via = $('#play-via'); via.replaceChildren(new Option('The game itself', ''));
  // Only apps that start the game themselves can be the Play target; trainers run alongside it.
  for (const tool of (game.tools || []).filter((tool) => startsGame(tool.kind))) via.append(new Option('Through ' + tool.name, String(tool.id)));
  via.value = [...via.options].some((option) => option.value === String(game.launch_via_tool_id)) ? String(game.launch_via_tool_id) : '';
  for (const radio of document.querySelectorAll('[name="display-mode"]')) radio.checked = radio.value === game.display_mode;
  const [width, height] = screenSize();
  $('#display-width').value = game.display_width ?? width;
  $('#display-height').value = game.display_height ?? height;
}
function displayMode() { return document.querySelector('[name="display-mode"]:checked')?.value || 'default'; }
function renderGame() {
  const game = current();
  if (!game) return;
  $('#game-cover').replaceChildren(artNode(game, 'cover-image'));
  $('#detail-title').textContent = game.name;
  $('#game-meta').textContent = ['Windows game', game.runtime_kind === 'proton' ? 'Proton' : 'Wine', runnerName(game), game.last_played_at ? 'played ' + relative(game.last_played_at) : 'not played yet'].join(' · ');
  $('#game-state').replaceWith(Object.assign(statusNode(game), { id: 'game-state' }));
  $('#play').textContent = gameSessionRunning(game) ? 'Running' : 'Play';
  $('#play').disabled = !canPlay(game);
  $('#view-log').disabled = !game.launch;
  $('#stop-room').hidden = !game.room?.active;
  $('#stop-room').disabled = busy;
  $('#game-playtime').textContent = duration(game.play_seconds);
  const last = (game.sessions || []).find((s) => s.seconds != null);
  $('#game-last-session').textContent = last ? duration(last.seconds) : '—';
  $('#last-played').textContent = last ? clock(last.started_at) : 'No finished sessions yet';
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
  const sessions = $('#game-session-list'); sessions.replaceChildren();
  if (!(game.sessions || []).length) sessions.append(element('li', 'No sessions yet.', 'empty'));
  for (const session of game.sessions || []) sessions.append(sessionItem(session));
  if (selectedFormId !== game.id || !dirty) populateSettings(game);
  const locked = environmentBusy(game);
  $('#save-settings').disabled = !dirty || busy || locked;
  $('#discard-settings').hidden = !dirty;
  $('#settings-note').textContent = locked ? 'Close the game and its companion apps before changing settings.' :
    dirty ? 'Save before launching or leaving this game.' : detailTab === 'display' ? 'Applied the next time the game starts.' : 'Arguments are passed directly to the game.';
  for (const input of $('#settings-form').querySelectorAll('input, select, textarea')) input.disabled = locked;
  const displayLocked = locked || !game.display_supported;
  for (const input of $('#display-panel').querySelectorAll('input')) input.disabled = displayLocked;
  $('#use-screen').disabled = displayLocked;
  // Default and borderless fill the monitor at its own resolution; the size is only for the others.
  const custom = displayMode() === 'fullscreen' || displayMode() === 'windowed';
  $('#display-width').disabled = $('#display-height').disabled = displayLocked || !custom;
  $('#display-note').textContent = game.display_supported ? 'The game is shown on its room’s own screen. Vessel sets that screen, never the game’s files, so this works for any game.' :
    'Each app in this room gets its own window, so the game uses its own window settings. Choose “One window for the whole room” under Display to set them here.';
  renderTools(game);
  renderRecovery(game);
  renderRoomProfile(game);
}
function renderRoomProfile(game) {
  const profile = game.room?.profile || {};
  $('#room-network').value = profile.network || 'online';
  $('#room-display').value = profile.display || 'auto';
  $('#room-audio').checked = profile.audio !== false;
  $('#room-controllers').checked = profile.controllers !== false;
  const packages = $('#room-packages'); packages.replaceChildren();
  for (const runtime of windowsRuntimes.filter((runtime) => runtime.manifest)) {
    const option = new Option(runtime.manifest.id + ' ' + runtime.manifest.version + ' · ' + runtime.manifest.architecture, runtime.digest);
    option.selected = (profile.packages || []).includes(runtime.digest); packages.append(option);
  }
  if (!packages.options.length) packages.append(new Option('No shared packages installed', ''));
  for (const id of ['room-network','room-display','room-packages','room-audio','room-controllers']) $('#' + id).disabled = busy || dirty || environmentBusy(game);
}
function renderRecovery(game) {
  const recovery = game.recovery || { supported: false, checkpoints: [] };
  const select = $('#recovery-checkpoint');
  const previous = select.value;
  select.replaceChildren();
  for (const checkpoint of recovery.checkpoints) select.append(new Option(clock(checkpoint.created_at) + ' · ' + checkpoint.label, String(checkpoint.id)));
  if (!recovery.checkpoints.length) select.append(new Option('No checkpoints yet', ''));
  if ([...select.options].some((option) => option.value === previous)) select.value = previous;
  const locked = busy || dirty || environmentBusy(game);
  $('#checkpoint-room').disabled = locked || !recovery.supported;
  $('#restore-room').disabled = locked || !recovery.supported || !recovery.checkpoints.length;
  select.disabled = locked || !recovery.checkpoints.length;
  $('#recovery-note').textContent = !recovery.supported ? 'Recovery currently supports Skyrim Special Edition and Fallout 4. You can keep playing other games while support is added.' :
    environmentBusy(game) ? 'Close the game and companion apps before restoring. This session uses the checkpoint made before the room opened.' :
    dirty ? 'Save or discard your launch settings before using recovery.' :
    'A restore point is saved before the room opens. Preparing large game folders may take a moment.';
}
function renderTools(game) {
  $('#add-tool').disabled = busy || dirty;
  const list = $('#tool-list'); list.replaceChildren();
  if (!(game.tools || []).length) list.append(element('p', 'No companion apps for this game yet. Add a mod manager, script extender or trainer.', 'empty'));
  for (const tool of game.tools || []) {
    const row = element('article', undefined, 'tool-row');
    const type = { mod_manager: 'Mod manager', script_extender: 'Script extender', trainer: 'Trainer', utility: 'Utility' }[tool.kind] || 'Mod manager';
    const text = element('div');
    const state = element('span', !tool.available ? 'Executable missing' : running(tool) ? 'Running' : tool.launch?.state === 'failed' ? 'Last launch failed' : 'Ready', 'state');
    state.dataset.state = running(tool) ? 'running' : !tool.available || tool.launch?.state === 'failed' ? 'attention' : 'ready';
    const head = element('div', undefined, 'tool-head'); head.append(element('strong', tool.name), state);
    text.append(head, element('span', type + ' · ' + tool.executable.split('/').pop(), 'muted small'));
    const actions = element('div', undefined, 'actions');
    for (const [label, handler, disabled, kind] of [
      ['Log', () => showLog({ type: 'tool', id: tool.id, name: tool.name }), !tool.launch, 'text-button'],
      ['Edit', () => editTool(tool), busy || running(tool), 'text-button'],
      ['Remove', () => removeToolDialog(tool), busy || running(tool), 'text-button'],
      [running(tool) ? 'Running' : startsGame(tool.kind) || gameSessionRunning(game) || game.room?.active ? 'Open' : 'Play with game', () => openTool(tool.id), busy || dirty || running(tool) || !tool.available || !game.runtime_path, 'ghost small-button'],
    ]) {
      const button = element('button', label, kind);
      button.disabled = disabled; button.addEventListener('click', handler); actions.append(button);
    }
    row.append(text, actions); list.append(row);
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
  if (changed) $('.main').scrollTo({ top: 0 });
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
        message(game.launch?.state === 'finished' ? game.name + ' closed.' :
          game.launch?.error || game.name + ' ended. Check the launch log for details.', game.launch?.state === 'failed');
      }
      const previousTools = previous.find((old) => old.id === game.id)?.tools || [];
      for (const tool of game.tools || []) {
        if (previousTools.some((old) => old.id === tool.id && running(old)) && !running(tool) && $('#message').textContent.startsWith('Starting ' + tool.name + '.')) {
          message(tool.launch?.state === 'finished' ? tool.name + ' closed.' : tool.launch?.error || tool.name + ' ended. Check its log.', tool.launch?.state === 'failed');
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
    message('Preparing ' + game.name + (game.recovery?.supported ? ' and its recovery checkpoint…' : '…'));
    await invoke('launch_game', { gameId: id });
    message('Starting ' + game.name + '.');
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
  const [environment, dependencies] = await Promise.all([invoke('get_environment_status'), invoke('list_windows_runtimes')]);
  windowsRuntimes = dependencies;
  runtimes = environment.runtimes;
  $('#environment-state').textContent = runtimes.length + (runtimes.length === 1 ? ' runtime' : ' runtimes') + ' ready';
  renderRuntimes(); render();
}
async function showLog(target = logTarget) {
  if (!target) return;
  logTarget = target;
  $('#log-title').textContent = target.type === 'tool' ? target.name + ' log' : 'Launch log';
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
    message('Preparing ' + tool.name + (current()?.recovery?.supported && !environmentBusy(current()) ? ' and its recovery checkpoint…' : '…'));
    await invoke('launch_tool', { toolId: id });
    message('Starting ' + tool.name + '.');
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
      display_height: mode === 'default' ? null : Number($('#display-height').value),
      launch_via_tool_id: $('#play-via').value ? Number($('#play-via').value) : null } });
    dirty = false; await loadGames(); message('Settings saved.');
  } catch (error) { message(String(error), true); }
  finally { setBusy(false); }
});
$('#browse-runtime').addEventListener('click', async () => {
  try { const path = await open({ multiple: false, title: 'Choose the proton script or Wine executable' }); if (path) { $('#custom-runtime-path').value = path; markDirty(); } }
  catch (error) { message(String(error), true); }
});
$('#play').addEventListener('click', () => { if (current()) playGame(view.id); });
$('#stop-room').addEventListener('click', async () => {
  const game = current(); if (!game || busy) return;
  setBusy(true); message('Stopping all apps in ' + game.name + '’s room…');
  try { await invoke('stop_room', { gameId: game.id }); await loadGames(); message('Room apps stopped.'); }
  catch (error) { message(String(error), true); }
  finally { setBusy(false); }
});
for (const id of ['room-network','room-display','room-packages','room-audio','room-controllers']) $('#' + id).addEventListener('change', async () => {
  const game = current(); if (!game || busy || dirty) return;
  const profile = { ...game.room.profile, network: $('#room-network').value, display: $('#room-display').value,
    packages: [...$('#room-packages').selectedOptions].map((option) => option.value).filter(Boolean), audio: $('#room-audio').checked, controllers: $('#room-controllers').checked };
  setBusy(true);
  try { await invoke('configure_room', { gameId: game.id, profile }); await loadGames(); message('Room preferences saved.'); }
  catch (error) { message(String(error), true); }
  finally { setBusy(false); }
});
$('#install-windows-runtime').addEventListener('click', async () => {
  if (busy) return;
  try {
    const source = await open({ directory: true, multiple: false, title: 'Choose a prepared Windows runtime package' });
    if (!source) return;
    setBusy(true); message('Importing and verifying the shared runtime package…');
    const installed = await invoke('install_windows_runtime', { source });
    await refreshEnvironment(); message(installed.manifest.id + ' installed in the shared store.');
  } catch (error) { message(String(error), true); }
  finally { setBusy(false); }
});
async function recoverRoom(restore) {
  const game = current();
  if (!game || busy || dirty || environmentBusy(game)) return;
  const checkpointId = Number($('#recovery-checkpoint').value);
  setBusy(true);
  message(restore ? 'Restoring ' + game.name + ' while keeping current saves…' : 'Creating a checkpoint for ' + game.name + '…');
  try {
    if (restore) await invoke('restore_room_checkpoint', { gameId: game.id, checkpointId });
    else await invoke('create_room_checkpoint', { gameId: game.id });
    selectedFormId = null;
    await loadGames();
    message(restore ? 'Setup restored. Current mapped saves were kept. Choose “Before restore — undo” to reverse this restore.' : 'Checkpoint created.');
  } catch (error) { message(String(error), true); await loadGames().catch(() => {}); }
  finally { setBusy(false); }
}
$('#checkpoint-room').addEventListener('click', () => recoverRoom(false));
$('#restore-room').addEventListener('click', () => recoverRoom(true));
$('#continue-play').addEventListener('click', (event) => { const id = Number(event.currentTarget.dataset.gameId); if (id) playGame(id); });
$('#search').addEventListener('input', () => { if (view.name !== 'library') location.hash = '#/library'; else renderLibrary(); });
for (const button of document.querySelectorAll('[data-filter]')) button.addEventListener('click', () => { filter = button.dataset.filter; renderLibrary(); });
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
  try { const path = await open({ directory: true, multiple: false, title: 'Choose the companion app’s working folder' }); if (path) $('#tool-working-directory').value = path; }
  catch (error) { $('#tool-error').textContent = String(error); $('#tool-error').hidden = false; }
});
$('#tool-form').addEventListener('submit', async (event) => {
  event.preventDefault(); $('#save-tool').disabled = true;
  try {
    await invoke('save_tool', { settings: { id: editingToolId, game_id: toolGameId, kind: $('#tool-kind').value, name: $('#tool-name').value, executable: $('#tool-executable').value,
      working_directory: $('#tool-working-directory').value, arguments: $('#tool-arguments').value.split('\n').filter((line) => line.length > 0) } });
    $('#tool-dialog').close(); await loadGames(); message('Companion app saved.');
  } catch (error) { $('#tool-error').textContent = String(error); $('#tool-error').hidden = false; }
  finally { $('#save-tool').disabled = false; }
});
$('#remove-tool-form').addEventListener('submit', async (event) => {
  event.preventDefault();
  try { await invoke('remove_tool', { toolId: removingToolId }); $('#remove-tool-dialog').close(); await loadGames(); message('Companion app removed. Its files were kept on disk.'); }
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
    $('#library-summary').textContent = 'Open Vessel as a desktop app to load your games.';
    $('#game-list').replaceChildren(element('p', 'The game library needs the Vessel desktop backend.', 'empty'));
    message(String(error), true); setBusy(false);
  }
}
initialize();
setInterval(() => { if (ready && !busy) loadGames().catch((error) => message('Could not refresh the library: ' + error, true)); }, 2500);
