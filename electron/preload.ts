import { contextBridge, ipcRenderer } from 'electron'

// --------- Expose some API to the Renderer process ---------
contextBridge.exposeInMainWorld('ipcRenderer', {
  on(...args: Parameters<typeof ipcRenderer.on>) {
    const [channel, listener] = args
    return ipcRenderer.on(channel, (event, ...args) => listener(event, ...args))
  },
  off(...args: Parameters<typeof ipcRenderer.off>) {
    const [channel, ...omit] = args
    return ipcRenderer.off(channel, ...omit)
  },
  send(...args: Parameters<typeof ipcRenderer.send>) {
    const [channel, ...omit] = args
    return ipcRenderer.send(channel, ...omit)
  },
  invoke(...args: Parameters<typeof ipcRenderer.invoke>) {
    const [channel, ...omit] = args
    return ipcRenderer.invoke(channel, ...omit)
  },
})

contextBridge.exposeInMainWorld('hardware', {
  getSerialPorts: () => ipcRenderer.invoke('get-serial-ports'),
  flashFirmware: (opts: { portPath: string; boardType: string; firmwareUrl: string }) =>
    ipcRenderer.invoke('flash-firmware', opts),
  checkTool: (tool: string) => ipcRenderer.invoke('check-tool', tool),
  installTool: (tool: 'espressif' | 'arduino') => ipcRenderer.invoke('install-tool', tool),
  openFirmwareDialog: (boardType: string) => ipcRenderer.invoke('open-firmware-dialog', boardType),
  openUrl: (url: string) => ipcRenderer.invoke('open-url', url),
  onFlashProgress: (callback: (_event: unknown, message: string) => void) => {
    ipcRenderer.on('flash-progress', callback)
    return () => { ipcRenderer.removeListener('flash-progress', callback) }
  },
  onInstallProgress: (callback: (_event: unknown, data: { tool: string; message: string }) => void) => {
    ipcRenderer.on('install-progress', callback)
    return () => { ipcRenderer.removeListener('install-progress', callback) }
  },
})