// reflow-ir: Impulse response waveform display for ConvolveActor
// Shows the loaded IR waveform and its duration.

class ReflowIR extends HTMLElement {
  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
    this.shadowRoot.innerHTML = `
      <style>
        :host { display: flex; flex-direction: column; flex: 1 1 auto; min-width: 0; width: 100%; }
        canvas { width: 100%; height: 50px; border-radius: 4px; background: #0a0a0a; }
        .info { font: 9px monospace; color: #888; padding: 2px 4px; }
      </style>
      <canvas></canvas>
      <div class="info">No impulse response loaded</div>
    `;
    this._canvas = this.shadowRoot.querySelector('canvas');
    this._info = this.shadowRoot.querySelector('.info');
  }

  connectedCallback() {
    const dpr = window.devicePixelRatio || 1;
    this._canvas.width = this._canvas.offsetWidth * dpr;
    this._canvas.height = 50 * dpr;
    this._ctx = this._canvas.getContext('2d');
    this._ctx.scale(dpr, dpr);
    this._w = this._canvas.offsetWidth;
    this._h = 50;

    // Stream frames show the convolution output
    this._unsub = this.zeal?.onStreamFrame((payload) => {
      this._drawWaveform(payload);
    });

    this._unsubState = this.zeal?.onStreamStateChange((state) => {
      if (state.phase === 'complete') {
        this._info.textContent = `Convolution complete \u2022 ${this._fmt(state.bytesReceived || 0)}`;
      }
    });
  }

  disconnectedCallback() { this._unsub?.(); this._unsubState?.(); }

  _drawWaveform(payload) {
    const samples = new Float32Array(payload.buffer, payload.byteOffset, payload.byteLength / 4);
    const ctx = this._ctx;
    const w = this._w, h = this._h, mid = h / 2;
    ctx.clearRect(0, 0, w, h);

    ctx.strokeStyle = '#6366f1';
    ctx.lineWidth = 1;
    ctx.beginPath();
    const step = Math.max(1, Math.floor(samples.length / w));
    for (let px = 0; px < w; px++) {
      const idx = Math.min(px * step, samples.length - 1);
      const y = mid - samples[idx] * mid * 0.9;
      if (px === 0) ctx.moveTo(px, y); else ctx.lineTo(px, y);
    }
    ctx.stroke();

    this._info.textContent = `${samples.length} samples`;
  }

  _fmt(b) {
    if (b < 1024) return b + ' B';
    if (b < 1048576) return (b / 1024).toFixed(1) + ' KB';
    return (b / 1048576).toFixed(1) + ' MB';
  }
}

customElements.define('reflow-ir', ReflowIR);
