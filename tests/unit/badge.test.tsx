import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { Badge, badgeVariants } from '@/components/ui/badge'

describe('Badge', () => {
  it('renders with default variant', () => {
    render(<Badge>Default</Badge>)
    expect(screen.getByText('Default')).toBeInTheDocument()
  })

  it('renders with outline variant', () => {
    render(<Badge variant="outline">Outline</Badge>)
    expect(screen.getByText('Outline')).toBeInTheDocument()
  })

  it('renders with secondary variant', () => {
    render(<Badge variant="secondary">Secondary</Badge>)
    expect(screen.getByText('Secondary')).toBeInTheDocument()
  })

  it('renders with destructive variant', () => {
    render(<Badge variant="destructive">Destructive</Badge>)
    expect(screen.getByText('Destructive')).toBeInTheDocument()
  })

  it('renders with ghost variant', () => {
    render(<Badge variant="ghost">Ghost</Badge>)
    expect(screen.getByText('Ghost')).toBeInTheDocument()
  })

  it('renders with link variant', () => {
    render(<Badge variant="link">Link</Badge>)
    expect(screen.getByText('Link')).toBeInTheDocument()
  })

  it('applies custom className', () => {
    render(<Badge className="custom-class">Custom</Badge>)
    expect(screen.getByText('Custom').className).toContain('custom-class')
  })

  it('renders as a span element by default', () => {
    render(<Badge>Test</Badge>)
    expect(screen.getByText('Test').tagName.toLowerCase()).toBe('span')
  })
})

describe('badgeVariants', () => {
  it('returns class string for default variant', () => {
    const result = badgeVariants()
    expect(result).toContain('bg-primary')
  })

  it('returns class string for outline variant', () => {
    const result = badgeVariants({ variant: 'outline' })
    expect(result).toContain('border-border')
  })

  it('returns class string for destructive variant', () => {
    const result = badgeVariants({ variant: 'destructive' })
    expect(result).toContain('text-destructive')
  })

  it('applies custom className', () => {
    const result = badgeVariants({ className: 'extra' })
    expect(result).toContain('extra')
  })
})
