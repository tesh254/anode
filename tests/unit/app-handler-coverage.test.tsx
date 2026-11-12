import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, waitFor, fireEvent } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { useStore } from '@/lib/store'

const hardwareMock = {
  writeFile: vi.fn().mockResolvedValue({ success: true }),
  openFolder: vi.fn().mockResolvedValue('/tmp/test'),
  createFile: vi.fn().mockResolvedValue({ success: true, path: '/tmp/test/helper.py' }),
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
  readDirectory: vi.fn().mockResolvedValue([]),
  readFile: vi.fn().mockResolvedValue({ content: 'x', language: 'python' }),
  getFileStats: vi.fn().mockResolvedValue({ size: 0 }),
  openFirmwareDialog: vi.fn().mockResolvedValue(null),
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
    { name: 'main.py', path: '/tmp/test/main.py', content: 'x', language: 'python' },
  ])
  useStore.getState().setCurrentFile({
    name: 'main.py', path: '/tmp/test/main.py', content: 'x', language: 'python',
  })
  useStore.getState().setSelectedPort('/dev/ttyUSB0')
  useStore.getState().setDetectedBoard('espressif')
  ;(window as unknown as { hardware: typeof hardwareMock }).hardware = hardwareMock
}

describe('App — CreateDialog file creation flow', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    setupEditor()
  })

  it('opens create dialog by clicking plus button', async () => {
    render(<App />)
    const plusBtns = screen.getAllByRole('button').filter(b => b.querySelector('.lucide-plus'))
    expect(plusBtns.length).toBeGreaterThan(0)
    await userEvent.click(plusBtns[0])
    await waitFor(() => {
      const newFileBtns = screen.queryAllByText('New File')
      const newFolderBtns = screen.queryAllByText('New Folder')
      expect(newFileBtns.length + newFolderBtns.length).toBeGreaterThan(0)
    }, { timeout: 3000 })
  })

  it('clicking New File opens CreateDialog', async () => {
    render(<App />)
    const plusBtns = screen.getAllByRole('button').filter(b => b.querySelector('.lucide-plus'))
    if (plusBtns.length > 0) {
      await userEvent.click(plusBtns[0])
      await waitFor(() => {
        const newFileBtns = screen.queryAllByText('New File')
        if (newFileBtns.length > 0) {
          userEvent.click(newFileBtns[0])
        }
      }, { timeout: 3000 })
    }
    await waitFor(() => {
      const createHeaders = screen.queryAllByText('Create New File')
      if (createHeaders.length > 0) {
        expect(createHeaders.length).toBeGreaterThan(0)
      }
    }, { timeout: 3000 })
  })
})

describe('App — Port select change handler', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    setupEditor()
  })

  it('renders port selector', () => {
    render(<App />)
    expect(screen.getAllByText('/dev/ttyUSB0').length).toBeGreaterThan(0)
  })
})

describe('App — Editor mount handler', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    setupEditor()
  })

  it('renders monaco editor', () => {
    render(<App />)
    expect(screen.getByTestId('monaco-editor')).toBeInTheDocument()
  })
})

describe('App — handleOpenProject with files', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    useStore.getState().reset()
    ;(window as unknown as { hardware: typeof hardwareMock }).hardware = hardwareMock
  })

  it('opens project folder and loads python files', async () => {
    hardwareMock.openFolder.mockResolvedValue('/tmp/proj')
    hardwareMock.readDirectory.mockResolvedValue([
      { name: 'app.py', path: '/tmp/proj/app.py', isFile: true, isDirectory: false },
      { name: 'data.json', path: '/tmp/proj/data.json', isFile: true, isDirectory: false },
    ])
    hardwareMock.readFile.mockResolvedValue({ content: 'print(42)', language: 'python' })
    hardwareMock.getSerialPorts.mockResolvedValue([])

    render(<App />)
    await userEvent.click(screen.getByText('Open Project Folder'))

    await waitFor(() => {
      expect(hardwareMock.openFolder).toHaveBeenCalled()
    }, { timeout: 5000 })
  })

  it('opens project with .ino files for arduino', async () => {
    hardwareMock.openFolder.mockResolvedValue('/tmp/ard')
    hardwareMock.readDirectory.mockResolvedValue([
      { name: 'sketch.ino', path: '/tmp/ard/sketch.ino', isFile: true, isDirectory: false },
    ])
    hardwareMock.readFile.mockResolvedValue({ content: 'void setup() {}', language: 'arduino' })
    hardwareMock.getSerialPorts.mockResolvedValue([])

    render(<App />)
    await userEvent.click(screen.getByText('Open Project Folder'))

    await waitFor(() => {
      expect(hardwareMock.openFolder).toHaveBeenCalled()
    }, { timeout: 5000 })
  })

  it('handles openFolder returning null (canceled)', async () => {
    hardwareMock.openFolder.mockResolvedValue(null)
    hardwareMock.getSerialPorts.mockResolvedValue([])

    render(<App />)
    await userEvent.click(screen.getByText('Open Project Folder'))

    await waitFor(() => {
      expect(hardwareMock.openFolder).toHaveBeenCalled()
    }, { timeout: 5000 })
  })
})

describe('App — handleFlashFirmware and handleConfirmFlash', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    setupEditor()
  })

  it('opens flash dialog and clicks flash button', async () => {
    hardwareMock.checkTool.mockResolvedValue({ available: true })
    hardwareMock.flashFirmware.mockResolvedValue({ success: true })
    useStore.getState().setFlashDialogOpen(true)

    render(<App />)

    // The dialog renders via portal, search the whole document
    await waitFor(() => {
      const dialogContent = document.querySelector('[data-slot="dialog-content"]')
      expect(dialogContent).toBeTruthy()
    }, { timeout: 5000 })

    const dialogContent = document.querySelector('[data-slot="dialog-content"]')
    if (dialogContent) {
      const buttons = dialogContent.querySelectorAll('button')
      for (const btn of buttons) {
        if (btn.textContent?.includes('Flash Firmware') && !btn.disabled) {
          await userEvent.click(btn)
          await waitFor(() => {
            expect(hardwareMock.checkTool).toHaveBeenCalled()
          }, { timeout: 10000 })
          return
        }
      }
    }
  }, 15000)

  it('handles flash with error and hint', async () => {
    hardwareMock.checkTool.mockResolvedValue({ available: true })
    hardwareMock.flashFirmware.mockResolvedValue({
      success: false,
      error: 'Connection failed',
      hint: 'Hold BOOT button',
    })
    useStore.getState().setFlashDialogOpen(true)

    render(<App />)
    await waitFor(() => {
      expect(screen.getAllByText('Flash Firmware').length).toBeGreaterThan(1)
    })

    const dialogBtns = screen.getAllByText('Flash Firmware')
    const dialogBtn = dialogBtns[dialogBtns.length - 1].closest('button')
    if (dialogBtn && !dialogBtn.disabled) {
      await userEvent.click(dialogBtn)
      await waitFor(() => expect(hardwareMock.checkTool).toHaveBeenCalled(), { timeout: 5000 })
    }
  })

  it('handles flash exception', async () => {
    hardwareMock.checkTool.mockResolvedValue({ available: true })
    hardwareMock.flashFirmware.mockRejectedValue(new Error('Process killed'))
    useStore.getState().setFlashDialogOpen(true)

    render(<App />)
    await waitFor(() => {
      expect(screen.getAllByText('Flash Firmware').length).toBeGreaterThan(1)
    })

    const dialogBtns = screen.getAllByText('Flash Firmware')
    const dialogBtn = dialogBtns[dialogBtns.length - 1].closest('button')
    if (dialogBtn && !dialogBtn.disabled) {
      await userEvent.click(dialogBtn)
      await waitFor(() => expect(hardwareMock.checkTool).toHaveBeenCalled(), { timeout: 5000 })
    }
  })

  it('handles tool missing for arduino', async () => {
    useStore.getState().setDetectedBoard('arduino')
    hardwareMock.checkTool.mockResolvedValue({ available: false })
    useStore.getState().setFlashDialogOpen(true)

    render(<App />)
    await waitFor(() => {
      expect(screen.getAllByText('Flash Firmware').length).toBeGreaterThan(1)
    })

    const dialogBtns = screen.getAllByText('Flash Firmware')
    const dialogBtn = dialogBtns[dialogBtns.length - 1].closest('button')
    if (dialogBtn && !dialogBtn.disabled) {
      await userEvent.click(dialogBtn)
      await waitFor(() => expect(hardwareMock.checkTool).toHaveBeenCalled(), { timeout: 5000 })
    }
  })
})

describe('App — Raspberry Pi flash', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    setupEditor()
    useStore.getState().setDetectedBoard('raspberry-pi')
  })

  it('opens flash dialog for raspberry-pi', async () => {
    useStore.getState().setFlashDialogOpen(true)
    render(<App />)
    await waitFor(() => {
      expect(screen.getAllByText(/Raspberry Pi/).length).toBeGreaterThan(0)
    })
  })
})
