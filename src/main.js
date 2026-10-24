// SPDX-License-Identifier: MIT
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import './styles.css';

const $ = (selector) => document.querySelector(selector);
let games = [], runtimes = [], selectedId = null, filter = 'all', dirty = false;
let selectedFormId = null, busy = false, refreshing = false, ready = false;
let detailTab = 'game', editingToolId = null, toolGameId = null, removingToolId = null, logTarget = null;

function message(text, error = false) {
  $('#message').textContent = text;
  $('#message').hidden = !text;
  $('#message').dataset.error = String(error);
}
function current() { return games.find((g) => g.id === selectedId); }
function running(game) { return game.launch?.state === 'running'; }
function gameSessionRunning(game) { return running(game) || (game.tools || []).some((tool) => tool.kind === 'script_extender' && running(tool)); }
function environmentBusy(game) { return running(game) || (game.tools || []).some(running); }
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
function selectGame(id, openDetail = true) {
  if (dirty && selectedId !== id) {
    message('Save your launch settings before choosing another game.', true);
    return;
  }
  selectedId = id;
  if (openDetail) document.body.dataset.detailOpen = 'true';
  render();
}
function renderList() {
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
  for (const game of shown) {
    const art = artwork(game);
    const row = element('article', undefined, 'game-card');
    row.dataset.selected = String(game.id === selectedId);
    row.dataset.featured = String(art?.kind === 'landscape');
    const select = element('button', undefined, 'game-select');
    select.setAttribute('aria-label', 'Select ' + game.name);
    select.setAttribute('aria-pressed', String(game.id === selectedId));
    const cover = element('div', undefined, 'game-art');
    const fallback = element('span', game.name, 'cover-fallback');
    if (art) {
      const image = element('img');
      image.src = art.src; image.alt = game.name + (art.kind === 'screenshot' ? ' gameplay screenshot' : ' game artwork');
      image.className = art.kind; image.referrerPolicy = 'no-referrer';
      image.addEventListener('error', () => { image.hidden = true; fallback.hidden = false; });
      fallback.hidden = true; cover.append(image);
    }
    cover.append(fallback);
    const info = element('div', undefined, 'game-info');
    info.append(element('strong', game.name), element('span', gameStatus(game), environmentBusy(game) ? 'status running' : 'status'));
    select.append(cover, info);
    select.addEventListener('click', () => selectGame(game.id));
    const play = element('button', gameSessionRunning(game) ? 'Running' : 'Play', 'row-play');
    play.setAttribute('aria-label', 'Play ' + game.name);
    play.disabled = busy || gameSessionRunning(game) || !game.available || !game.runtime_path || dirty;
    play.addEventListener('click', () => playGame(game.id));
    const footer = element('div', undefined, 'game-card-footer');
    const source = element('span', 'Windows · ' + (game.runtime_kind === 'proton' ? 'Proton' : 'Wine'), 'card-platform');
    if (art) source.title = 'Artwork: ' + art.credit;
    footer.append(source, play);
    row.append(select, footer); list.append(row);
  }
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
function populateSettings(game) {
  selectedFormId = game.id;
  $('#game-name').value = game.name;
  const select = $('#runtime-choice'); select.replaceChildren();
  select.append(new Option('Choose a runtime', ''));
  for (const runtime of runtimes.filter((r) => r.kind !== 'umu')) {
    const option = new Option(runtime.name + ' · ' + runtime.kind, runtime.kind + '|' + runtime.path);
    select.append(option);
  }
  select.append(new Option('Custom runtime…', 'custom'));
  const value = game.runtime_kind + '|' + game.runtime_path;
  select.value = [...select.options].some((o) => o.value === value) ? value : (game.runtime_path ? 'custom' : '');
  $('#custom-runtime-kind').value = game.runtime_kind;
  $('#custom-runtime-path').value = game.runtime_path;
  $('#custom-runtime').hidden = select.value !== 'custom';
  $('#argument-lines').value = game.arguments.join('\n');
}
function renderDetail() {
  const game = current();
  $('#detail-empty').hidden = !!game;
  $('#detail-content').hidden = !game;
  $('#play').disabled = !game || busy || dirty || gameSessionRunning(game) || !game.available || !game.runtime_path;
  if (!game) return;
  $('#detail-game').setAttribute('aria-pressed', String(detailTab === 'game'));
  $('#detail-tools').setAttribute('aria-pressed', String(detailTab === 'tools'));
  $('#game-launch-panel').hidden = detailTab !== 'game';
  $('#tools-panel').hidden = detailTab !== 'tools';
  $('#play').hidden = detailTab === 'tools';
  $('#detail-title').textContent = game.name;
  $('#game-state').textContent = gameStatus(game);
  $('#game-state').classList.toggle('running', running(game));
  $('#play').textContent = gameSessionRunning(game) ? 'Running' : 'Play';
  $('#executable-path').textContent = game.executable;
  $('#prefix-path').textContent = game.prefix;
  $('#last-played').textContent = game.last_played_at ? game.last_played_at + ' UTC' : 'Not played yet';
  $('#view-log').disabled = !game.launch;
  if (selectedFormId !== game.id || !dirty) populateSettings(game);
  $('#save-settings').disabled = !dirty || busy || environmentBusy(game);
  $('#discard-settings').hidden = !dirty;
  $('#settings-note').textContent = environmentBusy(game) ? 'Close the game and its companion apps before changing launch settings.' :
    dirty ? 'Save these changes before launching or choosing another game.' : 'Arguments are passed directly to the game.';
  for (const input of $('#settings-form').querySelectorAll('input, select, textarea')) input.disabled = environmentBusy(game);
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
function render() { renderList(); renderDetail(); }
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
    if (!games.some((g) => g.id === selectedId)) selectedId = games[0]?.id ?? null;
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
  if (dirty) { message('Save your launch settings before playing.', true); return; }
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
    selectedId = game.id; selectedFormId = null;
    await loadGames(); message(game.name + ' added to your library.');
  } finally { setBusy(false); }
}
async function refreshEnvironment() {
  const environment = await invoke('get_environment_status');
  runtimes = environment.runtimes;
  $('#environment-state').textContent = 'Local library connected';
  renderRuntimes(); renderDetail();
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
  toolGameId = selectedId; editingToolId = tool?.id ?? null;
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
  if (dirty) { message('Save your launch settings before opening a manager.', true); return; }
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
  renderList(); renderDetail();
}

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
$('#discard-settings').addEventListener('click', () => { dirty = false; render(); message('Unsaved changes discarded.'); });
$('#runtime-choice').addEventListener('change', markDirty);
$('#settings-form').addEventListener('submit', async (event) => {
  event.preventDefault();
  const choice = $('#runtime-choice').value;
  if (!choice) { message('Choose an installed runtime or a custom runner.', true); return; }
  const separator = choice.indexOf('|');
  const kind = choice === 'custom' ? $('#custom-runtime-kind').value : choice.slice(0, separator);
  const path = choice === 'custom' ? $('#custom-runtime-path').value.trim() : choice.slice(separator + 1);
  setBusy(true);
  try {
    await invoke('save_game', { settings: { id: selectedId, name: $('#game-name').value, runtime_kind: kind, runtime_path: path,
      arguments: $('#argument-lines').value.split('\n').filter((line) => line.length > 0) } });
    dirty = false; await loadGames(); message('Launch settings saved.');
  } catch (error) { message(String(error), true); }
  finally { setBusy(false); }
});
$('#browse-runtime').addEventListener('click', async () => {
  try { const path = await open({ multiple: false, title: 'Choose the proton script or Wine executable' }); if (path) { $('#custom-runtime-path').value = path; markDirty(); } }
  catch (error) { message(String(error), true); }
});
$('#play').addEventListener('click', () => { if (current()) playGame(selectedId); });
$('#search').addEventListener('input', renderList);
for (const mode of ['all', 'running']) $('#filter-' + mode).addEventListener('click', () => { filter = mode; renderList(); });
$('#refresh').addEventListener('click', () => refreshEnvironment().catch((error) => message(String(error), true)));
$('#view-log').addEventListener('click', () => showLog({ type: 'game', id: selectedId }));
$('#refresh-log').addEventListener('click', () => showLog());
$('#detail-game').addEventListener('click', () => { detailTab = 'game'; renderDetail(); });
$('#detail-tools').addEventListener('click', () => { detailTab = 'tools'; renderDetail(); });
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
  try { await invoke('remove_tool', { toolId: removingToolId }); $('#remove-tool-dialog').close(); await loadGames(); message('Mod manager profile removed. Files were kept on disk.'); }
  catch (error) { $('#remove-tool-error').textContent = String(error); $('#remove-tool-error').hidden = false; }
});
$('#back-library').addEventListener('click', () => { document.body.dataset.detailOpen = 'false'; });
for (const button of document.querySelectorAll('[data-close]')) button.addEventListener('click', () => $('#' + button.dataset.close).close());

async function initialize() {
  try {
    await refreshEnvironment(); ready = true; await loadGames(); setBusy(false);
    if (!games.length) await scan();
  } catch (error) {
    $('#environment-state').textContent = 'Desktop backend unavailable';
    $('#library-summary').textContent = 'Open Respawn+ as a desktop app to load your games.';
    $('#game-list').replaceChildren(element('p', 'The local game library requires the Respawn+ desktop backend.', 'empty'));
    message(String(error), true); setBusy(false);
  }
}
initialize();
setInterval(() => { if (ready && !busy) loadGames().catch((error) => message('Could not refresh library: ' + error, true)); }, 2500);

