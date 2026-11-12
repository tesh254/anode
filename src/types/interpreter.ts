export type RuntimeEnvironment = 'MicroPython' | 'CircuitPython' | 'Arduino'

export type WorkspaceMode = 'micropython' | 'circuitpython' | 'arduino'

export interface ExecutionProfile {
  id: string
  runtime: RuntimeEnvironment
  variant: string
  boardName: string
  portPath: string
  fqbn?: string
  isGeneric: boolean
  confidence: 'high' | 'low'
}

export interface ProfileDetectionInput {
  vendorId?: string
  productId?: string
  manufacturer?: string
  path?: string
  // Set when a CIRCUITPY drive is mounted (from the main-process `detect-
  // circuitpython` probe). This is the authoritative CircuitPython signal and
  // outranks USB-descriptor guesses — a bare ESP32-S3 looks identical to
  // MicroPython over USB, but a mounted CIRCUITPY drive only ever means CP.
  circuitPython?: { boardName?: string; version?: string }
}

export interface KnownBoardSignature {
  vid: string
  pidPrefix?: string
  runtime: RuntimeEnvironment
  variant: string
  boardName: string
  fqbn?: string
}

export const KNOWN_BOARD_SIGNATURES: KnownBoardSignature[] = [
  {
    vid: '10c4',
    pidPrefix: 'ea60',
    runtime: 'MicroPython',
    variant: 'ESP32',
    boardName: 'M5Stack Atom Light',
  },
  {
    vid: '303a',
    pidPrefix: '80',
    runtime: 'MicroPython',
    variant: 'ESP32-S3',
    boardName: 'M5StampS3',
  },
  {
    vid: '2e8a',
    pidPrefix: '0005',
    runtime: 'MicroPython',
    variant: 'RP2040',
    boardName: 'Raspberry Pi Pico',
  },
  {
    // Adafruit's USB vendor id — these boards ship CircuitPython by default.
    vid: '239a',
    runtime: 'CircuitPython',
    variant: 'Adafruit',
    boardName: 'Adafruit CircuitPython',
  },
  {
    vid: '2341',
    runtime: 'Arduino',
    variant: 'AVR',
    boardName: 'Arduino Uno / Genuine',
    fqbn: 'arduino:avr:uno',
  },
  {
    vid: '2a03',
    runtime: 'Arduino',
    variant: 'AVR',
    boardName: 'Arduino / Genuino',
    fqbn: 'arduino:avr:uno',
  },
  {
    vid: '1a86',
    runtime: 'MicroPython',
    variant: 'Generic ESP',
    boardName: 'CH340 USB Bridge (Generic ESP)',
  },
  {
    vid: '10c4',
    runtime: 'MicroPython',
    variant: 'Generic ESP',
    boardName: 'CP210x USB Bridge (Generic ESP)',
  },
  {
    vid: '303a',
    runtime: 'MicroPython',
    variant: 'ESP32-S3',
    boardName: 'Espressif USB (Generic ESP32-S3)',
  },
]

export const GENERIC_PROFILES: Omit<ExecutionProfile, 'portPath'>[] = [
  {
    id: 'micropython-generic',
    runtime: 'MicroPython',
    variant: 'Generic',
    boardName: 'MicroPython (generic)',
    isGeneric: true,
    confidence: 'low',
  },
  {
    id: 'circuitpython-generic',
    runtime: 'CircuitPython',
    variant: 'Generic',
    boardName: 'CircuitPython (generic)',
    isGeneric: true,
    confidence: 'low',
  },
  {
    id: 'arduino-generic',
    runtime: 'Arduino',
    variant: 'Generic',
    boardName: 'Arduino (generic)',
    fqbn: 'arduino:avr:uno',
    isGeneric: true,
    confidence: 'low',
  },
]

export function runtimeToWorkspaceMode(runtime: RuntimeEnvironment): WorkspaceMode {
  switch (runtime) {
    case 'MicroPython':
      return 'micropython'
    case 'CircuitPython':
      return 'circuitpython'
    case 'Arduino':
      return 'arduino'
  }
}

export interface WorkspaceLayout {
  showDeviceFileTree: boolean
  showRunButton: boolean
  showCompileButton: boolean
  showUploadButton: boolean
  terminalMode: 'repl' | 'serial-monitor'
  executionPipeline: 'mpremote' | 'arduino-cli'
}

export function getWorkspaceLayout(mode: WorkspaceMode): WorkspaceLayout {
  switch (mode) {
    case 'micropython':
    case 'circuitpython':
      return {
        showDeviceFileTree: true,
        showRunButton: true,
        showCompileButton: false,
        showUploadButton: true,
        terminalMode: 'repl',
        executionPipeline: 'mpremote',
      }
    case 'arduino':
      return {
        showDeviceFileTree: false,
        showRunButton: false,
        showCompileButton: true,
        showUploadButton: true,
        terminalMode: 'serial-monitor',
        executionPipeline: 'arduino-cli',
      }
  }
}
