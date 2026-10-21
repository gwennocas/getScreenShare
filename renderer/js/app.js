// getScreenShare: entra na sala do LiveKit, publica a tela e mostra as telas dos outros.
// "LivekitClient" vem do livekit-client.umd.js carregado no index.html.
const { Room, RoomEvent, Track } = LivekitClient;

const $ = (id) => document.getElementById(id);
const SVG_NS = 'http://www.w3.org/2000/svg';
const ROOM_RE = /^[\p{L}\p{N}_\- ]{1,64}$/u;
const AVATAR_COLORS = ['#e6c84a', '#4ac2c2', '#5cc060', '#c985e3', '#7da2f0'];

const els = {
  lobby: $('lobby'), roomView: $('room-view'), roomChip:$('room-chip'),
  roomName: $('room-name'), copyRoom:$('btn-copy-room'),
  conn: $('conn'), connText:$('conn-text'),
  form: $('join-form'), name: $('name'), room:$('room'), lobbyError: $('lobby-error'), join:$('btn-join'),
  peopleList: $('people-list'), peopleCount:$('people-count'),
  own: $('own'), ownStats:$('own-stats'),
  grid: $('grid'), empty:$('empty'), emptyTitle: $('empty-title'), emptyText:$('empty-text'),
  share: $('btn-share'), stop: $('btn-stop'), leave:$('btn-leave'),
  audioSink: $('audio-sink'), toasts: $('toasts'), settingsBtn:$('btn-settings'),
  picker: $('picker'), pickerGrid: $('picker-grid'), pickerHint:$('picker-hint'),
  pickerStart: $('picker-start'), pickerCancel: $('picker-cancel'), pickerClose:$('picker-close'),
  pickerRefresh: $('picker-refresh'), tabScreen: $('tab-screen'), tabWindow:$('tab-window'),
  pkQuality: $('pk-quality'), pkHint: $('pk-hint'), pkAudio:$('pk-audio'),
  settings: $('settings'), stMode: $('st-mode'), stModeHelp:$('st-mode-help'),
  stServerFields: $('st-server-fields'), stServerUrl: $('st-server-url'), stAccessCode:$('st-access-code'),
  stCheck: $('st-check'), stCheckResult:$('st-check-result'),
  stCodec: $('st-codec'), stSimulcast:$('st-simulcast'), stStats: $('st-stats'), stLowLatency:$('st-lowlatency'),
  stGpu: $('st-gpu'), appVersion:$('app-version'),
  settingsTabGeneral: $('settings-tab-general'), settingsTabUpdates: $('settings-tab-updates'),
  settingsPaneGeneral: $('settings-pane-general'), settingsPaneUpdates: $('settings-pane-updates'),
  stUpdateVersion: $('st-update-version'), stUpdateCheck: $('st-update-check'),
  stUpdateAction: $('st-update-action'), stUpdateStatus: $('st-update-status'),
  settingsClose: $('settings-close'), settingsDone:$('settings-done'),
};

let room = null;
let leaving = false;
let focusedId = null;
let stopOwnStats = null;
let localShareTrack = null;
let pickedSourceId = null;
let pickerKind = 'screen';
let pickerSources = [];
let excludedAudioContext = null;
let excludedAudioNode = null;
let excludedAudioTrack = null;

const tiles = new Map();       // identity -> { el, video, track, stop, statsEl }
const audioByPeer = new Map(); // identity -> { els: [], volume, muted }

// ---------- Utilidades ----------

function icon(id) {
  const svg = document.createElementNS(SVG_NS, 'svg');
  svg.setAttribute('class', 'i');
  const use = document.createElementNS(SVG_NS, 'use');
  use.setAttribute('href', `#${id}`);
  svg.appendChild(use);
  return svg;
}

function iconButton(iconId, label, className) {
  const b = document.createElement('button');
  b.type = 'button';
  b.className = `tile-btn ${className || ''}`.trim();
  b.setAttribute('aria-label', label);
  b.title = label;
  b.appendChild(icon(iconId));
  return b;
}

function toast(message, { kind = 'info', sticky = false } = {}) {
  const t = document.createElement('div');
  t.className = `toast ${kind}`;
  t.textContent = message;
  els.toasts.appendChild(t);
  const dismiss = () => t.remove();
  if (!sticky) setTimeout(dismiss, kind === 'error' ? 7000 : 4000);
  return { dismiss };
}

function friendlyError(err) {
  const msg = (err && err.message) || String(err);
  return msg.replace(/^Error invoking remote method '[^']+': (Error: )?/, '');
}

function cleanName(v) {
  return String(v || '').replace(/\s+/g, ' ').trim().slice(0, 40);
}

function cleanRoom(v) {
  const r = String(v || '').replace(/\s+/g, ' ').trim();
  return ROOM_RE.test(r) ? r : '';
}

function displayName(p) {
  return p.name || p.identity;
}

function hashIndex(str, mod) {
  let h = 0;
  for (let i = 0; i < str.length; i++) h = (h * 31 + str.charCodeAt(i)) >>> 0;
  return h % mod;
}

function renderKv(dl, rows) {
  const frag = document.createDocumentFragment();
  rows.forEach(([k, v]) => {
    const dt = document.createElement('dt');
    dt.textContent = k;
    const dd = document.createElement('dd');
    dd.textContent = v;
    if (k === 'Codificação' || k === 'Decodificador') {
      dd.dataset.tone = v === 'placa de vídeo' ? 'good' : v === 'processador' ? 'warn' : '';
    }
    frag.append(dt, dd);
  });
  dl.replaceChildren(frag);
}

// Linha curta de estatísticas (em cima da tela): cada item é um pedaço separado.
function renderBrief(container, segments) {
  const frag = document.createDocumentFragment();
  (segments || []).forEach((text) => {
    const s = document.createElement('span');
    s.textContent = text;
    frag.appendChild(s);
  });
  container.replaceChildren(frag);
}

function setView(view) {
  document.body.dataset.view = view;
  els.lobby.hidden = view !== 'lobby';
  els.roomView.hidden = view !== 'room';
  els.roomChip.hidden = view !== 'room';
}

function showLobbyError(text) {
  els.lobbyError.textContent = text || '';
  els.lobbyError.hidden = !text;
}

const QUALITY_LABEL = {
  excellent: 'Conexão ótima',
  good: 'Conexão boa',
  poor: 'Conexão ruim',
  lost: 'Sem conexão',
  unknown: 'Medindo conexão',
};

function setConnQuality(q) {
  const key = QUALITY_LABEL[q] ? q : 'unknown';
  els.conn.dataset.q = key;
  els.connText.textContent = QUALITY_LABEL[key];
}

// ---------- Entrar e sair ----------

async function joinRoom(event) {
  event.preventDefault();
  showLobbyError('');

  const name = cleanName(els.name.value) || 'Convidado';
  const roomName = cleanRoom(els.room.value);
  if (!roomName) {
    showLobbyError('Escreva o nome da sala usando letras, números, espaço, - ou _ (até 64 caracteres).');
    els.room.focus();
    return;
  }

  const s = getSettings();
  els.join.disabled = true;
  els.join.textContent = 'Conectando';

  try {
    const identity = `${name.replace(/[^\p{L}\p{N}_-]/gu, '').slice(0, 24) || 'user'}-${Math.random().toString(36).slice(2, 7)}`;
    const { token, url } = await window.api.getToken({
      mode: s.tokenMode,
      serverUrl: s.serverUrl,
      accessCode: s.accessCode,
      room: roomName,
      identity,
      name,
    });

    const r = new Room({
      adaptiveStream: true, // só baixa a resolução que o quadro realmente mostra
      dynacast: true,       // quem transmite para de enviar camadas que ninguém está vendo
    });
    bindRoomEvents(r);
    await r.connect(url, token);
    try { await r.startAudio(); } catch (_) { /* sem problema */ }

    room = r;
    saveSettings({ name: els.name.value.trim(), room: roomName });
    els.roomName.textContent = roomName;
    setConnQuality('unknown');
    setView('room');
    renderPeople();
    updateGrid();
  } catch (err) {
    console.error(err);
    showLobbyError(friendlyError(err));
  } finally {
    els.join.disabled = false;
    els.join.textContent = 'Entrar na sala';
  }
}

async function leaveRoom() {
  if (!room) return;
  leaving = true;
  await stopExcludedAudioTrack();
  try { await room.disconnect(); } catch (_) { /* ignora */ }
  // o evento Disconnected faz a limpeza
}

function cleanupRoom() {
  void stopExcludedAudioTrack();
  if (stopOwnStats) { stopOwnStats(); stopOwnStats = null; }
  localShareTrack = null;
  [...tiles.keys()].forEach(removeTile);
  audioByPeer.forEach((a) => a.els.forEach((e) => e.remove()));
  audioByPeer.clear();
  focusedId = null;
  room = null;
  els.own.hidden = true;
  els.share.hidden = false;
  els.stop.hidden = true;
  els.peopleList.replaceChildren();
  updateGrid();
}

function bindRoomEvents(r) {
  let reconnectToast = null;
  r.on(RoomEvent.TrackSubscribed, onTrackSubscribed)
    .on(RoomEvent.TrackUnsubscribed, onTrackUnsubscribed)
    .on(RoomEvent.ParticipantConnected, (p) => { toast(`${displayName(p)} entrou`); renderPeople(); })
    .on(RoomEvent.ParticipantDisconnected, (p) => {
      toast(`${displayName(p)} saiu`);
      removeTile(p.identity);
      renderPeople();
    })
    .on(RoomEvent.TrackPublished, renderPeople)
    .on(RoomEvent.TrackUnpublished, renderPeople)
    .on(RoomEvent.LocalTrackPublished, renderPeople)
    .on(RoomEvent.LocalTrackUnpublished, (pub) => {
      if (pub.source === Track.Source.ScreenShare) onShareEnded();
      renderPeople();
    })
    .on(RoomEvent.ConnectionQualityChanged, (q, p) => { if (p && p.isLocal) setConnQuality(q); })
    .on(RoomEvent.Reconnecting, () => {
      reconnectToast = toast('Conexão perdida. Tentando reconectar', { kind: 'warn', sticky: true });
      setConnQuality('lost');
    })
    .on(RoomEvent.Reconnected, () => {
      if (reconnectToast) reconnectToast.dismiss();
      toast('Reconectado', { kind: 'ok' });
      setConnQuality('unknown');
    })
    .on(RoomEvent.Disconnected, () => {
      if (reconnectToast) reconnectToast.dismiss();
      const wasLeaving = leaving;
      leaving = false;
      cleanupRoom();
      setView('lobby');
      if (!wasLeaving) toast('Você foi desconectado da sala', { kind: 'error' });
    });
}

// ---------- Pessoas ----------

function isSharing(p) {
  return !!p.getTrackPublication(Track.Source.ScreenShare);
}

function renderPeople() {
  if (!room) return;
  const remotes = [...room.remoteParticipants.values()]
    .sort((a, b) => displayName(a).localeCompare(displayName(b)));
  const all = [room.localParticipant, ...remotes];
  els.peopleCount.textContent = String(all.length);

  const frag = document.createDocumentFragment();
  all.forEach((p) => {
    const li = document.createElement('li');
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'person';

    const av = document.createElement('span');
    av.className = 'avatar';
    av.style.background = AVATAR_COLORS[hashIndex(p.identity, AVATAR_COLORS.length)];
    av.textContent = (displayName(p).trim()[0] || '?').toUpperCase();

    const nm = document.createElement('span');
    nm.className = 'person-name';
    nm.textContent = displayName(p);

    btn.append(av, nm);
    if (p.isLocal) {
      const me = document.createElement('span');
      me.className = 'person-tag';
      me.textContent = 'você';
      btn.append(me);
    }
    if (isSharing(p)) {
      const live = document.createElement('span');
      live.className = 'live-badge small';
      live.append(document.createElement('i'), document.createTextNode('ao vivo'));
      btn.append(live);
    }

    const canWatch = !p.isLocal && tiles.has(p.identity);
    btn.disabled = !canWatch;
    if (canWatch) {
      btn.title = 'Dar foco nesta tela';
      btn.addEventListener('click', () => toggleFocus(p.identity));
    }
    li.appendChild(btn);
    frag.appendChild(li);
  });
  els.peopleList.replaceChildren(frag);
}

// ---------- Telas recebidas ----------

function onTrackSubscribed(track, publication, participant) {
  if (publication.source === Track.Source.ScreenShare && track.kind === Track.Kind.Video) {
    addTile(participant, track);
  } else if (publication.source === Track.Source.ScreenShareAudio && track.kind === Track.Kind.Audio) {
    addAudio(participant, track);
  }
}

function onTrackUnsubscribed(track, publication, participant) {
  if (publication.source === Track.Source.ScreenShare && track.kind === Track.Kind.Video) {
    removeTile(participant.identity);
  } else if (publication.source === Track.Source.ScreenShareAudio) {
    track.detach().forEach((e) => e.remove());
    audioByPeer.delete(participant.identity);
    const t = tiles.get(participant.identity);
    if (t) t.el.classList.remove('has-audio');
  }
}

function addAudio(participant, track) {
  const id = participant.identity;
  const state = audioByPeer.get(id) || { els: [], volume: 1, muted: false };
  const el = track.attach();
  el.volume = state.volume;
  el.muted = state.muted;
  els.audioSink.appendChild(el);
  state.els.push(el);
  audioByPeer.set(id, state);
  const t = tiles.get(id);
  if (t) t.el.classList.add('has-audio');
}

function addTile(participant, track) {
  const id = participant.identity;
  removeTile(id);
  const s = getSettings();

  const el = document.createElement('div');
  el.className = 'tile';
  el.dataset.id = id;
  el.dataset.stats = s.showStats ? 'on' : 'off';
  el.tabIndex = 0;
  el.setAttribute('role', 'group');
  el.setAttribute('aria-label', `Tela de ${displayName(participant)}`);

  const video = track.attach();
  video.autoplay = true;
  video.playsInline = true;
  video.muted = true; // o áudio vem de um elemento separado

  const top = document.createElement('div');
  top.className = 'tile-top';
  const nm = document.createElement('span');
  nm.className = 'tile-name';
  nm.textContent = displayName(participant);

  const ctl = document.createElement('div');
  ctl.className = 'tile-ctl';

  // Cada clique troca: resumo, detalhes completos, escondido
  const bStats = iconButton('i-info', 'Estatísticas: resumo, detalhes ou escondido', 'btn-stats');
  bStats.addEventListener('click', (e) => {
    e.stopPropagation();
    const next = { on: 'full', full: 'off', off: 'on' };
    el.dataset.stats = next[el.dataset.stats] || 'on';
  });

  const vol = document.createElement('div');
  vol.className = 'vol';
  const bMute = iconButton('i-volume', 'Silenciar ou ativar o som', 'btn-mute');
  const range = document.createElement('input');
  range.type = 'range';
  range.min = '0';
  range.max = '100';
  range.value = '100';
  range.setAttribute('aria-label', 'Volume');
  const applyVolume = () => {
    const a = audioByPeer.get(id);
    if (!a) return;
    a.volume = range.value / 100;
    a.els.forEach((e) => { e.volume = a.volume; e.muted = a.muted; });
    bMute.replaceChildren(icon(a.muted || a.volume === 0 ? 'i-volume-off' : 'i-volume'));
  };
  bMute.addEventListener('click', (e) => {
    e.stopPropagation();
    const a = audioByPeer.get(id);
    if (!a) return;
    a.muted = !a.muted;
    applyVolume();
  });
  range.addEventListener('input', () => {
    const a = audioByPeer.get(id);
    if (a && a.muted && range.value > 0) a.muted = false;
    applyVolume();
  });
  range.addEventListener('click', (e) => e.stopPropagation());
  vol.append(bMute, range);

  const bFocus = iconButton('i-focus', 'Dar foco nesta tela', 'btn-focus');
  bFocus.addEventListener('click', (e) => { e.stopPropagation(); toggleFocus(id); });

  const bFull = iconButton('i-expand', 'Tela cheia', 'btn-full');
  bFull.addEventListener('click', (e) => { e.stopPropagation(); toggleFullscreen(el); });

  ctl.append(bStats, vol, bFocus, bFull);
  top.append(nm, ctl);

  const stats = document.createElement('div');
  stats.className = 'stats';
  const statsLine = document.createElement('div');
  statsLine.className = 'stats-line';
  const statsList = document.createElement('dl');
  statsList.className = 'kv';
  stats.append(statsLine, statsList);

  el.append(video, top, stats);
  el.addEventListener('click', () => toggleFocus(id));
  el.addEventListener('dblclick', () => toggleFullscreen(el));
  el.addEventListener('keydown', (e) => {
    if (e.target === el && (e.key === 'Enter' || e.key === ' ')) {
      e.preventDefault();
      toggleFocus(id);
    }
  });

  els.grid.appendChild(el);

  // Segura o mínimo possível de vídeo na memória (menos atraso).
  if (s.lowLatency && track.receiver) {
    try { track.receiver.jitterBufferTarget = 0; } catch (_) { /* navegador sem suporte */ }
    try { track.receiver.playoutDelayHint = 0; } catch (_) { /* navegador sem suporte */ }
  }

  const stop = pollReceiverStats(track, (rows, brief) => {
    renderKv(statsList, rows);
    renderBrief(statsLine, brief);
  });
  tiles.set(id, { el, video, track, stop });
  if (audioByPeer.has(id)) el.classList.add('has-audio');

  updateGrid();
  renderPeople();
}

function removeTile(id) {
  const t = tiles.get(id);
  if (!t) return;
  t.stop();
  t.track.detach().forEach((e) => { if (e !== t.video) e.remove(); });
  if (document.fullscreenElement === t.el) document.exitFullscreen().catch(() => {});
  t.el.remove();
  tiles.delete(id);
  if (focusedId === id) focusedId = null;
  updateGrid();
  renderPeople();
}

function toggleFocus(id) {
  if (!tiles.has(id) || tiles.size < 2) return;
  focusedId = focusedId === id ? null : id;
  updateGrid();
}

function toggleFullscreen(el) {
  if (document.fullscreenElement === el) document.exitFullscreen().catch(() => {});
  else el.requestFullscreen().catch(() => {});
}

function updateGrid() {
  const count = tiles.size;
  els.grid.dataset.count = String(count);
  els.grid.classList.toggle('has-focus', !!focusedId && count > 1);
  tiles.forEach((t, id) => t.el.classList.toggle('focused', id === focusedId && count > 1));
  els.grid.hidden = count === 0;
  els.empty.hidden = count > 0;

  const sharing = !!localShareTrack;
  els.emptyTitle.textContent = sharing ? 'Você está compartilhando' : 'Ninguém está compartilhando';
  els.emptyText.textContent = sharing
    ? 'Sua própria tela não aparece aqui, para não virar um espelho infinito. Quem está na sala já consegue ver.'
    : 'Compartilhe sua tela ou espere alguém começar. As telas aparecem aqui.';
}

// ---------- Escolher e compartilhar a própria tela ----------

async function openPicker() {
  if (!room) return;
  const s = getSettings();
  els.pkQuality.value = s.quality;
  els.pkHint.value = s.hint;
  els.pkAudio.checked = s.shareAudio;
  els.picker.showModal();
  await loadSources();
}

async function loadSources() {
  els.pickerGrid.replaceChildren(Object.assign(document.createElement('p'), {
    className: 'hint', textContent: 'Procurando telas e janelas',
  }));
  try {
    pickerSources = await window.api.listSources();
  } catch (err) {
    pickerSources = [];
    els.pickerGrid.replaceChildren(Object.assign(document.createElement('p'), {
      className: 'form-error', textContent: `Não foi possível listar as telas. ${friendlyError(err)}`,
    }));
    return;
  }
  if (!pickerSources.some((x) => x.id === pickedSourceId)) pickedSourceId = null;
  if (!pickerSources.some((x) => x.kind === pickerKind)) {
    pickerKind = pickerSources.some((x) => x.kind === 'screen') ? 'screen' : 'window';
  }
  renderSources();
}

function renderSources() {
  els.tabScreen.setAttribute('aria-selected', String(pickerKind === 'screen'));
  els.tabWindow.setAttribute('aria-selected', String(pickerKind === 'window'));

  const list = pickerSources.filter((x) => x.kind === pickerKind);
  if (!list.length) {
    els.pickerGrid.replaceChildren(Object.assign(document.createElement('p'), {
      className: 'hint',
      textContent: pickerKind === 'screen' ? 'Nenhuma tela encontrada.' : 'Nenhuma janela aberta encontrada.',
    }));
  } else {
    const frag = document.createDocumentFragment();
    list.forEach((src) => {
      const b = document.createElement('button');
      b.type = 'button';
      b.className = 'source';
      b.setAttribute('role', 'option');
      b.setAttribute('aria-selected', String(src.id === pickedSourceId));
      const img = document.createElement('img');
      img.src = src.thumbnail;
      img.alt = '';
      const label = document.createElement('span');
      label.className = 'source-name';
      label.append(icon(src.kind === 'screen' ? 'i-screen' : 'i-window'));
      const t = document.createElement('span');
      t.textContent = src.name;
      label.append(t);
      b.append(img, label);
      b.addEventListener('click', () => {
        pickedSourceId = src.id;
        renderSources();
      });
      frag.appendChild(b);
    });
    els.pickerGrid.replaceChildren(frag);
  }

  const picked = pickerSources.find((x) => x.id === pickedSourceId);
  els.pickerStart.disabled = !picked;
  els.pickerHint.textContent = picked ? `Escolhido: ${picked.name}` : 'Escolha uma tela ou janela para começar.';
}


// Recebe PCM float32 estéreo (48 kHz) do WASAPI, monta um MediaStreamTrack e
// publica-o como ScreenShareAudio no LiveKit. A captura já vem com o Discord excluído.
async function startExcludedAudioTrack() {
  if (!room) throw new Error('Você não está conectado à sala.');
  await stopExcludedAudioTrack();

  const context = new AudioContext({ sampleRate: 48000, latencyHint: 'interactive' });
  let node = null;
  let track = null;
  try {
    await context.audioWorklet.addModule('./js/pcm-audio-worklet.js');
    node = new AudioWorkletNode(context, 'pcm-stream-processor', {
      numberOfInputs: 0,
      numberOfOutputs: 1,
      outputChannelCount: [2],
    });
    const destination = context.createMediaStreamDestination();
    node.connect(destination);

    window.api.onExcludedAudioChunk((data) => {
      if (!node) return;
      let buffer;
      if (data instanceof ArrayBuffer) {
        buffer = data;
      } else if (ArrayBuffer.isView(data)) {
        buffer = data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength);
      } else {
        return;
      }
      try { node.port.postMessage(buffer, [buffer]); } catch (_) { /* descarta bloco inválido/atrasado */ }
    });

    await context.resume();
    await window.api.startExcludedAudio();
    track = destination.stream.getAudioTracks()[0];
    if (!track) throw new Error('Não foi possível criar a faixa de áudio filtrado.');
    await room.localParticipant.publishTrack(track, { source: Track.Source.ScreenShareAudio });

    excludedAudioContext = context;
    excludedAudioNode = node;
    excludedAudioTrack = track;
  } catch (err) {
    window.api.removeExcludedAudioChunkListener();
    try { await window.api.stopExcludedAudio(); } catch (_) { /* captura pode não ter iniciado */ }
    if (track) { try { track.stop(); } catch (_) { /* ignora */ } }
    if (context.state !== 'closed') { try { await context.close(); } catch (_) { /* ignora */ } }
    throw err;
  }
}

async function stopExcludedAudioTrack() {
  // Zera o estado antes do primeiro await para tornar chamadas simultâneas idempotentes.
  const track = excludedAudioTrack;
  const context = excludedAudioContext;
  excludedAudioTrack = null;
  excludedAudioContext = null;
  excludedAudioNode = null;

  if (window.api && window.api.removeExcludedAudioChunkListener) {
    window.api.removeExcludedAudioChunkListener();
  }
  if (window.api && window.api.stopExcludedAudio) {
    try { await window.api.stopExcludedAudio(); } catch (_) { /* pode já estar parado */ }
  }
  if (track) {
    if (room) {
      try { await room.localParticipant.unpublishTrack(track, false); } catch (_) { /* pode já ter sido removida */ }
    }
    try { track.stop(); } catch (_) { /* ignora */ }
  }
  if (context && context.state !== 'closed') {
    try { await context.close(); } catch (_) { /* ignora */ }
  }
}

async function startShare() {
  if (!room || !pickedSourceId) return;
  const s = getSettings();
  const qualityKey = els.pkQuality.value;
  const hint = els.pkHint.value;
  const withAudio = els.pkAudio.checked;
  const q = QUALITY[qualityKey];

  els.pickerStart.disabled = true;
  try {
    await window.api.selectSource(pickedSourceId, false);

    // Configuração de compartilhamento de tela com suporte otimizado para GPU/Hardware
    const pub = await room.localParticipant.setScreenShareEnabled(
      true,
      {
        audio: false, // o áudio é publicado separadamente, após excluir o processo do Discord
        contentHint: hint, // 'motion' = fluidez, 'detail' = nitidez
        resolution: { width: q.width, height: q.height, frameRate: q.fps },
      },
      {
        videoCodec: s.codec,
        simulcast: s.simulcast,
        screenShareEncoding: { maxBitrate: q.bitrate, maxFramerate: q.fps },
        // Se a rede piorar, baixa a resolução em vez de travar os frames
        degradationPreference: 'maintain-framerate',
      }
    );
    let audioError = '';
    if (withAudio) {
      try {
        await startExcludedAudioTrack();
      } catch (err) {
        console.error('Áudio filtrado do Discord desativado:', err);
        audioError = friendlyError(err);
      }
    }
    saveSettings({ quality: qualityKey, hint, shareAudio: withAudio });
    els.picker.close();

    if (pub && pub.track) {
      localShareTrack = pub.track;
      if (stopOwnStats) stopOwnStats();
      stopOwnStats = pollSenderStats(pub.track, (rows) => renderKv(els.ownStats, rows));
    }
    els.own.hidden = false;
    els.ownStats.hidden = !getSettings().showStats;
    els.share.hidden = true;
    els.stop.hidden = false;
    updateGrid();
    renderPeople();
    toast(audioError
      ? `Tela compartilhada; áudio desativado para evitar loopback. ${audioError}`
      : 'Você está compartilhando a tela', { kind: audioError ? 'warn' : 'ok' });
  } catch (err) {
    console.error(err);
    toast(`Não foi possível compartilhar. ${friendlyError(err)}`, { kind: 'error' });
    els.pickerStart.disabled = false;
  }
}

async function stopShare() {
  if (room) {
    try { await room.localParticipant.setScreenShareEnabled(false); } catch (_) { /* ignora */ }
  }
  onShareEnded();
}

function onShareEnded() {
  void stopExcludedAudioTrack();
  if (stopOwnStats) { stopOwnStats(); stopOwnStats = null; }
  localShareTrack = null;
  els.own.hidden = true;
  els.share.hidden = false;
  els.stop.hidden = true;
  updateGrid();
}

// ---------- Atualizações ----------

let updaterUiState = { status: 'idle' };

function renderUpdaterStatus(state) {
  if (!state || typeof state.status !== 'string') return;
  updaterUiState = state;

  if (els.stUpdateVersion) {
    els.stUpdateVersion.textContent = 'Versão instalada: ' + (state.currentVersion || 'desconhecida');
  }

  const messages = {
    idle: 'O aplicativo procura atualizações ao iniciar. Você também pode verificar manualmente.',
    checking: 'Procurando atualizações…',
    available: 'A versão ' + (state.version || 'nova') + ' está disponível. Clique em “Baixar atualização” quando quiser.',
    downloading: 'Baixando atualização… ' + (state.percent || 0) + '%',
    downloaded: 'A versão ' + (state.version || 'nova') + ' foi baixada. Clique para reiniciar e instalar.',
    'not-available': 'Você já está na versão mais recente (' + (state.currentVersion || 'instalada') + ').',
    dev: 'O atualizador só funciona na versão instalada; npm start não pode instalar atualizações.',
    unsupported: 'O atualizador automático deste aplicativo está disponível no Windows.',
    error: 'Não foi possível atualizar: ' + (state.error || 'erro desconhecido'),
  };
  els.stUpdateStatus.textContent = messages[state.status] || 'Estado do atualizador desconhecido.';
  const busy = state.status === 'checking' || state.status === 'downloading';
  els.stUpdateCheck.disabled = busy || state.status === 'downloaded';
  const canAct = state.status === 'available' || state.status === 'downloaded';
  els.stUpdateAction.hidden = !canAct;
  els.stUpdateAction.disabled = busy;
  els.stUpdateAction.textContent = state.status === 'downloaded'
    ? 'Reiniciar e instalar'
    : 'Baixar atualização';
}

// ---------- Configurações ----------

function fillSettings() {
  const s = getSettings();
  els.stMode.value = s.tokenMode;
  els.stServerUrl.value = s.serverUrl;
  els.stAccessCode.value = s.accessCode;
  els.stCodec.value = s.codec;
  els.stSimulcast.checked = s.simulcast;
  els.stStats.checked = s.showStats;
  els.stLowLatency.checked = s.lowLatency;
  els.stCheckResult.textContent = '';
  refreshModeUi();
}

function refreshModeUi() {
  const server = els.stMode.value === 'server';
  els.stServerFields.hidden = !server;
  els.stModeHelp.textContent = server
    ? 'Um servidor seu guarda a chave e entrega o token. É o jeito certo de usar com amigos.'
    : 'Usa as chaves do arquivo .env deste computador. Serve para testar sozinha.';
}

function openSettings() {
  fillSettings();
  setSettingsTab('general');
  els.settings.showModal();
}

function setSettingsTab(tab) {
  const updates = tab === 'updates';
  els.settingsTabGeneral.setAttribute('aria-selected', String(!updates));
  els.settingsTabUpdates.setAttribute('aria-selected', String(updates));
  els.settingsPaneGeneral.hidden = updates;
  els.settingsPaneUpdates.hidden = !updates;
}

async function checkServer() {
  els.stCheckResult.className = 'hint';
  els.stCheckResult.textContent = 'Testando';
  try {
    await window.api.checkServer(els.stServerUrl.value);
    els.stCheckResult.className = 'hint ok';
    els.stCheckResult.textContent = 'O servidor respondeu.';
  } catch (err) {
    els.stCheckResult.className = 'hint error';
    els.stCheckResult.textContent = friendlyError(err);
  }
}

function bindSettings() {
  els.settingsTabGeneral.addEventListener('click', () => setSettingsTab('general'));
  els.settingsTabUpdates.addEventListener('click', () => setSettingsTab('updates'));
  els.stMode.addEventListener('change', () => { saveSettings({ tokenMode: els.stMode.value }); refreshModeUi(); });
  els.stServerUrl.addEventListener('input', () => saveSettings({ serverUrl: els.stServerUrl.value.trim() }));
  els.stAccessCode.addEventListener('input', () => saveSettings({ accessCode: els.stAccessCode.value }));
  els.stCodec.addEventListener('change', () => saveSettings({ codec: els.stCodec.value }));
  els.stSimulcast.addEventListener('change', () => saveSettings({ simulcast: els.stSimulcast.checked }));
  els.stLowLatency.addEventListener('change', () => saveSettings({ lowLatency: els.stLowLatency.checked }));
  els.stStats.addEventListener('change', () => {
    saveSettings({ showStats: els.stStats.checked });
    tiles.forEach((t) => { t.el.dataset.stats = els.stStats.checked ? 'on' : 'off'; });
    els.ownStats.hidden = !els.stStats.checked;
  });
  els.stCheck.addEventListener('click', checkServer);
  els.stGpu.addEventListener('click', () => window.api.openGpuDiagnostics());

  els.stUpdateCheck.addEventListener('click', async () => {
    try {
      await window.api.checkForUpdates();
    } catch (err) {
      renderUpdaterStatus({ status: 'error', currentVersion: updaterUiState.currentVersion, error: friendlyError(err) });
    }
  });
  els.stUpdateAction.addEventListener('click', async () => {
    try {
      if (updaterUiState.status === 'available') await window.api.downloadUpdate();
      else if (updaterUiState.status === 'downloaded') await window.api.installUpdate();
    } catch (err) {
      renderUpdaterStatus({ status: 'error', currentVersion: updaterUiState.currentVersion, error: friendlyError(err) });
    }
  });
  window.api.onUpdaterStatus(renderUpdaterStatus);
  window.api.getUpdaterStatus().then(renderUpdaterStatus).catch(() => {});
  els.settingsBtn.addEventListener('click', openSettings);
  els.settingsClose.addEventListener('click', () => els.settings.close());
  els.settingsDone.addEventListener('click', () => els.settings.close());
}

// ---------- Início ----------

function closeOnBackdrop(dialog) {
  dialog.addEventListener('click', (e) => { if (e.target === dialog) dialog.close(); });
}

function init() {
  const s = loadSettings();
  els.name.value = s.name;
  els.room.value = s.room;

  els.form.addEventListener('submit', joinRoom);
  els.leave.addEventListener('click', leaveRoom);
  els.share.addEventListener('click', openPicker);
  els.stop.addEventListener('click', stopShare);

  els.copyRoom.addEventListener('click', async () => {
    try {
      await navigator.clipboard.writeText(els.roomName.textContent);
      toast('Nome da sala copiado', { kind: 'ok' });
    } catch (_) {
      toast('Não foi possível copiar o nome da sala', { kind: 'error' });
    }
  });

  els.pickerStart.addEventListener('click', startShare);
  els.pickerCancel.addEventListener('click', () => els.picker.close());
  els.pickerClose.addEventListener('click', () => els.picker.close());
  els.pickerRefresh.addEventListener('click', loadSources);
  [els.tabScreen, els.tabWindow].forEach((tab) => tab.addEventListener('click', () => {
    pickerKind = tab.dataset.kind;
    renderSources();
  }));
  closeOnBackdrop(els.picker);
  closeOnBackdrop(els.settings);

  bindSettings();
  setView('lobby');
  updateGrid();

  if (window.api && window.api.getAppInfo) {
    window.api.getAppInfo().then((info) => { els.appVersion.textContent = `Versão ${info.version}`; if (els.stUpdateVersion) els.stUpdateVersion.textContent = `Versão instalada: ${info.version}`; }).catch(() => {});
  }
}

init();
