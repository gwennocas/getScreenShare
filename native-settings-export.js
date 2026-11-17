const fs = require('fs/promises');
const path = require('path');
const { randomUUID } = require('crypto');

function nativeSettings(settings) {
  const value = settings && typeof settings === 'object' ? settings : {};
  const text = (field, limit) => typeof value[field] === 'string' ? value[field].slice(0, limit) : '';
  return {
    name: text('name', 40),
    room: text('room', 64),
    server: text('serverUrl', 2048),
    share_audio: value.shareAudio === true,
    fps_mode: typeof value.quality === 'string' && /60$/.test(value.quality) ? 1 : 0,
    show_stats: value.showStats !== false,
  };
}

async function writeNativeSettings(directory, settings) {
  await fs.mkdir(directory, { recursive: true });
  const target = path.join(directory, 'native-settings.json');
  const temporary = `${target}.${randomUUID()}.tmp`;
  try {
    await fs.writeFile(temporary, JSON.stringify(nativeSettings(settings)), { flag: 'wx' });
    await fs.rename(temporary, target);
  } catch (error) {
    await fs.rm(temporary, { force: true }).catch(() => {});
    throw error;
  }
}

module.exports = { nativeSettings, writeNativeSettings };
