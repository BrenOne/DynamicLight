'use strict';
const defaults = Object.freeze({
  color: '#9b72ff', brightness: 38, count: 65, mode: 'solid',
  sides: 3, reverse: false, offset: 0, smoothing: 65,
  audioSource: 'system', sensitivity: 50, fps: 30, screenId: '',
  offOnSleep: true, offOnQuit: true
});
function validate(input) {
  if (!input || typeof input !== 'object' || Array.isArray(input)) throw new Error('Invalid settings');
  const result = { ...defaults };
  const ranges = { brightness: [0, 100], count: [1, 127], sides: [3, 4], offset: [0, 126], smoothing: [0, 95], sensitivity: [1, 100], fps: [10, 30] };
  for (const key of Object.keys(defaults)) {
    if (!(key in input)) continue;
    const value = input[key];
    if (ranges[key]) {
      const [min, max] = ranges[key];
      if (!Number.isInteger(value) || value < min || value > max) throw new Error(`Invalid ${key}`);
    } else if (typeof defaults[key] === 'boolean') {
      if (typeof value !== 'boolean') throw new Error(`Invalid ${key}`);
    } else if (key === 'color') {
      if (typeof value !== 'string' || !/^#[0-9a-f]{6}$/i.test(value)) throw new Error('Invalid color');
    } else if (key === 'mode') {
      if (!['solid', 'screen', 'music', 'rainbow'].includes(value)) throw new Error('Invalid mode');
    } else if (key === 'audioSource') {
      if (!['system', 'microphone'].includes(value)) throw new Error('Invalid audio source');
    } else if (typeof value !== 'string' || value.length > 200) throw new Error(`Invalid ${key}`);
    result[key] = value;
  }
  return result;
}
module.exports = { defaults, validate };
