const ROOM_RE = /^[\p{L}\p{N}_\- ]{1,64}$/u;

function registerRoomClipboard({ ipcMain, clipboard, getWindow }) {
  ipcMain.handle('copy-room-name', async (event, roomName) => {
    const win = getWindow();
    if (!win || win.isDestroyed()
      || event.sender !== win.webContents
      || event.senderFrame !== win.webContents.mainFrame) {
      throw new Error('Solicitação de cópia não autorizada.');
    }
    if (typeof roomName !== 'string' || !ROOM_RE.test(roomName)) {
      throw new Error('Nome de sala inválido.');
    }
    await clipboard.writeText(roomName);
    return true;
  });
}

module.exports = { registerRoomClipboard };
