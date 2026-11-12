import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
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

const hardwareMock = {
  writeFile: vi.fn().mockResolvedValue({ success: true }),
  uploadCode: vi.fn().mockResolvedValue({ success: true }),
  runCode: vi.fn().mockResolvedValue({ success: true }),
  stopCode: vi.fn().mockResolvedValue({ success: true }),
  getSerialPorts: vi.fn().mockResolvedValue([
    { path: '/dev/ttyUSB0', manufacturer: 'Espressif', vendorId: '303a', productId: '8010' },
  ]),
  readDirectory: vi.fn(),
  readFile: vi.fn(),
  createFile: vi.fn(),
  createDirectory: vi.fn(),
  flashFirmware: vi.fn(),
  checkTool: vi.fn().mockResolvedValue({ available: true }),
  installTool: vi.fn(),
  onFlashProgress: vi.fn(() => () => {}),
  onInstallProgress: vi.fn(() => () => {}),
}

describe('App workflow headless', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    useStore.getState().reset()
    useStore.getState().setWelcomeScreenVisible(false)
    useStore.getState().setProjectPath('/tmp/demo')
    useStore.getState().setProjectFiles([
      { name: 'main.py', path: '/tmp/demo/main.py', content: 'print("main")', language: 'python' },
      { name: 'lib.py', path: '/tmp/demo/lib.py', content: 'print("lib")', language: 'python' },
    ])
    useStore.getState().setCurrentFile({
      name: 'main.py',
      path: '/tmp/demo/main.py',
      content: 'print("main")',
      language: 'python',
    })
    useStore.getState().setSelectedPort('/dev/ttyUSB0')
    useStore.getState().setDetectedBoard('m5stamp-s3')
    ;(window as unknown as { hardware: typeof hardwareMock }).hardware = hardwareMock
  })

  it('Save All writes every project file to disk', async () => {
    render(<App />)

    await userEvent.click(screen.getByText('Save All'))

    await waitFor(() => {
      expect(hardwareMock.writeFile).toHaveBeenCalledTimes(2)
    })
    expect(hardwareMock.writeFile).toHaveBeenNthCalledWith(1, '/tmp/demo/main.py', 'print("main")')
    expect(hardwareMock.writeFile).toHaveBeenNthCalledWith(2, '/tmp/demo/lib.py', 'print("lib")')
  })

  it('Run saves current file before sending code to board', async () => {
    render(<App />)

    await userEvent.click(screen.getByText('Run'))

    await waitFor(() => {
      expect(hardwareMock.writeFile).toHaveBeenCalledWith('/tmp/demo/main.py', 'print("main")')
      expect(hardwareMock.runCode).toHaveBeenCalledWith({
        portPath: '/dev/ttyUSB0',
        code: 'print("main")',
      })
    })
  })

  it('Upload Project saves all files then uploads whole project', async () => {
    render(<App />)

    const uploadButtons = screen.getAllByText('Upload')
    await userEvent.click(uploadButtons[0])

    await waitFor(() => {
      expect(hardwareMock.writeFile).toHaveBeenCalledTimes(2)
      expect(hardwareMock.uploadCode).toHaveBeenCalledTimes(2)
    })
    expect(hardwareMock.uploadCode).toHaveBeenNthCalledWith(1, {
      portPath: '/dev/ttyUSB0',
      fileName: 'main.py',
      code: 'print("main")',
    })
    expect(hardwareMock.uploadCode).toHaveBeenNthCalledWith(2, {
      portPath: '/dev/ttyUSB0',
      fileName: 'lib.py',
      code: 'print("lib")',
    })
  })

  it('board panel shows USB bridge metadata for detected board', async () => {
    render(<App />)

    await waitFor(() => {
      expect(screen.getAllByText('Espressif').length).toBeGreaterThan(0)
      expect(screen.getByText('303a:8010')).toBeInTheDocument()
    })
  })

  it('auto-selects real hardware port instead of debug console', async () => {
    hardwareMock.getSerialPorts.mockResolvedValueOnce([
      { path: '/dev/tty.debug-console' },
      { path: '/dev/cu.usbmodem11301', manufacturer: 'Espressif', vendorId: '303a', productId: '8010' },
    ])
    useStore.getState().setSelectedPort('')

    render(<App />)

    await waitFor(() => {
      expect(useStore.getState().selectedPort).toBe('/dev/cu.usbmodem11301')
      expect(useStore.getState().detectedBoard).toBe('m5stamp-s3')
    })
  })

  it('blocks flash on debug console port', async () => {
    hardwareMock.getSerialPorts.mockResolvedValueOnce([
      { path: '/dev/tty.debug-console' },
    ])
    useStore.getState().setSelectedPort('/dev/tty.debug-console')
    useStore.getState().setDetectedBoard('espressif')

    render(<App />)

    const flashButtons = await screen.findAllByText('Flash Firmware')
    await userEvent.click(flashButtons[0].closest('button')!)

    await waitFor(() => {
      expect(hardwareMock.flashFirmware).not.toHaveBeenCalled()
    })
  })
})
