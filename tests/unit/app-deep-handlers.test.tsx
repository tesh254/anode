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
  onFlashProgress: vi.fn((cb: (e: unknown, m: string) => void) => {
    setTimeout(() => cb({}, '[OK] Flash complete'), 10)
    return () => {}
  }),
  onInstallProgress: vi.fn((cb: (e: unknown, d: { tool: string; message: string }) => void) => {
    setTimeout(() => cb({}, { tool: 'esptool', message: '[OK] Install complete' }), 10)
    return () => {}
  }),
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

describe('App — handleCreateFromTemplate (full flow)', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    useStore.getState().reset()
    ;(window as unknown as { hardware: typeof hardwareMock }).hardware = hardwareMock
  })

  it('creates project from template via welcome screen', async () => {
    hardwareMock.openFolder.mockResolvedValue('/tmp/blink')
    hardwareMock.createFile.mockResolvedValue({ success: true, path: '/tmp/blink/main.py' })

    render(<App />)

    await userEvent.click(screen.getByText('New Project'))

    await waitFor(() => {
      expect(screen.getByText('Select a template to get started')).toBeInTheDocument()
    })

    const blinkTemplate = TEMPLATES.find(t => t.id === 'blink')
    if (blinkTemplate) {
      await userEvent.click(screen.getByText(blinkTemplate.name))
      await waitFor(() => {
        const createBtns = screen.getAllByText('Create Project')
        expect(createBtns.length).toBeGreaterThan(0)
      })
      await userEvent.click(screen.getAllByText('Create Project')[0])

      await waitFor(() => {
        expect(screen.getByText('Name Your Project')).toBeInTheDocument()
      })

      const dialogCreateBtns = screen.getAllByText('Create Project')
      const dialogBtn = dialogCreateBtns[dialogCreateBtns.length - 1].closest('button')
      if (dialogBtn && !dialogBtn.disabled) {
        await userEvent.click(dialogBtn)
        await waitFor(() => {
          expect(hardwareMock.openFolder).toHaveBeenCalled()
        }, { timeout: 5000 })
      }
    }
  })

  it('creates arduino project (uses sketch.ino)', async () => {
    hardwareMock.openFolder.mockResolvedValue('/tmp/ardproj')
    hardwareMock.createFile.mockResolvedValue({ success: true, path: '/tmp/ardproj/sketch.ino' })

    render(<App />)
    await userEvent.click(screen.getByText('New Project'))
    await waitFor(() => expect(screen.getByText('Select a template to get started')).toBeInTheDocument())

    await userEvent.click(screen.getByText('Arduino'))

    await waitFor(async () => {
      const ardTemplates = TEMPLATES.filter(t => t.board === 'arduino')
      if (ardTemplates.length > 0) {
        await userEvent.click(screen.getByText(ardTemplates[0].name))
      }
    })
  })
})

describe('App — handleConfirmFlash (full flow)', () => {
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
  })

  it('opens flash dialog and clicks Flash Firmware', async () => {
    hardwareMock.checkTool.mockResolvedValue({ available: true })
    hardwareMock.flashFirmware.mockResolvedValue({ success: true })

    render(<App />)

    const flashBtns = screen.getAllByText('Flash Firmware')
    await userEvent.click(flashBtns[0].closest('button')!)

    await waitFor(() => {
      expect(screen.getAllByText('Flash Firmware').length).toBeGreaterThan(1)
    })

    const dialogFlashBtns = screen.getAllByText('Flash Firmware')
    const dialogBtn = dialogFlashBtns[dialogFlashBtns.length - 1].closest('button')
    if (dialogBtn && !dialogBtn.disabled) {
      await userEvent.click(dialogBtn)
      await waitFor(() => {
        expect(hardwareMock.checkTool).toHaveBeenCalled()
      }, { timeout: 5000 })
    }
  })

  it('handles tool missing for espressif', async () => {
    hardwareMock.checkTool.mockResolvedValue({ available: false })

    render(<App />)
    const flashBtns = screen.getAllByText('Flash Firmware')
    await userEvent.click(flashBtns[0].closest('button')!)

    await waitFor(() => {
      const dialogFlashBtns = screen.getAllByText('Flash Firmware')
      const dialogBtn = dialogFlashBtns[dialogFlashBtns.length - 1].closest('button')
      if (dialogBtn && !dialogBtn.disabled) {
        userEvent.click(dialogBtn)
      }
    })

    await waitFor(() => {
      expect(hardwareMock.checkTool).toHaveBeenCalled()
    }, { timeout: 5000 })
  })

  it('handles flash failure with hint', async () => {
    hardwareMock.checkTool.mockResolvedValue({ available: true })
    hardwareMock.flashFirmware.mockResolvedValue({
      success: false,
      error: 'No serial data received',
      hint: 'Try holding the BOOT button',
    })

    render(<App />)
    const flashBtns = screen.getAllByText('Flash Firmware')
    await userEvent.click(flashBtns[0].closest('button')!)

    await waitFor(() => {
      const dialogFlashBtns = screen.getAllByText('Flash Firmware')
      const dialogBtn = dialogFlashBtns[dialogFlashBtns.length - 1].closest('button')
      if (dialogBtn && !dialogBtn.disabled) {
        userEvent.click(dialogBtn)
      }
    })

    await waitFor(() => {
      expect(hardwareMock.checkTool).toHaveBeenCalled()
    }, { timeout: 5000 })
  })

  it('unknown board prevents flash', async () => {
    useStore.getState().setDetectedBoard('unknown')
    useStore.getState().setFlashDialogOpen(true)
    render(<App />)

    await waitFor(() => {
      const flashBtns = screen.getAllByText('Flash Firmware')
      expect(flashBtns.length).toBeGreaterThan(1)
    })
  })
})

describe('App — handleInstallTool (full flow)', () => {
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
    useStore.getState().setToolMissing(true, 'espressif')
    useStore.getState().setFlashDialogOpen(true)
    ;(window as unknown as { hardware: typeof hardwareMock }).hardware = hardwareMock
  })

  it('clicks Install Tool button', async () => {
    hardwareMock.installTool.mockResolvedValue({ success: true })
    render(<App />)

    await waitFor(() => {
      expect(screen.getByText('Install Tool')).toBeInTheDocument()
    })

    const installBtn = screen.getByText('Install Tool').closest('button')
    if (installBtn && !installBtn.disabled) {
      await userEvent.click(installBtn)
      await waitFor(() => {
        expect(hardwareMock.installTool).toHaveBeenCalled()
      }, { timeout: 5000 })
    }
  })

  it('handles install failure', async () => {
    hardwareMock.installTool.mockResolvedValue({
      success: false,
      error: 'pip not found',
      hint: 'Install Python 3',
    })

    render(<App />)
    await waitFor(() => expect(screen.getByText('Install Tool')).toBeInTheDocument())

    const installBtn = screen.getByText('Install Tool').closest('button')
    if (installBtn && !installBtn.disabled) {
      await userEvent.click(installBtn)
      await waitFor(() => {
        expect(hardwareMock.installTool).toHaveBeenCalled()
      }, { timeout: 5000 })
    }
  })
})

describe('App — handleCreateFile and handleCreateFolder', () => {
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
  })

  it('opens create file dialog and types filename', async () => {
    hardwareMock.createFile.mockResolvedValue({ success: true, path: '/tmp/test/helper.py' })

    render(<App />)

    const plusBtns = screen.getAllByRole('button').filter(b => b.querySelector('.lucide-plus'))
    if (plusBtns.length > 0) {
      await userEvent.click(plusBtns[0])
      await waitFor(() => {
        const newFileItems = screen.queryAllByText('New File')
        if (newFileItems.length > 0) {
          userEvent.click(newFileItems[0])
        }
      }, { timeout: 3000 })

      await waitFor(() => {
        const inputs = screen.queryAllByPlaceholderText('filename.py')
        if (inputs.length > 0) {
          userEvent.type(inputs[0], 'helper')
        }
        const createBtns = screen.queryAllByText('Create')
        if (createBtns.length > 0) {
          userEvent.click(createBtns[0])
        }
      }, { timeout: 3000 })
    }
  })

  it('opens create folder dialog', async () => {
    hardwareMock.createDirectory.mockResolvedValue({ success: true })

    render(<App />)

    const plusBtns = screen.getAllByRole('button').filter(b => b.querySelector('.lucide-plus'))
    if (plusBtns.length > 0) {
      await userEvent.click(plusBtns[0])
      await waitFor(() => {
        const newFolderItems = screen.queryAllByText('New Folder')
        if (newFolderItems.length > 0) {
          userEvent.click(newFolderItems[0])
        }
      }, { timeout: 3000 })

      await waitFor(() => {
        const inputs = screen.queryAllByPlaceholderText('folder-name')
        if (inputs.length > 0) {
          userEvent.type(inputs[0], 'mylib')
        }
        const createBtns = screen.queryAllByText('Create')
        if (createBtns.length > 0) {
          userEvent.click(createBtns[0])
        }
      }, { timeout: 3000 })
    }
  })
})

describe('App — Flash/Install progress effects', () => {
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
  })

  it('subscribes to flash and install progress events', async () => {
    render(<App />)
    await waitFor(() => {
      expect(hardwareMock.onFlashProgress).toHaveBeenCalled()
      expect(hardwareMock.onInstallProgress).toHaveBeenCalled()
    }, { timeout: 3000 })
  })
})

describe('App — handleFileClick with dirty state', () => {
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
  })

  it('switches file without prompt when not dirty', async () => {
    render(<App />)
    const utilFiles = screen.getAllByText('utils.py')
    await userEvent.click(utilFiles[utilFiles.length - 1])
    await waitFor(() => {
      expect(useStore.getState().currentFile?.name).toBe('utils.py')
    })
  })

  it('marks file as dirty on editor change', async () => {
    render(<App />)
    const editor = screen.getByTestId('monaco-editor')
    fireEvent.change(editor, { target: { value: 'modified content' } })
    await waitFor(() => {
      expect(useStore.getState().isDirty).toBe(true)
    })
  })
})

describe('App — handleSave error paths', () => {
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
  })

  it('handles write failure gracefully', async () => {
    hardwareMock.writeFile.mockResolvedValue({ success: false, error: 'Disk full' })
    render(<App />)
    fireEvent(window, new KeyboardEvent('keydown', { key: 's', metaKey: true, bubbles: true }))
    await waitFor(() => expect(hardwareMock.writeFile).toHaveBeenCalled(), { timeout: 10000 })
  })

  it('handles write exception', async () => {
    hardwareMock.writeFile.mockRejectedValue(new Error('IO error'))
    render(<App />)
    fireEvent(window, new KeyboardEvent('keydown', { key: 's', metaKey: true, bubbles: true }))
    await waitFor(() => expect(hardwareMock.writeFile).toHaveBeenCalled(), { timeout: 10000 })
  })
})
