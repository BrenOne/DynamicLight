const test = require('node:test');
const assert = require('node:assert/strict');
const { command, solid, frame, reports } = require('../src/protocol.cjs');
const { validate } = require('../src/settings.cjs');
const L = require('../src/lighting.js');

test('initialization commands match observed wire packets', () => {
  assert.equal(command(2, 0x82).toString('hex'), '52420602821e');
  assert.equal(command(3, 0x93, [0]).toString('hex'), '52420703930031');
});
test('solid covers requested LEDs and clears remaining physical LEDs', () => {
  const p = solid(1, [255, 0, 128], 65);
  assert.deepEqual([...p.subarray(5, -1)], [1, 255, 0, 128, 65, 66, 0, 0, 0, 254]);
  assert.equal(p[2], 16);
  assert.equal(p.at(-1), [...p.subarray(0, -1)].reduce((a, b) => a + b, 0) % 256);
});
test('65-LED frame uses 332 bytes and survives 64-byte HID chunking', () => {
  const p = frame(7, Array.from({ length: 65 }, (_, i) => [i, 255 - i, 128]));
  assert.equal(p.length, 332); assert.equal(p.readUInt16BE(2), 332);
  assert.deepEqual([...p.subarray(5, 10)], [128, 0, 0, 255, 128]);
  assert.deepEqual([...p.subarray(325, 330)], [64, 64, 64, 191, 128]);
  assert.equal(p[330], 65);
  const chunks = reports(p);
  assert.equal(chunks.length, 6);
  assert.ok(chunks.every(report => report.length === 65 && report[0] === 0));
  assert.deepEqual(Buffer.concat(chunks.map(report => report.subarray(1))).subarray(0, 332), p);
  assert.ok(chunks.at(-1).subarray(13).every(value => value === 0));
});
test('rejects invalid colors/counts/settings before USB writes', () => {
  for (const count of [0, 128, 2.5, NaN]) assert.throws(() => solid(1, [1, 2, 3], count));
  assert.throws(() => solid(1, [-1, 0, 0], 65));
  assert.throws(() => frame(1, [[NaN, 2, 3]]));
  assert.throws(() => validate({ brightness: 101 }));
  assert.throws(() => validate({ mode: 'anything' }));
  assert.throws(() => validate({ offOnQuit: 'false' }));
});
test('screen edge sampling honors spatial colors and three-sided path', () => {
  const w = 160, h = 90, data = new Uint8ClampedArray(w * h * 4);
  for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
    const i = (y * w + x) * 4; data[i] = x < w / 2 ? 255 : 0; data[i + 2] = x >= w / 2 ? 255 : 0; data[i + 3] = 255;
  }
  const colors = L.sampleEdges(data, w, h, 65, 3);
  assert.deepEqual(colors[0], [255, 0, 0]); assert.deepEqual(colors.at(-1), [0, 0, 255]);
  const points = L.positions(65, 3);
  assert.equal(points[0][0], 0); assert.equal(points.at(-1)[0], 1);
});
test('brightness, smoothing, reverse, and offset compose predictably', () => {
  const colors = [[100, 200, 50], [200, 100, 0]];
  assert.deepEqual(L.transform(colors, null, { brightness: 50, smoothing: 65 }), [[50, 100, 25], [100, 50, 0]]);
  assert.deepEqual(L.transform(colors, colors, { brightness: 0, smoothing: 0 }), [[0, 0, 0], [0, 0, 0]]);
  assert.deepEqual(L.reorder([[1], [2], [3]], true, 1), [[2], [1], [3]]);
});
