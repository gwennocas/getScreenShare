const start = document.querySelector('#start');
const stop = document.querySelector('#stop');
const status = document.querySelector('#status');
const preview = document.querySelector('#preview');
let stream;

function finish() {
  if (stream) stream.getTracks().forEach((track) => track.stop());
  stream = null;
  preview.srcObject = null;
  start.disabled = false;
  stop.disabled = true;
  status.textContent = 'Nenhuma captura ativa.';
}

start.addEventListener('click', async () => {
  start.disabled = true;
  try {
    stream = await navigator.mediaDevices.getDisplayMedia({ video: true, audio: false });
    stream.getVideoTracks()[0].addEventListener('ended', finish, { once: true });
    preview.srcObject = stream;
    stop.disabled = false;
    status.textContent = 'Captura ativa.';
  } catch (error) {
    start.disabled = false;
    status.textContent = `Não foi possível iniciar a captura: ${error.message}`;
  }
});
stop.addEventListener('click', finish);
