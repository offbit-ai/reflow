// reflow-texture-preview: Texture image thumbnail on the node
// Renders the output metadata (dimensions, mapping type, vertex count).
// The actual texture is Message::Bytes, not a stream — we show stats
// and a colored indicator based on metadata from the actor output.

class ReflowTexturePreview extends ReflowUI.ReflowComponent {
  get styles() {
    const T = ReflowUI.theme;
    return `
      .wrap { padding: ${T.pad}; }
      .row { display: flex; justify-content: space-between; margin: 1px 0; }
      .label { color: ${T.textMuted}; }
      .value { color: ${T.green}; }
      .mapping { color: ${T.cyan}; font-weight: bold; }
      .bar { height: 4px; background: ${T.bgSurface}; border-radius: 2px; margin-top: 4px; overflow: hidden; }
      .bar-fill { height: 100%; border-radius: 2px; background: linear-gradient(90deg, ${T.cyan}, ${T.purple}); transition: width 200ms; }
    `;
  }

  get template() {
    return `
      <div class="wrap">
        <div class="row">
          <span class="label">${ReflowUI.icon('image', 12)} Texture</span>
          <span class="mapping" id="mode">-</span>
        </div>
        <div class="row">
          <span class="label">Size</span>
          <span class="value" id="dims">-</span>
        </div>
        <div class="row">
          <span class="label">Vertices</span>
          <span class="value" id="verts">-</span>
        </div>
        <div class="bar"><div class="bar-fill" id="bar" style="width:0%"></div></div>
      </div>
    `;
  }

  onConnect() {
    const props = this.getProps();
    this._updateFromProps(props);

    this.sub(() => this.zeal?.onPropertyChange((values) => {
      this._updateFromProps(values);
    }));
  }

  _updateFromProps(props) {
    if (props.textureWidth && props.textureHeight) {
      this.$('dims').textContent = `${props.textureWidth}×${props.textureHeight}`;
    }
    if (props.mapping) {
      this.$('mode').textContent = props.mapping;
    }
    if (props.vertexCount) {
      this.$('verts').textContent = Number(props.vertexCount).toLocaleString();
      this.$('bar').style.width = '100%';
    }

    // Show scale/sharpness for triplanar
    if (props.scale !== undefined) {
      this.$('mode').textContent = `triplanar ×${props.scale}`;
    }
  }

  set scale(v) { this.$('mode').textContent = `triplanar ×${v}`; }
  set sharpness(v) { /* reflected */ }
}

customElements.define('reflow-texture-preview', ReflowTexturePreview);
