'use strict';

const VID = 0x1a86;
const PID = 0xfe07;
const byte = n => Number.isInteger(n) && n >= 0 && n <= 255;
function checkRGB(rgb) {
  if (!Array.isArray(rgb) || rgb.length !== 3 || !rgb.every(byte)) throw new Error('Invalid RGB color');
}
function checksum(data) { return data.reduce((a, b) => a + b, 0) & 255; }
function command(id, action, payload = []) {
  if (!byte(id) || !byte(action) || !payload.every(byte) || payload.length > 249) throw new Error('Invalid command');
  const data = Buffer.from([0x52, 0x42, payload.length + 6, id, action, ...payload, 0]);
  data[data.length - 1] = checksum(data.subarray(0, -1));
  return data;
}
function solid(id, rgb, count) {
  checkRGB(rgb);
  checkCount(count);
  // RB section commands use one-based inclusive ranges; clear unused LEDs too.
  return command(id, 0x86, [1, ...rgb, count, count + 1, 0, 0, 0, 254]);
}
function checkCount(count) {
  if (!Number.isInteger(count) || count < 1 || count > 127) throw new Error('LED count must be 1–127');
}
function frame(id, colors) {
  checkCount(colors.length);
  colors.forEach(checkRGB);
  const data = Buffer.alloc(7 + colors.length * 5);
  data[0] = 0x53; data[1] = 0x43;
  data.writeUInt16BE(data.length, 2);
  data[4] = id;
  colors.forEach((rgb, i) => {
    const offset = 5 + i * 5;
    data[offset] = i === 0 ? 0x80 : i;
    data[offset + 1] = i;
    data.set(rgb, offset + 2);
  });
  data[data.length - 2] = colors.length;
  data[data.length - 1] = checksum(data.subarray(0, -1));
  return data;
}
function reports(packet) {
  const chunks = [];
  for (let offset = 0; offset < packet.length; offset += 64) {
    const report = Buffer.alloc(65);
    packet.copy(report, 1, offset, offset + 64);
    chunks.push(report);
  }
  return chunks;
}
module.exports = { VID, PID, command, solid, frame, reports, checkRGB, checkCount };
