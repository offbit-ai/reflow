// reflow-stats: Live stream throughput counters
// No config — pure visualization.

class ReflowStats extends ReflowUI.ReflowComponent {
  get styles() {
    const T = ReflowUI.theme;
    return `
      .rf-grid { grid-template-columns: 1fr 1fr; }
      .value.active { color: ${T.amber}; }
    `;
  }

  get template() {
    return `
      <div class="rf-grid">
        <span class="label">${ReflowUI.icon('upload', 12)} Bytes</span><span class="value" id="bytes">0</span>
        <span class="label">${ReflowUI.icon('layers', 12)} Frames</span><span class="value" id="frames">0</span>
        <span class="label">${ReflowUI.icon('clock', 12)} Time</span><span class="value" id="time">0s</span>
        <span class="label">${ReflowUI.icon('zap', 12)} Rate</span><span class="value" id="rate">0 B/s</span>
      </div>
    `;
  }

  onConnect() {
    this._bytes = 0;
    this._frames = 0;
    this._startTime = null;

    this.sub(() => this.zeal?.onStreamFrame((payload) => {
      if (!this._startTime) this._startTime = performance.now();
      this._bytes += payload.byteLength;
      this._frames++;
      this._update();
    }));

    this.sub(() => this.zeal?.onStreamStateChange((state) => {
      if (state.phase === 'complete') {
        this._update();
        this.shadowRoot.querySelectorAll('.value').forEach(el => el.classList.remove('active'));
      }
    }));
  }

  _update() {
    const elapsed = this._startTime ? (performance.now() - this._startTime) / 1000 : 0;
    const rate = elapsed > 0 ? this._bytes / elapsed : 0;
    this.$('bytes').textContent = ReflowUI.formatBytes(this._bytes);
    this.$('frames').textContent = this._frames.toLocaleString();
    this.$('time').textContent = elapsed.toFixed(1) + 's';
    this.$('rate').textContent = ReflowUI.formatBytes(rate) + '/s';
    this.shadowRoot.querySelectorAll('.value').forEach(el => el.classList.add('active'));
  }
}

customElements.define('reflow-stats', ReflowStats);
