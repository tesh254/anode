import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { Button, buttonVariants } from '@/components/ui/button'

describe('Button', () => {
  it('renders with default variant', () => {
    render(<Button>Click me</Button>)
    expect(screen.getByText('Click me')).toBeInTheDocument()
  })

  it('renders with outline variant', () => {
    render(<Button variant="outline">Outline</Button>)
    const btn = screen.getByText('Outline')
    expect(btn).toBeInTheDocument()
    expect(btn.className).toContain('border-border')
  })

  it('renders with ghost variant', () => {
    render(<Button variant="ghost">Ghost</Button>)
    expect(screen.getByText('Ghost')).toBeInTheDocument()
  })

  it('renders with destructive variant', () => {
    render(<Button variant="destructive">Delete</Button>)
    expect(screen.getByText('Delete')).toBeInTheDocument()
  })

  it('renders with link variant', () => {
    render(<Button variant="link">Link</Button>)
    expect(screen.getByText('Link')).toBeInTheDocument()
  })

  it('renders with secondary variant', () => {
    render(<Button variant="secondary">Secondary</Button>)
    expect(screen.getByText('Secondary')).toBeInTheDocument()
  })

  it('renders with different sizes', () => {
    const { rerender } = render(<Button size="xs">XS</Button>)
    expect(screen.getByText('XS')).toBeInTheDocument()
    rerender(<Button size="sm">SM</Button>)
    expect(screen.getByText('SM')).toBeInTheDocument()
    rerender(<Button size="lg">LG</Button>)
    expect(screen.getByText('LG')).toBeInTheDocument()
    rerender(<Button size="icon">Icon</Button>)
    expect(screen.getByText('Icon')).toBeInTheDocument()
    rerender(<Button size="icon-xs">IconXS</Button>)
    expect(screen.getByText('IconXS')).toBeInTheDocument()
    rerender(<Button size="icon-sm">IconSM</Button>)
    expect(screen.getByText('IconSM')).toBeInTheDocument()
    rerender(<Button size="icon-lg">IconLG</Button>)
    expect(screen.getByText('IconLG')).toBeInTheDocument()
  })

  it('handles click events', async () => {
    const onClick = vi.fn()
    render(<Button onClick={onClick}>Click</Button>)
    await userEvent.click(screen.getByText('Click'))
    expect(onClick).toHaveBeenCalledOnce()
  })

  it('applies custom className', () => {
    render(<Button className="my-custom">Custom</Button>)
    expect(screen.getByText('Custom').className).toContain('my-custom')
  })

  it('renders as disabled', () => {
    render(<Button disabled>Disabled</Button>)
    const btn = screen.getByText('Disabled')
    expect(btn).toBeDisabled()
  })
})

describe('buttonVariants', () => {
  it('returns class string for default variant and size', () => {
    const result = buttonVariants()
    expect(result).toContain('bg-primary')
  })

  it('returns class string for specific variant', () => {
    const result = buttonVariants({ variant: 'outline' })
    expect(result).toContain('border-border')
  })

  it('returns class string for specific size', () => {
    const result = buttonVariants({ size: 'lg' })
    expect(result).toContain('h-9')
  })

  it('combines variant and size', () => {
    const result = buttonVariants({ variant: 'destructive', size: 'sm' })
    expect(result).toContain('text-destructive')
    expect(result).toContain('h-7')
  })

  it('applies custom className', () => {
    const result = buttonVariants({ className: 'extra-class' })
    expect(result).toContain('extra-class')
  })
})
