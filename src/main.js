// SPDX-License-Identifier: MIT
import { invoke } from '@tauri-apps/api/core';
import { getVersion } from '@tauri-apps/api/app';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { open } from '@tauri-apps/plugin-dialog';
import '@fontsource-variable/hanken-grotesk';
import './styles.css';
import { appIcon } from './brand.js';

const $ = (selector) => document.querySelector(selector);
const ICONS = {
  library: '<path d="M3 3h4v10H3zM9 3h4v10H9z"/>',
  layers: '<path d="M8 2 2 5l6 3 6-3zM2 8l6 3 6-3M2 11l6 3 6-3"/>',
  settings: '<path d="M2 4h7M12 4h2M2 12h2M7 12h7"/><path d="M9 2.5h3v3H9zM4 10.5h3v3H4z"/>',
  search: '<path d="M7 12A5 5 0 1 0 7 2a5 5 0 0 0 0 10ZM10.5 10.5 14 14"/>',
  back: '<path d="M10 3 5 8l5 5"/>',
  plus: '<path d="M8 3v10M3 8h10"/>',
  play: '<path d="M4 2.5v11L13 8z"/>',
};
function icon(name) { return `<svg viewBox="0 0 16 16" aria-hidden="true">${ICONS[name]}</svg>`; }
for (const slot of document.querySelectorAll('[data-app-icon]')) slot.innerHTML = appIcon;
for (const slot of document.querySelectorAll('[data-icon]')) slot.innerHTML = icon(slot.dataset.icon);
getVersion().then((version) => { $('#version').textContent = 'v' + version; $('#about-version').textContent = 'Vessel ' + version; }).catch(() => {});

// ---------- Theme: Milky, Night, Ashen, or follow the system ----------
const THEMES = [
  { id: 'system', name: 'Match system', swatch: ['#f3f2ee', '#0a1012', '#18e6e4'] },
  { id: 'milky', name: 'Milky', swatch: ['#f3f2ee', '#141818', '#18e6e4'] },
  { id: 'night', name: 'Night', swatch: ['#0a1012', '#edf2f1', '#18e6e4'] },
  { id: 'ashen', name: 'Ashen', swatch: ['#2d3030', '#f3f4f3', '#18e6e4'] },
];
const systemDark = matchMedia('(prefers-color-scheme: dark)');
function themeChoice() {
  let stored = null;
  try { stored = localStorage.getItem('vessel-theme'); } catch { /* the system setting applies */ }
  if (stored === 'light') return 'milky';
  if (stored === 'dark') return 'night';
  return THEMES.some((t) => t.id === stored) ? stored : 'system';
}
function applyTheme() {
  const choice = themeChoice();
  document.documentElement.dataset.theme = choice === 'system' ? (systemDark.matches ? 'night' : 'milky') : choice;
  for (const button of document.querySelectorAll('[data-theme-id]')) button.setAttribute('aria-checked', String(button.dataset.themeId === choice));
}
function renderThemes() {
  $('#themes').replaceChildren(...THEMES.map((theme) => {
    const button = element('button', undefined, 'theme');
    button.type = 'button'; button.setAttribute('role', 'radio'); button.dataset.themeId = theme.id;
    const swatch = element('span', undefined, 'swatch');
    for (const colour of theme.swatch) { const cell = element('span'); cell.style.background = colour; swatch.append(cell); }
    button.append(swatch, element('span', theme.name));
    button.addEventListener('click', () => {
      try { localStorage.setItem('vessel-theme', theme.id); } catch { /* applies for this session */ }
      applyTheme();
    });
    return button;
  }));
  applyTheme();
}
systemDark.addEventListener('change', applyTheme);
applyTheme();

// ---------- Window controls (decorations are off) ----------
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

let games = [], runtimes = [], windowsRuntimes = [], environment = null;
let view = { name: 'library', id: null }, filter = 'all', gameTab = 'dashboard';
let busy = false, ready = false, refreshing = false;
let editingToolId = null, toolGameId = null, removingToolId = null, logTarget = null;

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
  if (!date) return 'Never played';
  const minutes = Math.round((Date.now() - date) / 60000);
  if (minutes < 1) return 'Just now';
  if (minutes < 60) return minutes + ' min ago';
  if (minutes < 24 * 60) return Math.round(minutes / 60) + ' h ago';
  if (minutes < 48 * 60) return 'Yesterday';
  return date.toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: date.getFullYear() === new Date().getFullYear() ? undefined : 'numeric' });
}
function clock(value) { return parseTime(value)?.toLocaleString(undefined, { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' }) ?? ''; }
function runnerName(game) {
  if (!game.runtime_path) return 'No runner';
  const parts = game.runtime_path.split('/');
  return game.runtime_kind === 'proton' ? parts[parts.length - 2] || 'Proton' : parts[parts.length - 1];
}
function element(tag, text, className) {
  const node = document.createElement(tag);
  if (text !== undefined) node.textContent = text;
  if (className) node.className = className;
  return node;
}
function list(names) { return names.length < 2 ? names.join('') : names.slice(0, -1).join(', ') + ' and ' + names[names.length - 1]; }
const KINDS = { mod_manager: 'Mod manager', script_extender: 'Script extender', trainer: 'Trainer', utility: 'Utility' };

// ---------- Game state ----------
function current() { return view.name === 'game' ? games.find((g) => g.id === view.id) : undefined; }
function running(item) { return item.launch?.state === 'running' || item.launch?.state === 'starting'; }
// Mod managers and script extenders start the game; trainers and utilities attach to it.
function startsGame(kind) { return kind === 'mod_manager' || kind === 'script_extender'; }
function starters(game) { return (game.tools || []).filter((t) => startsGame(t.kind)); }
function attachers(game) { return (game.tools || []).filter((t) => !startsGame(t.kind)); }
function starter(game) { return starters(game).find((t) => t.id === game.launch_via_tool_id) || null; }
function companionsOn(game) { return attachers(game).filter((t) => t.with_game); }
function roomInUse(game) { return !!game.room?.active || running(game) || (game.tools || []).some(running); }
function gameRunning(game) { return running(game) || !!game.room?.active || (game.tools || []).some((t) => startsGame(t.kind) && running(t)); }
function needsSetup(game) { return !game.available || !game.runtime_path; }
function canPlay(game) { return !busy && !roomInUse(game) && game.available && !!game.runtime_path; }
function statusOf(game) {
  if (!game.available) return ['The game’s executable is missing', 'attention'];
  if (gameRunning(game)) return ['Running', 'running'];
  if (game.launch?.state === 'failed') return ['The last launch failed', 'attention'];
  if (!game.runtime_path) return ['Choose a runner in the Room tab', 'attention'];
  return ['Ready to play', 'ready'];
}
// The setup in words, so Play never surprises anyone.
function setupSentence(game) {
  const start = starter(game);
  const extras = companionsOn(game).map((t) => t.name);
  const first = start ? 'Play opens ' + start.name + ', which starts ' + game.name : 'Play starts ' + game.name;
  return first + (extras.length ? ', then opens ' + list(extras) + '.' : '.');
}
function setupShort(game) {
  const start = starter(game);
  const extras = companionsOn(game).map((t) => t.name);
  return [start ? 'Through ' + start.name : runnerName(game), extras.length ? 'with ' + list(extras) : null].filter(Boolean).join(' · ');
}
function artwork(game) {
  const executable = game.executable.toLowerCase();
  if (executable.endsWith('/skyrimse.exe')) return { src: 'https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/489830/library_hero.jpg', credit: 'Bethesda · Steam' };
  if (executable.endsWith('/mines.exe')) return { src: '/artwork/mines.png', credit: 'Simon Tatham’s Portable Puzzles' };
  return null;
}
// Artwork, falling back to the game's name on a quiet field if the image is missing or fails.
function fillArt(frame, game) {
  const art = artwork(game);
  const fallback = element('span', game.name, 'art-fallback');
  frame.replaceChildren();
  if (art) {
    const image = element('img');
    image.src = art.src; image.alt = ''; image.referrerPolicy = 'no-referrer'; image.loading = 'lazy';
    image.addEventListener('error', () => { image.remove(); fallback.hidden = false; });
    fallback.hidden = true; frame.append(image); frame.title = 'Artwork: ' + art.credit;
  }
  frame.append(fallback);
}

// ---------- Shell ----------
function renderShell() {
  for (const link of document.querySelectorAll('[data-nav]')) {
    const active = link.dataset.nav === view.name || (view.name === 'game' && link.dataset.nav === 'library');
    if (active) link.setAttribute('aria-current', 'page'); else link.removeAttribute('aria-current');
  }
  $('#nav-count').textContent = games.length ? String(games.length) : '';
  const rows = games.filter(roomInUse).map((game) => {
    const item = element('li'); const link = element('a'); link.href = '#/game/' + game.id;
    const apps = (game.tools || []).filter(running).map((t) => t.name);
    link.append(element('strong', game.name), element('span', apps.length ? apps.join(', ') : 'Game'));
    item.append(link); return item;
  });
  $('#running-list').replaceChildren(...(rows.length ? rows : [element('li', 'Nothing running', 'empty')]));
  for (const name of ['library', 'game', 'dependencies', 'settings']) $('#view-' + name).hidden = view.name !== name;
}

// ---------- Library ----------
function matches(game, name) {
  if (name === 'running') return roomInUse(game);
  if (name === 'ready') return !needsSetup(game) && !roomInUse(game);
  if (name === 'setup') return needsSetup(game);
  return true;
}
function miniStrip(game) {
  const strip = element('span', undefined, 'mini-strip');
  strip.title = setupSentence(game);
  strip.setAttribute('role', 'img'); strip.setAttribute('aria-label', setupSentence(game));
  const start = element('i', undefined, 'start'); strip.append(start);
  for (const tool of attachers(game)) strip.append(element('i', undefined, tool.with_game ? 'on' : ''));
  return strip;
}
function gameRow(game) {
  const row = element('li', undefined, 'game-row');
  const link = element('a', undefined, 'row-link'); link.href = '#/game/' + game.id;
  const thumb = element('span', undefined, 'thumb'); fillArt(thumb, game);
  const text = element('span', undefined, 'row-text');
  const [status, state] = statusOf(game);
  text.append(element('span', game.name, 'row-name'), element('span', state === 'ready' ? setupShort(game) : status, 'row-sub'));
  if (state === 'attention') text.lastChild.style.color = 'var(--negative)';
  const time = element('span', undefined, 'row-time num');
  time.append(element('strong', game.play_seconds ? duration(game.play_seconds) : '0 m'), document.createTextNode(relative(game.last_played_at)));
  link.append(thumb, text, miniStrip(game), time);
  const playCell = element('span', undefined, 'row-play');
  const play = element('button', gameRunning(game) ? 'Running' : 'Play', 'button');
  play.type = 'button'; play.disabled = !canPlay(game);
  play.setAttribute('aria-label', (gameRunning(game) ? game.name + ' is running' : 'Play ' + game.name));
  play.addEventListener('click', () => playGame(game.id));
  playCell.append(play);
  row.append(link, playCell);
  return row;
}
function renderLibrary() {
  const query = $('#search').value.trim().toLocaleLowerCase();
  for (const tab of document.querySelectorAll('[data-filter]')) {
    tab.setAttribute('aria-selected', String(tab.dataset.filter === filter));
    tab.querySelector('[data-count]').textContent = String(games.filter((g) => matches(g, tab.dataset.filter)).length);
  }
  const sorted = [...games].sort((a, b) => (b.last_played_at || '').localeCompare(a.last_played_at || '') || a.name.localeCompare(b.name));
  const shown = sorted.filter((g) => matches(g, filter) && g.name.toLocaleLowerCase().includes(query));
  const playing = games.filter(gameRunning).map((g) => g.name);
  $('#library-summary').textContent = !ready ? 'Loading your games' :
    (games.length === 1 ? '1 game' : games.length + ' games') + (playing.length ? ' · ' + list(playing) + (playing.length === 1 ? ' is' : ' are') + ' running' : '');
  if (!shown.length) {
    const empty = element('li', undefined, 'empty-state');
    if (!games.length && ready) {
      empty.append(element('p', 'No games yet. Vessel can look in common game folders, or you can add a game’s .exe.'));
      const actions = element('div', undefined, 'actions');
      const find = element('button', 'Find games', 'button'); find.type = 'button'; find.addEventListener('click', () => scan());
      const add = element('button', 'Add game', 'button'); add.type = 'button'; add.addEventListener('click', addGame);
      actions.append(find, add); empty.append(actions);
    } else empty.append(element('p', ready ? (query ? 'No game matches “' + $('#search').value.trim() + '”.' : 'No games here.') : 'Loading…'));
    $('#game-list').replaceChildren(empty);
    return;
  }
  $('#game-list').replaceChildren(...shown.map(gameRow));
}

// ---------- Game page ----------
function block({ kind, name, state, role, checked, disabled, onClick, missing }) {
  const button = element('button', undefined, 'block' + (missing ? ' missing' : ''));
  button.type = 'button';
  button.setAttribute(role === 'radio' ? 'role' : 'data-role', role);
  if (role === 'radio') button.setAttribute('aria-checked', String(checked)); else button.setAttribute('aria-pressed', String(checked));
  button.disabled = disabled;
  const stateNode = element('span', undefined, 'block-state');
  if (state.running) stateNode.append(element('i', undefined, 'running-dot'));
  stateNode.append(document.createTextNode(state.text));
  button.append(element('span', kind, 'block-kind'), element('span', name, 'block-name'), stateNode);
  button.addEventListener('click', onClick);
  return button;
}
function renderStrip(game) {
  const locked = busy || roomInUse(game);
  const start = starter(game);
  const startBlocks = [block({
    kind: 'Game', name: game.name, role: 'radio', checked: !start, disabled: locked,
    state: { text: !start ? 'Starts directly' : 'Not used', running: running(game) },
    onClick: () => setStarter(game, null),
  })];
  for (const tool of starters(game)) startBlocks.push(block({
    kind: KINDS[tool.kind], name: tool.name, role: 'radio', checked: start?.id === tool.id, disabled: locked || !tool.available, missing: !tool.available,
    state: { text: !tool.available ? 'Executable missing' : running(tool) ? 'Running' : start?.id === tool.id ? 'Starts the game' : 'Not used', running: running(tool) },
    onClick: () => setStarter(game, tool.id),
  }));
  $('#start-blocks').replaceChildren(...startBlocks);
  const withBlocks = attachers(game).map((tool) => block({
    kind: KINDS[tool.kind], name: tool.name, role: 'switch', checked: tool.with_game, disabled: busy || !tool.available, missing: !tool.available,
    state: { text: !tool.available ? 'Executable missing' : running(tool) ? 'Running' : tool.with_game ? 'On' : 'Off', running: running(tool) },
    onClick: () => toggleWithGame(tool),
  }));
  const add = element('button', undefined, 'block add'); add.type = 'button'; add.disabled = busy;
  const plus = element('span', undefined, 'icon'); plus.innerHTML = icon('plus');
  add.append(plus, element('span', 'Add app'));
  add.addEventListener('click', () => editTool());
  $('#with-blocks').replaceChildren(...withBlocks, add);
  const isRunning = gameRunning(game);
  $('#play').disabled = !canPlay(game);
  $('#play').dataset.running = String(isRunning);
  $('#play-label').textContent = isRunning ? 'Running' : 'Play';
  $('#stop-room').hidden = !game.room?.active;
  $('#stop-room').disabled = busy;
  $('#setup-sentence').textContent = isRunning ? game.name + ' is running. Trainers and utilities you open now join it.' : setupSentence(game);
}
function renderChart(game) {
  const days = [];
  const today = new Date();
  for (let i = 13; i >= 0; i--) {
    const d = new Date(Date.UTC(today.getUTCFullYear(), today.getUTCMonth(), today.getUTCDate() - i));
    days.push({ key: d.toISOString().slice(0, 10), date: d, seconds: 0 });
  }
  for (const day of game.daily_play || []) { const slot = days.find((d) => d.key === day.date); if (slot) slot.seconds += day.seconds; }
  const total = days.reduce((sum, d) => sum + d.seconds, 0);
  $('#chart-total').textContent = duration(total);
  const maxMinutes = Math.max(...days.map((d) => d.seconds / 60));
  const steps = [5, 10, 15, 30, 60, 120, 180, 240, 360, 480, 720];
  const top = steps.find((s) => s > maxMinutes) || Math.ceil(maxMinutes / 60) * 60 + 60;
  const W = Math.max(280, $('#chart').clientWidth || 480), H = 180, left = 40, bottom = 22, plot = H - bottom - 8, slot = (W - left) / days.length;
  const ns = 'http://www.w3.org/2000/svg';
  const svg = document.createElementNS(ns, 'svg');
  svg.setAttribute('viewBox', `0 0 ${W} ${H}`); svg.setAttribute('role', 'img');
  svg.setAttribute('aria-label', 'Playtime per day over the last 14 days, ' + duration(total) + ' in total');
  const add = (tag, attrs, text) => { const n = document.createElementNS(ns, tag); for (const [k, v] of Object.entries(attrs)) n.setAttribute(k, v); if (text != null) n.textContent = text; svg.append(n); return n; };
  for (const value of [0, top / 2, top]) {
    const y = 8 + plot - (value / top) * plot;
    add('line', { x1: left, x2: W, y1: y, y2: y, class: value ? 'grid-line' : 'base-line' });
    add('text', { x: left - 8, y: y + 4, 'text-anchor': 'end', class: 'tick' }, value >= 60 ? value / 60 + ' h' : value + ' m');
  }
  days.forEach((day, i) => {
    const h = (day.seconds / 60 / top) * plot;
    const x = left + i * slot + slot * 0.18;
    if (h > 0) {
      const bar = add('rect', { x, y: 8 + plot - h, width: slot * 0.64, height: Math.max(h, 1.5), class: i === 13 ? 'bar today' : 'bar' });
      const title = document.createElementNS(ns, 'title');
      title.textContent = day.date.toLocaleDateString(undefined, { weekday: 'short', day: 'numeric', month: 'short', timeZone: 'UTC' }) + ': ' + duration(day.seconds);
      bar.append(title);
    }
    if (i % 3 === 1 || i === 13) add('text', { x: x + slot * 0.32, y: H - 6, 'text-anchor': 'middle', class: 'tick' }, i === 13 ? 'Today' : day.date.toLocaleDateString(undefined, { day: 'numeric', month: 'short', timeZone: 'UTC' }));
  });
  $('#chart').replaceChildren(svg);
}
function renderDashboard(game) {
  const finished = (game.sessions || []).filter((s) => s.seconds != null);
  const weekStart = new Date(Date.now() - 6 * 86400000).toISOString().slice(0, 10);
  $('#fig-total').textContent = duration(game.play_seconds || 0);
  $('#fig-week').textContent = duration((game.daily_play || []).filter((d) => d.date >= weekStart).reduce((sum, d) => sum + d.seconds, 0));
  $('#fig-last').textContent = finished[0] ? duration(finished[0].seconds) : '—';
  $('#fig-sessions').textContent = String((game.sessions || []).length);
  renderChart(game);
  $('#view-log').disabled = !game.launch;
  const rows = (game.sessions || []).slice(0, 8).map((session) => {
    const item = element('li');
    const live = session.state === 'running' || session.state === 'starting';
    item.append(element('span', clock(session.started_at) + (session.via ? ' · ' + session.via : ''), 'when'),
      element('span', live ? 'Playing now' : duration(session.seconds), 'num' + (live ? ' live' : '')));
    if (session.seconds == null && !live) item.lastChild.title = 'This session ended while Vessel was closed, so its length is unknown';
    return item;
  });
  $('#session-list').replaceChildren(...(rows.length ? rows : [element('li', 'No sessions yet. Press Play to start one.', 'empty')]));
}
function renderApps(game) {
  $('#add-tool').disabled = busy;
  const rows = (game.tools || []).map((tool) => {
    const row = element('tr');
    const name = element('td', tool.name, 'name'); name.append(element('span', tool.executable));
    const state = !tool.available ? 'Executable missing' : running(tool) ? 'Running' : tool.launch?.state === 'failed' ? 'Last launch failed' : startsGame(tool.kind) ? (starter(game)?.id === tool.id ? 'Starts the game' : 'Not used by Play') : tool.with_game ? 'Opens with the game' : 'Opens when you choose';
    const actions = element('div', undefined, 'actions');
    const open = element('button', running(tool) ? 'Running' : startsGame(tool.kind) || gameRunning(game) ? 'Open' : 'Play with game', 'button');
    open.type = 'button'; open.disabled = busy || running(tool) || !tool.available || !game.runtime_path;
    open.addEventListener('click', () => openTool(tool.id));
    const log = element('button', 'Log', 'button'); log.type = 'button'; log.disabled = !tool.launch;
    log.addEventListener('click', () => showLog({ type: 'tool', id: tool.id, name: tool.name }));
    const edit = element('button', 'Edit', 'button'); edit.type = 'button'; edit.disabled = busy || running(tool);
    edit.addEventListener('click', () => editTool(tool));
    const remove = element('button', 'Remove', 'button danger'); remove.type = 'button'; remove.disabled = busy || running(tool);
    remove.addEventListener('click', () => removeToolDialog(tool));
    actions.append(open, log, edit, remove);
    const cell = element('td', undefined, 'row-actions'); cell.append(actions);
    row.append(name, element('td', KINDS[tool.kind], 'nowrap'), element('td', state, 'nowrap'), cell);
    return row;
  });
  if (!rows.length) {
    const row = element('tr', undefined, 'empty'); const cell = element('td', 'No companion apps yet. Add a mod manager, script extender, trainer or utility.'); cell.colSpan = 4; row.append(cell); rows.push(row);
  }
  $('#tool-rows').replaceChildren(...rows);
}
function screenSize() { return [Math.round(screen.width * devicePixelRatio), Math.round(screen.height * devicePixelRatio)]; }
let roomFormGame = null;
function renderRoom(game, force = false) {
  const locked = busy || roomInUse(game);
  $('#room-lock').hidden = !roomInUse(game);
  const editing = !force && roomFormGame === game.id && document.activeElement && $('#panel-room').contains(document.activeElement);
  if (!editing) {
    roomFormGame = game.id;
    $('#game-name').value = game.name;
    const select = $('#runtime-choice'); select.replaceChildren(new Option('Choose a runner', ''));
    for (const runtime of runtimes.filter((r) => r.kind !== 'umu')) select.append(new Option(runtime.name + ' · ' + runtime.kind, runtime.kind + '|' + runtime.path));
    select.append(new Option('Another runner…', 'custom'));
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
    const profile = game.room?.profile || {};
    $('#room-network').value = profile.network || 'online';
    $('#room-display').value = profile.display || 'auto';
    $('#room-audio').checked = profile.audio !== false;
    $('#room-controllers').checked = profile.controllers !== false;
    const packages = windowsRuntimes.filter((r) => r.manifest).map((runtime) => {
      const label = element('label'); const box = element('input'); box.type = 'checkbox'; box.value = runtime.digest;
      box.checked = (profile.packages || []).includes(runtime.digest);
      label.append(box, document.createTextNode(runtime.manifest.id + ' ' + runtime.manifest.version + ' · ' + runtime.manifest.architecture));
      return label;
    });
    $('#room-packages').replaceChildren(...(packages.length ? packages : [element('p', 'No shared packages imported yet.', 'empty')]));
  }
  for (const control of $('#panel-room').querySelectorAll('input, select, textarea, button')) control.disabled = locked;
  const oneWindow = game.display_supported;
  for (const control of $('#display-modes').querySelectorAll('input')) control.disabled = locked || !oneWindow;
  const sized = ['fullscreen', 'windowed'].includes(document.querySelector('[name="display-mode"]:checked')?.value);
  $('#display-width').disabled = $('#display-height').disabled = locked || !oneWindow || !sized;
  $('#use-screen').disabled = locked || !oneWindow || !sized;
  $('#display-note').textContent = oneWindow ? 'Everything in this room shares one window. Vessel sets its mode and size, never the game’s files.' :
    'Each app gets its own window on your desktop, and the game uses its own display settings.';
  $('#executable-path').textContent = game.executable;
  $('#prefix-path').textContent = game.prefix;
  $('#runtime-path').textContent = game.runtime_path || 'Not chosen';
  const recovery = game.recovery || { supported: false, checkpoints: [] };
  const checkpoint = $('#recovery-checkpoint'); const previous = checkpoint.value;
  checkpoint.replaceChildren(...(recovery.checkpoints.length ? recovery.checkpoints.map((c) => new Option(clock(c.created_at) + ' · ' + c.label, String(c.id))) : [new Option('No restore points yet', '')]));
  if ([...checkpoint.options].some((o) => o.value === previous)) checkpoint.value = previous;
  checkpoint.disabled = locked || !recovery.checkpoints.length;
  $('#checkpoint-room').disabled = locked || !recovery.supported;
  $('#restore-room').disabled = locked || !recovery.supported || !recovery.checkpoints.length;
  $('#recovery-note').textContent = recovery.supported ? 'Vessel saves a restore point before the room opens. Restoring brings back the setup and keeps your current saves.' :
    'Restore points are not available for this game yet.';
}
function renderGame() {
  const game = current();
  if (!game) return;
  fillArt($('#game-art'), game);
  $('#game-title').textContent = game.name;
  $('#game-meta').textContent = [game.runtime_kind === 'proton' ? 'Proton' : 'Wine', runnerName(game), game.last_played_at ? 'Played ' + relative(game.last_played_at).toLocaleLowerCase() : 'Not played yet'].join(' · ');
  const [status, state] = statusOf(game);
  $('#game-state').textContent = status; $('#game-state').dataset.state = state;
  renderStrip(game);
  for (const tab of document.querySelectorAll('[data-tab]')) tab.setAttribute('aria-selected', String(tab.dataset.tab === gameTab));
  for (const name of ['dashboard', 'apps', 'room']) $('#panel-' + name).hidden = gameTab !== name;
  if (gameTab === 'dashboard') renderDashboard(game);
  if (gameTab === 'apps') renderApps(game);
  if (gameTab === 'room') renderRoom(game);
}

// ---------- Dependencies and settings ----------
function renderDependencies() {
  const healthy = windowsRuntimes.filter((r) => r.manifest);
  $('#deps-summary').textContent = healthy.length ? healthy.length + (healthy.length === 1 ? ' package' : ' packages') + ', stored once and shared read-only with the rooms that use them.' :
    'Packages such as Visual C++ or .NET runtimes, stored once and shared read-only with the rooms that use them.';
  const rows = windowsRuntimes.map((runtime) => {
    const row = element('tr');
    if (!runtime.manifest) { const cell = element('td', 'A package needs repair: ' + runtime.error); cell.colSpan = 4; cell.className = 'error'; row.append(cell); return row; }
    const users = games.filter((g) => (g.room?.profile?.packages || []).includes(runtime.digest)).map((g) => g.name);
    const name = element('td', runtime.manifest.id, 'name'); name.append(element('span', runtime.digest.slice(0, 12)));
    row.append(name, element('td', runtime.manifest.version, 'num'), element('td', runtime.manifest.architecture), element('td', users.length ? users.join(', ') : 'No game yet'));
    return row;
  });
  if (!rows.length) { const row = element('tr', undefined, 'empty'); const cell = element('td', 'No packages imported. Your runner’s built-in libraries are used.'); cell.colSpan = 4; row.append(cell); rows.push(row); }
  $('#dependency-rows').replaceChildren(...rows);
}
function renderSettings() {
  $('#runtime-summary').textContent = runtimes.length ? runtimes.length + (runtimes.length === 1 ? ' runner' : ' runners') + ' found on this computer.' : 'No Proton or Wine build found yet.';
  $('#runtimes').replaceChildren(...(runtimes.length ? runtimes.map((r) => { const item = element('li'); item.append(element('strong', r.name), element('span', r.kind + ' · ' + r.path, 'path')); return item; }) : [element('li', 'Install Proton through UMU, or Wine, then look again.', 'muted')]));
  $('#database-path').textContent = environment?.database_path || '—';
  applyTheme();
}
function render() {
  renderShell();
  if (view.name === 'library') renderLibrary();
  if (view.name === 'game') renderGame();
  if (view.name === 'dependencies') renderDependencies();
  if (view.name === 'settings') renderSettings();
}

// ---------- Navigation ----------
function parseRoute() {
  const [, name, id] = location.hash.match(/^#\/(library|game|dependencies|settings)(?:\/(\d+))?/) || [];
  return { name: name || 'library', id: id ? Number(id) : null };
}
function route() {
  const next = parseRoute();
  if (next.name === 'game' && ready && !games.some((g) => g.id === next.id)) { location.hash = '#/library'; return; }
  const changed = next.name !== view.name || next.id !== view.id;
  if (changed && next.name === 'game') { gameTab = 'dashboard'; roomFormGame = null; }
  view = next;
  render();
  if (changed) $('#canvas').scrollTo({ top: 0 });
}
window.addEventListener('hashchange', route);

// ---------- Actions ----------
function setBusy(value) { busy = value; render(); }
async function loadGames() {
  if (refreshing) return;
  refreshing = true;
  try {
    games = await invoke('list_games');
    if (view.name === 'game' && !games.some((g) => g.id === view.id)) location.hash = '#/library';
    render();
    updateLaunch();
  } finally { refreshing = false; }
}
async function scan(directory = null) {
  setBusy(true); message('Looking for Windows games…');
  try {
    const result = await invoke('scan_games', { directory });
    await loadGames();
    message('Added ' + result.added + (result.added === 1 ? ' game' : ' games') + ' from ' + result.roots.length + ' folders.' +
      (result.limited ? ' The scan stopped early; scan a more specific folder for more.' : '') + (result.skipped_directories ? ' Some folders could not be read.' : ''));
  } catch (error) { message(String(error), true); }
  finally { setBusy(false); }
}
async function addGame() {
  try { const path = await open({ multiple: false, filters: [{ name: 'Windows game', extensions: ['exe', 'EXE'] }], title: 'Choose a game’s .exe' }); if (path) await importPath(path); }
  catch (error) { message(String(error), true); }
}
async function importPath(path, name = null) {
  setBusy(true);
  try { const game = await invoke('import_game', { path, name }); await loadGames(); message(game.name + ' added.'); location.hash = '#/game/' + game.id; }
  finally { setBusy(false); }
}
async function refreshEnvironment() {
  const [status, dependencies] = await Promise.all([invoke('get_environment_status'), invoke('list_windows_runtimes')]);
  environment = status; windowsRuntimes = dependencies; runtimes = status.runtimes;
  render();
}
// Save one game's settings, starting from what is stored and changing only what was asked.
async function saveGame(game, changes) {
  const settings = { id: game.id, name: game.name, runtime_kind: game.runtime_kind, runtime_path: game.runtime_path, arguments: game.arguments,
    display_mode: game.display_mode, display_width: game.display_width, display_height: game.display_height, launch_via_tool_id: game.launch_via_tool_id, ...changes };
  if (settings.display_mode !== 'default' && (settings.display_width == null || settings.display_height == null)) [settings.display_width, settings.display_height] = screenSize();
  await invoke('save_game', { settings });
}
function stamp(text) { $('#saved-stamp').textContent = text; }
async function autosave(work, done = 'Saved') {
  const game = current(); if (!game || busy) return;
  setBusy(true); stamp('Saving…');
  try { await work(game); await loadGames(); stamp(done); }
  catch (error) { stamp(''); message(String(error), true); roomFormGame = null; render(); }
  finally { setBusy(false); }
}
async function setStarter(game, toolId) {
  if (game.launch_via_tool_id === toolId || (toolId == null && !starter(game))) return;
  setBusy(true);
  try { await saveGame(game, { launch_via_tool_id: toolId }); await loadGames(); message(toolId ? 'Play now goes through ' + game.tools.find((t) => t.id === toolId).name + '.' : 'Play now starts ' + game.name + ' directly.'); }
  catch (error) { message(String(error), true); }
  finally { setBusy(false); }
}
async function toggleWithGame(tool) {
  setBusy(true);
  try {
    await invoke('save_tool', { settings: { id: tool.id, game_id: tool.game_id, kind: tool.kind, name: tool.name, executable: tool.executable, working_directory: tool.working_directory, arguments: tool.arguments, with_game: !tool.with_game } });
    await loadGames(); message(tool.name + (tool.with_game ? ' no longer opens with the game.' : ' now opens with the game.'));
  } catch (error) { message(String(error), true); }
  finally { setBusy(false); }
}

// ---------- Launch screen: the strip fills as each part comes up ----------
let launch = null;
function showLaunch(game) {
  const start = starter(game);
  const steps = [{ id: 'room', label: 'Preparing ' + game.name + '’s room' }];
  steps.push({ id: 'start', label: start ? 'Starting ' + start.name : 'Starting ' + game.name });
  for (const tool of companionsOn(game).filter((t) => !running(t))) steps.push({ id: 'tool-' + tool.id, label: 'Opening ' + tool.name, tool: tool.id });
  launch = { gameId: game.id, steps, states: { room: 'active' }, started: Date.now(), readySince: null, done: false, failed: false };
  $('#launch-title').textContent = 'Starting ' + game.name;
  $('#launch-note').textContent = 'The first start of a room can take a minute while Proton prepares it.';
  $('#launch-log').hidden = true;
  $('#launch').hidden = false; $('#launch').classList.add('busy');
  drawLaunch();
  $('#launch-hide').focus();
}
function drawLaunch() {
  if (!launch) return;
  $('#launch-steps').replaceChildren(...launch.steps.map((step) => { const item = element('li', step.label); item.dataset.state = launch.states[step.id] || 'waiting'; return item; }));
}
function hideLaunch() { launch = null; $('#launch').hidden = true; $('#launch').classList.remove('busy'); }
function failLaunch(stepId, text) {
  if (!launch) return;
  launch.failed = true; launch.states[stepId] = 'failed';
  $('#launch').classList.remove('busy');
  $('#launch-title').textContent = 'It did not start';
  $('#launch-note').textContent = text;
  $('#launch-log').hidden = false;
  drawLaunch();
}
async function updateLaunch() {
  if (!launch || launch.failed || launch.done || launch.states.room !== 'done') return;
  const game = games.find((g) => g.id === launch.gameId);
  if (!game) return;
  const start = starter(game);
  const record = start ? start.launch : game.launch;
  if (record?.state === 'failed') { failLaunch('start', record.error || 'The launch ended with an error. Its log says why.'); return; }
  if (launch.states.start !== 'done') {
    const isReady = await invoke('room_ready', { gameId: game.id }).catch(() => false);
    if (isReady) launch.states.start = 'done';
  }
  for (const step of launch.steps.filter((s) => s.tool)) {
    if (launch.states.start !== 'done') break;
    const tool = game.tools.find((t) => t.id === step.tool);
    if (tool?.launch?.state === 'failed') launch.states[step.id] = 'failed';
    else if (tool && running(tool)) launch.states[step.id] = 'done';
    else launch.states[step.id] = 'active';
  }
  const pending = launch.steps.some((s) => launch.states[s.id] !== 'done' && launch.states[s.id] !== 'failed');
  if (!pending) {
    launch.done = true;
    $('#launch').classList.remove('busy');
    const failedTools = launch.steps.filter((s) => launch.states[s.id] === 'failed').map((s) => s.label.replace('Opening ', ''));
    $('#launch-title').textContent = game.name + ' is running';
    $('#launch-note').textContent = failedTools.length ? list(failedTools) + ' did not open. Its log in Companion apps says why.' : 'Its windows open on your desktop. Vessel stays here when you need it.';
    setTimeout(() => { if (launch?.done) hideLaunch(); }, failedTools.length ? 8000 : 3000);
  } else if (launch.states.start !== 'done') launch.states.start = 'active';
  drawLaunch();
}
async function playGame(id) {
  const game = games.find((g) => g.id === id);
  if (!game || !canPlay(game)) return;
  showLaunch(game);
  setBusy(true);
  try {
    await invoke('launch_game', { gameId: id });
    launch.states.room = 'done'; launch.states.start = 'active'; drawLaunch();
    message('Started ' + game.name + '.');
  } catch (error) { failLaunch('room', String(error)); message(String(error), true); }
  finally { setBusy(false); await loadGames().catch(() => {}); }
}
async function openTool(id) {
  const game = current();
  const tool = game?.tools.find((t) => t.id === id);
  if (!tool) return;
  // A trainer or utility on a stopped game starts the game first: show that as a launch.
  if (!startsGame(tool.kind) && !gameRunning(game)) showLaunch({ ...game, tools: game.tools.map((t) => ({ ...t, with_game: t.id === id })) });
  setBusy(true);
  try {
    message('Opening ' + tool.name + '…');
    await invoke('launch_tool', { toolId: id });
    if (launch) { launch.states.room = 'done'; launch.states.start = 'active'; drawLaunch(); }
    message('Opened ' + tool.name + '.');
  } catch (error) { if (launch) failLaunch('room', String(error)); message(String(error), true); }
  finally { setBusy(false); await loadGames().catch(() => {}); }
}
async function showLog(target = logTarget) {
  if (!target) return;
  logTarget = target;
  $('#log-title').textContent = target.type === 'tool' ? target.name + ' log' : 'Launch log';
  $('#log-content').textContent = 'Loading…';
  if (!$('#log-dialog').open) $('#log-dialog').showModal();
  try { $('#log-content').textContent = await invoke(target.type === 'tool' ? 'tool_log' : 'game_log', target.type === 'tool' ? { toolId: target.id } : { gameId: target.id }); }
  catch (error) { $('#log-content').textContent = String(error); }
}
function editTool(tool = null) {
  if (!current()) return;
  toolGameId = view.id; editingToolId = tool?.id ?? null;
  $('#tool-dialog-title').textContent = tool ? 'Edit ' + tool.name : 'Add companion app';
  $('#tool-kind').value = tool?.kind || 'mod_manager';
  $('#tool-name').value = tool?.name || '';
  $('#tool-executable').value = tool?.executable || '';
  $('#tool-working-directory').value = tool?.working_directory || '';
  $('#tool-arguments').value = (tool?.arguments || []).join('\n');
  $('#tool-dialog').dataset.withGame = String(!!tool?.with_game);
  $('#tool-error').hidden = true;
  $('#tool-dialog').showModal();
}
function removeToolDialog(tool) {
  removingToolId = tool.id;
  $('#remove-tool-description').textContent = 'Remove ' + tool.name + ' from ' + current().name + '?';
  $('#remove-tool-error').hidden = true;
  $('#remove-tool-dialog').showModal();
}

// ---------- Events ----------
$('#scan').addEventListener('click', () => scan());
$('#scan-again').addEventListener('click', () => scan());
$('#add-game').addEventListener('click', addGame);
$('#scan-folder').addEventListener('click', async () => {
  try { const folder = await open({ directory: true, multiple: false, title: 'Choose a folder to scan for games' }); if (folder) await scan(folder); }
  catch (error) { message(String(error), true); }
});
$('#enter-path').addEventListener('click', () => { $('#import-error').hidden = true; $('#import-dialog').showModal(); });
$('#import-form').addEventListener('submit', async (event) => {
  event.preventDefault();
  try { await importPath($('#import-path').value, $('#import-name').value || null); $('#import-dialog').close(); event.target.reset(); }
  catch (error) { $('#import-error').textContent = String(error); $('#import-error').hidden = false; }
});
$('#search').addEventListener('input', renderLibrary);
for (const tab of document.querySelectorAll('[data-filter]')) tab.addEventListener('click', () => { filter = tab.dataset.filter; renderLibrary(); });
for (const tab of document.querySelectorAll('[data-tab]')) tab.addEventListener('click', () => { gameTab = tab.dataset.tab; roomFormGame = null; renderGame(); });
$('#play').addEventListener('click', () => { if (current()) playGame(view.id); });
$('#stop-room').addEventListener('click', async () => {
  const game = current(); if (!game || busy) return;
  setBusy(true); message('Closing every app in ' + game.name + '’s room…');
  try { await invoke('stop_room', { gameId: game.id }); await loadGames(); message(game.name + '’s room is closed.'); }
  catch (error) { message(String(error), true); }
  finally { setBusy(false); }
});
$('#view-log').addEventListener('click', () => showLog({ type: 'game', id: view.id }));
$('#refresh-log').addEventListener('click', () => showLog());
$('#launch-hide').addEventListener('click', hideLaunch);
$('#launch-log').addEventListener('click', () => { const id = launch?.gameId; hideLaunch(); if (id) showLog({ type: 'game', id }); });
$('#add-tool').addEventListener('click', () => editTool());
$('#refresh').addEventListener('click', () => refreshEnvironment().catch((error) => message(String(error), true)));

// Room tab: each control saves itself.
$('#runtime-choice').addEventListener('change', () => {
  const choice = $('#runtime-choice').value;
  $('#custom-runtime').hidden = choice !== 'custom';
  if (!choice || choice === 'custom') return;
  const separator = choice.indexOf('|');
  autosave((game) => saveGame(game, { runtime_kind: choice.slice(0, separator), runtime_path: choice.slice(separator + 1) }), 'Runner saved');
});
for (const id of ['custom-runtime-kind', 'custom-runtime-path']) $('#' + id).addEventListener('change', () => {
  const path = $('#custom-runtime-path').value.trim(); if (!path) return;
  autosave((game) => saveGame(game, { runtime_kind: $('#custom-runtime-kind').value, runtime_path: path }), 'Runner saved');
});
$('#browse-runtime').addEventListener('click', async () => {
  try { const path = await open({ multiple: false, title: 'Choose the proton script or Wine executable' }); if (path) { $('#custom-runtime-path').value = path; $('#custom-runtime-path').dispatchEvent(new Event('change')); } }
  catch (error) { message(String(error), true); }
});
$('#argument-lines').addEventListener('change', () => autosave((game) => saveGame(game, { arguments: $('#argument-lines').value.split('\n').filter((line) => line.length > 0) }), 'Arguments saved'));
$('#game-name').addEventListener('change', () => { const name = $('#game-name').value.trim(); if (name) autosave((game) => saveGame(game, { name }), 'Name saved'); });
function saveDisplay() {
  const mode = document.querySelector('[name="display-mode"]:checked')?.value || 'default';
  autosave((game) => saveGame(game, { display_mode: mode, display_width: mode === 'default' ? null : Number($('#display-width').value), display_height: mode === 'default' ? null : Number($('#display-height').value) }), 'Display saved');
}
for (const radio of document.querySelectorAll('[name="display-mode"]')) radio.addEventListener('change', saveDisplay);
for (const id of ['display-width', 'display-height']) $('#' + id).addEventListener('change', saveDisplay);
$('#use-screen').addEventListener('click', () => { const [w, h] = screenSize(); $('#display-width').value = w; $('#display-height').value = h; saveDisplay(); });
function saveRoomProfile() {
  autosave((game) => invoke('configure_room', { gameId: game.id, profile: { ...game.room.profile, network: $('#room-network').value, display: $('#room-display').value,
    packages: [...$('#room-packages').querySelectorAll('input:checked')].map((box) => box.value), audio: $('#room-audio').checked, controllers: $('#room-controllers').checked } }), 'Room saved');
}
for (const id of ['room-network', 'room-display', 'room-audio', 'room-controllers']) $('#' + id).addEventListener('change', saveRoomProfile);
$('#room-packages').addEventListener('change', saveRoomProfile);
async function recoverRoom(restore) {
  const game = current();
  if (!game || busy || roomInUse(game)) return;
  const checkpointId = Number($('#recovery-checkpoint').value);
  setBusy(true); message(restore ? 'Restoring ' + game.name + ', keeping your saves…' : 'Saving a restore point for ' + game.name + '…');
  try {
    if (restore) await invoke('restore_room_checkpoint', { gameId: game.id, checkpointId });
    else await invoke('create_room_checkpoint', { gameId: game.id });
    roomFormGame = null; await loadGames();
    message(restore ? 'Restored. Your saves were kept; “Before restore” undoes this.' : 'Restore point saved.');
  } catch (error) { message(String(error), true); await loadGames().catch(() => {}); }
  finally { setBusy(false); }
}
$('#checkpoint-room').addEventListener('click', () => recoverRoom(false));
$('#restore-room').addEventListener('click', () => recoverRoom(true));
$('#install-windows-runtime').addEventListener('click', async () => {
  if (busy) return;
  try {
    const source = await open({ directory: true, multiple: false, title: 'Choose a prepared Windows runtime package' });
    if (!source) return;
    setBusy(true); message('Importing and checking the package…');
    const installed = await invoke('install_windows_runtime', { source });
    await refreshEnvironment(); message(installed.manifest.id + ' imported.');
  } catch (error) { message(String(error), true); }
  finally { setBusy(false); }
});

$('#browse-tool').addEventListener('click', async () => {
  try {
    const path = await open({ multiple: false, title: 'Choose a companion app’s .exe', filters: [{ name: 'Windows executable', extensions: ['exe', 'EXE'] }] });
    if (path) { $('#tool-executable').value = path; if (!$('#tool-name').value) $('#tool-name').value = path.split('/').pop().replace(/\.exe$/i, ''); }
  } catch (error) { $('#tool-error').textContent = String(error); $('#tool-error').hidden = false; }
});
$('#browse-tool-folder').addEventListener('click', async () => {
  try { const path = await open({ directory: true, multiple: false, title: 'Choose the app’s working folder' }); if (path) $('#tool-working-directory').value = path; }
  catch (error) { $('#tool-error').textContent = String(error); $('#tool-error').hidden = false; }
});
$('#tool-form').addEventListener('submit', async (event) => {
  event.preventDefault(); $('#save-tool').disabled = true;
  try {
    await invoke('save_tool', { settings: { id: editingToolId, game_id: toolGameId, kind: $('#tool-kind').value, name: $('#tool-name').value, executable: $('#tool-executable').value,
      working_directory: $('#tool-working-directory').value, arguments: $('#tool-arguments').value.split('\n').filter((line) => line.length > 0), with_game: $('#tool-dialog').dataset.withGame === 'true' } });
    $('#tool-dialog').close(); await loadGames(); message('Saved ' + $('#tool-name').value + '.');
  } catch (error) { $('#tool-error').textContent = String(error); $('#tool-error').hidden = false; }
  finally { $('#save-tool').disabled = false; }
});
$('#remove-tool-form').addEventListener('submit', async (event) => {
  event.preventDefault();
  try { await invoke('remove_tool', { toolId: removingToolId }); $('#remove-tool-dialog').close(); await loadGames(); message('Removed. Its files stay on disk.'); }
  catch (error) { $('#remove-tool-error').textContent = String(error); $('#remove-tool-error').hidden = false; }
});
for (const button of document.querySelectorAll('[data-close]')) button.addEventListener('click', () => $('#' + button.dataset.close).close());

// Keyboard: / or Ctrl+K searches, arrows move through games, Ctrl+Enter plays, Esc backs out.
document.addEventListener('keydown', (event) => {
  const typing = event.target.matches('input, textarea, select');
  if ((event.key === '/' && !typing) || ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k')) {
    event.preventDefault();
    if (view.name !== 'library') location.hash = '#/library';
    requestAnimationFrame(() => { $('#search').focus(); $('#search').select(); });
    return;
  }
  if (event.key === 'Escape') {
    if (!$('#launch').hidden && (launch?.done || launch?.failed)) { hideLaunch(); return; }
    if (event.target === $('#search') && $('#search').value) { $('#search').value = ''; renderLibrary(); return; }
    if (view.name === 'game' && !typing && !document.querySelector('dialog[open]')) location.hash = '#/library';
    return;
  }
  if ((event.ctrlKey || event.metaKey) && event.key === 'Enter' && view.name === 'game') { event.preventDefault(); if (current()) playGame(view.id); return; }
  if (view.name === 'library' && (event.key === 'ArrowDown' || event.key === 'ArrowUp')) {
    const links = [...document.querySelectorAll('.row-link')];
    if (!links.length) return;
    const index = links.indexOf(document.activeElement);
    if (event.target === $('#search') && event.key === 'ArrowDown') { event.preventDefault(); links[0].focus(); return; }
    if (index === -1) return;
    event.preventDefault();
    const next = index + (event.key === 'ArrowDown' ? 1 : -1);
    if (next < 0) $('#search').focus(); else links[Math.min(next, links.length - 1)].focus();
  }
});
$('#search').addEventListener('keydown', (event) => {
  if (event.key !== 'Enter') return;
  const first = document.querySelector('.row-link');
  if (first) location.hash = first.getAttribute('href');
});

async function initialize() {
  renderThemes();
  route();
  const shown = Date.now();
  try {
    $('#boot').classList.add('busy');
    await refreshEnvironment(); ready = true; await loadGames(); route();
    if (!games.length) scan();
  } catch (error) {
    ready = true;
    $('#library-summary').textContent = 'Open Vessel as a desktop app to load your games.';
    message(String(error), true); render();
  } finally {
    setTimeout(() => { $('#boot').classList.add('done'); setTimeout(() => $('#boot').remove(), 220); }, Math.max(0, 350 - (Date.now() - shown)));
  }
}
initialize();
setInterval(() => { if (ready && !busy) loadGames().catch((error) => message('Could not refresh the library: ' + error, true)); }, 1500);
