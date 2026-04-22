// reflow-crossover: 3-band frequency display with draggable split points
// Config: lowFrequency, highFrequency, sampleRate

class ReflowCrossover extends ReflowUI.ReflowComponent {
  get styles() { return `canvas { height: 80px; cursor: col-resize; }` }

  get template() {
    return `<canvas id="cv"></canvas><div class="rf-info">Drag crossover points to adjust</div>`;
  }

  onConnect() {
    const { ctx, width, height } = ReflowUI.canvas.setupHiDPI(this.$('cv'), 80);
    this._ctx = ctx; this._w = width; this._h = height;

    const props = this.getProps();
    this._lowFreq = props.lowFrequency ?? 200;
    this._highFreq = props.highFrequency ?? 4000;
    this._dragging = null;

    const cv = this.$('cv');
    cv.addEventListener('pointerdown', (e) => this._onDown(e));
    cv.addEventListener('pointermove', (e) => this._onMove(e));
    cv.addEventListener('pointerup', () => { this._dragging = null; });

    this.sub(() => this.zeal?.onPropertyChange((values) => {
      if (values.lowFrequency !== undefined) this._lowFreq = values.lowFrequency;
      if (values.highFrequency !== undefined) this._highFreq = values.highFrequency;
      this._draw();
    }));

    this._draw();
  }

  _draw() {
    const ctx = this._ctx, w = this._w, h = this._h, T = ReflowUI.theme, C = ReflowUI.canvas;
    ctx.clearRect(0, 0, w, h);

    const lowX = C.freqToX(this._lowFreq, w);
    const highX = C.freqToX(this._highFreq, w);

    // Band fills
    ctx.fillStyle = 'rgba(239, 68, 68, 0.12)';
    ctx.fillRect(0, 0, lowX, h);
    ctx.fillStyle = 'rgba(34, 197, 94, 0.12)';
    ctx.fillRect(lowX, 0, highX - lowX, h);
    ctx.fillStyle = 'rgba(59, 130, 246, 0.12)';
    ctx.fillRect(highX, 0, w - highX, h);

    // Crossover lines + handles
    for (const [x] of [[lowX, this._lowFreq], [highX, this._highFreq]]) {
      ctx.strokeStyle = T.amber;
      ctx.lineWidth = 2;
      ctx.beginPath(); ctx.moveTo(x, 0); ctx.lineTo(x, h); ctx.stroke();

      ctx.beginPath();
      ctx.arc(x, h / 2, 5, 0, Math.PI * 2);
      ctx.fillStyle = T.amber;
      ctx.fill();
    }

    // Band labels
    ctx.fillStyle = T.textDim;
    ctx.font = ReflowUI.theme.fontSmall;
    ctx.fillText('Low', 4, 12);
    ctx.fillText('Mid', (lowX + highX) / 2 - 8, 12);
    ctx.fillText('High', highX + 4, 12);

    this.$q('.rf-info').textContent = `Low: ${ReflowUI.formatFreq(this._lowFreq)} | High: ${ReflowUI.formatFreq(this._highFreq)}`;
  }

  _onDown(e) {
    const rect = this.$('cv').getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const C = ReflowUI.canvas;
    const lowX = C.freqToX(this._lowFreq, this._w);
    const highX = C.freqToX(this._highFreq, this._w);
    if (Math.abs(mx - lowX) < 12) this._dragging = 'low';
    else if (Math.abs(mx - highX) < 12) this._dragging = 'high';
    if (this._dragging) this.$('cv').setPointerCapture(e.pointerId);
  }

  _onMove(e) {
    if (!this._dragging) return;
    const rect = this.$('cv').getBoundingClientRect();
    const freq = Math.round(ReflowUI.canvas.xToFreq(e.clientX - rect.left, this._w));
    if (this._dragging === 'low') {
      this._lowFreq = Math.max(20, Math.min(freq, this._highFreq - 100));
      this.zeal?.setProperty('lowFrequency', this._lowFreq);
    } else {
      this._highFreq = Math.max(this._lowFreq + 100, Math.min(freq, 20000));
      this.zeal?.setProperty('highFrequency', this._highFreq);
    }
    this._draw();
  }

  set lowFrequency(v) { this._lowFreq = v; this._draw?.(); }
  set highFrequency(v) { this._highFreq = v; this._draw?.(); }
}

customElements.define('reflow-crossover', ReflowCrossover);
