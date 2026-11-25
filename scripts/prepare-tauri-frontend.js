const fs = require('node:fs');
const path = require('node:path');

const root = path.resolve(__dirname, '..');
const source = path.join(root, 'renderer');
const output = path.join(root, 'experiments', 'tauri-capture', 'frontend');
const livekit = path.join(root, 'node_modules', 'livekit-client', 'dist', 'livekit-client.umd.js');

if (!fs.existsSync(livekit)) {
  throw new Error('Instale as dependências do projeto antes de preparar a interface Tauri.');
}

fs.rmSync(output, { recursive: true, force: true });
fs.cpSync(source, output, { recursive: true });
fs.mkdirSync(path.join(output, 'vendor'), { recursive: true });
fs.copyFileSync(livekit, path.join(output, 'vendor', 'livekit-client.umd.js'));

const htmlPath = path.join(output, 'index.html');
const html = fs.readFileSync(htmlPath, 'utf8').replace(
  '../node_modules/livekit-client/dist/livekit-client.umd.js',
  'vendor/livekit-client.umd.js',
);
fs.writeFileSync(htmlPath, html);
