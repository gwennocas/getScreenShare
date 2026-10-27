const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');

class Capture extends EventTarget {
  constructor(label) { super(); this.label = label; this.readyState = 'live'; this.stops = 0; }
  stop() { this.readyState = 'ended'; this.stops++; }
}

function deferred() {
  let resolve;
  const promise = new Promise((done) => { resolve = done; });
  return { promise, resolve };
}

function harness({ captureError, replaceError, capturePromise, replacePromise, commitError } = {}) {
  const elements = new Map();
  const element = (id) => {
    if (!elements.has(id)) elements.set(id, {
      hidden: false, disabled: false, open: false, value: '', textContent: '',
      showModal() { this.open = true; }, close() { this.open = false; },
      closest() { return element(`${id}-label`); },
      replaceChildren() {}, addEventListener() {},
    });
    return elements.get(id);
  };
  const old = new Capture('Monitor 1');
  const next = new Capture('Monitor 2');
  const calls = { select: 0, capture: 0, replace: 0, publish: 0, unpublish: 0, disable: 0, audioStop: 0, statsStop: 0 };
  const stream = { getVideoTracks: () => [next], getTracks: () => [next] };
  const local = {
    mediaStreamTrack: old,
    stop() { this.mediaStreamTrack.stop(); },
    async replaceTrack(replacement, userProvided) {
      calls.replace++;
      assert.equal(userProvided, false, 'SDK deve gerenciar a captura substituta');
      if (replacePromise) await replacePromise;
      if (replaceError) throw replaceError;
      this.mediaStreamTrack = replacement;
      old.stop();
      if (commitError) throw commitError;
    },
  };
  const participant = {
    async setScreenShareEnabled(enabled) {
      if (enabled) { calls.publish++; return { track: local }; }
      else { calls.disable++; local.mediaStreamTrack.stop(); }
    },
    async unpublishTrack() { calls.unpublish++; },
  };
  const messages = [];
  const audio = { id: 'audio-existente' };
  const context = vm.createContext({
    LivekitClient: { Track: { Source: { ScreenShare: 'screen_share' } } },
    document: { getElementById: element, querySelector: element },
    navigator: { mediaDevices: { async getDisplayMedia(options) {
      calls.capture++;
      assert.equal(options.audio, false, 'áudio existente não deve ser recapturado');
      assert.equal(options.video.frameRate.max, 60);
      if (captureError) throw captureError;
      return capturePromise ? await capturePromise : stream;
    } } },
    window: { api: { async selectSource() { calls.select++; return true; } } },
    console: { error() {} }, local, participant, calls, messages, audio,
  });
  const run = (script) => vm.runInContext(script, context);
  run(fs.readFileSync(path.join(__dirname, '../renderer/js/settings.js'), 'utf8'));
  run(fs.readFileSync(path.join(__dirname, '../renderer/js/app.js'), 'utf8').replace(/\ninit\(\);\s*$/, ''));
  run(`
    room = { localParticipant: participant };
    localShareTrack = local;
    shareOptions = { quality: '1080p60', hint: 'detail' };
    activeSourceId = 'screen:1';
    pickedSourceId = 'screen:2';
    pickerMode = 'switch';
    pickerSources = [{ id: 'screen:2', name: 'Monitor 2', kind: 'screen' }];
    els.picker.open = true;
    excludedAudioTrack = audio;
    stopOwnStats = () => { calls.statsStop++; };
    stopExcludedAudioTrack = async () => { calls.audioStop++; excludedAudioTrack = null; };
    toast = (message) => messages.push(message);
    renderSources = () => { els.pickerStart.disabled = shareBusy; };
    updateGrid = () => {};
    renderPeople = () => {};
    pollSenderStats = () => () => {};
  `);
  return { run, context, calls, local, old, next, stream, element, messages, audio };
}

test('troca a captura na mesma publicação e mantém áudio, estatísticas e qualidade', async () => {
  const h = harness();
  await h.run('startShare()');
  assert.equal(h.calls.replace, 1);
  assert.equal(h.calls.publish, 0);
  assert.equal(h.calls.disable, 0);
  assert.equal(h.calls.audioStop, 0);
  assert.equal(h.calls.statsStop, 0);
  assert.equal(h.run('excludedAudioTrack'), h.audio);
  assert.equal(h.local.mediaStreamTrack, h.next);
  assert.equal(h.old.readyState, 'ended');
  assert.equal(h.next.readyState, 'live');
  assert.equal(h.next.contentHint, 'detail');
  assert.equal(h.element('picker').open, false);
  assert.equal(h.run('activeSourceId'), 'screen:2');
  assert.equal(h.run('shareOptions.quality'), '1080p60');
});

for (const name of ['NotAllowedError', 'AbortError']) {
  test(`cancelamento da captura (${name}) mantém a tela e o áudio atuais`, async () => {
    const h = harness({ captureError: Object.assign(new Error('cancelado'), { name }) });
    await h.run('startShare()');
    assert.equal(h.local.mediaStreamTrack, h.old);
    assert.equal(h.old.readyState, 'live');
    assert.equal(h.calls.replace, 0);
    assert.equal(h.calls.audioStop, 0);
    assert.equal(h.messages.length, 0);
    assert.equal(h.element('picker-start').disabled, false);
    assert.equal(h.element('btn-switch').disabled, false);
  });
}

test('falha do sender preserva a captura anterior e libera a candidata', async () => {
  const h = harness({ replaceError: new Error('sender rejeitou a nova captura') });
  await h.run('startShare()');
  assert.equal(h.old.readyState, 'live');
  assert.equal(h.next.readyState, 'ended');
  assert.equal(h.calls.audioStop, 0);
  assert.match(h.messages[0], /transmissão atual continua/);
  assert.equal(h.run('activeSourceId'), 'screen:1');
});

test('erro de parâmetros após a troca não encerra a nova captura já adotada', async () => {
  const h = harness({ commitError: new Error('setParameters falhou') });
  await h.run('startShare()');
  assert.equal(h.local.mediaStreamTrack, h.next);
  assert.equal(h.next.readyState, 'live');
  assert.equal(h.old.readyState, 'ended');
  assert.equal(h.calls.audioStop, 0);
  assert.match(h.messages[0], /não foi possível ajustar o encoder/);
});

test('cancelar o seletor enquanto aguarda a captura descarta a nova fonte', async () => {
  const pending = deferred();
  const h = harness({ capturePromise: pending.promise });
  const operation = h.run('startShare()');
  await new Promise(setImmediate);
  h.element('picker').close();
  pending.resolve(h.stream);
  await operation;
  assert.equal(h.calls.replace, 0);
  assert.equal(h.old.readyState, 'live');
  assert.equal(h.next.readyState, 'ended');
});

test('parar durante a seleção impede uma troca atrasada de reativar o vídeo', async () => {
  const pending = deferred();
  const h = harness({ capturePromise: pending.promise });
  const operation = h.run('startShare()');
  await new Promise(setImmediate);
  await h.run('stopShare()');
  pending.resolve(h.stream);
  await operation;
  assert.equal(h.calls.replace, 0);
  assert.equal(h.run('localShareTrack'), null);
  assert.equal(h.next.readyState, 'ended');
  assert.equal(h.element('btn-switch').hidden, true);
});

test('sair da sala enquanto aguarda captura libera a nova fonte sem publicá-la', async () => {
  const pending = deferred();
  const h = harness({ capturePromise: pending.promise });
  const operation = h.run('startShare()');
  await new Promise(setImmediate);
  h.run('room = null; localShareTrack = null; shareGeneration++');
  pending.resolve(h.stream);
  await operation;
  assert.equal(h.calls.replace, 0);
  assert.equal(h.next.readyState, 'ended');
  assert.equal(h.messages.length, 0);
});

test('parar durante replaceTrack também libera a captura que terminou de trocar', async () => {
  const pending = deferred();
  const h = harness({ replacePromise: pending.promise });
  const operation = h.run('startShare()');
  await new Promise(setImmediate);
  assert.equal(h.calls.replace, 1);
  await h.run('stopShare()');
  pending.resolve();
  await operation;
  assert.equal(h.next.readyState, 'ended');
  assert.equal(h.run('localShareTrack'), null);
  assert.equal(h.messages.length, 0);
});

test('cliques repetidos iniciam somente uma captura', async () => {
  const pending = deferred();
  const h = harness({ capturePromise: pending.promise });
  const operation = h.run('startShare()');
  await h.run('startShare()');
  await new Promise(setImmediate);
  assert.equal(h.calls.capture, 1);
  pending.resolve(h.stream);
  await operation;
  assert.equal(h.calls.replace, 1);
});

test('a fonte atual não é recapturada', async () => {
  const h = harness();
  h.run("pickedSourceId = 'screen:1'");
  await h.run('startShare()');
  assert.equal(h.calls.capture, 0);
  assert.equal(h.calls.replace, 0);
});

test('Wayland solicita nova fonte no portal sem selecionar ID no Electron', async () => {
  const h = harness();
  h.run('usesWaylandPortal = true; pickedSourceId = null');
  await h.run('startShare()');
  assert.equal(h.calls.select, 0);
  assert.equal(h.calls.capture, 1);
  assert.equal(h.calls.replace, 1);
});

test('a nova captura encerrada pelo sistema também encerra o compartilhamento', async () => {
  const h = harness();
  await h.run('startShare()');
  h.next.dispatchEvent(new Event('ended'));
  await new Promise(setImmediate);
  assert.equal(h.calls.disable, 1);
  assert.equal(h.run('localShareTrack'), null);
  assert.equal(h.element('btn-switch').hidden, true);
});

test('o compartilhamento inicial ainda publica a tela e mostra o botão de troca', async () => {
  const h = harness();
  h.run(`
    localShareTrack = null; pickerMode = 'start';
    els.pkQuality.value = '1080p60'; els.pkHint.value = 'motion';
  `);
  await h.run('startShare()');
  assert.equal(h.calls.publish, 1);
  assert.equal(h.calls.replace, 0);
  assert.equal(h.run('localShareTrack'), h.local);
  assert.equal(h.element('btn-switch').hidden, false);
  assert.equal(h.element('btn-share').hidden, true);
  assert.equal(h.element('btn-stop').hidden, false);
  assert.equal(h.element('picker').open, false);
});

test('cancelar a primeira captura enquanto publica libera a faixa atrasada', async () => {
  const h = harness();
  const pending = deferred();
  h.context.pendingPublication = pending.promise;
  h.run(`
    localShareTrack = null; pickerMode = 'start';
    els.pkQuality.value = '1080p60'; els.pkHint.value = 'motion';
    participant.setScreenShareEnabled = () => pendingPublication;
  `);
  const operation = h.run('startShare()');
  await new Promise(setImmediate);
  h.element('picker').close();
  pending.resolve({ track: h.local });
  await operation;
  assert.equal(h.calls.unpublish, 1);
  assert.equal(h.old.readyState, 'ended');
  assert.equal(h.run('localShareTrack'), null);
});

test('não inicia outra seleção antes de a parada anterior terminar', async () => {
  const h = harness();
  const pending = deferred();
  h.context.pendingStop = pending.promise;
  h.run('participant.setScreenShareEnabled = () => pendingStop');
  const operation = h.run('stopShare()');
  await h.run("openPicker('start')");
  assert.equal(h.element('picker').open, false);
  assert.equal(h.element('btn-share').disabled, true);
  pending.resolve();
  await operation;
  assert.equal(h.element('btn-share').disabled, false);
});
