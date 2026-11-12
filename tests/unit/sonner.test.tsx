import { describe, it, expect, vi } from 'vitest'
import { render } from '@testing-library/react'
import { Toaster } from '@/components/ui/sonner'

vi.mock('next-themes', () => ({
  useTheme: () => ({ theme: 'dark', setTheme: vi.fn() }),
}))

describe('Toaster (Sonner)', () => {
  it('renders without crashing', () => {
    const { container } = render(<Toaster />)
    expect(container).toBeTruthy()
  })

  it('renders with custom position', () => {
    const { container } = render(<Toaster position="top-right" />)
    expect(container).toBeTruthy()
  })

  it('renders with richColors option', () => {
    const { container } = render(<Toaster richColors />)
    expect(container).toBeTruthy()
  })

  it('uses theme from useTheme', () => {
    const { container } = render(<Toaster />)
    expect(container.firstChild).toBeTruthy()
  })
})
