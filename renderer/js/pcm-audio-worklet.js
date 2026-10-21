'use strict';

// Recebe buffers interleaved float32 stereo de 48 kHz via MessagePort e os
// converte para canais planar do AudioWorklet. Mantém a fila limitada para
// que um renderer lento não acumule latência indefinidamente.
class PcmStreamProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this.queue = [];
    this.current = null;
    this.frameOffset = 0;
    this.maxQueuedChunks = 30; // ~300 ms de PCM a 48 kHz, 480 frames por bloco
    this.port.onmessage = ({ data }) => {
      let bytes;
      if (data instanceof ArrayBuffer) {
        bytes = data;
      } else if (ArrayBuffer.isView(data)) {
        bytes = data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength);
      } else {
        return;
      }
      if (bytes.byteLength < 8 || bytes.byteLength % 8 !== 0) return;
      const samples = new Float32Array(bytes);
      if (this.queue.length >= this.maxQueuedChunks) this.queue.shift();
      this.queue.push(samples);
    };
  }

  process(_inputs, outputs) {
    const output = outputs[0];
    if (!output || output.length < 2) return true;
    const left = output[0];
    const right = output[1];

    for (let i = 0; i < left.length; i++) {
      if (!this.current || this.frameOffset >= this.current.length / 2) {
        this.current = this.queue.shift() || null;
        this.frameOffset = 0;
      }
      if (this.current) {
        const index = this.frameOffset * 2;
        left[i] = this.current[index];
        right[i] = this.current[index + 1];
        this.frameOffset++;
      } else {
        left[i] = 0;
        right[i] = 0;
      }
    }
    return true;
  }
}

registerProcessor('pcm-stream-processor', PcmStreamProcessor);
