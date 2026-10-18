const hid = require('node-hid');
const { VID, PID } = require('../src/protocol.cjs');
hid.devicesAsync(VID, PID).then(devices => console.log(JSON.stringify(devices, null, 2))).catch(error => { console.error(error.message); process.exitCode = 1; });
