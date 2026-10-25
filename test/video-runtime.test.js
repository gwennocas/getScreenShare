const assert = require('node:assert/strict');
const test = require('node:test');
const { configureVideoRuntime } = require('../video-runtime');

function runtime(initial = {}) {
  const switches = new Map(Object.entries(initial));
  return { switches, app: { commandLine: {
    getSwitchValue: (key) => switches.get(key) || '',
    hasSwitch: (key) => switches.has(key),
    appendSwitch: (key, value) => switches.set(key, value),
  } } };
}

test('habilita o encoder Linux sem apagar recursos já configurados', () => {
  const { app, switches } = runtime({ 'enable-features': 'WebRTCPipeWireCapturer' });
  configureVideoRuntime(app, 'linux');
  configureVideoRuntime(app, 'linux');
  assert.equal(switches.get('enable-features'), 'WebRTCPipeWireCapturer,AcceleratedVideoEncoder');
  assert.equal(switches.get('use-gl'), 'angle');
  assert.equal(switches.get('use-angle'), 'gl');
});

test('preserva o backend e os parâmetros explícitos do encoder', () => {
  const { app, switches } = runtime({
    'enable-features': 'AcceleratedVideoEncoder:example/true',
    'use-gl': 'angle', 'use-angle': 'vulkan',
  });
  configureVideoRuntime(app, 'linux');
  assert.equal(switches.get('use-angle'), 'vulkan');
  assert.equal(switches.get('enable-features'), 'AcceleratedVideoEncoder:example/true');
});

test('preserva a configuração do Windows', () => {
  const { app, switches } = runtime();
  configureVideoRuntime(app, 'win32');
  assert.equal(switches.size, 0);
});
