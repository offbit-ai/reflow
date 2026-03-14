// reflow-spectrum: Real-time FFT spectrum analyzer
// Config: fftSize, hopSize, sampleRate

class ReflowSpectrum extends ReflowUI.ReflowComponent {
  get styles() { return `canvas { height: 120px; }` }

  get template() {
    return `<canvas></canvas><div class="rf-info"></div>`;
  }

  onConnect() {
    const c = this.$q('canvas');
    const { ctx, width, height } = ReflowUI.canvas.setupHiDPI(c, 120);
    this._ctx = ctx; this._w = width; this._h = height;
    this._peaks = null;
    this._peakDecay = 0.98;
    this._frames = 0;

    this.sub(() => this.zeal?.onStreamFrame((payload) => {
      const data = new Float32Array(payload.buffer, payload.byteOffset, payload.byteLength / 4);
      this._renderBins(data);
    }));

    this.sub(() => this.zeal?.onStreamStateChange((state) => {
      if (state.phase === 'complete') {
        this.$q('.rf-info').textContent = `Done \u2014 ${this._frames} frames`;
      }
    }));
  }

  _renderBins(data) {
    const binCount = data.length;
    if (!this._peaks) this._peaks = new Float32Array(binCount);
    this._frames++;

    const ctx = this._ctx, w = this._w, h = this._h;
    const barW = Math.max(1, w / binCount);
    const T = ReflowUI.theme;
    ctx.clearRect(0, 0, w, h);

    for (let i = 0; i < binCount; i++) {
      const db = data[i] > 0 ? 20 * Math.log10(data[i]) : -80;
      const norm = Math.max(0, Math.min(1, (db + 80) / 80));
      const barH = norm * h;

      if (norm > this._peaks[i]) this._peaks[i] = norm;
      else this._peaks[i] *= this._peakDecay;

      const hue = 120 - norm * 120;
      ctx.fillStyle = `hsl(${hue}, 80%, ${40 + norm * 20}%)`;
      ctx.fillRect(i * barW, h - barH, barW - 0.5, barH);

      ctx.fillStyle = T.textPrimary;
      ctx.fillRect(i * barW, h - this._peaks[i] * h, barW - 0.5, 1);
    }

    this.$q('.rf-info').textContent = `${binCount} bins \u2022 frame ${this._frames}`;
  }

  set fftSize(v) {}
}

customElements.define('reflow-spectrum', ReflowSpectrum);
