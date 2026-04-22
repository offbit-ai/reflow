// reflow-ui.js — Shared component library for Reflow display components
// Provides: ReflowComponent base class, theme tokens, icon SVGs, reusable widgets.
// All display components should import from this via the global `ReflowUI` namespace.

const ReflowUI = (() => {

  // ─── Theme Tokens ──────────────────────────────────────────────────

  const theme = {
    // Backgrounds
    bg:        '#0a0a0a',
    bgSurface: '#111',
    bgElevated:'#1a1a1a',

    // Borders
    border:    '#222',
    borderLight:'#333',

    // Text
    textPrimary:   '#e5e5e5',
    textSecondary: '#999',
    textMuted:     '#666',
    textDim:       '#444',

    // Accent colors
    green:   '#22c55e',
    blue:    '#3b82f6',
    amber:   '#f59e0b',
    red:     '#ef4444',
    purple:  '#a855f7',
    orange:  '#f97316',
    cyan:    '#06b6d4',
    pink:    '#ec4899',

    // Semantic
    gain:      '#f59e0b',
    reduction: '#ef4444',
    signal:    '#22c55e',
    spectrum:  '#22c55e',
    inactive:  '#666',

    // Typography
    fontMono: '10px ui-monospace, "SF Mono", "Cascadia Code", monospace',
    fontSmall: '9px ui-monospace, "SF Mono", "Cascadia Code", monospace',
    fontTiny:  '8px ui-monospace, "SF Mono", "Cascadia Code", monospace',
    fontLabel: '11px ui-monospace, "SF Mono", "Cascadia Code", monospace',

    // Spacing
    pad: '4px',
    padSmall: '2px',
    radius: '4px',
    radiusSmall: '3px',
  };

  // ─── Common CSS ────────────────────────────────────────────────────

  const hostCSS = `
    :host {
      display: flex;
      flex-direction: column;
      flex: 1 1 auto;
      min-width: 0;
      width: 100%;
      color: ${theme.textPrimary};
      font: ${theme.fontMono};
      --rf-bg: ${theme.bg};
      --rf-surface: ${theme.bgSurface};
      --rf-border: ${theme.border};
      --rf-green: ${theme.green};
      --rf-blue: ${theme.blue};
      --rf-amber: ${theme.amber};
      --rf-red: ${theme.red};
      --rf-text: ${theme.textPrimary};
      --rf-text-muted: ${theme.textMuted};
      --rf-radius: ${theme.radius};
    }
  `;

  const canvasCSS = `
    canvas {
      width: 100%;
      border-radius: var(--rf-radius);
      background: var(--rf-bg);
    }
  `;

  const infoCSS = `
    .rf-info {
      font: ${theme.fontSmall};
      color: ${theme.textMuted};
      padding: ${theme.padSmall} ${theme.pad};
    }
  `;

  const gridCSS = `
    .rf-grid {
      display: grid;
      gap: 2px 8px;
      padding: ${theme.pad};
      font: ${theme.fontMono};
    }
    .rf-grid .label { color: ${theme.textMuted}; }
    .rf-grid .value { color: ${theme.green}; text-align: right; }
  `;

  // ─── Tabler Icons (SVG paths, 24x24 viewBox) ──────────────────────

  const icons = {
    activity:     '<path stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M3 12h4l3-9 4 18 3-9h4"/>',
    'volume-2':   '<path stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M15 8a5 5 0 0 1 0 8M17.7 5a9 9 0 0 1 0 14M6 15H4a1 1 0 0 1-1-1v-4a1 1 0 0 1 1-1h2l3.5-4.5A.8.8 0 0 1 11 5v14a.8.8 0 0 1-1.5.5L6 15"/>',
    'volume-x':   '<path stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M6 15H4a1 1 0 0 1-1-1v-4a1 1 0 0 1 1-1h2l3.5-4.5A.8.8 0 0 1 11 5v14a.8.8 0 0 1-1.5.5L6 15M16 9l4 4m0-4-4 4"/>',
    sliders:      '<path stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M7 3v3m0 4v10M12 3v10m0 4v3M17 3v5m0 4v8M4 10h6M9 17h6M14 12h6"/>',
    'bar-chart':  '<path stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M3 12v8h4v-8H3Zm7-8v16h4V4h-4Zm7 4v12h4V8h-4Z"/>',
    'bar-chart-2':'<path stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M3 3v18h18M7 16v-3m4 3V8m4 8v-5m4 5V5"/>',
    zap:          '<path stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M13 3L4 14h7l-1 7 9-11h-7l1-7z"/>',
    sun:          '<circle cx="12" cy="12" r="4" stroke="currentColor" stroke-width="1.5" fill="none"/><path stroke="currentColor" stroke-linecap="round" stroke-width="1.5" d="M12 2v2m0 16v2M4.93 4.93l1.41 1.41m11.32 11.32 1.41 1.41M2 12h2m16 0h2M4.93 19.07l1.41-1.41m11.32-11.32 1.41-1.41"/>',
    image:        '<rect x="3" y="3" width="18" height="18" rx="2" stroke="currentColor" stroke-width="1.5" fill="none"/><circle cx="8.5" cy="8.5" r="1.5" stroke="currentColor" stroke-width="1.5" fill="none"/><path stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="m21 15-3.09-3.09a2 2 0 0 0-2.82 0L6 21"/>',
    scissors:     '<path stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M6 6a3 3 0 1 0 0-6 3 3 0 0 0 0 6Zm0 12a3 3 0 1 0 0 6 3 3 0 0 0 0-6Zm12-6a3 3 0 1 0 0-6 3 3 0 0 0 0 6ZM8.12 8.12 12 12m0 0 3.88 3.88M8.12 15.88 12 12"/>',
    'git-merge':  '<path stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M18 18a3 3 0 1 0 0-6 3 3 0 0 0 0 6ZM6 6a3 3 0 1 0 0-6 3 3 0 0 0 0 6ZM6 21V6m0 6c0-2 2-4 6-4h3"/>',
    shield:       '<path stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/>',
    'minimize-2': '<path stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M4 14h6v6M20 10h-6V4M14 10l7-7M3 21l7-7"/>',
    music:        '<path stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M9 18V5l12-2v13M9 18a3 3 0 1 1-6 0 3 3 0 0 1 6 0Zm12-2a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z"/>',
    radio:        '<path stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M4.9 4.9C1.68 8.13 1.68 13.37 4.9 16.6m12.73.53c3.22-3.23 3.22-8.47 0-11.7M7.76 7.76a6 6 0 0 0 0 8.49m6.48-.01a6 6 0 0 0 0-8.49"/><circle cx="12" cy="12" r="2" fill="currentColor"/>',
    wind:         '<path stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M17.7 7.7a2.5 2.5 0 1 1 1.8 4.3H2m12-6a2.5 2.5 0 1 1 1.8 4.3m-9 3.2a2.5 2.5 0 1 0 1.8 4.3H2"/>',
    layers:       '<path stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="m12 2 10 6.5v7L12 22 2 15.5v-7L12 2Zm0 20v-6.5M22 8.5l-10 7-10-7"/>',
    clock:        '<circle cx="12" cy="12" r="10" stroke="currentColor" stroke-width="1.5" fill="none"/><path stroke="currentColor" stroke-linecap="round" stroke-width="1.5" d="M12 6v6l4 2"/>',
    upload:       '<path stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4m14-7-5-5-5 5m5-5v12"/>',
    download:     '<path stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4m5-3 5 5 5-5m-5 5V3"/>',
  };

  function icon(name, size = 16, color = 'currentColor') {
    const svg = icons[name];
    if (!svg) return '';
    return `<svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" style="color:${color};flex-shrink:0">${svg}</svg>`;
  }

  // ─── Canvas Utilities ──────────────────────────────────────────────

  const canvas = {
    /** Log frequency to pixel X (20Hz–20kHz over width). */
    freqToX(freq, width) {
      return (Math.log10(freq / 20) / Math.log10(1000)) * width;
    },

    /** Pixel X to log frequency. */
    xToFreq(x, width) {
      return 20 * Math.pow(1000, x / width);
    },

    /** dB to pixel Y (centered, ±range). */
    dbToY(db, height, range = 24) {
      return height / 2 - (db / range) * (height / 2);
    },

    /** Pixel Y to dB. */
    yToDb(y, height, range = 24) {
      return -((y - height / 2) / (height / 2)) * range;
    },

    /** Draw frequency grid lines (100, 1k, 10k). */
    drawFreqGrid(ctx, width, height) {
      ctx.strokeStyle = theme.bgElevated;
      ctx.lineWidth = 0.5;
      for (const f of [100, 1000, 10000]) {
        const x = canvas.freqToX(f, width);
        ctx.beginPath(); ctx.moveTo(x, 0); ctx.lineTo(x, height); ctx.stroke();
      }
    },

    /** Draw dB grid lines at ±intervals. */
    drawDbGrid(ctx, width, height, interval = 12) {
      ctx.strokeStyle = theme.bgElevated;
      ctx.lineWidth = 0.5;
      const mid = height / 2;
      ctx.beginPath(); ctx.moveTo(0, mid); ctx.lineTo(width, mid); ctx.stroke();
      for (let db = interval; db <= 24; db += interval) {
        for (const sign of [1, -1]) {
          const y = canvas.dbToY(db * sign, height);
          ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(width, y); ctx.stroke();
        }
      }
    },

    /** Setup canvas for HiDPI. Returns { ctx, width, height }. */
    setupHiDPI(canvasEl, cssHeight) {
      const dpr = window.devicePixelRatio || 1;
      const w = canvasEl.offsetWidth;
      const h = cssHeight;
      canvasEl.width = w * dpr;
      canvasEl.height = h * dpr;
      const ctx = canvasEl.getContext('2d');
      ctx.scale(dpr, dpr);
      return { ctx, width: w, height: h };
    },
  };

  // ─── Meter Widget ──────────────────────────────────────────────────

  /** Render a vertical meter bar into a canvas context. */
  function drawMeter(ctx, x, y, width, height, value, color = theme.green) {
    // Background
    ctx.fillStyle = theme.bgSurface;
    ctx.beginPath();
    ctx.roundRect(x, y, width, height, 2);
    ctx.fill();

    // Fill (from bottom)
    const fillH = Math.min(1, Math.max(0, value)) * height;
    if (fillH > 0) {
      ctx.fillStyle = color;
      ctx.beginPath();
      ctx.roundRect(x, y + height - fillH, width, fillH, 2);
      ctx.fill();
    }
  }

  /** Render a horizontal meter bar. */
  function drawHMeter(ctx, x, y, width, height, value, color = theme.green) {
    ctx.fillStyle = theme.bgSurface;
    ctx.beginPath();
    ctx.roundRect(x, y, width, height, 2);
    ctx.fill();

    const fillW = Math.min(1, Math.max(0, value)) * width;
    if (fillW > 0) {
      ctx.fillStyle = color;
      ctx.beginPath();
      ctx.roundRect(x, y, fillW, height, 2);
      ctx.fill();
    }
  }

  // ─── Format Utilities ──────────────────────────────────────────────

  function formatBytes(b) {
    if (b < 1024) return b + ' B';
    if (b < 1048576) return (b / 1024).toFixed(1) + ' KB';
    if (b < 1073741824) return (b / 1048576).toFixed(1) + ' MB';
    return (b / 1073741824).toFixed(2) + ' GB';
  }

  function formatDb(db) {
    const sign = db >= 0 ? '+' : '';
    return `${sign}${db.toFixed(1)} dB`;
  }

  function formatFreq(hz) {
    if (hz >= 1000) return (hz / 1000).toFixed(1) + ' kHz';
    return Math.round(hz) + ' Hz';
  }

  // ─── Base Component Class ──────────────────────────────────────────

  class ReflowComponent extends HTMLElement {
    constructor() {
      super();
      this.attachShadow({ mode: 'open' });
      this._unsubs = [];
    }

    /** Override to return inner HTML (without <style>). */
    get template() { return ''; }

    /** Override to return additional CSS. */
    get styles() { return ''; }

    /** Override with component-specific connectedCallback logic. */
    onConnect() {}

    /** Override with component-specific disconnectedCallback logic. */
    onDisconnect() {}

    connectedCallback() {
      this.shadowRoot.innerHTML = `
        <style>${hostCSS}${canvasCSS}${infoCSS}${gridCSS}${this.styles}</style>
        ${this.template}
      `;
      this.onConnect();
    }

    disconnectedCallback() {
      this.onDisconnect();
      for (const unsub of this._unsubs) unsub?.();
      this._unsubs = [];
    }

    /** Subscribe to zeal events with automatic cleanup. */
    sub(fn) {
      const unsub = fn?.();
      if (unsub) this._unsubs.push(unsub);
    }

    /** Shorthand for this.shadowRoot.getElementById. */
    $(id) { return this.shadowRoot.getElementById(id); }

    /** Shorthand for this.shadowRoot.querySelector. */
    $q(sel) { return this.shadowRoot.querySelector(sel); }

    /** Read all current properties from zeal. */
    getProps() { return this.zeal?.getProperties() || {}; }
  }

  // ─── Public API ────────────────────────────────────────────────────

  return {
    theme,
    icons,
    icon,
    canvas,
    drawMeter,
    drawHMeter,
    formatBytes,
    formatDb,
    formatFreq,
    hostCSS,
    canvasCSS,
    infoCSS,
    gridCSS,
    ReflowComponent,
  };

})();

// Make globally available for other display components
if (typeof globalThis !== 'undefined') globalThis.ReflowUI = ReflowUI;
