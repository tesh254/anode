import { describe, it, expect } from 'vitest'
import { cn } from '@/lib/utils'

describe('cn', () => {
  it('merges class names', () => {
    expect(cn('foo', 'bar')).toBe('foo bar')
  })

  it('handles conditional classes', () => {
    const cond = false
    expect(cn('base', cond && 'hidden', 'visible')).toBe('base visible')
  })

  it('handles undefined and null', () => {
    expect(cn('base', undefined, null, 'end')).toBe('base end')
  })

  it('handles empty strings', () => {
    expect(cn('base', '', 'end')).toBe('base end')
  })

  it('merges tailwind conflict classes', () => {
    expect(cn('px-4', 'px-6')).toBe('px-6')
  })

  it('merges responsive tailwind classes', () => {
    expect(cn('px-4', 'md:px-6')).toBe('px-4 md:px-6')
  })

  it('handles no arguments', () => {
    expect(cn()).toBe('')
  })

  it('handles arrays of classes', () => {
    expect(cn(['foo', 'bar'], 'baz')).toBe('foo bar baz')
  })

  it('deduplicates conflicting tailwind utilities', () => {
    expect(cn('text-red-500', 'text-blue-500')).toBe('text-blue-500')
  })
})
