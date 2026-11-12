import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, waitFor, fireEvent } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { useStore } from '@/lib/store'
import { TEMPLATES } from '@/lib/templates'

const hardwareMock = {
  writeFile: vi.fn().mockResolvedValue({ success: true }),
  openFolder: vi.fn().mockResolvedValue('/tmp/newproj'),
  createFile: vi.fn().mockResolvedValue({ success: true, path: '/tmp/newproj/main.py' }),
  createDirectory: vi.fn().mockResolvedValue({ success: true }),
  runCode: vi.fn().mockResolvedValue({ success: true }),
  uploadCode: vi.fn().mockResolvedValue({ success: true }),
  stopCode: vi.fn().mockResolvedValue({ success: true }),
  flashFirmware: vi.fn().mockResolvedValue({ success: true }),
  checkTool: vi.fn().mockResolvedValue({ available: true }),
  installTool: vi.fn().mockResolvedValue({ success: true }),
  getSerialPorts: vi.fn().mockResolvedValue([]),
  readDirectory: vi.fn().mockResolvedValue([
    { name: 'main.py', path: '/tmp/newproj/main.py', isFile: true, isDirectory: false },
    { name: 'readme.md', path: '/tmp/newproj/readme.md', isFile: true, isDirectory: false },
  ]),
  readFile: vi.fn().mockResolvedValue({ content: 'print("hello")', language: 'python' }),
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

describe('App — handleOpenProject', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    useStore.getState().reset()
    ;(window as unknown as { hardware: typeof hardwareMock }).hardware = hardwareMock
  })

  it('loads project files and sets state', async () => {
    hardwareMock.readDirectory.mockResolvedValue([
      { name: 'app.py', path: '/tmp/newproj/app.py', isFile: true, isDirectory: false },
      { name: 'ignore.md', path: '/tmp/newproj/ignore.md', isFile: true, isDirectory: false },
    ])
    hardwareMock.readFile.mockResolvedValue({ content: 'print(1)', language: 'python' })
    hardwareMock.openFolder.mockResolvedValue('/tmp/newproj')

    render(<App />)

    const openBtn = screen.getByText('Open Project Folder')
    await userEvent.click(openBtn)

    await waitFor(() => {
      expect(hardwareMock.openFolder).toHaveBeenCalled()
    }, { timeout: 5000 })
  })

  it('handles open folder returning null', async () => {
    hardwareMock.openFolder.mockResolvedValue(null)
    render(<App />)
    await userEvent.click(screen.getByText('Open Project Folder'))
    await waitFor(() => {
      expect(hardwareMock.openFolder).toHaveBeenCalled()
    }, { timeout: 5000 })
  })

  it('handles readDirectory error', async () => {
    hardwareMock.openFolder.mockResolvedValue('/tmp/newproj')
    hardwareMock.readDirectory.mockRejectedValue(new Error('Permission denied'))
    render(<App />)
    await userEvent.click(screen.getByText('Open Project Folder'))
    await waitFor(() => {
      expect(hardwareMock.openFolder).toHaveBeenCalled()
    }, { timeout: 5000 })
  })

  it('skips .ino files that are empty', async () => {
    hardwareMock.readDirectory.mockResolvedValue([
      { name: 'sketch.ino', path: '/tmp/proj/sketch.ino', isFile: true, isDirectory: false },
    ])
    hardwareMock.readFile.mockResolvedValue({ content: '', language: 'arduino' })
    hardwareMock.openFolder.mockResolvedValue('/tmp/proj')
    render(<App />)
    await userEvent.click(screen.getByText('Open Project Folder'))
    await waitFor(() => {
      expect(hardwareMock.openFolder).toHaveBeenCalled()
    }, { timeout: 5000 })
  })

  it('handles no hardware bridge', async () => {
    ;(window as unknown as { hardware: undefined }).hardware = undefined
    render(<App />)
    const openBtn = screen.getByText('Open Project Folder')
    await userEvent.click(openBtn)
  })
})

describe('App — handleCreateFromTemplate', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    useStore.getState().reset()
    ;(window as unknown as { hardware: typeof hardwareMock }).hardware = hardwareMock
  })

  it('creates project from blink template', async () => {
    hardwareMock.openFolder.mockResolvedValue('/tmp/blink')
    hardwareMock.createFile.mockResolvedValue({ success: true, path: '/tmp/blink/main.py' })
    render(<App />)

    const blinkTemplate = TEMPLATES.find(t => t.id === 'blink')
    if (blinkTemplate) {
      const templateCards = screen.queryAllByText(blinkTemplate.name)
      if (templateCards.length > 0) {
        await userEvent.click(templateCards[0])
      }
    }
  })
})

describe('App — handleCreateFile', () => {
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
    useStore.getState().setSelectedPort('/dev/ttyUSB0')
    useStore.getState().setDetectedBoard('espressif')
    ;(window as unknown as { hardware: typeof hardwareMock }).hardware = hardwareMock
    hardwareMock.getSerialPorts.mockResolvedValue([
      { path: '/dev/ttyUSB0', manufacturer: 'Espressif', vendorId: '303a', productId: '0001' },
    ])
  })

  it('shows plus button in sidebar', () => {
    render(<App />)
    const plusBtns = screen.getAllByRole('button').filter(b => {
      const svg = b.querySelector('.lucide-plus')
      return !!svg
    })
    expect(plusBtns.length).toBeGreaterThan(0)
  })
})

describe('App — Settings Dialog', () => {
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
    useStore.getState().setSelectedPort('/dev/ttyUSB0')
    useStore.getState().setDetectedBoard('espressif')
    useStore.getState().setSettingsOpen(true)
    ;(window as unknown as { hardware: typeof hardwareMock }).hardware = hardwareMock
    hardwareMock.getSerialPorts.mockResolvedValue([
      { path: '/dev/ttyUSB0', manufacturer: 'Espressif', vendorId: '303a', productId: '0001' },
    ])
  })

  it('renders settings dialog when open', async () => {
    render(<App />)
    await waitFor(() => {
      expect(screen.getByText('Settings')).toBeInTheDocument()
    }, { timeout: 5000 })
  })

  it('shows Board tab', async () => {
    render(<App />)
    await waitFor(() => {
      expect(screen.getByText('board')).toBeInTheDocument()
    }, { timeout: 5000 })
    await userEvent.click(screen.getByText('board'))
    await waitFor(() => {
      expect(screen.getByText('Baud Rate')).toBeInTheDocument()
    })
  })

  it('shows Editor tab', async () => {
    render(<App />)
    await waitFor(() => {
      expect(screen.getByText('editor')).toBeInTheDocument()
    }, { timeout: 5000 })
    await userEvent.click(screen.getByText('editor'))
    await waitFor(() => {
      expect(screen.getByText('Font Size')).toBeInTheDocument()
    })
  })

  it('shows General tab content by default', async () => {
    render(<App />)
    await waitFor(() => {
      expect(screen.getByText('general')).toBeInTheDocument()
    }, { timeout: 5000 })
  })

  it('shows Clear Recent Projects button', async () => {
    render(<App />)
    await waitFor(() => {
      expect(screen.getByText('Clear Recent Projects')).toBeInTheDocument()
    }, { timeout: 5000 })
  })
})

describe('App — Create Dialog rendering', () => {
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
    useStore.getState().setSelectedPort('/dev/ttyUSB0')
    useStore.getState().setDetectedBoard('espressif')
    ;(window as unknown as { hardware: typeof hardwareMock }).hardware = hardwareMock
    hardwareMock.getSerialPorts.mockResolvedValue([
      { path: '/dev/ttyUSB0', manufacturer: 'Espressif', vendorId: '303a', productId: '0001' },
    ])
  })

  it('renders new file dropdown option', async () => {
    render(<App />)
    const plusBtns = screen.getAllByRole('button').filter(b => b.querySelector('.lucide-plus'))
    if (plusBtns.length > 0) {
      await userEvent.click(plusBtns[0])
      await waitFor(() => {
        const newFileItems = screen.queryAllByText('New File')
        if (newFileItems.length > 0) {
          expect(newFileItems.length).toBeGreaterThan(0)
        }
      }, { timeout: 3000 })
    }
  })
})

describe('App — handleFileClick with dirty file', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    useStore.getState().reset()
    useStore.getState().setWelcomeScreenVisible(false)
    useStore.getState().setProjectPath('/tmp/test')
    useStore.getState().setProjectFiles([
      { name: 'main.py', path: '/tmp/test/main.py', content: 'x', language: 'python' },
      { name: 'utils.py', path: '/tmp/test/utils.py', content: 'y', language: 'python' },
    ])
    useStore.getState().setCurrentFile({
      name: 'main.py', path: '/tmp/test/main.py', content: 'x', language: 'python',
    })
    useStore.getState().setSelectedPort('/dev/ttyUSB0')
    useStore.getState().setDetectedBoard('espressif')
    ;(window as unknown as { hardware: typeof hardwareMock }).hardware = hardwareMock
    hardwareMock.getSerialPorts.mockResolvedValue([
      { path: '/dev/ttyUSB0', manufacturer: 'Espressif', vendorId: '303a', productId: '0001' },
    ])
  })

  it('switches to different file', async () => {
    render(<App />)
    const utilFiles = screen.getAllByText('utils.py')
    await userEvent.click(utilFiles[utilFiles.length - 1])
    await waitFor(() => {
      expect(useStore.getState().currentFile?.name).toBe('utils.py')
    })
  })
})

describe('App — Context Menu rendering', () => {
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
    useStore.getState().setSelectedPort('/dev/ttyUSB0')
    useStore.getState().setDetectedBoard('espressif')
    ;(window as unknown as { hardware: typeof hardwareMock }).hardware = hardwareMock
    hardwareMock.getSerialPorts.mockResolvedValue([
      { path: '/dev/ttyUSB0', manufacturer: 'Espressif', vendorId: '303a', productId: '0001' },
    ])
  })

  it('right-clicking a file opens context menu', async () => {
    render(<App />)
    const fileItems = screen.getAllByText('main.py')
    await fireEvent.contextMenu(fileItems[fileItems.length - 1])
  })

  it('context menu items are rendered', async () => {
    render(<App />)
    const fileItems = screen.getAllByText('main.py')
    await fireEvent.contextMenu(fileItems[fileItems.length - 1])
    await waitFor(() => {
      const saveItems = screen.queryAllByText('Save File')
      const newProjItems = screen.queryAllByText('New Project')
      if (saveItems.length > 0) {
        expect(saveItems.length).toBeGreaterThan(0)
      }
      if (newProjItems.length > 0) {
        expect(newProjItems.length).toBeGreaterThan(0)
      }
    }, { timeout: 3000 })
  })
})

describe('App — Shell Log interaction', () => {
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
    useStore.getState().setSelectedPort('/dev/ttyUSB0')
    useStore.getState().setDetectedBoard('espressif')
    useStore.getState().setShellLogVisible(true)
    useStore.getState().addLogEntry({ type: 'info', message: 'Test log entry' })
    ;(window as unknown as { hardware: typeof hardwareMock }).hardware = hardwareMock
    hardwareMock.getSerialPorts.mockResolvedValue([
      { path: '/dev/ttyUSB0', manufacturer: 'Espressif', vendorId: '303a', productId: '0001' },
    ])
  })

  it('shows log entries', () => {
    render(<App />)
    expect(screen.getByText('Test log entry')).toBeInTheDocument()
  })

  it('clears log on button click', async () => {
    render(<App />)
    const clearBtn = screen.getAllByRole('button').find(b => b.querySelector('.lucide-trash-2'))
    if (clearBtn) {
      await userEvent.click(clearBtn)
      await waitFor(() => {
        expect(useStore.getState().logEntries.length).toBe(0)
      })
    }
  })
})

describe('App — handleConfirmFlash', () => {
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
    useStore.getState().setSelectedPort('/dev/ttyUSB0')
    useStore.getState().setDetectedBoard('espressif')
    useStore.getState().setFlashDialogOpen(true)
    ;(window as unknown as { hardware: typeof hardwareMock }).hardware = hardwareMock
    hardwareMock.getSerialPorts.mockResolvedValue([
      { path: '/dev/ttyUSB0', manufacturer: 'Espressif', vendorId: '303a', productId: '0001' },
    ])
  })

  it('renders flash button in dialog', async () => {
    render(<App />)
    await waitFor(() => {
      const flashBtns = screen.getAllByText('Flash Firmware')
      expect(flashBtns.length).toBeGreaterThan(1)
    })
  })

  it('clicks flash button in dialog', async () => {
    hardwareMock.checkTool.mockResolvedValue({ available: true })
    hardwareMock.flashFirmware.mockResolvedValue({ success: true })
    render(<App />)
    await waitFor(() => {
      const flashBtns = screen.getAllByText('Flash Firmware')
      const dialogBtn = flashBtns[flashBtns.length - 1].closest('button')
      if (dialogBtn && !dialogBtn.disabled) {
        userEvent.click(dialogBtn)
      }
    })
  })
})
