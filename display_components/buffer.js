// reflow-buffer: Fill level gauge with flush counter
// Config: bufferBytes
// Shows accumulation progress and flush events.

class ReflowBuffer extends HTMLElement {
  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
    this.shadowRoot.innerHTML = `
      <style>
        :host { display: flex; flex-direction: column; flex: 1 1 auto; min-width: 0; width: 100%; }
        .wrap { padding: 4px 6px; font: 10px monospace; }
        .bar-bg { height: 10px; background: #111; border-radius: 3px; overflow: hidden; margin: 3px 0; }
        .bar-fill { height: 100%; background: #3b82f6; border-radius: 3px; transition: width 60ms; }
        .row { display: flex; justify-content: space-between; color: #888; }
        .value { color: #22c55e; }
      </style>
      <div class="wrap">
        <div class="bar-bg"><div class="bar-fill" id="fill"></div></div>
        <div class="row">
          <span>Buffer: <span class="value" id="current">0</span> / <span id="target">64 KB</span></span>
          <span>Flushes: <span class="value" id="flushes">0</span></span>
        </div>
      </div>
    `;
    this._bufferBytes = 65536;
    this._currentBytes = 0;
    this._flushes = 0;
    this._lastSize = 0;
  }

  connectedCallback() {
    const props = this.zeal?.getProperties();
    if (props?.bufferBytes) {
      this._bufferBytes = props.bufferBytes;
      this.shadowRoot.getElementById('target').textContent = this._fmt(this._bufferBytes);
    }

    this._unsubProps = this.zeal?.onPropertyChange((values) => {
      if (values.bufferBytes !== undefined) {
        this._bufferBytes = values.bufferBytes;
        this.shadowRoot.getElementById('target').textContent = this._fmt(this._bufferBytes);
      }
    });

    this._unsub = this.zeal?.onStreamFrame((payload) => {
      this._currentBytes += payload.byteLength;
      if (this._currentBytes >= this._bufferBytes) {
        this._flushes += Math.floor(this._currentBytes / this._bufferBytes);
        this._currentBytes = this._currentBytes % this._bufferBytes;
      }
      this._update();
    });
  }

  disconnectedCallback() { this._unsub?.(); this._unsubProps?.(); }

  _update() {
    const pct = Math.min(100, (this._currentBytes / this._bufferBytes) * 100);
    this.shadowRoot.getElementById('fill').style.width = pct + '%';
    this.shadowRoot.getElementById('current').textContent = this._fmt(this._currentBytes);
    this.shadowRoot.getElementById('flushes').textContent = this._flushes;
  }

  _fmt(b) {
    if (b < 1024) return b + ' B';
    if (b < 1048576) return (b / 1024).toFixed(0) + ' KB';
    return (b / 1048576).toFixed(1) + ' MB';
  }

  set bufferBytes(v) { this._bufferBytes = v; }
}

customElements.define('reflow-buffer', ReflowBuffer);
