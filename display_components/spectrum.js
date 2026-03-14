// reflow-spectrum: Real-time FFT spectrum analyzer display
// Renders frequency magnitude bins as a bar chart with peak hold.
// Editable: fftSize, hopSize via property panel integration.

class ReflowSpectrum extends HTMLElement {
  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
    this.shadowRoot.innerHTML = `
      <style>
        :host { display: flex; flex-direction: column; flex: 1 1 auto; min-width: 0; width: 100%; }
        canvas { width: 100%; height: 120px; border-radius: 4px; background: #0a0a0a; }
        .info { font: 10px monospace; color: #888; padding: 2px 4px; }
      </style>
      <canvas></canvas>
      <div class="info"></div>
    `;
    this._canvas = this.shadowRoot.querySelector('canvas');
    this._info = this.shadowRoot.querySelector('.info');
    this._ctx = null;
    this._peaks = null;
    this._peakDecay = 0.98;
    this._frames = 0;
  }

  connectedCallback() {
    const canvas = this._canvas;
    canvas.width = canvas.offsetWidth * (window.devicePixelRatio || 1);
    canvas.height = 120 * (window.devicePixelRatio || 1);
    this._ctx = canvas.getContext('2d');
    this._ctx.scale(window.devicePixelRatio || 1, window.devicePixelRatio || 1);

    this._unsub = this.zeal?.onStreamFrame((payload, meta) => {
      this._renderBins(payload);
    });

    this._unsubState = this.zeal?.onStreamStateChange((state) => {
      if (state.phase === 'complete') {
        this._info.textContent = `Done \u2014 ${this._frames} frames`;
      }
    });
  }

  disconnectedCallback() {
    this._unsub?.();
    this._unsubState?.();
  }

  _renderBins(payload) {
    const data = new Float32Array(payload.buffer, payload.byteOffset, payload.byteLength / 4);
    const binCount = data.length;
    if (!this._peaks) this._peaks = new Float32Array(binCount);

    this._frames++;
    const ctx = this._ctx;
    const w = this._canvas.offsetWidth;
    const h = 120;
    const barW = Math.max(1, w / binCount);

    ctx.clearRect(0, 0, w, h);

    // Convert to dB, normalize
    const minDb = -80;
    const maxDb = 0;
    const range = maxDb - minDb;

    for (let i = 0; i < binCount; i++) {
      const mag = data[i];
      const db = mag > 0 ? 20 * Math.log10(mag) : minDb;
      const norm = Math.max(0, Math.min(1, (db - minDb) / range));
      const barH = norm * h;

      // Peak hold
      if (norm > this._peaks[i]) {
        this._peaks[i] = norm;
      } else {
        this._peaks[i] *= this._peakDecay;
      }

      // Bar gradient: green → yellow → red
      const hue = 120 - norm * 120;
      ctx.fillStyle = `hsl(${hue}, 80%, ${40 + norm * 20}%)`;
      ctx.fillRect(i * barW, h - barH, barW - 0.5, barH);

      // Peak marker
      const peakY = h - this._peaks[i] * h;
      ctx.fillStyle = '#fff';
      ctx.fillRect(i * barW, peakY, barW - 0.5, 1);
    }

    this._info.textContent = `${binCount} bins \u2022 frame ${this._frames}`;
  }

  set fftSize(v) {
    this._info.textContent = `FFT: ${v}`;
  }
}

customElements.define('reflow-spectrum', ReflowSpectrum);
