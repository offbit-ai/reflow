// reflow-filter-response: Frequency response curve with draggable cutoff
// Config: filterType, frequency, q, gainDb, sampleRate

class ReflowFilterResponse extends ReflowUI.ReflowComponent {
  get styles() { return `canvas { height: 80px; cursor: ew-resize; }` }

  get template() {
    return `<canvas id="cv"></canvas><div class="rf-info"></div>`;
  }

  onConnect() {
    const { ctx, width, height } = ReflowUI.canvas.setupHiDPI(this.$('cv'), 80);
    this._ctx = ctx; this._w = width; this._h = height;
    this._dragging = false;

    const props = this.getProps();
    this._filterType = props.filterType || 'lowpass';
    this._frequency = props.frequency || 1000;
    this._q = props.q || 0.707;
    this._gainDb = props.gainDb || 0;

    // Drag cutoff frequency
    const cv = this.$('cv');
    cv.addEventListener('pointerdown', (e) => {
      this._dragging = true;
      cv.setPointerCapture(e.pointerId);
    });
    cv.addEventListener('pointermove', (e) => {
      if (!this._dragging) return;
      const rect = cv.getBoundingClientRect();
      this._frequency = Math.round(ReflowUI.canvas.xToFreq(e.clientX - rect.left, this._w));
      this._frequency = Math.max(20, Math.min(20000, this._frequency));
      this.zeal?.setProperty('frequency', this._frequency);
      this._draw();
    });
    cv.addEventListener('pointerup', () => { this._dragging = false; });

    this.sub(() => this.zeal?.onPropertyChange((values) => {
      if (values.filterType !== undefined) this._filterType = values.filterType;
      if (values.frequency !== undefined && !this._dragging) this._frequency = values.frequency;
      if (values.q !== undefined) this._q = values.q;
      if (values.gainDb !== undefined) this._gainDb = values.gainDb;
      this._draw();
    }));

    this._draw();
  }

  _draw() {
    const ctx = this._ctx, w = this._w, h = this._h, T = ReflowUI.theme, C = ReflowUI.canvas;
    ctx.clearRect(0, 0, w, h);
    C.drawFreqGrid(ctx, w, h);
    C.drawDbGrid(ctx, w, h, 12);

    // Response curve
    ctx.strokeStyle = T.green;
    ctx.lineWidth = 2;
    ctx.beginPath();
    for (let px = 0; px < w; px++) {
      const freq = C.xToFreq(px, w);
      const resp = this._biquadResponse(freq);
      const db = 20 * Math.log10(Math.max(0.001, resp));
      const y = Math.max(0, Math.min(h, C.dbToY(db, h)));
      if (px === 0) ctx.moveTo(px, y); else ctx.lineTo(px, y);
    }
    ctx.stroke();

    // Cutoff marker (draggable)
    const cutX = C.freqToX(this._frequency, w);
    ctx.strokeStyle = T.amber;
    ctx.lineWidth = 1;
    ctx.setLineDash([3, 3]);
    ctx.beginPath(); ctx.moveTo(cutX, 0); ctx.lineTo(cutX, h); ctx.stroke();
    ctx.setLineDash([]);

    // Handle
    ctx.beginPath();
    ctx.arc(cutX, h / 2, 4, 0, Math.PI * 2);
    ctx.fillStyle = T.amber;
    ctx.fill();

    this.$q('.rf-info').textContent = `${this._filterType} ${ReflowUI.formatFreq(this._frequency)} Q=${this._q}`;
  }

  _biquadResponse(freq) {
    const f0 = this._frequency, q = this._q, ratio = freq / f0;
    switch (this._filterType) {
      case 'lowpass': case 'lpf':
        return 1 / Math.sqrt(1 + Math.pow(ratio, 4) / (q * q));
      case 'highpass': case 'hpf':
        return ratio * ratio / Math.sqrt(1 + Math.pow(ratio, 4) / (q * q));
      case 'bandpass': case 'bpf':
        return (ratio / q) / Math.sqrt(Math.pow(1 - ratio * ratio, 2) + Math.pow(ratio / q, 2));
      case 'notch':
        return Math.abs(1 - ratio * ratio) / Math.sqrt(Math.pow(1 - ratio * ratio, 2) + Math.pow(ratio / q, 2));
      case 'peaking': case 'eq': {
        const A = Math.pow(10, this._gainDb / 40);
        return Math.sqrt(Math.pow(1 - ratio * ratio, 2) + Math.pow(A * ratio / q, 2))
             / Math.sqrt(Math.pow(1 - ratio * ratio, 2) + Math.pow(ratio / (A * q), 2));
      }
      case 'lowshelf': {
        const A = Math.pow(10, this._gainDb / 40);
        return ratio < 1 ? A : 1 + (A - 1) * Math.exp(-2 * (ratio - 1) * q);
      }
      case 'highshelf': {
        const A = Math.pow(10, this._gainDb / 40);
        return ratio > 1 ? A : 1 + (A - 1) * Math.exp(-2 * (1 - ratio) * q);
      }
      default: return 1;
    }
  }

  set filterType(v) { this._filterType = v; this._draw?.(); }
  set frequency(v) { this._frequency = v; this._draw?.(); }
  set q(v) { this._q = v; this._draw?.(); }
  set gainDb(v) { this._gainDb = v; this._draw?.(); }
}

customElements.define('reflow-filter-response', ReflowFilterResponse);
