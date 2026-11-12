import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, waitFor, fireEvent } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { useStore } from '@/lib/store'

const mockWriteFile = vi.fn()
const mockOpenFolder = vi.fn()
const mockCreateFile = vi.fn()
const mockRunCode = vi.fn()
const mockUploadCode = vi.fn()
const mockStopCode = vi.fn()
const mockFlashFirmware = vi.fn()
const mockCheckTool = vi.fn()
const mockInstallTool = vi.fn()
const mockGetSerialPorts = vi.fn()

const hardwareMock = {
  writeFile: mockWriteFile,
  openFolder: mockOpenFolder,
  createFile: mockCreateFile,
  runCode: mockRunCode,
  uploadCode: mockUploadCode,
  stopCode: mockStopCode,
  flashFirmware: mockFlashFirmware,
  checkTool: mockCheckTool,
  installTool: mockInstallTool,
  getSerialPorts: mockGetSerialPorts,
  readDirectory: vi.fn().mockResolvedValue([]),
  readFile: vi.fn().mockResolvedValue({ content: '', language: 'python' }),
  createDirectory: vi.fn().mockResolvedValue({ success: true }),
  getFileStats: vi.fn().mockResolvedValue({ size: 0 }),
  openFirmwareDialog: vi.fn().mockResolvedValue(null),
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

function setupStore() {
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
}

describe('App — IPC Handler Tests (window.hardware mock)', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockGetSerialPorts.mockResolvedValue([
      { path: '/dev/ttyUSB0', manufacturer: 'Espressif', vendorId: '303a', productId: '0001' },
    ])
    mockWriteFile.mockResolvedValue({ success: true })
    mockRunCode.mockResolvedValue({ success: true })
    mockUploadCode.mockResolvedValue({ success: true })
    mockStopCode.mockResolvedValue({ success: true })
    setupStore()
    ;(window as unknown as { hardware: typeof hardwareMock }).hardware = hardwareMock
  })

  describe('handleRun', () => {
    it('renders Run button and it is not disabled', async () => {
      mockRunCode.mockResolvedValue({ success: true })
      render(<App />)
      const runBtn = screen.getByText('Run').closest('button')
      expect(runBtn).toBeTruthy()
      expect(runBtn?.disabled).toBe(false)
    })

    it('calls hardware.runCode with correct params', async () => {
      mockRunCode.mockResolvedValue({ success: true })
      render(<App />)
      const runBtn = screen.getByText('Run').closest('button')!
      await userEvent.click(runBtn)
      await waitFor(() => {
        expect(mockRunCode).toHaveBeenCalledWith({
          portPath: '/dev/ttyUSB0',
          code: 'print("hello")',
        })
      })
    })

    it('does not call runCode when no current file', async () => {
      useStore.getState().setCurrentFile(null)
      mockGetSerialPorts.mockResolvedValue([])
      render(<App />)
      const runButtons = screen.queryAllByText('Run')
      for (const btn of runButtons) {
        if (btn.closest('button') && !btn.closest('button')?.disabled) {
          await userEvent.click(btn)
        }
      }
      expect(mockRunCode).not.toHaveBeenCalled()
    })

    it('does not call runCode when no port selected', async () => {
      useStore.getState().setSelectedPort('')
      mockGetSerialPorts.mockResolvedValue([])
      render(<App />)
      const btn = screen.getByText('Run').closest('button')
      if (btn && !btn.disabled) {
        await userEvent.click(btn)
      }
      expect(mockRunCode).not.toHaveBeenCalled()
    })
  })

  describe('handleUpload', () => {
    it('calls hardware.uploadCode with correct params', async () => {
      mockUploadCode.mockResolvedValue({ success: true })
      render(<App />)
      await userEvent.click(screen.getByText('Upload'))
      await waitFor(() => {
        expect(mockUploadCode).toHaveBeenCalledWith({
          portPath: '/dev/ttyUSB0',
          fileName: 'main.py',
          code: 'print("hello")',
        })
      })
    })

    it('does not call uploadCode when no file', async () => {
      useStore.getState().setCurrentFile(null)
      render(<App />)
      await userEvent.click(screen.getByText('Upload'))
      expect(mockUploadCode).not.toHaveBeenCalled()
    })
  })

  describe('handleStop', () => {
    it('calls hardware.stopCode with correct params', async () => {
      mockStopCode.mockResolvedValue({ success: true })
      render(<App />)
      await userEvent.click(screen.getByText('Stop'))
      await waitFor(() => {
        expect(mockStopCode).toHaveBeenCalledWith({ portPath: '/dev/ttyUSB0' })
      })
    })
  })

  describe('handleSaveFile', () => {
    it('calls hardware.writeFile on Ctrl+S', async () => {
      mockWriteFile.mockResolvedValue({ success: true })
      render(<App />)
      await userEvent.keyboard('{Meta>}s{/Meta}')
      await waitFor(() => {
        expect(mockWriteFile).toHaveBeenCalledWith('/tmp/test/main.py', 'print("hello")')
      })
    })

    it('saves file via keyboard shortcut', async () => {
      mockWriteFile.mockResolvedValue({ success: true })
      render(<App />)
      fireEvent(window, new KeyboardEvent('keydown', { key: 's', metaKey: true, bubbles: true }))
      await waitFor(() => expect(mockWriteFile).toHaveBeenCalled(), { timeout: 10000 })
    }, 15000)
  })

  describe('serial port polling', () => {
    it('polls serial ports on mount', async () => {
      mockGetSerialPorts.mockResolvedValue([
        { path: '/dev/ttyUSB0', manufacturer: 'Espressif', vendorId: '303a' },
      ])
      render(<App />)
      await waitFor(() => {
        expect(mockGetSerialPorts).toHaveBeenCalled()
      }, { timeout: 5000 })
    })
  })

  describe('command palette', () => {
    it('opens command palette with Cmd+K', async () => {
      render(<App />)
      await userEvent.keyboard('{Meta>}k{/Meta}')
      await waitFor(() => {
        expect(screen.getByText('Save File')).toBeInTheDocument()
      }, { timeout: 5000 })
    })
  })
})
