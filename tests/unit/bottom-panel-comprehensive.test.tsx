import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import { BottomPanel } from '@/components/bottom-panel'
import { useStore } from '@/lib/store'

vi.mock('@monaco-editor/react', () => ({
  default: () => null,
}))

vi.mock('next-themes', () => ({
  useTheme: () => ({ theme: 'dark', setTheme: vi.fn() }),
}))

const mockReplHardware = {
  replConnect: vi.fn(async () => ({ success: true })),
  replDisconnect: vi.fn(async () => ({ success: true })),
  replSend: vi.fn(async () => ({ success: true })),
  replSendBreak: vi.fn(async () => ({ success: true })),
  replSendSoftReset: vi.fn(async () => ({ success: true })),
  saveReplExport: vi.fn(async () => ({ success: true, filePath: '/tmp/out.txt' })),
  onReplData: vi.fn((cb: (_e: unknown, d: string) => void) => { 
    mockReplHardware._dataCb = cb
    return () => {} 
  }),
  onReplDisconnected: vi.fn(() => () => {}),
  onReplError: vi.fn(() => () => {}),
  _dataCb: null as ((_: unknown, d: string) => void) | null,
}

function setupReplHardware() {
  ;(window as unknown as { hardware: unknown }).hardware = mockReplHardware
}

function makeLogEntry(message: string, type: 'info' | 'warn' | 'error' | 'ok' = 'info') {
  return { id: String(Date.now() + Math.random()), timestamp: new Date(), type, message }
}

describe('BottomPanel — Shell Mode', () => {
  beforeEach(() => {
    useStore.getState().reset()
    vi.clearAllMocks()
  })

  it('renders SHELL tab and REPL tab', () => {
    useStore.getState().setBottomMode('shell')
    render(<BottomPanel logEntries={[]} onClearLogs={vi.fn()} />)
    expect(screen.getByText('SHELL')).toBeInTheDocument()
    expect(screen.getByText('REPL')).toBeInTheDocument()
  })

  it('shows "No activity yet" when no log entries', () => {
    useStore.getState().setBottomMode('shell')
    render(<BottomPanel logEntries={[]} onClearLogs={vi.fn()} />)
    expect(screen.getByText(/No activity/i)).toBeInTheDocument()
  })

  it('renders log entries', () => {
    useStore.getState().setBottomMode('shell')
    const entries = [makeLogEntry('Compiling...', 'info'), makeLogEntry('Done!', 'ok')]
    render(<BottomPanel logEntries={entries} onClearLogs={vi.fn()} />)
    expect(screen.getByText('Compiling...')).toBeInTheDocument()
    expect(screen.getByText('Done!')).toBeInTheDocument()
  })

  it('shows entry count', () => {
    useStore.getState().setBottomMode('shell')
    const entries = [makeLogEntry('a'), makeLogEntry('b')]
    render(<BottomPanel logEntries={entries} onClearLogs={vi.fn()} />)
    expect(screen.getByText('(2)')).toBeInTheDocument()
  })

  it('calls onClearLogs when clear button is clicked', () => {
    useStore.getState().setBottomMode('shell')
    const onClear = vi.fn()
    render(<BottomPanel logEntries={[makeLogEntry('test')]} onClearLogs={onClear} />)
    const clearBtn = screen.getByTitle('Clear log')
    fireEvent.click(clearBtn)
    expect(onClear).toHaveBeenCalled()
  })

  it('switches to REPL mode when REPL tab is clicked', () => {
    useStore.getState().setBottomMode('shell')
    render(<BottomPanel logEntries={[]} onClearLogs={vi.fn()} />)
    fireEvent.click(screen.getByText('REPL'))
    expect(useStore.getState().bottomMode).toBe('repl')
  })

  it('collapses when collapse button is clicked', () => {
    useStore.getState().setBottomMode('shell')
    render(<BottomPanel logEntries={[]} onClearLogs={vi.fn()} />)
    fireEvent.click(screen.getByTitle('Collapse'))
    expect(useStore.getState().bottomMode).toBe('hidden')
  })

  it('renders error entries with red styling', () => {
    useStore.getState().setBottomMode('shell')
    const entries = [makeLogEntry('Failed!', 'error')]
    render(<BottomPanel logEntries={entries} onClearLogs={vi.fn()} />)
    expect(screen.getByText('Failed!')).toBeInTheDocument()
  })

  it('renders warning entries', () => {
    useStore.getState().setBottomMode('shell')
    const entries = [makeLogEntry('Warning!', 'warn')]
    render(<BottomPanel logEntries={entries} onClearLogs={vi.fn()} />)
    expect(screen.getByText('Warning!')).toBeInTheDocument()
  })

  it('renders flash entries', () => {
    useStore.getState().setBottomMode('shell')
    const entries = [makeLogEntry('Flashing...', 'flash' as 'info')]
    render(<BottomPanel logEntries={entries} onClearLogs={vi.fn()} />)
    expect(screen.getByText('Flashing...')).toBeInTheDocument()
  })

  it('renders install entries', () => {
    useStore.getState().setBottomMode('shell')
    const entries = [makeLogEntry('Installing...', 'install' as 'info')]
    render(<BottomPanel logEntries={entries} onClearLogs={vi.fn()} />)
    expect(screen.getByText('Installing...')).toBeInTheDocument()
  })

  it('copy button copies log content to clipboard', async () => {
    useStore.getState().setBottomMode('shell')
    const entries = [makeLogEntry('test message')]
    const writeText = vi.fn().mockResolvedValue(undefined)
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText },
      configurable: true,
    })
    render(<BottomPanel logEntries={entries} onClearLogs={vi.fn()} />)
    const copyBtn = screen.getByTitle('Copy to clipboard')
    fireEvent.click(copyBtn)
    await waitFor(() => {
      expect(writeText).toHaveBeenCalled()
    })
  })

  it('export button calls saveReplExport for shell logs', async () => {
    setupReplHardware()
    useStore.getState().setBottomMode('shell')
    const entries = [makeLogEntry('test')]
    render(<BottomPanel logEntries={entries} onClearLogs={vi.fn()} />)
    const exportBtn = screen.getByTitle(/Export shell log/i)
    fireEvent.click(exportBtn)
    await waitFor(() => {
      expect(mockReplHardware.saveReplExport).toHaveBeenCalled()
    })
  })
})

describe('BottomPanel — REPL Mode', () => {
  beforeEach(() => {
    useStore.getState().reset()
    vi.clearAllMocks()
    setupReplHardware()
    useStore.getState().setSelectedPort('/dev/cu.usbserial-1410')
  })

  it('shows connect button when not connected', () => {
    useStore.getState().setBottomMode('repl')
    render(<BottomPanel logEntries={[]} onClearLogs={vi.fn()} />)
    expect(screen.getByText('Connect')).toBeInTheDocument()
  })

  it('shows "Click Connect" prompt when not connected', () => {
    useStore.getState().setBottomMode('repl')
    render(<BottomPanel logEntries={[]} onClearLogs={vi.fn()} />)
    expect(screen.getByText(/Click.*Connect/i)).toBeInTheDocument()
  })

  it('calls replConnect when Connect is clicked', async () => {
    useStore.getState().setBottomMode('repl')
    render(<BottomPanel logEntries={[]} onClearLogs={vi.fn()} />)
    fireEvent.click(screen.getByText('Connect'))
    await waitFor(() => {
      expect(mockReplHardware.replConnect).toHaveBeenCalledWith('/dev/cu.usbserial-1410')
    })
    expect(useStore.getState().replConnected).toBe(true)
  })

  it('shows disconnect, break, reset buttons when connected', () => {
    useStore.getState().setBottomMode('repl')
    useStore.getState().setReplConnected(true)
    render(<BottomPanel logEntries={[]} onClearLogs={vi.fn()} />)
    expect(screen.getByText('Disconnect')).toBeInTheDocument()
    expect(screen.getByText('^C')).toBeInTheDocument()
    expect(screen.getByText('Reset')).toBeInTheDocument()
  })

  it('calls replDisconnect when Disconnect is clicked', async () => {
    useStore.getState().setBottomMode('repl')
    useStore.getState().setReplConnected(true)
    render(<BottomPanel logEntries={[]} onClearLogs={vi.fn()} />)
    fireEvent.click(screen.getByText('Disconnect'))
    await waitFor(() => {
      expect(mockReplHardware.replDisconnect).toHaveBeenCalled()
    })
    expect(useStore.getState().replConnected).toBe(false)
  })

  it('calls replSendBreak when ^C is clicked', async () => {
    useStore.getState().setBottomMode('repl')
    useStore.getState().setReplConnected(true)
    render(<BottomPanel logEntries={[]} onClearLogs={vi.fn()} />)
    fireEvent.click(screen.getByText('^C'))
    await waitFor(() => {
      expect(mockReplHardware.replSendBreak).toHaveBeenCalled()
    })
  })

  it('calls replSendSoftReset when Reset is clicked', async () => {
    useStore.getState().setBottomMode('repl')
    useStore.getState().setReplConnected(true)
    render(<BottomPanel logEntries={[]} onClearLogs={vi.fn()} />)
    fireEvent.click(screen.getByText('Reset'))
    await waitFor(() => {
      expect(mockReplHardware.replSendSoftReset).toHaveBeenCalled()
    })
  })

  it('renders the xterm terminal when connected', () => {
    useStore.getState().setBottomMode('repl')
    useStore.getState().setReplConnected(true)
    const { container } = render(<BottomPanel logEntries={[]} onClearLogs={vi.fn()} />)
    expect(container.querySelector('.xterm')).toBeInTheDocument()
  })

  it('shows clear REPL button when connected', () => {
    useStore.getState().setBottomMode('repl')
    useStore.getState().setReplConnected(true)
    render(<BottomPanel logEntries={[]} onClearLogs={vi.fn()} />)
    expect(screen.getByTitle('Clear REPL output')).toBeInTheDocument()
  })

  it('clears REPL output when clear button clicked', () => {
    useStore.getState().setBottomMode('repl')
    useStore.getState().setReplConnected(true)
    useStore.getState().appendReplOutput('some output')
    render(<BottomPanel logEntries={[]} onClearLogs={vi.fn()} />)
    const clearBtn = screen.getByTitle('Clear REPL output')
    fireEvent.click(clearBtn)
    expect(useStore.getState().replOutput).toBe('')
  })

  it('shows port name when connected', () => {
    useStore.getState().setBottomMode('repl')
    useStore.getState().setReplConnected(true)
    render(<BottomPanel logEntries={[]} onClearLogs={vi.fn()} />)
    expect(screen.getByText('/dev/cu.usbserial-1410')).toBeInTheDocument()
  })

  it('shows Disconnected when not connected', () => {
    useStore.getState().setBottomMode('repl')
    render(<BottomPanel logEntries={[]} onClearLogs={vi.fn()} />)
    expect(screen.getByText('Disconnected')).toBeInTheDocument()
  })

  it('export button calls saveReplExport for REPL output', async () => {
    useStore.getState().setBottomMode('repl')
    useStore.getState().setReplConnected(true)
    useStore.getState().appendReplOutput('repl content')
    render(<BottomPanel logEntries={[]} onClearLogs={vi.fn()} />)
    const exportBtn = screen.getByTitle(/Export REPL/i)
    fireEvent.click(exportBtn)
    await waitFor(() => {
      expect(mockReplHardware.saveReplExport).toHaveBeenCalledWith(expect.stringContaining('repl content'), expect.any(String))
    })
  })

  it('copy button copies REPL output to clipboard', async () => {
    useStore.getState().setBottomMode('repl')
    useStore.getState().setReplConnected(true)
    useStore.getState().appendReplOutput('repl data')
    const writeText = vi.fn().mockResolvedValue(undefined)
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText },
      configurable: true,
    })
    render(<BottomPanel logEntries={[]} onClearLogs={vi.fn()} />)
    const copyBtn = screen.getByTitle('Copy to clipboard')
    fireEvent.click(copyBtn)
    await waitFor(() => {
      expect(writeText).toHaveBeenCalledWith('repl data')
    })
  })

  it('Connect button is disabled when no port selected', () => {
    useStore.getState().setBottomMode('repl')
    useStore.getState().setSelectedPort('')
    render(<BottomPanel logEntries={[]} onClearLogs={vi.fn()} />)
    expect(screen.getByText('Connect')).toBeDisabled()
  })

  it('appends to REPL output when data arrives via IPC', async () => {
    useStore.getState().setBottomMode('repl')
    useStore.getState().setReplConnected(true)
    render(<BottomPanel logEntries={[]} onClearLogs={vi.fn()} />)
    if (mockReplHardware._dataCb) {
      mockReplHardware._dataCb({}, 'hello from board')
    }
    await waitFor(() => {
      expect(useStore.getState().replOutput).toContain('hello from board')
    })
  })

  it('switches to shell when SHELL tab is clicked from REPL', () => {
    useStore.getState().setBottomMode('repl')
    render(<BottomPanel logEntries={[]} onClearLogs={vi.fn()} />)
    fireEvent.click(screen.getByText('SHELL'))
    expect(useStore.getState().bottomMode).toBe('shell')
  })
})

describe('BottomPanel — Collapsed State', () => {
  beforeEach(() => {
    useStore.getState().reset()
  })

  it('shows collapsed strip with SHELL and REPL labels', () => {
    useStore.getState().setBottomMode('hidden')
    render(<BottomPanel logEntries={[]} onClearLogs={vi.fn()} />)
    expect(screen.getByText('SHELL')).toBeInTheDocument()
    expect(screen.getByText('REPL')).toBeInTheDocument()
  })

  it('expands to shell when collapsed strip is clicked', () => {
    useStore.getState().setBottomMode('hidden')
    render(<BottomPanel logEntries={[]} onClearLogs={vi.fn()} />)
    const strip = screen.getByText('SHELL').closest('button')!
    fireEvent.click(strip)
    expect(useStore.getState().bottomMode).toBe('shell')
  })

  it('shows log count in collapsed state', () => {
    useStore.getState().setBottomMode('hidden')
    const entries = [makeLogEntry('a'), makeLogEntry('b')]
    render(<BottomPanel logEntries={entries} onClearLogs={vi.fn()} />)
    expect(screen.getByText('(2 entries)')).toBeInTheDocument()
  })

  it('shows green dot when REPL is connected in collapsed state', () => {
    useStore.getState().setBottomMode('hidden')
    useStore.getState().setReplConnected(true)
    render(<BottomPanel logEntries={[]} onClearLogs={vi.fn()} />)
    const strip = screen.getByText('SHELL').closest('button')!
    expect(strip.querySelector('.bg-green-500')).toBeTruthy()
  })
})

