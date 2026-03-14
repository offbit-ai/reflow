// reflow-ir: Impulse response waveform display
// Used by ConvolveActor — shows convolution output waveform.

class ReflowIR extends ReflowUI.ReflowComponent {
  get styles() { return `canvas { height: 50px; }` }

  get template() {
    return `<canvas id="cv"></canvas><div class="rf-info">No impulse response loaded</div>`;
  }

  onConnect() {
    const { ctx, width, height } = ReflowUI.canvas.setupHiDPI(this.$('cv'), 50);
    this._ctx = ctx; this._w = width; this._h = height;

    this.sub(() => this.zeal?.onStreamFrame((payload) => {
      const samples = new Float32Array(payload.buffer, payload.byteOffset, payload.byteLength / 4);
      const ctx = this._ctx, w = this._w, h = this._h, mid = h / 2, T = ReflowUI.theme;
      ctx.clearRect(0, 0, w, h);

      ctx.strokeStyle = T.purple;
      ctx.lineWidth = 1;
      ctx.beginPath();
      const step = Math.max(1, Math.floor(samples.length / w));
      for (let px = 0; px < w; px++) {
        const idx = Math.min(px * step, samples.length - 1);
        const y = mid - samples[idx] * mid * 0.9;
        if (px === 0) ctx.moveTo(px, y); else ctx.lineTo(px, y);
      }
      ctx.stroke();
      this.$q('.rf-info').textContent = `${samples.length} samples`;
    }));

    this.sub(() => this.zeal?.onStreamStateChange((state) => {
      if (state.phase === 'complete') {
        this.$q('.rf-info').textContent = `Complete \u2022 ${ReflowUI.formatBytes(state.bytesReceived || 0)}`;
      }
    }));
  }
}

customElements.define('reflow-ir', ReflowIR);
