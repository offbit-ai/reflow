// reflow-eq: Interactive multi-band parametric EQ
// Config: bands (JSON array), sampleRate
// Drag handles to edit freq/gain, scroll to adjust Q, double-click to add band.

class ReflowEq extends ReflowUI.ReflowComponent {
  get styles() { return `canvas { height: 140px; cursor: crosshair; }` }

  get template() {
    return `<canvas id="cv"></canvas><div class="rf-info">Drag bands to adjust. Scroll to change Q.</div>`;
  }

  onConnect() {
    const { ctx, width, height } = ReflowUI.canvas.setupHiDPI(this.$('cv'), 140);
    this._ctx = ctx; this._w = width; this._h = height;
    this._bands = [];
    this._dragIdx = -1;

    const props = this.getProps();
    if (props.bands) this._bands = Array.isArray(props.bands) ? props.bands : JSON.parse(props.bands);

    const cv = this.$('cv');
    cv.addEventListener('pointerdown', (e) => this._onDown(e));
    cv.addEventListener('pointermove', (e) => this._onMove(e));
    cv.addEventListener('pointerup', () => this._onUp());
    cv.addEventListener('wheel', (e) => this._onWheel(e), { passive: false });
    cv.addEventListener('dblclick', (e) => this._addBand(e));

    this.sub(() => this.zeal?.onPropertyChange((values) => {
      if (values.bands) {
        this._bands = Array.isArray(values.bands) ? values.bands : JSON.parse(values.bands);
        this._draw();
      }
    }));

    this._draw();
  }

  _draw() {
    const ctx = this._ctx, w = this._w, h = this._h, T = ReflowUI.theme, C = ReflowUI.canvas;
    ctx.clearRect(0, 0, w, h);
    C.drawFreqGrid(ctx, w, h);
    C.drawDbGrid(ctx, w, h, 12);

    // Frequency response curve
    ctx.strokeStyle = T.green;
    ctx.lineWidth = 2;
    ctx.beginPath();
    for (let px = 0; px < w; px++) {
      const freq = C.xToFreq(px, w);
      let totalGain = 0;
      for (const band of this._bands) {
        const f0 = band.frequency || 1000;
        const gain = band.gain || 0;
        const q = band.q || 1;
        const logRatio = Math.log2(freq / f0);
        totalGain += gain * Math.exp(-0.5 * Math.pow(logRatio * q * 2, 2));
      }
      const y = C.dbToY(totalGain, h);
      if (px === 0) ctx.moveTo(px, y); else ctx.lineTo(px, y);
    }
    ctx.stroke();

    // Fill under curve
    ctx.lineTo(w, h / 2);
    ctx.lineTo(0, h / 2);
    ctx.closePath();
    ctx.fillStyle = 'rgba(34, 197, 94, 0.08)';
    ctx.fill();

    // Band handles
    for (let i = 0; i < this._bands.length; i++) {
      const band = this._bands[i];
      const x = C.freqToX(band.frequency || 1000, w);
      const y = C.dbToY(band.gain || 0, h);
      const r = this._dragIdx === i ? 7 : 5;

      ctx.beginPath();
      ctx.arc(x, y, r, 0, Math.PI * 2);
      ctx.fillStyle = this._dragIdx === i ? T.textPrimary : T.green;
      ctx.fill();
      ctx.strokeStyle = T.bg;
      ctx.lineWidth = 1;
      ctx.stroke();
    }
  }

  _hitTest(e) {
    const rect = this.$('cv').getBoundingClientRect();
    const mx = e.clientX - rect.left, my = e.clientY - rect.top;
    const C = ReflowUI.canvas;
    for (let i = 0; i < this._bands.length; i++) {
      const x = C.freqToX(this._bands[i].frequency || 1000, this._w);
      const y = C.dbToY(this._bands[i].gain || 0, this._h);
      if (Math.hypot(mx - x, my - y) < 10) return i;
    }
    return -1;
  }

  _onDown(e) {
    this._dragIdx = this._hitTest(e);
    if (this._dragIdx >= 0) this.$('cv').setPointerCapture(e.pointerId);
  }

  _onMove(e) {
    if (this._dragIdx < 0) return;
    const rect = this.$('cv').getBoundingClientRect();
    const C = ReflowUI.canvas;
    const band = this._bands[this._dragIdx];
    band.frequency = Math.round(C.xToFreq(e.clientX - rect.left, this._w));
    band.gain = Math.round(C.yToDb(e.clientY - rect.top, this._h) * 10) / 10;
    this._draw();
    this.$q('.rf-info').textContent = `Band ${this._dragIdx + 1}: ${ReflowUI.formatFreq(band.frequency)}, ${ReflowUI.formatDb(band.gain)}, Q ${band.q}`;
  }

  _onUp() {
    if (this._dragIdx >= 0) {
      this.zeal?.setProperty('bands', JSON.stringify(this._bands));
      this._dragIdx = -1;
      this._draw();
    }
  }

  _onWheel(e) {
    const idx = this._hitTest(e);
    if (idx < 0) return;
    e.preventDefault();
    const band = this._bands[idx];
    band.q = Math.max(0.1, Math.min(30, (band.q || 1) + (e.deltaY > 0 ? -0.1 : 0.1)));
    band.q = Math.round(band.q * 10) / 10;
    this._draw();
    this.$q('.rf-info').textContent = `Band ${idx + 1}: Q = ${band.q}`;
    this.zeal?.setProperty('bands', JSON.stringify(this._bands));
  }

  _addBand(e) {
    const rect = this.$('cv').getBoundingClientRect();
    const freq = Math.round(ReflowUI.canvas.xToFreq(e.clientX - rect.left, this._w));
    this._bands.push({ type: 'peaking', frequency: freq, gain: 0, q: 1.0 });
    this.zeal?.setProperty('bands', JSON.stringify(this._bands));
    this._draw();
  }

  set bands(v) { this._bands = Array.isArray(v) ? v : JSON.parse(v || '[]'); this._draw?.(); }
}

customElements.define('reflow-eq', ReflowEq);
