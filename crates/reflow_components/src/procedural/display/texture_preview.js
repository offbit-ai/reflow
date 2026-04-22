// reflow-texture-preview: Renders texture thumbnail from actor metadata.
// The actor produces a base64 PNG thumbnail in its output metadata.

class ReflowTexturePreview extends ReflowUI.ReflowComponent {
  get styles() {
    const T = ReflowUI.theme;
    return `
      .wrap { position: relative; border-radius: ${T.radius}; overflow: hidden; background: ${T.bgSurface}; }
      img { width: 100%; display: block; border-radius: ${T.radius}; image-rendering: auto; }
      .overlay { position: absolute; bottom: 0; left: 0; right: 0;
        background: linear-gradient(transparent, rgba(0,0,0,0.8));
        font: ${T.fontSmall}; color: ${T.textSecondary};
        padding: 12px ${T.pad} ${T.padSmall}; display: flex; justify-content: space-between; }
      .mapping { color: ${T.cyan}; }
      .placeholder { width: 100%; height: 64px; display: flex; align-items: center;
        justify-content: center; color: ${T.textDim}; font: ${T.fontSmall}; }
    `;
  }

  get template() {
    return `
      <div class="wrap">
        <div class="placeholder" id="ph">${ReflowUI.icon('image', 20, ReflowUI.theme.textDim)} Awaiting texture...</div>
        <img id="img" style="display:none" />
        <div class="overlay" style="display:none" id="ov">
          <span id="dims"></span>
          <span class="mapping" id="mode"></span>
        </div>
      </div>
    `;
  }

  onConnect() {
    this.sub(() => this.zeal?.onPropertyChange((values) => {
      if (values.thumbnail) this._showThumbnail(values);
      if (values.textureWidth) this._updateInfo(values);
    }));

    const props = this.getProps();
    if (props.thumbnail) this._showThumbnail(props);
  }

  _showThumbnail(props) {
    const img = this.$('img');
    img.src = props.thumbnail;
    img.style.display = 'block';
    this.$('ph').style.display = 'none';
    this.$('ov').style.display = 'flex';
    this._updateInfo(props);
  }

  _updateInfo(props) {
    if (props.textureWidth && props.textureHeight) {
      this.$('dims').textContent = `${props.textureWidth}\u00d7${props.textureHeight}`;
    }
    this.$('mode').textContent = props.mapping || '';
  }

  set scale(v) { this.$('mode').textContent = `triplanar \u00d7${v}`; }
}

customElements.define('reflow-texture-preview', ReflowTexturePreview);
