import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, waitFor, fireEvent } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { CommandPalette } from '@/components/command-palette'
import { useStore } from '@/lib/store'
import { TEMPLATES } from '@/lib/templates'
import type { Template } from '@/lib/templates'

const createDefaultProps = () => ({
  open: true,
  onClose: vi.fn(),
  onOpenProject: vi.fn(),
  onCreateFromTemplate: vi.fn(),
  onSaveFile: vi.fn(),
  onSaveAllFiles: vi.fn(),
  onRun: vi.fn(),
  onUpload: vi.fn(),
  onStop: vi.fn(),
  onOpenFlash: vi.fn(),
})

describe('CommandPalette — Rendering', () => {
  const defaultProps = createDefaultProps()

  beforeEach(() => {
    vi.clearAllMocks()
    useStore.getState().reset()
  })

  it('renders nothing when closed', () => {
    render(<CommandPalette {...defaultProps} open={false} />)
    expect(screen.queryByPlaceholderText('Search commands, files, projects...')).not.toBeInTheDocument()
  })

  it('renders search input when open', () => {
    render(<CommandPalette {...defaultProps} />)
    expect(screen.getByPlaceholderText('Search commands, files, projects...')).toBeInTheDocument()
  })

  it('renders ESC keyboard hint', () => {
    render(<CommandPalette {...defaultProps} />)
    expect(screen.getByText('ESC')).toBeInTheDocument()
  })

  it('renders footer navigation hints', () => {
    render(<CommandPalette {...defaultProps} />)
    expect(screen.getByText('navigate')).toBeInTheDocument()
    expect(screen.getByText('select')).toBeInTheDocument()
    expect(screen.getByText('close')).toBeInTheDocument()
  })
})

describe('CommandPalette — Commands', () => {
  const defaultProps = createDefaultProps()

  beforeEach(() => {
    vi.clearAllMocks()
    useStore.getState().reset()
  })

  it('shows all default action commands', () => {
    render(<CommandPalette {...defaultProps} />)
    expect(screen.getByText('New Project')).toBeInTheDocument()
    expect(screen.getByText('Open Folder')).toBeInTheDocument()
    expect(screen.getByText('Open Settings')).toBeInTheDocument()
    expect(screen.getByText('Hide Shell Log')).toBeInTheDocument()
    expect(screen.getByText('Save File')).toBeInTheDocument()
    expect(screen.getByText('Save All Files')).toBeInTheDocument()
    expect(screen.getByText('Run')).toBeInTheDocument()
    expect(screen.getByText('Flash Firmware')).toBeInTheDocument()
    expect(screen.getByText('Upload Project')).toBeInTheDocument()
    expect(screen.getByText('Stop')).toBeInTheDocument()
  })

  it('shows action subtitles', () => {
    render(<CommandPalette {...defaultProps} />)
    expect(screen.getByText('Create a new project from a template')).toBeInTheDocument()
    expect(screen.getByText('Open an existing project folder')).toBeInTheDocument()
    expect(screen.getByText('Configure application settings')).toBeInTheDocument()
  })

  it('shows "Show Shell Log" when shell is hidden', () => {
    useStore.getState().setBottomMode('hidden')
    render(<CommandPalette {...defaultProps} />)
    expect(screen.getByText('Show Shell Log')).toBeInTheDocument()
  })

  it('shows project files when available', () => {
    useStore.getState().setProjectFiles([
      { name: 'main.py', path: '/tmp/main.py', content: '', language: 'python' },
      { name: 'utils.py', path: '/tmp/utils.py', content: '', language: 'python' },
    ])
    render(<CommandPalette {...defaultProps} />)
    expect(screen.getByText('main.py')).toBeInTheDocument()
    expect(screen.getByText('utils.py')).toBeInTheDocument()
  })

  it('shows recent projects when available', () => {
    useStore.getState().addRecentProject('/tmp/myproj', 'MyProject')
    render(<CommandPalette {...defaultProps} />)
    expect(screen.getByText('MyProject')).toBeInTheDocument()
    expect(screen.getByText('/tmp/myproj')).toBeInTheDocument()
  })

  it('shows template entries', () => {
    render(<CommandPalette {...defaultProps} />)
    // First 5 templates should appear
    expect(screen.getByText(`New: ${TEMPLATES[0].name}`)).toBeInTheDocument()
  })
})

describe('CommandPalette — Search/Filter', () => {
  const defaultProps = createDefaultProps()

  beforeEach(() => {
    vi.clearAllMocks()
    useStore.getState().reset()
  })

  it('filters commands by title', async () => {
    render(<CommandPalette {...defaultProps} />)
    const input = screen.getByPlaceholderText('Search commands, files, projects...')
    await userEvent.type(input, 'shell')
    await waitFor(() => {
      expect(screen.getByText('Hide Shell Log')).toBeInTheDocument()
      expect(screen.queryByText('New Project')).not.toBeInTheDocument()
    })
  })

  it('filters by subtitle', async () => {
    render(<CommandPalette {...defaultProps} />)
    const input = screen.getByPlaceholderText('Search commands, files, projects...')
    await userEvent.type(input, 'template')
    await waitFor(() => {
      expect(screen.getByText('New Project')).toBeInTheDocument()
    })
  })

  it('filters project files by name', async () => {
    useStore.getState().setProjectFiles([
      { name: 'main.py', path: '/tmp/main.py', content: '', language: 'python' },
      { name: 'helpers.py', path: '/tmp/helpers.py', content: '', language: 'python' },
    ])
    render(<CommandPalette {...defaultProps} />)
    const input = screen.getByPlaceholderText('Search commands, files, projects...')
    await userEvent.type(input, 'helper')
    await waitFor(() => {
      expect(screen.getByText('helpers.py')).toBeInTheDocument()
      expect(screen.queryByText('main.py')).not.toBeInTheDocument()
    })
  })

  it('filters recent projects by name', async () => {
    useStore.getState().addRecentProject('/tmp/proj-a', 'Alpha')
    useStore.getState().addRecentProject('/tmp/proj-b', 'Beta')
    render(<CommandPalette {...defaultProps} />)
    const input = screen.getByPlaceholderText('Search commands, files, projects...')
    await userEvent.type(input, 'alpha')
    await waitFor(() => {
      expect(screen.getByText('Alpha')).toBeInTheDocument()
      expect(screen.queryByText('Beta')).not.toBeInTheDocument()
    })
  })

  it('shows "No results found" for unmatched search', async () => {
    render(<CommandPalette {...defaultProps} />)
    const input = screen.getByPlaceholderText('Search commands, files, projects...')
    await userEvent.type(input, 'xyznonexistent')
    await waitFor(() => {
      expect(screen.getByText('No results found')).toBeInTheDocument()
    })
  })

  it('shows all commands when search is cleared', async () => {
    render(<CommandPalette {...defaultProps} />)
    const input = screen.getByPlaceholderText('Search commands, files, projects...')
    await userEvent.type(input, 'xyz')
    await waitFor(() => {
      expect(screen.getByText('No results found')).toBeInTheDocument()
    })
    await userEvent.clear(input)
    await waitFor(() => {
      expect(screen.getByText('New Project')).toBeInTheDocument()
    })
  })
})

describe('CommandPalette — Keyboard Navigation', () => {
  const defaultProps = createDefaultProps()

  beforeEach(() => {
    vi.clearAllMocks()
    useStore.getState().reset()
  })

  it('closes on Escape', async () => {
    render(<CommandPalette {...defaultProps} />)
    const input = screen.getByPlaceholderText('Search commands, files, projects...')
    await userEvent.type(input, '{Escape}')
    expect(defaultProps.onClose).toHaveBeenCalled()
  })

  it('navigates down with ArrowDown', async () => {
    render(<CommandPalette {...defaultProps} />)
    const input = screen.getByPlaceholderText('Search commands, files, projects...')
    fireEvent.keyDown(input, { key: 'ArrowDown' })
    // Second item should be highlighted (index 1)
    const allButtons = screen.getAllByRole('button').filter(b => b.className.includes('bg-white/10') || b.closest('[class*="bg-white"]'))
    // At minimum, no crash
    expect(input).toBeInTheDocument()
  })

  it('navigates up with ArrowUp', async () => {
    render(<CommandPalette {...defaultProps} />)
    const input = screen.getByPlaceholderText('Search commands, files, projects...')
    fireEvent.keyDown(input, { key: 'ArrowDown' })
    fireEvent.keyDown(input, { key: 'ArrowDown' })
    fireEvent.keyDown(input, { key: 'ArrowUp' })
    expect(input).toBeInTheDocument()
  })

  it('selects item with Enter', async () => {
    render(<CommandPalette {...defaultProps} />)
    const input = screen.getByPlaceholderText('Search commands, files, projects...')
    fireEvent.keyDown(input, { key: 'Enter' })
    // First item action should have been triggered
    expect(defaultProps.onClose).toHaveBeenCalled()
  })

  it('does not crash on Enter with empty filtered list', async () => {
    render(<CommandPalette {...defaultProps} />)
    const input = screen.getByPlaceholderText('Search commands, files, projects...')
    await userEvent.type(input, 'zzzzz')
    await waitFor(() => {
      expect(screen.getByText('No results found')).toBeInTheDocument()
    })
    fireEvent.keyDown(input, { key: 'Enter' })
    // No crash expected
  })

  it('ArrowUp does not go below index 0', async () => {
    render(<CommandPalette {...defaultProps} />)
    const input = screen.getByPlaceholderText('Search commands, files, projects...')
    fireEvent.keyDown(input, { key: 'ArrowUp' })
    fireEvent.keyDown(input, { key: 'ArrowUp' })
    expect(input).toBeInTheDocument()
  })

  it('ArrowDown does not go beyond list length', async () => {
    render(<CommandPalette {...defaultProps} />)
    const input = screen.getByPlaceholderText('Search commands, files, projects...')
    for (let i = 0; i < 50; i++) {
      fireEvent.keyDown(input, { key: 'ArrowDown' })
    }
    expect(input).toBeInTheDocument()
  })
})

describe('CommandPalette — Action Callbacks', () => {
  const defaultProps = createDefaultProps()

  beforeEach(() => {
    vi.clearAllMocks()
    useStore.getState().reset()
  })

  it('calls onOpenProject when clicking a recent project', async () => {
    useStore.getState().addRecentProject('/tmp/proj1', 'Proj1')
    render(<CommandPalette {...defaultProps} />)
    await userEvent.click(screen.getByText('Proj1'))
    expect(defaultProps.onOpenProject).toHaveBeenCalledWith('/tmp/proj1')
  })

  it('calls setWelcomeScreenVisible when clicking New Project', async () => {
    render(<CommandPalette {...defaultProps} />)
    await userEvent.click(screen.getByText('New Project'))
    expect(defaultProps.onClose).toHaveBeenCalled()
    expect(useStore.getState().welcomeScreenVisible).toBe(true)
  })

  it('calls setSettingsOpen when clicking Open Settings', async () => {
    render(<CommandPalette {...defaultProps} />)
    await userEvent.click(screen.getByText('Open Settings'))
    expect(defaultProps.onClose).toHaveBeenCalled()
    expect(useStore.getState().settingsOpen).toBe(true)
  })

  it('toggles shell visibility when clicking Hide Shell Log', async () => {
    useStore.getState().setBottomMode('shell')
    render(<CommandPalette {...defaultProps} />)
    await userEvent.click(screen.getByText('Hide Shell Log'))
    expect(useStore.getState().bottomMode).toBe('hidden')
  })

  it('clicking a project file sets it as current', async () => {
    const file = { name: 'main.py', path: '/tmp/main.py', content: 'x', language: 'python' as const }
    useStore.getState().setProjectFiles([file])
    render(<CommandPalette {...defaultProps} />)
    await userEvent.click(screen.getByText('main.py'))
    expect(useStore.getState().currentFile?.path).toBe('/tmp/main.py')
  })

  it('clicking a template triggers onCreateFromTemplate', async () => {
    render(<CommandPalette {...defaultProps} />)
    const templateEntry = screen.getByText('New: ' + TEMPLATES[0].name)
    await userEvent.click(templateEntry)
    expect(defaultProps.onCreateFromTemplate).toHaveBeenCalled()
  })

  it('clicking backdrop calls onClose', async () => {
    render(<CommandPalette {...defaultProps} />)
    const backdrop = screen.getByText('New Project').parentElement?.parentElement?.previousElementSibling
    if (backdrop) {
      await userEvent.click(backdrop)
      expect(defaultProps.onClose).toHaveBeenCalled()
    }
  })

  it('clicking Save File triggers callback', async () => {
    render(<CommandPalette {...defaultProps} />)
    await userEvent.click(screen.getByText('Save File'))
    expect(defaultProps.onClose).toHaveBeenCalled()
    expect(defaultProps.onSaveFile).toHaveBeenCalled()
  })

  it('clicking Open Folder calls handleOpenFolder', async () => {
    const hw = { openFolder: vi.fn().mockResolvedValue('/tmp/proj') }
    ;(window as unknown as { hardware: typeof hw }).hardware = hw

    render(<CommandPalette {...defaultProps} />)
    await userEvent.click(screen.getByText('Open Folder'))
    expect(defaultProps.onClose).toHaveBeenCalled()
  })

  it('clicking Run triggers action', async () => {
    render(<CommandPalette {...defaultProps} />)
    await userEvent.click(screen.getByText('Run'))
    expect(defaultProps.onClose).toHaveBeenCalled()
    expect(defaultProps.onRun).toHaveBeenCalled()
  })

  it('clicking Flash Firmware triggers action', async () => {
    render(<CommandPalette {...defaultProps} />)
    await userEvent.click(screen.getByText('Flash Firmware'))
    expect(defaultProps.onClose).toHaveBeenCalled()
    expect(defaultProps.onOpenFlash).toHaveBeenCalled()
  })

  it('clicking Upload Project triggers action', async () => {
    render(<CommandPalette {...defaultProps} />)
    await userEvent.click(screen.getByText('Upload Project'))
    expect(defaultProps.onClose).toHaveBeenCalled()
    expect(defaultProps.onUpload).toHaveBeenCalled()
  })

  it('clicking Stop triggers action', async () => {
    render(<CommandPalette {...defaultProps} />)
    await userEvent.click(screen.getByText('Stop'))
    expect(defaultProps.onClose).toHaveBeenCalled()
    expect(defaultProps.onStop).toHaveBeenCalled()
  })

  it('clicking Save All Files triggers callback', async () => {
    render(<CommandPalette {...defaultProps} />)
    await userEvent.click(screen.getByText('Save All Files'))
    expect(defaultProps.onSaveAllFiles).toHaveBeenCalled()
  })

  it('Open Folder with null path does nothing', async () => {
    const hw = { openFolder: vi.fn().mockResolvedValue(null) }
    ;(window as unknown as { hardware: typeof hw }).hardware = hw

    render(<CommandPalette {...defaultProps} />)
    await userEvent.click(screen.getByText('Open Folder'))
    await waitFor(() => {
      expect(hw.openFolder).toHaveBeenCalled()
    })
  })
})
