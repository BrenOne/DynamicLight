const test = require('node:test');
const assert = require('node:assert/strict');
const { EventEmitter } = require('node:events');
const { Controller } = require('../src/controller.cjs');

function fakeHid() {
  const writes = []; let device;
  const hid = {
    devicesAsync: async () => [{ path: 'keyboard', interface: 1, usagePage: 1 }, { path: 'lighting', interface: 0, product: 'Test strip' }],
    HIDAsync: { open: async path => {
      assert.equal(path, 'lighting'); device = new EventEmitter();
      device.write = async bytes => { writes.push(bytes); return bytes.length; };
      device.close = async () => {}; return device;
    } }
  };
  return { hid, writes, device: () => device };
}
test('opens lighting interface, sends complete frames, recovers from unplug', async () => {
  const fake = fakeHid(), c = new Controller(fake.hid);
  assert.equal((await c.connect()).connected, true);
  assert.equal(fake.writes.length, 3);
  assert.equal(await c.frame(Array.from({ length: 65 }, () => [1, 2, 3])), true);
  assert.equal(fake.writes.length, 9);
  fake.device().emit('error', new Error('Unplugged'));
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(c.status.connected, false);
  assert.equal((await c.connect()).connected, true);
  await c.disconnect();
});
test('short writes disconnect instead of reporting success', async () => {
  const fake = fakeHid(), c = new Controller(fake.hid);
  await c.connect(); fake.device().write = async () => 4;
  assert.equal(await c.solid([1, 2, 3], 65), false);
  assert.equal(c.status.connected, false);
});
test('does not open keyboard-only devices', async () => {
  let opened = false;
  const c = new Controller({ devicesAsync: async () => [{ path: 'keyboard', interface: 1, usagePage: 1 }], HIDAsync: { open: async () => { opened = true; } } });
  await c.connect(); assert.equal(opened, false); assert.equal(c.status.connected, false);
});
