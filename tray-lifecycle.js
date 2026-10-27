const path = require('node:path');

function createTrayLifecycle({ app, Tray, Menu, nativeImage, nativeUpdater, getWindow, iconPath, platform = process.platform, logError = console.error }) {
  let tray = null;
  let quitting = false;

  const beginQuit = () => { quitting = true; };
  const showWindow = () => {
    const win = getWindow();
    if (!win || win.isDestroyed() || quitting) return;
    if (win.isMinimized()) win.restore();
    win.show();
    win.focus();
  };

  app.on('before-quit', beginQuit);
  nativeUpdater.on('before-quit-for-update', beginQuit);
  app.on('activate', showWindow);
  app.on('will-quit', () => {
    if (tray && !tray.isDestroyed()) tray.destroy();
    tray = null;
  });

  function createTray() {
    if (tray && !tray.isDestroyed()) return true;
    try {
      const icon = nativeImage.createFromPath(path.resolve(iconPath));
      if (icon.isEmpty()) throw new Error('Ícone da bandeja não encontrado.');
      const size = platform === 'win32' ? 16 : 24;
      tray = new Tray(icon.resize({ width: size, height: size }));
      tray.setToolTip('getScreenShare');
      tray.setContextMenu(Menu.buildFromTemplate([
        { label: 'Abrir getScreenShare', click: showWindow },
        { type: 'separator' },
        { label: 'Sair', click: () => app.quit() },
      ]));
      tray.on('click', showWindow);
      tray.on('double-click', showWindow);
      return true;
    } catch (err) {
      if (tray && !tray.isDestroyed()) tray.destroy();
      tray = null;
      logError('Não foi possível criar o ícone da bandeja:', err);
      return false;
    }
  }

  function attachWindow(win) {
    win.on('close', (event) => {
      // Se a bandeja não estiver disponível, mantém o fechamento normal para
      // não deixar o aplicativo escondido sem uma maneira de reabri-lo.
      if (quitting || !tray || tray.isDestroyed()) return;
      event.preventDefault();
      win.hide();
    });
    win.on('query-session-end', beginQuit);
  }

  return { createTray, attachWindow, showWindow };
}

module.exports = { createTrayLifecycle };
