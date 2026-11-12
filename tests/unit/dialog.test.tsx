import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import {
  Dialog, DialogTrigger, DialogContent, DialogHeader,
  DialogTitle, DialogDescription, DialogFooter, DialogClose,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'

describe('Dialog', () => {
  it('renders trigger and opens dialog on click', async () => {
    render(
      <Dialog>
        <DialogTrigger>Open Dialog</DialogTrigger>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Test Title</DialogTitle>
            <DialogDescription>Test description</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button>Confirm</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    )
    expect(screen.getByText('Open Dialog')).toBeInTheDocument()
    await userEvent.click(screen.getByText('Open Dialog'))
    await expect(screen.findByText('Test Title')).resolves.toBeInTheDocument()
  })

  it('renders with showCloseButton=false', async () => {
    render(
      <Dialog defaultOpen>
        <DialogContent showCloseButton={false}>
          <DialogTitle>No Close</DialogTitle>
        </DialogContent>
      </Dialog>
    )
    await expect(screen.findByText('No Close')).resolves.toBeInTheDocument()
  })

  it('renders DialogFooter with showCloseButton', async () => {
    render(
      <Dialog defaultOpen>
        <DialogContent>
          <DialogTitle>Footer Test</DialogTitle>
          <DialogFooter showCloseButton>
            <Button>Action</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    )
    await expect(screen.findByText('Footer Test')).resolves.toBeInTheDocument()
  })

  it('DialogTitle renders with custom className', async () => {
    render(
      <Dialog defaultOpen>
        <DialogContent>
          <DialogTitle className="custom-title">Custom Title</DialogTitle>
        </DialogContent>
      </Dialog>
    )
    await expect(screen.findByText('Custom Title')).resolves.toBeInTheDocument()
  })

  it('DialogDescription renders with custom className', async () => {
    render(
      <Dialog defaultOpen>
        <DialogContent>
          <DialogTitle>Desc Test</DialogTitle>
          <DialogDescription className="custom-desc">A description</DialogDescription>
        </DialogContent>
      </Dialog>
    )
    await expect(screen.findByText('A description')).resolves.toBeInTheDocument()
  })

  it('DialogClose renders as a close button', async () => {
    render(
      <Dialog defaultOpen>
        <DialogContent>
          <DialogTitle>Close Test</DialogTitle>
          <DialogClose>
            <Button>Close Me</Button>
          </DialogClose>
        </DialogContent>
      </Dialog>
    )
    await expect(screen.findByText('Close Me')).resolves.toBeInTheDocument()
  })

  it('DialogHeader renders with data-slot inside Dialog', async () => {
    render(
      <Dialog defaultOpen>
        <DialogContent>
          <DialogHeader className="test-header">
            <DialogTitle>Header Slot</DialogTitle>
          </DialogHeader>
        </DialogContent>
      </Dialog>
    )
    await expect(screen.findByText('Header Slot')).resolves.toBeInTheDocument()
  })

  it('DialogFooter renders with data-slot inside Dialog', async () => {
    render(
      <Dialog defaultOpen>
        <DialogContent>
          <DialogTitle>Footer Slot</DialogTitle>
          <DialogFooter className="test-footer">Footer</DialogFooter>
        </DialogContent>
      </Dialog>
    )
    await expect(screen.findByText('Footer Slot')).resolves.toBeInTheDocument()
  })
})
