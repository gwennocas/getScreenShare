// Configurações do usuário, salvas no próprio app (localStorage).

const SETTINGS_KEY = 'getscreenshare.settings.v1';

const QUALITY = {
  '1080p60': { width: 1920, height: 1080, fps: 60, bitrate: 6_000_000 },
  '1080p30': { width: 1920, height: 1080, fps: 30, bitrate: 4_000_000 },
  '720p60':  { width: 1280, height: 720,  fps: 60, bitrate: 3_500_000 },
  '720p30':  { width: 1280, height: 720,  fps: 30, bitrate: 2_000_000 },
};
const DEFAULT_SETTINGS = {
  name: '',
  room: '',
  serverUrl: '',
  accessCode: '',
  codec: 'h264',
  simulcast: true,
  quality: '1080p60',
  hint: 'motion',       // 'motion' = fluidez, 'detail' = nitidez
  shareAudio: false,
  showStats: true,
  lowLatency: true,
};

let currentSettings = { ...DEFAULT_SETTINGS };

function loadSettings() {
  try {
    const raw = localStorage.getItem(SETTINGS_KEY);
    if (raw) currentSettings = { ...DEFAULT_SETTINGS, ...JSON.parse(raw) };
  } catch (_) { /* sem armazenamento: usa os padrões */ }
  // Descarta o modo de autenticação antigo, preservando URL e preferências.
  delete currentSettings.tokenMode;
  if (!QUALITY[currentSettings.quality]) currentSettings.quality = DEFAULT_SETTINGS.quality;
  return currentSettings;
}

function getSettings() {
  return currentSettings;
}

function saveSettings(patch) {
  currentSettings = { ...currentSettings, ...patch };
  try {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(currentSettings));
  } catch (_) { /* ignora */ }
  return currentSettings;
}
