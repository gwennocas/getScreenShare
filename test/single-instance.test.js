const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');
const { EventEmitter } = require('node:events');
const { acquireSingleInstance } = require('../single-instance');

function setup({ owner = true, ready = Promise.resolve() } = {}) {
  const app = new EventEmitter();
  const calls = { quit: 0, show: 0, lock: 0 };
  app.requestSingleInstanceLock = () => { calls.lock++; return owner; };
  app.quit = () => { calls.quit++; };
  app.whenReady = () => ready;
  const acquired = acquireSingleInstance({ app, showWindow: () => { calls.show++; } });
  return { app, calls, acquired };
}

test('a primeira instância mantém o bloqueio e inicia normalmente', () => {
  const h = setup();
  assert.equal(h.acquired, true);
  assert.equal(h.calls.lock, 1);
  assert.equal(h.calls.quit, 0);
  assert.equal(h.calls.show, 0);
});

test('uma cópia adicional sai sem registrar a reabertura de janela', () => {
  const h = setup({ owner: false });
  assert.equal(h.acquired, false);
  assert.equal(h.calls.quit, 1);
  assert.equal(h.app.listenerCount('second-instance'), 0);
  assert.equal(h.calls.show, 0);
});

test('abrir o app novamente reabre a janela da instância existente', async () => {
  const h = setup();
  h.app.emit('second-instance');
  await Promise.resolve();
  assert.equal(h.calls.show, 1);
  assert.equal(h.calls.quit, 0);
  assert.equal(h.calls.lock, 1);
});

test('abertura durante a inicialização aguarda o Electron ficar pronto', async () => {
  let finishReady;
  const ready = new Promise((resolve) => { finishReady = resolve; });
  const h = setup({ ready });
  h.app.emit('second-instance');
  await Promise.resolve();
  assert.equal(h.calls.show, 0);
  finishReady();
  await Promise.resolve();
  assert.equal(h.calls.show, 1);
});

test('aberturas repetidas reutilizam a instância sem adquirir novos bloqueios', async () => {
  const h = setup();
  for (let i = 0; i < 10; i++) h.app.emit('second-instance');
  await Promise.resolve();
  assert.equal(h.calls.show, 10);
  assert.equal(h.calls.lock, 1);
  assert.equal(h.calls.quit, 0);
});

test('o entrypoint de uma cópia adicional não carrega serviços, IPC ou bandeja', () => {
  const h = setup({ owner: false });
  h.calls.quit = 0;
  h.app.setName = () => {};
  h.app.setAppUserModelId = () => {};
  const source = fs.readFileSync(path.join(__dirname, '../main.js'), 'utf8');
  const entrypoint = vm.runInNewContext(`(function(require, process) { ${source}\n})`);
  const loaded = [];
  entrypoint((name) => {
    loaded.push(name);
    if (name === 'electron') return { app: h.app };
    if (name === './single-instance') return { acquireSingleInstance };
    if (['path', 'child_process', 'util'].includes(name)) return require(name);
    throw new Error('Serviço carregado pela cópia adicional: ' + name);
  }, { platform: 'win32' });
  assert.equal(h.calls.quit, 1);
  assert.equal(loaded.includes('./token'), false);
  assert.equal(loaded.includes('./tray-lifecycle'), false);
  assert.equal(loaded.includes('electron-updater'), false);
});
