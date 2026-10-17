// Processo principal do Electron: janela, lista de telas, captura e tokens.
const path = require('path');
const dotenv = require('dotenv');
const { app, BrowserWindow, ipcMain, desktopCapturer, session, shell } = require('electron');
const tokens = require('./token');

app.setName('getScreenShare');
if (process.platform === 'win32') app.setAppUserModelId('com.getscreenshare.app');

// Lê o .env da pasta de dados do app (versão instalada) e depois da pasta do projeto (npm start).
dotenv.config({ path: path.join(app.getPath('userData'), '.env'), quiet: true });
dotenv.config({ quiet: true });

const BG = '#151a24';
let win;
let selectedSourceId = null; // tela/janela escolhida pelo usuário
let shareAudio = false;      // se true, manda o áudio do sistema junto

function createWindow() {
  win = new BrowserWindow({
    width: 1280,
    height: 800,
    minWidth: 960,
    minHeight: 600,
    title: 'getScreenShare',
    backgroundColor: BG,
    icon: path.join(__dirname, 'build', 'icon.png'),
    // Barra de título própria; os botões de minimizar/fechar do Windows ficam por cima.
    titleBarStyle: 'hidden',
    titleBarOverlay: { color: BG, symbolColor: '#e9ecf4', height: 40 },
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
    },
  });
  win.setMenuBarVisibility(false);
  win.loadFile(path.join(__dirname, 'renderer', 'index.html'));

  // A janela só mostra a nossa interface: links externos abrem no navegador.
  win.webContents.setWindowOpenHandler(({ url }) => {
    if (/^https?:\/\//.test(url)) shell.openExternal(url);
    return { action: 'deny' };
  });
  win.webContents.on('will-navigate', (event) => event.preventDefault());
}

// Quando a página chama getDisplayMedia(), o Electron cai aqui e entrega a tela escolhida.
function setupDisplayMediaHandler() {
  session.defaultSession.setDisplayMediaRequestHandler(async (_request, callback) => {
    try {
      const sources = await desktopCapturer.getSources({ types: ['screen', 'window'] });
      const source = sources.find((s) => s.id === selectedSourceId) || sources[0];
      const response = { video: source };
      // 'loopback' = áudio do sistema no Windows
      if (shareAudio) response.audio = 'loopback';
      callback(response);
    } catch (err) {
      console.error('Erro ao capturar tela:', err);
      callback({});
    }
  });
}

ipcMain.handle('list-sources', async () => {
  const sources = await desktopCapturer.getSources({
    types: ['screen', 'window'],
    thumbnailSize: { width: 320, height: 180 },
    fetchWindowIcons: false,
  });
  return sources.map((s) => ({
    id: s.id,
    name: s.name,
    kind: s.id.startsWith('screen:') ? 'screen' : 'window',
    thumbnail: s.thumbnail.toDataURL(),
  }));
});

ipcMain.handle('select-source', (_event, { sourceId, audio }) => {
  selectedSourceId = sourceId;
  shareAudio = !!audio;
  return true;
});

ipcMain.handle('get-token', async (_event, params) => {
  const { mode, room, identity, name, serverUrl, accessCode } = params || {};
  if (mode === 'server') {
    return tokens.fromServer({ room, identity, name, serverUrl, accessCode });
  }
  return tokens.fromEnv({ room, identity, name });
});

ipcMain.handle('check-server', async (_event, serverUrl) => tokens.checkServer(serverUrl));

// Abre a página interna chrome://gpu, que diz se o vídeo está sendo codificado pela placa de vídeo.
ipcMain.handle('open-gpu-diagnostics', () => {
  const w = new BrowserWindow({ width: 1000, height: 800, title: 'Diagnóstico da GPU', autoHideMenuBar: true });
  w.loadURL('chrome://gpu');
  return true;
});

ipcMain.handle('get-app-info', () => ({ version: app.getVersion() }));

app.whenReady().then(() => {
  setupDisplayMediaHandler();
  createWindow();
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit();
});
