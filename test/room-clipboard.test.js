const assert = require('node:assert/strict');
const test = require('node:test');
const { registerRoomClipboard } = require('../room-clipboard');

function setup(writeText = async () => {}) {
  const win = { isDestroyed: () => false, webContents: { mainFrame: {} } };
  let handler;
  registerRoomClipboard({
    ipcMain: { handle: (channel, callback) => {
      assert.equal(channel, 'copy-room-name');
      handler = callback;
    } },
    clipboard: { writeText }, getWindow: () => win,
  });
  const event = { sender: win.webContents, senderFrame: win.webContents.mainFrame };
  return { win, event, copy: (name, source = event) => handler(source, name) };
}

test('copia nomes Unicode e aguarda a escrita nativa antes de confirmar', async () => {
  let written;
  let finish;
  const { copy } = setup((text) => {
    written = text;
    return new Promise((resolve) => { finish = resolve; });
  });
  let done = false;
  const pending = copy('Sala da Gŵen 123').then((result) => { done = true; return result; });
  await Promise.resolve();
  assert.equal(written, 'Sala da Gŵen 123');
  assert.equal(done, false);
  finish();
  assert.equal(await pending, true);
});

test('recusa outras janelas, subframes e a janela encerrada', async () => {
  let writes = 0;
  const { win, event, copy } = setup(async () => { writes++; });
  await assert.rejects(copy('gwen123', { ...event, sender: {} }), /não autorizada/);
  await assert.rejects(copy('gwen123', { ...event, senderFrame: {} }), /não autorizada/);
  win.isDestroyed = () => true;
  await assert.rejects(copy('gwen123'), /não autorizada/);
  assert.equal(writes, 0);
});

test('recusa valores inválidos sem alterar o clipboard', async () => {
  let writes = 0;
  const { copy } = setup(async () => { writes++; });
  for (const value of [null, 123, '', 'x'.repeat(65), 'sala\ncom quebra', '<script>']) {
    await assert.rejects(copy(value), /inválido/);
  }
  assert.equal(writes, 0);
});

test('propaga uma falha nativa para a interface mostrar o erro', async () => {
  const { copy } = setup(async () => { throw new Error('Clipboard indisponível'); });
  await assert.rejects(copy('gwen123'), /Clipboard indisponível/);
});
