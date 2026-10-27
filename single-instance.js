function acquireSingleInstance({ app, showWindow }) {
  if (!app.requestSingleInstanceLock()) {
    app.quit();
    return false;
  }

  app.on('second-instance', () => {
    // Aguarda a criação da janela se outra abertura chegar durante o início.
    void app.whenReady().then(showWindow);
  });
  return true;
}

module.exports = { acquireSingleInstance };
