const { contextBridge, ipcRenderer } = require('electron');
contextBridge.exposeInMainWorld('light', {
  init: () => ipcRenderer.invoke('init'),
  connect: () => ipcRenderer.invoke('connect'),
  disconnect: () => ipcRenderer.invoke('disconnect'),
  settings: value => ipcRenderer.invoke('settings', value),
  solid: rgb => ipcRenderer.invoke('solid', rgb),
  frame: colors => ipcRenderer.invoke('frame', colors),
  screens: () => ipcRenderer.invoke('screens'),
  prepareCapture: options => ipcRenderer.invoke('prepare-capture', options),
  onStatus: callback => ipcRenderer.on('status', (_, data) => callback(data)),
  onPause: callback => ipcRenderer.on('pause', (_, data) => callback(data)),
  onResume: callback => ipcRenderer.on('resume', () => callback())
});
