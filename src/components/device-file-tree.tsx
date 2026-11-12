import { useState, useCallback, useEffect, useRef } from 'react'
import { HardDrive, Download, Trash2, RefreshCw, Folder, FileCode2, ChevronRight, ChevronDown } from 'lucide-react'
import { useStore, type DeviceFileEntry } from '@/lib/store'
import { cn } from '@/lib/utils'

interface DeviceFileTreeProps {
  className?: string
}

interface DeviceNode {
  name: string
  path: string
  isDirectory: boolean
  size: number
  children?: DeviceNode[]
  expanded?: boolean
}

export function DeviceFileTree({ className }: DeviceFileTreeProps) {
  const {
    selectedPort,
    deviceFilesLoading,
    deviceConnected,
    replConnected,
    replConnecting,
    isFlashing,
    isInstalling,
    setDeviceFiles,
    setDeviceFilesLoading,
    setDeviceConnected,
    addLogEntry,
  } = useStore()

  const [tree, setTree] = useState<DeviceNode[]>([])
  const [expandedDirs, setExpandedDirs] = useState<Set<string>>(new Set(['/']))
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; node: DeviceNode | null } | null>(null)
  const lastErrorRef = useRef<string>('')
  const debounceTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  const getHardware = useCallback(() => {
    return (window as unknown as {
      hardware?: {
        deviceFsLs?: (port: string, remote: string) => Promise<{ success: boolean; entries: DeviceFileEntry[]; error?: string }>
        deviceFsRm?: (port: string, remote: string) => Promise<{ success: boolean; error?: string }>
        deviceFsCat?: (port: string, remote: string) => Promise<{ success: boolean; content: string; error?: string }>
        deviceFsPut?: (port: string, local: string, remote: string) => Promise<{ success: boolean; error?: string }>
      }
    }).hardware
  }, [])

  const refreshDeviceFiles = useCallback(async () => {
    if (!selectedPort) {
      setDeviceFiles([])
      setDeviceConnected(false)
      return
    }
    if (replConnected || replConnecting) {
      setDeviceConnected(false)
      return
    }
    // esptool/installers need exclusive port access — never let the device-tree
    // mpremote probe grab the port mid-flash (the classic "port is busy" race).
    if (isFlashing || isInstalling) {
      setDeviceConnected(false)
      return
    }
    const hw = getHardware()
    if (!hw?.deviceFsLs) return

    setDeviceFilesLoading(true)
    try {
      const result = await hw.deviceFsLs(selectedPort, ':')
      if (result.success) {
        setDeviceFiles(result.entries)
        setDeviceConnected(true)
        const rootNodes = buildTree(result.entries)
        setTree(rootNodes)
        lastErrorRef.current = ''
      } else {
        setDeviceConnected(false)
        // Clean up mpremote traceback into a short message
        const cleanError = cleanMpremoteError(result.error || 'cannot list')
        if (cleanError !== lastErrorRef.current) {
          lastErrorRef.current = cleanError
          addLogEntry({ type: 'warn', message: `Device FS: ${cleanError}` })
        }
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e)
      const cleanError = cleanMpremoteError(msg)
      if (cleanError !== lastErrorRef.current) {
        lastErrorRef.current = cleanError
        addLogEntry({ type: 'error', message: `Device FS error: ${cleanError}` })
      }
      setDeviceConnected(false)
    } finally {
      setDeviceFilesLoading(false)
    }
  }, [selectedPort, replConnected, replConnecting, isFlashing, isInstalling, getHardware, setDeviceFiles, setDeviceFilesLoading, setDeviceConnected, addLogEntry])

  // Debounced auto-refresh — wait 1.5s after port change before trying mpremote
  // (gives MicroPython time to boot after flash/reset)
  useEffect(() => {
    if (debounceTimerRef.current) {
      clearTimeout(debounceTimerRef.current)
    }
    debounceTimerRef.current = setTimeout(() => {
      refreshDeviceFiles()
    }, 1500)
    return () => {
      if (debounceTimerRef.current) {
        clearTimeout(debounceTimerRef.current)
      }
    }
  }, [refreshDeviceFiles])

  useEffect(() => {
    const handleClick = () => setContextMenu(null)
    window.addEventListener('click', handleClick)
    return () => window.removeEventListener('click', handleClick)
  }, [])

  const handleToggleDir = (node: DeviceNode) => {
    setExpandedDirs((prev) => {
      const next = new Set(prev)
      if (next.has(node.path)) {
        next.delete(node.path)
      } else {
        next.add(node.path)
      }
      return next
    })
  }

  const handleDeleteFile = useCallback(async (node: DeviceNode) => {
    if (!selectedPort) return
    const hw = getHardware()
    if (!hw?.deviceFsRm) return
    addLogEntry({ type: 'info', message: `Deleting ${node.path} from device...` })
    const result = await hw.deviceFsRm(selectedPort, node.path)
    if (result.success) {
      addLogEntry({ type: 'ok', message: `Deleted ${node.path}` })
      refreshDeviceFiles()
    } else {
      addLogEntry({ type: 'error', message: result.error || 'Delete failed' })
    }
  }, [selectedPort, getHardware, addLogEntry, refreshDeviceFiles])

  const handleDownloadFile = useCallback(async (node: DeviceNode) => {
    if (!selectedPort || node.isDirectory) return
    const hw = getHardware()
    if (!hw?.deviceFsCat) return
    addLogEntry({ type: 'info', message: `Downloading ${node.path} from device...` })
    const result = await hw.deviceFsCat(selectedPort, node.path)
    if (result.success) {
      addLogEntry({ type: 'ok', message: `Downloaded ${node.path} (${result.content.length} bytes)` })
    } else {
      addLogEntry({ type: 'error', message: result.error || 'Download failed' })
    }
  }, [selectedPort, getHardware, addLogEntry])

  const handleContextMenu = (e: React.MouseEvent, node: DeviceNode) => {
    e.preventDefault()
    e.stopPropagation()
    setContextMenu({ x: e.clientX, y: e.clientY, node })
  }

  const renderNode = (node: DeviceNode, depth: number = 0): React.ReactNode => {
    const isExpanded = expandedDirs.has(node.path)
    return (
      <div key={node.path}>
        <div
          className={cn(
            'flex items-center gap-1.5 px-2 py-1 text-[12px] cursor-pointer hover:bg-gray-200 dark:hover:bg-white/5 rounded-sm transition-colors',
          )}
          style={{ paddingLeft: `${depth * 12 + 8}px` }}
          onClick={() => node.isDirectory ? handleToggleDir(node) : undefined}
          onContextMenu={(e) => handleContextMenu(e, node)}
        >
          {node.isDirectory ? (
            <>
              {isExpanded ? <ChevronDown className="size-3 shrink-0" /> : <ChevronRight className="size-3 shrink-0" />}
              <Folder className="size-3.5 shrink-0 text-blue-500" />
            </>
          ) : (
            <>
              <span className="size-3 shrink-0" />
              <FileCode2 className="size-3.5 shrink-0 text-teal-500" />
            </>
          )}
          <span className="truncate flex-1">{node.name}</span>
        </div>
        {node.isDirectory && isExpanded && node.children?.map((child) => renderNode(child, depth + 1))}
      </div>
    )
  }

  return (
    <div className={cn('flex flex-col bg-gray-50 dark:bg-[#0a0a0a] border-t border-gray-200 dark:border-white/5', className)}>
      <div className="flex items-center justify-between px-3 py-2 border-b border-gray-200 dark:border-white/5 bg-gray-100 dark:bg-[#0f0f0f] shrink-0">
        <div className="flex items-center gap-2 text-[10px] font-bold text-gray-500 dark:text-gray-500 tracking-widest uppercase">
          <HardDrive className="size-3.5" />
          DEVICE
          {deviceConnected && <span className="size-1.5 rounded-full bg-green-500" />}
        </div>
        <button
          onClick={refreshDeviceFiles}
          disabled={!selectedPort || deviceFilesLoading || replConnected || replConnecting || isFlashing || isInstalling}
          className="p-1 hover:bg-gray-200 dark:hover:bg-white/10 rounded transition-colors disabled:opacity-40"
          title={replConnected ? 'Disconnect REPL to refresh device files' : isFlashing || isInstalling ? 'Busy — flashing/installing' : 'Refresh device files'}
        >
          <RefreshCw className={cn('size-3.5', deviceFilesLoading && 'animate-spin')} />
        </button>
      </div>
      <div className="flex-1 overflow-y-auto py-1 min-h-0">
        {!selectedPort ? (
          <div className="text-[11px] text-gray-500 dark:text-gray-600 italic px-3 py-2">No port selected</div>
        ) : deviceFilesLoading ? (
          <div className="text-[11px] text-gray-500 dark:text-gray-600 italic px-3 py-2">Loading device files...</div>
        ) : tree.length === 0 ? (
          <div className="text-[11px] text-gray-500 dark:text-gray-600 italic px-3 py-2">
            {replConnected ? 'Disconnect REPL to browse files' : deviceConnected ? 'No files on device' : 'Not connected to device'}
          </div>
        ) : (
          tree.map((node) => renderNode(node))
        )}
      </div>
      {contextMenu && contextMenu.node && (
        <div
          className="fixed bg-[#1a1a1a] border border-white/10 rounded-md shadow-lg py-1 z-50 min-w-[160px]"
          style={{ left: contextMenu.x, top: contextMenu.y }}
          onClick={(e) => e.stopPropagation()}
        >
          <button
            onClick={() => { if (contextMenu.node) handleDownloadFile(contextMenu.node); setContextMenu(null) }}
            disabled={contextMenu.node.isDirectory}
            className="w-full px-3 py-1.5 text-sm text-gray-300 hover:bg-white/10 flex items-center gap-2 disabled:opacity-40"
          >
            <Download className="size-3.5" /> Download
          </button>
          <button
            onClick={() => { if (contextMenu.node) handleDeleteFile(contextMenu.node); setContextMenu(null) }}
            className="w-full px-3 py-1.5 text-sm text-red-400 hover:bg-white/10 flex items-center gap-2"
          >
            <Trash2 className="size-3.5" /> Delete
          </button>
        </div>
      )}
    </div>
  )
}

function buildTree(entries: DeviceFileEntry[]): DeviceNode[] {
  const root: DeviceNode = { name: '/', path: '/', isDirectory: true, size: 0, children: [] }
  for (const entry of entries) {
    const cleanPath = entry.path.replace(/^:?\//, '/')
    const parts = cleanPath.split('/').filter(Boolean)
    let current = root
    for (let i = 0; i < parts.length; i++) {
      const part = parts[i]
      const isLast = i === parts.length - 1
      const childPath = '/' + parts.slice(0, i + 1).join('/')
      if (!current.children) current.children = []
      let child = current.children.find((c) => c.name === part)
      if (!child) {
        child = {
          name: part,
          path: childPath,
          isDirectory: isLast ? entry.isDirectory : true,
          size: isLast ? entry.size : 0,
          children: isLast && !entry.isDirectory ? undefined : [],
        }
        current.children.push(child)
      }
      current = child
    }
  }
  return root.children ?? []
}

function cleanMpremoteError(error: string): string {
  // Extract the last meaningful line from an mpremote traceback
  if (error.includes('could not enter raw repl')) {
    return 'could not enter raw REPL (board may be booting or in use)'
  }
  if (error.includes('it may be in use by another program')) {
    return 'serial port in use (disconnect REPL first)'
  }
  if (error.includes('No module named mpremote')) {
    return 'mpremote not installed (pip install mpremote)'
  }
  if (error.includes('Device not configured')) {
    return 'device disconnected during operation'
  }
  // Strip traceback, keep last line
  const lines = error.trim().split('\n').filter(Boolean)
  const lastLine = lines[lines.length - 1] ?? error
  if (lastLine.startsWith('mpremote.transport.')) {
    return lastLine.replace(/^mpremote\.\w+\./, '').replace(/^TransportError:\s*/, '')
  }
  return lastLine.length > 120 ? lastLine.slice(0, 120) + '...' : lastLine
}
