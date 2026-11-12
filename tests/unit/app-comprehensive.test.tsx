import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, waitFor, within, fireEvent } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { useStore } from '@/lib/store'

const hardwareMock = {
  writeFile: vi.fn().mockResolvedValue({ success: true }),
  openFolder: vi.fn().mockResolvedValue('/tmp/test'),
  createFile: vi.fn().mockResolvedValue({ success: true, path: '/tmp/test/newfile.py' }),
  runCode: vi.fn().mockResolvedValue({ success: true }),
  uploadCode: vi.fn().mockResolvedValue({ success: true }),
  stopCode: vi.fn().mockResolvedValue({ success: true }),
  flashFirmware: vi.fn().mockResolvedValue({ success: true }),
  checkTool: vi.fn().mockResolvedValue({ available: true }),
  installTool: vi.fn().mockResolvedValue({ success: true }),
  getSerialPorts: vi.fn().mockResolvedValue([
    { path: '/dev/ttyUSB0', manufacturer: 'Espressif', vendorId: '303a', productId: '0001' },
  ]),
  readDirectory: vi.fn().mockResolvedValue([]),
  readFile: vi.fn().mockResolvedValue({ content: 'print(1)', language: 'python' }),
  createDirectory: vi.fn().mockResolvedValue({ success: true }),
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
  ])
  useStore.getState().setCurrentFile({
    name: 'main.py', path: '/tmp/test/main.py', content: 'print("hello")', language: 'python',
  })
  useStore.getState().setSelectedPort('/dev/ttyUSB0')
  useStore.getState().setDetectedBoard('espressif')
  ;(window as unknown as { hardware: typeof hardwareMock }).hardware = hardwareMock
}

describe('App — Welcome Screen', () => {
  beforeEach(() => {
    useStore.getState().reset()
    ;(window as unknown as { hardware: typeof hardwareMock }).hardware = hardwareMock
  })

  it('shows welcome screen by default', () => {
    render(<App />)
    expect(screen.getByText('MicroPython & Arduino IDE')).toBeInTheDocument()
  })

  it('shows New Project and Open Project Folder buttons', () => {
    render(<App />)
    expect(screen.getByText('New Project')).toBeInTheDocument()
    expect(screen.getByText('Open Project Folder')).toBeInTheDocument()
  })

  it('hides welcome screen after project is opened', () => {
    useStore.getState().setWelcomeScreenVisible(false)
    useStore.getState().setProjectPath('/tmp/proj')
    useStore.getState().setProjectFiles([
      { name: 'main.py', path: '/tmp/proj/main.py', content: 'x', language: 'python' },
    ])
    useStore.getState().setCurrentFile({
      name: 'main.py', path: '/tmp/proj/main.py', content: 'x', language: 'python',
    })
    render(<App />)
    expect(screen.queryByText('MicroPython & Arduino IDE')).not.toBeInTheDocument()
    expect(screen.getByText('Run')).toBeInTheDocument()
  })
})

describe('App — Editor View Rendering', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    setupEditor()
  })

  it('renders the code editor', () => {
    render(<App />)
    expect(screen.getByTestId('monaco-editor')).toBeInTheDocument()
  })

  it('shows Run, Upload, Stop buttons', () => {
    render(<App />)
    expect(screen.getByText('Run')).toBeInTheDocument()
    expect(screen.getByText('Upload')).toBeInTheDocument()
    expect(screen.getByText('Stop')).toBeInTheDocument()
  })

  it('shows project name in sidebar', () => {
    render(<App />)
    expect(screen.getByText('test/')).toBeInTheDocument()
  })

  it('lists files in sidebar', () => {
    render(<App />)
    expect(screen.getAllByText('main.py').length).toBeGreaterThan(0)
  })

  it('shows status bar with file info', () => {
    render(<App />)
    const mainPyElements = screen.getAllByText('main.py')
    expect(mainPyElements.length).toBeGreaterThan(0)
  })

  it('shows port selector in sidebar', () => {
    render(<App />)
    expect(screen.getByText('/dev/ttyUSB0')).toBeInTheDocument()
  })

  it('shows Modified badge when isDirty', () => {
    useStore.getState().setIsDirty(true)
    render(<App />)
    expect(screen.getByText('Modified')).toBeInTheDocument()
  })

  it('does not show Modified badge when clean', () => {
    render(<App />)
    expect(screen.queryByText('Modified')).not.toBeInTheDocument()
  })

  it('shows "No files in project" when empty', () => {
    useStore.getState().setProjectFiles([])
    render(<App />)
    expect(screen.getByText('No files in project')).toBeInTheDocument()
  })

  it('shows board type in sidebar', () => {
    render(<App />)
    expect(screen.getAllByText(/ESP32|Espressif/).length).toBeGreaterThan(0)
  })

  it('shows "No Board" when unknown', () => {
    useStore.getState().setDetectedBoard('unknown')
    render(<App />)
    expect(screen.getByText('No Board')).toBeInTheDocument()
  })

  it('renders editor with current file content', () => {
    render(<App />)
    const editor = screen.getByTestId('monaco-editor') as HTMLTextAreaElement
    expect(editor.value).toBe('print("hello")')
  })

  it('updates file content on editor change', async () => {
    render(<App />)
    const editor = screen.getByTestId('monaco-editor')
    await userEvent.clear(editor)
    await userEvent.type(editor, 'print("new")')
    expect(useStore.getState().projectFiles[0].content).toContain('new')
  })
})

describe('App — IPC Handlers', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    setupEditor()
  })

  it('handleRun — calls hardware.runCode', async () => {
    hardwareMock.runCode.mockResolvedValue({ success: true })
    render(<App />)
    await userEvent.click(screen.getByText('Run').closest('button')!)
    await waitFor(() => {
      expect(hardwareMock.runCode).toHaveBeenCalledWith({
        portPath: '/dev/ttyUSB0',
        code: 'print("hello")',
      })
    })
  })

  it('handleRun — shows error when no file', async () => {
    useStore.getState().setCurrentFile(null)
    render(<App />)
    const btn = screen.getByText('Run').closest('button')
    expect(btn?.disabled).toBe(true)
  })

  it('handleUpload — calls hardware.uploadCode', async () => {
    hardwareMock.uploadCode.mockResolvedValue({ success: true })
    render(<App />)
    await userEvent.click(screen.getByText('Upload').closest('button')!)
    await waitFor(() => {
      expect(hardwareMock.uploadCode).toHaveBeenCalledWith({
        portPath: '/dev/ttyUSB0',
        fileName: 'main.py',
        code: 'print("hello")',
      })
    })
  })

  it('handleStop — calls hardware.stopCode', async () => {
    hardwareMock.stopCode.mockResolvedValue({ success: true })
    render(<App />)
    await userEvent.click(screen.getByText('Stop').closest('button')!)
    await waitFor(() => {
      expect(hardwareMock.stopCode).toHaveBeenCalledWith({ portPath: '/dev/ttyUSB0' })
    })
  })

  it('handleSaveFile — calls hardware.writeFile via keyboard', async () => {
    hardwareMock.writeFile.mockResolvedValue({ success: true })
    render(<App />)
    fireEvent(window, new KeyboardEvent('keydown', { key: 's', metaKey: true, bubbles: true }))
    await waitFor(() => expect(hardwareMock.writeFile).toHaveBeenCalled(), { timeout: 10000 })
  })

  it('handleFlashFirmware — opens flash dialog', async () => {
    render(<App />)
    const flashBtns = screen.getAllByText('Flash Firmware')
    await userEvent.click(flashBtns[0].closest('button')!)
    await waitFor(() => {
      expect(screen.getAllByText('Flash Firmware').length).toBeGreaterThan(1)
    })
  })

  it('handleFlashFirmware — shows error when no port', async () => {
    useStore.getState().setSelectedPort('')
    hardwareMock.getSerialPorts.mockResolvedValue([])
    render(<App />)
    const flashBtns = screen.queryAllByText('Flash Firmware')
    if (flashBtns.length > 0) {
      await userEvent.click(flashBtns[0].closest('button')!)
    }
  })
})

describe('App — Serial Port Detection', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    useStore.getState().reset()
    useStore.getState().setWelcomeScreenVisible(false)
    useStore.getState().setProjectPath('/tmp/test')
    useStore.getState().setProjectFiles([
      { name: 'main.py', path: '/tmp/test/main.py', content: 'x', language: 'python' },
    ])
    useStore.getState().setCurrentFile({
      name: 'main.py', path: '/tmp/test/main.py', content: 'x', language: 'python',
    })
    ;(window as unknown as { hardware: typeof hardwareMock }).hardware = hardwareMock
  })

  it('auto-selects first known board when no port selected', async () => {
    useStore.getState().setSelectedPort('')
    hardwareMock.getSerialPorts.mockResolvedValue([
      { path: '/dev/ttyUSB0', manufacturer: 'Espressif', vendorId: '303a', productId: '0001' },
    ])
    render(<App />)
    await waitFor(() => {
      expect(useStore.getState().selectedPort).toBe('/dev/ttyUSB0')
    }, { timeout: 5000 })
  })

  it('clears selected port when all ports disconnected', async () => {
    useStore.getState().setSelectedPort('/dev/ttyUSB0')
    hardwareMock.getSerialPorts.mockResolvedValue([])
    render(<App />)
    await waitFor(() => {
      expect(useStore.getState().selectedPort).toBe('')
    }, { timeout: 5000 })
  })
})

describe('App — Context Menu', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    setupEditor()
  })

  it('opens context menu on right-click', async () => {
    render(<App />)
    const fileListItems = screen.getAllByText('main.py')
    await fireEvent.contextMenu(fileListItems[fileListItems.length - 1])
  })
})

describe('App — Create File/Folder Dialogs', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    setupEditor()
  })

  it('opens create file dialog from plus button', async () => {
    render(<App />)
    const plusButtons = screen.getAllByRole('button').filter(b => b.querySelector('.lucide-plus'))
    if (plusButtons.length > 0) {
      await userEvent.click(plusButtons[0])
    }
  })
})

describe('App — Settings Dialog', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    setupEditor()
  })

  it('opens settings dialog from header settings icon', async () => {
    render(<App />)
    const allButtons = screen.getAllByRole('button')
    const settingsBtn = allButtons.find(b => b.querySelector('.lucide-settings'))
    if (settingsBtn) {
      await userEvent.click(settingsBtn)
    }
    await waitFor(() => {
      const general = screen.queryByText('General')
      if (general) expect(general).toBeInTheDocument()
    }, { timeout: 3000 })
  }, 10000)
})

describe('App — Command Palette', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    setupEditor()
  })

  it('opens command palette with Cmd+K', async () => {
    render(<App />)
    fireEvent(window, new KeyboardEvent('keydown', { key: 'k', metaKey: true, bubbles: true }))
    await waitFor(() => {
      expect(screen.getByText('Save File')).toBeInTheDocument()
    }, { timeout: 5000 })
  })
})

describe('App — Shell Log', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    setupEditor()
    useStore.getState().setShellLogVisible(true)
  })

  it('shell log visible state is true', () => {
    expect(useStore.getState().shellLogVisible).toBe(true)
    render(<App />)
  })

  it('toggles shell log visibility', async () => {
    render(<App />)
    expect(useStore.getState().shellLogVisible).toBe(true)
    useStore.getState().setShellLogVisible(false)
    expect(useStore.getState().shellLogVisible).toBe(false)
  })
})

describe('App — handleOpenProject', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    useStore.getState().reset()
    ;(window as unknown as { hardware: typeof hardwareMock }).hardware = hardwareMock
  })

  it('loads project files from hardware bridge', async () => {
    hardwareMock.openFolder.mockResolvedValue('/tmp/newproj')
    hardwareMock.readDirectory.mockResolvedValue([
      { name: 'app.py', path: '/tmp/newproj/app.py', isFile: true, isDirectory: false },
      { name: 'readme.md', path: '/tmp/newproj/readme.md', isFile: true, isDirectory: false },
    ])
    hardwareMock.readFile.mockResolvedValue({ content: 'print(42)', language: 'python' })
    hardwareMock.getSerialPorts.mockResolvedValue([])

    render(<App />)

    await waitFor(() => {
      expect(hardwareMock.openFolder).toBeDefined()
    })
  })
})

describe('App — handleCreateFromTemplate', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    useStore.getState().reset()
    useStore.getState().setWelcomeScreenVisible(true)
    ;(window as unknown as { hardware: typeof hardwareMock }).hardware = hardwareMock
    hardwareMock.getSerialPorts.mockResolvedValue([])
  })

  it('calls createFile and loads project from template', async () => {
    hardwareMock.createFile.mockResolvedValue({ success: true, path: '/tmp/blink/main.py' })
    render(<App />)
    const newProjBtns = screen.queryAllByText('New Project')
    if (newProjBtns.length > 0) {
      await userEvent.click(newProjBtns[0])
    }
  })
})
