import { useEffect, useRef } from 'react'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import '@xterm/xterm/css/xterm.css'
import { useStore } from '@/lib/store'

type ReplHardware = {
  replSend?: (data: string) => Promise<{ success: boolean; error?: string }>
  onReplData?: (cb: (_e: unknown, data: string) => void) => () => void
}

// A real terminal for the MicroPython REPL: raw keystrokes are forwarded to the
// device and device bytes are written straight to xterm, so the device's own
// echo, cursor movement, ANSI colors and the version banner all render
// correctly — unlike a plain <pre>, which can't interpret control codes.
export function ReplTerminal() {
  const containerRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const container = containerRef.current
    if (!container) return

    const term = new Terminal({
      fontFamily: 'ui-monospace, SFMono-Regular, Menlo, monospace',
      fontSize: 12,
      cursorBlink: true,
      convertEol: false,
      theme: { background: '#050505', foreground: '#d1d5db', cursor: '#22c55e' },
    })
    const fit = new FitAddon()
    term.loadAddon(fit)
    term.open(container)
    fit.fit()

    const hw = (window as unknown as { hardware?: ReplHardware }).hardware

    // Restore prior output (e.g. after switching tabs and back).
    const prev = useStore.getState().replOutput
    if (prev) term.write(prev)

    // Device → terminal (display only; the store is updated elsewhere).
    const unsubData = hw?.onReplData?.((_e, data) => term.write(data))
    // Keyboard → device (raw bytes; the device echoes them back).
    const onData = term.onData((data) => { hw?.replSend?.(data) })

    const ro = new ResizeObserver(() => fit.fit())
    ro.observe(container)

    term.focus()

    return () => {
      unsubData?.()
      onData.dispose()
      ro.disconnect()
      term.dispose()
    }
  }, [])

  return <div ref={containerRef} className="flex-1 min-h-0 overflow-hidden bg-[#050505] px-2 py-1" />
}
