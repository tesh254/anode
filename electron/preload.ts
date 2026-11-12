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
  // Serial ports
  getSerialPorts: () => ipcRenderer.invoke('get-serial-ports'),
  detectCircuitPython: () => ipcRenderer.invoke('detect-circuitpython'),
  circuitPythonDeploy: (opts: { mountPath: string; fileName: string; code: string }) =>
    ipcRenderer.invoke('circuitpython-deploy', opts),
  
  // File system
  openFolder: () => ipcRenderer.invoke('open-folder-dialog'),
  readDirectory: (dirPath: string) => ipcRenderer.invoke('read-directory', dirPath),
  readFile: (filePath: string) => ipcRenderer.invoke('read-file', filePath),
  writeFile: (filePath: string, content: string) => ipcRenderer.invoke('write-file', filePath, content),
  createFile: (dirPath: string, fileName: string, content: string) => 
    ipcRenderer.invoke('create-file', dirPath, fileName, content),
  createDirectory: (dirPath: string) => ipcRenderer.invoke('create-directory', dirPath),
  getFileStats: (filePath: string) => ipcRenderer.invoke('get-file-stats', filePath),
  
  // Code upload/run/stop
  uploadCode: (opts: { portPath: string; fileName: string; code: string }) =>
    ipcRenderer.invoke('upload-code', opts),
  runCode: (opts: { portPath: string; code: string }) =>
    ipcRenderer.invoke('run-code', opts),
  stopCode: (opts: { portPath: string }) =>
    ipcRenderer.invoke('stop-code', opts),
  
  // Firmware
  flashFirmware: (opts: { portPath: string; boardType: string; firmwareUrl: string }) =>
    ipcRenderer.invoke('flash-firmware', opts),
  openFirmwareDialog: (boardType: string) => ipcRenderer.invoke('open-firmware-dialog', boardType),
  
  // Tools
  checkTool: (tool: string) => ipcRenderer.invoke('check-tool', tool),
  installTool: (tool: 'espressif' | 'arduino') => ipcRenderer.invoke('install-tool', tool),
  
  // External
  openUrl: (url: string) => ipcRenderer.invoke('open-url', url),
  
  // Event listeners
  onFlashProgress: (callback: (_event: unknown, message: string) => void) => {
    ipcRenderer.on('flash-progress', callback)
    return () => { ipcRenderer.removeListener('flash-progress', callback) }
  },
  onInstallProgress: (callback: (_event: unknown, data: { tool: string; message: string }) => void) => {
    ipcRenderer.on('install-progress', callback)
    return () => { ipcRenderer.removeListener('install-progress', callback) }
  },

  // REPL
  replConnect: (portPath: string) => ipcRenderer.invoke('repl-connect', portPath),
  replDisconnect: () => ipcRenderer.invoke('repl-disconnect'),
  replSend: (data: string) => ipcRenderer.invoke('repl-send', data),
  replSendBreak: () => ipcRenderer.invoke('repl-send-break'),
  replSendSoftReset: () => ipcRenderer.invoke('repl-send-soft-reset'),
  replIsConnected: () => ipcRenderer.invoke('repl-is-connected'),
  saveReplExport: (content: string, defaultName?: string) =>
    ipcRenderer.invoke('repl-save-export', content, defaultName),
  onReplData: (callback: (_event: unknown, data: string) => void) => {
    ipcRenderer.on('repl-data', callback)
    return () => { ipcRenderer.removeListener('repl-data', callback) }
  },
  onReplDisconnected: (callback: (_event: unknown) => void) => {
    ipcRenderer.on('repl-disconnected', callback)
    return () => { ipcRenderer.removeListener('repl-disconnected', callback) }
  },
  onReplError: (callback: (_event: unknown, error: string) => void) => {
    ipcRenderer.on('repl-error', callback)
    return () => { ipcRenderer.removeListener('repl-error', callback) }
  },

  // Device filesystem (mpremote fs)
  deviceFsLs: (portPath: string, remotePath: string) =>
    ipcRenderer.invoke('device-fs-ls', portPath, remotePath),
  deviceFsCat: (portPath: string, remotePath: string) =>
    ipcRenderer.invoke('device-fs-cat', portPath, remotePath),
  deviceFsPut: (portPath: string, localFilePath: string, remotePath: string) =>
    ipcRenderer.invoke('device-fs-put', portPath, localFilePath, remotePath),
  deviceFsRm: (portPath: string, remotePath: string) =>
    ipcRenderer.invoke('device-fs-rm', portPath, remotePath),

  // Arduino compile/upload
  arduinoCompile: (opts: { sketchPath: string; fqbn: string }) =>
    ipcRenderer.invoke('arduino-compile', opts),
  arduinoUpload: (opts: { portPath: string; sketchPath: string; fqbn: string }) =>
    ipcRenderer.invoke('arduino-upload', opts),
})