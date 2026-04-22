// reflow-dynamics: Transfer curve + GR meter with draggable threshold
// Config: thresholdDb, ratio, kneeDb, attackMs, releaseMs, makeupDb, ceilingDb
// Used by: Compressor, Limiter, NoiseGate, DeEsser

class ReflowDynamics extends ReflowUI.ReflowComponent {
  get styles() {
    const T = ReflowUI.theme;
    return `
      .container { display: flex; gap: 4px; padding: ${T.pad}; }
      .curve { width: 100px; height: 100px; cursor: ns-resize; }
      .meter { width: 20px; height: 100px; }
      .gr-label { font: ${T.fontSmall}; color: ${T.amber}; padding: ${T.padSmall} ${T.pad}; }
    `;
  }

  get template() {
    return `
      <div class="container">
        <canvas class="curve" id="curve"></canvas>
        <canvas class="meter" id="meter"></canvas>
      </div>
      <div class="gr-label" id="gr">GR: 0.0 dB</div>
    `;
  }

  onConnect() {
    const T = ReflowUI.theme;
    const props = this.getProps();
    this._threshold = props.thresholdDb ?? props.ceilingDb ?? -20;
    this._ratio = props.ratio ?? Infinity;
    this._knee = props.kneeDb ?? 0;
    this._gr = 0;
    this._dragging = false;

    const { ctx: curveCtx } = ReflowUI.canvas.setupHiDPI(this.$('curve'), 100);
    const { ctx: meterCtx } = ReflowUI.canvas.setupHiDPI(this.$('meter'), 100);
    this._curveCtx = curveCtx;
    this._meterCtx = meterCtx;
    this._drawCurve();

    // Drag threshold
    const curve = this.$('curve');
    curve.addEventListener('pointerdown', (e) => {
      this._dragging = true;
      curve.setPointerCapture(e.pointerId);
    });
    curve.addEventListener('pointermove', (e) => {
      if (!this._dragging) return;
      const rect = curve.getBoundingClientRect();
      const y = (e.clientY - rect.top) / rect.height;
      this._threshold = Math.round(-(y * 60));
      this._threshold = Math.max(-60, Math.min(0, this._threshold));
      this.zeal?.setProperty('thresholdDb', this._threshold);
      this._drawCurve();
    });
    curve.addEventListener('pointerup', () => { this._dragging = false; });

    this.sub(() => this.zeal?.onStreamFrame((payload) => {
      const samples = new Float32Array(payload.buffer, payload.byteOffset, payload.byteLength / 4);
      let peak = 0;
      for (let i = 0; i < samples.length; i++) {
        const a = Math.abs(samples[i]);
        if (a > peak) peak = a;
      }
      const peakDb = peak > 0 ? 20 * Math.log10(peak) : -60;
      this._gr = Math.min(0, peakDb - this._threshold);
      this._drawMeter();
    }));

    this.sub(() => this.zeal?.onPropertyChange((values) => {
      if (values.thresholdDb !== undefined && !this._dragging) this._threshold = values.thresholdDb;
      if (values.ceilingDb !== undefined && !this._dragging) this._threshold = values.ceilingDb;
      if (values.ratio !== undefined) this._ratio = values.ratio;
      if (values.kneeDb !== undefined) this._knee = values.kneeDb;
      this._drawCurve();
    }));
  }

  _drawCurve() {
    const ctx = this._curveCtx, s = 100, T = ReflowUI.theme;
    ctx.clearRect(0, 0, s, s);

    // Grid
    ctx.strokeStyle = T.bgElevated;
    ctx.lineWidth = 0.5;
    for (let i = 1; i <= 3; i++) {
      const p = (i / 4) * s;
      ctx.beginPath(); ctx.moveTo(p, 0); ctx.lineTo(p, s); ctx.stroke();
      ctx.beginPath(); ctx.moveTo(0, p); ctx.lineTo(s, p); ctx.stroke();
    }

    // Unity
    ctx.strokeStyle = T.borderLight;
    ctx.lineWidth = 1;
    ctx.beginPath(); ctx.moveTo(0, s); ctx.lineTo(s, 0); ctx.stroke();

    // Transfer curve
    ctx.strokeStyle = T.green;
    ctx.lineWidth = 2;
    ctx.beginPath();
    const ratio = isFinite(this._ratio) ? this._ratio : 1000;
    for (let x = 0; x < s; x++) {
      const inputDb = -60 + (x / s) * 60;
      const outputDb = inputDb < this._threshold
        ? inputDb
        : this._threshold + (inputDb - this._threshold) / ratio;
      const y = s - ((outputDb + 60) / 60) * s;
      if (x === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y);
    }
    ctx.stroke();

    // Threshold line
    const threshY = s - ((this._threshold + 60) / 60) * s;
    ctx.strokeStyle = T.amber;
    ctx.lineWidth = 1;
    ctx.setLineDash([3, 3]);
    ctx.beginPath(); ctx.moveTo(0, threshY); ctx.lineTo(s, threshY); ctx.stroke();
    ctx.setLineDash([]);

    // Threshold handle
    ctx.beginPath();
    ctx.arc(s / 2, threshY, 4, 0, Math.PI * 2);
    ctx.fillStyle = T.amber;
    ctx.fill();
  }

  _drawMeter() {
    const ctx = this._meterCtx, w = 20, h = 100, T = ReflowUI.theme;
    ctx.clearRect(0, 0, w, h);
    const grNorm = Math.min(1, Math.abs(this._gr) / 30);
    ReflowUI.drawMeter(ctx, 2, 0, w - 4, h, grNorm, this._gr < -6 ? T.red : T.amber);
    this.$('gr').textContent = `GR: ${this._gr.toFixed(1)} dB`;
  }

  set thresholdDb(v) { this._threshold = v; this._drawCurve?.(); }
  set ceilingDb(v) { this._threshold = v; this._drawCurve?.(); }
  set ratio(v) { this._ratio = v; this._drawCurve?.(); }
  set kneeDb(v) { this._knee = v; this._drawCurve?.(); }
}

customElements.define('reflow-dynamics', ReflowDynamics);
