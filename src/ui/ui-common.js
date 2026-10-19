"use strict";
// Shared, local-only presentation for the main and Quick-controls windows.
window.LightUI = (() => {
  const svgNS = "http://www.w3.org/2000/svg";
  function icon(name) {
    const svg = document.createElementNS(svgNS, "svg");
    svg.classList.add("icon");
    svg.setAttribute("viewBox", "0 0 24 24");
    svg.setAttribute("aria-hidden", "true");
    const use = document.createElementNS(svgNS, "use");
    use.setAttribute("href", `icons.svg#${name}`);
    svg.append(use);
    return svg;
  }
  async function version() {
    const value = await window.__TAURI__.core.invoke("app_version");
    document.querySelectorAll("[data-app-version]").forEach((el) => {
      el.textContent = `v${value}`;
      el.setAttribute("aria-label", `DynamicLight version ${value}`);
    });
  }
  function status(s) {
    if (s.calibrating) return "Calibrating";
    if (s.pauseReason) return "Paused";
    if (s.recovering) return "Reconnecting";
    if (!s.status.connected) return "Not connected";
    if (!s.requested || !s.powered) return "Stopped";
    if (s.captureRequiresStart && !s.capturing) return "Capture paused";
    if (!s.settings.brightness) return "Brightness 0%";
    if (["screen", "music", "blend"].includes(s.settings.mode) && !s.capturing)
      return "Capture paused";
    return "Sending";
  }
  function matchesScene(profile, snapshot) {
    if (!snapshot || snapshot.calibrating) return false;
    const fields = {
      solid: ["color"],
      rainbow: [
        "effect",
        "palette",
        "customPalette",
        "color",
        "speed",
        "effectReverse",
      ],
      music: [
        "musicStyle",
        "palette",
        "customPalette",
        "color",
        "audioSource",
        "inputGain",
        "sensitivity",
        "noiseFloor",
        "release",
        "speed",
      ],
      screen: ["screenId", "excludeBars", "rgbBalance", "saturation", "gamma"],
      blend: [
        "screenId",
        "audioSource",
        "blend",
        "inputGain",
        "sensitivity",
        "noiseFloor",
        "release",
        "excludeBars",
        "rgbBalance",
        "saturation",
        "gamma",
      ],
    };
    const s = {
      ...profile.settings,
      screenId: profile.source?.screenId ?? profile.settings.screenId ?? "",
    };
    return ["mode", "brightness", "smoothing", ...(fields[s.mode] || [])].every(
      (key) =>
        JSON.stringify(s[key]) === JSON.stringify(snapshot.settings[key]),
    );
  }
  let themeCache, themeDark;
  function theme() {
    const dark = matchMedia("(prefers-color-scheme: dark)").matches;
    if (!themeCache || dark !== themeDark) {
      let probe = document.querySelector(".theme-probe");
      if (!probe) {
        probe = document.createElement("span");
        probe.className = "theme-probe";
        probe.setAttribute("aria-hidden", "true");
        document.body.append(probe);
      }
      const style = getComputedStyle(probe);
      themeCache = {
        stage: style.backgroundColor,
        line: style.borderTopColor,
        muted: style.caretColor,
        text: style.outlineColor,
        track: style.color,
      };
      themeDark = dark;
    }
    return themeCache;
  }
  function surface(canvas) {
    const width = canvas.clientWidth;
    const height = canvas.clientHeight;
    if (!width || !height) return null;
    const scale = Math.min(window.devicePixelRatio || 1, 2);
    const w = Math.round(width * scale),
      h = Math.round(height * scale);
    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w;
      canvas.height = h;
    }
    const ctx = canvas.getContext("2d");
    ctx.setTransform(scale, 0, 0, scale, 0, 0);
    ctx.clearRect(0, 0, width, height);
    return { ctx, width, height };
  }
  function pattern(canvas, colors) {
    const target = surface(canvas);
    if (!target || !colors?.length) return;
    const { ctx, width: w, height: h } = target;
    const c = theme();
    const at = (u) => [
      w * (0.09 + 0.82 * u),
      h * (0.55 - 0.16 * Math.sin(u * Math.PI * 2 - 0.65)),
    ];
    ctx.beginPath();
    for (let i = 0; i <= 80; i++) {
      const [x, y] = at(i / 80);
      if (i) ctx.lineTo(x, y);
      else ctx.moveTo(x, y);
    }
    ctx.strokeStyle = c.track;
    ctx.lineWidth = 5;
    ctx.lineCap = "round";
    ctx.stroke();
    colors.forEach((rgb, i) => {
      const u = (i + 0.5) / colors.length,
        [x, y] = at(u),
        [nx, ny] = at(u + 0.002);
      const lit = rgb.some((v) => v > 0);
      ctx.save();
      ctx.translate(x, y);
      ctx.rotate(Math.atan2(ny - y, nx - x));
      ctx.fillStyle = lit ? `rgb(${rgb.join(",")})` : c.track;
      ctx.shadowColor = lit ? ctx.fillStyle : "transparent";
      ctx.shadowBlur = lit ? 11 : 0;
      ctx.fillRect(-2.2, -2, 4.4, 4);
      ctx.shadowBlur = 0;
      if (lit) {
        ctx.globalAlpha = (Math.max(...rgb) / 255) * 0.4;
        ctx.fillStyle = "#ffffff";
        ctx.fillRect(-0.55, -0.6, 1.1, 1.2);
      }
      ctx.restore();
    });
  }
  function strip(canvas, colors, s, powered, physicalTest = false) {
    const target = surface(canvas);
    if (!target) return;
    const { ctx, width, height } = target;
    const count = colors.length || s.count;
    ctx.fillStyle = theme().track;
    ctx.fillRect(0, 0, width, height);
    colors.forEach((_, i) => {
      const j = (i + (physicalTest ? 0 : s.offset)) % count;
      const rgb = powered
        ? colors[s.reverse && !physicalTest ? count - 1 - j : j]
        : [0, 0, 0];
      ctx.fillStyle = `rgb(${rgb.join(",")})`;
      ctx.fillRect(
        (i * width) / count,
        0,
        Math.max(1, width / count - 1),
        height,
      );
    });
  }
  function monitor(canvas, colors, s, powered) {
    const target = surface(canvas);
    if (!target) return;
    const { ctx, width: w, height: h } = target,
      c = theme();
    // Fit the monitor and its stand together. Independent width/height fractions
    // flattened the monitor when the window was maximized on a wide display.
    const aspect = 16 / 9,
      vh = Math.min(h * 0.68, (w * 0.82) / aspect),
      vw = vh * aspect,
      left = (w - vw) / 2,
      right = left + vw,
      top = (h - vh * 1.16) / 2,
      bottom = top + vh;
    ctx.fillStyle = c.stage;
    ctx.fillRect(0, 0, w, h);
    ctx.strokeStyle = c.track;
    ctx.lineWidth = 5;
    ctx.lineJoin = "round";
    ctx.beginPath();
    ctx.moveTo(left, bottom);
    ctx.lineTo(left, top);
    ctx.lineTo(right, top);
    ctx.lineTo(right, bottom);
    if (s.sides === 4) ctx.closePath();
    ctx.stroke();
    const perimeter = 2 + aspect * (s.sides === 4 ? 2 : 1);
    colors.forEach((color, i) => {
      const p = ((i + 0.5) / colors.length) * perimeter;
      const [x, y] =
        p < 1
          ? [0, 1 - p]
          : p < 1 + aspect
            ? [(p - 1) / aspect, 0]
            : p < 2 + aspect
              ? [1, p - 1 - aspect]
              : [1 - (p - 2 - aspect) / aspect, 1];
      const rgb = powered ? color : [0, 0, 0],
        lit = rgb.some((v) => v > 0);
      ctx.fillStyle = lit ? `rgb(${rgb.join(",")})` : c.track;
      ctx.shadowColor = ctx.fillStyle;
      ctx.shadowBlur = lit ? 17 : 0;
      const radius = Math.min(3, (vw / Math.max(colors.length, 32)) * 0.5);
      ctx.fillRect(
        left + x * vw - radius,
        top + y * vh - radius,
        radius * 2,
        radius * 2,
      );
      ctx.shadowBlur = 0;
    });
    ctx.fillStyle = c.track;
    ctx.fillRect(w / 2 - vw * 0.018, bottom + 6, vw * 0.036, vh * 0.1);
    ctx.beginPath();
    ctx.roundRect(w / 2 - vw * 0.11, bottom + vh * 0.1 + 6, vw * 0.22, 3, 1.5);
    ctx.fill();
  }
  return {
    icon,
    version,
    status,
    matchesScene,
    surface,
    theme,
    pattern,
    strip,
    monitor,
  };
})();
