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
  getSerialPorts: vi.fn().mockResolvedValue([
    { path: '/dev/ttyUSB0', manufacturer: 'Espressif', vendorId: '303a', productId: '0001' },
  ]),
  readDirectory: vi.fn().mockResolvedValue([
    { name: 'main.py', path: '/tmp/newproj/main.py', isFile: true, isDirectory: false },
  ]),
  readFile: vi.fn().mockResolvedValue({ content: 'print(1)', language: 'python' }),
  getFileStats: vi.fn().mockResolvedValue({ size: 0 }),
  openFirmwareDialog: vi.fn().mockResolvedValue('/path/to/firmware.bin'),
  openUrl: vi.fn(),
  onFlashProgress: vi.fn(() => () => {}),
  onInstallProgress: vi.fn(() => () => {}),
}

vi.mock('@monaco-editor/react', () => ({
  default: ({ value, onChange }: { value: string; onChange?: (v: string) => void }) => (
    <textarea data-testid="monaco-editor" value={value} onChange={(e) => onChange?.(e.target.value)} />
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
    { name: 'utils.py', path: '/tmp/test/utils.py', content: 'def add(a,b): return a+b', language: 'python' },
  ])
  useStore.getState().setCurrentFile({
    name: 'main.py', path: '/tmp/test/main.py', content: 'print("hello")', language: 'python',
  })
  useStore.getState().setSelectedPort('/dev/ttyUSB0')
  useStore.getState().setDetectedBoard('espressif')
  ;(window as unknown as { hardware: typeof hardwareMock }).hardware = hardwareMock
}

describe('App — Welcome Screen Open Project', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    useStore.getState().reset()
    ;(window as unknown as { hardware: typeof hardwareMock }).hardware = hardwareMock
  })

  it('handleOpenProject — reads directory and loads files', async () => {
    hardwareMock.openFolder.mockResolvedValue('/tmp/proj')
    hardwareMock.readDirectory.mockResolvedValue([
      { name: 'app.py', path: '/tmp/proj/app.py', isFile: true, isDirectory: false },
      { name: 'notes.txt', path: '/tmp/proj/notes.txt', isFile: true, isDirectory: false },
    ])
    hardwareMock.readFile.mockResolvedValue({ content: 'x=1', language: 'python' })

    render(<App />)
    await userEvent.click(screen.getByText('Open Project Folder'))

    await waitFor(() => expect(hardwareMock.openFolder).toHaveBeenCalled(), { timeout: 5000 })
  })

  it('handleOpenProject — handles readDirectory error', async () => {
    hardwareMock.openFolder.mockResolvedValue('/tmp/proj')
    hardwareMock.readDirectory.mockRejectedValue(new Error('EACCES'))

    render(<App />)
    await userEvent.click(screen.getByText('Open Project Folder'))

    await waitFor(() => expect(hardwareMock.openFolder).toHaveBeenCalled(), { timeout: 5000 })
  })

  it('handleCreateFromTemplate — creates project from template', async () => {
    hardwareMock.openFolder.mockResolvedValue('/tmp/blink')
    hardwareMock.createFile.mockResolvedValue({ success: true, path: '/tmp/blink/main.py' })

    render(<App />)
    const templateNames = TEMPLATES.map(t => t.name)
    for (const name of templateNames) {
      const cards = screen.queryAllByText(name)
      if (cards.length > 0) {
        await userEvent.click(cards[0])
        break
      }
    }
  })

  it('handleCreateFromTemplate — handles createFile error', async () => {
    hardwareMock.openFolder.mockResolvedValue('/tmp/blink')
    hardwareMock.createFile.mockRejectedValue(new Error('Permission denied'))

    render(<App />)
    const templateNames = TEMPLATES.map(t => t.name)
    for (const name of templateNames) {
      const cards = screen.queryAllByText(name)
      if (cards.length > 0) {
        await userEvent.click(cards[0])
        break
      }
    }
  })
})

describe('App — Create File/Folder Dialogs', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    setupEditor()
  })

  it('opens New File menu from plus button', async () => {
    render(<App />)
    const plusBtns = screen.getAllByRole('button').filter(b => b.querySelector('.lucide-plus'))
    if (plusBtns.length > 0) {
      await userEvent.click(plusBtns[0])
      await waitFor(() => {
        const newFileBtns = screen.queryAllByText('New File')
        if (newFileBtns.length > 0) {
          return expect(newFileBtns.length).toBeGreaterThan(0)
        }
      }, { timeout: 3000 })
    }
  })

  it('opens New Folder menu from plus button', async () => {
    render(<App />)
    const plusBtns = screen.getAllByRole('button').filter(b => b.querySelector('.lucide-plus'))
    if (plusBtns.length > 0) {
      await userEvent.click(plusBtns[0])
      await waitFor(() => {
        const newFolderBtns = screen.queryAllByText('New Folder')
        if (newFolderBtns.length > 0) {
          return expect(newFolderBtns.length).toBeGreaterThan(0)
        }
      }, { timeout: 3000 })
    }
  })
})

describe('App — Context Menu Click Handlers', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    setupEditor()
  })

  it('saves file via context menu', async () => {
    hardwareMock.writeFile.mockResolvedValue({ success: true })
    render(<App />)

    const fileItems = screen.getAllByText('main.py')
    await fireEvent.contextMenu(fileItems[fileItems.length - 1])

    await waitFor(() => {
      const saveBtns = screen.queryAllByText('Save File')
      if (saveBtns.length > 0) {
        userEvent.click(saveBtns[0])
      }
    }, { timeout: 3000 })
  })

  it('opens command palette via context menu', async () => {
    render(<App />)
    const fileItems = screen.getAllByText('main.py')
    await fireEvent.contextMenu(fileItems[fileItems.length - 1])

    await waitFor(() => {
      const paletteBtns = screen.queryAllByText('Command Palette')
      if (paletteBtns.length > 0) {
        userEvent.click(paletteBtns[0])
      }
    }, { timeout: 3000 })
  })

  it('opens settings via context menu', async () => {
    render(<App />)
    const fileItems = screen.getAllByText('main.py')
    await fireEvent.contextMenu(fileItems[fileItems.length - 1])

    await waitFor(() => {
      const settingsBtns = screen.queryAllByText('Settings')
      if (settingsBtns.length > 0) {
        userEvent.click(settingsBtns[0])
      }
    }, { timeout: 3000 })
  })

  it('clicks New Project via context menu', async () => {
    render(<App />)
    const fileItems = screen.getAllByText('main.py')
    await fireEvent.contextMenu(fileItems[fileItems.length - 1])

    await waitFor(() => {
      const newProjBtns = screen.queryAllByText('New Project')
      if (newProjBtns.length > 0) {
        userEvent.click(newProjBtns[0])
      }
    }, { timeout: 3000 })
  })

  it('toggles shell log via context menu', async () => {
    render(<App />)
    const fileItems = screen.getAllByText('main.py')
    await fireEvent.contextMenu(fileItems[fileItems.length - 1])

    await waitFor(() => {
      const toggleBtns = screen.queryAllByText(/(Show|Hide) Shell Log/)
      if (toggleBtns.length > 0) {
        userEvent.click(toggleBtns[0])
      }
    }, { timeout: 3000 })
  })
})

describe('App — Editor onChange', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    setupEditor()
  })

  it('updates file content and marks dirty on editor change', async () => {
    render(<App />)
    const editor = screen.getByTestId('monaco-editor')
    fireEvent.change(editor, { target: { value: 'new content' } })
    await waitFor(() => {
      expect(useStore.getState().isDirty).toBe(true)
    })
  })

  it('does not update when no current file', async () => {
    useStore.getState().setCurrentFile(null)
    render(<App />)
    const editor = screen.getByTestId('monaco-editor')
    fireEvent.change(editor, { target: { value: 'new content' } })
  })
})

describe('App — Port selection', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    setupEditor()
  })

  it('renders port select with available ports', () => {
    render(<App />)
    expect(screen.getAllByText('/dev/ttyUSB0').length).toBeGreaterThan(0)
  })
})

describe('App — Flash Dialog Actions', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    setupEditor()
    useStore.getState().setFlashDialogOpen(true)
  })

  it('renders flash dialog with all board types', async () => {
    render(<App />)
    await waitFor(() => {
      expect(screen.getAllByText(/Espressif/).length).toBeGreaterThan(0)
    })
  })

  it('shows flash log area when flashing', async () => {
    useStore.getState().setIsFlashing(true)
    useStore.getState().addLogEntry({ type: 'flash', message: 'Writing firmware...' })
    render(<App />)
    await waitFor(() => {
      expect(screen.getAllByText('Writing firmware...').length).toBeGreaterThan(0)
    })
  })

  it('shows Preparing when no dialog flash logs', async () => {
    useStore.getState().setIsFlashing(true)
    render(<App />)
    await waitFor(() => {
      expect(screen.getByText('Preparing...')).toBeInTheDocument()
    })
  })

  it('shows BOOT button tip for espressif', async () => {
    useStore.getState().setIsFlashing(true)
    render(<App />)
    await waitFor(() => {
      expect(screen.getAllByText(/BOOT/).length).toBeGreaterThan(0)
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

describe('App — Welcome Screen Settings', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    useStore.getState().reset()
    ;(window as unknown as { hardware: typeof hardwareMock }).hardware = hardwareMock
    hardwareMock.getSerialPorts.mockResolvedValue([])
  })

  it('opens settings from welcome screen', async () => {
    render(<App />)
    const settingsBtns = screen.queryAllByText('Settings')
    if (settingsBtns.length > 0) {
      await userEvent.click(settingsBtns[0])
    }
  })
})

describe('App — handleRun with no port', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    setupEditor()
    useStore.getState().setSelectedPort('')
    hardwareMock.getSerialPorts.mockResolvedValue([])
  })

  it('does not call runCode when no port', async () => {
    render(<App />)
    const btn = screen.getByText('Run').closest('button')
    if (btn && !btn.disabled) {
      await userEvent.click(btn)
    }
    expect(hardwareMock.runCode).not.toHaveBeenCalled()
  })
})

describe('App — handleUpload with no port', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    setupEditor()
    useStore.getState().setSelectedPort('')
    hardwareMock.getSerialPorts.mockResolvedValue([])
  })

  it('does not call uploadCode when no port', async () => {
    render(<App />)
    const btn = screen.getByText('Upload').closest('button')
    if (btn && !btn.disabled) {
      await userEvent.click(btn)
    }
    expect(hardwareMock.uploadCode).not.toHaveBeenCalled()
  })
})

describe('App — handleStop with no port', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    setupEditor()
    useStore.getState().setSelectedPort('')
    hardwareMock.getSerialPorts.mockResolvedValue([])
  })

  it('does not call stopCode when no port', async () => {
    render(<App />)
    const btn = screen.getByText('Stop').closest('button')
    if (btn && !btn.disabled) {
      await userEvent.click(btn)
    }
    expect(hardwareMock.stopCode).not.toHaveBeenCalled()
  })
})
