import { describe, it, expect, beforeEach } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { EditorTabs } from '@/components/editor-tabs'
import { useStore, type OpenTab } from '@/lib/store'

vi.mock('@monaco-editor/react', () => ({
  default: () => null,
}))

vi.mock('next-themes', () => ({
  useTheme: () => ({ theme: 'dark', setTheme: vi.fn() }),
}))

const makeTab = (overrides: Partial<OpenTab> = {}): OpenTab => ({
  path: '/main.py',
  name: 'main.py',
  isDirty: false,
  scope: 'local',
  language: 'python',
  ...overrides,
})

describe('EditorTabs', () => {
  beforeEach(() => {
    useStore.getState().reset()
  })

  it('returns null when no tabs open', () => {
    const { container } = render(<EditorTabs onTabSelect={vi.fn()} onTabClose={vi.fn()} />)
    expect(container.firstChild).toBeNull()
  })

  it('renders a tab when one is open', () => {
    useStore.getState().openTab(makeTab())
    render(<EditorTabs onTabSelect={vi.fn()} onTabClose={vi.fn()} />)
    expect(screen.getByText('main.py')).toBeInTheDocument()
  })

  it('renders multiple tabs', () => {
    useStore.getState().openTab(makeTab({ path: '/a.py', name: 'a.py' }))
    useStore.getState().openTab(makeTab({ path: '/b.py', name: 'b.py' }))
    render(<EditorTabs onTabSelect={vi.fn()} onTabClose={vi.fn()} />)
    expect(screen.getByText('a.py')).toBeInTheDocument()
    expect(screen.getByText('b.py')).toBeInTheDocument()
  })

  it('calls onTabSelect when tab is clicked', () => {
    const onSelect = vi.fn()
    useStore.getState().openTab(makeTab({ path: '/a.py', name: 'a.py' }))
    useStore.getState().openTab(makeTab({ path: '/b.py', name: 'b.py' }))
    render(<EditorTabs onTabSelect={onSelect} onTabClose={vi.fn()} />)
    fireEvent.click(screen.getByText('a.py'))
    expect(onSelect).toHaveBeenCalledWith(expect.objectContaining({ path: '/a.py' }))
  })

  it('calls onTabClose when close button is clicked', () => {
    const onClose = vi.fn()
    useStore.getState().openTab(makeTab({ path: '/a.py', name: 'a.py' }))
    render(<EditorTabs onTabSelect={vi.fn()} onTabClose={onClose} />)
    const closeBtn = screen.getByRole('button')
    fireEvent.click(closeBtn)
    expect(onClose).toHaveBeenCalledWith(expect.objectContaining({ path: '/a.py' }))
  })

  it('shows dirty indicator instead of close button when tab is dirty', () => {
    useStore.getState().openTab(makeTab({ path: '/a.py', name: 'a.py', isDirty: true }))
    render(<EditorTabs onTabSelect={vi.fn()} onTabClose={vi.fn()} />)
    const tab = screen.getByText('a.py').closest('div')
    expect(tab?.querySelector('.fill-current')).toBeTruthy()
  })

  it('shows remote indicator for remote scope tabs', () => {
    useStore.getState().openTab(makeTab({ path: ':/main.py', name: 'main.py', scope: 'remote' }))
    render(<EditorTabs onTabSelect={vi.fn()} onTabClose={vi.fn()} />)
    const tab = screen.getByText('main.py').closest('div')
    expect(tab?.querySelector('.rounded-full')).toBeTruthy()
  })

  it('highlights active tab', () => {
    useStore.getState().openTab(makeTab({ path: '/a.py', name: 'a.py' }))
    useStore.getState().openTab(makeTab({ path: '/b.py', name: 'b.py' }))
    useStore.getState().setActiveTab('/a.py')
    render(<EditorTabs onTabSelect={vi.fn()} onTabClose={vi.fn()} />)
    const activeTab = screen.getByText('a.py').closest('div')
    expect(activeTab?.className).toContain('border-b-green-500')
  })
})
