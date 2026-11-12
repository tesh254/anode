import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, fireEvent, act } from '@testing-library/react'
import { DeviceFileTree } from '@/components/device-file-tree'
import { useStore } from '@/lib/store'

vi.mock('@monaco-editor/react', () => ({
  default: () => null,
}))

vi.mock('next-themes', () => ({
  useTheme: () => ({ theme: 'dark', setTheme: vi.fn() }),
}))

const mockHardware = {
  deviceFsLs: vi.fn(),
  deviceFsCat: vi.fn(),
  deviceFsRm: vi.fn(),
  deviceFsPut: vi.fn(),
}

function setupHardware() {
  ;(window as unknown as { hardware: unknown }).hardware = mockHardware
}

// Helper: render and advance past the 1.5s debounce
async function renderAndFlush(ui: React.ReactElement) {
  vi.useFakeTimers()
  const result = render(ui)
  await act(async () => {
    vi.advanceTimersByTime(2000)
  })
  vi.useRealTimers()
  return result
}

describe('DeviceFileTree', () => {
  beforeEach(() => {
    useStore.getState().reset()
    vi.clearAllMocks()
    setupHardware()
  })

  it('shows "No port selected" when no port', () => {
    render(<DeviceFileTree />)
    expect(screen.getByText(/No port selected/i)).toBeInTheDocument()
  })

  it('calls deviceFsLs after debounce when port is selected', async () => {
    useStore.getState().setSelectedPort('/dev/cu.usbserial-1410')
    mockHardware.deviceFsLs.mockResolvedValue({
      success: true,
      entries: [{ name: 'main.py', path: '/main.py', isDirectory: false, size: 100 }],
    })
    await renderAndFlush(<DeviceFileTree />)
    expect(mockHardware.deviceFsLs).toHaveBeenCalledWith('/dev/cu.usbserial-1410', ':')
  })

  it('does not call deviceFsLs while flashing (avoids port-busy race)', async () => {
    useStore.getState().setSelectedPort('/dev/cu.usbserial-1410')
    useStore.getState().setIsFlashing(true)
    mockHardware.deviceFsLs.mockResolvedValue({ success: true, entries: [] })
    await renderAndFlush(<DeviceFileTree />)
    expect(mockHardware.deviceFsLs).not.toHaveBeenCalled()
  })

  it('does not call deviceFsLs while installing a tool', async () => {
    useStore.getState().setSelectedPort('/dev/cu.usbserial-1410')
    useStore.getState().setIsInstalling(true)
    mockHardware.deviceFsLs.mockResolvedValue({ success: true, entries: [] })
    await renderAndFlush(<DeviceFileTree />)
    expect(mockHardware.deviceFsLs).not.toHaveBeenCalled()
  })

  it('shows file entries when device returns files', async () => {
    useStore.getState().setSelectedPort('/dev/cu.usbserial-1410')
    mockHardware.deviceFsLs.mockResolvedValue({
      success: true,
      entries: [
        { name: 'main.py', path: '/main.py', isDirectory: false, size: 100 },
        { name: 'lib', path: '/lib', isDirectory: true, size: 0 },
      ],
    })
    await renderAndFlush(<DeviceFileTree />)
    expect(screen.getByText('main.py')).toBeInTheDocument()
    expect(screen.getByText('lib')).toBeInTheDocument()
  })

  it('shows loading message while fetching', async () => {
    useStore.getState().setSelectedPort('/dev/cu.usbserial-1410')
    vi.useFakeTimers()
    mockHardware.deviceFsLs.mockImplementation(() => new Promise(() => {}))
    render(<DeviceFileTree />)
    // Advance past debounce — loading state is set when the async call starts
    await act(async () => {
      vi.advanceTimersByTime(1600)
    })
    expect(screen.getByText(/Loading/i)).toBeInTheDocument()
    vi.useRealTimers()
  })

  it('shows not connected when deviceFsLs fails', async () => {
    useStore.getState().setSelectedPort('/dev/cu.usbserial-1410')
    mockHardware.deviceFsLs.mockResolvedValue({ success: false, entries: [], error: 'timeout' })
    await renderAndFlush(<DeviceFileTree />)
    expect(screen.getByText(/No files on device|Not connected/i)).toBeInTheDocument()
  })

  it('shows DEVICE header', () => {
    render(<DeviceFileTree />)
    expect(screen.getByText('DEVICE')).toBeInTheDocument()
  })

  it('refresh button calls deviceFsLs again', async () => {
    useStore.getState().setSelectedPort('/dev/cu.usbserial-1410')
    mockHardware.deviceFsLs.mockResolvedValue({ success: true, entries: [] })
    await renderAndFlush(<DeviceFileTree />)
    const callsBefore = mockHardware.deviceFsLs.mock.calls.length
    const refreshBtn = screen.getByTitle('Refresh device files')
    fireEvent.click(refreshBtn)
    await new Promise((r) => setTimeout(r, 50))
    expect(mockHardware.deviceFsLs.mock.calls.length).toBeGreaterThan(callsBefore)
  })

  it('does not call deviceFsLs when hardware is unavailable', async () => {
    ;(window as unknown as { hardware: unknown }).hardware = {}
    useStore.getState().setSelectedPort('/dev/cu.usbserial-1410')
    await renderAndFlush(<DeviceFileTree />)
    expect(mockHardware.deviceFsLs).not.toHaveBeenCalled()
  })

  it('sets deviceConnected to true on success', async () => {
    useStore.getState().setSelectedPort('/dev/cu.usbserial-1410')
    mockHardware.deviceFsLs.mockResolvedValue({
      success: true,
      entries: [{ name: 'boot.py', path: '/boot.py', isDirectory: false, size: 50 }],
    })
    await renderAndFlush(<DeviceFileTree />)
    expect(useStore.getState().deviceConnected).toBe(true)
  })

  it('sets deviceFiles in store on success', async () => {
    useStore.getState().setSelectedPort('/dev/cu.usbserial-1410')
    const entries = [{ name: 'main.py', path: '/main.py', isDirectory: false, size: 100 }]
    mockHardware.deviceFsLs.mockResolvedValue({ success: true, entries })
    await renderAndFlush(<DeviceFileTree />)
    expect(useStore.getState().deviceFiles).toEqual(entries)
  })

  it('shows context menu on right-click of file', async () => {
    useStore.getState().setSelectedPort('/dev/cu.usbserial-1410')
    mockHardware.deviceFsLs.mockResolvedValue({
      success: true,
      entries: [{ name: 'main.py', path: '/main.py', isDirectory: false, size: 100 }],
    })
    await renderAndFlush(<DeviceFileTree />)
    const fileItem = screen.getByText('main.py')
    fireEvent.contextMenu(fileItem)
    await new Promise((r) => setTimeout(r, 10))
    expect(screen.getByText('Download')).toBeInTheDocument()
    expect(screen.getByText('Delete')).toBeInTheDocument()
  })

  it('calls deviceFsRm when Delete is clicked', async () => {
    useStore.getState().setSelectedPort('/dev/cu.usbserial-1410')
    mockHardware.deviceFsLs.mockResolvedValue({
      success: true,
      entries: [{ name: 'main.py', path: '/main.py', isDirectory: false, size: 100 }],
    })
    mockHardware.deviceFsRm.mockResolvedValue({ success: true })
    await renderAndFlush(<DeviceFileTree />)
    const fileItem = screen.getByText('main.py')
    fireEvent.contextMenu(fileItem)
    await new Promise((r) => setTimeout(r, 10))
    fireEvent.click(screen.getByText('Delete'))
    await new Promise((r) => setTimeout(r, 10))
    expect(mockHardware.deviceFsRm).toHaveBeenCalledWith('/dev/cu.usbserial-1410', '/main.py')
  })

  it('calls deviceFsCat when Download is clicked', async () => {
    useStore.getState().setSelectedPort('/dev/cu.usbserial-1410')
    mockHardware.deviceFsLs.mockResolvedValue({
      success: true,
      entries: [{ name: 'main.py', path: '/main.py', isDirectory: false, size: 100 }],
    })
    mockHardware.deviceFsCat.mockResolvedValue({ success: true, content: 'print("hi")' })
    await renderAndFlush(<DeviceFileTree />)
    const fileItem = screen.getByText('main.py')
    fireEvent.contextMenu(fileItem)
    await new Promise((r) => setTimeout(r, 10))
    fireEvent.click(screen.getByText('Download'))
    await new Promise((r) => setTimeout(r, 10))
    expect(mockHardware.deviceFsCat).toHaveBeenCalledWith('/dev/cu.usbserial-1410', '/main.py')
  })

  it('logs error when deviceFsLs throws', async () => {
    useStore.getState().setSelectedPort('/dev/cu.usbserial-1410')
    mockHardware.deviceFsLs.mockRejectedValue(new Error('connection refused'))
    await renderAndFlush(<DeviceFileTree />)
    const logs = useStore.getState().logEntries
    expect(logs.some(l => l.type === 'error')).toBe(true)
  })

  it('sets deviceConnected to false on failure', async () => {
    useStore.getState().setSelectedPort('/dev/cu.usbserial-1410')
    useStore.getState().setDeviceConnected(true)
    mockHardware.deviceFsLs.mockResolvedValue({ success: false, entries: [], error: 'timeout' })
    await renderAndFlush(<DeviceFileTree />)
    expect(useStore.getState().deviceConnected).toBe(false)
  })

  it('logs warning when deviceFsLs returns failure', async () => {
    useStore.getState().setSelectedPort('/dev/cu.usbserial-1410')
    mockHardware.deviceFsLs.mockResolvedValue({ success: false, entries: [], error: 'cannot list' })
    await renderAndFlush(<DeviceFileTree />)
    const logs = useStore.getState().logEntries
    expect(logs.some(l => l.type === 'warn')).toBe(true)
  })

  it('renders directory entries with folder icon', async () => {
    useStore.getState().setSelectedPort('/dev/cu.usbserial-1410')
    mockHardware.deviceFsLs.mockResolvedValue({
      success: true,
      entries: [
        { name: 'lib', path: '/lib', isDirectory: true, size: 0 },
        { name: 'main.py', path: '/main.py', isDirectory: false, size: 100 },
      ],
    })
    await renderAndFlush(<DeviceFileTree />)
    expect(screen.getByText('lib')).toBeInTheDocument()
    expect(screen.getByText('main.py')).toBeInTheDocument()
  })

  it('toggles directory expansion on click', async () => {
    useStore.getState().setSelectedPort('/dev/cu.usbserial-1410')
    mockHardware.deviceFsLs.mockResolvedValue({
      success: true,
      entries: [
        { name: 'lib', path: '/lib', isDirectory: true, size: 0 },
        { name: 'main.py', path: '/main.py', isDirectory: false, size: 100 },
      ],
    })
    await renderAndFlush(<DeviceFileTree />)
    const dirItem = screen.getByText('lib')
    fireEvent.click(dirItem)
    await new Promise((r) => setTimeout(r, 10))
  })

  it('does not log duplicate errors', async () => {
    useStore.getState().setSelectedPort('/dev/cu.usbserial-1410')
    mockHardware.deviceFsLs.mockResolvedValue({ success: false, entries: [], error: 'could not enter raw repl' })
    await renderAndFlush(<DeviceFileTree />)
    // Click refresh to trigger a second attempt
    const refreshBtn = screen.getByTitle('Refresh device files')
    fireEvent.click(refreshBtn)
    await new Promise((r) => setTimeout(r, 50))
    const warnCount = useStore.getState().logEntries.filter(l => l.type === 'warn').length
    expect(warnCount).toBe(1)
  })

  it('shows disconnect REPL message when REPL is connected', () => {
    useStore.getState().setSelectedPort('/dev/cu.usbserial-1410')
    useStore.getState().setReplConnected(true)
    render(<DeviceFileTree />)
    expect(screen.getByText(/Disconnect REPL/i)).toBeInTheDocument()
  })
})
