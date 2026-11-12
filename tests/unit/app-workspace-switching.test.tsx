import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import { useStore, detectExecutionProfiles } from '@/lib/store'
import type { ExecutionProfile } from '@/types/interpreter'

vi.mock('@monaco-editor/react', () => ({
  default: ({ value, onChange }: { value: string; onChange?: (v: string) => void }) => (
    <textarea
      data-testid="monaco-editor"
      value={value}
      onChange={(e) => onChange?.(e.target.value)}
    />
  ),
}))

vi.mock('next-themes', () => ({
  ThemeProvider: ({ children }: { children: React.ReactNode }) => <>{children}</>,
  useTheme: () => ({ theme: 'dark', setTheme: vi.fn() }),
}))

import App from '@/App'

const mockHardware = {
  getSerialPorts: vi.fn(async () => []),
  readDirectory: vi.fn(async () => []),
  readFile: vi.fn(async () => ({ content: '', language: 'python' as const })),
  writeFile: vi.fn(async () => ({ success: true })),
  createFile: vi.fn(async () => ({ success: true, path: '/tmp/x' })),
  createDirectory: vi.fn(async () => ({ success: true })),
  runCode: vi.fn(async () => ({ success: true })),
  uploadCode: vi.fn(async () => ({ success: true })),
  stopCode: vi.fn(async () => ({ success: true })),
  flashFirmware: vi.fn(async () => ({ success: true })),
  checkTool: vi.fn(async () => ({ available: true })),
  installTool: vi.fn(async () => ({ success: true })),
  openFolder: vi.fn(async () => null),
  replConnect: vi.fn(async () => ({ success: true })),
  replDisconnect: vi.fn(async () => ({ success: true })),
  deviceFsLs: vi.fn(async () => ({ success: true, entries: [] })),
  deviceFsCat: vi.fn(async () => ({ success: true, content: '' })),
  deviceFsPut: vi.fn(async () => ({ success: true })),
  deviceFsRm: vi.fn(async () => ({ success: true })),
  arduinoCompile: vi.fn(async () => ({ success: true, stdout: 'Done' })),
  arduinoUpload: vi.fn(async () => ({ success: true, stdout: 'Done' })),
  onFlashProgress: vi.fn(() => () => {}),
  onInstallProgress: vi.fn(() => () => {}),
}

function setupApp() {
  ;(window as unknown as { hardware: unknown }).hardware = mockHardware
}

describe('App — Workspace Context Switching', () => {
  beforeEach(() => {
    useStore.getState().reset()
    vi.clearAllMocks()
    setupApp()
    useStore.getState().setWelcomeScreenVisible(false)
    useStore.getState().setProjectPath('/tmp/test')
    useStore.getState().setProjectFiles([
      { name: 'main.py', path: '/tmp/test/main.py', content: 'print("hi")', language: 'python' },
    ])
    useStore.getState().setCurrentFile({
      name: 'main.py', path: '/tmp/test/main.py', content: 'print("hi")', language: 'python',
    })
  })

  it('shows Run button in MicroPython mode', () => {
    useStore.getState().setWorkspaceMode('micropython')
    render(<App />)
    expect(screen.getByText('Run')).toBeInTheDocument()
    expect(screen.queryByText('Compile')).not.toBeInTheDocument()
  })

  it('shows Compile button in Arduino mode and hides Run', () => {
    useStore.getState().setWorkspaceMode('arduino')
    render(<App />)
    expect(screen.getByText('Compile')).toBeInTheDocument()
    expect(screen.queryByText('Run')).not.toBeInTheDocument()
  })

  it('shows DEVICE panel in MicroPython mode', () => {
    useStore.getState().setWorkspaceMode('micropython')
    useStore.getState().setSelectedPort('/dev/cu.usbserial-1410')
    render(<App />)
    expect(screen.getByText('DEVICE')).toBeInTheDocument()
  })

  it('hides DEVICE panel in Arduino mode', () => {
    useStore.getState().setWorkspaceMode('arduino')
    useStore.getState().setSelectedPort('/dev/cu.usbmodem')
    render(<App />)
    expect(screen.queryByText('DEVICE')).not.toBeInTheDocument()
  })

  it('shows connection dropped banner when connectionDropped is true', () => {
    useStore.getState().setSelectedPort('/dev/cu.usbserial-1410')
    useStore.getState().setConnectionDropped(true)
    render(<App />)
    expect(screen.getByText(/Connection lost/i)).toBeInTheDocument()
  })

  it('does not show connection dropped banner when not dropped', () => {
    useStore.getState().setSelectedPort('/dev/cu.usbserial-1410')
    render(<App />)
    expect(screen.queryByText(/Connection lost/i)).not.toBeInTheDocument()
  })

  it('shows Runtime Profile selector when profiles are available', () => {
    const profiles = detectExecutionProfiles({ vendorId: '10c4', productId: 'ea60', path: '/dev/cu.usbserial-1410' })
    useStore.getState().setAvailableProfiles(profiles)
    useStore.getState().setSelectedPort('/dev/cu.usbserial-1410')
    render(<App />)
    expect(screen.getByText('Runtime Profile')).toBeInTheDocument()
  })

  it('shows Compile button and calls arduinoCompile when clicked', async () => {
    useStore.getState().setWorkspaceMode('arduino')
    const profile: ExecutionProfile = {
      id: 'arduino-1', runtime: 'Arduino', variant: 'AVR', boardName: 'Arduino Uno',
      portPath: '/dev/cu.usbmodem', fqbn: 'arduino:avr:uno', isGeneric: false, confidence: 'high',
    }
    useStore.getState().setActiveProfile(profile)
    useStore.getState().setSelectedPort('/dev/cu.usbmodem')
    render(<App />)
    const compileBtn = screen.getByText('Compile')
    fireEvent.click(compileBtn)
    await waitFor(() => {
      expect(mockHardware.arduinoCompile).toHaveBeenCalledWith({
        sketchPath: '/tmp/test',
        fqbn: 'arduino:avr:uno',
      })
    })
  })

  it('does not call arduinoCompile when no project is open', async () => {
    useStore.getState().setWorkspaceMode('arduino')
    useStore.getState().setProjectPath(null)
    render(<App />)
    const compileBtn = screen.getByText('Compile')
    expect(compileBtn).toBeDisabled()
  })

  it('shows editor tab bar when tabs are open', () => {
    useStore.getState().openTab({
      path: '/tmp/test/main.py', name: 'main.py', isDirty: false, scope: 'local', language: 'python',
    })
    render(<App />)
    const tabs = screen.getAllByText('main.py')
    expect(tabs.length).toBeGreaterThanOrEqual(2)
  })
})

describe('App — Profile Detection Integration', () => {
  beforeEach(() => {
    useStore.getState().reset()
    vi.clearAllMocks()
    setupApp()
    useStore.getState().setWelcomeScreenVisible(false)
    useStore.getState().setProjectPath('/tmp/test')
    useStore.getState().setProjectFiles([
      { name: 'main.py', path: '/tmp/test/main.py', content: 'print("hi")', language: 'python' },
    ])
    useStore.getState().setCurrentFile({
      name: 'main.py', path: '/tmp/test/main.py', content: 'print("hi")', language: 'python',
    })
  })

  it('shows high-confidence marker for known board profiles', () => {
    const profiles = detectExecutionProfiles({ vendorId: '2341', path: '/dev/cu.usbmodem' })
    useStore.getState().setAvailableProfiles(profiles)
    useStore.getState().setSelectedPort('/dev/cu.usbmodem')
    const arduinoProfile = profiles.find(p => p.runtime === 'Arduino' && !p.isGeneric)
    useStore.getState().setActiveProfile(arduinoProfile ?? null)
    render(<App />)
    expect(screen.getByText('Arduino')).toBeInTheDocument()
  })

  it('shows runtime badge for active profile', () => {
    const profiles = detectExecutionProfiles({ vendorId: '10c4', productId: 'ea60', path: '/dev/cu.usbserial' })
    useStore.getState().setAvailableProfiles(profiles)
    useStore.getState().setSelectedPort('/dev/cu.usbserial')
    const best = profiles.find(p => p.confidence === 'high')
    useStore.getState().setActiveProfile(best ?? null)
    render(<App />)
    expect(screen.getByText('MicroPython')).toBeInTheDocument()
  })

  it('updates workspace mode when selecting Arduino profile', () => {
    const profiles = detectExecutionProfiles({ vendorId: '2341', path: '/dev/cu.usbmodem' })
    useStore.getState().setAvailableProfiles(profiles)
    useStore.getState().setSelectedPort('/dev/cu.usbmodem')
    const arduinoId = profiles.find(p => p.runtime === 'Arduino' && !p.isGeneric)?.id
    useStore.getState().selectProfile(arduinoId!)
    expect(useStore.getState().workspaceMode).toBe('arduino')
  })

  it('updates workspace mode when selecting MicroPython profile', () => {
    const profiles = detectExecutionProfiles({ vendorId: '10c4', productId: 'ea60', path: '/dev/cu.usbserial' })
    useStore.getState().setAvailableProfiles(profiles)
    useStore.getState().setSelectedPort('/dev/cu.usbserial')
    const mpId = profiles.find(p => p.confidence === 'high')?.id
    useStore.getState().selectProfile(mpId!)
    expect(useStore.getState().workspaceMode).toBe('micropython')
  })
})
