import { describe, it, expect } from 'vitest'
import {
  type RuntimeEnvironment,
  type WorkspaceMode,
  type ExecutionProfile,
  KNOWN_BOARD_SIGNATURES,
  GENERIC_PROFILES,
  runtimeToWorkspaceMode,
  getWorkspaceLayout,
} from '@/types/interpreter'
import { detectExecutionProfiles, selectBestProfile, useStore } from '@/lib/store'

describe('runtimeToWorkspaceMode', () => {
  it('maps MicroPython to micropython', () => {
    expect(runtimeToWorkspaceMode('MicroPython')).toBe('micropython')
  })
  it('maps CircuitPython to circuitpython', () => {
    expect(runtimeToWorkspaceMode('CircuitPython')).toBe('circuitpython')
  })
  it('maps Arduino to arduino', () => {
    expect(runtimeToWorkspaceMode('Arduino')).toBe('arduino')
  })
  it('covers all RuntimeEnvironment values', () => {
    const all: RuntimeEnvironment[] = ['MicroPython', 'CircuitPython', 'Arduino']
    all.forEach((r) => {
      const mode = runtimeToWorkspaceMode(r)
      expect(mode).toBeDefined()
    })
  })
})

describe('getWorkspaceLayout', () => {
  it('micropython mode shows device file tree, run button, and REPL', () => {
    const layout = getWorkspaceLayout('micropython')
    expect(layout.showDeviceFileTree).toBe(true)
    expect(layout.showRunButton).toBe(true)
    expect(layout.showCompileButton).toBe(false)
    expect(layout.showUploadButton).toBe(true)
    expect(layout.terminalMode).toBe('repl')
    expect(layout.executionPipeline).toBe('mpremote')
  })

  it('circuitpython mode shows device file tree, run button, and REPL', () => {
    const layout = getWorkspaceLayout('circuitpython')
    expect(layout.showDeviceFileTree).toBe(true)
    expect(layout.showRunButton).toBe(true)
    expect(layout.showCompileButton).toBe(false)
    expect(layout.terminalMode).toBe('repl')
    expect(layout.executionPipeline).toBe('mpremote')
  })

  it('arduino mode hides device file tree, shows compile button, serial monitor', () => {
    const layout = getWorkspaceLayout('arduino')
    expect(layout.showDeviceFileTree).toBe(false)
    expect(layout.showRunButton).toBe(false)
    expect(layout.showCompileButton).toBe(true)
    expect(layout.showUploadButton).toBe(true)
    expect(layout.terminalMode).toBe('serial-monitor')
    expect(layout.executionPipeline).toBe('arduino-cli')
  })

  it('covers all WorkspaceMode values', () => {
    const all: WorkspaceMode[] = ['micropython', 'circuitpython', 'arduino']
    all.forEach((m) => {
      const layout = getWorkspaceLayout(m)
      expect(layout).toBeDefined()
      expect(typeof layout.showDeviceFileTree).toBe('boolean')
      expect(typeof layout.executionPipeline).toBe('string')
    })
  })
})

describe('KNOWN_BOARD_SIGNATURES', () => {
  it('includes M5Stack Atom (CP210x)', () => {
    const sig = KNOWN_BOARD_SIGNATURES.find(s => s.vid === '10c4' && s.pidPrefix === 'ea60')
    expect(sig).toBeDefined()
    expect(sig?.runtime).toBe('MicroPython')
    expect(sig?.boardName).toContain('M5Stack')
  })

  it('includes M5StampS3', () => {
    const sig = KNOWN_BOARD_SIGNATURES.find(s => s.vid === '303a' && s.pidPrefix === '80')
    expect(sig).toBeDefined()
    expect(sig?.variant).toBe('ESP32-S3')
  })

  it('includes Arduino signatures with FQBN', () => {
    const arduinos = KNOWN_BOARD_SIGNATURES.filter(s => s.runtime === 'Arduino')
    expect(arduinos.length).toBeGreaterThan(0)
    arduinos.forEach(s => {
      expect(s.fqbn).toBeDefined()
    })
  })

  it('includes Raspberry Pi Pico', () => {
    const pico = KNOWN_BOARD_SIGNATURES.find(s => s.vid === '2e8a')
    expect(pico).toBeDefined()
    expect(pico?.variant).toBe('RP2040')
  })
})

describe('GENERIC_PROFILES', () => {
  it('has exactly 3 generic profiles', () => {
    expect(GENERIC_PROFILES).toHaveLength(3)
  })

  it('includes MicroPython generic', () => {
    const mp = GENERIC_PROFILES.find(p => p.runtime === 'MicroPython')
    expect(mp).toBeDefined()
    expect(mp?.isGeneric).toBe(true)
    expect(mp?.confidence).toBe('low')
  })

  it('includes CircuitPython generic', () => {
    const cp = GENERIC_PROFILES.find(p => p.runtime === 'CircuitPython')
    expect(cp).toBeDefined()
    expect(cp?.isGeneric).toBe(true)
  })

  it('includes Arduino generic with FQBN', () => {
    const ar = GENERIC_PROFILES.find(p => p.runtime === 'Arduino')
    expect(ar).toBeDefined()
    expect(ar?.fqbn).toBe('arduino:avr:uno')
    expect(ar?.isGeneric).toBe(true)
  })
})

describe('detectExecutionProfiles', () => {
  it('returns generic profiles when no VID/PID match', () => {
    const profiles = detectExecutionProfiles({ path: '/dev/ttyUSB0' })
    expect(profiles.length).toBeGreaterThanOrEqual(3)
    const generics = profiles.filter(p => p.isGeneric)
    expect(generics).toHaveLength(3)
  })

  it('detects M5Stack Atom with high confidence', () => {
    const profiles = detectExecutionProfiles({
      vendorId: '10c4',
      productId: 'ea60',
      path: '/dev/cu.usbserial-1410',
    })
    const m5 = profiles.find(p => p.boardName === 'M5Stack Atom Light')
    expect(m5).toBeDefined()
    expect(m5?.confidence).toBe('high')
    expect(m5?.isGeneric).toBe(false)
    expect(m5?.runtime).toBe('MicroPython')
    expect(m5?.portPath).toBe('/dev/cu.usbserial-1410')
  })

  it('detects M5StampS3 with high confidence', () => {
    const profiles = detectExecutionProfiles({
      vendorId: '303a',
      productId: '8001',
      path: '/dev/cu.usbmodem',
    })
    const stamp = profiles.find(p => p.boardName === 'M5StampS3')
    expect(stamp).toBeDefined()
    expect(stamp?.confidence).toBe('high')
    expect(stamp?.runtime).toBe('MicroPython')
  })

  it('detects Arduino with high confidence and FQBN', () => {
    const profiles = detectExecutionProfiles({
      vendorId: '2341',
      productId: '0001',
      path: '/dev/cu.usbmodem',
    })
    const arduino = profiles.find(p => p.runtime === 'Arduino' && !p.isGeneric)
    expect(arduino).toBeDefined()
    expect(arduino?.fqbn).toBe('arduino:avr:uno')
    expect(arduino?.confidence).toBe('high')
  })

  it('detects Adafruit (239a) as CircuitPython with high confidence', () => {
    const profiles = detectExecutionProfiles({
      vendorId: '239a',
      productId: '8014',
      path: '/dev/cu.usbmodem',
    })
    const cp = profiles.find(p => p.runtime === 'CircuitPython' && !p.isGeneric)
    expect(cp).toBeDefined()
    expect(cp?.confidence).toBe('high')
  })

  it('a mounted CIRCUITPY drive wins over the USB-descriptor guess on a bare ESP32-S3', () => {
    const profiles = detectExecutionProfiles({
      vendorId: '303a',
      productId: '1001', // bare S3 — would otherwise be guessed MicroPython
      path: '/dev/cu.usbmodem',
      circuitPython: { boardName: 'Seeed XIAO ESP32S3', version: '9.0.0' },
    })
    const best = selectBestProfile(profiles)
    expect(best?.runtime).toBe('CircuitPython')
    expect(best?.confidence).toBe('high')
    expect(best?.isGeneric).toBe(false)
    expect(best?.boardName).toContain('Seeed XIAO ESP32S3')
  })

  it('CIRCUITPY drive without a board name still yields a high-confidence CircuitPython profile', () => {
    const profiles = detectExecutionProfiles({
      path: '/dev/cu.usbmodem',
      circuitPython: {},
    })
    const best = selectBestProfile(profiles)
    expect(best?.runtime).toBe('CircuitPython')
    expect(best?.confidence).toBe('high')
  })

  it('detects Raspberry Pi Pico by VID', () => {
    const profiles = detectExecutionProfiles({
      vendorId: '2e8a',
      productId: '0005',
      path: '/dev/ttyACM0',
    })
    const pico = profiles.find(p => p.variant === 'RP2040' && !p.isGeneric)
    expect(pico).toBeDefined()
    expect(pico?.boardName).toContain('Pico')
  })

  it('detects by manufacturer string (arduino)', () => {
    const profiles = detectExecutionProfiles({
      manufacturer: 'Arduino LLC',
      path: '/dev/cu.usbmodem',
    })
    const arduino = profiles.find(p => p.runtime === 'Arduino' && !p.isGeneric)
    expect(arduino).toBeDefined()
    expect(arduino?.confidence).toBe('high')
  })

  it('detects by path pattern (pico)', () => {
    const profiles = detectExecutionProfiles({
      path: '/dev/ttyACM0-rpi-rp2',
    })
    const pico = profiles.find(p => p.variant === 'RP2040' && !p.isGeneric)
    expect(pico).toBeDefined()
  })

  it('always appends generic profiles at the tail', () => {
    const profiles = detectExecutionProfiles({
      vendorId: '10c4',
      productId: 'ea60',
      path: '/dev/cu.usbserial-1410',
    })
    const last3 = profiles.slice(-3)
    expect(last3.every(p => p.isGeneric)).toBe(true)
  })

  it('high-confidence profiles come before generic ones', () => {
    const profiles = detectExecutionProfiles({
      vendorId: '303a',
      productId: '8001',
      path: '/dev/cu.usbmodem',
    })
    const firstHighIdx = profiles.findIndex(p => p.confidence === 'high')
    const firstGenericIdx = profiles.findIndex(p => p.isGeneric)
    expect(firstHighIdx).toBeLessThan(firstGenericIdx)
  })

  it('sets portPath on all profiles', () => {
    const profiles = detectExecutionProfiles({
      vendorId: '10c4',
      path: '/dev/cu.usbserial-1410',
    })
    profiles.forEach(p => {
      expect(p.portPath).toBe('/dev/cu.usbserial-1410')
    })
  })

  it('handles empty input gracefully', () => {
    const profiles = detectExecutionProfiles({})
    expect(profiles.length).toBeGreaterThanOrEqual(3)
  })

  it('handles case-insensitive VID/PID', () => {
    const profiles = detectExecutionProfiles({
      vendorId: '10C4',
      productId: 'EA60',
      path: '/dev/cu.usbserial',
    })
    const m5 = profiles.find(p => p.boardName === 'M5Stack Atom Light')
    expect(m5).toBeDefined()
  })

  it('detects CH340 bridge as MicroPython', () => {
    const profiles = detectExecutionProfiles({
      vendorId: '1a86',
      productId: '7523',
      path: '/dev/cu.usbserial',
    })
    const ch = profiles.find(p => p.boardName.includes('CH340'))
    expect(ch).toBeDefined()
    expect(ch?.runtime).toBe('MicroPython')
  })

  it('produces unique profile IDs', () => {
    const profiles = detectExecutionProfiles({
      vendorId: '303a',
      productId: '8001',
      path: '/dev/cu.usbmodem',
    })
    const ids = profiles.map(p => p.id)
    const unique = new Set(ids)
    expect(unique.size).toBe(ids.length)
  })
})

describe('selectBestProfile', () => {
  it('returns null for empty array', () => {
    expect(selectBestProfile([])).toBeNull()
  })

  it('prefers high-confidence non-generic profiles', () => {
    const profiles = detectExecutionProfiles({
      vendorId: '10c4',
      productId: 'ea60',
      path: '/dev/cu.usbserial',
    })
    const best = selectBestProfile(profiles)
    expect(best).toBeDefined()
    expect(best?.confidence).toBe('high')
    expect(best?.isGeneric).toBe(false)
  })

  it('falls back to non-generic profiles', () => {
    const profiles: ExecutionProfile[] = [
      { id: 'g1', runtime: 'MicroPython', variant: 'Generic', boardName: 'MP', portPath: '/p', isGeneric: true, confidence: 'low' },
      { id: 's1', runtime: 'Arduino', variant: 'AVR', boardName: 'Arduino', portPath: '/p', isGeneric: false, confidence: 'low', fqbn: 'arduino:avr:uno' },
    ]
    const best = selectBestProfile(profiles)
    expect(best?.id).toBe('s1')
  })

  it('falls back to first generic if nothing else', () => {
    const profiles: ExecutionProfile[] = [
      { id: 'g1', runtime: 'MicroPython', variant: 'Generic', boardName: 'MP', portPath: '/p', isGeneric: true, confidence: 'low' },
    ]
    const best = selectBestProfile(profiles)
    expect(best?.id).toBe('g1')
  })
})

describe('store execution profile integration', () => {
  beforeEach(() => {
    useStore.getState().reset()
  })

  it('setAvailableProfiles stores profiles', () => {
    const profiles = detectExecutionProfiles({ vendorId: '10c4', productId: 'ea60', path: '/dev/p' })
    useStore.getState().setAvailableProfiles(profiles)
    expect(useStore.getState().availableProfiles).toHaveLength(profiles.length)
  })

  it('setActiveProfile updates workspace mode', () => {
    const profile: ExecutionProfile = {
      id: 'arduino-1',
      runtime: 'Arduino',
      variant: 'AVR',
      boardName: 'Arduino Uno',
      portPath: '/dev/cu.usbmodem',
      fqbn: 'arduino:avr:uno',
      isGeneric: false,
      confidence: 'high',
    }
    useStore.getState().setActiveProfile(profile)
    expect(useStore.getState().activeProfile?.id).toBe('arduino-1')
    expect(useStore.getState().workspaceMode).toBe('arduino')
    expect(useStore.getState().lastProfileId).toBe('arduino-1')
  })

  it('setActiveProfile null clears profile', () => {
    useStore.getState().setActiveProfile({
      id: 'mp-1', runtime: 'MicroPython', variant: 'ESP32', boardName: 'MP', portPath: '/p', isGeneric: false, confidence: 'high',
    })
    useStore.getState().setActiveProfile(null)
    expect(useStore.getState().activeProfile).toBeNull()
  })

  it('selectProfile picks from available profiles', () => {
    const profiles = detectExecutionProfiles({ vendorId: '2341', path: '/dev/cu.usbmodem' })
    useStore.getState().setAvailableProfiles(profiles)
    const arduinoId = profiles.find(p => p.runtime === 'Arduino' && !p.isGeneric)?.id
    expect(arduinoId).toBeDefined()
    useStore.getState().selectProfile(arduinoId!)
    expect(useStore.getState().activeProfile?.runtime).toBe('Arduino')
    expect(useStore.getState().workspaceMode).toBe('arduino')
  })

  it('selectProfile is a no-op for unknown id', () => {
    const profiles = detectExecutionProfiles({ path: '/dev/p' })
    useStore.getState().setAvailableProfiles(profiles)
    useStore.getState().selectProfile('nonexistent')
    expect(useStore.getState().activeProfile).toBeNull()
  })

  it('refreshProfiles generates profiles from port path', () => {
    useStore.getState().refreshProfiles('/dev/cu.usbserial-1410')
    expect(useStore.getState().availableProfiles.length).toBeGreaterThanOrEqual(3)
  })

  it('setWorkspaceMode updates mode directly', () => {
    useStore.getState().setWorkspaceMode('arduino')
    expect(useStore.getState().workspaceMode).toBe('arduino')
    useStore.getState().setWorkspaceMode('circuitpython')
    expect(useStore.getState().workspaceMode).toBe('circuitpython')
  })
})

describe('store multi-tab state', () => {
  beforeEach(() => {
    useStore.getState().reset()
  })

  it('openTab adds a new tab and sets active', () => {
    useStore.getState().openTab({ path: '/main.py', name: 'main.py', isDirty: false, scope: 'local', language: 'python' })
    expect(useStore.getState().openTabs).toHaveLength(1)
    expect(useStore.getState().activeTabPath).toBe('/main.py')
  })

  it('openTab does not duplicate existing tabs', () => {
    useStore.getState().openTab({ path: '/main.py', name: 'main.py', isDirty: false, scope: 'local', language: 'python' })
    useStore.getState().openTab({ path: '/main.py', name: 'main.py', isDirty: true, scope: 'local', language: 'python' })
    expect(useStore.getState().openTabs).toHaveLength(1)
    expect(useStore.getState().activeTabPath).toBe('/main.py')
  })

  it('openTab supports multiple tabs', () => {
    useStore.getState().openTab({ path: '/a.py', name: 'a.py', isDirty: false, scope: 'local', language: 'python' })
    useStore.getState().openTab({ path: '/b.py', name: 'b.py', isDirty: false, scope: 'local', language: 'python' })
    expect(useStore.getState().openTabs).toHaveLength(2)
    expect(useStore.getState().activeTabPath).toBe('/b.py')
  })

  it('closeTab removes tab and switches active to last remaining', () => {
    useStore.getState().openTab({ path: '/a.py', name: 'a.py', isDirty: false, scope: 'local', language: 'python' })
    useStore.getState().openTab({ path: '/b.py', name: 'b.py', isDirty: false, scope: 'local', language: 'python' })
    useStore.getState().closeTab('/b.py')
    expect(useStore.getState().openTabs).toHaveLength(1)
    expect(useStore.getState().activeTabPath).toBe('/a.py')
  })

  it('closeTab clears active when no tabs remain', () => {
    useStore.getState().openTab({ path: '/a.py', name: 'a.py', isDirty: false, scope: 'local', language: 'python' })
    useStore.getState().closeTab('/a.py')
    expect(useStore.getState().openTabs).toHaveLength(0)
    expect(useStore.getState().activeTabPath).toBeNull()
  })

  it('closeTab does not change active if closing non-active tab', () => {
    useStore.getState().openTab({ path: '/a.py', name: 'a.py', isDirty: false, scope: 'local', language: 'python' })
    useStore.getState().openTab({ path: '/b.py', name: 'b.py', isDirty: false, scope: 'local', language: 'python' })
    useStore.getState().setActiveTab('/a.py')
    useStore.getState().closeTab('/b.py')
    expect(useStore.getState().activeTabPath).toBe('/a.py')
  })

  it('setActiveTab sets the active path', () => {
    useStore.getState().openTab({ path: '/a.py', name: 'a.py', isDirty: false, scope: 'local', language: 'python' })
    useStore.getState().openTab({ path: '/b.py', name: 'b.py', isDirty: false, scope: 'local', language: 'python' })
    useStore.getState().setActiveTab('/a.py')
    expect(useStore.getState().activeTabPath).toBe('/a.py')
  })

  it('markTabClean marks specific tab as not dirty', () => {
    useStore.getState().openTab({ path: '/a.py', name: 'a.py', isDirty: true, scope: 'local', language: 'python' })
    useStore.getState().markTabClean('/a.py')
    expect(useStore.getState().openTabs[0].isDirty).toBe(false)
  })

  it('markTabDirty marks specific tab as dirty', () => {
    useStore.getState().openTab({ path: '/a.py', name: 'a.py', isDirty: false, scope: 'local', language: 'python' })
    useStore.getState().markTabDirty('/a.py')
    expect(useStore.getState().openTabs[0].isDirty).toBe(true)
  })

  it('openTab supports remote scope', () => {
    useStore.getState().openTab({ path: ':/main.py', name: 'main.py', isDirty: false, scope: 'remote', language: 'python' })
    expect(useStore.getState().openTabs[0].scope).toBe('remote')
  })
})

describe('store device file state', () => {
  beforeEach(() => {
    useStore.getState().reset()
  })

  it('setDeviceFiles stores entries', () => {
    useStore.getState().setDeviceFiles([
      { name: 'main.py', path: '/main.py', isDirectory: false, size: 100 },
      { name: 'lib', path: '/lib', isDirectory: true, size: 0 },
    ])
    expect(useStore.getState().deviceFiles).toHaveLength(2)
  })

  it('setDeviceFilesLoading toggles loading', () => {
    useStore.getState().setDeviceFilesLoading(true)
    expect(useStore.getState().deviceFilesLoading).toBe(true)
    useStore.getState().setDeviceFilesLoading(false)
    expect(useStore.getState().deviceFilesLoading).toBe(false)
  })

  it('setDeviceConnected toggles connected', () => {
    useStore.getState().setDeviceConnected(true)
    expect(useStore.getState().deviceConnected).toBe(true)
  })
})

describe('store auto-connect state', () => {
  beforeEach(() => {
    useStore.getState().reset()
  })

  it('autoConnect defaults to true', () => {
    expect(useStore.getState().autoConnect).toBe(true)
  })

  it('setAutoConnect toggles', () => {
    useStore.getState().setAutoConnect(false)
    expect(useStore.getState().autoConnect).toBe(false)
  })

  it('setLastPortPath stores path', () => {
    useStore.getState().setLastPortPath('/dev/cu.usbserial-1410')
    expect(useStore.getState().lastPortPath).toBe('/dev/cu.usbserial-1410')
  })

  it('setLastProfileId stores id', () => {
    useStore.getState().setLastProfileId('micropython-esp32')
    expect(useStore.getState().lastProfileId).toBe('micropython-esp32')
  })

  it('setConnectionDropped toggles dropped', () => {
    useStore.getState().setConnectionDropped(true)
    expect(useStore.getState().connectionDropped).toBe(true)
  })
})

describe('workspace layout integration with store', () => {
  beforeEach(() => {
    useStore.getState().reset()
  })

  it('switching to Arduino profile hides device tree and shows compile', () => {
    const profile: ExecutionProfile = {
      id: 'arduino-1',
      runtime: 'Arduino',
      variant: 'AVR',
      boardName: 'Arduino Uno',
      portPath: '/dev/cu.usbmodem',
      fqbn: 'arduino:avr:uno',
      isGeneric: false,
      confidence: 'high',
    }
    useStore.getState().setActiveProfile(profile)
    const layout = getWorkspaceLayout(useStore.getState().workspaceMode)
    expect(layout.showDeviceFileTree).toBe(false)
    expect(layout.showCompileButton).toBe(true)
    expect(layout.terminalMode).toBe('serial-monitor')
  })

  it('switching to MicroPython profile shows device tree and REPL', () => {
    const profile: ExecutionProfile = {
      id: 'mp-1',
      runtime: 'MicroPython',
      variant: 'ESP32',
      boardName: 'M5Stack Atom',
      portPath: '/dev/cu.usbserial',
      isGeneric: false,
      confidence: 'high',
    }
    useStore.getState().setActiveProfile(profile)
    const layout = getWorkspaceLayout(useStore.getState().workspaceMode)
    expect(layout.showDeviceFileTree).toBe(true)
    expect(layout.showRunButton).toBe(true)
    expect(layout.terminalMode).toBe('repl')
  })

  it('CircuitPython profile shows device tree', () => {
    const profile: ExecutionProfile = {
      id: 'cp-1',
      runtime: 'CircuitPython',
      variant: 'Generic',
      boardName: 'CircuitPython',
      portPath: '/dev/cu.usbmodem',
      isGeneric: true,
      confidence: 'low',
    }
    useStore.getState().setActiveProfile(profile)
    const layout = getWorkspaceLayout(useStore.getState().workspaceMode)
    expect(layout.showDeviceFileTree).toBe(true)
    expect(layout.executionPipeline).toBe('mpremote')
  })
})
