import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import { ScrollArea } from '@/components/ui/scroll-area'

describe('ScrollArea', () => {
  it('renders children inside scroll area', () => {
    render(
      <ScrollArea>
        <div>Scrollable content</div>
      </ScrollArea>
    )
    expect(screen.getByText('Scrollable content')).toBeInTheDocument()
  })

  it('renders with custom className', () => {
    const { container } = render(
      <ScrollArea className="custom-scroll">
        <div>Content</div>
      </ScrollArea>
    )
    const root = container.querySelector('[data-slot="scroll-area"]')
    expect(root?.className).toContain('custom-scroll')
  })

  it('renders with data-slot attribute', () => {
    const { container } = render(
      <ScrollArea>
        <div>Test</div>
      </ScrollArea>
    )
    expect(container.querySelector('[data-slot="scroll-area"]')).toBeTruthy()
  })

  it('renders viewport with data-slot attribute', () => {
    const { container } = render(
      <ScrollArea>
        <div>Test</div>
      </ScrollArea>
    )
    expect(container.querySelector('[data-slot="scroll-area-viewport"]')).toBeTruthy()
  })

  it('renders corner element', () => {
    const { container } = render(
      <ScrollArea>
        <div>Content</div>
      </ScrollArea>
    )
    expect(container.innerHTML).toContain('scroll-area')
  })

  it('renders multiple children', () => {
    render(
      <ScrollArea>
        <div>First</div>
        <div>Second</div>
        <div>Third</div>
      </ScrollArea>
    )
    expect(screen.getByText('First')).toBeInTheDocument()
    expect(screen.getByText('Second')).toBeInTheDocument()
    expect(screen.getByText('Third')).toBeInTheDocument()
  })
})
