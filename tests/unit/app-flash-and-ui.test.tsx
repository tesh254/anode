import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, waitFor, fireEvent } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { useStore } from '@/lib/store'

const hardwareMock = {
  writeFile: vi.fn().mockResolvedValue({ success: true }),
  openFolder: vi.fn().mockResolvedValue('/tmp/test'),
  createFile: vi.fn().mockResolvedValue({ success: true, path: '/tmp/test/newfile.py' }),
  createDirectory: vi.fn().mockResolvedValue({ success: true }),
  runCode: vi.fn(),
  uploadCode: vi.fn(),
  stopCode: vi.fn(),
  flashFirmware: vi.fn().mockResolvedValue({ success: true }),
  checkTool: vi.fn().mockResolvedValue({ available: true }),
  installTool: vi.fn().mockResolvedValue({ success: true }),
  getSerialPorts: vi.fn().mockResolvedValue([
    { path: '/dev/ttyUSB0', manufacturer: 'Espressif', vendorId: '303a', productId: '0001' },
  ]),
  readDirectory: vi.fn().mockResolvedValue([]),
  readFile: vi.fn().mockResolvedValue({ content: 'x', language: 'python' }),
  getFileStats: vi.fn().mockResolvedValue({ size: 0 }),
  openFirmwareDialog: vi.fn().mockResolvedValue('/path/to/firmware.bin'),
  openUrl: vi.fn(),
  onFlashProgress: vi.fn(() => () => {}),
  onInstallProgress: vi.fn(() => () => {}),
}

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

function setupEditor() {
  useStore.getState().reset()
  useStore.getState().setWelcomeScreenVisible(false)
  useStore.getState().setProjectPath('/tmp/test')
  useStore.getState().setProjectFiles([
    { name: 'main.py', path: '/tmp/test/main.py', content: 'print("hello")', language: 'python' },
    { name: 'utils.py', path: '/tmp/test/utils.py', content: 'def add(a, b): return a + b', language: 'python' },
  ])
  useStore.getState().setCurrentFile({
    name: 'main.py', path: '/tmp/test/main.py', content: 'print("hello")', language: 'python',
  })
  useStore.getState().setSelectedPort('/dev/ttyUSB0')
  useStore.getState().setDetectedBoard('espressif')
  ;(window as unknown as { hardware: typeof hardwareMock }).hardware = hardwareMock
}

describe('App — Flash Firmware Dialog', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    setupEditor()
  })

  it('opens flash dialog and shows board info', async () => {
    render(<App />)
    const flashBtns = screen.getAllByText('Flash Firmware')
    await userEvent.click(flashBtns[0].closest('button')!)
    await waitFor(() => {
      expect(screen.getAllByText(/Espressif/).length).toBeGreaterThan(0)
    })
  })

  it('shows tool missing state', async () => {
    useStore.getState().setToolMissing(true, 'espressif')
    useStore.getState().setFlashDialogOpen(true)
    render(<App />)
    await waitFor(() => {
      expect(screen.getAllByText('Tool Not Found').length).toBeGreaterThan(0)
    })
  })

  it('shows isFlashing state', async () => {
    useStore.getState().setIsFlashing(true)
    useStore.getState().setFlashDialogOpen(true)
    render(<App />)
    await waitFor(() => {
      expect(screen.getAllByText('Flashing...').length).toBeGreaterThan(0)
    })
  })

  it('shows isInstalling state', async () => {
    useStore.getState().setIsInstalling(true)
    useStore.getState().setFlashDialogOpen(true)
    render(<App />)
    await waitFor(() => {
      expect(screen.getAllByText('Installing...').length).toBeGreaterThan(0)
    })
  })

  it('unknown board disables flash', async () => {
    useStore.getState().setFlashDialogOpen(true)
    useStore.getState().setDetectedBoard('unknown')
    render(<App />)
    await waitFor(() => {
      const flashBtns = screen.getAllByText('Flash Firmware')
      const dialogBtn = flashBtns[flashBtns.length - 1].closest('button')
      expect(dialogBtn?.disabled).toBe(true)
    })
  })

  it('raspberry-pi board info', async () => {
    useStore.getState().setFlashDialogOpen(true)
    useStore.getState().setDetectedBoard('raspberry-pi')
    render(<App />)
    await waitFor(() => {
      expect(screen.getAllByText(/Raspberry Pi/).length).toBeGreaterThan(0)
    })
  })

  it('arduino board info', async () => {
    useStore.getState().setFlashDialogOpen(true)
    useStore.getState().setDetectedBoard('arduino')
    render(<App />)
    await waitFor(() => {
      expect(screen.getAllByText(/Arduino/).length).toBeGreaterThan(0)
    })
  })

  it('shows install tool button when tool missing', async () => {
    useStore.getState().setToolMissing(true, 'espressif')
    useStore.getState().setFlashDialogOpen(true)
    render(<App />)
    await waitFor(() => {
      expect(screen.getByText('Install Tool')).toBeInTheDocument()
    })
  })

  it('shows esptool install hint', async () => {
    useStore.getState().setToolMissing(true, 'espressif')
    useStore.getState().setFlashDialogOpen(true)
    render(<App />)
    await waitFor(() => {
      expect(screen.getAllByText(/esptool/).length).toBeGreaterThan(0)
    })
  })
})

describe('App — File Operations', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    setupEditor()
  })

  it('switches files when clicking file in sidebar', async () => {
    render(<App />)
    const utilFiles = screen.getAllByText('utils.py')
    await userEvent.click(utilFiles[utilFiles.length - 1])
    await waitFor(() => {
      expect(useStore.getState().currentFile?.name).toBe('utils.py')
    })
  })

  it('updates editor content on change', async () => {
    render(<App />)
    const editor = screen.getByTestId('monaco-editor')
    await userEvent.clear(editor)
    await userEvent.type(editor, 'x = 42')
    expect(useStore.getState().isDirty).toBe(true)
  })
})

describe('App — Status Bar', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    setupEditor()
  })

  it('shows port when connected', () => {
    render(<App />)
    expect(screen.getAllByText('/dev/ttyUSB0').length).toBeGreaterThan(0)
  })

  it('shows No Board when unknown', () => {
    useStore.getState().setDetectedBoard('unknown')
    render(<App />)
    expect(screen.getByText('No Board')).toBeInTheDocument()
  })
})

describe('App — handleRun error paths', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    setupEditor()
  })

  it('calls runCode and handles failure', async () => {
    hardwareMock.runCode.mockResolvedValue({ success: false, error: 'Board timeout' })
    render(<App />)
    await userEvent.click(screen.getByText('Run').closest('button')!)
    await waitFor(() => {
      expect(hardwareMock.runCode).toHaveBeenCalled()
    })
  })

  it('calls runCode and handles exception', async () => {
    hardwareMock.runCode.mockRejectedValue(new Error('Network error'))
    render(<App />)
    await userEvent.click(screen.getByText('Run').closest('button')!)
    await waitFor(() => {
      expect(hardwareMock.runCode).toHaveBeenCalled()
    })
  })

  it('handles upload failure', async () => {
    hardwareMock.uploadCode.mockResolvedValue({ success: false, error: 'Upload failed' })
    render(<App />)
    await userEvent.click(screen.getByText('Upload').closest('button')!)
    await waitFor(() => {
      expect(hardwareMock.uploadCode).toHaveBeenCalled()
    })
  })

  it('handles upload exception', async () => {
    hardwareMock.uploadCode.mockRejectedValue(new Error('Serial error'))
    render(<App />)
    await userEvent.click(screen.getByText('Upload').closest('button')!)
    await waitFor(() => {
      expect(hardwareMock.uploadCode).toHaveBeenCalled()
    })
  })

  it('handles stop exception', async () => {
    hardwareMock.stopCode.mockRejectedValue(new Error('Stop failed'))
    render(<App />)
    await userEvent.click(screen.getByText('Stop').closest('button')!)
    await waitFor(() => {
      expect(hardwareMock.stopCode).toHaveBeenCalled()
    })
  })
})

describe('App — Welcome Screen', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    useStore.getState().reset()
    ;(window as unknown as { hardware: typeof hardwareMock }).hardware = hardwareMock
    hardwareMock.getSerialPorts.mockResolvedValue([])
  })

  it('shows welcome screen by default', () => {
    render(<App />)
    expect(screen.getByText('New Project')).toBeInTheDocument()
  })
})

describe('App — Keyboard Shortcuts', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    setupEditor()
  })

  it('Cmd+K opens command palette', async () => {
    render(<App />)
    fireEvent(window, new KeyboardEvent('keydown', { key: 'k', metaKey: true, bubbles: true }))
    await waitFor(() => {
      expect(screen.getByText('Save File')).toBeInTheDocument()
    }, { timeout: 5000 })
  })
})

describe('App — Sidebar', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    setupEditor()
  })

  it('shows settings icon buttons', () => {
    render(<App />)
    const settingsBtns = screen.getAllByRole('button').filter(b => b.querySelector('.lucide-settings'))
    expect(settingsBtns.length).toBeGreaterThan(0)
  })

  it('shows Flash Firmware button in sidebar', () => {
    render(<App />)
    expect(screen.getAllByText('Flash Firmware').length).toBeGreaterThan(0)
  })

  it('shows file list in sidebar', () => {
    render(<App />)
    expect(screen.getAllByText('main.py').length).toBeGreaterThan(0)
    expect(screen.getAllByText('utils.py').length).toBeGreaterThan(0)
  })

  it('shows empty project message when no files', () => {
    useStore.getState().setProjectFiles([])
    render(<App />)
    expect(screen.getByText('No files in project')).toBeInTheDocument()
  })
})
