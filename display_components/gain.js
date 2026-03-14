// reflow-gain: VU meter with draggable gain fader
// Config: gainDb, gainLinear
// Interactive: drag the fader to adjust gain, reflected in zeal property store.

class ReflowGain extends HTMLElement {
  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
    this.shadowRoot.innerHTML = `
      <style>
        :host { display: flex; flex-direction: column; flex: 1 1 auto; min-width: 0; width: 100%; }
        .container { display: flex; gap: 4px; padding: 4px; align-items: stretch; height: 80px; }
        .meters { flex: 1; display: flex; gap: 3px; }
        .meter-col { flex: 1; display: flex; flex-direction: column; align-items: center; gap: 2px; }
        .meter-track { flex: 1; width: 14px; background: #111; border-radius: 3px; position: relative; overflow: hidden; }
        .meter-fill { position: absolute; bottom: 0; width: 100%; border-radius: 3px; transition: height 60ms; }
        .meter-fill.in { background: linear-gradient(0deg, #22c55e, #eab308, #ef4444); }
        .meter-fill.out { background: linear-gradient(0deg, #3b82f6, #8b5cf6, #ef4444); }
        .meter-label { font: 8px monospace; color: #666; }
        .fader { width: 36px; display: flex; flex-direction: column; align-items: center; cursor: ns-resize; }
        .fader-track { flex: 1; width: 4px; background: #333; border-radius: 2px; position: relative; }
        .fader-thumb { position: absolute; width: 20px; height: 8px; background: #f59e0b; border-radius: 2px; left: -8px; cursor: grab; }
        .fader-thumb:active { cursor: grabbing; background: #fbbf24; }
        .gain-value { font: 10px monospace; color: #f59e0b; text-align: center; margin-top: 2px; }
        .db-scale { display: flex; flex-direction: column; justify-content: space-between; font: 7px monospace; color: #444; width: 20px; text-align: right; padding: 2px 0; }
      </style>
      <div class="container">
        <div class="db-scale">
          <span>+60</span><span>+30</span><span>0</span><span>-30</span><span>-60</span>
        </div>
        <div class="meters">
          <div class="meter-col">
            <div class="meter-track"><div class="meter-fill in" id="in-meter"></div></div>
            <span class="meter-label">IN</span>
          </div>
          <div class="meter-col">
            <div class="meter-track"><div class="meter-fill out" id="out-meter"></div></div>
            <span class="meter-label">OUT</span>
          </div>
        </div>
        <div class="fader" id="fader-area">
          <div class="fader-track" id="fader-track">
            <div class="fader-thumb" id="fader-thumb"></div>
          </div>
          <div class="gain-value" id="gain-value">0.0</div>
        </div>
      </div>
    `;
    this._gainDb = 0;
    this._dragging = false;
  }

  connectedCallback() {
    const props = this.zeal?.getProperties();
    if (props?.gainDb !== undefined) { this._gainDb = props.gainDb; }
    this._updateFader();

    // Draggable fader
    const track = this.shadowRoot.getElementById('fader-track');
    const thumb = this.shadowRoot.getElementById('fader-thumb');

    thumb.addEventListener('pointerdown', (e) => {
      this._dragging = true;
      thumb.setPointerCapture(e.pointerId);
      e.stopPropagation();
    });
    thumb.addEventListener('pointermove', (e) => {
      if (!this._dragging) return;
      const rect = track.getBoundingClientRect();
      const y = (e.clientY - rect.top) / rect.height;
      // top = +60dB, bottom = -60dB
      this._gainDb = Math.round((60 - y * 120) * 10) / 10;
      this._gainDb = Math.max(-60, Math.min(60, this._gainDb));
      this.zeal?.setProperty('gainDb', this._gainDb);
      this._updateFader();
    });
    thumb.addEventListener('pointerup', () => { this._dragging = false; });

    this._unsubProps = this.zeal?.onPropertyChange((values) => {
      if (values.gainDb !== undefined && !this._dragging) {
        this._gainDb = values.gainDb;
        this._updateFader();
      }
    });

    this._unsub = this.zeal?.onStreamFrame((payload) => {
      const samples = new Float32Array(payload.buffer, payload.byteOffset, payload.byteLength / 4);
      let peak = 0;
      for (let i = 0; i < samples.length; i++) {
        const a = Math.abs(samples[i]);
        if (a > peak) peak = a;
      }
      const gainLin = Math.pow(10, this._gainDb / 20);
      const peakIn = gainLin > 0 ? peak / gainLin : peak;
      this.shadowRoot.getElementById('in-meter').style.height = Math.min(100, peakIn * 100) + '%';
      this.shadowRoot.getElementById('out-meter').style.height = Math.min(100, peak * 100) + '%';
    });
  }

  disconnectedCallback() { this._unsub?.(); this._unsubProps?.(); }

  _updateFader() {
    // Map -60..+60 to track position (top=+60, bottom=-60)
    const norm = (60 - this._gainDb) / 120;
    const track = this.shadowRoot.getElementById('fader-track');
    const thumb = this.shadowRoot.getElementById('fader-thumb');
    if (track && thumb) {
      const trackH = track.offsetHeight || 60;
      thumb.style.top = (norm * trackH - 4) + 'px';
    }
    const label = this.shadowRoot.getElementById('gain-value');
    const sign = this._gainDb >= 0 ? '+' : '';
    label.textContent = `${sign}${this._gainDb.toFixed(1)}`;
    label.style.color = this._gainDb > 0 ? '#ef4444' : this._gainDb < 0 ? '#3b82f6' : '#f59e0b';
  }

  set gainDb(v) { this._gainDb = v; this._updateFader?.(); }
}

customElements.define('reflow-gain', ReflowGain);
