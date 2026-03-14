// reflow-crossover: 3-band frequency response with draggable crossover points
// Editable: lowFrequency, highFrequency via drag handles.

class ReflowCrossover extends HTMLElement {
  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
    this.shadowRoot.innerHTML = `
      <style>
        :host { display: flex; flex-direction: column; flex: 1 1 auto; min-width: 0; width: 100%; }
        canvas { width: 100%; height: 80px; border-radius: 4px; background: #0a0a0a; cursor: col-resize; }
        .info { font: 9px monospace; color: #888; padding: 2px 4px; }
      </style>
      <canvas></canvas>
      <div class="info">Drag crossover points to adjust</div>
    `;
    this._canvas = this.shadowRoot.querySelector('canvas');
    this._info = this.shadowRoot.querySelector('.info');
    this._lowFreq = 200;
    this._highFreq = 4000;
    this._dragging = null; // 'low' | 'high' | null
  }

  connectedCallback() {
    const dpr = window.devicePixelRatio || 1;
    this._canvas.width = this._canvas.offsetWidth * dpr;
    this._canvas.height = 80 * dpr;
    this._ctx = this._canvas.getContext('2d');
    this._ctx.scale(dpr, dpr);
    this._w = this._canvas.offsetWidth;
    this._h = 80;

    this._canvas.addEventListener('pointerdown', (e) => this._onDown(e));
    this._canvas.addEventListener('pointermove', (e) => this._onMove(e));
    this._canvas.addEventListener('pointerup', () => this._onUp());

    this._unsubProps = this.zeal?.onPropertyChange((values) => {
      if (values.lowFrequency !== undefined) this._lowFreq = values.lowFrequency;
      if (values.highFrequency !== undefined) this._highFreq = values.highFrequency;
      this._draw();
    });

    this._draw();
  }

  disconnectedCallback() { this._unsubProps?.(); }

  _freqToX(f) { return (Math.log10(f / 20) / Math.log10(1000)) * this._w; }
  _xToFreq(x) { return 20 * Math.pow(1000, x / this._w); }

  _draw() {
    const ctx = this._ctx;
    const w = this._w;
    const h = this._h;
    ctx.clearRect(0, 0, w, h);

    const lowX = this._freqToX(this._lowFreq);
    const highX = this._freqToX(this._highFreq);

    // Band fills
    ctx.fillStyle = 'rgba(239, 68, 68, 0.15)'; // low = red
    ctx.fillRect(0, 0, lowX, h);
    ctx.fillStyle = 'rgba(34, 197, 94, 0.15)'; // mid = green
    ctx.fillRect(lowX, 0, highX - lowX, h);
    ctx.fillStyle = 'rgba(59, 130, 246, 0.15)'; // high = blue
    ctx.fillRect(highX, 0, w - highX, h);

    // Crossover lines
    ctx.strokeStyle = '#f59e0b';
    ctx.lineWidth = 2;
    ctx.beginPath(); ctx.moveTo(lowX, 0); ctx.lineTo(lowX, h); ctx.stroke();
    ctx.beginPath(); ctx.moveTo(highX, 0); ctx.lineTo(highX, h); ctx.stroke();

    // Handle dots
    for (const x of [lowX, highX]) {
      ctx.beginPath();
      ctx.arc(x, h / 2, 5, 0, Math.PI * 2);
      ctx.fillStyle = '#f59e0b';
      ctx.fill();
    }

    // Labels
    ctx.fillStyle = '#666';
    ctx.font = '9px monospace';
    ctx.fillText('Low', 4, 12);
    ctx.fillText('Mid', (lowX + highX) / 2 - 8, 12);
    ctx.fillText('High', highX + 4, 12);

    this._info.textContent = `Low: ${this._lowFreq} Hz | High: ${this._highFreq} Hz`;
  }

  _onDown(e) {
    const rect = this._canvas.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const lowX = this._freqToX(this._lowFreq);
    const highX = this._freqToX(this._highFreq);

    if (Math.abs(mx - lowX) < 12) this._dragging = 'low';
    else if (Math.abs(mx - highX) < 12) this._dragging = 'high';

    if (this._dragging) this._canvas.setPointerCapture(e.pointerId);
  }

  _onMove(e) {
    if (!this._dragging) return;
    const rect = this._canvas.getBoundingClientRect();
    const freq = Math.round(this._xToFreq(e.clientX - rect.left));

    if (this._dragging === 'low') {
      this._lowFreq = Math.max(20, Math.min(freq, this._highFreq - 100));
      this.zeal?.setProperty('lowFrequency', this._lowFreq);
    } else {
      this._highFreq = Math.max(this._lowFreq + 100, Math.min(freq, 20000));
      this.zeal?.setProperty('highFrequency', this._highFreq);
    }
    this._draw();
  }

  _onUp() { this._dragging = null; }

  set lowFrequency(v) { this._lowFreq = v; this._draw?.(); }
  set highFrequency(v) { this._highFreq = v; this._draw?.(); }
}

customElements.define('reflow-crossover', ReflowCrossover);
