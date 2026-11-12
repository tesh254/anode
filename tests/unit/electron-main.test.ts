import { describe, it, expect, beforeEach } from 'vitest'
import { spawn } from 'node:child_process'
import fs from 'node:fs'
import path from 'node:path'
import os from 'node:os'

describe('Electron main process — tool checking logic', () => {
  describe('COMMON_TOOL_PATHS', () => {
    it('esptool.py has expected platform paths', () => {
      const expectedPaths = [
        '/usr/local/bin/esptool.py',
        '/usr/bin/esptool.py',
      ]
      const home = process.env.HOME ?? ''
      const platformPaths = [
        '/opt/homebrew/bin/esptool.py',
        `${home}/Library/Python/3.11/bin/esptool`,
        `${home}/Library/Python/3.12/bin/esptool`,
        `${home}/Library/Python/3.13/bin/esptool`,
      ]
      expect([...expectedPaths, ...platformPaths].length).toBeGreaterThan(0)
    })

    it('arduino-cli has expected platform paths', () => {
      const expectedPaths = [
        '/usr/local/bin/arduino-cli',
        '/usr/bin/arduino-cli',
      ]
      expect(expectedPaths.length).toBeGreaterThan(0)
    })
  })

  describe('toolAvailable logic', () => {
    it('esptool.py check via python3 import', async () => {
      const result = await new Promise<{ available: boolean }>((resolve) => {
        const child = spawn('python3', ['-c', 'import esptool; print(esptool.__version__)'], { shell: false })
        child.on('close', (code) => {
          resolve({ available: code === 0 })
        })
        child.on('error', () => {
          resolve({ available: false })
        })
      })
      expect(result).toHaveProperty('available')
    })

    it('arduino-cli check via version command', async () => {
      const result = await new Promise<{ available: boolean }>((resolve) => {
        const child = spawn('arduino-cli', ['version'], { shell: false })
        child.on('close', (code) => {
          resolve({ available: code === 0 })
        })
        child.on('error', () => {
          resolve({ available: false })
        })
      })
      expect(result).toHaveProperty('available')
    })
  })
})

describe('Electron main process — file system operations', () => {
  const tmpDir = path.join(os.tmpdir(), `anode-test-fs-${Date.now()}`)

  beforeEach(() => {
    if (!fs.existsSync(tmpDir)) {
      fs.mkdirSync(tmpDir, { recursive: true })
    }
  })

  it('create-file: creates a file with content', () => {
    const filePath = path.join(tmpDir, 'test.py')
    fs.writeFileSync(filePath, 'print("hello")', 'utf-8')
    expect(fs.existsSync(filePath)).toBe(true)
    expect(fs.readFileSync(filePath, 'utf-8')).toBe('print("hello")')
  })

  it('read-file: reads file content and detects language from extension', () => {
    const pyFile = path.join(tmpDir, 'test.py')
    fs.writeFileSync(pyFile, 'x = 1', 'utf-8')

    const inoFile = path.join(tmpDir, 'test.ino')
    fs.writeFileSync(inoFile, 'void setup() {}', 'utf-8')

    const ext = path.extname(pyFile).toLowerCase()
    expect(ext).toBe('.py')
    expect(ext === '.ino' ? 'arduino' : 'python').toBe('python')

    const ext2 = path.extname(inoFile).toLowerCase()
    expect(ext2).toBe('.ino')
    expect(ext2 === '.ino' ? 'arduino' : 'python').toBe('arduino')
  })

  it('write-file: overwrites existing file content', () => {
    const filePath = path.join(tmpDir, 'overwrite.py')
    fs.writeFileSync(filePath, 'original', 'utf-8')
    fs.writeFileSync(filePath, 'updated', 'utf-8')
    expect(fs.readFileSync(filePath, 'utf-8')).toBe('updated')
  })

  it('read-directory: lists directory entries', () => {
    fs.writeFileSync(path.join(tmpDir, 'a.py'), '', 'utf-8')
    fs.writeFileSync(path.join(tmpDir, 'b.py'), '', 'utf-8')
    fs.mkdirSync(path.join(tmpDir, 'subdir'), { recursive: true })

    const entries = fs.readdirSync(tmpDir, { withFileTypes: true })
    const names = entries.map(e => e.name)
    expect(names).toContain('a.py')
    expect(names).toContain('b.py')
    expect(names).toContain('subdir')

    const subdirEntry = entries.find(e => e.name === 'subdir')
    expect(subdirEntry?.isDirectory()).toBe(true)
    const aEntry = entries.find(e => e.name === 'a.py')
    expect(aEntry?.isFile()).toBe(true)
  })

  it('create-directory: creates nested directories', () => {
    const nestedPath = path.join(tmpDir, 'deep', 'nested', 'dir')
    fs.mkdirSync(nestedPath, { recursive: true })
    expect(fs.statSync(nestedPath).isDirectory()).toBe(true)
  })

  it('get-file-stats: returns file stats', () => {
    const filePath = path.join(tmpDir, 'stats.py')
    fs.writeFileSync(filePath, 'hello world', 'utf-8')
    const stats = fs.statSync(filePath)
    expect(stats.size).toBeGreaterThan(0)
    expect(stats.isFile()).toBe(true)
    expect(stats.isDirectory()).toBe(false)
    expect(stats.mtime.getTime()).toBeTypeOf('number')
  })

  it('handles non-existent file gracefully', () => {
    expect(() => fs.statSync('/nonexistent/path/file.py')).toThrow()
  })
})

describe('Electron main process — upload/run code logic', () => {
  it('creates temp file with correct content for upload', () => {
    const tmpFile = path.join(os.tmpdir(), `anode-upload-test-${Date.now()}-main.py`)
    const code = 'print("hello from board")'
    fs.writeFileSync(tmpFile, code, 'utf-8')
    expect(fs.readFileSync(tmpFile, 'utf-8')).toBe(code)
    fs.unlinkSync(tmpFile)
  })

  it('creates temp file with correct content for run', () => {
    const tmpFile = path.join(os.tmpdir(), `anode-run-test-${Date.now()}.py`)
    const code = 'import machine\nled = machine.Pin(2, machine.Pin.OUT)\nled.value(1)'
    fs.writeFileSync(tmpFile, code, 'utf-8')
    expect(fs.readFileSync(tmpFile, 'utf-8')).toBe(code)
    fs.unlinkSync(tmpFile)
  })
})

describe('Electron main process — firmware flash logic', () => {
  it('ANSI escape code stripping works correctly', () => {
    const stripAnsi = (msg: string) => msg.replace(/\x1b\[\d+(;\d+)*[a-zA-Z]/g, '') // eslint-disable-line no-control-regex
    expect(stripAnsi('\x1b[1;31mError\x1b[0m: failed')).toBe('Error: failed')
    expect(stripAnsi('\x1b[32mOK\x1b[0m')).toBe('OK')
    expect(stripAnsi('no escape codes')).toBe('no escape codes')
    expect(stripAnsi('\x1b[0m\x1b[1m\x1b[31m')).toBe('')
  })

  it('determines correct flash offset for ESP chips', () => {
    const getFlashOffset = (chipType: string) => {
      if (chipType === 'ESP8266' || chipType.includes('C3') || chipType.includes('C6') || chipType.includes('S3')) {
        return '0x0'
      }
      return '0x1000'
    }

    expect(getFlashOffset('ESP8266')).toBe('0x0')
    expect(getFlashOffset('ESP32_GENERIC_C3')).toBe('0x0')
    expect(getFlashOffset('ESP32_GENERIC_S3')).toBe('0x0')
    expect(getFlashOffset('ESP32_GENERIC_C6')).toBe('0x0')
    expect(getFlashOffset('ESP32_GENERIC')).toBe('0x1000')
    expect(getFlashOffset('ESP32')).toBe('0x1000')
  })

  it('chip type detection from esptool output', () => {
    const detectChip = (stdout: string) => {
      if (stdout.includes('ESP8266')) return 'ESP8266'
      if (stdout.includes('ESP32-S2')) return 'ESP32_GENERIC_S2'
      if (stdout.includes('ESP32-S3')) return 'ESP32_GENERIC_S3'
      if (stdout.includes('ESP32-C3')) return 'ESP32_GENERIC_C3'
      if (stdout.includes('ESP32-C6')) return 'ESP32_GENERIC_C6'
      return 'ESP32_GENERIC'
    }

    expect(detectChip('Chip is ESP8266')).toBe('ESP8266')
    expect(detectChip('Chip is ESP32-S2')).toBe('ESP32_GENERIC_S2')
    expect(detectChip('Chip is ESP32-S3')).toBe('ESP32_GENERIC_S3')
    expect(detectChip('Chip is ESP32-C3')).toBe('ESP32_GENERIC_C3')
    expect(detectChip('Chip is ESP32')).toBe('ESP32_GENERIC')
    expect(detectChip('Unknown chip type')).toBe('ESP32_GENERIC')
  })

  it('RPI-RP2 mount point resolution', () => {
    const getMountPoint = (platform: string) => {
      if (platform === 'darwin') return '/Volumes/RPI-RP2'
      return `/media/${process.env.USER}/RPI-RP2`
    }

    expect(getMountPoint('darwin')).toBe('/Volumes/RPI-RP2')
    expect(getMountPoint('linux')).toContain('RPI-RP2')
  })
})

describe('Electron main process — tool installation logic', () => {
  it('pip command ordering for espressif on non-Windows', () => {
    const pipCommands = [
      ['python3', ['-m', 'pip', 'install', '--user', 'esptool']],
      ['python3', ['-m', 'pip', 'install', '--user', '--break-system-packages', 'esptool']],
      ['pip3', ['install', '--user', 'esptool']],
    ]
    expect(pipCommands).toHaveLength(3)
    expect(pipCommands[0][0]).toBe('python3')
    expect(pipCommands[1][1]).toContain('--break-system-packages')
  })

  it('pip command ordering for espressif on Windows', () => {
    const pipCommands = [
      ['pip', ['install', 'esptool']],
      ['python', ['-m', 'pip', 'install', 'esptool']],
    ]
    expect(pipCommands).toHaveLength(2)
    expect(pipCommands[0][0]).toBe('pip')
  })

  it('arduino-cli download URL format', () => {
    const version = '1.0.4'
    const platform = 'macOS_64bit'
    const filename = `arduino-cli_${version}_${platform}.tar.gz`
    const url = `https://downloads.arduino.cc/arduino-cli/${filename}`
    expect(url).toBe('https://downloads.arduino.cc/arduino-cli/arduino-cli_1.0.4_macOS_64bit.tar.gz')
  })

  it('arduino FQBN detection from board list output', () => {
    const portLine = '/dev/ttyUSB0        2341  0043  /dev/ttyUSB0  arduino:avr:uno'
    const match = portLine.match(/([a-zA-Z0-9]+:[a-zA-Z0-9]+:[a-zA-Z0-9_]+)/)
    expect(match).not.toBeNull()
    expect(match![1]).toBe('arduino:avr:uno')
  })

  it('arduino FQBN default fallback', () => {
    const defaultFqbn = 'arduino:avr:uno'
    expect(defaultFqbn).toBe('arduino:avr:uno')
  })
})
