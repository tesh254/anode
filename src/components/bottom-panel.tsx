import { useEffect, useRef, useState, useCallback } from 'react'
import { Terminal, Trash2, ChevronUp, ChevronDown, Copy, Download } from 'lucide-react'
import { useStore } from '@/lib/store'
import { cn } from '@/lib/utils'
import type { LogEntry } from '@/components/ui/shell-log'
import { ReplTerminal } from '@/components/repl-terminal'

type BottomPanelProps = {
  logEntries: LogEntry[]
  onClearLogs: () => void
}

const TYPE_STYLES: Record<LogEntry['type'], string> = {
  info: 'text-green-600 dark:text-green-500',
  warn: 'text-yellow-600 dark:text-yellow-400',
  error: 'text-red-600 dark:text-red-400',
  ok: 'text-green-600 dark:text-green-400',
  install: 'text-blue-600 dark:text-blue-400',
  flash: 'text-cyan-600 dark:text-cyan-400',
}

const TYPE_PREFIX: Record<LogEntry['type'], string> = {
  info: '',
  warn: '[WARN]',
  error: '[ERROR]',
  ok: '[OK]',
  install: '[INSTALL]',
  flash: '[FLASH]',
}

function formatTime(date: Date) {
  return date.toLocaleTimeString('en-US', { hour12: false, hour: '2-digit', minute: '2-digit', second: '2-digit' })
}

export function BottomPanel({ logEntries, onClearLogs }: BottomPanelProps) {
  const {
    bottomMode,
    setBottomMode,
    selectedPort,
    replConnected,
    replOutput,
    setReplConnected,
    setReplConnecting,
    appendReplOutput,
    clearReplOutput,
  } = useStore()

  const [copyState, setCopyState] = useState<'idle' | 'copied' | 'error'>('idle')

  const shellScrollRef = useRef<HTMLDivElement>(null)

  // Auto-scroll shell
  useEffect(() => {
    if (bottomMode === 'shell' && shellScrollRef.current) {
      shellScrollRef.current.scrollTop = shellScrollRef.current.scrollHeight
    }
  }, [logEntries, bottomMode])

  // Subscribe to REPL IPC events
  useEffect(() => {
    const hw = (window as unknown as {
      hardware?: {
        onReplData?: (cb: (_e: unknown, data: string) => void) => () => void
        onReplDisconnected?: (cb: () => void) => () => void
        onReplError?: (cb: (_e: unknown, err: string) => void) => () => void
      }
    }).hardware
    if (!hw?.onReplData || !hw?.onReplDisconnected || !hw?.onReplError) return
    const unsubData = hw.onReplData((_e, data) => appendReplOutput(data))
    const unsubDisconnect = hw.onReplDisconnected(() => {
      setReplConnected(false)
      appendReplOutput('\n--- Disconnected ---\n')
    })
    const unsubError = hw.onReplError((_e, err) => appendReplOutput(`\n[ERROR] ${err}\n`))
    return () => { unsubData(); unsubDisconnect(); unsubError() }
  }, [appendReplOutput, setReplConnected])

  const handleReplConnect = useCallback(async () => {
    const hw = (window as unknown as {
      hardware?: { replConnect: (port: string) => Promise<{ success: boolean; error?: string }> }
    }).hardware
    if (!hw || !selectedPort) return
    clearReplOutput()
    // Stop the device-tree `mpremote ls` probe from grabbing the port mid-connect.
    setReplConnecting(true)
    try {
      const result = await hw.replConnect(selectedPort)
      if (result.success) {
        setReplConnected(true)
        appendReplOutput(`--- Connected to ${selectedPort} ---\n`)
      } else {
        appendReplOutput(`[ERROR] Connection failed: ${result.error || 'Unknown error'}\n`)
      }
    } finally {
      setReplConnecting(false)
    }
  }, [selectedPort, clearReplOutput, setReplConnected, setReplConnecting, appendReplOutput])

  const handleReplDisconnect = useCallback(async () => {
    const hw = (window as unknown as {
      hardware?: { replDisconnect: () => Promise<{ success: boolean }> }
    }).hardware
    if (!hw) return
    await hw.replDisconnect()
    setReplConnected(false)
    appendReplOutput('\n--- Disconnected ---\n')
  }, [setReplConnected, appendReplOutput])

  // ^C and soft-reset are sent as raw bytes; the device echoes them, so the
  // terminal renders the result — no local echo needed.
  const handleBreak = useCallback(async () => {
    const hw = (window as unknown as {
      hardware?: { replSendBreak: () => Promise<{ success: boolean }> }
    }).hardware
    await hw?.replSendBreak()
  }, [])

  const handleSoftReset = useCallback(async () => {
    const hw = (window as unknown as {
      hardware?: { replSendSoftReset: () => Promise<{ success: boolean }> }
    }).hardware
    await hw?.replSendSoftReset()
  }, [])

  const handleCopy = useCallback(async () => {
    const text = bottomMode === 'shell'
      ? logEntries.map((e) => `${formatTime(e.timestamp)} ${TYPE_PREFIX[e.type]} ${e.message}`).join('\n')
      : replOutput
    try {
      await navigator.clipboard.writeText(text)
      setCopyState('copied')
      setTimeout(() => setCopyState('idle'), 1500)
    } catch {
      setCopyState('error')
      setTimeout(() => setCopyState('idle'), 1500)
    }
  }, [bottomMode, logEntries, replOutput])

  const handleExport = useCallback(async () => {
    const hw = (window as unknown as {
      hardware?: { saveReplExport?: (c: string, n?: string) => Promise<{ success: boolean; canceled?: boolean; error?: string; filePath?: string }> }
    }).hardware
    if (!hw?.saveReplExport) return
    const content = bottomMode === 'shell'
      ? logEntries.map((e) => `${formatTime(e.timestamp)} ${TYPE_PREFIX[e.type]} ${e.message}`).join('\n')
      : replOutput
    const stamp = new Date().toISOString().replace(/[:.]/g, '-')
    const defaultName = bottomMode === 'shell' ? `shell-log-${stamp}.txt` : `repl-output-${stamp}.txt`
    const result = await hw.saveReplExport(content, defaultName)
    if (!result.success && result.error) {
      appendReplOutput(`[ERROR] Export failed: ${result.error}\n`)
    } else if (result.success && !result.canceled && result.filePath) {
      appendReplOutput(`[OK] Exported to ${result.filePath}\n`)
    }
  }, [bottomMode, logEntries, replOutput, appendReplOutput])

  // Collapsed strip
  if (bottomMode === 'hidden') {
    return (
      <button
        onClick={() => setBottomMode('shell')}
        className="absolute bottom-0 left-0 right-0 h-8 border-t border-gray-200 dark:border-white/5 bg-gray-100 dark:bg-[#0f0f0f] flex items-center px-4 cursor-pointer hover:bg-gray-200 dark:hover:bg-white/5 transition-colors z-20"
      >
        <div className="flex items-center gap-3 text-[10px] font-bold text-gray-500 dark:text-gray-500 tracking-widest uppercase">
          <ChevronUp className="size-3.5" />
          <span>SHELL</span>
          <span className="text-gray-700 dark:text-gray-700">·</span>
          <span>REPL</span>
          {logEntries.length > 0 && (
            <span className="text-green-600 dark:text-green-500/60 ml-1">({logEntries.length} entries)</span>
          )}
          {replConnected && (
            <span className="ml-1 w-1.5 h-1.5 rounded-full bg-green-500 inline-block" />
          )}
        </div>
      </button>
    )
  }

  const isShell = bottomMode === 'shell'

  return (
    <div className="h-64 max-h-[40vh] min-h-[80px] border-t border-gray-200 dark:border-white/5 bg-gray-50 dark:bg-[#0a0a0a] flex flex-col shrink-0 relative overflow-hidden">
      {/* Header with tab toggle */}
      <div className="flex items-center justify-between px-2 h-10 border-b border-gray-200 dark:border-white/5 bg-gray-100 dark:bg-[#0f0f0f] shrink-0">
        <div className="flex items-center gap-1">
          {/* Shell tab */}
          <button
            onClick={() => setBottomMode('shell')}
            className={cn(
              'flex items-center gap-2 px-3 h-8 rounded-md text-xs font-semibold tracking-wider transition-colors',
              isShell
                ? 'bg-white dark:bg-white/10 text-green-600 dark:text-green-500'
                : 'text-gray-500 dark:text-gray-500 hover:text-gray-700 dark:hover:text-gray-300 hover:bg-gray-200/50 dark:hover:bg-white/5'
            )}
          >
            <Terminal className="size-3.5" />
            SHELL
            {logEntries.length > 0 && (
              <span className="text-[10px] text-gray-500 dark:text-gray-600">({logEntries.length})</span>
            )}
          </button>
          {/* REPL tab */}
          <button
            onClick={() => setBottomMode('repl')}
            className={cn(
              'flex items-center gap-2 px-3 h-8 rounded-md text-xs font-semibold tracking-wider transition-colors',
              !isShell
                ? 'bg-white dark:bg-white/10 text-green-600 dark:text-green-500'
                : 'text-gray-500 dark:text-gray-500 hover:text-gray-700 dark:hover:text-gray-300 hover:bg-gray-200/50 dark:hover:bg-white/5'
            )}
          >
            <span className="size-2 rounded-full bg-gray-400 dark:bg-gray-600" />
            REPL
            {replConnected && (
              <span className="w-1.5 h-1.5 rounded-full bg-green-500" />
            )}
          </button>
        </div>

        <div className="flex items-center gap-1">
          {/* Mode-specific actions */}
          {isShell ? (
            <>
              <span className="text-[10px] text-gray-500 dark:text-gray-600 mr-2">{logEntries.length} entries</span>
              <button
                onClick={onClearLogs}
                className="p-1.5 text-gray-500 dark:text-gray-500 hover:text-gray-700 dark:hover:text-gray-300 hover:bg-gray-200 dark:hover:bg-white/5 rounded transition-colors"
                title="Clear log"
              >
                <Trash2 className="size-3.5" />
              </button>
            </>
          ) : (
            <>
              <span className="text-[10px] text-gray-500 dark:text-gray-600 mr-2 font-mono">
                {replConnected ? selectedPort : 'Disconnected'}
              </span>
              {!replConnected ? (
                <button
                  onClick={handleReplConnect}
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
                    onClick={handleReplDisconnect}
                    className="px-2 py-0.5 text-[11px] bg-red-600 hover:bg-red-700 text-white rounded"
                  >
                    Disconnect
                  </button>
                  <button
                    onClick={() => clearReplOutput()}
                    className="p-1.5 text-gray-500 dark:text-gray-500 hover:text-gray-700 dark:hover:text-gray-300 hover:bg-gray-200 dark:hover:bg-white/5 rounded transition-colors"
                    title="Clear REPL output"
                  >
                    <Trash2 className="size-3.5" />
                  </button>
                </>
              )}
            </>
          )}
          {/* Shared copy/export actions */}
          <button
            onClick={handleCopy}
            className={cn(
              'p-1.5 rounded transition-colors',
              copyState === 'copied'
                ? 'text-green-500'
                : copyState === 'error'
                ? 'text-red-500'
                : 'text-gray-500 dark:text-gray-500 hover:text-gray-700 dark:hover:text-gray-300 hover:bg-gray-200 dark:hover:bg-white/5'
            )}
            title={copyState === 'copied' ? 'Copied!' : copyState === 'error' ? 'Copy failed' : 'Copy to clipboard'}
          >
            <Copy className="size-3.5" />
          </button>
          <button
            onClick={handleExport}
            className="p-1.5 text-gray-500 dark:text-gray-500 hover:text-gray-700 dark:hover:text-gray-300 hover:bg-gray-200 dark:hover:bg-white/5 rounded transition-colors"
            title={`Export ${isShell ? 'shell log' : 'REPL output'} to file`}
          >
            <Download className="size-3.5" />
          </button>
          <button
            onClick={() => setBottomMode('hidden')}
            className="p-1.5 text-gray-500 dark:text-gray-500 hover:text-gray-700 dark:hover:text-gray-300 hover:bg-gray-200 dark:hover:bg-white/5 rounded transition-colors"
            title="Collapse"
          >
            <ChevronDown className="size-3.5" />
          </button>
        </div>
      </div>

      {/* Body — renders one of the two modes */}
      {isShell ? (
        <div
          ref={shellScrollRef}
          className="flex-1 overflow-y-auto overflow-x-hidden p-4 font-mono text-[13px] leading-relaxed bg-white dark:bg-[#050505] min-h-0 select-text"
        >
          {logEntries.length === 0 ? (
            <div className="text-gray-400 dark:text-gray-600 italic">No activity yet. Progress will appear here...</div>
          ) : (
            logEntries.map((entry) => (
              <div
                key={entry.id}
                className={cn(
                  'flex gap-3 select-text',
                  entry.type === 'error' && 'text-red-600 dark:text-red-400 bg-red-500/5 dark:bg-red-500/10 px-2 py-0.5 -mx-2 rounded'
                )}
              >
                <span className="text-gray-400 dark:text-gray-600 shrink-0 select-none">
                  {formatTime(entry.timestamp)}
                </span>
                <span className={cn('shrink-0 font-medium select-text', TYPE_STYLES[entry.type])}>
                  {TYPE_PREFIX[entry.type]}
                  {TYPE_PREFIX[entry.type] && ' '}
                </span>
                <span className={cn(
                  'text-gray-700 dark:text-gray-300 break-words whitespace-pre-wrap min-w-0 select-text',
                  TYPE_STYLES[entry.type]
                )}>
                  {entry.message}
                </span>
              </div>
            ))
          )}
        </div>
      ) : replConnected ? (
        <ReplTerminal />
      ) : (
        <div className="flex-1 p-3 font-mono text-[12px] text-gray-500 min-h-0">
          Click "Connect" to start the REPL...
        </div>
      )}
    </div>
  )
}
