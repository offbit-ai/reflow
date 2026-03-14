// reflow-stats: Live stream throughput counters
// Shows bytes, frames, duration, and throughput in real-time.
// No editing — pure visualization.

class ReflowStats extends HTMLElement {
  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
    this.shadowRoot.innerHTML = `
      <style>
        :host { display: flex; flex-direction: column; flex: 1 1 auto; min-width: 0; width: 100%; font: 11px monospace; color: #ccc; }
        .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 2px 8px; padding: 4px 6px; }
        .label { color: #666; }
        .value { color: #22c55e; text-align: right; }
        .value.active { color: #f59e0b; }
      </style>
      <div class="grid">
        <span class="label">Bytes</span><span class="value" id="bytes">0</span>
        <span class="label">Frames</span><span class="value" id="frames">0</span>
        <span class="label">Time</span><span class="value" id="time">0s</span>
        <span class="label">Rate</span><span class="value" id="rate">0 B/s</span>
      </div>
    `;
    this._bytes = 0;
    this._frames = 0;
    this._startTime = null;
  }

  connectedCallback() {
    this._unsub = this.zeal?.onStreamFrame((payload) => {
      if (!this._startTime) this._startTime = performance.now();
      this._bytes += payload.byteLength;
      this._frames++;
      this._update();
    });

    this._unsubState = this.zeal?.onStreamStateChange((state) => {
      if (state.phase === 'complete') {
        this._update();
        this.shadowRoot.querySelectorAll('.value').forEach(el => el.classList.remove('active'));
      }
    });
  }

  disconnectedCallback() {
    this._unsub?.();
    this._unsubState?.();
  }

  _update() {
    const elapsed = this._startTime ? (performance.now() - this._startTime) / 1000 : 0;
    const rate = elapsed > 0 ? this._bytes / elapsed : 0;

    this.shadowRoot.getElementById('bytes').textContent = this._formatBytes(this._bytes);
    this.shadowRoot.getElementById('frames').textContent = this._frames.toLocaleString();
    this.shadowRoot.getElementById('time').textContent = elapsed.toFixed(1) + 's';
    this.shadowRoot.getElementById('rate').textContent = this._formatBytes(rate) + '/s';

    this.shadowRoot.querySelectorAll('.value').forEach(el => el.classList.add('active'));
  }

  _formatBytes(b) {
    if (b < 1024) return b + ' B';
    if (b < 1048576) return (b / 1024).toFixed(1) + ' KB';
    return (b / 1048576).toFixed(1) + ' MB';
  }
}

customElements.define('reflow-stats', ReflowStats);
