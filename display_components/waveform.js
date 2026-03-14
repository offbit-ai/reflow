// reflow-waveform: Scrolling audio waveform with threshold line
// Used by EnvelopeFollower, PeakDetect, SilenceDetect
// Editable: thresholdDb (drag), sensitivity (drag)

class ReflowWaveform extends ReflowUI.ReflowComponent {
  get styles() { return `canvas { height: 80px; cursor: ns-resize; }` }

  get template() {
    return `<canvas id="cv"></canvas><div class="rf-info"></div>`;
  }

  onConnect() {
    const { ctx, width, height } = ReflowUI.canvas.setupHiDPI(this.$('cv'), 80);
    this._ctx = ctx; this._w = width; this._h = height;
    this._buffer = new Float32Array(2048);
    this._writePos = 0;
    this._frames = 0;
    this._dragging = false;

    const props = this.getProps();
    this._thresholdDb = props.thresholdDb ?? -40;

    // Drag threshold line
    const cv = this.$('cv');
    cv.addEventListener('pointerdown', (e) => {
      this._dragging = true;
      cv.setPointerCapture(e.pointerId);
    });
    cv.addEventListener('pointermove', (e) => {
      if (!this._dragging) return;
      const rect = cv.getBoundingClientRect();
      const y = (e.clientY - rect.top) / rect.height;
      // Map y position to threshold: center=0dB, edges=-60dB
      const linear = 1 - Math.abs(y - 0.5) * 2;
      this._thresholdDb = Math.round(20 * Math.log10(Math.max(0.001, linear)));
      this.zeal?.setProperty('thresholdDb', this._thresholdDb);
      this._draw();
    });
    cv.addEventListener('pointerup', () => { this._dragging = false; });

    this.sub(() => this.zeal?.onPropertyChange((values) => {
      if (values.thresholdDb !== undefined && !this._dragging) {
        this._thresholdDb = values.thresholdDb;
        this._draw();
      }
    }));

    this.sub(() => this.zeal?.onStreamFrame((payload) => {
      const samples = new Float32Array(payload.buffer, payload.byteOffset, payload.byteLength / 4);
      const step = Math.max(1, Math.floor(samples.length / 32));
      for (let i = 0; i < samples.length; i += step) {
        this._buffer[this._writePos % this._buffer.length] = samples[i];
        this._writePos++;
      }
      this._frames++;
      this._draw();
    }));
  }

  _draw() {
    const ctx = this._ctx, w = this._w, h = this._h, mid = h / 2, T = ReflowUI.theme;
    ctx.clearRect(0, 0, w, h);

    // Center line
    ctx.strokeStyle = T.border;
    ctx.lineWidth = 0.5;
    ctx.beginPath(); ctx.moveTo(0, mid); ctx.lineTo(w, mid); ctx.stroke();

    // Threshold lines (draggable)
    const threshLinear = Math.pow(10, this._thresholdDb / 20);
    const threshY = threshLinear * mid;
    ctx.strokeStyle = 'rgba(239, 68, 68, 0.4)';
    ctx.lineWidth = 1;
    ctx.setLineDash([2, 2]);
    ctx.beginPath(); ctx.moveTo(0, mid - threshY); ctx.lineTo(w, mid - threshY); ctx.stroke();
    ctx.beginPath(); ctx.moveTo(0, mid + threshY); ctx.lineTo(w, mid + threshY); ctx.stroke();
    ctx.setLineDash([]);

    // Waveform
    const len = Math.min(this._writePos, this._buffer.length);
    const start = this._writePos >= this._buffer.length ? this._writePos % this._buffer.length : 0;

    ctx.strokeStyle = T.green;
    ctx.lineWidth = 1;
    ctx.beginPath();
    for (let px = 0; px < w; px++) {
      const idx = (start + Math.floor(px * len / w)) % this._buffer.length;
      const y = mid - this._buffer[idx] * mid * 0.9;
      if (px === 0) ctx.moveTo(px, y); else ctx.lineTo(px, y);
    }
    ctx.stroke();

    // Fill
    ctx.lineTo(w, mid); ctx.lineTo(0, mid); ctx.closePath();
    ctx.fillStyle = 'rgba(34, 197, 94, 0.05)';
    ctx.fill();

    this.$q('.rf-info').textContent = `Frame ${this._frames} \u2022 Threshold: ${this._thresholdDb} dB`;
  }

  set thresholdDb(v) { this._thresholdDb = v; this._draw?.(); }
  set sensitivity(v) { /* reflected via observedProps */ }
}

customElements.define('reflow-waveform', ReflowWaveform);
