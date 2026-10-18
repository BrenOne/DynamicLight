(function (root) {
  'use strict';
  const clamp = (x, min, max) => Math.max(min, Math.min(max, x));
  function hexRGB(hex) { return [1, 3, 5].map(i => parseInt(hex.slice(i, i + 2), 16)); }
  function hsv(h, s = 1, v = 1) {
    return [0, 2 / 3, 1 / 3].map(shift => Math.round(255 * v * (1 - s + s * clamp(Math.abs(((h + shift) % 1) * 6 - 3) - 1, 0, 1))));
  }
  function positions(count, sides, aspect = 16 / 9) {
    const perimeter = 2 + aspect * (sides === 4 ? 2 : 1);
    return Array.from({ length: count }, (_, i) => {
      const p = (i + 0.5) / count * perimeter;
      if (p < 1) return [0, 1 - p];
      if (p < 1 + aspect) return [(p - 1) / aspect, 0];
      if (p < 2 + aspect) return [1, p - 1 - aspect];
      return [1 - (p - 2 - aspect) / aspect, 1];
    });
  }
  function sampleEdges(data, width, height, count, sides) {
    return positions(count, sides, width / height).map(([x, y]) => {
      const cx = clamp(Math.round(x * (width - 1)), 0, width - 1);
      const cy = clamp(Math.round(y * (height - 1)), 0, height - 1);
      const rx = Math.max(1, Math.round(width * 0.035));
      const ry = Math.max(1, Math.round(height * 0.06));
      const rgb = [0, 0, 0]; let n = 0;
      for (let py = Math.max(0, cy - ry); py <= Math.min(height - 1, cy + ry); py += 2) {
        for (let px = Math.max(0, cx - rx); px <= Math.min(width - 1, cx + rx); px += 2) {
          const p = (py * width + px) * 4;
          for (let c = 0; c < 3; c++) rgb[c] += data[p + c];
          n++;
        }
      }
      return rgb.map(c => Math.round(c / n));
    });
  }
  function transform(colors, previous, settings) {
    const blend = settings.smoothing / 100;
    const result = colors.map((rgb, i) => rgb.map((c, channel) => {
      const target = c * settings.brightness / 100;
      return Math.round(previous?.[i] ? previous[i][channel] * blend + target * (1 - blend) : target);
    }));
    return result;
  }
  function reorder(colors, reverse, offset) {
    const arranged = reverse ? [...colors].reverse() : [...colors];
    return arranged.map((_, i) => arranged[(i + offset) % arranged.length]);
  }
  const api = { clamp, hexRGB, hsv, positions, sampleEdges, transform, reorder };
  if (typeof module !== 'undefined') module.exports = api; else root.Lighting = api;
})(typeof window !== 'undefined' ? window : globalThis);
