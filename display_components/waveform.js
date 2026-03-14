// reflow-waveform: Audio waveform display with peak/silence markers
// Used by EnvelopeFollower, PeakDetect, SilenceDetect.
// Renders a scrolling waveform with event overlays.

class ReflowWaveform extends HTMLElement {
  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
    this.shadowRoot.innerHTML = `
      <style>
        :host { display: flex; flex-direction: column; flex: 1 1 auto; min-width: 0; width: 100%; }
        canvas { width: 100%; height: 80px; border-radius: 4px; background: #0a0a0a; }
        .info { font: 9px monospace; color: #888; padding: 2px 4px; }
      </style>
      <canvas></canvas>
      <div class="info"></div>
    `;
    this._canvas = this.shadowRoot.querySelector('canvas');
    this._info = this.shadowRoot.querySelector('.info');
    this._buffer = new Float32Array(2048);
    this._writePos = 0;
    this._peaks = [];
    this._silenceRegions = [];
    this._isSilent = false;
    this._frames = 0;
  }

  connectedCallback() {
    const dpr = window.devicePixelRatio || 1;
    this._canvas.width = this._canvas.offsetWidth * dpr;
    this._canvas.height = 80 * dpr;
    this._ctx = this._canvas.getContext('2d');
    this._ctx.scale(dpr, dpr);
    this._w = this._canvas.offsetWidth;
    this._h = 80;

    this._unsub = this.zeal?.onStreamFrame((payload) => {
      const samples = new Float32Array(payload.buffer, payload.byteOffset, payload.byteLength / 4);
      // Downsample into ring buffer
      const step = Math.max(1, Math.floor(samples.length / 32));
      for (let i = 0; i < samples.length; i += step) {
        this._buffer[this._writePos % this._buffer.length] = samples[i];
        this._writePos++;
      }
      this._frames++;
      this._draw();
    });

    // Listen for events from PeakDetect/SilenceDetect
    this._unsubProps = this.zeal?.onPropertyChange((values) => {
      // Events arrive as property updates from the actor
    });
  }

  disconnectedCallback() {
    this._unsub?.();
    this._unsubProps?.();
  }

  _draw() {
    const ctx = this._ctx;
    const w = this._w;
    const h = this._h;
    const mid = h / 2;
    ctx.clearRect(0, 0, w, h);

    // Center line
    ctx.strokeStyle = '#222';
    ctx.lineWidth = 0.5;
    ctx.beginPath(); ctx.moveTo(0, mid); ctx.lineTo(w, mid); ctx.stroke();

    // Threshold lines (if configured)
    const thresholdDb = parseFloat(this.getAttribute('threshold-db') || '-40');
    const threshLinear = Math.pow(10, thresholdDb / 20);
    const threshY = threshLinear * mid;
    ctx.strokeStyle = 'rgba(239, 68, 68, 0.3)';
    ctx.lineWidth = 0.5;
    ctx.setLineDash([2, 2]);
    ctx.beginPath(); ctx.moveTo(0, mid - threshY); ctx.lineTo(w, mid - threshY); ctx.stroke();
    ctx.beginPath(); ctx.moveTo(0, mid + threshY); ctx.lineTo(w, mid + threshY); ctx.stroke();
    ctx.setLineDash([]);

    // Waveform
    const len = Math.min(this._writePos, this._buffer.length);
    const start = this._writePos >= this._buffer.length ? this._writePos % this._buffer.length : 0;
    const samplesPerPixel = Math.max(1, Math.floor(len / w));

    ctx.strokeStyle = '#22c55e';
    ctx.lineWidth = 1;
    ctx.beginPath();

    for (let px = 0; px < w; px++) {
      const idx = (start + Math.floor(px * len / w)) % this._buffer.length;
      const val = this._buffer[idx];
      const y = mid - val * mid * 0.9;
      if (px === 0) ctx.moveTo(px, y); else ctx.lineTo(px, y);
    }
    ctx.stroke();

    // Fill
    ctx.lineTo(w, mid);
    ctx.lineTo(0, mid);
    ctx.closePath();
    ctx.fillStyle = 'rgba(34, 197, 94, 0.05)';
    ctx.fill();

    this._info.textContent = `Frame ${this._frames}`;
  }

  set thresholdDb(v) { this.setAttribute('threshold-db', v); this._draw?.(); }
  set sensitivity(v) { /* for peak detect */ }
}

customElements.define('reflow-waveform', ReflowWaveform);
