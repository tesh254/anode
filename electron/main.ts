import { app, BrowserWindow, ipcMain, shell } from 'electron'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { execFile, spawn } from 'node:child_process'
import { existsSync, copyFileSync } from 'node:fs'
import { SerialPort } from 'serialport'

const __dirname = path.dirname(fileURLToPath(import.meta.url))
process.env.APP_ROOT = path.join(__dirname, '..')

export const VITE_DEV_SERVER_URL = process.env['VITE_DEV_SERVER_URL']
export const MAIN_DIST = path.join(process.env.APP_ROOT, 'dist-electron')
export const RENDERER_DIST = path.join(process.env.APP_ROOT, 'dist')

process.env.VITE_PUBLIC = VITE_DEV_SERVER_URL ? path.join(process.env.APP_ROOT, 'public') : RENDERER_DIST

// Set application name for macOS menu bar
app.setName('Anode')

let win: BrowserWindow | null

function createWindow() {
  win = new BrowserWindow({
    width: 1200,
    height: 800,
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
        : [['pip3', ['install', 'esptool']], ['pip', ['install', 'esptool']], ['python3', ['-m', 'pip', 'install', 'esptool']]]

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
          try { unlinkSync(destPath) } catch {}
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

ipcMain.handle('flash-firmware', async (_event, { portPath, boardType, firmwareUrl }: {
  portPath: string
  boardType: 'espressif' | 'raspberry-pi' | 'arduino'
  firmwareUrl: string
}) => {
  // Send progress updates to the renderer
  const sendProgress = (message: string) => {
    win?.webContents.send('flash-progress', message)
  }

  try {
    sendProgress(`Starting ${boardType} firmware flash on ${portPath}...`)

    switch (boardType) {
      case 'espressif': {
        // python3 -m esptool --port <port> --baud 460800 write_flash -z 0x1000 <firmware.bin>
        return new Promise((resolve) => {
          sendProgress('Running esptool via python3...')
          execFile('python3', [
            '-m', 'esptool',
            '--port', portPath,
            '--baud', '460800',
            'write_flash', '-z', '0x1000',
            firmwareUrl,
          ], (error, stdout, stderr) => {
            if (error) {
              sendProgress(`[ERROR] esptool failed: ${error.message}`)
              resolve({ success: false, error: error.message, hint: 'Install esptool: pip install esptool' })
              return
            }
            sendProgress(stdout)
            if (stderr) sendProgress(stderr)
            sendProgress('[OK] Espressif firmware flashed successfully')
            resolve({ success: true })
          })
        })
      }

      case 'raspberry-pi': {
        // For RP2040/Pico: device enters BOOTSEL mode and mounts as USB drive
        // We copy the .uf2 file to the mounted volume
        return new Promise((resolve) => {
          sendProgress('Looking for RPI-RP2 boot volume...')

          // On macOS the Pico mounts at /Volumes/RPI-RP2, on Linux typically /media/<user>/RPI-RP2
          const mountPoints = process.platform === 'darwin'
            ? ['/Volumes/RPI-RP2']
            : [`/media/${process.env.USER}/RPI-RP2`, '/run/media/' + process.env.USER + '/RPI-RP2']

          const dest = mountPoints.find((mp: string) => existsSync(mp))

          if (!dest) {
            sendProgress('[ERROR] RPI-RP2 volume not found. Hold BOOTSEL while plugging in the board.')
            resolve({ success: false, error: 'RPI-RP2 boot volume not found', hint: 'Hold BOOTSEL button while connecting the board via USB' })
            return
          }

          const filename = path.basename(firmwareUrl)
          const targetPath = path.join(dest, filename)

          try {
            copyFileSync(firmwareUrl, targetPath)
            sendProgress(`[OK] Copied ${filename} to ${dest}`)
            sendProgress('[OK] Raspberry Pi Pico firmware flashed successfully')
            resolve({ success: true })
          } catch (copyError: any) {
            sendProgress(`[ERROR] Failed to copy firmware: ${copyError.message}`)
            resolve({ success: false, error: copyError.message })
          }
        })
      }

      case 'arduino': {
        // arduino-cli upload -p <port> --fqbn <board> --input-file <firmware>
        return new Promise((resolve) => {
          sendProgress('Running arduino-cli...')
          execFile('arduino-cli', [
            'upload',
            '-p', portPath,
            '--input-file', firmwareUrl,
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
  } catch (error: any) {
    sendProgress(`[ERROR] ${error.message}`)
    return { success: false, error: error.message }
  }
})
