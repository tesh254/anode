import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import { useStore } from '@/lib/store'

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

function makeMockHardware(overrides: Record<string, unknown> = {}) {
  return {
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
    ...overrides,
  }
}

function setupApp(hw: Record<string, unknown> = {}) {
  const mock = makeMockHardware(hw)
  ;(window as unknown as { hardware: unknown }).hardware = mock
  return mock
}

describe('App — Arduino Compile/Upload Handlers', () => {
  beforeEach(() => {
    useStore.getState().reset()
  })

  it('calls arduinoCompile with project path and fqbn', async () => {
    const mock = setupApp()
    useStore.getState().setWelcomeScreenVisible(false)
    useStore.getState().setProjectPath('/tmp/arduino-proj')
    useStore.getState().setProjectFiles([
      { name: 'sketch.ino', path: '/tmp/arduino-proj/sketch.ino', content: 'void setup() {}', language: 'arduino' },
    ])
    useStore.getState().setCurrentFile({
      name: 'sketch.ino', path: '/tmp/arduino-proj/sketch.ino', content: 'void setup() {}', language: 'arduino',
    })
    useStore.getState().setWorkspaceMode('arduino')
    useStore.getState().setSelectedPort('/dev/cu.usbmodem')
    useStore.getState().setActiveProfile({
      id: 'arduino-1', runtime: 'Arduino', variant: 'AVR', boardName: 'Arduino',
      portPath: '/dev/cu.usbmodem', fqbn: 'arduino:avr:uno', isGeneric: false, confidence: 'high',
    })
    render(<App />)
    fireEvent.click(screen.getByText('Compile'))
    await waitFor(() => {
      expect(mock.arduinoCompile).toHaveBeenCalledWith({
        sketchPath: '/tmp/arduino-proj',
        fqbn: 'arduino:avr:uno',
      })
    })
  })

  it('logs error when arduinoCompile fails', async () => {
    const mock = setupApp({
      arduinoCompile: vi.fn(async () => ({ success: false, error: 'compile error' })),
    })
    useStore.getState().setWelcomeScreenVisible(false)
    useStore.getState().setProjectPath('/tmp/arduino-proj')
    useStore.getState().setProjectFiles([
      { name: 'sketch.ino', path: '/tmp/arduino-proj/sketch.ino', content: 'void setup() {}', language: 'arduino' },
    ])
    useStore.getState().setCurrentFile({
      name: 'sketch.ino', path: '/tmp/arduino-proj/sketch.ino', content: 'void setup() {}', language: 'arduino',
    })
    useStore.getState().setWorkspaceMode('arduino')
    useStore.getState().setSelectedPort('/dev/cu.usbmodem')
    render(<App />)
    fireEvent.click(screen.getByText('Compile'))
    await waitFor(() => {
      expect(mock.arduinoCompile).toHaveBeenCalled()
    })
    expect(useStore.getState().logEntries.some(l => l.type === 'error')).toBe(true)
  })

  it('calls arduinoUpload when Upload clicked in Arduino mode', async () => {
    const mock = setupApp()
    useStore.getState().setWelcomeScreenVisible(false)
    useStore.getState().setProjectPath('/tmp/arduino-proj')
    useStore.getState().setProjectFiles([
      { name: 'sketch.ino', path: '/tmp/arduino-proj/sketch.ino', content: 'void setup() {}', language: 'arduino' },
    ])
    useStore.getState().setCurrentFile({
      name: 'sketch.ino', path: '/tmp/arduino-proj/sketch.ino', content: 'void setup() {}', language: 'arduino',
    })
    useStore.getState().setWorkspaceMode('arduino')
    useStore.getState().setSelectedPort('/dev/cu.usbmodem')
    useStore.getState().setActiveProfile({
      id: 'arduino-1', runtime: 'Arduino', variant: 'AVR', boardName: 'Arduino',
      portPath: '/dev/cu.usbmodem', fqbn: 'arduino:avr:uno', isGeneric: false, confidence: 'high',
    })
    render(<App />)
    const uploadButtons = screen.getAllByText('Upload')
    fireEvent.click(uploadButtons[uploadButtons.length - 1])
    await waitFor(() => {
      expect(mock.arduinoUpload).toHaveBeenCalledWith({
        portPath: '/dev/cu.usbmodem',
        sketchPath: '/tmp/arduino-proj',
        fqbn: 'arduino:avr:uno',
      })
    })
  })

  it('shows toast when arduinoCompile throws', async () => {
    const mock = setupApp({
      arduinoCompile: vi.fn(async () => { throw new Error('crash') }),
    })
    useStore.getState().setWelcomeScreenVisible(false)
    useStore.getState().setProjectPath('/tmp/arduino-proj')
    useStore.getState().setProjectFiles([
      { name: 'sketch.ino', path: '/tmp/arduino-proj/sketch.ino', content: 'void setup() {}', language: 'arduino' },
    ])
    useStore.getState().setCurrentFile({
      name: 'sketch.ino', path: '/tmp/arduino-proj/sketch.ino', content: 'void setup() {}', language: 'arduino',
    })
    useStore.getState().setWorkspaceMode('arduino')
    useStore.getState().setSelectedPort('/dev/cu.usbmodem')
    render(<App />)
    fireEvent.click(screen.getByText('Compile'))
    await waitFor(() => {
      expect(mock.arduinoCompile).toHaveBeenCalled()
    })
    expect(useStore.getState().logEntries.some(l => l.type === 'error')).toBe(true)
  })
})

describe('App — Run/Upload/Stop Error Paths', () => {
  beforeEach(() => {
    useStore.getState().reset()
  })

  it('shows error when Run is clicked with no file open', async () => {
    const mock = setupApp()
    useStore.getState().setWelcomeScreenVisible(false)
    useStore.getState().setProjectPath('/tmp/test')
    useStore.getState().setSelectedPort('/dev/cu.usbserial')
    render(<App />)
    fireEvent.click(screen.getByText('Run'))
    await waitFor(() => {
      expect(mock.runCode).not.toHaveBeenCalled()
    })
  })

  it('shows error when Run is clicked with no port', async () => {
    const mock = setupApp()
    useStore.getState().setWelcomeScreenVisible(false)
    useStore.getState().setProjectPath('/tmp/test')
    useStore.getState().setProjectFiles([
      { name: 'main.py', path: '/tmp/test/main.py', content: 'print("hi")', language: 'python' },
    ])
    useStore.getState().setCurrentFile({
      name: 'main.py', path: '/tmp/test/main.py', content: 'print("hi")', language: 'python',
    })
    render(<App />)
    fireEvent.click(screen.getByText('Run'))
    await waitFor(() => {
      expect(mock.runCode).not.toHaveBeenCalled()
    })
  })

  it('logs error when runCode fails', async () => {
    const mock = setupApp({
      runCode: vi.fn(async () => ({ success: false, error: 'syntax error' })),
      writeFile: vi.fn(async () => ({ success: true })),
    })
    useStore.getState().setWelcomeScreenVisible(false)
    useStore.getState().setProjectPath('/tmp/test')
    useStore.getState().setProjectFiles([
      { name: 'main.py', path: '/tmp/test/main.py', content: 'print("hi")', language: 'python' },
    ])
    useStore.getState().setCurrentFile({
      name: 'main.py', path: '/tmp/test/main.py', content: 'print("hi")', language: 'python',
    })
    useStore.getState().setSelectedPort('/dev/cu.usbserial')
    render(<App />)
    fireEvent.click(screen.getByText('Run'))
    await waitFor(() => {
      expect(mock.runCode).toHaveBeenCalled()
    })
    expect(useStore.getState().logEntries.some(l => l.type === 'error')).toBe(true)
  })

  it('logs error when uploadCode fails', async () => {
    const mock = setupApp({
      uploadCode: vi.fn(async () => ({ success: false, error: 'upload failed' })),
      writeFile: vi.fn(async () => ({ success: true })),
    })
    useStore.getState().setWelcomeScreenVisible(false)
    useStore.getState().setProjectPath('/tmp/test')
    useStore.getState().setProjectFiles([
      { name: 'main.py', path: '/tmp/test/main.py', content: 'print("hi")', language: 'python' },
    ])
    useStore.getState().setCurrentFile({
      name: 'main.py', path: '/tmp/test/main.py', content: 'print("hi")', language: 'python',
    })
    useStore.getState().setSelectedPort('/dev/cu.usbserial')
    render(<App />)
    const uploadButtons = screen.getAllByText('Upload')
    fireEvent.click(uploadButtons[uploadButtons.length - 1])
    await waitFor(() => {
      expect(mock.uploadCode).toHaveBeenCalled()
    })
    expect(useStore.getState().logEntries.some(l => l.type === 'error')).toBe(true)
  })

  it('calls stopCode when Stop is clicked', async () => {
    const mock = setupApp()
    useStore.getState().setWelcomeScreenVisible(false)
    useStore.getState().setProjectPath('/tmp/test')
    useStore.getState().setSelectedPort('/dev/cu.usbserial')
    render(<App />)
    fireEvent.click(screen.getByText('Stop'))
    await waitFor(() => {
      expect(mock.stopCode).toHaveBeenCalled()
    })
  })

  it('shows error when Stop is clicked with no port', async () => {
    const mock = setupApp()
    useStore.getState().setWelcomeScreenVisible(false)
    useStore.getState().setProjectPath('/tmp/test')
    render(<App />)
    fireEvent.click(screen.getByText('Stop'))
    await waitFor(() => {
      expect(mock.stopCode).not.toHaveBeenCalled()
    })
  })
})

describe('App — File Click and Tab Management', () => {
  beforeEach(() => {
    useStore.getState().reset()
  })

  it('opens a tab when clicking a file in sidebar', async () => {
    setupApp()
    useStore.getState().setWelcomeScreenVisible(false)
    useStore.getState().setProjectPath('/tmp/test')
    useStore.getState().setProjectFiles([
      { name: 'main.py', path: '/tmp/test/main.py', content: 'print("hi")', language: 'python' },
      { name: 'utils.py', path: '/tmp/test/utils.py', content: 'def add(): pass', language: 'python' },
    ])
    useStore.getState().setCurrentFile({
      name: 'main.py', path: '/tmp/test/main.py', content: 'print("hi")', language: 'python',
    })
    render(<App />)
    const utilsLink = screen.getByText('utils.py')
    fireEvent.click(utilsLink)
    expect(useStore.getState().activeTabPath).toBe('/tmp/test/utils.py')
  })

  it('switches current file when selecting an open tab', async () => {
    setupApp()
    useStore.getState().setWelcomeScreenVisible(false)
    useStore.getState().setProjectPath('/tmp/test')
    useStore.getState().setProjectFiles([
      { name: 'a.py', path: '/tmp/test/a.py', content: 'a', language: 'python' },
      { name: 'b.py', path: '/tmp/test/b.py', content: 'b', language: 'python' },
    ])
    useStore.getState().setCurrentFile({
      name: 'a.py', path: '/tmp/test/a.py', content: 'a', language: 'python',
    })
    useStore.getState().openTab({ path: '/tmp/test/a.py', name: 'a.py', isDirty: false, scope: 'local', language: 'python' })
    useStore.getState().openTab({ path: '/tmp/test/b.py', name: 'b.py', isDirty: false, scope: 'local', language: 'python' })
    render(<App />)
    expect(useStore.getState().openTabs.length).toBe(2)
  })

  it('marks tab dirty when editor content changes', async () => {
    setupApp()
    useStore.getState().setWelcomeScreenVisible(false)
    useStore.getState().setProjectPath('/tmp/test')
    useStore.getState().setProjectFiles([
      { name: 'main.py', path: '/tmp/test/main.py', content: 'print("hi")', language: 'python' },
    ])
    useStore.getState().setCurrentFile({
      name: 'main.py', path: '/tmp/test/main.py', content: 'print("hi")', language: 'python',
    })
    useStore.getState().openTab({ path: '/tmp/test/main.py', name: 'main.py', isDirty: false, scope: 'local', language: 'python' })
    render(<App />)
    const editor = screen.getByTestId('monaco-editor')
    fireEvent.change(editor, { target: { value: 'print("modified")' } })
    expect(useStore.getState().isDirty).toBe(true)
  })
})

describe('App — Flash Firmware Flow', () => {
  beforeEach(() => {
    useStore.getState().reset()
  })

  it('opens flash dialog when Flash Firmware is clicked', async () => {
    setupApp()
    useStore.getState().setWelcomeScreenVisible(false)
    useStore.getState().setProjectPath('/tmp/test')
    useStore.getState().setProjectFiles([
      { name: 'main.py', path: '/tmp/test/main.py', content: 'print("hi")', language: 'python' },
    ])
    useStore.getState().setCurrentFile({
      name: 'main.py', path: '/tmp/test/main.py', content: 'print("hi")', language: 'python',
    })
    useStore.getState().setSelectedPort('/dev/cu.usbserial-1410')
    useStore.getState().setPorts([
      { path: '/dev/cu.usbserial-1410', vendorId: '10c4', productId: 'ea60', manufacturer: 'Silicon Labs' },
    ])
    useStore.getState().setDetectedBoard('m5stack-atom')
    render(<App />)
    const flashBtn = screen.getByText('Flash Firmware')
    fireEvent.click(flashBtn)
    await waitFor(() => {
      expect(useStore.getState().flashDialogOpen).toBe(true)
    })
  })

  it('shows error when Flash clicked with no port selected', async () => {
    const mock = setupApp()
    useStore.getState().setWelcomeScreenVisible(false)
    useStore.getState().setProjectPath('/tmp/test')
    useStore.getState().setProjectFiles([
      { name: 'main.py', path: '/tmp/test/main.py', content: 'print("hi")', language: 'python' },
    ])
    useStore.getState().setCurrentFile({
      name: 'main.py', path: '/tmp/test/main.py', content: 'print("hi")', language: 'python',
    })
    render(<App />)
    const flashBtn = screen.getByText('Flash Firmware')
    fireEvent.click(flashBtn)
    await waitFor(() => {
      expect(useStore.getState().flashDialogOpen).toBe(false)
    })
    expect(mock.flashFirmware).not.toHaveBeenCalled()
  })
})
