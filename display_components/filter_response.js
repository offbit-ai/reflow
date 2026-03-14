// reflow-filter-response: Frequency response curve for BiquadFilter
// Config: filterType, frequency, q, gainDb, sampleRate
// Renders the theoretical biquad magnitude response.

class ReflowFilterResponse extends HTMLElement {
  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
    this.shadowRoot.innerHTML = `
      <style>
        :host { display: flex; flex-direction: column; flex: 1 1 auto; min-width: 0; width: 100%; }
        canvas { width: 100%; height: 80px; border-radius: 4px; background: #0a0a0a; }
        .info { font: 9px monospace; color: #888; padding: 2px 4px; }
      </style>
      <canvas></canvas>
      <div class="info"></div>
    `;
    this._canvas = this.shadowRoot.querySelector('canvas');
    this._info = this.shadowRoot.querySelector('.info');
    this._filterType = 'lowpass';
    this._frequency = 1000;
    this._q = 0.707;
    this._gainDb = 0;
    this._sampleRate = 44100;
  }

  connectedCallback() {
    const dpr = window.devicePixelRatio || 1;
    this._canvas.width = this._canvas.offsetWidth * dpr;
    this._canvas.height = 80 * dpr;
    this._ctx = this._canvas.getContext('2d');
    this._ctx.scale(dpr, dpr);
    this._w = this._canvas.offsetWidth;
    this._h = 80;

    const props = this.zeal?.getProperties();
    if (props) {
      this._filterType = props.filterType || 'lowpass';
      this._frequency = props.frequency || 1000;
      this._q = props.q || 0.707;
      this._gainDb = props.gainDb || 0;
      this._sampleRate = props.sampleRate || 44100;
    }
    this._draw();

    this._unsubProps = this.zeal?.onPropertyChange((values) => {
      if (values.filterType !== undefined) this._filterType = values.filterType;
      if (values.frequency !== undefined) this._frequency = values.frequency;
      if (values.q !== undefined) this._q = values.q;
      if (values.gainDb !== undefined) this._gainDb = values.gainDb;
      if (values.sampleRate !== undefined) this._sampleRate = values.sampleRate;
      this._draw();
    });
  }

  disconnectedCallback() { this._unsubProps?.(); }

  _freqToX(f) { return (Math.log10(f / 20) / Math.log10(1000)) * this._w; }

  _draw() {
    const ctx = this._ctx;
    const w = this._w, h = this._h;
    ctx.clearRect(0, 0, w, h);

    // Grid
    ctx.strokeStyle = '#1a1a1a';
    ctx.lineWidth = 0.5;
    for (const f of [100, 1000, 10000]) {
      const x = this._freqToX(f);
      ctx.beginPath(); ctx.moveTo(x, 0); ctx.lineTo(x, h); ctx.stroke();
    }
    const zeroY = h / 2;
    ctx.beginPath(); ctx.moveTo(0, zeroY); ctx.lineTo(w, zeroY); ctx.stroke();

    // Approximate biquad response
    ctx.strokeStyle = '#22c55e';
    ctx.lineWidth = 2;
    ctx.beginPath();
    for (let px = 0; px < w; px++) {
      const freq = 20 * Math.pow(1000, px / w);
      const response = this._biquadResponse(freq);
      const db = 20 * Math.log10(Math.max(0.001, response));
      const y = zeroY - (db / 24) * zeroY;
      if (px === 0) ctx.moveTo(px, Math.max(0, Math.min(h, y)));
      else ctx.lineTo(px, Math.max(0, Math.min(h, y)));
    }
    ctx.stroke();

    // Cutoff marker
    const cutX = this._freqToX(this._frequency);
    ctx.strokeStyle = '#f59e0b';
    ctx.lineWidth = 1;
    ctx.setLineDash([3, 3]);
    ctx.beginPath(); ctx.moveTo(cutX, 0); ctx.lineTo(cutX, h); ctx.stroke();
    ctx.setLineDash([]);

    this._info.textContent = `${this._filterType} ${this._frequency} Hz Q=${this._q}`;
  }

  _biquadResponse(freq) {
    const f0 = this._frequency;
    const q = this._q;
    const ratio = freq / f0;
    switch (this._filterType) {
      case 'lowpass': case 'lpf':
        return 1.0 / Math.sqrt(1 + Math.pow(ratio, 4) * Math.pow(1 / q, 2));
      case 'highpass': case 'hpf':
        return Math.pow(ratio, 2) / Math.sqrt(1 + Math.pow(ratio, 4) * Math.pow(1 / q, 2));
      case 'bandpass': case 'bpf':
        return (ratio / q) / Math.sqrt(Math.pow(1 - ratio * ratio, 2) + Math.pow(ratio / q, 2));
      case 'notch':
        return Math.abs(1 - ratio * ratio) / Math.sqrt(Math.pow(1 - ratio * ratio, 2) + Math.pow(ratio / q, 2));
      case 'peaking': case 'eq': {
        const A = Math.pow(10, this._gainDb / 40);
        const num = Math.sqrt(Math.pow(1 - ratio * ratio, 2) + Math.pow(A * ratio / q, 2));
        const den = Math.sqrt(Math.pow(1 - ratio * ratio, 2) + Math.pow(ratio / (A * q), 2));
        return num / den;
      }
      case 'lowshelf': {
        const A = Math.pow(10, this._gainDb / 40);
        return ratio < 1 ? A : 1 + (A - 1) * Math.exp(-2 * (ratio - 1) * q);
      }
      case 'highshelf': {
        const A = Math.pow(10, this._gainDb / 40);
        return ratio > 1 ? A : 1 + (A - 1) * Math.exp(-2 * (1 - ratio) * q);
      }
      default: return 1.0;
    }
  }

  set filterType(v) { this._filterType = v; this._draw?.(); }
  set frequency(v) { this._frequency = v; this._draw?.(); }
  set q(v) { this._q = v; this._draw?.(); }
  set gainDb(v) { this._gainDb = v; this._draw?.(); }
  set sampleRate(v) { this._sampleRate = v; }
}

customElements.define('reflow-filter-response', ReflowFilterResponse);
