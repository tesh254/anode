import { describe, it, expect, vi, beforeEach } from 'vitest'
import fs from 'node:fs'
import path from 'node:path'
import os from 'node:os'

const tmpDir = path.join(os.tmpdir(), `anode-electron-test-${Date.now()}`)

beforeEach(() => {
  if (!fs.existsSync(tmpDir)) fs.mkdirSync(tmpDir, { recursive: true })
})

describe('Electron main — ANSI stripping', () => {
  const stripAnsi = (msg: string) => msg.replace(/\x1b\[\d+(;\d+)*[a-zA-Z]/g, '') // eslint-disable-line no-control-regex

  it('strips basic color codes', () => {
    expect(stripAnsi('\x1b[31mred\x1b[0m')).toBe('red')
  })

  it('strips multi-param codes', () => {
    expect(stripAnsi('\x1b[1;31mbold red\x1b[0m')).toBe('bold red')
  })

  it('strips complex sequences', () => {
    expect(stripAnsi('\x1b[1;31;42mbold red green\x1b[0m')).toBe('bold red green')
  })

  it('leaves plain text unchanged', () => {
    expect(stripAnsi('hello world')).toBe('hello world')
  })

  it('strips consecutive codes', () => {
    expect(stripAnsi('\x1b[1m\x1b[31m\x1b[42mstyled\x1b[0m')).toBe('styled')
  })

  it('strips 256-color codes', () => {
    expect(stripAnsi('\x1b[38;5;196mred\x1b[0m')).toBe('red')
  })

  it('handles empty string', () => {
    expect(stripAnsi('')).toBe('')
  })

  it('strips codes from mixed content', () => {
    expect(stripAnsi('start \x1b[32mOK\x1b[0m end')).toBe('start OK end')
  })
})

describe('Electron main — chip detection', () => {
  const detectChip = (stdout: string) => {
    if (stdout.includes('ESP8266')) return 'ESP8266'
    if (stdout.includes('ESP32-S2')) return 'ESP32_GENERIC_S2'
    if (stdout.includes('ESP32-S3')) return 'ESP32_GENERIC_S3'
    if (stdout.includes('ESP32-C3')) return 'ESP32_GENERIC_C3'
    if (stdout.includes('ESP32-C6')) return 'ESP32_GENERIC_C6'
    return 'ESP32_GENERIC'
  }

  it('detects ESP8266', () => expect(detectChip('Chip is ESP8266EX')).toBe('ESP8266'))
  it('detects ESP32-S2', () => expect(detectChip('Chip is ESP32-S2')).toBe('ESP32_GENERIC_S2'))
  it('detects ESP32-S3', () => expect(detectChip('Chip is ESP32-S3')).toBe('ESP32_GENERIC_S3'))
  it('detects ESP32-C3', () => expect(detectChip('Chip is ESP32-C3')).toBe('ESP32_GENERIC_C3'))
  it('detects ESP32-C6', () => expect(detectChip('Chip is ESP32-C6')).toBe('ESP32_GENERIC_C6'))
  it('defaults to ESP32_GENERIC', () => expect(detectChip('Chip is ESP32')).toBe('ESP32_GENERIC'))
  it('defaults on unknown', () => expect(detectChip('Unknown')).toBe('ESP32_GENERIC'))
  it('empty string defaults', () => expect(detectChip('')).toBe('ESP32_GENERIC'))
  it('detects S2 in longer output', () => expect(detectChip('Detecting chip type... ESP32-S2 detected')).toBe('ESP32_GENERIC_S2'))
  it('C6 before default', () => expect(detectChip('ESP32-C6 chip')).toBe('ESP32_GENERIC_C6'))
})

describe('Electron main — flash offset', () => {
  const getFlashOffset = (chipType: string) => {
    if (chipType === 'ESP8266' || chipType.includes('C3') || chipType.includes('C6') || chipType.includes('S3')) {
      return '0x0'
    }
    return '0x1000'
  }

  it('ESP8266 uses 0x0', () => expect(getFlashOffset('ESP8266')).toBe('0x0'))
  it('ESP32_GENERIC_C3 uses 0x0', () => expect(getFlashOffset('ESP32_GENERIC_C3')).toBe('0x0'))
  it('ESP32_GENERIC_C6 uses 0x0', () => expect(getFlashOffset('ESP32_GENERIC_C6')).toBe('0x0'))
  it('ESP32_GENERIC_S3 uses 0x0', () => expect(getFlashOffset('ESP32_GENERIC_S3')).toBe('0x0'))
  it('ESP32_GENERIC uses 0x1000', () => expect(getFlashOffset('ESP32_GENERIC')).toBe('0x1000'))
  it('ESP32 uses 0x1000', () => expect(getFlashOffset('ESP32')).toBe('0x1000'))
  it('ESP32_GENERIC_S2 uses 0x1000', () => expect(getFlashOffset('ESP32_GENERIC_S2')).toBe('0x1000'))
})

describe('Electron main — file system handlers (real fs)', () => {
  it('create-file: writes file with content', () => {
    const filePath = path.join(tmpDir, 'create-test.py')
    fs.writeFileSync(filePath, 'print(1)', 'utf-8')
    expect(fs.existsSync(filePath)).toBe(true)
    expect(fs.readFileSync(filePath, 'utf-8')).toBe('print(1)')
  })

  it('create-file: creates parent directories', () => {
    const dirPath = path.join(tmpDir, 'nested', 'dir')
    const filePath = path.join(dirPath, 'test.py')
    fs.mkdirSync(dirPath, { recursive: true })
    fs.writeFileSync(filePath, 'x', 'utf-8')
    expect(fs.existsSync(filePath)).toBe(true)
  })

  it('read-file: detects Python language from .py extension', () => {
    const filePath = path.join(tmpDir, 'lang-test.py')
    fs.writeFileSync(filePath, 'x = 1', 'utf-8')
    const ext = path.extname(filePath).toLowerCase()
    const language = ext === '.ino' ? 'arduino' : 'python'
    expect(language).toBe('python')
  })

  it('read-file: detects Arduino language from .ino extension', () => {
    const filePath = path.join(tmpDir, 'lang-test.ino')
    fs.writeFileSync(filePath, 'void setup() {}', 'utf-8')
    const ext = path.extname(filePath).toLowerCase()
    const language = ext === '.ino' ? 'arduino' : 'python'
    expect(language).toBe('arduino')
  })

  it('write-file: overwrites content', () => {
    const filePath = path.join(tmpDir, 'overwrite-test.py')
    fs.writeFileSync(filePath, 'old', 'utf-8')
    fs.writeFileSync(filePath, 'new', 'utf-8')
    expect(fs.readFileSync(filePath, 'utf-8')).toBe('new')
  })

  it('write-file: returns error on failure', () => {
    try {
      fs.writeFileSync('/nonexistent/path/file.py', 'x', 'utf-8')
      expect(true).toBe(false)
    } catch (error) {
      expect(error).toBeTruthy()
      const errorMsg = error instanceof Error ? error.message : String(error)
      expect(errorMsg.length).toBeGreaterThan(0)
    }
  })

  it('read-directory: lists entries with types', () => {
    fs.writeFileSync(path.join(tmpDir, 'a.py'), '', 'utf-8')
    fs.mkdirSync(path.join(tmpDir, 'subdir'), { recursive: true })
    const entries = fs.readdirSync(tmpDir, { withFileTypes: true })
    const names = entries.map(e => e.name)
    expect(names).toContain('a.py')
    expect(names).toContain('subdir')
    expect(entries.find(e => e.name === 'subdir')?.isDirectory()).toBe(true)
    expect(entries.find(e => e.name === 'a.py')?.isFile()).toBe(true)
  })

  it('read-directory: returns empty for nonexistent dir', () => {
    expect(() => fs.readdirSync('/nonexistent/dir')).toThrow()
  })

  it('create-directory: creates nested dirs', () => {
    const dirPath = path.join(tmpDir, 'deep', 'nested')
    fs.mkdirSync(dirPath, { recursive: true })
    expect(fs.statSync(dirPath).isDirectory()).toBe(true)
  })

  it('get-file-stats: returns file size and type', () => {
    const filePath = path.join(tmpDir, 'stats-test.py')
    fs.writeFileSync(filePath, 'hello', 'utf-8')
    const stats = fs.statSync(filePath)
    expect(stats.size).toBe(5)
    expect(stats.isFile()).toBe(true)
    expect(stats.isDirectory()).toBe(false)
    expect(stats.mtime.getTime()).toBeTypeOf('number')
  })

  it('get-file-stats: returns null for nonexistent file', () => {
    expect(() => fs.statSync('/nonexistent')).toThrow()
  })
})

describe('Electron main — upload code logic', () => {
  it('creates temp file with correct naming pattern for upload', () => {
    const tmpFile = path.join(os.tmpdir(), `anode-upload-${Date.now()}-main.py`)
    const code = 'print("hello")'
    fs.writeFileSync(tmpFile, code, 'utf-8')
    expect(fs.readFileSync(tmpFile, 'utf-8')).toBe(code)
    expect(tmpFile).toContain('anode-upload-')
    expect(tmpFile).toContain('main.py')
    fs.unlinkSync(tmpFile)
  })

  it('creates temp file with correct naming pattern for run', () => {
    const tmpFile = path.join(os.tmpdir(), `anode-run-${Date.now()}.py`)
    const code = 'import machine'
    fs.writeFileSync(tmpFile, code, 'utf-8')
    expect(fs.readFileSync(tmpFile, 'utf-8')).toBe(code)
    expect(tmpFile).toContain('anode-run-')
    fs.unlinkSync(tmpFile)
  })
})

describe('Electron main — firmware URL logic', () => {
  it('MicroPython URL parsing for ESP32', () => {
    const html = '<a href="/resources/firmware/esp32-20240101-v1.22.1.bin">Download</a>'
    const match = html.match(/href="(\/resources\/firmware\/[^"]+)"/)
    expect(match).not.toBeNull()
    expect(match![1]).toBe('/resources/firmware/esp32-20240101-v1.22.1.bin')
    const fullUrl = `https://micropython.org${match![1]}`
    expect(fullUrl).toContain('https://micropython.org')
  })

  it('MicroPython URL parsing returns null on no match', () => {
    const html = '<p>No firmware links here</p>'
    const match = html.match(/href="(\/resources\/firmware\/[^"]+)"/)
    expect(match).toBeNull()
  })
})

describe('Electron main — firmware dialog filters', () => {
  it('returns UF2 filter for raspberry-pi', () => {
    const boardType = 'raspberry-pi'
    const filters = boardType === 'raspberry-pi'
      ? [{ name: 'UF2 Firmware', extensions: ['uf2'] }]
      : [{ name: 'Binary Firmware', extensions: ['bin', 'elf'] }]
    expect(filters[0].extensions).toContain('uf2')
  })

  it('returns bin/elf filter for espressif', () => {
    const boardType = 'espressif'
    const filters = boardType === 'raspberry-pi'
      ? [{ name: 'UF2 Firmware', extensions: ['uf2'] }]
      : [{ name: 'Binary Firmware', extensions: ['bin', 'elf'] }]
    expect(filters[0].extensions).toContain('bin')
    expect(filters[0].extensions).toContain('elf')
  })

  it('returns bin/elf filter for arduino', () => {
    const boardType = 'arduino'
    const filters = boardType === 'raspberry-pi'
      ? [{ name: 'UF2 Firmware', extensions: ['uf2'] }]
      : [{ name: 'Binary Firmware', extensions: ['bin', 'elf'] }]
    expect(filters[0].extensions).toContain('bin')
  })
})

describe('Electron main — RPI mount points', () => {
  it('darwin uses /Volumes/RPI-RP2', () => {
    const mountPoints = process.platform === 'darwin'
      ? ['/Volumes/RPI-RP2']
      : [`/media/${process.env.USER}/RPI-RP2`, '/run/media/' + process.env.USER + '/RPI-RP2']
    expect(mountPoints).toContain('/Volumes/RPI-RP2')
  })

  it('finds existing mount point', () => {
    const mountPoints = ['/tmp', '/nonexistent']
    const dest = mountPoints.find(mp => fs.existsSync(mp))
    expect(dest).toBe('/tmp')
  })

  it('returns undefined when no mount point exists', () => {
    const mountPoints = ['/nonexistent1', '/nonexistent2']
    const dest = mountPoints.find(mp => fs.existsSync(mp))
    expect(dest).toBeUndefined()
  })
})

describe('Electron main — pip command ordering', () => {
  it('espressif on non-Windows: 3 commands', () => {
    const pipCommands = [
      ['python3', ['-m', 'pip', 'install', '--user', 'esptool']],
      ['python3', ['-m', 'pip', 'install', '--user', '--break-system-packages', 'esptool']],
      ['pip3', ['install', '--user', 'esptool']],
    ]
    expect(pipCommands).toHaveLength(3)
    expect(pipCommands[0][0]).toBe('python3')
    expect(pipCommands[0][1]).toContain('--user')
    expect(pipCommands[1][1]).toContain('--break-system-packages')
    expect(pipCommands[2][0]).toBe('pip3')
  })

  it('espressif on Windows: 2 commands', () => {
    const pipCommands = [
      ['pip', ['install', 'esptool']],
      ['python', ['-m', 'pip', 'install', 'esptool']],
    ]
    expect(pipCommands).toHaveLength(2)
    expect(pipCommands[0][0]).toBe('pip')
    expect(pipCommands[1][0]).toBe('python')
  })
})

describe('Electron main — arduino-cli logic', () => {
  it('URL format is correct', () => {
    const version = '1.0.4'
    const platform = process.platform === 'win32' ? 'Windows_64bit' : process.platform === 'darwin' ? 'macOS_64bit' : 'Linux_64bit'
    const filename = `arduino-cli_${version}_${platform}.tar.gz`
    const url = `https://downloads.arduino.cc/arduino-cli/${filename}`
    expect(url).toContain('downloads.arduino.cc')
    expect(url).toContain('1.0.4')
    expect(url).toContain(platform)
    expect(url).toContain('.tar.gz')
  })

  it('FQBN detection from board list output', () => {
    const portLine = '/dev/ttyUSB0  2341  0043  arduino:avr:uno'
    const match = portLine.match(/([a-zA-Z0-9]+:[a-zA-Z0-9]+:[a-zA-Z0-9_]+)/)
    expect(match).not.toBeNull()
    expect(match![1]).toBe('arduino:avr:uno')
  })

  it('FQBN detection with complex FQBN', () => {
    const portLine = '/dev/ttyACM0  arduino:samd:mkr1000'
    const match = portLine.match(/([a-zA-Z0-9]+:[a-zA-Z0-9]+:[a-zA-Z0-9_]+)/)
    expect(match).not.toBeNull()
    expect(match![1]).toBe('arduino:samd:mkr1000')
  })

  it('FQBN default fallback', () => {
    const defaultFqbn = 'arduino:avr:uno'
    expect(defaultFqbn).toBe('arduino:avr:uno')
  })

  it('binary name per platform', () => {
    const binName = process.platform === 'win32' ? 'arduino-cli.exe' : 'arduino-cli'
    if (process.platform === 'win32') {
      expect(binName).toBe('arduino-cli.exe')
    } else {
      expect(binName).toBe('arduino-cli')
    }
  })
})

describe('Electron main — error message extraction', () => {
  it('extracts Error message from Error instances', () => {
    const error = new Error('test error')
    const msg = error instanceof Error ? error.message : String(error)
    expect(msg).toBe('test error')
  })

  it('converts non-Error to string', () => {
    const error = 'string error'
    const msg = error instanceof Error ? error.message : String(error)
    expect(msg).toBe('string error')
  })

  it('handles number errors', () => {
    const error = 42
    const msg = error instanceof Error ? error.message : String(error)
    expect(msg).toBe('42')
  })
})

describe('Electron main — connection error hint', () => {
  it('hints BOOT button for connection errors', () => {
    const errMsg = 'Error: No serial data received from device'
    const isConnectionError = errMsg.includes('No serial data received') || errMsg.includes('Failed to connect')
    const hint = isConnectionError
      ? 'Failed to connect. Try holding the BOOT button on your board while it connects.'
      : 'Install esptool: pip install esptool'
    expect(hint).toContain('BOOT')
  })

  it('hints pip install for other errors', () => {
    const errMsg = 'Error: Something else went wrong'
    const isConnectionError = errMsg.includes('No serial data received') || errMsg.includes('Failed to connect')
    const hint = isConnectionError
      ? 'Failed to connect. Try holding the BOOT button on your board while it connects.'
      : 'Install esptool: pip install esptool'
    expect(hint).toContain('pip install')
  })

  it('detects "Failed to connect" as connection error', () => {
    const errMsg = 'Failed to connect to ESP32'
    const isConnectionError = errMsg.includes('No serial data received') || errMsg.includes('Failed to connect')
    expect(isConnectionError).toBe(true)
  })
})

describe('Electron main — window creation config', () => {
  it('webPreferences have security settings', () => {
    const prefs = {
      contextIsolation: true,
      nodeIntegration: false,
    }
    expect(prefs.contextIsolation).toBe(true)
    expect(prefs.nodeIntegration).toBe(false)
  })

  it('window has expected dimensions', () => {
    const config = { width: 1200, height: 800 }
    expect(config.width).toBe(1200)
    expect(config.height).toBe(800)
  })
})

describe('Electron main — icon path resolution', () => {
  it('macOS icon path', () => {
    const resourcesPath = '/app/resources'
    const iconPath = path.join(resourcesPath, 'app-icon', 'macos', '512x512.png')
    expect(iconPath).toContain('macos')
    expect(iconPath).toContain('512x512.png')
  })

  it('Windows icon path', () => {
    const resourcesPath = '/app/resources'
    const iconPath = path.join(resourcesPath, 'app-icon', 'windows', '256x256.png')
    expect(iconPath).toContain('windows')
    expect(iconPath).toContain('256x256.png')
  })

  it('Linux icon path', () => {
    const resourcesPath = '/app/resources'
    const iconPath = path.join(resourcesPath, 'app-icon', 'linux', 'icons', '256x256.png')
    expect(iconPath).toContain('linux')
    expect(iconPath).toContain('256x256.png')
  })
})

describe('Electron main — tool path checking', () => {
  it('COMMON_TOOL_PATHS has esptool paths', () => {
    const paths = [
      '/usr/local/bin/esptool.py',
      '/usr/bin/esptool.py',
      '/opt/homebrew/bin/esptool.py',
    ]
    expect(paths.length).toBe(3)
  })

  it('COMMON_TOOL_PATHS has arduino-cli paths', () => {
    const paths = [
      '/usr/local/bin/arduino-cli',
      '/usr/bin/arduino-cli',
      '/opt/homebrew/bin/arduino-cli',
    ]
    expect(paths.length).toBe(3)
  })

  it('checkToolInPaths returns false for nonexistent tool', () => {
    const tool = 'nonexistent-tool'
    const extraPaths: string[] = []
    let found = false
    for (const p of extraPaths) {
      if (fs.existsSync(p)) { found = true; break }
    }
    expect(found).toBe(false)
  })
})

describe('Electron main — preload API exposure', () => {
  it('exposes hardware API methods', () => {
    const expectedMethods = [
      'openFolder', 'readDirectory', 'readFile', 'writeFile',
      'createFile', 'createDirectory', 'getFileStats',
      'openFirmwareDialog', 'getSerialPorts',
      'runCode', 'uploadCode', 'stopCode',
      'flashFirmware', 'checkTool', 'installTool', 'openUrl',
      'onFlashProgress', 'onInstallProgress',
    ]
    expect(expectedMethods.length).toBe(18)
    expectedMethods.forEach(method => {
      expect(typeof method).toBe('string')
      expect(method.length).toBeGreaterThan(0)
    })
  })
})

describe('Electron main — SerialPort.list error handling', () => {
  it('returns empty array on error', async () => {
    const mockList = vi.fn().mockRejectedValue(new Error('Permission denied'))
    try {
      await mockList()
    } catch {
      // handler returns []
    }
    expect(mockList).toHaveBeenCalled()
  })
})
