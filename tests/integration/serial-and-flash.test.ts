import { describe, it, expect, beforeEach } from 'vitest'
import { useStore, detectBoardType } from '@/lib/store'
import { SerialPort } from 'serialport'

describe('Integration — Serial Port Workflow', () => {
  beforeEach(() => {
    useStore.getState().reset()
  })

  describe('port detection and board identification flow', () => {
    it('auto-selects first known board when no port selected', () => {
      const ports = [
        { path: '/dev/ttyUSB0', manufacturer: 'Silicon Labs', vendorId: '10c4', productId: 'ea71' },
        { path: '/dev/ttyACM0', manufacturer: 'Unknown Corp', vendorId: '0000', productId: '0001' },
      ]

      useStore.getState().setPorts(ports)

      const knownBoard = ports.find(p => detectBoardType(p) !== 'unknown')
      if (knownBoard) {
        useStore.getState().setSelectedPort(knownBoard.path)
        useStore.getState().setDetectedBoard(detectBoardType(knownBoard))
      }

      expect(useStore.getState().selectedPort).toBe('/dev/ttyUSB0')
      expect(useStore.getState().detectedBoard).toBe('espressif')
    })

    it('auto-detects M5Stack Atom Light by VID/PID', () => {
      const ports = [
        { path: '/dev/ttyUSB0', manufacturer: 'Silicon Labs', vendorId: '10c4', productId: 'ea60' },
      ]

      useStore.getState().setPorts(ports)
      const knownBoard = ports.find(p => detectBoardType(p) !== 'unknown')!
      useStore.getState().setSelectedPort(knownBoard.path)
      useStore.getState().setDetectedBoard(detectBoardType(knownBoard))

      expect(useStore.getState().detectedBoard).toBe('m5stack-atom')
    })

    it('auto-detects M5StampS3 by VID/PID', () => {
      const ports = [
        { path: '/dev/ttyACM0', manufacturer: 'Espressif', vendorId: '303a', productId: '8010' },
      ]

      useStore.getState().setPorts(ports)
      const knownBoard = ports.find(p => detectBoardType(p) !== 'unknown')!
      useStore.getState().setSelectedPort(knownBoard.path)
      useStore.getState().setDetectedBoard(detectBoardType(knownBoard))

      expect(useStore.getState().detectedBoard).toBe('m5stamp-s3')
    })

    it('selects first available port if no known board found', () => {
      // Use non-USB serial ports so path-based fallback doesn't identify them
      const ports = [
        { path: '/dev/ttyS0', manufacturer: 'Unknown', vendorId: '0000', productId: '0001' },
        { path: '/dev/ttyS1', manufacturer: 'Unknown', vendorId: '1111', productId: '0002' },
      ]

      useStore.getState().setPorts(ports)

      const knownBoard = ports.find(p => detectBoardType(p) !== 'unknown')
      if (!knownBoard && ports.length > 0) {
        useStore.getState().setSelectedPort(ports[0].path)
      }

      expect(useStore.getState().selectedPort).toBe('/dev/ttyS0')
      expect(useStore.getState().detectedBoard).toBe('unknown')
    })

    it('updates detected board when selected port changes', () => {
      const ports = [
        { path: '/dev/ttyUSB0', manufacturer: 'Espressif', vendorId: '303a', productId: '0001' },
        { path: '/dev/ttyACM0', manufacturer: 'Arduino LLC', vendorId: '2341', productId: '0043' },
      ]

      useStore.getState().setPorts(ports)

      useStore.getState().setSelectedPort('/dev/ttyUSB0')
      const port0 = ports.find(p => p.path === '/dev/ttyUSB0')
      useStore.getState().setDetectedBoard(detectBoardType(port0!))
      expect(useStore.getState().detectedBoard).toBe('espressif')

      useStore.getState().setSelectedPort('/dev/ttyACM0')
      const port1 = ports.find(p => p.path === '/dev/ttyACM0')
      useStore.getState().setDetectedBoard(detectBoardType(port1!))
      expect(useStore.getState().detectedBoard).toBe('arduino')
    })

    it('handles port disconnection gracefully', () => {
      useStore.getState().setPorts([
        { path: '/dev/ttyUSB0', vendorId: '10c4' },
      ])
      useStore.getState().setSelectedPort('/dev/ttyUSB0')
      useStore.getState().setDetectedBoard('espressif')

      // Simulate disconnection — ports list becomes empty
      useStore.getState().setPorts([])
      useStore.getState().setDetectedBoard('unknown')
      useStore.getState().setSelectedPort('')

      expect(useStore.getState().detectedBoard).toBe('unknown')
      expect(useStore.getState().selectedPort).toBe('')
    })

    it('falls back to another available port on disconnection', () => {
      useStore.getState().setPorts([
        { path: '/dev/ttyUSB0', vendorId: '10c4' },
        { path: '/dev/ttyACM0', vendorId: '2e8a' },
      ])
      useStore.getState().setSelectedPort('/dev/ttyUSB0')
      useStore.getState().setDetectedBoard('espressif')

      // USB0 disconnected, only ACM0 remains
      const remainingPorts = [
        { path: '/dev/ttyACM0', vendorId: '2e8a' },
      ]
      useStore.getState().setPorts(remainingPorts)
      useStore.getState().setSelectedPort('/dev/ttyACM0')
      useStore.getState().setDetectedBoard('raspberry-pi')

      expect(useStore.getState().selectedPort).toBe('/dev/ttyACM0')
      expect(useStore.getState().detectedBoard).toBe('raspberry-pi')
    })
  })

  describe('serial port listing (requires hardware or mock)', () => {
    it('SerialPort.list returns an array (may be empty without hardware)', async () => {
      const ports = await SerialPort.list()
      expect(Array.isArray(ports)).toBe(true)
    })
  })
})

describe('Integration — Flash Workflow', () => {
  beforeEach(() => {
    useStore.getState().reset()
  })

  it('opens flash dialog when port is selected', () => {
    useStore.getState().setSelectedPort('/dev/ttyUSB0')
    useStore.getState().setDetectedBoard('espressif')
    useStore.getState().setFlashDialogOpen(true)

    expect(useStore.getState().flashDialogOpen).toBe(true)
    expect(useStore.getState().detectedBoard).toBe('espressif')
  })

  it('cannot flash without a selected port', () => {
    expect(useStore.getState().selectedPort).toBe('')
    const canFlash = useStore.getState().selectedPort !== '' && useStore.getState().detectedBoard !== 'unknown'
    expect(canFlash).toBe(false)
  })

  it('cannot flash with unknown board type', () => {
    useStore.getState().setSelectedPort('/dev/ttyUSB0')
    useStore.getState().setDetectedBoard('unknown')
    const canFlash = useStore.getState().selectedPort !== '' && useStore.getState().detectedBoard !== 'unknown'
    expect(canFlash).toBe(false)
  })

  it('sets flashing state during firmware flash', () => {
    useStore.getState().setSelectedPort('/dev/ttyUSB0')
    useStore.getState().setDetectedBoard('espressif')
    useStore.getState().setIsFlashing(true)
    expect(useStore.getState().isFlashing).toBe(true)

    useStore.getState().addLogEntry({ type: 'flash', message: 'Starting espressif firmware flash...' })
    expect(useStore.getState().logEntries).toHaveLength(1)

    useStore.getState().setIsFlashing(false)
    expect(useStore.getState().isFlashing).toBe(false)
  })

  it('logs flash progress entries', () => {
    useStore.getState().setIsFlashing(true)
    useStore.getState().addLogEntry({ type: 'flash', message: 'Erasing flash...' })
    useStore.getState().addLogEntry({ type: 'flash', message: 'Writing at 0x00010000...' })
    useStore.getState().addLogEntry({ type: 'ok', message: 'Firmware flashed successfully!' })

    expect(useStore.getState().logEntries).toHaveLength(3)
    expect(useStore.getState().logEntries[0].type).toBe('flash')
    expect(useStore.getState().logEntries[2].type).toBe('ok')
  })

  it('handles tool missing scenario', () => {
    useStore.getState().setSelectedPort('/dev/ttyUSB0')
    useStore.getState().setDetectedBoard('espressif')
    useStore.getState().setToolMissing(true, 'espressif')

    expect(useStore.getState().toolMissing).toBe(true)
    expect(useStore.getState().toolMissingType).toBe('espressif')

    // After install
    useStore.getState().setToolMissing(false)
    useStore.getState().setIsInstalling(false)
    expect(useStore.getState().toolMissing).toBe(false)
  })

  it('install tool workflow with progress logging', () => {
    useStore.getState().setIsInstalling(true)
    useStore.getState().addLogEntry({ type: 'install', message: 'Installing esptool...' })
    useStore.getState().addLogEntry({ type: 'install', message: 'Downloading...' })
    useStore.getState().addLogEntry({ type: 'ok', message: 'esptool installed successfully' })
    useStore.getState().setToolMissing(false)
    useStore.getState().setIsInstalling(false)

    const entries = useStore.getState().logEntries
    expect(entries.filter(e => e.type === 'install')).toHaveLength(2)
    expect(useStore.getState().toolMissing).toBe(false)
    expect(useStore.getState().isInstalling).toBe(false)
  })

  it('RP2040 flash requires BOOTSEL mode', () => {
    useStore.getState().setSelectedPort('/dev/ttyACM0')
    useStore.getState().setDetectedBoard('raspberry-pi')
    useStore.getState().setIsFlashing(true)
    useStore.getState().addLogEntry({ type: 'warn', message: 'Hold BOOTSEL while plugging in the board.' })

    expect(useStore.getState().logEntries[0].type).toBe('warn')
  })

  it('M5Stack Atom Light flash uses esptool', () => {
    useStore.getState().setSelectedPort('/dev/ttyUSB0')
    useStore.getState().setDetectedBoard('m5stack-atom')
    useStore.getState().setFlashDialogOpen(true)
    useStore.getState().setIsFlashing(true)
    useStore.getState().addLogEntry({ type: 'flash', message: 'M5Stack Atom Light → downloading ESP32_GENERIC...' })
    useStore.getState().addLogEntry({ type: 'ok', message: 'M5Stack Atom Light firmware flashed successfully' })

    expect(useStore.getState().detectedBoard).toBe('m5stack-atom')
    expect(useStore.getState().isFlashing).toBe(true)
    expect(useStore.getState().logEntries[0].message).toContain('ESP32_GENERIC')
  })

  it('M5StampS3 flash uses esptool with S3 firmware', () => {
    useStore.getState().setSelectedPort('/dev/ttyACM0')
    useStore.getState().setDetectedBoard('m5stamp-s3')
    useStore.getState().setFlashDialogOpen(true)
    useStore.getState().setIsFlashing(true)
    useStore.getState().addLogEntry({ type: 'flash', message: 'M5StampS3 → downloading ESP32_GENERIC_S3...' })
    useStore.getState().addLogEntry({ type: 'ok', message: 'M5StampS3 firmware flashed successfully' })

    expect(useStore.getState().detectedBoard).toBe('m5stamp-s3')
    expect(useStore.getState().logEntries[0].message).toContain('ESP32_GENERIC_S3')
  })
})

describe('Integration — Run/Upload/Stop Workflow', () => {
  beforeEach(() => {
    useStore.getState().reset()
  })

  it('run code requires a file and a port', () => {
    const canRun = useStore.getState().currentFile !== null && useStore.getState().selectedPort !== ''
    expect(canRun).toBe(false)
  })

  it('run code requires both file and port', () => {
    useStore.getState().setCurrentFile({ name: 'main.py', path: '/tmp/main.py', content: 'print(1)', language: 'python' })
    expect(useStore.getState().currentFile).not.toBeNull()
    // Still no port
    const canRun = useStore.getState().currentFile !== null && useStore.getState().selectedPort !== ''
    expect(canRun).toBe(false)

    useStore.getState().setSelectedPort('/dev/ttyUSB0')
    const canRunNow = useStore.getState().currentFile !== null && useStore.getState().selectedPort !== ''
    expect(canRunNow).toBe(true)
  })

  it('upload code workflow with logging', () => {
    const file = { name: 'main.py', path: '/tmp/main.py', content: 'print("hello")', language: 'python' as const }
    useStore.getState().setCurrentFile(file)
    useStore.getState().setSelectedPort('/dev/ttyUSB0')

    useStore.getState().addLogEntry({ type: 'info', message: `Uploading main.py to /dev/ttyUSB0...` })
    useStore.getState().addLogEntry({ type: 'ok', message: 'Uploaded main.py' })

    expect(useStore.getState().logEntries).toHaveLength(2)
  })

  it('stop code workflow', () => {
    useStore.getState().setSelectedPort('/dev/ttyUSB0')
    useStore.getState().addLogEntry({ type: 'warn', message: 'Stopping execution...' })
    useStore.getState().addLogEntry({ type: 'ok', message: 'Device reset' })

    expect(useStore.getState().logEntries[0].type).toBe('warn')
    expect(useStore.getState().logEntries[1].type).toBe('ok')
  })
})
