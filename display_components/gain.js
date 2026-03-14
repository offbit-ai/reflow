// reflow-gain: VU meters + draggable gain fader
// Config: gainDb, gainLinear

class ReflowGain extends ReflowUI.ReflowComponent {
  get styles() {
    const T = ReflowUI.theme;
    return `
      .container { display: flex; gap: 4px; padding: ${T.pad}; align-items: stretch; height: 80px; }
      .meters { flex: 1; display: flex; gap: 3px; }
      .meter-col { flex: 1; display: flex; flex-direction: column; align-items: center; gap: 2px; }
      .meter-track { flex: 1; width: 14px; background: ${T.bgSurface}; border-radius: ${T.radiusSmall}; position: relative; overflow: hidden; }
      .meter-fill { position: absolute; bottom: 0; width: 100%; border-radius: ${T.radiusSmall}; transition: height 60ms; }
      .meter-fill.in { background: linear-gradient(0deg, ${T.green}, ${T.amber}, ${T.red}); }
      .meter-fill.out { background: linear-gradient(0deg, ${T.blue}, ${T.purple}, ${T.red}); }
      .meter-label { font: ${T.fontTiny}; color: ${T.textMuted}; }
      .fader { width: 36px; display: flex; flex-direction: column; align-items: center; }
      .fader-track { flex: 1; width: 4px; background: ${T.borderLight}; border-radius: 2px; position: relative; }
      .fader-thumb { position: absolute; width: 20px; height: 8px; background: ${T.amber}; border-radius: 2px; left: -8px; cursor: grab; }
      .fader-thumb:active { cursor: grabbing; background: #fbbf24; }
      .gain-value { font: ${T.fontMono}; color: ${T.amber}; text-align: center; margin-top: 2px; }
      .db-scale { display: flex; flex-direction: column; justify-content: space-between; font: ${T.fontTiny}; color: ${T.textDim}; width: 20px; text-align: right; padding: 2px 0; }
    `;
  }

  get template() {
    return `
      <div class="container">
        <div class="db-scale"><span>+60</span><span>+30</span><span>0</span><span>-30</span><span>-60</span></div>
        <div class="meters">
          <div class="meter-col">
            <div class="meter-track"><div class="meter-fill in" id="in-m"></div></div>
            <span class="meter-label">IN</span>
          </div>
          <div class="meter-col">
            <div class="meter-track"><div class="meter-fill out" id="out-m"></div></div>
            <span class="meter-label">OUT</span>
          </div>
        </div>
        <div class="fader">
          <div class="fader-track" id="ft">
            <div class="fader-thumb" id="fth"></div>
          </div>
          <div class="gain-value" id="gv">0.0</div>
        </div>
      </div>
    `;
  }

  onConnect() {
    const props = this.getProps();
    this._gainDb = props.gainDb ?? 0;
    this._dragging = false;
    this._updateFader();

    const thumb = this.$('fth');
    const track = this.$('ft');

    thumb.addEventListener('pointerdown', (e) => {
      this._dragging = true;
      thumb.setPointerCapture(e.pointerId);
      e.stopPropagation();
    });
    thumb.addEventListener('pointermove', (e) => {
      if (!this._dragging) return;
      const rect = track.getBoundingClientRect();
      const y = (e.clientY - rect.top) / rect.height;
      this._gainDb = Math.round((60 - y * 120) * 10) / 10;
      this._gainDb = Math.max(-60, Math.min(60, this._gainDb));
      this.zeal?.setProperty('gainDb', this._gainDb);
      this._updateFader();
    });
    thumb.addEventListener('pointerup', () => { this._dragging = false; });

    this.sub(() => this.zeal?.onPropertyChange((values) => {
      if (values.gainDb !== undefined && !this._dragging) {
        this._gainDb = values.gainDb;
        this._updateFader();
      }
    }));

    this.sub(() => this.zeal?.onStreamFrame((payload) => {
      const samples = new Float32Array(payload.buffer, payload.byteOffset, payload.byteLength / 4);
      let peak = 0;
      for (let i = 0; i < samples.length; i++) {
        const a = Math.abs(samples[i]);
        if (a > peak) peak = a;
      }
      const gainLin = Math.pow(10, this._gainDb / 20);
      const peakIn = gainLin > 0 ? peak / gainLin : peak;
      this.$('in-m').style.height = Math.min(100, peakIn * 100) + '%';
      this.$('out-m').style.height = Math.min(100, peak * 100) + '%';
    }));
  }

  _updateFader() {
    const norm = (60 - this._gainDb) / 120;
    const track = this.$('ft');
    const thumb = this.$('fth');
    if (track && thumb) {
      thumb.style.top = (norm * (track.offsetHeight || 60) - 4) + 'px';
    }
    const T = ReflowUI.theme;
    const label = this.$('gv');
    if (label) {
      label.textContent = ReflowUI.formatDb(this._gainDb);
      label.style.color = this._gainDb > 0 ? T.red : this._gainDb < 0 ? T.blue : T.amber;
    }
  }

  set gainDb(v) { this._gainDb = v; this._updateFader?.(); }
}

customElements.define('reflow-gain', ReflowGain);
