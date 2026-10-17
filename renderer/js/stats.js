// Estatísticas ao vivo do WebRTC. Cada função devolve uma função "parar".
// O callback recebe uma lista de linhas [rótulo, valor], pronta pra mostrar na tela.

function fmtBitrate(bps) {
  if (!isFinite(bps)) return '-';
  return bps >= 1e6 ? `${(bps / 1e6).toFixed(2)} Mbps` : `${Math.round(bps / 1e3)} kbps`;
}

function fmtMs(v) {
  return isFinite(v) ? `${v.toFixed(1)} ms` : '-';
}

// Ping (RTT) do caminho de rede em uso
function findRtt(report) {
  let rtt = NaN;
  report.forEach((s) => {
    if (s.type === 'candidate-pair' && s.nominated && s.currentRoundTripTime != null) {
      rtt = s.currentRoundTripTime * 1000;
    }
  });
  return rtt;
}

function codecLabel(report, codecId) {
  const c = codecId ? report.get(codecId) : null;
  return c && c.mimeType ? c.mimeType.replace('video/', '') : '?';
}

// "powerEfficient" é o sinal do navegador de que o codec roda na placa de vídeo.
function accelLabel(powerEfficient) {
  if (powerEfficient === true) return 'placa de vídeo';
  if (powerEfficient === false) return 'processador';
  return 'não informado';
}

// Quem RECEBE a tela de alguém
function pollReceiverStats(track, onUpdate) {
  let prev = null;
  const id = setInterval(async () => {
    if (!track.receiver) return;
    try {
      const report = await track.receiver.getStats();
      let s = null;
      report.forEach((r) => { if (r.type === 'inbound-rtp' && r.kind === 'video') s = r; });
      if (!s) return;

      // Médias só do último segundo (diferença entre duas leituras)
      const dt = prev ? (s.timestamp - prev.timestamp) / 1000 : 0;
      const bitrate = dt > 0 ? ((s.bytesReceived - prev.bytesReceived) * 8) / dt : NaN;
      const dFrames = prev ? s.framesDecoded - prev.framesDecoded : 0;
      const decodeMs = dFrames > 0 ? ((s.totalDecodeTime - prev.totalDecodeTime) / dFrames) * 1000 : NaN;
      const dEmit = prev ? s.jitterBufferEmittedCount - prev.jitterBufferEmittedCount : 0;
      const bufferMs = dEmit > 0 ? ((s.jitterBufferDelay - prev.jitterBufferDelay) / dEmit) * 1000 : NaN;
      const dRecv = prev ? s.packetsReceived - prev.packetsReceived : 0;
      const dLost = prev ? s.packetsLost - prev.packetsLost : 0;
      const lossPct = dRecv + dLost > 0 ? (dLost / (dRecv + dLost)) * 100 : 0;
      prev = s;

      const res = `${s.frameWidth || '?'}x${s.frameHeight || '?'}`;
      const fps = Math.round(s.framesPerSecond || 0);
      const rtt = findRtt(report);
      onUpdate(
        [
          ['Resolução', res],
          ['Frames', `${fps} fps`],
          ['Bitrate', fmtBitrate(bitrate)],
          ['Ping', fmtMs(rtt)],
          ['Buffer', fmtMs(bufferMs)],
          ['Decode', fmtMs(decodeMs)],
          ['Perda', `${lossPct.toFixed(1)}%`],
          ['Descartados', String(s.framesDropped || 0)],
          ['Codec', codecLabel(report, s.codecId)],
          ['Decodificador', accelLabel(s.powerEfficientDecoder)],
        ],
        // versão curta, para a linha em cima da tela
        [
          `${res} ${fps} fps`,
          fmtBitrate(bitrate),
          `ping ${isFinite(rtt) ? Math.round(rtt) : '-'} ms`,
          `perda ${lossPct.toFixed(1)}%`,
          s.powerEfficientDecoder === true ? 'decode GPU' : s.powerEfficientDecoder === false ? 'decode CPU' : 'decode ?',
        ]
      );
    } catch (_) { /* a track pode ter acabado de sumir */ }
  }, 1000);
  return () => clearInterval(id);
}

// Quem ENVIA a própria tela
function pollSenderStats(track, onUpdate) {
  const prev = new Map(); // id da camada -> leitura anterior
  const id = setInterval(async () => {
    if (!track.sender) return;
    try {
      const report = await track.sender.getStats();
      const layers = [];
      report.forEach((r) => { if (r.type === 'outbound-rtp' && r.kind === 'video') layers.push(r); });
      if (!layers.length) return;

      // Com simulcast tem uma camada por qualidade; o bitrate é a soma de todas
      let totalBps = 0;
      layers.forEach((r) => {
        const p = prev.get(r.id);
        const dt = p ? (r.timestamp - p.timestamp) / 1000 : 0;
        if (dt > 0) totalBps += ((r.bytesSent - p.bytesSent) * 8) / dt;
      });

      // A camada principal é a de maior resolução
      const top = layers.reduce((a, b) => ((b.frameWidth || 0) > (a.frameWidth || 0) ? b : a));
      const p = prev.get(top.id);
      const dFrames = p ? top.framesEncoded - p.framesEncoded : 0;
      const encodeMs = dFrames > 0 ? ((top.totalEncodeTime - p.totalEncodeTime) / dFrames) * 1000 : NaN;
      layers.forEach((r) => prev.set(r.id, r));

      const heights = layers
        .map((r) => r.frameHeight || 0)
        .filter(Boolean)
        .sort((a, b) => b - a)
        .map((h) => `${h}p`)
        .join(', ');

      const reasons = { none: 'nada', cpu: 'processador', bandwidth: 'internet', other: 'outro' };

      onUpdate([
        ['Resolução', `${top.frameWidth || '?'}x${top.frameHeight || '?'}`],
        ['Frames', `${Math.round(top.framesPerSecond || 0)} fps`],
        ['Codec', codecLabel(report, top.codecId)],
        ['Bitrate', fmtBitrate(totalBps)],
        ['Codificação', accelLabel(top.powerEfficientEncoder)],
        ['Encoder', top.encoderImplementation || '?'],
        ['Por frame', fmtMs(encodeMs)],
        ['Ping', fmtMs(findRtt(report))],
        ['Limite', reasons[top.qualityLimitationReason] || top.qualityLimitationReason || 'nada'],
        ['Camadas', heights || '-'],
      ]);
    } catch (_) { /* ignora */ }
  }, 1000);
  return () => clearInterval(id);
}
