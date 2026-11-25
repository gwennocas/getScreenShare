// Mantém o contrato usado pela interface enquanto o shell migra de Electron.
if (!window.api && window.__TAURI__) {
  const { invoke } = window.__TAURI__.core;
  const { listen } = window.__TAURI__.event;
  const currentWindow = window.__TAURI__.window.getCurrentWindow();
  let audioListener;

  document.body.dataset.shell = 'tauri';
  document.getElementById('topbar').addEventListener('mousedown', (event) => {
    if (event.button === 0 && !event.target.closest('button')) {
      void currentWindow.startDragging();
    }
  });
  document.getElementById('btn-window-minimize').addEventListener('click', () => {
    void currentWindow.minimize();
  });
  document.getElementById('btn-window-maximize').addEventListener('click', () => {
    void currentWindow.toggleMaximize();
  });
  document.getElementById('btn-window-close').addEventListener('click', () => {
    void currentWindow.close();
  });
  const maximizeButton = document.getElementById('btn-window-maximize');
  const syncMaximizeButton = async () => {
    const maximized = await currentWindow.isMaximized();
    const label = maximized ? 'Restaurar' : 'Maximizar';
    maximizeButton.setAttribute('aria-label', label);
    maximizeButton.title = label;
    maximizeButton.querySelector('use').setAttribute('href', maximized ? '#i-restore' : '#i-maximize');
  };
  void syncMaximizeButton();
  void currentWindow.onResized(() => { void syncMaximizeButton(); });

  window.api = {
    copyRoomName: (roomName) => navigator.clipboard.writeText(roomName),
    listSources: () => invoke('list_sources'),
    selectSource: (sourceId) => invoke('select_source', { sourceId }),
    startExcludedAudio: () => invoke('start_excluded_audio'),
    stopExcludedAudio: () => invoke('stop_excluded_audio'),
    onExcludedAudioChunk: (callback) => {
      if (audioListener) void audioListener();
      audioListener = listen('excluded-audio-chunk', ({ payload }) => {
        callback(Uint8Array.from(payload).buffer);
      });
    },
    removeExcludedAudioChunkListener: () => {
      if (audioListener) void audioListener.then((unlisten) => unlisten());
      audioListener = null;
    },
    getToken: (params) => invoke('get_token', { params }),
    loadLegacySettings: () => invoke('load_legacy_settings'),
    exportNativeSettings: (settings) => invoke('save_native_settings', { settings }),
    checkServer: (serverUrl) => invoke('check_server', { serverUrl }),
    openGpuDiagnostics: () => invoke('open_gpu_diagnostics'),
    getGpuStatus: () => invoke('get_gpu_status'),
    getAppInfo: () => invoke('get_app_info'),
    getUpdaterStatus: () => invoke('updater_get_status'),
    checkForUpdates: () => invoke('updater_check'),
    downloadUpdate: () => invoke('updater_download'),
    installUpdate: () => invoke('updater_install'),
    onUpdaterStatus: (callback) => {
      let unlisten;
      void listen('updater-status', ({ payload }) => callback(payload))
        .then((stop) => { unlisten = stop; });
      return () => { if (unlisten) unlisten(); };
    },
  };
}
