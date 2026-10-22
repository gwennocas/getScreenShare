// O H.264 High é anunciado pelo Chromium quando há um encoder compatível.
// Na RX 6600, High usa o encoder AMD; Constrained Baseline usa OpenH264.
function h264CodecPreferences(codecs) {
  const isH264 = (codec) => codec.mimeType.toLowerCase() === 'video/h264';
  const isHigh = (codec) => isH264(codec)
    && /(?:^|;)profile-level-id=64[0-9a-f]{4}(?:;|$)/i.test(codec.sdpFmtpLine || '')
    && /(?:^|;)packetization-mode=1(?:;|$)/i.test(codec.sdpFmtpLine || '');
  if (!codecs.some(isHigh)) return null;
  // Preserva os outros perfis e codecs para negociação com servidores antigos.
  return [
    ...codecs.filter(isHigh),
    ...codecs.filter((codec) => isH264(codec) && !isHigh(codec)),
    ...codecs.filter((codec) => !isH264(codec)),
  ];
}

function installH264ProfilePreference(room, readSettings) {
  // LocalSenderCreated acontece após addTransceiver e antes da oferta do SDK.
  // O acesso ao transporte é interno ao LiveKit 2.23.0 e fica isolado aqui.
  room.localParticipant.on(LivekitClient.ParticipantEvent.LocalSenderCreated, (sender, track) => {
    if (track.kind !== 'video' || readSettings().codec !== 'h264') return;
    try {
      const transceiver = room.engine?.pcManager?.publisher?.getTransceivers()
        .find((item) => item.sender === sender);
      const codecs = RTCRtpSender.getCapabilities('video')?.codecs || [];
      const preferences = h264CodecPreferences(codecs);
      if (!transceiver || !preferences) return;
      transceiver.setCodecPreferences(preferences);
      console.info('Preferência H.264:', preferences[0].sdpFmtpLine);
    } catch (error) {
      // Mantém a negociação padrão se o runtime ou o SDK não aceitar a preferência.
      console.warn('Não foi possível priorizar H.264 High:', error);
    }
  });
}
