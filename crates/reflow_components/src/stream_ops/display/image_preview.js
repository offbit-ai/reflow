// reflow-image-preview: Live image preview from stream frames
// Renders accumulated row data onto a canvas as the stream progresses.

class ReflowImagePreview extends ReflowUI.ReflowComponent {
  get styles() {
    const T = ReflowUI.theme;
    return `
      .wrap { position: relative; background: ${T.bgSurface}; border-radius: ${T.radius}; overflow: hidden; }
      canvas { width: 100%; image-rendering: pixelated; }
      .overlay { position: absolute; bottom: 0; left: 0; right: 0;
        background: rgba(0,0,0,0.7); font: ${T.fontSmall}; color: ${T.textSecondary};
        padding: ${T.padSmall} ${T.pad}; }
    `;
  }

  get template() {
    return `
      <div class="wrap">
        <canvas id="cv"></canvas>
        <div class="overlay" id="ov">Waiting for stream...</div>
      </div>
    `;
  }

  onConnect() {
    this._width = 0;
    this._height = 0;
    this._channels = 4;
    this._row = 0;
    this._totalBytes = 0;
    this._ctx = null;

    this.sub(() => this.zeal?.onStreamStateChange((state) => {
      if (state.phase === 'begin' && state.meta) {
        this._initCanvas(state.meta);
      }
      if (state.phase === 'complete') {
        this.$('ov').textContent = `${this._width}\u00d7${this._height} \u2022 ${ReflowUI.formatBytes(this._totalBytes)}`;
      }
    }));

    this.sub(() => this.zeal?.onStreamFrame((payload) => {
      this._handleFrame(payload);
    }));
  }

  _initCanvas(meta) {
    this._width = meta.width || 256;
    this._height = meta.height || 256;
    this._channels = meta.format === 'Gray8' ? 1 : 4;
    const cv = this.$('cv');
    cv.width = this._width;
    cv.height = this._height;
    this._ctx = cv.getContext('2d');
    this._row = 0;
    this._totalBytes = 0;
    this.$('ov').textContent = `${this._width}\u00d7${this._height} streaming...`;
  }

  _handleFrame(payload) {
    this._totalBytes += payload.byteLength;
    if (!this._ctx) {
      this._width = 256;
      this._height = 256;
      const cv = this.$('cv');
      cv.width = this._width;
      cv.height = this._height;
      this._ctx = cv.getContext('2d');
    }

    const rowBytes = this._width * this._channels;
    const data = new Uint8Array(payload.buffer, payload.byteOffset, payload.byteLength);

    for (let offset = 0; offset < data.length && this._row < this._height; offset += rowBytes) {
      const rowData = data.subarray(offset, offset + rowBytes);
      const imgData = this._ctx.createImageData(this._width, 1);

      if (this._channels === 4) {
        imgData.data.set(rowData);
      } else if (this._channels === 1) {
        for (let i = 0; i < this._width; i++) {
          const g = rowData[i] || 0;
          imgData.data[i * 4] = g;
          imgData.data[i * 4 + 1] = g;
          imgData.data[i * 4 + 2] = g;
          imgData.data[i * 4 + 3] = 255;
        }
      }

      this._ctx.putImageData(imgData, 0, this._row);
      this._row++;
    }

    this.$('ov').textContent = `Row ${this._row}/${this._height} \u2022 ${ReflowUI.formatBytes(this._totalBytes)}`;
  }
}

customElements.define('reflow-image-preview', ReflowImagePreview);
