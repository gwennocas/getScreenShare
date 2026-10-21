// Ponte segura entre a interface (renderer) e o processo principal.
const { contextBridge, ipcRenderer } = require('electron');

let audioChunkListener = null;

contextBridge.exposeInMainWorld('api', {
  listSources: () => ipcRenderer.invoke('list-sources'),
  selectSource: (sourceId, audio) => ipcRenderer.invoke('select-source', { sourceId, audio }),
  startExcludedAudio: () => ipcRenderer.invoke('start-excluded-audio'),
  stopExcludedAudio: () => ipcRenderer.invoke('stop-excluded-audio'),
  onExcludedAudioChunk: (callback) => {
    if (audioChunkListener) ipcRenderer.removeListener('excluded-audio-chunk', audioChunkListener);
    audioChunkListener = (_event, data) => { if (typeof callback === 'function') callback(data); };
    ipcRenderer.on('excluded-audio-chunk', audioChunkListener);
  },
  removeExcludedAudioChunkListener: () => {
    if (audioChunkListener) ipcRenderer.removeListener('excluded-audio-chunk', audioChunkListener);
    audioChunkListener = null;
  },
  getToken: (params) => ipcRenderer.invoke('get-token', params),
  checkServer: (serverUrl) => ipcRenderer.invoke('check-server', serverUrl),
  openGpuDiagnostics: () => ipcRenderer.invoke('open-gpu-diagnostics'),
  getAppInfo: () => ipcRenderer.invoke('get-app-info'),
  getUpdaterStatus: () => ipcRenderer.invoke('updater:get-status'),
  checkForUpdates: () => ipcRenderer.invoke('updater:check'),
  downloadUpdate: () => ipcRenderer.invoke('updater:download'),
  installUpdate: () => ipcRenderer.invoke('updater:install'),
  onUpdaterStatus: (callback) => {
    if (typeof callback !== 'function') return () => {};
    const listener = (_event, state) => callback(state);
    ipcRenderer.on('updater:status', listener);
    return () => ipcRenderer.removeListener('updater:status', listener);
  },
});
