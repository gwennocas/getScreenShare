const assert = require('node:assert/strict');
const test = require('node:test');
const { EventEmitter } = require('node:events');
const { createTrayLifecycle } = require('../tray-lifecycle');

function setup({ failTray = false, emptyIcon = false } = {}) {
  const app = new EventEmitter();
  const nativeUpdater = new EventEmitter();
  const win = new EventEmitter();
  const calls = { hide: 0, show: 0, focus: 0, restore: 0, quit: 0, errors: 0 };
  win.isDestroyed = () => false;
  win.isMinimized = () => false;
  for (const key of ['hide', 'show', 'focus', 'restore']) win[key] = () => { calls[key]++; };
  app.quit = () => { calls.quit++; app.emit('before-quit'); win.emit('close', event()); };
  let tray;
  class FakeTray extends EventEmitter {
    constructor() {
      super();
      if (failTray) throw new Error('sem bandeja');
      tray = this;
      this.destroyed = false;
    }
    setToolTip(value) { this.tooltip = value; }
    setContextMenu(menu) { this.menu = menu; }
    isDestroyed() { return this.destroyed; }
    destroy() { this.destroyed = true; }
  }
  const lifecycle = createTrayLifecycle({
    app, nativeUpdater, Tray: FakeTray,
    Menu: { buildFromTemplate: (items) => items },
    nativeImage: { createFromPath: () => ({ isEmpty: () => emptyIcon, resize: () => ({}) }) },
    getWindow: () => win, iconPath: 'build/icon.png', platform: 'win32',
    logError: () => { calls.errors++; },
  });
  const event = () => ({ prevented: false, preventDefault() { this.prevented = true; } });
  lifecycle.createTray();
  lifecycle.attachWindow(win);
  return { app, nativeUpdater, win, calls, lifecycle, tray, event };
}

test('X esconde a janela sem fechá-la nem encerrar o aplicativo', () => {
  const h = setup();
  const event = h.event();
  h.win.emit('close', event);
  assert.equal(event.prevented, true);
  assert.equal(h.calls.hide, 1);
  assert.equal(h.calls.quit, 0);
  assert.equal(h.tray.tooltip, 'getScreenShare');
});

test('clique na bandeja restaura e dá foco à janela minimizada', () => {
  const h = setup();
  h.win.isMinimized = () => true;
  h.tray.emit('click');
  assert.equal(h.calls.restore, 1);
  assert.equal(h.calls.show, 1);
  assert.equal(h.calls.focus, 1);
});

test('o menu da bandeja reabre o aplicativo e permite sair de verdade', () => {
  const h = setup();
  h.tray.menu.find((item) => item.label === 'Abrir getScreenShare').click();
  assert.equal(h.calls.show, 1);
  h.tray.menu.find((item) => item.label === 'Sair').click();
  const event = h.event();
  h.win.emit('close', event);
  assert.equal(h.calls.quit, 1);
  assert.equal(event.prevented, false);
  assert.equal(h.calls.hide, 0);
});

test('reiniciar para instalar atualização não é bloqueado pela bandeja', () => {
  const h = setup();
  h.nativeUpdater.emit('before-quit-for-update');
  const event = h.event();
  h.win.emit('close', event);
  assert.equal(event.prevented, false);
  assert.equal(h.calls.hide, 0);
});

test('desligamento ou logout do Windows permite o fechamento da janela', () => {
  const h = setup();
  h.win.emit('query-session-end');
  const event = h.event();
  h.win.emit('close', event);
  assert.equal(event.prevented, false);
});

for (const option of ['failTray', 'emptyIcon']) {
  test(`falha da bandeja (${option}) preserva o fechamento normal`, () => {
    const h = setup({ [option]: true });
    const event = h.event();
    h.win.emit('close', event);
    assert.equal(event.prevented, false);
    assert.equal(h.calls.hide, 0);
    assert.equal(h.calls.errors, 1);
  });
}

test('sair destrói o ícone e eventos posteriores não tentam reabrir a janela', () => {
  const h = setup();
  h.app.emit('before-quit');
  h.app.emit('will-quit');
  h.tray.emit('click');
  assert.equal(h.tray.destroyed, true);
  assert.equal(h.calls.show, 0);
});

test('uma janela destruída não é reaberta por clique na bandeja', () => {
  const h = setup();
  h.win.isDestroyed = () => true;
  h.tray.emit('click');
  assert.equal(h.calls.show, 0);
});
