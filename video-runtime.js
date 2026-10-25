// O Chromium desativa VA-API para codificação no Linux por padrão.
// https://chromium.googlesource.com/chromium/src/+/HEAD/docs/gpu/vaapi.md
function configureVideoRuntime(app, platform = process.platform) {
  if (platform !== 'linux') return;

  const commandLine = app.commandLine;
  const features = commandLine.getSwitchValue('enable-features').split(',').filter(Boolean);
  if (!features.some((feature) => /^AcceleratedVideoEncoder(?:$|[<:])/.test(feature))) {
    features.push('AcceleratedVideoEncoder');
  }
  commandLine.appendSwitch('enable-features', features.join(','));

  // VA-API pode compartilhar superfícies com ANGLE/OpenGL no Wayland.
  // Preserva escolhas explícitas de backend passadas pelo usuário.
  if (!commandLine.hasSwitch('use-gl')) commandLine.appendSwitch('use-gl', 'angle');
  if (!commandLine.hasSwitch('use-angle')) commandLine.appendSwitch('use-angle', 'gl');
}

module.exports = { configureVideoRuntime };
