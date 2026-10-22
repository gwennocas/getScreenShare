// Processo principal do Electron: janela, lista de telas, captura e tokens.
const path = require('path');
const { execFile } = require('child_process');
const { promisify } = require('util');
const execFileAsync = promisify(execFile);
const { app, BrowserWindow, ipcMain, desktopCapturer, session, shell } = require('electron');
const tokens = require('./token');
const { autoUpdater } = require('electron-updater');

// A aceleração usa os padrões do Chromium. A preferência de perfil H.264 é
// configurada no transceiver, antes da negociação, em renderer/js/encoder.js.

app.setName('getScreenShare');
if (process.platform === 'win32') app.setAppUserModelId('com.getscreenshare.app');

const BG = '#151a24';
let win;

// GETSCREENSHARE_AUTO_UPDATER: atualizações explícitas via GitHub Releases.
let updaterState = {
  status: 'idle',
  currentVersion: app.getVersion(),
  version: null,
  percent: 0,
  error: null,
};
let updaterCheckPromise = null;
let updaterDownloadPromise = null;

function publishUpdaterState(status, extra = {}) {
  updaterState = { ...updaterState, status, currentVersion: app.getVersion(), ...extra };
  if (win && !win.isDestroyed()) {
    win.webContents.send('updater:status', updaterState);
  }
  return updaterState;
}

function updaterErrorMessage(error) {
  return String((error && error.message) || error || 'Erro desconhecido').slice(0, 500);
}

function setupAutoUpdater() {
  // O usuário decide quando baixar e quando reiniciar para instalar.
  autoUpdater.autoDownload = false;
  autoUpdater.autoInstallOnAppQuit = false;
  autoUpdater.allowPrerelease = false;

  autoUpdater.on('checking-for-update', () => {
    publishUpdaterState('checking', { error: null, percent: 0 });
  });
  autoUpdater.on('update-available', (info) => {
    publishUpdaterState('available', { version: info.version, percent: 0, error: null });
  });
  autoUpdater.on('update-not-available', () => {
    publishUpdaterState('not-available', { version: app.getVersion(), percent: 0, error: null });
  });
  autoUpdater.on('download-progress', (progress) => {
    publishUpdaterState('downloading', {
      percent: Math.max(0, Math.min(100, Math.floor(progress.percent || 0))),
      transferred: progress.transferred || 0,
      total: progress.total || 0,
      bytesPerSecond: progress.bytesPerSecond || 0,
    });
  });
  autoUpdater.on('update-downloaded', (info) => {
    publishUpdaterState('downloaded', { version: info.version, percent: 100, error: null });
  });
  autoUpdater.on('error', (error) => {
    publishUpdaterState('error', { error: updaterErrorMessage(error) });
  });
}

async function checkForUpdates() {
  if (!app.isPackaged) {
    return publishUpdaterState('dev', { error: null });
  }
  if (process.platform !== 'win32') {
    return publishUpdaterState('unsupported', { error: null });
  }
  if (updaterCheckPromise) return updaterCheckPromise;

  publishUpdaterState('checking', { error: null, percent: 0 });
  updaterCheckPromise = (async () => {
    try {
      await autoUpdater.checkForUpdates();
      return updaterState;
    } catch (error) {
      return publishUpdaterState('error', { error: updaterErrorMessage(error) });
    } finally {
      updaterCheckPromise = null;
    }
  })();
  return updaterCheckPromise;
}

async function downloadUpdate() {
  if (!app.isPackaged || process.platform !== 'win32') {
    return updaterState;
  }
  if (updaterState.status !== 'available') return updaterState;
  if (updaterDownloadPromise) return updaterDownloadPromise;

  publishUpdaterState('downloading', { percent: 0, error: null });
  updaterDownloadPromise = (async () => {
    try {
      await autoUpdater.downloadUpdate();
      return updaterState;
    } catch (error) {
      return publishUpdaterState('error', { error: updaterErrorMessage(error) });
    } finally {
      updaterDownloadPromise = null;
    }
  })();
  return updaterDownloadPromise;
}

let selectedSourceId = null; // tela/janela escolhida pelo usuário
let excludedAudioAddon = null;
let excludedAudioActive = false;

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
      // Nunca pedir o loopback do Electron: o áudio é capturado separadamente
      // pelo WASAPI com exclusão explícita do processo do Discord.
      callback({ video: source });
    } catch (err) {
      console.error('Erro ao capturar tela:', err);
      callback({});
    }
  });
}


// Encontra o processo raiz do Discord (os processos filhos também serão excluídos
// pelo WASAPI Process Loopback). Não usa shell para construir o comando.
async function findDiscordRootPid() {
  if (process.platform !== 'win32' || process.arch !== 'x64') {
    throw new Error('A exclusão do áudio do Discord está disponível nesta versão apenas no Windows x64.');
  }

  const powershell = path.join(
    process.env.SystemRoot || 'C:\\Windows',
    'System32', 'WindowsPowerShell', 'v1.0', 'powershell.exe'
  );
  const script = [
    "$names = @('Discord.exe','DiscordPTB.exe','DiscordCanary.exe')",
    "$procs = @(Get-CimInstance -ClassName Win32_Process -ErrorAction Stop | Where-Object { $names -contains $_.Name })",
    "$ids = @($procs | ForEach-Object { [int]$_.ProcessId })",
    "$roots = @($procs | Where-Object { $ids -notcontains [int]$_.ParentProcessId } | ForEach-Object { [pscustomobject]@{ pid=[int]$_.ProcessId; name=[string]$_.Name } })",
    "[pscustomobject]@{ roots=$roots } | ConvertTo-Json -Compress -Depth 3"
  ].join('; ');

  let stdout;
  try {
    ({ stdout } = await execFileAsync(powershell, [
      '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-Command', script
    ], { windowsHide: true, timeout: 8000, maxBuffer: 1024 * 1024 }));
  } catch (err) {
    throw new Error('Não consegui consultar os processos do Discord. ' + err.message);
  }

  let result;
  try {
    result = JSON.parse(String(stdout || '').trim());
  } catch (_) {
    throw new Error('Não consegui interpretar a lista de processos do Discord. Abra o Discord e tente novamente.');
  }
  const value = result && result.roots;
  const roots = Array.isArray(value) ? value : (value ? [value] : []);
  if (!roots.length) {
    throw new Error('O Discord não foi encontrado. Abra o Discord antes de compartilhar o áudio; o áudio ficará desativado para evitar loopback.');
  }
  const rootProcess = roots.find((item) => /^Discord\.exe$/i.test(item.name || '')) || roots[0];
  const pid = Number(rootProcess.pid);
  if (!Number.isInteger(pid) || pid <= 0) {
    throw new Error('Não consegui identificar o PID raiz do Discord. O áudio ficará desativado para evitar loopback.');
  }
  return pid;
}

function loadWasapiLoopback() {
  if (process.platform !== 'win32' || process.arch !== 'x64') {
    throw new Error('A captura filtrada exige Windows x64. O vídeo pode ser compartilhado sem áudio.');
  }
  const addonPath = path.join(
    __dirname, 'node_modules', 'wasapi-loopback', 'prebuilds',
    'windows-x86_64', 'wasapi-loopback-win32-x64.node'
  );
  try {
    return require(addonPath);
  } catch (err) {
    throw new Error('O módulo WASAPI não foi carregado. Execute npm install e compile novamente. Detalhe: ' + err.message);
  }
}

function stopNativeExcludedAudio() {
  const addon = excludedAudioAddon;
  excludedAudioAddon = null;
  excludedAudioActive = false;
  if (addon && typeof addon.stop === 'function') {
    try { addon.stop(); } catch (err) { console.error('Erro ao parar a captura WASAPI:', err); }
  }
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

ipcMain.handle('select-source', (_event, { sourceId }) => {
  selectedSourceId = sourceId;
  return true;
});

ipcMain.handle('start-excluded-audio', async () => {
  if (excludedAudioActive) return true;
  const discordPid = await findDiscordRootPid();
  const addon = loadWasapiLoopback();
  let started = false;
  try {
    started = addon.start(discordPid, (chunk) => {
      if (!win || win.isDestroyed()) return;
      try {
        const bytes = Buffer.from(chunk);
        const audioBuffer = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
        win.webContents.send('excluded-audio-chunk', audioBuffer);
      } catch (err) {
        console.error('Erro ao encaminhar um bloco de áudio:', err);
      }
    });
  } catch (err) {
    throw new Error(`Falha ao iniciar WASAPI: ${err.message}`);
  }
  if (!started) {
    throw new Error('O WASAPI Process Loopback não iniciou. Ele requer Windows 10 versão 2004 ou mais recente; o áudio ficará desativado para evitar loopback.');
  }
  excludedAudioAddon = addon;
  excludedAudioActive = true;
  return true;
});

ipcMain.handle('stop-excluded-audio', () => {
  stopNativeExcludedAudio();
  return true;
});

ipcMain.handle('get-token', async (_event, params) => {
  const { room, identity, name, serverUrl, accessCode } = params || {};
  return tokens.fromServer({ room, identity, name, serverUrl, accessCode });
});

ipcMain.handle('check-server', async (_event, serverUrl) => tokens.checkServer(serverUrl));

ipcMain.handle('updater:get-status', () => updaterState);
ipcMain.handle('updater:check', () => checkForUpdates());
ipcMain.handle('updater:download', () => downloadUpdate());
ipcMain.handle('updater:install', () => {
  if (!app.isPackaged || process.platform !== 'win32' || updaterState.status !== 'downloaded') return false;
  // Só instala depois do clique explícito do usuário.
  autoUpdater.quitAndInstall(false, true);
  return true;
});

// Abre a página interna chrome://gpu, que diz se o vídeo está sendo codificado pela placa de vídeo.
ipcMain.handle('open-gpu-diagnostics', () => {
  const w = new BrowserWindow({ width: 1000, height: 800, title: 'Diagnóstico da GPU', autoHideMenuBar: true });
  w.loadURL('chrome://gpu');
  return true;
});

ipcMain.handle('get-gpu-status', async () => {
  try { await app.getGPUInfo('basic'); } catch (_) { /* GPU pode estar indisponível */ }
  return {
    hardwareAcceleration: app.isHardwareAccelerationEnabled(),
    videoEncode: app.getGPUFeatureStatus().video_encode || 'unknown',
  };
});

ipcMain.handle('get-app-info', () => ({ version: app.getVersion() }));

app.whenReady().then(() => {
  setupDisplayMediaHandler();
  createWindow();
  setupAutoUpdater();

  // Verifica sozinho ao abrir, mas nunca baixa/instala sem a confirmação do usuário.
  if (app.isPackaged && process.platform === 'win32') {
    setTimeout(() => { checkForUpdates(); }, 6000);
  }
});

app.on('before-quit', () => {
  stopNativeExcludedAudio();
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit();
});
