import { app, BrowserWindow, ipcMain, shell } from 'electron'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { execFile, spawn } from 'node:child_process'
import { existsSync, copyFileSync, readFileSync, writeFileSync, readdirSync, statSync, mkdirSync, unlinkSync } from 'node:fs'
import { SerialPort } from 'serialport'
import { getMpremoteInvocation, getArduinoCliPath } from './main/utils/paths'
import {
  parseBootOut,
  needsManualReset as computeNeedsManualReset,
  flashAttempts,
  rp2040MountPoints,
  chipTypeFromFlashId,
  chipTypeFromVid,
  flashOffset as computeFlashOffset,
  isEsptoolConnectionError,
} from './main/utils/flashing'

const __dirname = path.dirname(fileURLToPath(import.meta.url))
process.env.APP_ROOT = path.join(__dirname, '..')

export const VITE_DEV_SERVER_URL = process.env['VITE_DEV_SERVER_URL']
export const MAIN_DIST = path.join(process.env.APP_ROOT, 'dist-electron')
export const RENDERER_DIST = path.join(process.env.APP_ROOT, 'dist')

process.env.VITE_PUBLIC = VITE_DEV_SERVER_URL ? path.join(process.env.APP_ROOT, 'public') : RENDERER_DIST

// Set application name for macOS menu bar
app.setName('Anode')

// Get icon path based on environment
function getIconPath(): string | undefined {
  if (VITE_DEV_SERVER_URL) {
    // Development: use public folder directly
    const appRoot = process.env.APP_ROOT ?? ''
    const devIcon = path.join(appRoot, 'public', 'app-icon', 'macos', '512x512.png')
    return existsSync(devIcon) ? devIcon : undefined
  } else {
    // Production: use bundled resources
    const resourcesPath = process.resourcesPath
    if (!resourcesPath) return undefined
    if (process.platform === 'darwin') {
      return path.join(resourcesPath, 'app-icon', 'macos', '512x512.png')
    } else if (process.platform === 'win32') {
      return path.join(resourcesPath, 'app-icon', 'windows', '256x256.png')
    }
    return path.join(resourcesPath, 'app-icon', 'linux', 'icons', '256x256.png')
  }
}

let win: BrowserWindow | null

function createWindow() {
  const iconPath = getIconPath()
  
  win = new BrowserWindow({
    width: 1200,
    height: 800,
    title: 'Anode',
    icon: iconPath,
    webPreferences: {
      preload: path.join(__dirname, 'preload.mjs'),
      contextIsolation: true,
      nodeIntegration: false,
    },
  })

  // Test active push message to Renderer-process.
  win.webContents.on('did-finish-load', () => {
    win?.webContents.send('main-process-message', (new Date).toLocaleString())
  })

  if (VITE_DEV_SERVER_URL) {
    win.loadURL(VITE_DEV_SERVER_URL)
  } else {
    win.loadFile(path.join(RENDERER_DIST, 'index.html'))
  }
}

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit()
  }
})

app.on('activate', () => {
  if (BrowserWindow.getAllWindows().length === 0) {
    createWindow()
  }
})

app.whenReady().then(createWindow)

// IPC Handlers

ipcMain.handle('open-url', async (_event, url: string) => {
  await shell.openExternal(url)
})

ipcMain.handle('open-firmware-dialog', async (_event, boardType: string) => {
  const { dialog } = await import('electron')
  const filters: Electron.FileFilter[] = boardType === 'raspberry-pi'
    ? [{ name: 'UF2 Firmware', extensions: ['uf2'] }]
    : [{ name: 'Binary Firmware', extensions: ['bin', 'elf'] }]

  const result = await dialog.showOpenDialog(win!, {
    title: 'Select Firmware File',
    properties: ['openFile'],
    filters,
  })

  if (result.canceled || result.filePaths.length === 0) {
    return null
  }
  return result.filePaths[0]
})

const COMMON_TOOL_PATHS: Record<string, string[]> = {
  'esptool.py': [
    '/usr/local/bin/esptool.py',
    '/usr/bin/esptool.py',
    '/opt/homebrew/bin/esptool.py',
    `${process.env.HOME}/Library/Python/3.11/bin/esptool`,
    `${process.env.HOME}/Library/Python/3.12/bin/esptool`,
    `${process.env.HOME}/Library/Python/3.13/bin/esptool`,
  ],
  'arduino-cli': [
    '/usr/local/bin/arduino-cli',
    '/usr/bin/arduino-cli',
    '/opt/homebrew/bin/arduino-cli',
    `${process.env.HOME}/.arduino-cli/bin/arduino-cli`,
    `${process.env.HOME}/.local/bin/arduino-cli`,
  ],
}

async function checkToolInPaths(tool: string): Promise<boolean> {
  const extraPaths = COMMON_TOOL_PATHS[tool] ?? []
  const fs = await import('node:fs')
  for (const p of extraPaths) {
    if (fs.existsSync(p)) return true
  }
  return false
}

function toolAvailable(tool: string): Promise<{ available: boolean }> {
  return new Promise((resolve) => {
    let settled = false
    const finish = (result: { available: boolean }) => {
      if (!settled) { settled = true; resolve(result) }
    }

    // Strategy 1: try python3 -c "import <tool>" (works for pip-installed Python packages)
    if (tool === 'esptool.py') {
      const child = spawn('python3', ['-c', 'import esptool; print(esptool.__version__)'], { shell: false })
      child.on('close', (code) => {
        if (code === 0) { finish({ available: true }); return }
        tryNext()
      })
      child.on('error', () => tryNext())
      return
    }

    // Strategy 2: try running arduino-cli directly
    const child2 = spawn(tool, ['version'], { shell: false })
    child2.on('close', (code) => {
      if (code === 0) { finish({ available: true }); return }
      tryNext2()
    })
    child2.on('error', () => tryNext2())

    function tryNext() {
      // Strategy 2: check common filesystem paths
      checkToolInPaths(tool).then((found) => finish({ available: found }))
    }

    function tryNext2() {
      checkToolInPaths(tool).then((found) => finish({ available: found }))
    }
  })
}

// Check if a required tool is available in PATH
ipcMain.handle('check-tool', async (_event, tool: string) => {
  return toolAvailable(tool)
})

type ToolType = 'espressif' | 'arduino'

ipcMain.handle('install-tool', async (_event, tool: ToolType) => {
  const sendProgress = (message: string) => {
    win?.webContents.send('install-progress', { tool, message })
  }

  const { existsSync, mkdirSync, chmodSync, createWriteStream, unlinkSync } = await import('node:fs')
  const https = await import('node:https')
  const http = await import('node:http')

  return new Promise((resolve) => {
    if (tool === 'espressif') {
      type PipCmd = [string, string[]]
      const pipCommands: PipCmd[] = process.platform === 'win32'
        ? [['pip', ['install', 'esptool']], ['python', ['-m', 'pip', 'install', 'esptool']]]
        : [
            ['python3', ['-m', 'pip', 'install', '--user', 'esptool']],
            ['python3', ['-m', 'pip', 'install', '--user', '--break-system-packages', 'esptool']],
            ['pip3', ['install', '--user', 'esptool']]
          ]

      sendProgress(`Installing esptool...`)
      tryRunInstall(0)

      function tryRunInstall(index: number) {
        if (index >= pipCommands.length) {
          sendProgress('[ERROR] Could not find pip. Install Python 3 from python.org or via brew install python3')
          resolve({ success: false, error: 'No Python pip found', hint: 'Install Python 3 from https://python.org or run: brew install python3' })
          return
        }

        const [pipCmd, pipArgs]: PipCmd = pipCommands[index]
        sendProgress(`Trying: ${pipCmd} ${pipArgs.join(' ')}`)

        const child = spawn(pipCmd, pipArgs, { shell: false })

        child.stdout?.on('data', (data: Buffer) => {
          sendProgress(data.toString().trim())
        })

        child.stderr?.on('data', (data: Buffer) => {
          const line = data.toString().trim()
          if (line) sendProgress(line)
        })

        child.on('close', (code) => {
          if (code === 0) {
            sendProgress('[OK] esptool installed successfully')
            resolve({ success: true })
          } else {
            // Try next command
            tryRunInstall(index + 1)
          }
        })

        child.on('error', (error: NodeJS.ErrnoException) => {
          if (error.code === 'ENOENT') {
            // pip not found at this path, try next
            tryRunInstall(index + 1)
          } else {
            sendProgress(`[ERROR] ${error.message}`)
            resolve({ success: false, error: error.message })
          }
        })
      }
      return
    }

    if (tool === 'arduino') {
      const platform = process.platform === 'win32' ? 'Windows_64bit' : process.platform === 'darwin' ? 'macOS_64bit' : 'Linux_64bit'
      const ext = '.tar.gz'
      const version = '1.0.4'
      const filename = `arduino-cli_${version}_${platform}${ext}`
      const url = `https://downloads.arduino.cc/arduino-cli/${filename}`
      const binDir = path.join(app.getPath('home'), '.arduino-cli', 'bin')
      const binName = process.platform === 'win32' ? 'arduino-cli.exe' : 'arduino-cli'
      const destPath = path.join(app.getPath('temp'), filename)

      sendProgress(`Downloading arduino-cli fromarduino.cc...`)
      sendProgress(`Installing to ${binDir}...`)

      if (!existsSync(binDir)) mkdirSync(binDir, { recursive: true })

      const file = createWriteStream(destPath)
      const protocol = url.startsWith('https') ? https : http

      const doExtract = () => {
        sendProgress('Extracting archive...')
        const extractCmd = process.platform === 'win32'
          ? `tar -xf "${destPath}" -C "${binDir}"`
          : `tar -xzf "${destPath}" -C "${binDir}"`

        execFile(process.platform === 'win32' ? 'cmd' : '/bin/sh', process.platform === 'win32' ? ['/c', extractCmd] : ['-c', extractCmd], (err) => {
          try { unlinkSync(destPath) } catch { /* ignore error */ }
          if (err) {
            sendProgress(`[ERROR] Extraction failed: ${err.message}`)
            resolve({ success: false, error: err.message })
            return
          }
          chmodSync(path.join(binDir, binName), 0o755)
          sendProgress(`[OK] arduino-cli installed to ${binDir}`)
          sendProgress(`Add "${binDir}" to your PATH environment variable.`)
          resolve({ success: true, installPath: binDir })
        })
      }

      protocol.get(url, (response) => {
        if (response.statusCode === 302 || response.statusCode === 301) {
          const redirectUrl = response.headers.location
          if (redirectUrl) {
            file.close()
            protocol.get(redirectUrl, (res) => {
              const outFile = createWriteStream(destPath)
              res.pipe(outFile)
              outFile.on('finish', () => { outFile.close(); doExtract() })
            }).on('error', (error: NodeJS.ErrnoException) => {
              sendProgress(`[ERROR] Download failed: ${error.message}`)
              resolve({ success: false, error: error.message })
            })
            return
          }
        }
        response.pipe(file)
        file.on('finish', () => { file.close(); doExtract() })
      }).on('error', (error: NodeJS.ErrnoException) => {
        sendProgress(`[ERROR] Download failed: ${error.message}`)
        resolve({ success: false, error: error.message })
      })
    }
  })
})
ipcMain.handle('get-serial-ports', async () => {
  try {
    const ports = await SerialPort.list()
    return ports
  } catch (error) {
    console.error('Failed to list serial ports', error)
    return []
  }
})

// readdir that never throws (missing dir / no permission) — returns [].
function safeReaddir(dir: string): string[] {
  try {
    return readdirSync(dir)
  } catch {
    return []
  }
}

// Detect a connected CircuitPython board by its hallmark: a mounted USB drive
// named CIRCUITPY containing boot_out.txt (board name + CP version). This is the
// universal CircuitPython signal across all chips (Adafruit, RP2040, ESP32-S3) —
// far more reliable than the USB descriptor, which a bare ESP32-S3 shares with
// MicroPython. Returns the first such drive found (single-board workflow).
ipcMain.handle('detect-circuitpython', async () => {
  // Candidate mount roots per OS. macOS mounts volumes flat under /Volumes;
  // Linux distros use /media/<user>/<label> or /run/media/<user>/<label>.
  const candidates: string[] = []
  if (process.platform === 'darwin') {
    candidates.push('/Volumes/CIRCUITPY')
  } else if (process.platform === 'linux') {
    for (const root of ['/media', '/run/media']) {
      if (!existsSync(root)) continue
      for (const user of safeReaddir(root)) {
        candidates.push(path.join(root, user, 'CIRCUITPY'))
      }
    }
  }
  // Windows: enumerating drive letters by volume label needs extra tooling;
  // left as a follow-up. (Detection still works via the Adafruit 239a VID.)

  for (const mountPath of candidates) {
    const bootOut = path.join(mountPath, 'boot_out.txt')
    if (!existsSync(bootOut)) continue
    try {
      const { boardName, version } = parseBootOut(readFileSync(bootOut, 'utf-8'))
      return { present: true, mountPath, boardName, version }
    } catch {
      return { present: true, mountPath }
    }
  }
  return { present: false }
})

// Deploy a file to a CircuitPython board by copying it onto the mounted CIRCUITPY
// drive. This is CircuitPython's native workflow: writing a file triggers the
// board's auto-reload, which re-runs code.py — no serial protocol, no port
// handoff. Program output is read separately over the serial REPL.
ipcMain.handle('circuitpython-deploy', async (_event, { mountPath, fileName, code }: {
  mountPath: string
  fileName: string
  code: string
}) => {
  try {
    // Guard against writing to the wrong place if the drive ejected between the
    // detection probe and this call — confirm it's really a CIRCUITPY drive.
    if (!mountPath || !existsSync(mountPath) || !existsSync(path.join(mountPath, 'boot_out.txt'))) {
      return { success: false, error: 'CIRCUITPY drive not mounted (reconnect the board)' }
    }
    // Disallow path traversal — write only to the drive root.
    const safeName = path.basename(fileName)
    const dest = path.join(mountPath, safeName)
    writeFileSync(dest, code, 'utf-8')
    return { success: true, path: dest }
  } catch (e: unknown) {
    const msg = e instanceof Error ? e.message : String(e)
    return { success: false, error: msg }
  }
})

// File system handlers
ipcMain.handle('open-folder-dialog', async () => {
  const { dialog } = await import('electron')
  const result = await dialog.showOpenDialog(win!, {
    title: 'Open Project Folder',
    properties: ['openDirectory'],
  })
  if (result.canceled || result.filePaths.length === 0) {
    return null
  }
  return result.filePaths[0]
})

ipcMain.handle('read-directory', async (_event, dirPath: string) => {
  try {
    const entries = readdirSync(dirPath, { withFileTypes: true })
    return entries.map(entry => ({
      name: entry.name,
      path: path.join(dirPath, entry.name),
      isDirectory: entry.isDirectory(),
      isFile: entry.isFile(),
    }))
  } catch (error) {
    console.error('Failed to read directory:', error)
    return []
  }
})

ipcMain.handle('read-file', async (_event, filePath: string) => {
  try {
    const content = readFileSync(filePath, 'utf-8')
    const ext = path.extname(filePath).toLowerCase()
    let language = 'python'
    if (ext === '.ino') language = 'arduino'
    return { content, language }
  } catch (error) {
    console.error('Failed to read file:', error)
    return { content: null, language: 'python' }
  }
})

ipcMain.handle('write-file', async (_event, filePath: string, content: string) => {
  try {
    writeFileSync(filePath, content, 'utf-8')
    return { success: true }
  } catch (error) {
    const errorMsg = error instanceof Error ? error.message : String(error)
    console.error('Failed to write file:', error)
    return { success: false, error: errorMsg }
  }
})

ipcMain.handle('create-file', async (_event, dirPath: string, fileName: string, content: string) => {
  try {
    mkdirSync(dirPath, { recursive: true })
    const filePath = path.join(dirPath, fileName)
    writeFileSync(filePath, content, 'utf-8')
    return { success: true, path: filePath }
  } catch (error) {
    const errorMsg = error instanceof Error ? error.message : String(error)
    console.error('Failed to create file:', error)
    return { success: false, error: errorMsg }
  }
})

ipcMain.handle('create-directory', async (_event, dirPath: string) => {
  try {
    mkdirSync(dirPath, { recursive: true })
    return { success: true }
  } catch (error) {
    const errorMsg = error instanceof Error ? error.message : String(error)
    console.error('Failed to create directory:', error)
    return { success: false, error: errorMsg }
  }
})

ipcMain.handle('get-file-stats', async (_event, filePath: string) => {
  try {
    const stats = statSync(filePath)
    return {
      size: stats.size,
      modifiedTime: stats.mtime.getTime(),
      isDirectory: stats.isDirectory(),
      isFile: stats.isFile(),
    }
  } catch {
    return null
  }
})

// Upload a Python file to the board via mpremote or ampy
ipcMain.handle('upload-code', async (_event, { portPath, fileName, code }: {
  portPath: string
  fileName: string
  code: string
}) => {
  const sendProgress = (message: string) => {
    // eslint-disable-next-line no-control-regex
    const clean = message.replace(/\x1b\[\d+(;\d+)*[a-zA-Z]/g, '')
    win?.webContents.send('flash-progress', clean)
  }

  const { tmpdir } = await import('node:os')
  const tmpFile = path.join(tmpdir(), `anode-upload-${Date.now()}-${fileName}`)
  writeFileSync(tmpFile, code, 'utf-8')

  // Try mpremote first, then ampy
  const tryMpremote = (): Promise<{ success: boolean; error?: string }> => {
    return new Promise((resolve) => {
      sendProgress(`Uploading ${fileName} via mpremote to ${portPath}...`)
      const proc = spawn('python3', ['-m', 'mpremote', 'connect', portPath, 'cp', tmpFile, `:${fileName}`])
      let output = ''
      let errOutput = ''
      proc.stdout.on('data', (d) => { output += d.toString(); sendProgress(d.toString().trim()) })
      proc.stderr.on('data', (d) => { errOutput += d.toString(); sendProgress(d.toString().trim()) })
      proc.on('close', (exitCode) => {
        if (exitCode === 0) {
          sendProgress(`[OK] Uploaded ${fileName}`)
          resolve({ success: true })
        } else {
          resolve({ success: false, error: errOutput || output })
        }
      })
      proc.on('error', () => {
        resolve({ success: false, error: 'mpremote not found' })
      })
    })
  }

  const tryAmpy = (): Promise<{ success: boolean; error?: string }> => {
    return new Promise((resolve) => {
      sendProgress(`Uploading ${fileName} via ampy to ${portPath}...`)
      const proc = spawn('python3', ['-m', 'ampy', '--port', portPath, 'put', tmpFile, `/${fileName}`])
      let output = ''
      let errOutput = ''
      proc.stdout.on('data', (d) => { output += d.toString(); sendProgress(d.toString().trim()) })
      proc.stderr.on('data', (d) => { errOutput += d.toString(); sendProgress(d.toString().trim()) })
      proc.on('close', (exitCode) => {
        if (exitCode === 0) {
          sendProgress(`[OK] Uploaded ${fileName}`)
          resolve({ success: true })
        } else {
          resolve({ success: false, error: errOutput || output })
        }
      })
      proc.on('error', () => {
        resolve({ success: false, error: 'ampy not found' })
      })
    })
  }

  // Preferred path: if the interactive REPL is already connected, write the file
  // straight through that open port via raw REPL. No port handoff means no
  // DTR/RTS reset and no contention — this sidesteps the "could not enter raw
  // repl" failure that mpremote hits on a busy, auto-resetting ESP32-S3.
  if (replPort && replPort.isOpen) {
    const replResult = await uploadViaRepl(fileName, code, sendProgress)
    if (replResult.success) return replResult
    sendProgress(`[WARN] REPL upload failed (${replResult.error}); falling back to mpremote...`)
  }

  // Fallback: REPL not connected (or REPL upload failed). mpremote needs
  // exclusive access to the serial port, so pause any REPL, then reconnect.
  return withReplPaused(async () => {
    try {
      let mpResult = await tryMpremote()
      // ESP32-S3 native USB re-enumerates when the REPL port closes; the first
      // mpremote attempt can hit a board still settling ("could not enter raw
      // repl"). Give it a moment and try once more before falling back.
      if (!mpResult.success && /raw repl/i.test(mpResult.error ?? '')) {
        sendProgress('[WARN] Board still settling — retrying mpremote once...')
        await new Promise((res) => setTimeout(res, 600))
        mpResult = await tryMpremote()
      }
      if (mpResult.success) return mpResult

      sendProgress('[WARN] mpremote failed, trying ampy...')
      const ampyResult = await tryAmpy()
      if (ampyResult.success) return ampyResult

      sendProgress('[ERROR] Neither mpremote nor ampy could upload the file.')
      sendProgress('[WARN] Install mpremote: python3 -m pip install mpremote')
      return { success: false, error: 'No upload tool available. Install mpremote: python3 -m pip install mpremote' }
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e)
      sendProgress(`[ERROR] Upload failed: ${msg}`)
      return { success: false, error: msg }
    }
  })
})

// Run a Python file on the board via mpremote
ipcMain.handle('run-code', async (_event, { portPath, code }: {
  portPath: string
  code: string
}) => {
  const sendProgress = (message: string) => {
    // eslint-disable-next-line no-control-regex
    const clean = message.replace(/\x1b\[\d+(;\d+)*[a-zA-Z]/g, '')
    win?.webContents.send('flash-progress', clean)
  }

  // If the interactive REPL holds the serial port, spawning mpremote would
  // fail to enter raw REPL (port is busy). Instead, run the code through the
  // existing connection using MicroPython paste mode (Ctrl-E … code … Ctrl-D).
  // Output streams back via the normal `repl-data` handler.
  if (replPort && replPort.isOpen) {
    const port = replPort
    return new Promise<{ success: boolean; error?: string; viaRepl?: boolean }>((resolve) => {
      sendProgress('Running code via REPL...')
      const write = (data: string) => new Promise<void>((res, rej) => {
        port.write(data, (err) => (err ? rej(err) : res()))
      })
      const delay = (ms: number) => new Promise((res) => setTimeout(res, ms))
      ;(async () => {
        try {
          await write('\r\x03\x03') // interrupt anything running
          await delay(200) // let the board settle at a clean prompt
          await write('\x05') // enter paste mode (Ctrl-E)
          await delay(200) // wait for the "paste mode" prompt before sending code
          // Paste mode preserves blank lines and indentation; send code as-is.
          await write(code.replace(/\r\n/g, '\n'))
          await write('\r\n\x04') // newline + Ctrl-D to execute
          resolve({ success: true, viaRepl: true })
        } catch (e: unknown) {
          const msg = e instanceof Error ? e.message : String(e)
          sendProgress(`[ERROR] ${msg}`)
          resolve({ success: false, error: msg, viaRepl: true })
        }
      })()
    })
  }

  const { tmpdir } = await import('node:os')
  const tmpFile = path.join(tmpdir(), `anode-run-${Date.now()}.py`)
  writeFileSync(tmpFile, code, 'utf-8')

  return new Promise<{ success: boolean; error?: string }>((resolve) => {
    sendProgress('Running code on device...')
    const proc = spawn('python3', ['-m', 'mpremote', 'connect', portPath, 'run', tmpFile])
    let output = ''
    let errOutput = ''
    proc.stdout.on('data', (d) => { output += d.toString(); sendProgress(d.toString().trim()) })
    proc.stderr.on('data', (d) => { errOutput += d.toString(); sendProgress(d.toString().trim()) })
    proc.on('close', (exitCode) => {
      if (exitCode === 0) {
        sendProgress('[OK] Code execution finished')
        resolve({ success: true })
      } else {
        sendProgress(`[ERROR] ${errOutput || output}`)
        resolve({ success: false, error: errOutput || output || 'Run failed' })
      }
    })
    proc.on('error', () => {
      sendProgress('[ERROR] mpremote not found. Install: python3 -m pip install mpremote')
      resolve({ success: false, error: 'mpremote not found. Install: python3 -m pip install mpremote' })
    })
  })
})

// Stop running code on the board
ipcMain.handle('stop-code', async (_event, { portPath }: { portPath: string }) => {
  const sendProgress = (message: string) => {
    win?.webContents.send('flash-progress', message)
  }

  return new Promise<{ success: boolean; error?: string }>((resolve) => {
    sendProgress('Sending soft reset (Ctrl+C) to device...')
    const proc = spawn('python3', ['-m', 'mpremote', 'connect', portPath, 'soft-reset'])
    proc.on('close', () => {
      sendProgress('[OK] Device reset')
      resolve({ success: true })
    })
    proc.on('error', () => {
      sendProgress('[ERROR] mpremote not found')
      resolve({ success: false, error: 'mpremote not found' })
    })
  })
})

// "1200-baud touch": briefly open the port at 1200 baud and drop DTR/RTS, then
// close. Native-USB boards running CircuitPython or recent MicroPython (the
// RTS/DTR bootloader-jump fix) listen for this and reboot themselves into the
// ROM/UF2 download bootloader — so flashing needs no physical BOOT button. A
// blank board (nothing listening) is unaffected; the manual BOOT+RESET is then
// still required for that first flash. Never rejects — flashing proceeds regardless.
function bootloaderTouch1200(portPath: string): Promise<void> {
  return new Promise((resolve) => {
    let settled = false
    const done = () => { if (!settled) { settled = true; resolve() } }
    // Safety net: never hang the flash if a serialport callback doesn't fire.
    const guard = setTimeout(done, 2500)
    try {
      const port = new SerialPort({ path: portPath, baudRate: 1200, autoOpen: false })
      port.open((err) => {
        if (err) { clearTimeout(guard); done(); return }
        port.set({ dtr: false, rts: false }, () => {
          setTimeout(() => {
            port.close(() => { clearTimeout(guard); done() })
          }, 250)
        })
      })
    } catch {
      clearTimeout(guard)
      done()
    }
  })
}

// Reboot an RP2040 (Pico) into its BOOTSEL UF2 bootloader by sending
// machine.bootloader() over the serial REPL — so the RPI-RP2 drive appears
// without holding the physical BOOTSEL button. Works when the board is running
// MicroPython; returns false (caller falls back to the manual button) otherwise.
function enterRp2040Bootloader(portPath: string): Promise<boolean> {
  return new Promise((resolve) => {
    let settled = false
    const done = (ok: boolean) => { if (!settled) { settled = true; resolve(ok) } }
    const guard = setTimeout(() => done(false), 3000)
    try {
      const port = new SerialPort({ path: portPath, baudRate: 115200, autoOpen: false })
      port.open((err) => {
        if (err) { clearTimeout(guard); done(false); return }
        port.write('\r\x03\x03', () => { // Ctrl-C ×2: interrupt anything running
          setTimeout(() => {
            port.write('import machine\r\nmachine.bootloader()\r\n', () => {
              // The board reboots immediately; the port vanishes, so a close
              // error here is expected — treat the command as delivered.
              setTimeout(() => {
                try { port.close(() => {}) } catch { /* port already gone */ }
                clearTimeout(guard)
                done(true)
              }, 200)
            })
          }, 150)
        })
      })
    } catch {
      clearTimeout(guard)
      done(false)
    }
  })
}

ipcMain.handle('flash-firmware', async (_event, { portPath, boardType, firmwareUrl, vendorId, productId }: {
  portPath: string
  boardType: 'espressif' | 'raspberry-pi' | 'arduino' | 'm5stack-atom' | 'm5stamp-s3'
  firmwareUrl: string
  vendorId?: string
  productId?: string
}) => {
  // Send progress updates to the renderer
  const sendProgress = (message: string) => {
    // Strip ANSI escape codes (like [1;31m) so they don't break the UI
    // eslint-disable-next-line no-control-regex
    const cleanMessage = message.replace(/\x1b\[\d+(;\d+)*[a-zA-Z]/g, '');
    win?.webContents.send('flash-progress', cleanMessage)
  }

  try {
    sendProgress(`Starting ${boardType} firmware flash on ${portPath}...`)

    // esptool/arduino-cli need exclusive access to the serial port. If the
    // interactive REPL still holds it (renderer state can be out of sync),
    // close it here — the authoritative place — before flashing. The board
    // reboots into fresh firmware afterwards, so we don't reconnect.
    if (replPort && replPort.isOpen) {
      sendProgress('Releasing REPL serial port for flashing...')
      await new Promise<void>((res) => replPort!.close(() => { replPort = null; res() }))
      await new Promise((res) => setTimeout(res, 300))
    }

    // Auto-download firmware if requested
    let finalFirmwareUrl = firmwareUrl;
    if (finalFirmwareUrl === 'auto') {
      sendProgress('Auto-detecting and downloading latest firmware...');
      const https = await import('node:https');
      const { createWriteStream } = await import('node:fs');
      const { tmpdir } = await import('node:os');
      
      // ESP chips are flashed by esptool, which needs the raw .bin image — NOT
      // the .uf2 (a drag-and-drop bootloader container) that micropython.org now
      // lists first for S3. RP2040 boards use .uf2. Pick the right one.
      const getMicroPythonUrl = (pageUrl: string, preferExt: '.bin' | '.uf2' = '.bin'): Promise<string> => {
        return new Promise((resolve, reject) => {
          https.get(pageUrl, (res) => {
            let data = '';
            res.on('data', chunk => data += chunk);
            res.on('end', () => {
              const links = [...data.matchAll(/href="(\/resources\/firmware\/[^"]+)"/g)].map((m) => m[1]);
              // Prefer the requested extension; for .bin exclude OTA .app-bin.
              const pick = links.find((l) => l.endsWith(preferExt) && !l.endsWith('.app-bin'))
                ?? links.find((l) => l.endsWith(preferExt))
                ?? links[0];
              if (pick) {
                resolve(`https://micropython.org${pick}`);
              } else {
                reject(new Error('Could not find firmware link on ' + pageUrl));
              }
            });
          }).on('error', reject);
        });
      };

      const downloadFile = (url: string, ext: string): Promise<string> => {
        return new Promise((resolve, reject) => {
          const dest = path.join(tmpdir(), `firmware-${Date.now()}${ext}`);
          const file = createWriteStream(dest);
          https.get(url, (res) => {
            if (res.statusCode === 301 || res.statusCode === 302) {
              https.get(res.headers.location!, (res2) => {
                res2.pipe(file);
                file.on('finish', () => { file.close(); resolve(dest); });
              }).on('error', reject);
            } else {
              res.pipe(file);
              file.on('finish', () => { file.close(); resolve(dest); });
            }
          }).on('error', reject);
        });
      };

      if (boardType === 'm5stack-atom') {
        return new Promise((resolve) => {
          (async () => {
            try {
              const chipType = 'ESP32_GENERIC';
              const flashOffset = '0x1000';
              const url = await getMicroPythonUrl(`https://micropython.org/download/${chipType}/`);
              sendProgress(`M5Stack Atom Light → downloading ${url}...`);
              finalFirmwareUrl = await downloadFile(url, '.bin');
              sendProgress(`Downloaded to ${finalFirmwareUrl}`);
              sendProgress('Running esptool via python3...');
              const tryFlash = (baudRate: string, onDone: (ok: boolean) => void) => {
                execFile('python3', [
                  '-m', 'esptool', '--port', portPath, '--baud', baudRate,
                  '--before', 'default_reset', '--after', 'hard_reset',
                  'write_flash', '-z', flashOffset, finalFirmwareUrl,
                ], (error, out, stderr) => {
                  if (error) {
                    const errMsg = error.message.replace(/\x1b\[\d+(;\d+)*[a-zA-Z]/g, '');
                    const dropped = errMsg.includes('chip stopped responding') || errMsg.includes('StopIteration') || errMsg.includes('Failed to connect');
                    if (dropped && baudRate !== '115200') {
                      sendProgress(`[WARN] Chip dropped at ${baudRate} baud — retrying at 115200...`);
                      tryFlash('115200', onDone);
                      return;
                    }
                    sendProgress(`[ERROR] esptool failed: ${errMsg}`);
                    const isConnectionError = errMsg.includes('No serial data received') || errMsg.includes('Failed to connect') || errMsg.includes('chip stopped responding');
                    resolve({
                      success: false,
                      error: errMsg.trim(),
                      hint: isConnectionError
                        ? 'Hold the side button (G39) while plugging in, or use a data+power USB-C cable.'
                        : 'Install esptool: pip install esptool',
                    });
                    onDone(false);
                    return;
                  }
                  sendProgress(out.trim());
                  if (stderr) sendProgress(stderr.trim());
                  sendProgress('[OK] M5Stack Atom Light firmware flashed successfully');
                  resolve({ success: true });
                  onDone(true);
                });
              };
              tryFlash('460800', () => {});
            } catch (e: unknown) {
              const errorMsg = e instanceof Error ? e.message : String(e);
              sendProgress(`[ERROR] Auto-download failed: ${errorMsg}`);
              resolve({ success: false, error: errorMsg });
            }
          })();
        });
      } else if (boardType === 'm5stamp-s3') {
        return new Promise((resolve) => {
          (async () => {
            try {
              const chipType = 'ESP32_GENERIC_S3';
              const flashOffset = '0x0';
              const url = await getMicroPythonUrl(`https://micropython.org/download/${chipType}/`);
              sendProgress(`M5StampS3 → downloading ${url}...`);
              finalFirmwareUrl = await downloadFile(url, '.bin');
              sendProgress(`Downloaded to ${finalFirmwareUrl}`);
              sendProgress('Running esptool via python3...');
              execFile('python3', [
                '-m', 'esptool', '--port', portPath, '--baud', '460800',
                '--before', 'default_reset', '--after', 'hard_reset',
                'write_flash', '-z', flashOffset, finalFirmwareUrl,
              ], (error, out, stderr) => {
                if (error) {
                  const errMsg = error.message.replace(/\x1b\[\d+(;\d+)*[a-zA-Z]/g, '');
                  sendProgress(`[ERROR] esptool failed: ${errMsg}`);
                  const isConnectionError = errMsg.includes('No serial data received') || errMsg.includes('Failed to connect');
                  resolve({ success: false, error: errMsg.trim(), hint: isConnectionError ? 'Hold BOOT button while connecting.' : 'Install esptool: pip install esptool' });
                  return;
                }
                sendProgress(out.trim());
                if (stderr) sendProgress(stderr.trim());
                sendProgress('[OK] M5StampS3 firmware flashed successfully');
                resolve({ success: true });
              });
            } catch (e: unknown) {
              const errorMsg = e instanceof Error ? e.message : String(e);
              sendProgress(`[ERROR] Auto-download failed: ${errorMsg}`);
              resolve({ success: false, error: errorMsg });
            }
          })();
        });
      } else if (boardType === 'espressif') {
        return new Promise((resolve) => {
          // If VID is 303a (Espressif native USB), this is an ESP32-S3
          const vid = (vendorId ?? '').toLowerCase()
          const needsManualReset = computeNeedsManualReset(vendorId, portPath)

          // Download the right firmware variant for `chipType` and write it.
          // On native-USB boards we first do a 1200-baud touch so the board
          // reboots itself into the download bootloader — no BOOT button needed.
          const doFlash = async (chipType: string) => {
            const flashOffset = computeFlashOffset(chipType);
            try {
              const url = await getMicroPythonUrl(`https://micropython.org/download/${chipType}/`);
              sendProgress(`Downloading ${url}...`);
              finalFirmwareUrl = await downloadFile(url, '.bin');
              sendProgress(`Downloaded to ${finalFirmwareUrl}`);

              if (needsManualReset) {
                sendProgress('Entering bootloader via 1200-baud touch (no BOOT button needed if the board runs CircuitPython / recent MicroPython)...');
                await bootloaderTouch1200(portPath);
                await new Promise((res) => setTimeout(res, 1500)); // let it re-enumerate into download mode
              }

              sendProgress('Running esptool via python3...');
              // After a touch the board is already in download mode, so try
              // no_reset first; otherwise let esptool drive the reset (works for
              // both classic DTR/RTS boards and USB-Serial/JTAG chips).
              const attempts = flashAttempts(needsManualReset)
              let attemptIdx = 0
              const runAttempt = () => {
                const { baud, before } = attempts[attemptIdx]
                sendProgress(`Flashing (baud ${baud}, reset: ${before})...`)
                execFile('python3', [
                  '-m', 'esptool',
                  '--port', portPath,
                  '--baud', baud,
                  '--before', before,
                  '--after', 'hard_reset',
                  'write_flash', '-z', flashOffset,
                  finalFirmwareUrl,
                ], (error, out, stderr) => {
                  if (error) {
                    // eslint-disable-next-line no-control-regex
                    const errMsg = error.message.replace(/\x1b\[\d+(;\d+)*[a-zA-Z]/g, '');
                    const isConnectionError = isEsptoolConnectionError(errMsg);
                    attemptIdx++
                    if (isConnectionError && attemptIdx < attempts.length) {
                      const next = attempts[attemptIdx]
                      sendProgress(`[WARN] Connect failed — retrying (baud ${next.baud}, reset: ${next.before})...`)
                      runAttempt();
                      return;
                    }
                    sendProgress(`[ERROR] esptool failed: ${errMsg}`);
                    const hint = isConnectionError
                      ? 'If this is a blank board (or recovery), hold BOOT, press RESET, release BOOT once, then flash again.'
                      : 'Install esptool: pip install esptool';
                    resolve({ success: false, error: errMsg.trim(), hint });
                    return;
                  }
                  sendProgress(out.trim());
                  if (stderr) sendProgress(stderr.trim());
                  sendProgress('[OK] Espressif firmware flashed successfully');
                  resolve({ success: true });
                });
              };
              runAttempt();
            } catch (e: unknown) {
              const errorMsg = e instanceof Error ? e.message : String(e);
              sendProgress(`[ERROR] Auto-download failed: ${errorMsg}`);
              resolve({ success: false, error: errorMsg });
            }
          };

          if (needsManualReset) {
            // The flash_id probe would reset the board back out of download mode,
            // and esptool re-detects the chip during write_flash anyway — so skip
            // it here and trust the VID (303a → ESP32-S3).
            const chipType = chipTypeFromVid(vid) ?? 'ESP32_GENERIC_S3'
            sendProgress(`Native USB Espressif (VID ${vid || 'usbmodem'}) → ${chipType}`)
            doFlash(chipType)
          } else {
            sendProgress('Detecting Espressif chip type...');
            execFile('python3', ['-m', 'esptool', '--port', portPath, 'flash_id'], (flashIdErr, stdout, stderr) => {
              const chipType = flashIdErr ? 'ESP32_GENERIC' : chipTypeFromFlashId(`${stdout}\n${stderr}`)
              if (flashIdErr) sendProgress(`[WARN] flash_id failed: ${flashIdErr.message}`)
              doFlash(chipType)
            });
          }
        });
      } else if (boardType === 'raspberry-pi') {
        const url = await getMicroPythonUrl('https://micropython.org/download/RPI_PICO/', '.uf2');
        sendProgress(`Downloading ${url}...`);
        finalFirmwareUrl = await downloadFile(url, '.uf2');
        sendProgress(`Downloaded to ${finalFirmwareUrl}`);
      } else if (boardType === 'arduino') {
        return new Promise((resolve) => {
          sendProgress('Auto-detecting Arduino board and compiling blink sketch...');
          import('node:fs').then(fs => {
            const sketchDir = path.join(tmpdir(), `Blink-${Date.now()}`);
            fs.mkdirSync(sketchDir);
            fs.writeFileSync(path.join(sketchDir, 'Blink.ino'), `
void setup() { pinMode(LED_BUILTIN, OUTPUT); }
void loop() { digitalWrite(LED_BUILTIN, HIGH); delay(1000); digitalWrite(LED_BUILTIN, LOW); delay(1000); }
            `);
            
            execFile('arduino-cli', ['board', 'list'], (_err, stdout) => {
              const lines = stdout.split('\n');
              const portLine = lines.find(l => l.includes(portPath));
              let fqbn = 'arduino:avr:uno'; // default
              if (portLine) {
                const match = portLine.match(/([a-zA-Z0-9]+:[a-zA-Z0-9]+:[a-zA-Z0-9_]+)/);
                if (match) fqbn = match[1];
              }
              sendProgress(`Detected FQBN: ${fqbn}`);
              
              execFile('arduino-cli', ['compile', '-b', fqbn, sketchDir], (cerr) => {
                if (cerr) {
                  sendProgress(`[ERROR] Compile failed: ${cerr.message}`);
                  resolve({ success: false, error: cerr.message, hint: `You may need to run 'arduino-cli core install ${fqbn.split(':').slice(0,2).join(':')}'` });
                  return;
                }
                sendProgress('Compiled successfully. Uploading...');
                execFile('arduino-cli', ['upload', '-p', portPath, '-b', fqbn, sketchDir], (uerr) => {
                  if (uerr) {
                    sendProgress(`[ERROR] Upload failed: ${uerr.message}`);
                    resolve({ success: false, error: uerr.message });
                    return;
                  }
                  sendProgress('[OK] Arduino firmware flashed successfully');
                  resolve({ success: true });
                });
              });
            });
          });
        });
      }
    }

    switch (boardType) {
      case 'espressif': {
        // python3 -m esptool --port <port> --baud 460800 write_flash -z 0x1000 <firmware.bin>
        return new Promise((resolve) => {
          sendProgress('Running esptool via python3...')
          execFile('python3', [
            '-m', 'esptool',
            '--port', portPath,
            '--baud', '460800',
            '--before', 'default_reset',
            '--after', 'hard_reset',
            'write_flash', '-z', '0x1000',
            finalFirmwareUrl,
          ], (error, stdout, stderr) => {
            if (error) {
              // eslint-disable-next-line no-control-regex
              const errMsg = error.message.replace(/\x1b\[\d+(;\d+)*[a-zA-Z]/g, '');
              sendProgress(`[ERROR] esptool failed: ${errMsg}`)
              const isConnectionError = errMsg.includes('No serial data received') || errMsg.includes('Failed to connect');
              const hint = isConnectionError 
                ? 'Failed to connect. Try holding the BOOT button on your board while it connects.'
                : 'Install esptool: pip install esptool';
              resolve({ success: false, error: errMsg.trim(), hint })
              return
            }
            sendProgress(stdout.trim())
            if (stderr) sendProgress(stderr.trim())
            sendProgress('[OK] Espressif firmware flashed successfully')
            resolve({ success: true })
          })
        })
      }

      case 'raspberry-pi': {
        // For RP2040/Pico: the board exposes its UF2 bootloader as the RPI-RP2
        // USB drive; we copy the .uf2 onto it. If the drive isn't already mounted
        // we first try machine.bootloader() over serial so no BOOTSEL button is
        // needed, then poll for the drive to appear.
        return new Promise((resolve) => {
          (async () => {
            // macOS mounts at /Volumes/RPI-RP2; Linux at /media|/run/media/<user>/RPI-RP2
            const mountPoints = rp2040MountPoints(process.platform, process.env.USER)
            const findMount = () => mountPoints.find((mp) => existsSync(mp))

            sendProgress('Looking for RPI-RP2 boot volume...')
            let dest = findMount()

            if (!dest && portPath && /tty|cu\.|COM/i.test(portPath)) {
              sendProgress('Rebooting Pico into BOOTSEL via machine.bootloader() (no button needed if it runs MicroPython)...')
              const sent = await enterRp2040Bootloader(portPath)
              if (!sent) {
                sendProgress('[WARN] Could not reach a MicroPython REPL — if the drive does not appear, hold BOOTSEL while plugging in.')
              }
              // Wait for USB re-enumeration into mass-storage mode (~up to 10s).
              for (let i = 0; i < 20 && !dest; i++) {
                await new Promise((res) => setTimeout(res, 500))
                dest = findMount()
              }
            }

            if (!dest) {
              sendProgress('[ERROR] RPI-RP2 volume not found. Hold BOOTSEL while plugging in the board.')
              resolve({ success: false, error: 'RPI-RP2 boot volume not found', hint: 'Hold BOOTSEL button while connecting the board via USB' })
              return
            }

            const filename = path.basename(finalFirmwareUrl)
            const targetPath = path.join(dest, filename)
            try {
              copyFileSync(finalFirmwareUrl, targetPath)
              sendProgress(`[OK] Copied ${filename} to ${dest}`)
              sendProgress('[OK] Raspberry Pi Pico firmware flashed successfully')
              resolve({ success: true })
            } catch (copyError: unknown) {
              const errorMsg = copyError instanceof Error ? copyError.message : String(copyError);
              sendProgress(`[ERROR] Failed to copy firmware: ${errorMsg}`)
              resolve({ success: false, error: errorMsg })
            }
          })()
        })
      }

      case 'arduino': {
        // arduino-cli upload -p <port> --fqbn <board> --input-file <firmware>
        return new Promise((resolve) => {
          sendProgress('Running arduino-cli...')
          execFile('arduino-cli', [
            'upload',
            '-p', portPath,
            '--input-file', finalFirmwareUrl,
          ], (error, stdout, stderr) => {
            if (error) {
              sendProgress(`[ERROR] arduino-cli failed: ${error.message}`)
              resolve({ success: false, error: error.message, hint: 'Install arduino-cli: https://arduino.github.io/arduino-cli/' })
              return
            }
            sendProgress(stdout)
            if (stderr) sendProgress(stderr)
            sendProgress('[OK] Arduino firmware flashed successfully')
            resolve({ success: true })
          })
        })
      }

      default:
        return { success: false, error: `Unknown board type: ${boardType}` }
    }
  } catch (error: unknown) {
    const errorMsg = error instanceof Error ? error.message : String(error);
    sendProgress(`[ERROR] ${errorMsg}`)
    return { success: false, error: errorMsg }
  }
})

// --- Interactive REPL via SerialPort ---
let replPort: InstanceType<typeof SerialPort> | null = null
// When true, suppress the 'repl-disconnected' event sent to the renderer — used
// while transparently pausing the REPL to free the port for mpremote (upload).
let replSuppressDisconnect = false
let bootLoopWarned = false
// When non-null, the REPL data handler appends incoming bytes here (as a binary
// string) instead of forwarding them to the renderer. Used to drive the
// raw-REPL protocol for uploads over the already-open port — no port handoff,
// no DTR/RTS reset, no contention with mpremote.
let replCaptureBuf: string | null = null

function attachReplHandlers(port: InstanceType<typeof SerialPort>) {
  let invalidHeaderCount = 0
  port.on('data', (data: Buffer) => {
    // While an internal raw-REPL operation (e.g. upload) is in flight, divert
    // all bytes into the capture buffer and do NOT forward them to the renderer
    // — the raw-REPL protocol bytes would otherwise spam the terminal.
    if (replCaptureBuf !== null) {
      replCaptureBuf += data.toString('binary')
      return
    }
    const text = data.toString('utf-8')
    // Filter out ESP32-S3 ROM bootloader boot-loop spam:
    // "invalid header: 0x..." repeats when flash has no valid firmware.
    const kept: string[] = []
    for (const line of text.split('\n')) {
      if (/^invalid header: 0x/.test(line.trim())) {
        invalidHeaderCount++
        continue
      }
      kept.push(line)
    }
    // Surface the boot-loop once instead of silently swallowing everything —
    // this is the usual reason the REPL/Run shows no output.
    if (invalidHeaderCount > 3 && !bootLoopWarned) {
      bootLoopWarned = true
      win?.webContents.send('repl-data',
        "\n[Anode] Board is resetting repeatedly ('invalid header: 0x...'). " +
        'No valid MicroPython firmware is running, the flash is corrupt, or the ' +
        'board reset on connect. Re-flash MicroPython firmware, then reconnect.\n')
    }
    const filtered = kept.join('\n')
    // Forward verbatim if anything survived the boot-loop filter. Do NOT gate on
    // `.trim()` — the REPL is a real terminal, and MicroPython sends bare "\r\n"
    // line terminators in their own chunks. Dropping those whitespace-only chunks
    // glues command output onto the prompt line ("print('hi')hi").
    if (filtered.length > 0) {
      win?.webContents.send('repl-data', filtered)
    }
  })
  port.on('close', () => {
    if (!replSuppressDisconnect) win?.webContents.send('repl-disconnected')
    replPort = null
  })
  port.on('error', (err: Error) => {
    win?.webContents.send('repl-error', err.message)
  })
}

function openReplPort(portPath: string, attempt = 0): Promise<{ success: boolean; error?: string }> {
  const MAX_ATTEMPTS = 5
  return new Promise((resolve) => {
    try {
      bootLoopWarned = false
      const port = new SerialPort({ path: portPath, baudRate: 115200 }, (err) => {
        if (err) {
          // The device file tree probes the port with `mpremote ls` whenever the
          // REPL is disconnected, and the native USB-Serial/JTAG re-enumerates
          // after any recent close — either leaves the port briefly busy. Retry a
          // few times so a single Connect click is enough instead of several.
          if (attempt < MAX_ATTEMPTS) {
            setTimeout(() => { openReplPort(portPath, attempt + 1).then(resolve) }, 350)
            return
          }
          win?.webContents.send('repl-error', err.message)
          replPort = null
          resolve({ success: false, error: err.message })
          return
        }
        replPort = port
        attachReplHandlers(port)
        // Opening the port can reset the ESP32-S3 (DTR/RTS) and re-run a blocking
        // main.py. Interrupt it (Ctrl-C ×2) and reprint the version banner +
        // prompt (Ctrl-B) so the REPL is interactive. The board may still be
        // booting on the first try, so nudge again a little later.
        const nudge = (banner: boolean) => {
          if (replPort && replPort.isOpen) {
            // Ctrl-C ×2 interrupts a running program; Ctrl-B reprints the
            // banner + a fresh prompt. The fallback sends a single Ctrl-C only,
            // to avoid stacking extra ">>>" prompt lines.
            replPort.write(banner ? '\x03\x03\x02' : '\x03', () => {})
          }
        }
        setTimeout(() => nudge(true), 350) // interrupt + version banner
        setTimeout(() => nudge(false), 1000) // fallback prompt if still booting at 350ms
        resolve({ success: true })
      })
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e)
      resolve({ success: false, error: msg })
    }
  })
}

// Temporarily release the serial port so a separate mpremote process can use it,
// then transparently reconnect the REPL. Renderer disconnect events are
// suppressed so the UI doesn't flicker.
async function withReplPaused<T>(fn: () => Promise<T>): Promise<T> {
  const wasOpen = !!(replPort && replPort.isOpen)
  const portPath = replPort?.path
  if (wasOpen) {
    replSuppressDisconnect = true
    // Interrupt any program left running by Run (e.g. a blocking WiFi loop) so
    // the board is idle at the REPL prompt before mpremote takes the port —
    // a busy board can't enter raw REPL and mpremote times out.
    await new Promise<void>((res) => replPort!.write('\r\x03\x03', () => res()))
    await new Promise((res) => setTimeout(res, 200)) // let the interrupt land
    await new Promise<void>((res) => replPort!.close(() => { replPort = null; res() }))
    await new Promise((res) => setTimeout(res, 250)) // let the OS/USB release the port
  }
  try {
    return await fn()
  } finally {
    if (wasOpen && portPath) {
      await openReplPort(portPath)
      replSuppressDisconnect = false
    }
  }
}

// Write a file to the board over the ALREADY-OPEN interactive REPL port using
// the raw-REPL protocol. This avoids closing/reopening the port (which makes
// the ESP32-S3 native USB-Serial/JTAG toggle DTR/RTS and reset the board,
// re-running a blocking main.py and locking mpremote out — "could not enter
// raw repl"). Same idea as Run-via-REPL, applied to file upload.
async function uploadViaRepl(
  fileName: string,
  code: string,
  sendProgress: (m: string) => void,
): Promise<{ success: boolean; error?: string }> {
  const port = replPort
  if (!port || !port.isOpen) {
    return { success: false, error: 'REPL not connected' }
  }
  const write = (data: string) =>
    new Promise<void>((res, rej) => port.write(data, (e) => (e ? rej(e) : res())))
  const delay = (ms: number) => new Promise((res) => setTimeout(res, ms))

  // Resolve once `marker` appears in the capture buffer; consumes up to and
  // including the marker and returns the bytes seen before it.
  const consumeUntil = (marker: string, timeoutMs: number) =>
    new Promise<string>((resolve, reject) => {
      const start = Date.now()
      const tick = () => {
        const buf = replCaptureBuf ?? ''
        const idx = buf.indexOf(marker)
        if (idx !== -1) {
          replCaptureBuf = buf.slice(idx + marker.length)
          resolve(buf.slice(0, idx))
          return
        }
        if (Date.now() - start > timeoutMs) {
          reject(new Error(`timed out waiting for board response`))
          return
        }
        setTimeout(tick, 20)
      }
      tick()
    })

  // Run one program inside raw REPL and wait for its OK / output / error frames.
  const rawExec = async (program: string) => {
    replCaptureBuf = ''
    await write(program)
    await write('\x04') // Ctrl-D: execute the buffered program
    await consumeUntil('OK', 5000) // device acks compile
    await consumeUntil('\x04', 10000) // stdout up to the EOT separator
    const err = await consumeUntil('\x04', 5000) // exception/traceback section
    await consumeUntil('>', 3000) // back at the raw-REPL prompt
    if (err.trim()) throw new Error(err.trim())
  }

  replCaptureBuf = ''
  try {
    sendProgress(`Uploading ${fileName} via REPL to ${port.path}...`)
    await write('\r\x03\x03') // interrupt any running program
    await delay(200)
    replCaptureBuf = ''
    await write('\x01') // Ctrl-A: enter raw REPL
    await consumeUntil('>', 3000)

    const bytes = Buffer.from(code, 'utf-8')
    await rawExec(`import binascii`)
    await rawExec(`f=open(${JSON.stringify(fileName)},'wb')`)
    const CHUNK = 512
    for (let i = 0; i < bytes.length; i += CHUNK) {
      const hex = bytes.subarray(i, i + CHUNK).toString('hex')
      await rawExec(`f.write(binascii.unhexlify('${hex}'))`)
    }
    await rawExec(`f.close()`)

    await write('\x02') // Ctrl-B: return to the friendly REPL
    await delay(100)
    sendProgress(`[OK] Uploaded ${fileName}`)
    return { success: true }
  } catch (e: unknown) {
    const msg = e instanceof Error ? e.message : String(e)
    // Best-effort: drop back to the friendly REPL so the terminal is usable.
    try { await write('\x02') } catch { /* ignore */ }
    return { success: false, error: msg }
  } finally {
    replCaptureBuf = null
  }
}

ipcMain.handle('repl-connect', async (_event, portPath: string) => {
  if (replPort && replPort.isOpen) {
    return { success: true, message: 'Already connected' }
  }
  return openReplPort(portPath)
})

ipcMain.handle('repl-disconnect', async () => {
  if (replPort && replPort.isOpen) {
    return new Promise<{ success: boolean }>((resolve) => {
      replPort!.close(() => {
        replPort = null
        resolve({ success: true })
      })
    })
  }
  replPort = null
  return { success: true }
})

ipcMain.handle('repl-send', async (_event, data: string) => {
  if (!replPort || !replPort.isOpen) {
    return { success: false, error: 'Not connected' }
  }
  return new Promise<{ success: boolean; error?: string }>((resolve) => {
    replPort!.write(data, (err) => {
      if (err) {
        resolve({ success: false, error: err.message })
      } else {
        resolve({ success: true })
      }
    })
  })
})

ipcMain.handle('repl-send-break', async () => {
  if (!replPort || !replPort.isOpen) {
    return { success: false, error: 'Not connected' }
  }
  return new Promise<{ success: boolean; error?: string }>((resolve) => {
    replPort!.write('\x03', (err) => {
      if (err) {
        resolve({ success: false, error: err.message })
      } else {
        resolve({ success: true })
      }
    })
  })
})

ipcMain.handle('repl-send-soft-reset', async () => {
  if (!replPort || !replPort.isOpen) {
    return { success: false, error: 'Not connected' }
  }
  return new Promise<{ success: boolean; error?: string }>((resolve) => {
    replPort!.write('\x04', (err) => {
      if (err) {
        resolve({ success: false, error: err.message })
      } else {
        resolve({ success: true })
      }
    })
  })
})

ipcMain.handle('repl-is-connected', async () => {
  // Report connected while the REPL is transiently paused for an upload, so the
  // device file tree doesn't fire a refresh that races with the port reconnect.
  return { connected: replSuppressDisconnect || (replPort !== null && replPort.isOpen) }
})

ipcMain.handle('repl-save-export', async (_event, content: string, defaultName?: string) => {
  if (!win) return { success: false, error: 'No window' }
  const { dialog } = await import('electron')
  const result = await dialog.showSaveDialog(win, {
    title: 'Export REPL output',
    defaultPath: defaultName || `repl-output-${Date.now()}.txt`,
    filters: [
      { name: 'Text', extensions: ['txt'] },
      { name: 'Log', extensions: ['log'] },
      { name: 'All Files', extensions: ['*'] },
    ],
  })
  if (result.canceled || !result.filePath) {
    return { success: true, canceled: true }
  }
  try {
    writeFileSync(result.filePath, content, 'utf-8')
    return { success: true, filePath: result.filePath }
  } catch (e: unknown) {
    const msg = e instanceof Error ? e.message : String(e)
    return { success: false, error: msg }
  }
})

// --- Device Filesystem (mpremote fs) ---

function buildMpremoteArgs(portPath: string, subCmd: string[]): string[] {
  const { args } = getMpremoteInvocation()
  return [...args, 'connect', portPath, ...subCmd]
}

function runMpremote(portPath: string, subCmd: string[]): Promise<{ success: boolean; stdout: string; stderr: string }> {
  const { cmd, args } = getMpremoteInvocation()
  const fullArgs = [...args, 'connect', portPath, ...subCmd]
  return new Promise((resolve) => {
    const proc = spawn(cmd, fullArgs, { shell: false })
    let stdout = ''
    let stderr = ''
    proc.stdout.on('data', (d: Buffer) => { stdout += d.toString() })
    proc.stderr.on('data', (d: Buffer) => { stderr += d.toString() })
    proc.on('close', (code) => {
      // Clean up tracebacks from stderr — keep only the meaningful error
      const cleanStderr = cleanMpremoteStderr(stderr)
      resolve({ success: code === 0, stdout, stderr: cleanStderr })
    })
    proc.on('error', (err) => {
      resolve({ success: false, stdout, stderr: err.message })
    })
  })
}

function cleanMpremoteStderr(stderr: string): string {
  if (!stderr.trim()) return ''
  if (stderr.includes('could not enter raw repl')) {
    return 'could not enter raw REPL (board may be booting or port in use)'
  }
  if (stderr.includes('it may be in use by another program')) {
    return 'serial port in use (disconnect REPL first)'
  }
  if (stderr.includes('No module named mpremote')) {
    return 'mpremote not installed (pip install mpremote)'
  }
  if (stderr.includes('Device not configured')) {
    return 'device disconnected during operation'
  }
  // Extract last non-traceback line
  const lines = stderr.trim().split('\n').filter((l) => {
    const t = l.trim()
    return t && !t.startsWith('File "') && !t.startsWith('~~~') && !t.startsWith('^')
    && !t.includes('runpy') && !t.includes('mpremote/') && !t.includes('site-packages')
    && !t.includes('Traceback') && !t.includes('During handling')
  })
  return lines[lines.length - 1] ?? stderr.trim().split('\n').pop() ?? ''
}

ipcMain.handle('device-fs-ls', async (_event, portPath: string, remotePath: string) => {
  const result = await runMpremote(portPath, ['fs', 'ls', remotePath])
  if (!result.success) {
    return { success: false, entries: [], error: result.stderr || result.stdout }
  }
  const entries: { name: string; path: string; isDirectory: boolean; size: number }[] = []
  for (const line of result.stdout.split('\n')) {
    const trimmed = line.trim()
    if (!trimmed) continue
    const isDir = trimmed.endsWith('/')
    const name = isDir ? trimmed.slice(0, -1).split('/').pop() ?? '' : trimmed.split('/').pop() ?? ''
    if (!name) continue
    entries.push({
      name,
      path: trimmed,
      isDirectory: isDir,
      size: 0,
    })
  }
  return { success: true, entries }
})

ipcMain.handle('device-fs-cat', async (_event, portPath: string, remotePath: string) => {
  const { cmd, args } = getMpremoteInvocation()
  const fullArgs = [...args, 'connect', portPath, 'fs', 'cat', remotePath]
  return new Promise((resolve) => {
    const proc = spawn(cmd, fullArgs, { shell: false })
    let stdout = ''
    let stderr = ''
    proc.stdout.on('data', (d: Buffer) => { stdout += d.toString() })
    proc.stderr.on('data', (d: Buffer) => { stderr += d.toString() })
    proc.on('close', (code) => {
      resolve({ success: code === 0, content: stdout, error: stderr })
    })
    proc.on('error', (err) => {
      resolve({ success: false, content: '', error: err.message })
    })
  })
})

ipcMain.handle('device-fs-put', async (_event, portPath: string, localFilePath: string, remotePath: string) => {
  const result = await runMpremote(portPath, ['fs', 'cp', localFilePath, remotePath])
  return { success: result.success, error: result.stderr || result.stdout }
})

ipcMain.handle('device-fs-rm', async (_event, portPath: string, remotePath: string) => {
  const result = await runMpremote(portPath, ['fs', 'rm', remotePath])
  return { success: result.success, error: result.stderr || result.stdout }
})

// --- Arduino compile/upload via bundled arduino-cli ---

function getArduinoCliCmd(): { cmd: string; args: string[] } {
  const cliPath = getArduinoCliPath()
  if (existsSync(cliPath)) {
    return { cmd: cliPath, args: [] }
  }
  return { cmd: 'arduino-cli', args: [] }
}

ipcMain.handle('arduino-compile', async (_event, opts: { sketchPath: string; fqbn: string }) => {
  const sendProgress = (message: string) => {
    const clean = message.replace(/\x1b\[\d+(;\d+)*[a-zA-Z]/g, '')
    win?.webContents.send('flash-progress', clean)
  }
  const { cmd, args } = getArduinoCliCmd()
  const fullArgs = [...args, 'compile', '--fqbn', opts.fqbn, opts.sketchPath]
  return new Promise((resolve) => {
    sendProgress(`Compiling ${opts.sketchPath} (${opts.fqbn})...`)
    const proc = spawn(cmd, fullArgs, { shell: false })
    let stdout = ''
    let stderr = ''
    proc.stdout.on('data', (d: Buffer) => { stdout += d.toString(); sendProgress(d.toString().trim()) })
    proc.stderr.on('data', (d: Buffer) => { stderr += d.toString(); sendProgress(d.toString().trim()) })
    proc.on('close', (code) => {
      if (code === 0) {
        sendProgress('[OK] Compile successful')
        resolve({ success: true, stdout })
      } else {
        sendProgress(`[ERROR] Compile failed: ${stderr || stdout}`)
        resolve({ success: false, error: stderr || stdout })
      }
    })
    proc.on('error', (err) => {
      sendProgress(`[ERROR] arduino-cli not found: ${err.message}`)
      resolve({ success: false, error: err.message })
    })
  })
})

ipcMain.handle('arduino-upload', async (_event, opts: { portPath: string; sketchPath: string; fqbn: string }) => {
  const sendProgress = (message: string) => {
    const clean = message.replace(/\x1b\[\d+(;\d+)*[a-zA-Z]/g, '')
    win?.webContents.send('flash-progress', clean)
  }
  const { cmd, args } = getArduinoCliCmd()
  const fullArgs = [...args, 'upload', '-p', opts.portPath, '--fqbn', opts.fqbn, opts.sketchPath]
  return new Promise((resolve) => {
    sendProgress(`Uploading to ${opts.portPath} (${opts.fqbn})...`)
    const proc = spawn(cmd, fullArgs, { shell: false })
    let stdout = ''
    let stderr = ''
    proc.stdout.on('data', (d: Buffer) => { stdout += d.toString(); sendProgress(d.toString().trim()) })
    proc.stderr.on('data', (d: Buffer) => { stderr += d.toString(); sendProgress(d.toString().trim()) })
    proc.on('close', (code) => {
      if (code === 0) {
        sendProgress('[OK] Upload successful')
        resolve({ success: true, stdout })
      } else {
        sendProgress(`[ERROR] Upload failed: ${stderr || stdout}`)
        resolve({ success: false, error: stderr || stdout })
      }
    })
    proc.on('error', (err) => {
      sendProgress(`[ERROR] arduino-cli not found: ${err.message}`)
      resolve({ success: false, error: err.message })
    })
  })
})
