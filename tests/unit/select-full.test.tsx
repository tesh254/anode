import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import {
  Select, SelectContent, SelectGroup, SelectItem,
  SelectLabel, SelectSeparator, SelectTrigger, SelectValue,
} from '@/components/ui/select'

describe('Select sub-components', () => {
  it('renders SelectTrigger with default size', () => {
    const { container } = render(
      <Select defaultValue="a">
        <SelectTrigger><SelectValue /></SelectTrigger>
        <SelectContent>
          <SelectItem value="a">Item A</SelectItem>
        </SelectContent>
      </Select>
    )
    const trigger = container.querySelector('[data-slot="select-trigger"]')
    expect(trigger).toBeTruthy()
    expect(trigger?.getAttribute('data-size')).toBe('default')
  })

  it('renders SelectTrigger with sm size', () => {
    const { container } = render(
      <Select defaultValue="a">
        <SelectTrigger size="sm"><SelectValue /></SelectTrigger>
        <SelectContent>
          <SelectItem value="a">Item A</SelectItem>
        </SelectContent>
      </Select>
    )
    const trigger = container.querySelector('[data-slot="select-trigger"]')
    expect(trigger?.getAttribute('data-size')).toBe('sm')
  })

  it('renders SelectValue', () => {
    const { container } = render(
      <Select defaultValue="a">
        <SelectTrigger><SelectValue /></SelectTrigger>
        <SelectContent>
          <SelectItem value="a">Item A</SelectItem>
        </SelectContent>
      </Select>
    )
    expect(container.querySelector('[data-slot="select-value"]')).toBeTruthy()
  })

  it('renders with custom className on trigger', () => {
    const { container } = render(
      <Select defaultValue="a">
        <SelectTrigger className="custom-trigger"><SelectValue /></SelectTrigger>
        <SelectContent>
          <SelectItem value="a">Item A</SelectItem>
        </SelectContent>
      </Select>
    )
    const trigger = container.querySelector('[data-slot="select-trigger"]')
    expect(trigger?.className).toContain('custom-trigger')
  })

  it('SelectContent renders with custom side and align', () => {
    const { container } = render(
      <Select defaultValue="a">
        <SelectTrigger><SelectValue /></SelectTrigger>
        <SelectContent side="top" align="start">
          <SelectItem value="a">Item A</SelectItem>
        </SelectContent>
      </Select>
    )
    expect(container).toBeTruthy()
  })

  it('opens Select and shows items', async () => {
    render(
      <Select defaultValue="a">
        <SelectTrigger><SelectValue /></SelectTrigger>
        <SelectContent>
          <SelectItem value="a">Alpha</SelectItem>
          <SelectItem value="b">Beta</SelectItem>
        </SelectContent>
      </Select>
    )

    await userEvent.click(screen.getByRole('combobox'))

    await expect(screen.findByText('Alpha')).resolves.toBeInTheDocument()
  })

  it('SelectGroup and SelectLabel render when open', async () => {
    render(
      <Select defaultValue="a">
        <SelectTrigger><SelectValue /></SelectTrigger>
        <SelectContent>
          <SelectGroup>
            <SelectLabel>Category</SelectLabel>
            <SelectItem value="a">Item A</SelectItem>
          </SelectGroup>
        </SelectContent>
      </Select>
    )

    await userEvent.click(screen.getByRole('combobox'))
    await expect(screen.findByText('Category')).resolves.toBeInTheDocument()
  })

  it('SelectSeparator renders when select is open', async () => {
    render(
      <Select defaultValue="a">
        <SelectTrigger><SelectValue /></SelectTrigger>
        <SelectContent>
          <SelectItem value="a">Item A</SelectItem>
          <SelectSeparator />
          <SelectItem value="b">Item B</SelectItem>
        </SelectContent>
      </Select>
    )

    await userEvent.click(screen.getByRole('combobox'))
    await expect(screen.findByText('Item B')).resolves.toBeInTheDocument()
  })
})
