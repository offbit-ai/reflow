// reflow-buffer: Fill level gauge with flush counter
// Config: bufferBytes

class ReflowBuffer extends ReflowUI.ReflowComponent {
  get styles() {
    const T = ReflowUI.theme;
    return `
      .wrap { padding: ${T.pad}; }
      .bar-bg { height: 10px; background: ${T.bgSurface}; border-radius: ${T.radiusSmall}; overflow: hidden; margin: 3px 0; }
      .bar-fill { height: 100%; background: ${T.blue}; border-radius: ${T.radiusSmall}; transition: width 60ms; }
      .row { display: flex; justify-content: space-between; color: ${T.textSecondary}; font: ${T.fontSmall}; }
      .row .value { color: ${T.green}; }
    `;
  }

  get template() {
    return `
      <div class="wrap">
        <div class="bar-bg"><div class="bar-fill" id="fill"></div></div>
        <div class="row">
          <span>${ReflowUI.icon('layers', 12)} <span class="value" id="cur">0</span> / <span id="tgt">64 KB</span></span>
          <span>Flushes: <span class="value" id="fl">0</span></span>
        </div>
      </div>
    `;
  }

  onConnect() {
    const props = this.getProps();
    this._bufferBytes = props.bufferBytes ?? 65536;
    this._currentBytes = 0;
    this._flushes = 0;
    this.$('tgt').textContent = ReflowUI.formatBytes(this._bufferBytes);

    this.sub(() => this.zeal?.onPropertyChange((values) => {
      if (values.bufferBytes !== undefined) {
        this._bufferBytes = values.bufferBytes;
        this.$('tgt').textContent = ReflowUI.formatBytes(this._bufferBytes);
      }
    }));

    this.sub(() => this.zeal?.onStreamFrame((payload) => {
      this._currentBytes += payload.byteLength;
      if (this._currentBytes >= this._bufferBytes) {
        this._flushes += Math.floor(this._currentBytes / this._bufferBytes);
        this._currentBytes = this._currentBytes % this._bufferBytes;
      }
      const pct = Math.min(100, (this._currentBytes / this._bufferBytes) * 100);
      this.$('fill').style.width = pct + '%';
      this.$('cur').textContent = ReflowUI.formatBytes(this._currentBytes);
      this.$('fl').textContent = this._flushes;
    }));
  }

  set bufferBytes(v) { this._bufferBytes = v; }
}

customElements.define('reflow-buffer', ReflowBuffer);
