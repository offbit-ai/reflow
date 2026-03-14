// reflow-image-preview: Live image preview from stream frames
// Renders accumulated row data onto a canvas as the stream progresses.
// Editable properties forwarded: brightness, contrast, saturation, width, height.

class ReflowImagePreview extends HTMLElement {
  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
    this.shadowRoot.innerHTML = `
      <style>
        :host { display: flex; flex-direction: column; flex: 1 1 auto; min-width: 0; width: 100%; }
        .wrap { position: relative; background: #111; border-radius: 4px; overflow: hidden; }
        canvas { width: 100%; image-rendering: pixelated; }
        .overlay {
          position: absolute; bottom: 0; left: 0; right: 0;
          background: rgba(0,0,0,0.7); font: 9px monospace; color: #aaa;
          padding: 2px 4px;
        }
      </style>
      <div class="wrap">
        <canvas></canvas>
        <div class="overlay">Waiting for stream...</div>
      </div>
    `;
    this._canvas = this.shadowRoot.querySelector('canvas');
    this._overlay = this.shadowRoot.querySelector('.overlay');
    this._ctx = null;
    this._width = 0;
    this._height = 0;
    this._channels = 4;
    this._row = 0;
    this._totalBytes = 0;
  }

  connectedCallback() {
    this._unsub = this.zeal?.onStreamFrame((payload, meta) => {
      this._handleFrame(payload, meta);
    });

    this._unsubState = this.zeal?.onStreamStateChange((state) => {
      if (state.phase === 'begin' && state.meta) {
        this._initCanvas(state.meta);
      }
      if (state.phase === 'complete') {
        this._overlay.textContent = `${this._width}\u00d7${this._height} \u2022 ${this._formatBytes(this._totalBytes)}`;
      }
    });
  }

  disconnectedCallback() {
    this._unsub?.();
    this._unsubState?.();
  }

  _initCanvas(meta) {
    this._width = meta.width || 256;
    this._height = meta.height || 256;
    this._channels = meta.format === 'Gray8' ? 1 : 4;
    this._canvas.width = this._width;
    this._canvas.height = this._height;
    this._ctx = this._canvas.getContext('2d');
    this._row = 0;
    this._totalBytes = 0;
    this._overlay.textContent = `${this._width}\u00d7${this._height} streaming...`;
  }

  _handleFrame(payload) {
    this._totalBytes += payload.byteLength;

    if (!this._ctx) {
      // Try to infer dimensions from first frame
      if (!this._width) {
        this._width = 256;
        this._height = 256;
        this._canvas.width = this._width;
        this._canvas.height = this._height;
        this._ctx = this._canvas.getContext('2d');
      }
    }

    if (!this._ctx) return;

    const rowBytes = this._width * this._channels;
    const data = new Uint8Array(payload.buffer, payload.byteOffset, payload.byteLength);

    // Process one or more rows per frame
    for (let offset = 0; offset < data.length && this._row < this._height; offset += rowBytes) {
      const rowData = data.subarray(offset, offset + rowBytes);
      const imgData = this._ctx.createImageData(this._width, 1);

      if (this._channels === 4) {
        imgData.data.set(rowData);
      } else if (this._channels === 1) {
        // Gray8 → RGBA
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

    this._overlay.textContent = `Row ${this._row}/${this._height} \u2022 ${this._formatBytes(this._totalBytes)}`;
  }

  _formatBytes(b) {
    if (b < 1024) return b + ' B';
    if (b < 1048576) return (b / 1024).toFixed(1) + ' KB';
    return (b / 1048576).toFixed(1) + ' MB';
  }

  set width(v) { /* Will reinit on stream begin */ }
  set height(v) { /* Will reinit on stream begin */ }
}

customElements.define('reflow-image-preview', ReflowImagePreview);
