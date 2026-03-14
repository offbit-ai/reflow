// reflow-dynamics: Gain reduction meter + editable transfer curve
// Used by Compressor, Limiter, Noise Gate, De-Esser.
// Editable: thresholdDb (drag horizontal line), ratio (drag curve slope)

class ReflowDynamics extends HTMLElement {
  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
    this.shadowRoot.innerHTML = `
      <style>
        :host { display: flex; flex-direction: column; flex: 1 1 auto; min-width: 0; width: 100%; }
        .container { display: flex; gap: 4px; }
        canvas { border-radius: 4px; background: #0a0a0a; }
        .curve { width: 100px; height: 100px; }
        .meter { width: 24px; height: 100px; }
        .labels { font: 9px monospace; color: #999; padding: 2px 0; }
        .labels span { display: block; }
        .gr { color: #f59e0b; }
      </style>
      <div class="container">
        <canvas class="curve"></canvas>
        <canvas class="meter"></canvas>
      </div>
      <div class="labels">
        <span class="gr">GR: 0.0 dB</span>
      </div>
    `;
    this._curve = this.shadowRoot.querySelector('.curve');
    this._meter = this.shadowRoot.querySelector('.meter');
    this._grLabel = this.shadowRoot.querySelector('.gr');
    this._threshold = -20;
    this._ratio = 4;
    this._knee = 6;
    this._gr = 0;
    this._dragging = false;
  }

  connectedCallback() {
    const dpr = window.devicePixelRatio || 1;
    this._curve.width = 100 * dpr;
    this._curve.height = 100 * dpr;
    this._meter.width = 24 * dpr;
    this._meter.height = 100 * dpr;

    this._curveCtx = this._curve.getContext('2d');
    this._meterCtx = this._meter.getContext('2d');
    this._curveCtx.scale(dpr, dpr);
    this._meterCtx.scale(dpr, dpr);

    this._drawCurve();

    // Make threshold draggable
    this._curve.addEventListener('pointerdown', (e) => {
      this._dragging = true;
      this._curve.setPointerCapture(e.pointerId);
    });
    this._curve.addEventListener('pointermove', (e) => {
      if (!this._dragging) return;
      const rect = this._curve.getBoundingClientRect();
      const y = (e.clientY - rect.top) / rect.height;
      // Map y (0=top=-0dB, 1=bottom=-60dB) to threshold
      const newThreshold = -(y * 60);
      this._threshold = Math.round(newThreshold);
      this.zeal?.setProperty('thresholdDb', this._threshold);
      this._drawCurve();
    });
    this._curve.addEventListener('pointerup', () => { this._dragging = false; });

    // Stream frames carry gain reduction info
    this._unsub = this.zeal?.onStreamFrame((payload) => {
      // Compute RMS of output to estimate GR
      const samples = new Float32Array(payload.buffer, payload.byteOffset, payload.byteLength / 4);
      let peak = 0;
      for (let i = 0; i < samples.length; i++) {
        const abs = Math.abs(samples[i]);
        if (abs > peak) peak = abs;
      }
      const peakDb = peak > 0 ? 20 * Math.log10(peak) : -60;
      this._gr = Math.min(0, peakDb - this._threshold);
      this._drawMeter();
    });

    this._unsubProps = this.zeal?.onPropertyChange((values) => {
      if (values.thresholdDb !== undefined) this._threshold = values.thresholdDb;
      if (values.ratio !== undefined) this._ratio = values.ratio;
      if (values.kneeDb !== undefined) this._knee = values.kneeDb;
      this._drawCurve();
    });
  }

  disconnectedCallback() {
    this._unsub?.();
    this._unsubProps?.();
  }

  _drawCurve() {
    const ctx = this._curveCtx;
    const s = 100;
    ctx.clearRect(0, 0, s, s);

    // Grid
    ctx.strokeStyle = '#222';
    ctx.lineWidth = 0.5;
    for (let i = 0; i <= 4; i++) {
      const p = (i / 4) * s;
      ctx.beginPath(); ctx.moveTo(p, 0); ctx.lineTo(p, s); ctx.stroke();
      ctx.beginPath(); ctx.moveTo(0, p); ctx.lineTo(s, p); ctx.stroke();
    }

    // Unity line
    ctx.strokeStyle = '#333';
    ctx.lineWidth = 1;
    ctx.beginPath(); ctx.moveTo(0, s); ctx.lineTo(s, 0); ctx.stroke();

    // Transfer curve
    ctx.strokeStyle = '#22c55e';
    ctx.lineWidth = 2;
    ctx.beginPath();
    for (let x = 0; x < s; x++) {
      const inputDb = -60 + (x / s) * 60;
      let outputDb;
      if (inputDb < this._threshold) {
        outputDb = inputDb;
      } else {
        outputDb = this._threshold + (inputDb - this._threshold) / this._ratio;
      }
      const y = s - ((outputDb + 60) / 60) * s;
      if (x === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y);
    }
    ctx.stroke();

    // Threshold line
    const threshY = s - ((this._threshold + 60) / 60) * s;
    ctx.strokeStyle = '#f59e0b';
    ctx.lineWidth = 1;
    ctx.setLineDash([3, 3]);
    ctx.beginPath(); ctx.moveTo(0, threshY); ctx.lineTo(s, threshY); ctx.stroke();
    ctx.setLineDash([]);
  }

  _drawMeter() {
    const ctx = this._meterCtx;
    const w = 24, h = 100;
    ctx.clearRect(0, 0, w, h);

    // Background
    ctx.fillStyle = '#111';
    ctx.fillRect(0, 0, w, h);

    // GR bar (grows downward from top)
    const grNorm = Math.min(1, Math.abs(this._gr) / 30);
    const barH = grNorm * h;
    ctx.fillStyle = this._gr < -6 ? '#ef4444' : '#f59e0b';
    ctx.fillRect(2, 0, w - 4, barH);

    this._grLabel.textContent = `GR: ${this._gr.toFixed(1)} dB`;
  }

  set thresholdDb(v) { this._threshold = v; this._drawCurve?.(); }
  set ratio(v) { this._ratio = v; this._drawCurve?.(); }
  set kneeDb(v) { this._knee = v; this._drawCurve?.(); }
}

customElements.define('reflow-dynamics', ReflowDynamics);
