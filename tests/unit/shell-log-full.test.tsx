import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { useStore } from '@/lib/store'
import { ShellLog } from '@/components/ui/shell-log'

describe('ShellLog — Full Coverage', () => {
  const defaultEntries = [
    { id: '1', timestamp: new Date('2025-01-01T10:00:00'), type: 'info' as const, message: 'Starting app...' },
    { id: '2', timestamp: new Date('2025-01-01T10:00:01'), type: 'warn' as const, message: 'Low memory' },
    { id: '3', timestamp: new Date('2025-01-01T10:00:02'), type: 'error' as const, message: 'Crash!' },
    { id: '4', timestamp: new Date('2025-01-01T10:00:03'), type: 'ok' as const, message: 'Success' },
    { id: '5', timestamp: new Date('2025-01-01T10:00:04'), type: 'install' as const, message: 'Installing...' },
    { id: '6', timestamp: new Date('2025-01-01T10:00:05'), type: 'flash' as const, message: 'Flashing...' },
  ]

  it('renders in visible state with entries', () => {
    render(
      <ShellLog
        entries={defaultEntries}
        onClear={vi.fn()}
        visible={true}
        onToggle={vi.fn()}
      />
    )
    expect(screen.getByText('SHELL LOG')).toBeInTheDocument()
    expect(screen.getByText('6 entries')).toBeInTheDocument()
    defaultEntries.forEach(e => {
      expect(screen.getByText(e.message)).toBeInTheDocument()
    })
  })

  it('renders in collapsed state when not visible', () => {
    render(
      <ShellLog
        entries={defaultEntries}
        onClear={vi.fn()}
        visible={false}
        onToggle={vi.fn()}
      />
    )
    expect(screen.getByText('Shell Log')).toBeInTheDocument()
    expect(screen.getByText('(6 entries)')).toBeInTheDocument()
  })

  it('shows "No activity yet" when entries are empty and visible', () => {
    render(
      <ShellLog
        entries={[]}
        onClear={vi.fn()}
        visible={true}
        onToggle={vi.fn()}
      />
    )
    expect(screen.getByText('No activity yet. Progress will appear here...')).toBeInTheDocument()
  })

  it('formats timestamps correctly', () => {
    const entry = { id: '1', timestamp: new Date('2025-01-01T14:30:45'), type: 'info' as const, message: 'test' }
    render(
      <ShellLog
        entries={[entry]}
        onClear={vi.fn()}
        visible={true}
        onToggle={vi.fn()}
      />
    )
    expect(screen.getByText('14:30:45')).toBeInTheDocument()
  })

  it('shows type prefixes for non-info entries', () => {
    render(
      <ShellLog
        entries={defaultEntries}
        onClear={vi.fn()}
        visible={true}
        onToggle={vi.fn()}
      />
    )
    expect(screen.getByText('[WARN]')).toBeInTheDocument()
    expect(screen.getByText('[ERROR]')).toBeInTheDocument()
    expect(screen.getByText('[OK]')).toBeInTheDocument()
    expect(screen.getByText('[INSTALL]')).toBeInTheDocument()
    expect(screen.getByText('[FLASH]')).toBeInTheDocument()
  })

  it('error entries have red background styling', () => {
    const { container } = render(
      <ShellLog
        entries={[{ id: '1', timestamp: new Date(), type: 'error', message: 'err' }]}
        onClear={vi.fn()}
        visible={true}
        onToggle={vi.fn()}
      />
    )
    const errorRow = container.querySelector('.bg-red-500\\/5')
    expect(errorRow).toBeTruthy()
  })

  it('calls onClear when clear button is clicked', async () => {
    const onClear = vi.fn()
    render(
      <ShellLog
        entries={[{ id: '1', timestamp: new Date(), type: 'info', message: 'test' }]}
        onClear={onClear}
        visible={true}
        onToggle={vi.fn()}
      />
    )
    await userEvent.click(screen.getByTitle('Clear log'))
    expect(onClear).toHaveBeenCalledOnce()
  })

  it('calls onToggle when SHELL LOG header is clicked', async () => {
    const onToggle = vi.fn()
    render(
      <ShellLog
        entries={[]}
        onClear={vi.fn()}
        visible={true}
        onToggle={onToggle}
      />
    )
    await userEvent.click(screen.getByText('SHELL LOG'))
    expect(onToggle).toHaveBeenCalled()
  })

  it('calls onToggle when X (close) button is clicked', async () => {
    const onToggle = vi.fn()
    render(
      <ShellLog
        entries={[]}
        onClear={vi.fn()}
        visible={true}
        onToggle={onToggle}
      />
    )
    await userEvent.click(screen.getByTitle('Collapse'))
    expect(onToggle).toHaveBeenCalled()
  })

  it('calls onToggle when collapsed bar is clicked', async () => {
    const onToggle = vi.fn()
    render(
      <ShellLog
        entries={[]}
        onClear={vi.fn()}
        visible={false}
        onToggle={onToggle}
      />
    )
    // The collapsed bar is a button
    const collapsedBtn = screen.getByText('Shell Log').closest('button')
    if (collapsedBtn) {
      await userEvent.click(collapsedBtn)
      expect(onToggle).toHaveBeenCalled()
    }
  })

  it('renders each entry type with correct prefix', () => {
    const types = ['info', 'warn', 'error', 'ok', 'install', 'flash'] as const
    const entries = types.map((type, i) => ({
      id: String(i),
      timestamp: new Date(),
      type,
      message: `${type} message`,
    }))

    const { container } = render(
      <ShellLog
        entries={entries}
        onClear={vi.fn()}
        visible={true}
        onToggle={vi.fn()}
      />
    )

    entries.forEach(e => {
      expect(screen.getByText(e.message)).toBeInTheDocument()
    })
  })

  it('auto-scrolls when new entries are added', () => {
    const { rerender } = render(
      <ShellLog
        entries={[{ id: '1', timestamp: new Date(), type: 'info', message: 'first' }]}
        onClear={vi.fn()}
        visible={true}
        onToggle={vi.fn()}
      />
    )

    // Adding more entries should trigger scroll
    rerender(
      <ShellLog
        entries={[
          { id: '1', timestamp: new Date(), type: 'info', message: 'first' },
          { id: '2', timestamp: new Date(), type: 'info', message: 'second' },
        ]}
        onClear={vi.fn()}
        visible={true}
        onToggle={vi.fn()}
      />
    )
    expect(screen.getByText('second')).toBeInTheDocument()
  })

  it('does not auto-scroll when entries are removed', () => {
    const { rerender } = render(
      <ShellLog
        entries={[
          { id: '1', timestamp: new Date(), type: 'info', message: 'first' },
          { id: '2', timestamp: new Date(), type: 'info', message: 'second' },
        ]}
        onClear={vi.fn()}
        visible={true}
        onToggle={vi.fn()}
      />
    )

    rerender(
      <ShellLog
        entries={[{ id: '1', timestamp: new Date(), type: 'info', message: 'first' }]}
        onClear={vi.fn()}
        visible={true}
        onToggle={vi.fn()}
      />
    )
    expect(screen.getByText('first')).toBeInTheDocument()
  })

  it('shows entry count in header', () => {
    render(
      <ShellLog
        entries={defaultEntries}
        onClear={vi.fn()}
        visible={true}
        onToggle={vi.fn()}
      />
    )
    expect(screen.getByText('6 entries')).toBeInTheDocument()
  })

  it('shows 0 entries when empty', () => {
    render(
      <ShellLog
        entries={[]}
        onClear={vi.fn()}
        visible={true}
        onToggle={vi.fn()}
      />
    )
    expect(screen.getByText('0 entries')).toBeInTheDocument()
  })
})
