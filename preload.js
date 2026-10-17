// Ponte segura entre a interface (renderer) e o processo principal.
const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('api', {
  listSources: () => ipcRenderer.invoke('list-sources'),
  selectSource: (sourceId, audio) => ipcRenderer.invoke('select-source', { sourceId, audio }),
  getToken: (params) => ipcRenderer.invoke('get-token', params),
  checkServer: (serverUrl) => ipcRenderer.invoke('check-server', serverUrl),
  openGpuDiagnostics: () => ipcRenderer.invoke('open-gpu-diagnostics'),
  getAppInfo: () => ipcRenderer.invoke('get-app-info'),
});
