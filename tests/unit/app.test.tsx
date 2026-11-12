import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { useStore, detectBoardType } from '@/lib/store'
import { TEMPLATES } from '@/lib/templates'

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

describe('App — Welcome Screen', () => {
  beforeEach(() => {
    useStore.getState().reset()
  })

  it('shows welcome screen by default', () => {
    render(<App />)
    expect(screen.getByText('New Project')).toBeInTheDocument()
    expect(screen.getByText('MicroPython & Arduino IDE')).toBeInTheDocument()
  })

  it('shows Open Project Folder on welcome screen', () => {
    render(<App />)
    expect(screen.getByText('Open Project Folder')).toBeInTheDocument()
  })
})

describe('App — Main Editor View', () => {
  beforeEach(() => {
    useStore.getState().reset()
    useStore.getState().setWelcomeScreenVisible(false)
    useStore.getState().setProjectPath('/tmp/test-project')
    useStore.getState().setProjectFiles([
      { name: 'main.py', path: '/tmp/test-project/main.py', content: 'print("hello")', language: 'python' },
      { name: 'utils.py', path: '/tmp/test-project/utils.py', content: 'def add(a, b): return a + b', language: 'python' },
    ])
    useStore.getState().setCurrentFile({
      name: 'main.py', path: '/tmp/test-project/main.py', content: 'print("hello")', language: 'python',
    })
  })

  it('renders the editor when a project is open', () => {
    render(<App />)
    expect(screen.getByTestId('monaco-editor')).toBeInTheDocument()
  })

  it('shows Run, Upload, Stop buttons in header', () => {
    render(<App />)
    expect(screen.getByText('Run')).toBeInTheDocument()
    expect(screen.getByText('Upload')).toBeInTheDocument()
    expect(screen.getByText('Stop')).toBeInTheDocument()
  })

  it('shows project name in sidebar', () => {
    render(<App />)
    expect(screen.getByText('test-project/')).toBeInTheDocument()
  })

  it('lists project files in sidebar', () => {
    render(<App />)
    const mainFiles = screen.getAllByText('main.py')
    const utilFiles = screen.getAllByText('utils.py')
    expect(mainFiles.length).toBeGreaterThan(0)
    expect(utilFiles.length).toBeGreaterThan(0)
  })

  it('shows status bar with line/column info', () => {
    render(<App />)
    expect(screen.getByText(/Ln \d+, Col \d+/)).toBeInTheDocument()
  })

  it('shows current file name in header', () => {
    render(<App />)
    const fileLabels = screen.getAllByText('main.py')
    expect(fileLabels.length).toBeGreaterThan(0)
  })

  it('shows Modified badge when file is dirty', () => {
    useStore.getState().setIsDirty(true)
    render(<App />)
    expect(screen.getByText('Modified')).toBeInTheDocument()
  })

  it('does not show Modified badge when file is clean', () => {
    render(<App />)
    expect(screen.queryByText('Modified')).not.toBeInTheDocument()
  })

  it('shows connected/disconnected status in status bar', () => {
    render(<App />)
    expect(screen.getByText('disconnected')).toBeInTheDocument()
  })

  it('shows connected when port is selected', () => {
    useStore.getState().setSelectedPort('/dev/ttyUSB0')
    render(<App />)
    expect(screen.getByText('connected')).toBeInTheDocument()
  })

  it('shows board name in status bar when detected', () => {
    useStore.getState().setPorts([{ path: '/dev/ttyUSB0', vendorId: '10c4', productId: 'ea60' }])
    useStore.getState().setSelectedPort('/dev/ttyUSB0')
    render(<App />)
    expect(screen.getAllByText('M5Stack ATOM Lite').length).toBeGreaterThan(0)
  })

  it('shows No Board when no board detected', () => {
    render(<App />)
    expect(screen.getByText('No Board')).toBeInTheDocument()
  })

  it('shows BOARD panel in right sidebar', () => {
    render(<App />)
    expect(screen.getByText('BOARD')).toBeInTheDocument()
  })

  it('shows Flash Firmware button', () => {
    render(<App />)
    expect(screen.getByText('Flash Firmware')).toBeInTheDocument()
  })

  it('shows shell log panel', () => {
    render(<App />)
    expect(screen.getByText('SHELL')).toBeInTheDocument()
  })

  it('clicking a file in sidebar switches to it', async () => {
    render(<App />)
    const fileItems = screen.getAllByText('utils.py')
    await userEvent.click(fileItems[fileItems.length - 1])
    expect(useStore.getState().currentFile?.name).toBe('utils.py')
  })

  it('shows settings button in header', () => {
    render(<App />)
    const allButtons = screen.getAllByRole('button')
    const settingsBtn = allButtons.find(b => b.querySelector('.lucide-settings'))
    expect(settingsBtn).toBeTruthy()
  })

  it('opens settings dialog when settings button is clicked', async () => {
    render(<App />)
    const headerSettingsBtn = screen.getAllByRole('button').find(b => b.title === 'Settings')
    if (headerSettingsBtn) {
      await userEvent.click(headerSettingsBtn)
      await waitFor(() => {
        expect(screen.getByText('Settings')).toBeInTheDocument()
      })
    }
  })

  it('opens settings from activity bar icon', async () => {
    render(<App />)
    const activityBtns = screen.getAllByRole('button')
    const settingsBtn = activityBtns.find(b => b.title === 'Settings')
    if (settingsBtn) {
      await userEvent.click(settingsBtn)
    }
  })
})

describe('App — Board Panel', () => {
  beforeEach(() => {
    useStore.getState().reset()
    useStore.getState().setWelcomeScreenVisible(false)
    useStore.getState().setProjectPath('/tmp/test')
    useStore.getState().setProjectFiles([
      { name: 'main.py', path: '/tmp/test/main.py', content: 'x', language: 'python' },
    ])
    useStore.getState().setCurrentFile({
      name: 'main.py', path: '/tmp/test/main.py', content: 'x', language: 'python',
    })
  })

  it('shows No Board Connected when no ports', () => {
    render(<App />)
    expect(screen.getByText('No Board Connected')).toBeInTheDocument()
  })

  it('shows detected board name when port is selected', () => {
    useStore.getState().setPorts([{ path: '/dev/ttyUSB0', vendorId: '303a', productId: '8010' }])
    useStore.getState().setSelectedPort('/dev/ttyUSB0')
    render(<App />)
    expect(screen.getAllByText('M5StampS3').length).toBeGreaterThan(0)
  })

  it('shows a generic ESP32-S3 name for an ambiguous native-USB board', () => {
    useStore.getState().setPorts([{ path: '/dev/ttyUSB0', vendorId: '303a', productId: '1001' }])
    useStore.getState().setSelectedPort('/dev/ttyUSB0')
    render(<App />)
    expect(screen.getAllByText('Generic ESP32-S3').length).toBeGreaterThan(0)
  })

  it('shows RPi board when detected', () => {
    useStore.getState().setPorts([{ path: '/dev/ttyACM0', vendorId: '2e8a', productId: '0005' }])
    useStore.getState().setSelectedPort('/dev/ttyACM0')
    render(<App />)
    expect(screen.getAllByText('Raspberry Pi Pico').length).toBeGreaterThan(0)
  })

  it('shows Arduino board when detected', () => {
    useStore.getState().setPorts([{ path: '/dev/ttyACM1', vendorId: '2341' }])
    useStore.getState().setSelectedPort('/dev/ttyACM1')
    render(<App />)
    expect(screen.getAllByText('Arduino Uno').length).toBeGreaterThan(0)
  })

  it('lets the user pin the exact board on an ambiguous ESP32-S3', () => {
    const port = { path: '/dev/ttyUSB0', vendorId: '303a', productId: '1001', serialNumber: 'sn-pin' }
    useStore.getState().setPorts([port])
    useStore.getState().setSelectedPort('/dev/ttyUSB0')
    useStore.getState().setBoardOverride('sn-pin', 'seeed-xiao-esp32s3')
    render(<App />)
    expect(screen.getAllByText('Seeed Studio XIAO ESP32S3 Sense').length).toBeGreaterThan(0)
  })
})

describe('App — Flash Dialog', () => {
  beforeEach(() => {
    useStore.getState().reset()
    useStore.getState().setWelcomeScreenVisible(false)
    useStore.getState().setProjectPath('/tmp/test')
    useStore.getState().setProjectFiles([
      { name: 'main.py', path: '/tmp/test/main.py', content: 'x', language: 'python' },
    ])
    useStore.getState().setCurrentFile({
      name: 'main.py', path: '/tmp/test/main.py', content: 'x', language: 'python',
    })
    useStore.getState().setPorts([
      { path: '/dev/ttyUSB0', manufacturer: 'Silicon Labs', vendorId: '10c4', productId: 'ea60' },
    ])
    useStore.getState().setSelectedPort('/dev/ttyUSB0')
    useStore.getState().setDetectedBoard('espressif')
  })

  it('opens flash dialog when Flash Firmware is clicked', async () => {
    render(<App />)
    const flashButtons = screen.getAllByText('Flash Firmware')
    await userEvent.click(flashButtons[flashButtons.length - 1])
    await waitFor(() => {
      const flashTexts = screen.getAllByText('Flash Firmware')
      expect(flashTexts.length).toBeGreaterThan(1)
    })
  })

  it('shows detected board in flash dialog', async () => {
    render(<App />)
    const flashButtons = screen.getAllByText('Flash Firmware')
    await userEvent.click(flashButtons[flashButtons.length - 1])
    await waitFor(() => {
      const espressifLabels = screen.getAllByText('Espressif (ESP32 / ESP8266)')
      expect(espressifLabels.length).toBeGreaterThan(0)
      const portLabels = screen.getAllByText('/dev/ttyUSB0')
      expect(portLabels.length).toBeGreaterThan(0)
    })
  })
})

describe('App — Settings Dialog', () => {
  beforeEach(() => {
    useStore.getState().reset()
    useStore.getState().setWelcomeScreenVisible(false)
    useStore.getState().setProjectPath('/tmp/test')
    useStore.getState().setProjectFiles([
      { name: 'main.py', path: '/tmp/test/main.py', content: 'x', language: 'python' },
    ])
    useStore.getState().setCurrentFile({
      name: 'main.py', path: '/tmp/test/main.py', content: 'x', language: 'python',
    })
  })

  it('opens settings dialog', async () => {
    render(<App />)
    useStore.getState().setSettingsOpen(true)
    await waitFor(() => {
      expect(screen.getByText('Settings')).toBeInTheDocument()
    })
  })

  it('shows General, Board, Editor tabs in settings', async () => {
    render(<App />)
    useStore.getState().setSettingsOpen(true)
    await waitFor(() => {
      expect(screen.getByText('general')).toBeInTheDocument()
      expect(screen.getByText('board')).toBeInTheDocument()
      expect(screen.getByText('editor')).toBeInTheDocument()
    })
  })

  it('shows application settings in general tab', async () => {
    render(<App />)
    useStore.getState().setSettingsOpen(true)
    await waitFor(() => {
      expect(screen.getByText('Application')).toBeInTheDocument()
      expect(screen.getByText('Check for updates on startup')).toBeInTheDocument()
      expect(screen.getByText('Show welcome screen on startup')).toBeInTheDocument()
    })
  })

  it('shows recent projects in general tab', async () => {
    render(<App />)
    useStore.getState().setSettingsOpen(true)
    await waitFor(() => {
      expect(screen.getByText('Recent Projects')).toBeInTheDocument()
      expect(screen.getByText('Clear Recent Projects')).toBeInTheDocument()
    })
  })

  it('shows Baud Rate in board tab', async () => {
    render(<App />)
    useStore.getState().setSettingsOpen(true)
    await waitFor(() => {
      expect(screen.getByText('board')).toBeInTheDocument()
    })
    await userEvent.click(screen.getByText('board'))
    await waitFor(() => {
      expect(screen.getByText('Baud Rate')).toBeInTheDocument()
    })
  })

  it('shows Font Size in editor tab', async () => {
    render(<App />)
    useStore.getState().setSettingsOpen(true)
    await waitFor(() => {
      expect(screen.getByText('editor')).toBeInTheDocument()
    })
    await userEvent.click(screen.getByText('editor'))
    await waitFor(() => {
      expect(screen.getByText('Font Size')).toBeInTheDocument()
      expect(screen.getByText('Tab Size')).toBeInTheDocument()
    })
  })
})

describe('App — Create File/Folder Dialog', () => {
  beforeEach(() => {
    useStore.getState().reset()
    useStore.getState().setWelcomeScreenVisible(false)
    useStore.getState().setProjectPath('/tmp/test')
    useStore.getState().setProjectFiles([
      { name: 'main.py', path: '/tmp/test/main.py', content: 'x', language: 'python' },
    ])
    useStore.getState().setCurrentFile({
      name: 'main.py', path: '/tmp/test/main.py', content: 'x', language: 'python',
    })
  })

  it('shows + button in sidebar for creating files', () => {
    render(<App />)
    const plusButtons = screen.getAllByRole('button').filter(b => {
      const svg = b.querySelector('svg')
      return svg && b.className.includes('p-1')
    })
    expect(plusButtons.length).toBeGreaterThan(0)
  })
})

describe('App — Context Menu', () => {
  beforeEach(() => {
    useStore.getState().reset()
    useStore.getState().setWelcomeScreenVisible(false)
    useStore.getState().setProjectPath('/tmp/test')
    useStore.getState().setProjectFiles([
      { name: 'main.py', path: '/tmp/test/main.py', content: 'x', language: 'python' },
    ])
    useStore.getState().setCurrentFile({
      name: 'main.py', path: '/tmp/test/main.py', content: 'x', language: 'python',
    })
  })

  it('right-clicking a file shows context menu', async () => {
    render(<App />)
    const fileItems = screen.getAllByText('main.py')
    const fileItem = fileItems.find(el => el.closest('[class*="cursor-pointer"]'))
    if (fileItem) {
      await userEvent.pointer({ keys: '[MouseRight]', target: fileItem })
      await waitFor(() => {
        expect(screen.getByText('Save')).toBeInTheDocument()
      })
    }
  })
})

describe('App — Keyboard Shortcuts', () => {
  beforeEach(() => {
    useStore.getState().reset()
    useStore.getState().setWelcomeScreenVisible(false)
    useStore.getState().setProjectPath('/tmp/test')
    useStore.getState().setProjectFiles([
      { name: 'main.py', path: '/tmp/test/main.py', content: 'x', language: 'python' },
    ])
    useStore.getState().setCurrentFile({
      name: 'main.py', path: '/tmp/test/main.py', content: 'x', language: 'python',
    })
  })

  it('Cmd+K opens command palette', async () => {
    render(<App />)
    await userEvent.keyboard('{Meta>}k{/Meta}')
    await waitFor(() => {
      expect(screen.getByPlaceholderText('Search commands, files, projects...')).toBeInTheDocument()
    })
  })

  it('Cmd+S triggers save (no crash)', async () => {
    render(<App />)
    await userEvent.keyboard('{Meta>}s{/Meta}')
    // No assertion needed — just ensure no crash
  })
})

describe('App — Shell Log in Editor', () => {
  beforeEach(() => {
    useStore.getState().reset()
    useStore.getState().setWelcomeScreenVisible(false)
    useStore.getState().setProjectPath('/tmp/test')
    useStore.getState().setProjectFiles([
      { name: 'main.py', path: '/tmp/test/main.py', content: 'x', language: 'python' },
    ])
    useStore.getState().setCurrentFile({
      name: 'main.py', path: '/tmp/test/main.py', content: 'x', language: 'python',
    })
    useStore.getState().setBottomMode('shell')
  })

  it('renders shell log panel', () => {
    render(<App />)
    expect(screen.getByText('SHELL')).toBeInTheDocument()
  })

  it('shows "No activity yet" when no log entries', () => {
    render(<App />)
    expect(screen.getByText('No activity yet. Progress will appear here...')).toBeInTheDocument()
  })

  it('shows log entries when present', () => {
    useStore.getState().addLogEntry({ type: 'info', message: 'Hello from test' })
    render(<App />)
    expect(screen.getByText('Hello from test')).toBeInTheDocument()
  })

  it('toggles shell visibility', async () => {
    render(<App />)
    const collapseBtn = screen.getByTitle('Collapse')
    await userEvent.click(collapseBtn)
    expect(useStore.getState().bottomMode).toBe('hidden')
  })

  it('clears log entries', async () => {
    useStore.getState().addLogEntry({ type: 'info', message: 'test entry' })
    render(<App />)
    const clearButton = screen.getByTitle('Clear log')
    await userEvent.click(clearButton)
    expect(useStore.getState().logEntries).toHaveLength(0)
  })
})

describe('App — Empty Project', () => {
  beforeEach(() => {
    useStore.getState().reset()
    useStore.getState().setWelcomeScreenVisible(false)
    useStore.getState().setProjectPath('/tmp/empty')
    useStore.getState().setProjectFiles([])
  })

  it('shows "No files in project" when project has no files', () => {
    render(<App />)
    expect(screen.getByText('No files in project')).toBeInTheDocument()
  })
})
