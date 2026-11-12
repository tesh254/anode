import * as React from "react"
import { cn } from "@/lib/utils"
import { Terminal, X, Trash2, ChevronUp, ChevronDown } from "lucide-react"

export type LogEntry = {
  id: string
  timestamp: Date
  type: "info" | "warn" | "error" | "ok" | "install" | "flash"
  message: string
}

type ShellLogProps = {
  entries: LogEntry[]
  onClear: () => void
  visible: boolean
  onToggle: () => void
}

const TYPE_STYLES: Record<LogEntry["type"], string> = {
  info: "text-green-600 dark:text-green-500",
  warn: "text-yellow-600 dark:text-yellow-400",
  error: "text-red-600 dark:text-red-400",
  ok: "text-green-600 dark:text-green-400",
  install: "text-blue-600 dark:text-blue-400",
  flash: "text-cyan-600 dark:text-cyan-400",
}

const TYPE_PREFIX: Record<LogEntry["type"], string> = {
  info: "",
  warn: "[WARN]",
  error: "[ERROR]",
  ok: "[OK]",
  install: "[INSTALL]",
  flash: "[FLASH]",
}

export function ShellLog({ entries, onClear, visible, onToggle }: ShellLogProps) {
  const scrollRef = React.useRef<HTMLDivElement>(null)
  const prevLengthRef = React.useRef(entries.length)

  React.useEffect(() => {
    if (entries.length > prevLengthRef.current) {
      const el = scrollRef.current
      if (el) {
        el.scrollTop = el.scrollHeight
      }
    }
    prevLengthRef.current = entries.length
  }, [entries.length])

  const formatTime = (date: Date) => {
    return date.toLocaleTimeString("en-US", {
      hour12: false,
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    })
  }

  if (!visible) {
    return (
      <button
        onClick={onToggle}
        className="absolute bottom-0 left-0 right-0 h-8 border-t border-gray-200 dark:border-white/5 bg-gray-100 dark:bg-[#0f0f0f] flex items-center px-4 cursor-pointer hover:bg-gray-200 dark:hover:bg-white/5 transition-colors z-20"
      >
        <div className="flex items-center gap-2 text-[10px] font-bold text-gray-500 dark:text-gray-500 tracking-widest uppercase">
          <ChevronUp className="size-3.5" />
          Shell Log
          <span className="text-green-600 dark:text-green-500/60 ml-1">({entries.length} entries)</span>
        </div>
      </button>
    )
  }

  return (
    <div className="h-64 max-h-[40vh] min-h-[80px] border-t border-gray-200 dark:border-white/5 bg-gray-50 dark:bg-[#0a0a0a] flex flex-col shrink-0 relative overflow-hidden">
      {/* Header */}
      <div className="flex items-center justify-between px-4 h-10 border-b border-gray-200 dark:border-white/5 bg-gray-100 dark:bg-[#0f0f0f] shrink-0">
        <button
          onClick={onToggle}
          className="flex items-center gap-2 text-green-600 dark:text-green-500 font-semibold text-xs tracking-wider cursor-pointer hover:text-green-700 dark:hover:text-green-400 transition-colors"
        >
          <ChevronDown className="size-4" />
          <Terminal className="size-3.5" />
          SHELL LOG
        </button>
        <div className="flex items-center gap-1">
          <span className="text-[10px] text-gray-500 dark:text-gray-600 mr-2">{entries.length} entries</span>
          <button
            onClick={onClear}
            className="p-1.5 text-gray-500 dark:text-gray-500 hover:text-gray-700 dark:hover:text-gray-300 hover:bg-gray-200 dark:hover:bg-white/5 rounded transition-colors"
            title="Clear log"
          >
            <Trash2 className="size-3.5" />
          </button>
          <button
            onClick={onToggle}
            className="p-1.5 text-gray-500 dark:text-gray-500 hover:text-gray-700 dark:hover:text-gray-300 hover:bg-gray-200 dark:hover:bg-white/5 rounded transition-colors"
            title="Collapse"
          >
            <X className="size-3.5" />
          </button>
        </div>
      </div>

      {/* Log Content - using native scroll with overflow */}
      <div 
        ref={scrollRef}
        className="flex-1 overflow-y-auto overflow-x-hidden p-4 font-mono text-[13px] leading-relaxed bg-white dark:bg-[#050505] min-h-0 select-text"
      >
        {entries.length === 0 ? (
          <div className="text-gray-400 dark:text-gray-600 italic select-text">No activity yet. Progress will appear here...</div>
        ) : (
          entries.map((entry) => (
            <div
              key={entry.id}
              className={cn("flex gap-3 select-text", entry.type === "error" && "text-red-600 dark:text-red-400 bg-red-500/5 dark:bg-red-500/10 px-2 py-0.5 -mx-2 rounded")}
            >
              <span className="text-gray-400 dark:text-gray-600 shrink-0 select-none">
                {formatTime(entry.timestamp)}
              </span>
              <span className={cn("shrink-0 font-medium select-text", TYPE_STYLES[entry.type])}>
                {TYPE_PREFIX[entry.type]}
                {TYPE_PREFIX[entry.type] && " "}
              </span>
              <span className={cn("text-gray-700 dark:text-gray-300 break-words whitespace-pre-wrap min-w-0 select-text", TYPE_STYLES[entry.type])}>
                {entry.message}
              </span>
            </div>
          ))
        )}
      </div>
    </div>
  )
}
