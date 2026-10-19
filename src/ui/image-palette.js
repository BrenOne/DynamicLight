"use strict";
window.ImagePalette = (() => {
  function extract(data) {
    // Quantize a bounded thumbnail, then choose prominent, distinct colors.
    const bins = new Map();
    for (let i = 0; i < data.length; i += 4) {
      if (data[i + 3] < 128) continue;
      const r = data[i],
        g = data[i + 1],
        b = data[i + 2],
        key = (r >> 4) * 256 + (g >> 4) * 16 + (b >> 4);
      const v = bins.get(key) || [0, 0, 0, 0];
      v[0] += r;
      v[1] += g;
      v[2] += b;
      v[3]++;
      bins.set(key, v);
    }
    const ranked = [...bins.values()]
        .sort((a, b) => b[3] - a[3])
        .map((v) => v.slice(0, 3).map((c) => Math.round(c / v[3]))),
      colors = [];
    for (const c of ranked) {
      if (
        colors.every(
          (p) => p.reduce((sum, v, i) => sum + (v - c[i]) ** 2, 0) > 35 ** 2,
        )
      )
        colors.push(c);
      if (colors.length === 8) break;
    }
    if (!colors.length) throw Error("The image has no visible colors.");
    return colors.map(
      (c) => "#" + c.map((v) => v.toString(16).padStart(2, "0")).join(""),
    );
  }
  async function read(file) {
    if (!file || file.size > 20 * 1024 * 1024)
      throw Error("Choose an image smaller than 20 MiB.");
    const bitmap = await createImageBitmap(file);
    try {
      if (bitmap.width * bitmap.height > 40_000_000)
        throw Error("Choose an image of at most 40 megapixels.");
      const canvas = document.createElement("canvas"),
        ratio = Math.min(128 / bitmap.width, 128 / bitmap.height, 1);
      canvas.width = Math.max(1, Math.round(bitmap.width * ratio));
      canvas.height = Math.max(1, Math.round(bitmap.height * ratio));
      const ctx = canvas.getContext("2d", { willReadFrequently: true });
      ctx.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
      return extract(ctx.getImageData(0, 0, canvas.width, canvas.height).data);
    } finally {
      bitmap.close();
    }
  }
  return { read, extract };
})();
