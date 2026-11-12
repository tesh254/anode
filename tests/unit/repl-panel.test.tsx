import { describe, it, expect, beforeEach } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { ReplPanel } from '@/components/repl-panel'
import { useStore } from '@/lib/store'

describe('ReplPanel', () => {
  beforeEach(() => {
    useStore.getState().reset()
    useStore.getState().setReplVisible(true)
  })

  it('renders nothing when replVisible is false', () => {
    useStore.getState().setReplVisible(false)
    const { container } = render(<ReplPanel />)
    expect(container.innerHTML).toBe('')
  })

  it('renders REPL header when visible', () => {
    render(<ReplPanel />)
    expect(screen.getByText('REPL')).toBeInTheDocument()
  })

  it('shows Connect button when disconnected', () => {
    render(<ReplPanel />)
    expect(screen.getByText('Connect')).toBeInTheDocument()
  })

  it('disables Connect when no port selected', () => {
    render(<ReplPanel />)
    expect(screen.getByText('Connect')).toBeDisabled()
  })

  it('enables Connect when port is selected', () => {
    useStore.getState().setSelectedPort('/dev/ttyUSB0')
    render(<ReplPanel />)
    expect(screen.getByText('Connect')).not.toBeDisabled()
  })

  it('shows Disconnect, ^C, Reset buttons when connected', () => {
    useStore.getState().setSelectedPort('/dev/ttyUSB0')
    useStore.getState().setReplConnected(true)
    render(<ReplPanel />)
    expect(screen.getByText('Disconnect')).toBeInTheDocument()
    expect(screen.getByText('^C')).toBeInTheDocument()
    expect(screen.getByText('Reset')).toBeInTheDocument()
  })

  it('shows port name in header when connected', () => {
    useStore.getState().setSelectedPort('/dev/ttyUSB0')
    useStore.getState().setReplConnected(true)
    render(<ReplPanel />)
    expect(screen.getByText('/dev/ttyUSB0')).toBeInTheDocument()
  })

  it('shows Disconnected when not connected', () => {
    render(<ReplPanel />)
    expect(screen.getByText('Disconnected')).toBeInTheDocument()
  })

  it('shows placeholder text when no output', () => {
    render(<ReplPanel />)
    expect(screen.getByText('Click "Connect" to start the REPL...')).toBeInTheDocument()
  })

  it('renders REPL output from store', () => {
    useStore.getState().appendReplOutput('>>> print("hello")\r\nhello\r\n')
    render(<ReplPanel />)
    expect(screen.getByText(/print\("hello"\)/)).toBeInTheDocument()
  })

  it('shows input form when connected', () => {
    useStore.getState().setSelectedPort('/dev/ttyUSB0')
    useStore.getState().setReplConnected(true)
    render(<ReplPanel />)
    expect(screen.getByPlaceholderText('Type MicroPython command...')).toBeInTheDocument()
  })

  it('does not show input form when disconnected', () => {
    render(<ReplPanel />)
    expect(screen.queryByPlaceholderText('Type MicroPython command...')).not.toBeInTheDocument()
  })

  it('clears REPL output', async () => {
    useStore.getState().appendReplOutput('some output')
    render(<ReplPanel />)
    expect(screen.getByText('some output')).toBeInTheDocument()
    useStore.getState().clearReplOutput()
    await screen.findByText('Click "Connect" to start the REPL...')
  })

  it('hides panel when Hide button clicked', () => {
    render(<ReplPanel />)
    fireEvent.click(screen.getByText('Hide'))
    expect(useStore.getState().replVisible).toBe(false)
  })

  it('sends input on form submit', async () => {
    const user = userEvent.setup()
    const mockSend = vi.fn().mockResolvedValue({ success: true })
    const mockConnect = vi.fn().mockResolvedValue({ success: true })
    window.hardware = {
      ...window.hardware,
      replConnect: mockConnect,
      replSend: mockSend,
      replDisconnect: vi.fn().mockResolvedValue({ success: true }),
      replSendBreak: vi.fn().mockResolvedValue({ success: true }),
      replSendSoftReset: vi.fn().mockResolvedValue({ success: true }),
    }

    useStore.getState().setSelectedPort('/dev/ttyUSB0')
    useStore.getState().setReplConnected(true)
    render(<ReplPanel />)

    const input = screen.getByPlaceholderText('Type MicroPython command...')
    await user.type(input, 'print("hi")')
    await user.keyboard('{Enter}')

    expect(mockSend).toHaveBeenCalledWith('print("hi")\r\n')
  })

  it('navigates history with arrow keys', async () => {
    const user = userEvent.setup()
    window.hardware = {
      ...window.hardware,
      replSend: vi.fn().mockResolvedValue({ success: true }),
      replDisconnect: vi.fn().mockResolvedValue({ success: true }),
      replSendBreak: vi.fn().mockResolvedValue({ success: true }),
      replSendSoftReset: vi.fn().mockResolvedValue({ success: true }),
    }

    useStore.getState().setSelectedPort('/dev/ttyUSB0')
    useStore.getState().setReplConnected(true)
    render(<ReplPanel />)

    const input = screen.getByPlaceholderText('Type MicroPython command...')

    await user.type(input, 'first')
    await user.keyboard('{Enter}')
    await user.type(input, 'second')
    await user.keyboard('{Enter}')

    fireEvent.keyDown(input, { key: 'ArrowUp' })
    expect(input).toHaveValue('second')

    fireEvent.keyDown(input, { key: 'ArrowUp' })
    expect(input).toHaveValue('first')

    fireEvent.keyDown(input, { key: 'ArrowDown' })
    expect(input).toHaveValue('second')

    fireEvent.keyDown(input, { key: 'ArrowDown' })
    expect(input).toHaveValue('')
  })

  it('connects to REPL on Connect click', async () => {
    const user = userEvent.setup()
    const mockConnect = vi.fn().mockResolvedValue({ success: true })
    window.hardware = {
      ...window.hardware,
      replConnect: mockConnect,
      replSend: vi.fn().mockResolvedValue({ success: true }),
      replDisconnect: vi.fn().mockResolvedValue({ success: true }),
      replSendBreak: vi.fn().mockResolvedValue({ success: true }),
      replSendSoftReset: vi.fn().mockResolvedValue({ success: true }),
    }

    useStore.getState().setSelectedPort('/dev/ttyUSB0')
    render(<ReplPanel />)

    await user.click(screen.getByText('Connect'))
    expect(mockConnect).toHaveBeenCalledWith('/dev/ttyUSB0')
  })

  it('shows error when Connect fails', async () => {
    const user = userEvent.setup()
    const mockConnect = vi.fn().mockResolvedValue({ success: false, error: 'Port busy' })
    window.hardware = {
      ...window.hardware,
      replConnect: mockConnect,
      replSend: vi.fn().mockResolvedValue({ success: true }),
      replDisconnect: vi.fn().mockResolvedValue({ success: true }),
      replSendBreak: vi.fn().mockResolvedValue({ success: true }),
      replSendSoftReset: vi.fn().mockResolvedValue({ success: true }),
    }

    useStore.getState().setSelectedPort('/dev/ttyUSB0')
    render(<ReplPanel />)

    await user.click(screen.getByText('Connect'))
    expect(useStore.getState().replOutput).toContain('Port busy')
  })

  it('disconnects on Disconnect click', async () => {
    const user = userEvent.setup()
    const mockDisconnect = vi.fn().mockResolvedValue({ success: true })
    window.hardware = {
      ...window.hardware,
      replConnect: vi.fn().mockResolvedValue({ success: true }),
      replSend: vi.fn().mockResolvedValue({ success: true }),
      replDisconnect: mockDisconnect,
      replSendBreak: vi.fn().mockResolvedValue({ success: true }),
      replSendSoftReset: vi.fn().mockResolvedValue({ success: true }),
    }

    useStore.getState().setSelectedPort('/dev/ttyUSB0')
    useStore.getState().setReplConnected(true)
    render(<ReplPanel />)

    await user.click(screen.getByText('Disconnect'))
    expect(mockDisconnect).toHaveBeenCalled()
    expect(useStore.getState().replConnected).toBe(false)
  })

  it('sends Ctrl+C on ^C click', async () => {
    const user = userEvent.setup()
    const mockBreak = vi.fn().mockResolvedValue({ success: true })
    window.hardware = {
      ...window.hardware,
      replConnect: vi.fn().mockResolvedValue({ success: true }),
      replSend: vi.fn().mockResolvedValue({ success: true }),
      replDisconnect: vi.fn().mockResolvedValue({ success: true }),
      replSendBreak: mockBreak,
      replSendSoftReset: vi.fn().mockResolvedValue({ success: true }),
    }

    useStore.getState().setSelectedPort('/dev/ttyUSB0')
    useStore.getState().setReplConnected(true)
    render(<ReplPanel />)

    await user.click(screen.getByText('^C'))
    expect(mockBreak).toHaveBeenCalled()
    expect(useStore.getState().replOutput).toContain('^C')
  })

  it('sends soft reset on Reset click', async () => {
    const user = userEvent.setup()
    const mockReset = vi.fn().mockResolvedValue({ success: true })
    window.hardware = {
      ...window.hardware,
      replConnect: vi.fn().mockResolvedValue({ success: true }),
      replSend: vi.fn().mockResolvedValue({ success: true }),
      replDisconnect: vi.fn().mockResolvedValue({ success: true }),
      replSendBreak: vi.fn().mockResolvedValue({ success: true }),
      replSendSoftReset: mockReset,
    }

    useStore.getState().setSelectedPort('/dev/ttyUSB0')
    useStore.getState().setReplConnected(true)
    render(<ReplPanel />)

    await user.click(screen.getByText('Reset'))
    expect(mockReset).toHaveBeenCalled()
    expect(useStore.getState().replOutput).toContain('Soft Reset')
  })

  it('shows error when send fails', async () => {
    const user = userEvent.setup()
    const mockSend = vi.fn().mockResolvedValue({ success: false, error: 'Not connected' })
    window.hardware = {
      ...window.hardware,
      replConnect: vi.fn().mockResolvedValue({ success: true }),
      replSend: mockSend,
      replDisconnect: vi.fn().mockResolvedValue({ success: true }),
      replSendBreak: vi.fn().mockResolvedValue({ success: true }),
      replSendSoftReset: vi.fn().mockResolvedValue({ success: true }),
    }

    useStore.getState().setSelectedPort('/dev/ttyUSB0')
    useStore.getState().setReplConnected(true)
    render(<ReplPanel />)

    const input = screen.getByPlaceholderText('Type MicroPython command...')
    await user.type(input, 'test')
    await user.keyboard('{Enter}')

    expect(useStore.getState().replOutput).toContain('Not connected')
  })

  it('subscribes to REPL data events when hardware supports it', async () => {
    const mockOnReplData = vi.fn(() => () => {})
    const mockOnReplDisconnected = vi.fn(() => () => {})
    const mockOnReplError = vi.fn(() => () => {})
    window.hardware = {
      ...window.hardware,
      onReplData: mockOnReplData,
      onReplDisconnected: mockOnReplDisconnected,
      onReplError: mockOnReplError,
      replConnect: vi.fn().mockResolvedValue({ success: true }),
      replSend: vi.fn().mockResolvedValue({ success: true }),
      replDisconnect: vi.fn().mockResolvedValue({ success: true }),
      replSendBreak: vi.fn().mockResolvedValue({ success: true }),
      replSendSoftReset: vi.fn().mockResolvedValue({ success: true }),
    }

    useStore.getState().setReplVisible(true)
    render(<ReplPanel />)

    expect(mockOnReplData).toHaveBeenCalled()
    expect(mockOnReplDisconnected).toHaveBeenCalled()
    expect(mockOnReplError).toHaveBeenCalled()
  })

  it('does not submit empty input when not connected', async () => {
    const user = userEvent.setup()
    const mockSend = vi.fn().mockResolvedValue({ success: true })
    window.hardware = {
      ...window.hardware,
      replSend: mockSend,
      replConnect: vi.fn().mockResolvedValue({ success: true }),
      replDisconnect: vi.fn().mockResolvedValue({ success: true }),
      replSendBreak: vi.fn().mockResolvedValue({ success: true }),
      replSendSoftReset: vi.fn().mockResolvedValue({ success: true }),
    }

    useStore.getState().setSelectedPort('/dev/ttyUSB0')
    useStore.getState().setReplConnected(true)
    render(<ReplPanel />)

    const input = screen.getByPlaceholderText('Type MicroPython command...')
    await user.keyboard('{Enter}')

    expect(mockSend).not.toHaveBeenCalled()
  })
})

describe('REPL store state', () => {
  beforeEach(() => {
    useStore.getState().reset()
  })

  it('initial state is disconnected and hidden', () => {
    const state = useStore.getState()
    expect(state.replConnected).toBe(false)
    expect(state.replVisible).toBe(false)
    expect(state.replOutput).toBe('')
  })

  it('setReplConnected sets connection state', () => {
    useStore.getState().setReplConnected(true)
    expect(useStore.getState().replConnected).toBe(true)
    useStore.getState().setReplConnected(false)
    expect(useStore.getState().replConnected).toBe(false)
  })

  it('appendReplOutput appends data', () => {
    useStore.getState().appendReplOutput('hello ')
    useStore.getState().appendReplOutput('world')
    expect(useStore.getState().replOutput).toBe('hello world')
  })

  it('clearReplOutput clears all output', () => {
    useStore.getState().appendReplOutput('some output')
    useStore.getState().clearReplOutput()
    expect(useStore.getState().replOutput).toBe('')
  })

  it('setReplVisible toggles visibility', () => {
    useStore.getState().setReplVisible(true)
    expect(useStore.getState().replVisible).toBe(true)
    useStore.getState().setReplVisible(false)
    expect(useStore.getState().replVisible).toBe(false)
  })

  it('reset clears REPL state', () => {
    useStore.getState().setReplConnected(true)
    useStore.getState().appendReplOutput('output')
    useStore.getState().setReplVisible(true)
    useStore.getState().reset()
    expect(useStore.getState().replConnected).toBe(false)
    expect(useStore.getState().replOutput).toBe('')
    expect(useStore.getState().replVisible).toBe(false)
  })
})
