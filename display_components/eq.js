// reflow-eq: Interactive multi-band parametric EQ display
// Renders frequency response curve with draggable band handles.
// Each handle controls frequency (x), gain (y), Q (scroll wheel).
// Edits flow back via zeal.setProperty('bands', [...]).

class ReflowEq extends HTMLElement {
  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
    this.shadowRoot.innerHTML = `
      <style>
        :host { display: flex; flex-direction: column; flex: 1 1 auto; min-width: 0; width: 100%; }
        canvas { width: 100%; height: 140px; border-radius: 4px; background: #0a0a0a; cursor: crosshair; }
        .info { font: 9px monospace; color: #888; padding: 2px 4px; }
      </style>
      <canvas></canvas>
      <div class="info">Drag bands to adjust. Scroll to change Q.</div>
    `;
    this._canvas = this.shadowRoot.querySelector('canvas');
    this._info = this.shadowRoot.querySelector('.info');
    this._bands = [];
    this._dragIdx = -1;
    this._sampleRate = 44100;
  }

  connectedCallback() {
    const dpr = window.devicePixelRatio || 1;
    const canvas = this._canvas;
    canvas.width = canvas.offsetWidth * dpr;
    canvas.height = 140 * dpr;
    this._ctx = canvas.getContext('2d');
    this._ctx.scale(dpr, dpr);
    this._w = canvas.offsetWidth;
    this._h = 140;

    canvas.addEventListener('pointerdown', (e) => this._onDown(e));
    canvas.addEventListener('pointermove', (e) => this._onMove(e));
    canvas.addEventListener('pointerup', () => this._onUp());
    canvas.addEventListener('wheel', (e) => this._onWheel(e), { passive: false });

    // Double-click to add a band
    canvas.addEventListener('dblclick', (e) => this._addBand(e));

    this._unsubProps = this.zeal?.onPropertyChange((values) => {
      if (values.bands) {
        this._bands = Array.isArray(values.bands) ? values.bands : JSON.parse(values.bands);
        this._draw();
      }
      if (values.sampleRate) this._sampleRate = values.sampleRate;
    });

    // Initialize from current properties
    const props = this.zeal?.getProperties?.();
    if (props?.bands) {
      this._bands = Array.isArray(props.bands) ? props.bands : JSON.parse(props.bands);
    }
    this._draw();
  }

  disconnectedCallback() {
    this._unsubProps?.();
  }

  _freqToX(freq) {
    // Log scale: 20Hz → 20kHz
    return (Math.log10(freq / 20) / Math.log10(1000)) * this._w;
  }

  _xToFreq(x) {
    return 20 * Math.pow(1000, x / this._w);
  }

  _gainToY(gain) {
    // -24dB at bottom, +24dB at top
    return this._h / 2 - (gain / 24) * (this._h / 2);
  }

  _yToGain(y) {
    return -((y - this._h / 2) / (this._h / 2)) * 24;
  }

  _draw() {
    const ctx = this._ctx;
    const w = this._w;
    const h = this._h;
    ctx.clearRect(0, 0, w, h);

    // Grid lines
    ctx.strokeStyle = '#1a1a1a';
    ctx.lineWidth = 0.5;
    // Frequency grid: 100, 1k, 10k
    for (const f of [100, 1000, 10000]) {
      const x = this._freqToX(f);
      ctx.beginPath(); ctx.moveTo(x, 0); ctx.lineTo(x, h); ctx.stroke();
    }
    // Gain grid: 0dB center
    const zeroY = h / 2;
    ctx.beginPath(); ctx.moveTo(0, zeroY); ctx.lineTo(w, zeroY); ctx.stroke();
    for (const db of [-12, 12]) {
      const y = this._gainToY(db);
      ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(w, y); ctx.stroke();
    }

    // Frequency response curve (approximate — sum of band contributions)
    ctx.strokeStyle = '#22c55e';
    ctx.lineWidth = 2;
    ctx.beginPath();
    for (let px = 0; px < w; px++) {
      const freq = this._xToFreq(px);
      let totalGain = 0;
      for (const band of this._bands) {
        const f0 = band.frequency || 1000;
        const gain = band.gain || 0;
        const q = band.q || 1;
        // Approximate bell response
        const ratio = freq / f0;
        const logRatio = Math.log2(ratio);
        const response = gain * Math.exp(-0.5 * Math.pow(logRatio * q * 2, 2));
        totalGain += response;
      }
      const y = this._gainToY(totalGain);
      if (px === 0) ctx.moveTo(px, y); else ctx.lineTo(px, y);
    }
    ctx.stroke();

    // Fill under curve
    ctx.lineTo(w, zeroY);
    ctx.lineTo(0, zeroY);
    ctx.closePath();
    ctx.fillStyle = 'rgba(34, 197, 94, 0.08)';
    ctx.fill();

    // Band handles
    for (let i = 0; i < this._bands.length; i++) {
      const band = this._bands[i];
      const x = this._freqToX(band.frequency || 1000);
      const y = this._gainToY(band.gain || 0);
      const r = this._dragIdx === i ? 7 : 5;

      ctx.beginPath();
      ctx.arc(x, y, r, 0, Math.PI * 2);
      ctx.fillStyle = this._dragIdx === i ? '#fff' : '#22c55e';
      ctx.fill();
      ctx.strokeStyle = '#000';
      ctx.lineWidth = 1;
      ctx.stroke();
    }
  }

  _hitTest(e) {
    const rect = this._canvas.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;
    for (let i = 0; i < this._bands.length; i++) {
      const x = this._freqToX(this._bands[i].frequency || 1000);
      const y = this._gainToY(this._bands[i].gain || 0);
      if (Math.hypot(mx - x, my - y) < 10) return i;
    }
    return -1;
  }

  _onDown(e) {
    this._dragIdx = this._hitTest(e);
    if (this._dragIdx >= 0) {
      this._canvas.setPointerCapture(e.pointerId);
    }
  }

  _onMove(e) {
    if (this._dragIdx < 0) return;
    const rect = this._canvas.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;

    const band = this._bands[this._dragIdx];
    band.frequency = Math.round(this._xToFreq(mx));
    band.gain = Math.round(this._yToGain(my) * 10) / 10;
    this._draw();
    this._info.textContent = `Band ${this._dragIdx + 1}: ${band.frequency} Hz, ${band.gain > 0 ? '+' : ''}${band.gain} dB, Q ${band.q}`;
  }

  _onUp() {
    if (this._dragIdx >= 0) {
      this.zeal?.setProperty('bands', JSON.stringify(this._bands));
      this._dragIdx = -1;
      this._draw();
    }
  }

  _onWheel(e) {
    const idx = this._hitTest(e);
    if (idx < 0) return;
    e.preventDefault();
    const band = this._bands[idx];
    const delta = e.deltaY > 0 ? -0.1 : 0.1;
    band.q = Math.max(0.1, Math.min(30, (band.q || 1) + delta));
    band.q = Math.round(band.q * 10) / 10;
    this._draw();
    this._info.textContent = `Band ${idx + 1}: Q = ${band.q}`;
    this.zeal?.setProperty('bands', JSON.stringify(this._bands));
  }

  _addBand(e) {
    const rect = this._canvas.getBoundingClientRect();
    const freq = Math.round(this._xToFreq(e.clientX - rect.left));
    this._bands.push({ type: 'peaking', frequency: freq, gain: 0, q: 1.0 });
    this.zeal?.setProperty('bands', JSON.stringify(this._bands));
    this._draw();
  }

  set bands(v) {
    this._bands = Array.isArray(v) ? v : JSON.parse(v || '[]');
    this._draw?.();
  }

  set sampleRate(v) { this._sampleRate = v; }
}

customElements.define('reflow-eq', ReflowEq);
