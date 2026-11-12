import { useEffect, useRef, useState, useCallback } from 'react'
import { useStore } from '@/lib/store'

export function ReplPanel() {
  const { selectedPort, replConnected, replOutput, replVisible,
    setReplConnected, appendReplOutput, clearReplOutput, setReplVisible } = useStore()
  const [input, setInput] = useState('')
  const [history, setHistory] = useState<string[]>([])
  const [, setHistoryIndex] = useState(-1)
  const outputRef = useRef<HTMLPreElement>(null)
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    if (outputRef.current) {
      outputRef.current.scrollTop = outputRef.current.scrollHeight
    }
  }, [replOutput])

  useEffect(() => {
    const hw = (window as unknown as {
      hardware?: {
        onReplData?: (cb: (_e: unknown, data: string) => void) => () => void
        onReplDisconnected?: (cb: () => void) => () => void
        onReplError?: (cb: (_e: unknown, err: string) => void) => () => void
      }
    }).hardware

    if (!hw?.onReplData || !hw?.onReplDisconnected || !hw?.onReplError) return

    const unsubData = hw.onReplData((_e, data) => {
      appendReplOutput(data)
    })
    const unsubDisconnect = hw.onReplDisconnected(() => {
      setReplConnected(false)
      appendReplOutput('\n--- Disconnected ---\n')
    })
    const unsubError = hw.onReplError((_e, err) => {
      appendReplOutput(`\n[ERROR] ${err}\n`)
    })

    return () => { unsubData(); unsubDisconnect(); unsubError() }
  }, [appendReplOutput, setReplConnected])

  const handleConnect = useCallback(async () => {
    const hw = (window as unknown as {
      hardware?: { replConnect: (port: string) => Promise<{ success: boolean; error?: string }> }
    }).hardware
    if (!hw || !selectedPort) return
    clearReplOutput()
    const result = await hw.replConnect(selectedPort)
    if (result.success) {
      setReplConnected(true)
      appendReplOutput(`--- Connected to ${selectedPort} ---\n`)
    } else {
      appendReplOutput(`[ERROR] Connection failed: ${result.error || 'Unknown error'}\n`)
    }
  }, [selectedPort, clearReplOutput, setReplConnected, appendReplOutput])

  const handleDisconnect = useCallback(async () => {
    const hw = (window as unknown as {
      hardware?: { replDisconnect: () => Promise<{ success: boolean }> }
    }).hardware
    if (!hw) return
    await hw.replDisconnect()
    setReplConnected(false)
    appendReplOutput('\n--- Disconnected ---\n')
  }, [setReplConnected, appendReplOutput])

  const handleSend = useCallback(async (data: string) => {
    const hw = (window as unknown as {
      hardware?: { replSend: (d: string) => Promise<{ success: boolean; error?: string }> }
    }).hardware
    if (!hw) return
    const result = await hw.replSend(data)
    if (!result.success) {
      appendReplOutput(`[ERROR] Send failed: ${result.error}\n`)
    }
  }, [appendReplOutput])

  const handleBreak = useCallback(async () => {
    const hw = (window as unknown as {
      hardware?: { replSendBreak: () => Promise<{ success: boolean }> }
    }).hardware
    if (!hw) return
    await hw.replSendBreak()
    appendReplOutput('^C')
  }, [appendReplOutput])

  const handleSoftReset = useCallback(async () => {
    const hw = (window as unknown as {
      hardware?: { replSendSoftReset: () => Promise<{ success: boolean }> }
    }).hardware
    if (!hw) return
    await hw.replSendSoftReset()
    appendReplOutput('\n[Soft Reset]\n')
  }, [appendReplOutput])

  const handleSubmit = useCallback((e: React.FormEvent) => {
    e.preventDefault()
    if (!input.trim() && !replConnected) return
    const line = input + '\r\n'
    handleSend(line)
    if (input.trim()) {
      setHistory((h) => [...h, input])
    }
    setHistoryIndex(-1)
    setInput('')
  }, [input, replConnected, handleSend])

  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === 'ArrowUp') {
      e.preventDefault()
      setHistoryIndex((i) => {
        const newIndex = Math.min(i + 1, history.length - 1)
        if (newIndex >= 0) setInput(history[history.length - 1 - newIndex])
        return newIndex
      })
    } else if (e.key === 'ArrowDown') {
      e.preventDefault()
      setHistoryIndex((i) => {
        const newIndex = i - 1
        if (newIndex < 0) { setInput(''); return -1 }
        setInput(history[history.length - 1 - newIndex])
        return newIndex
      })
    }
  }, [history])

  if (!replVisible) return null

  return (
    <div className="flex flex-col h-full bg-[#0d0d0d] border-t border-white/5">
      <div className="flex items-center justify-between px-3 py-1.5 border-b border-white/5 bg-black/30">
        <div className="flex items-center gap-2">
          <span className="text-[11px] font-bold text-gray-500 tracking-widest uppercase">REPL</span>
          {replConnected && (
            <span className="w-1.5 h-1.5 rounded-full bg-green-500" />
          )}
          <span className="text-[11px] text-gray-600 font-mono">
            {replConnected ? selectedPort : 'Disconnected'}
          </span>
        </div>
        <div className="flex items-center gap-1">
          {!replConnected ? (
            <button
              onClick={handleConnect}
              disabled={!selectedPort}
              className="px-2 py-0.5 text-[11px] bg-green-600 hover:bg-green-700 text-white rounded disabled:opacity-40"
            >
              Connect
            </button>
          ) : (
            <>
              <button
                onClick={handleBreak}
                className="px-2 py-0.5 text-[11px] bg-yellow-600 hover:bg-yellow-700 text-white rounded"
                title="Send Ctrl+C"
              >
                ^C
              </button>
              <button
                onClick={handleSoftReset}
                className="px-2 py-0.5 text-[11px] bg-blue-600 hover:bg-blue-700 text-white rounded"
                title="Soft Reset (Ctrl+D)"
              >
                Reset
              </button>
              <button
                onClick={handleDisconnect}
                className="px-2 py-0.5 text-[11px] bg-red-600 hover:bg-red-700 text-white rounded"
              >
                Disconnect
              </button>
            </>
          )}
          <button
            onClick={() => setReplVisible(false)}
            className="px-2 py-0.5 text-[11px] text-gray-500 hover:text-white rounded"
          >
            Hide
          </button>
        </div>
      </div>
      <pre
        ref={outputRef}
        className="flex-1 p-3 font-mono text-[12px] text-gray-300 overflow-y-auto whitespace-pre-wrap leading-relaxed select-text min-h-0"
      >
        {replOutput || (replConnected ? '' : 'Click "Connect" to start the REPL...')}
      </pre>
      {replConnected && (
        <form onSubmit={handleSubmit} className="flex items-center border-t border-white/5 px-3 py-1.5 bg-black/30">
          <span className="text-gray-500 font-mono text-[12px] mr-2">&gt;&gt;&gt;</span>
          <input
            ref={inputRef}
            type="text"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            className="flex-1 bg-transparent text-gray-200 font-mono text-[12px] outline-none placeholder-gray-700"
            placeholder="Type MicroPython command..."
            autoComplete="off"
            spellCheck={false}
          />
        </form>
      )}
    </div>
  )
}
