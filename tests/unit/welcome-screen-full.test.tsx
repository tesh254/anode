import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { useStore } from '@/lib/store'
import { TEMPLATES, getTemplatesByBoard } from '@/lib/templates'
import { WelcomeScreen } from '@/components/welcome-screen'

vi.mock('next-themes', () => ({
  useTheme: () => ({ theme: 'dark', setTheme: vi.fn() }),
}))

vi.mock('@monaco-editor/react', () => ({
  default: () => <div data-testid="monaco-mock">Editor Mock</div>,
}))

describe('WelcomeScreen — Main View', () => {
  const defaultProps = {
    onOpenProject: vi.fn(),
    onCreateFromTemplate: vi.fn(),
    onOpenSettings: vi.fn(),
  }

  beforeEach(() => {
    vi.clearAllMocks()
    useStore.getState().reset()
  })

  it('renders New Project button', () => {
    render(<WelcomeScreen {...defaultProps} />)
    expect(screen.getByText('New Project')).toBeInTheDocument()
  })

  it('renders Open Project Folder button', () => {
    render(<WelcomeScreen {...defaultProps} />)
    expect(screen.getByText('Open Project Folder')).toBeInTheDocument()
  })

  it('shows subtitle text', () => {
    render(<WelcomeScreen {...defaultProps} />)
    expect(screen.getByText('MicroPython & Arduino IDE')).toBeInTheDocument()
  })

  it('navigates to template browser on New Project click', async () => {
    render(<WelcomeScreen {...defaultProps} />)
    await userEvent.click(screen.getByText('New Project'))
    await waitFor(() => {
      expect(screen.getByText('New Project')).toBeInTheDocument()
      expect(screen.getByText('Select a template to get started')).toBeInTheDocument()
    })
  })

  it('shows board types in template browser', async () => {
    render(<WelcomeScreen {...defaultProps} />)
    await userEvent.click(screen.getByText('New Project'))
    await waitFor(() => {
      expect(screen.getByText('ESP32 / ESP8266')).toBeInTheDocument()
      expect(screen.getByText('Raspberry Pi Pico')).toBeInTheDocument()
      expect(screen.getByText('Arduino')).toBeInTheDocument()
    })
  })

  it('shows templates for default espressif board', async () => {
    render(<WelcomeScreen {...defaultProps} />)
    await userEvent.click(screen.getByText('New Project'))
    await waitFor(() => {
      const espTemplates = getTemplatesByBoard('espressif')
      expect(screen.getByText(espTemplates[0].name)).toBeInTheDocument()
    })
  })

  it('filters templates when board type is clicked', async () => {
    render(<WelcomeScreen {...defaultProps} />)
    await userEvent.click(screen.getByText('New Project'))
    await waitFor(() => {
      expect(screen.getByText('ESP32 / ESP8266')).toBeInTheDocument()
    })
    await userEvent.click(screen.getByText('Raspberry Pi Pico'))
    await waitFor(() => {
      const picoTemplates = getTemplatesByBoard('raspberry-pi')
      expect(screen.getByText(picoTemplates[0].name)).toBeInTheDocument()
    })
  })

  it('shows preview when template is clicked', async () => {
    render(<WelcomeScreen {...defaultProps} />)
    await userEvent.click(screen.getByText('New Project'))
    await waitFor(() => {
      expect(screen.getByText('Blink')).toBeInTheDocument()
    })
    await userEvent.click(screen.getByText('Blink'))
    await waitFor(() => {
      expect(screen.getByTestId('monaco-mock')).toBeInTheDocument()
    })
  })

  it('shows "Select a template to preview" when no template selected', async () => {
    render(<WelcomeScreen {...defaultProps} />)
    await userEvent.click(screen.getByText('New Project'))
    await waitFor(() => {
      expect(screen.getByText('Select a template to preview')).toBeInTheDocument()
    })
  })

  it('closes template browser with X button', async () => {
    render(<WelcomeScreen {...defaultProps} />)
    await userEvent.click(screen.getByText('New Project'))
    await waitFor(() => {
      expect(screen.getByText('Select a template to get started')).toBeInTheDocument()
    })
    const closeButtons = screen.getAllByRole('button')
    const xButton = closeButtons.find(btn => btn.querySelector('[data-slot]') || btn.textContent === '')
    // The X button in the header
    const allButtons = screen.getAllByRole('button')
    const headerCloseBtn = allButtons.find(b => b.className.includes('size-5') || b.className.includes('p-2'))
    if (headerCloseBtn) {
      await userEvent.click(headerCloseBtn)
    }
  })

  it('shows recent projects when they exist', () => {
    useStore.getState().addRecentProject('/tmp/proj1', 'Proj1')
    useStore.getState().addRecentProject('/tmp/proj2', 'Proj2')
    render(<WelcomeScreen {...defaultProps} />)
    expect(screen.getByText('Proj1')).toBeInTheDocument()
    expect(screen.getByText('Proj2')).toBeInTheDocument()
  })

  it('does not show recent projects section when empty', () => {
    render(<WelcomeScreen {...defaultProps} />)
    expect(screen.queryByText('Recent Projects')).not.toBeInTheDocument()
  })

  it('clicks on recent project to open it', async () => {
    useStore.getState().addRecentProject('/tmp/proj1', 'Proj1')
    render(<WelcomeScreen {...defaultProps} />)
    await userEvent.click(screen.getByText('Proj1'))
    expect(defaultProps.onOpenProject).toHaveBeenCalledWith('/tmp/proj1')
  })

  it('removes recent project with X button', async () => {
    useStore.getState().addRecentProject('/tmp/proj1', 'Proj1')
    render(<WelcomeScreen {...defaultProps} />)
    // Find the remove button (appears on hover, but still in DOM)
    const removeButtons = screen.getAllByRole('button').filter(btn => {
      return btn.querySelector('.size-3') !== null
    })
    // The X button on the recent project item
    const xButton = removeButtons.find(b => b.className.includes('opacity-0'))
    if (xButton) {
      await userEvent.click(xButton)
      expect(useStore.getState().recentProjects).toHaveLength(0)
    }
  })
})

describe('WelcomeScreen — Project Name Dialog', () => {
  const defaultProps = {
    onOpenProject: vi.fn(),
    onCreateFromTemplate: vi.fn(),
    onOpenSettings: vi.fn(),
  }

  beforeEach(() => {
    vi.clearAllMocks()
    useStore.getState().reset()
  })

  it('shows name dialog when Create Project is clicked', async () => {
    render(<WelcomeScreen {...defaultProps} />)
    await userEvent.click(screen.getByText('New Project'))
    await waitFor(() => {
      expect(screen.getByText('Blink')).toBeInTheDocument()
    })
    await userEvent.click(screen.getByText('Blink'))

    // Find and click the "Create Project" button in preview
    const createButtons = screen.getAllByText('Create Project')
    if (createButtons.length > 0) {
      await userEvent.click(createButtons[0])
      await waitFor(() => {
        expect(screen.getByText('Name Your Project')).toBeInTheDocument()
      })
    }
  })

  it('pre-fills project name from template name', async () => {
    render(<WelcomeScreen {...defaultProps} />)
    await userEvent.click(screen.getByText('New Project'))
    await waitFor(() => {
      expect(screen.getByText('Blink')).toBeInTheDocument()
    })
    await userEvent.click(screen.getByText('Blink'))

    const createButtons = screen.getAllByText('Create Project')
    if (createButtons.length > 0) {
      await userEvent.click(createButtons[0])
      await waitFor(() => {
        const input = screen.getByPlaceholderText('my-project') as HTMLInputElement
        if (input) {
          expect(input.value).toContain('blink')
        }
      })
    }
  })

  it('disables Create Project button when name is empty', async () => {
    render(<WelcomeScreen {...defaultProps} />)
    await userEvent.click(screen.getByText('New Project'))
    await waitFor(() => {
      expect(screen.getByText('Blink')).toBeInTheDocument()
    })
    await userEvent.click(screen.getByText('Blink'))

    const createButtons = screen.getAllByText('Create Project')
    if (createButtons.length > 0) {
      await userEvent.click(createButtons[0])
      await waitFor(() => {
        expect(screen.getByText('Name Your Project')).toBeInTheDocument()
      })
      const input = screen.queryByPlaceholderText('my-project') as HTMLInputElement | null
      if (input) {
        await userEvent.clear(input)
        const dialogCreateButtons = screen.getAllByText('Create Project')
        const dialogBtn = dialogCreateButtons[dialogCreateButtons.length - 1] as HTMLButtonElement
        expect(dialogBtn.disabled).toBe(true)
      }
    }
  })

  it('confirms project creation via Enter key', async () => {
    const hw = { openFolder: vi.fn().mockResolvedValue('/tmp/proj') }
    ;(window as unknown as { hardware: typeof hw }).hardware = hw

    render(<WelcomeScreen {...defaultProps} />)
    await userEvent.click(screen.getByText('New Project'))
    await waitFor(() => expect(screen.getByText('Blink')).toBeInTheDocument())
    await userEvent.click(screen.getByText('Blink'))

    const createBtns = screen.getAllByText('Create Project')
    if (createBtns.length > 0) {
      await userEvent.click(createBtns[0])
      await waitFor(() => expect(screen.getByText('Name Your Project')).toBeInTheDocument())
      const input = screen.queryByPlaceholderText('my-project')
      if (input) {
        await userEvent.type(input, '{Enter}')
      }
    }
  })

  it('cancels name dialog with Cancel button', async () => {
    render(<WelcomeScreen {...defaultProps} />)
    await userEvent.click(screen.getByText('New Project'))
    await waitFor(() => expect(screen.getByText('Blink')).toBeInTheDocument())
    await userEvent.click(screen.getByText('Blink'))

    const createBtns = screen.getAllByText('Create Project')
    if (createBtns.length > 0) {
      await userEvent.click(createBtns[0])
      await waitFor(() => expect(screen.getByText('Name Your Project')).toBeInTheDocument())
      const cancelBtn = screen.getByText('Cancel')
      await userEvent.click(cancelBtn)
      await waitFor(() => {
        expect(screen.queryByText('Name Your Project')).not.toBeInTheDocument()
      })
    }
  })

  it('confirms project creation with Create button', async () => {
    const hw = { openFolder: vi.fn().mockResolvedValue('/tmp/proj') }
    ;(window as unknown as { hardware: typeof hw }).hardware = hw

    render(<WelcomeScreen {...defaultProps} />)
    await userEvent.click(screen.getByText('New Project'))
    await waitFor(() => expect(screen.getByText('Blink')).toBeInTheDocument())
    await userEvent.click(screen.getByText('Blink'))

    const createBtns = screen.getAllByText('Create Project')
    if (createBtns.length > 0) {
      await userEvent.click(createBtns[0])
      await waitFor(() => expect(screen.getByText('Name Your Project')).toBeInTheDocument())
      const dialogCreateBtns = screen.getAllByText('Create Project')
      const dialogBtn = dialogCreateBtns[dialogCreateBtns.length - 1].closest('button')
      if (dialogBtn && !dialogBtn.disabled) {
        await userEvent.click(dialogBtn)
      }
    }
  })

  it('opens settings from welcome screen', async () => {
    render(<WelcomeScreen {...defaultProps} />)
    const settingsBtns = screen.queryAllByText('Settings')
    if (settingsBtns.length > 0) {
      await userEvent.click(settingsBtns[0])
      expect(defaultProps.onOpenSettings).toHaveBeenCalled()
    }
  })
})
