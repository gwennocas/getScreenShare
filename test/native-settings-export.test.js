const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('fs/promises');
const os = require('os');
const path = require('path');
const { writeNativeSettings } = require('../native-settings-export');

test('exports Windows migration settings without the access code', async () => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'gss-settings-'));
  try {
    await writeNativeSettings(directory, {
      name: 'Ana', room: 'jogo', serverUrl: 'https://tokens.example',
      shareAudio: true, quality: '1080p60', accessCode: 'secret',
    });
    const saved = await fs.readFile(path.join(directory, 'native-settings.json'), 'utf8');
    assert.deepEqual(JSON.parse(saved), {
      name: 'Ana', room: 'jogo', server: 'https://tokens.example', share_audio: true, fps_mode: 1, show_stats: true,
    });
    assert.equal(saved.includes('secret'), false);
  } finally {
    await fs.rm(directory, { recursive: true, force: true });
  }
});
