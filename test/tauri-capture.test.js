const { test } = require('node:test');
const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const { runInNewContext } = require('node:vm');
const { join } = require('node:path');

function setup(getDisplayMedia) {
  const elements = Object.fromEntries(['start', 'stop', 'status', 'preview'].map((id) => [id, {
    disabled: id === 'stop',
    addEventListener(_event, handler) { this.click = handler; },
  }]));
  runInNewContext(readFileSync(join(__dirname, '../experiments/tauri-capture/ui/app.js'), 'utf8'), {
    document: { querySelector: (selector) => elements[selector.slice(1)] },
    navigator: { mediaDevices: { getDisplayMedia } },
  });
  return elements;
}

test('prova de captura inicia e encerra a faixa', async () => {
  let stopped = 0;
  const track = { addEventListener(_event, handler) { this.ended = handler; }, stop() { stopped++; } };
  const stream = { getTracks: () => [track], getVideoTracks: () => [track] };
  const ui = setup(async () => stream);
  await ui.start.click();
  assert.equal(ui.preview.srcObject, stream);
  assert.equal(ui.stop.disabled, false);
  ui.stop.click();
  assert.equal(stopped, 1);
  assert.equal(ui.preview.srcObject, null);
  assert.equal(ui.start.disabled, false);
});

test('cancelamento permite tentar novamente', async () => {
  const ui = setup(async () => { throw new Error('Cancelado'); });
  await ui.start.click();
  assert.equal(ui.start.disabled, false);
  assert.match(ui.status.textContent, /Cancelado/);
});

test('interrupção pelo sistema encerra a prévia', async () => {
  const track = { addEventListener(_event, handler) { this.ended = handler; }, stop() {} };
  const stream = { getTracks: () => [track], getVideoTracks: () => [track] };
  const ui = setup(async () => stream);
  await ui.start.click();
  track.ended();
  assert.equal(ui.preview.srcObject, null);
  assert.equal(ui.stop.disabled, true);
});
