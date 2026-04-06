import { useEffect, useRef, useState } from "react"
import Editor from "@monaco-editor/react"
import { Button } from "@/components/ui/button"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Badge } from "@/components/ui/badge"
import {
  Dialog, DialogContent, DialogHeader, DialogTitle,
  DialogDescription, DialogFooter, DialogClose,
} from "@/components/ui/dialog"
import { ShellLog, type LogEntry } from "@/components/ui/shell-log"
import { Toaster, toast } from "sonner"
import {
  Folder, FileCode2, Play, Settings, Upload, Square, Zap,
  Search, GitBranch, LayoutGrid, Files, ChevronRight, FileText,
  AlertTriangle, Loader2,
} from "lucide-react"

// Types
type PortInfo = {
  path: string
  manufacturer?: string
  serialNumber?: string
  vendorId?: string
  productId?: string
}

type BoardType = 'espressif' | 'raspberry-pi' | 'arduino' | 'unknown'

const BOARD_INFO: Record<Exclude<BoardType, 'unknown'>, {
  label: string
  tool: string
  installHint: string
  description: string
}> = {
  espressif: {
    label: 'Espressif (ESP32 / ESP8266)',
    tool: 'esptool.py',
    installHint: 'pip install esptool',
    description: 'This will use esptool.py to write MicroPython firmware to the flash memory of your ESP board.',
  },
  'raspberry-pi': {
    label: 'Raspberry Pi Pico (RP2040)',
    tool: 'UF2 file copy',
    installHint: 'Hold BOOTSEL while plugging in the board, then select the .uf2 firmware file.',
    description: 'This will copy a .uf2 firmware file to the RPI-RP2 boot volume. Make sure the board is in BOOTSEL mode.',
  },
  arduino: {
    label: 'Arduino',
    tool: 'arduino-cli',
    installHint: 'Install from https://arduino.github.io/arduino-cli/',
    description: 'This will use arduino-cli to upload firmware to your Arduino board.',
  },
}

/** Detect board family from serial port metadata */
function detectBoardType(port: PortInfo | undefined): BoardType {
  if (!port) return 'unknown'
  const vid = port.vendorId?.toLowerCase() ?? ''
  const mfr = port.manufacturer?.toLowerCase() ?? ''
  const pid = port.productId?.toLowerCase() ?? ''

  // Espressif: CP210x (Silicon Labs bridge used on most ESP dev boards) or CH340
  if (vid === '10c4' || vid === '1a86' || mfr.includes('silicon labs') || mfr.includes('espressif')) {
    return 'espressif'
  }
  // Raspberry Pi Pico / RP2040
  if (vid === '2e8a' || mfr.includes('raspberry') || mfr.includes('rp2040') || pid === '0005') {
    return 'raspberry-pi'
  }
  // Arduino
  if (vid === '2341' || vid === '2a03' || mfr.includes('arduino') || mfr.includes('genuino')) {
    return 'arduino'
  }
  return 'unknown'
}

const INITIAL_CODE = `import machine
import time

def blink():
    led = machine.Pin(25, machine.Pin.OUT)
    while True:
        led.value(1)
        time.sleep(0.5)
        led.value(0)
        time.sleep(0.5)
`

export default function App() {
  const [ports, setPorts] = useState<PortInfo[]>([])
  const [selectedPort, setSelectedPort] = useState<string>("")
  const [code, setCode] = useState<string>(INITIAL_CODE)

  // Flash firmware state
  const [flashDialogOpen, setFlashDialogOpen] = useState(false)
  const [detectedBoard, setDetectedBoard] = useState<BoardType>('unknown')
  const [isFlashing, setIsFlashing] = useState(false)
  const [isInstalling, setIsInstalling] = useState(false)

  const [toolMissing, setToolMissing] = useState(false)
  const [toolMissingType, setToolMissingType] = useState<'espressif' | 'arduino' | null>(null)

  // Selected firmware file path
  const [firmwareUrl, setFirmwareUrl] = useState('')

  // Dialog flash log (shown inside the dialog during flashing/install)
  const [dialogFlashLogs, setDialogFlashLogs] = useState<LogEntry[]>([])

  // Shell log state
  const [logEntries, setLogEntries] = useState<LogEntry[]>([])
  const [shellLogVisible, setShellLogVisible] = useState(true)
  const logIdRef = useRef(0)

  // Keep a ref in sync with selectedPort so the interval closure always reads fresh state
  const selectedPortRef = useRef(selectedPort)
  useEffect(() => { selectedPortRef.current = selectedPort }, [selectedPort])

  // Poll serial ports + auto-detect known boards
  useEffect(() => {
    const fetchPorts = async () => {
      const hw = (window as any).hardware
      if (!hw) return
      try {
        const availablePorts = await hw.getSerialPorts()
        setPorts(availablePorts)

        // Auto-select once: only when nothing is selected yet
        if (!selectedPortRef.current) {
          const knownBoard = availablePorts.find((p: PortInfo) => {
            const vid = p.vendorId?.toLowerCase() ?? ''
            const mfr = p.manufacturer?.toLowerCase() ?? ''
            const pid = p.productId?.toLowerCase() ?? ''

            // Espressif: CH340, CP210x, or native Espressif USB
            if (vid === '1a86' || vid === '10c4' || vid === '303a' || mfr.includes('espressif') || mfr.includes('silicon labs')) return true
            // Raspberry Pi Pico / RP2040
            if (vid === '2e8a' || mfr.includes('raspberry') || mfr.includes('rp2040') || pid === '0005') return true
            // Arduino
            if (vid === '2341' || vid === '2a03' || mfr.includes('arduino') || mfr.includes('genuino')) return true

            return false
          })
          if (knownBoard) {
            setSelectedPort(knownBoard.path)
            toast.success(`Auto-detected board on ${knownBoard.path}`)
          }
        }
      } catch (error) {
        console.error("Failed to fetch ports", error)
      }
    }

    fetchPorts()
    const interval = setInterval(fetchPorts, 3000)
    return () => clearInterval(interval)
  }, [])

  const handleRun = () => {
    toast.success("Running code on board...")
  }

  const handleUpload = () => {
    if (!selectedPort) {
      toast.error("Please select a target board first.")
      return
    }
    toast.info("Uploading to board...")
    // Simulate flashing process
    setTimeout(() => {
      toast.success("Firmware successfully uploaded!")
    }, 2000)
  }

  const handleStop = () => {
    toast.warning("Stopped execution.")
  }

  // Flash firmware: detect board type and open confirmation dialog
  const handleFlashFirmware = () => {
    if (!selectedPort) {
      toast.error("Please select a target board first.")
      return
    }
    const port = ports.find(p => p.path === selectedPort)
    const board = detectBoardType(port)
    setDetectedBoard(board)
    setFirmwareUrl('')
    setFlashDialogOpen(true)
  }

  const handleConfirmFlash = async () => {
    if (detectedBoard === 'unknown') return

    const hw = (window as any).hardware
    if (!hw) {
      toast.error("Hardware bridge not available.")
      return
    }

    // Determine which CLI tool is needed for this board
    const toolName = detectedBoard === 'espressif' ? 'esptool.py'
      : detectedBoard === 'arduino' ? 'arduino-cli'
      : null

    if (toolName) {
      const check = await hw.checkTool(toolName)
      if (!check.available) {
        setToolMissing(true)
        setToolMissingType(detectedBoard as 'espressif' | 'arduino')
        toast.error(`${toolName} not found. Click "Install" to install it.`)
        return
      }
    }

    setToolMissing(false)
    setToolMissingType(null)
    setDialogFlashLogs([])
    setIsFlashing(true)
    setShellLogVisible(true)

    try {
      const result = await hw.flashFirmware({
        portPath: selectedPort,
        boardType: detectedBoard,
        firmwareUrl: '',
      })
      if (result.success) {
        setLogEntries(prev => [...prev, {
          id: String(++logIdRef.current),
          timestamp: new Date(),
          type: 'ok',
          message: 'Firmware flashed successfully!',
        }])
        toast.success("Firmware flashed successfully!")
      } else {
        setLogEntries(prev => [...prev, {
          id: String(++logIdRef.current),
          timestamp: new Date(),
          type: 'error',
          message: result.error || 'Flash failed.',
        }])
        toast.error(result.error || "Flash failed.")
        if (result.hint) {
          toast.info(result.hint)
        }
      }
    } catch (error: any) {
      setLogEntries(prev => [...prev, {
        id: String(++logIdRef.current),
        timestamp: new Date(),
        type: 'error',
        message: `Flash failed: ${error.message}`,
      }])
      toast.error(`Flash failed: ${error.message}`)
    } finally {
      setIsFlashing(false)
      setFlashDialogOpen(false)
    }
  }

  const handleInstallTool = async () => {
    if (!toolMissingType) return

    const hw = (window as any).hardware
    if (!hw) return

    setIsInstalling(true)
    setShellLogVisible(true)
    setDialogFlashLogs([])

    const toolName = toolMissingType === 'espressif' ? 'esptool' : 'arduino-cli'
    setLogEntries(prev => [...prev, {
      id: String(++logIdRef.current),
      timestamp: new Date(),
      type: 'info',
      message: `Installing ${toolName}...`,
    }])

    try {
      const result = await hw.installTool(toolMissingType)
      if (result.success) {
        setLogEntries(prev => [...prev, {
          id: String(++logIdRef.current),
          timestamp: new Date(),
          type: 'ok',
          message: `${toolName} installed successfully!`,
        }])
        setToolMissing(false)
        setToolMissingType(null)
        setIsInstalling(false)
        await handleConfirmFlash()
      } else {
        setLogEntries(prev => [...prev, {
          id: String(++logIdRef.current),
          timestamp: new Date(),
          type: 'error',
          message: result.error || 'Installation failed.',
        }])
        if (result.hint) {
          setLogEntries(prev => [...prev, {
            id: String(++logIdRef.current),
            timestamp: new Date(),
            type: 'warn',
            message: result.hint,
          }])
        }
        setIsInstalling(false)
      }
    } catch (error: any) {
      setLogEntries(prev => [...prev, {
        id: String(++logIdRef.current),
        timestamp: new Date(),
        type: 'error',
        message: `Installation failed: ${error.message}`,
      }])
      setIsInstalling(false)
    }
  }

  // Listen for flash and install progress messages from main process
  useEffect(() => {
    const hw = (window as any).hardware
    if (!hw) return

    const flashCleanup = hw.onFlashProgress((_event: unknown, message: string) => {
      const type: LogEntry["type"] = message.startsWith("[ERROR]") ? "error"
        : message.startsWith("[WARN]") ? "warn"
        : message.startsWith("[OK]") ? "ok"
        : "flash"
      const cleanMsg = message.replace(/^\[(ERROR|WARN|OK)\]\s*/, "")
      const entry: LogEntry = {
        id: String(++logIdRef.current),
        timestamp: new Date(),
        type,
        message: cleanMsg,
      }
      setLogEntries(prev => [...prev, entry])
      setDialogFlashLogs(prev => [...prev, entry])
    })

    const installCleanup = hw.onInstallProgress((_event: unknown, data: { tool: string; message: string }) => {
      const type: LogEntry["type"] = data.message.startsWith("[ERROR]") ? "error"
        : data.message.startsWith("[WARN]") ? "warn"
        : data.message.startsWith("[OK]") ? "ok"
        : "install"
      const cleanMsg = data.message.replace(/^\[(ERROR|WARN|OK)\]\s*/, "")
      const entry: LogEntry = {
        id: String(++logIdRef.current),
        timestamp: new Date(),
        type,
        message: cleanMsg,
      }
      setLogEntries(prev => [...prev, entry])
      setDialogFlashLogs(prev => [...prev, entry])
    })

    return () => { flashCleanup(); installCleanup() }
  }, [])

  return (
    <div className="flex h-screen w-full bg-[#0a0a0a] text-gray-300 font-sans overflow-hidden select-none">
      
      {/* Left Activity Bar */}
      <div className="w-14 flex flex-col items-center py-4 border-r border-white/5 bg-[#0a0a0a] shrink-0 gap-6">
        <Files className="size-6 text-green-500 cursor-pointer" />
        <Search className="size-6 text-green-500 opacity-60 hover:opacity-100 cursor-pointer transition-opacity" />
        <GitBranch className="size-6 text-green-500 opacity-60 hover:opacity-100 cursor-pointer transition-opacity" />
        <LayoutGrid className="size-6 text-green-500 opacity-60 hover:opacity-100 cursor-pointer transition-opacity" />
        <div className="mt-auto flex flex-col gap-6 items-center pb-2">
          <Settings className="size-6 text-green-500 opacity-60 hover:opacity-100 cursor-pointer transition-opacity" />
        </div>
      </div>

      {/* Center Area (Toolbar + Editor + Terminal) */}
      <div className="flex-1 flex flex-col min-w-0 border-r border-white/5 relative">
        {/* Top Toolbar */}
        <div className="h-14 flex items-center justify-between px-4 border-b border-white/5 shrink-0 bg-[#0f0f0f]">
          <div className="flex items-center gap-2">
            <Button onClick={handleRun} className="bg-green-600 hover:bg-green-700 text-white gap-2 font-medium h-9 px-4 rounded-md">
              <Play className="size-4" /> Run
            </Button>
            <Button onClick={handleUpload} className="bg-blue-600 hover:bg-blue-700 text-white gap-2 font-medium h-9 px-4 rounded-md">
              <Upload className="size-4" /> Upload
            </Button>
            <Button onClick={handleStop} className="bg-red-500 hover:bg-red-600 text-white gap-2 font-medium h-9 px-4 rounded-md">
              <Square className="size-4 fill-current" /> Stop
            </Button>
          </div>
          <Button variant="ghost" size="icon" className="text-gray-400 hover:text-white hover:bg-white/10 rounded-md">
            <Settings className="size-5" />
          </Button>
        </div>

        {/* Editor Area */}
        <div className="flex-1 relative bg-[#0a0a0a] pt-2">
          <Editor
            height="100%"
            defaultLanguage="python"
            theme="vs-dark"
            value={code}
            onChange={(val) => setCode(val || "")}
            options={{
              minimap: { enabled: false },
              fontSize: 14,
              fontFamily: "'JetBrains Mono', 'Fira Code', 'Menlo', monospace",
              smoothScrolling: true,
              cursorBlinking: "smooth",
              lineNumbersMinChars: 3,
              renderLineHighlight: "none",
              scrollbar: { verticalScrollbarSize: 10, horizontalScrollbarSize: 10 }
            }}
          />
        </div>

        {/* Shell Log (collapsible) */}
        <ShellLog
          entries={logEntries}
          onClear={() => setLogEntries([])}
          visible={shellLogVisible}
          onToggle={() => setShellLogVisible(v => !v)}
        />

        {/* Center Bottom Status Bar */}
        <div className="h-7 border-t border-white/5 bg-[#0a0a0a] flex items-center justify-between px-3 text-[11px] text-gray-400 font-mono tracking-wide shrink-0 z-10">
          <div className="flex items-center gap-4">
            <span className="text-gray-300">connected</span>
            <span className="text-gray-600">|</span>
            <span>115200</span>
          </div>
          <div className="flex items-center gap-4">
            <span className="text-gray-300">main.py</span>
            <span className="text-gray-600">|</span>
            <span>Ln 8, Col 24</span>
            <span className="text-gray-600">|</span>
            <span>UTF-8</span>
          </div>
        </div>
      </div>

      {/* Right Sidebar */}
      <div className="w-80 flex flex-col bg-[#0f0f0f] shrink-0 relative pb-7">
         {/* Right Sidebar Header */}
         <div className="h-14 flex items-center px-4 border-b border-white/5 gap-3 shrink-0">
            <div className="bg-[#dca48c] p-1.5 rounded-sm">
              <Zap className="size-4 text-black fill-current" />
            </div>
            <div className="flex items-center gap-2 text-green-600 font-bold tracking-widest text-[11px] uppercase">
               <Folder className="size-3.5 stroke-2" /> ANODE PROJECT
            </div>
         </div>

         {/* Right Sidebar Content */}
         <ScrollArea className="flex-1">
           <div className="p-4 flex flex-col gap-8">
              
              {/* Project Files */}
              <div>
                 <div className="text-[11px] text-gray-500 text-right mb-3 cursor-pointer hover:text-gray-300 transition-colors">
                    View more templates &rarr;
                 </div>
                 <div className="flex flex-col">
                   <div className="flex items-center gap-2 px-3 py-2 bg-white/5 rounded-sm text-[13px] text-gray-200 cursor-pointer border border-white/5">
                     <FileCode2 className="size-4 text-teal-500" /> main.py
                   </div>
                   <div className="flex items-center gap-2 px-3 py-2 text-[13px] text-gray-400 hover:bg-white/5 rounded-sm cursor-pointer transition-colors">
                     <FileText className="size-4" /> config.py
                   </div>
                 </div>
              </div>

              {/* Board Panel */}
              <div className="bg-white/2 border border-white/5 rounded-md p-4 flex flex-col gap-3">
                <div className="flex items-center gap-2 text-[10px] font-bold text-gray-500 tracking-widest uppercase">
                  <div className="size-2 rounded-full bg-green-500 shadow-[0_0_8px_rgba(34,197,94,0.5)]" /> BOARD
                </div>
                <div className="text-teal-500 font-medium text-base tracking-wide">
                  {selectedPort ? 'ESP32-WROOM-32' : 'No Board Connected'}
                </div>
                
                <Select value={selectedPort} onValueChange={(val) => setSelectedPort(val || "")}>
                  <SelectTrigger className="w-full h-8 text-xs bg-black/20 border-white/10">
                    <SelectValue placeholder="Select COM Port" />
                  </SelectTrigger>
                  <SelectContent className="bg-[#111] border-white/10 text-gray-300">
                    {ports.length === 0 ? (
                      <SelectItem value="none" disabled>No boards detected</SelectItem>
                    ) : (
                      ports.map(port => (
                        <SelectItem key={port.path} value={port.path}>
                          {port.path}
                        </SelectItem>
                      ))
                    )}
                  </SelectContent>
                </Select>

                <div className="flex items-center gap-2 text-xs text-gray-400 font-mono mt-1">
                   COM3 &middot; 115200 baud 
                   <Badge variant="secondary" className="bg-teal-500/20 text-teal-400 hover:bg-teal-500/30 text-[10px] h-5 px-2 font-mono rounded-sm ml-auto">USB</Badge>
                </div>
                <Button
                  onClick={handleFlashFirmware}
                  className="w-full bg-[#77a666] hover:bg-[#648c56] text-black font-semibold mt-1 h-9 rounded-sm shadow-none"
                >
                  Flash Firmware
                </Button>
              </div>

              {/* Templates */}
              <div className="flex flex-col gap-5">
                <div className="text-[10px] font-bold text-gray-500 tracking-widest uppercase">TEMPLATES</div>
                
                <div className="flex flex-col gap-1.5 group cursor-pointer">
                  <div className="text-sm font-medium text-gray-200 group-hover:text-white transition-colors">Blink</div>
                  <div className="text-[13px] text-gray-500">LED blink example</div>
                  <div className="mt-1">
                    <Badge variant="outline" className="text-[10px] text-gray-400 border-white/10 font-normal bg-black/20 rounded-sm">MicroPython</Badge>
                  </div>
                </div>

                <div className="flex flex-col gap-1.5 group cursor-pointer">
                  <div className="text-sm font-medium text-gray-200 group-hover:text-white transition-colors">WiFi Connect</div>
                  <div className="text-[13px] text-gray-500">WiFi connection example</div>
                  <div className="mt-1">
                    <Badge variant="outline" className="text-[10px] text-gray-400 border-white/10 font-normal bg-black/20 rounded-sm">MicroPython</Badge>
                  </div>
                </div>
              </div>
           </div>
         </ScrollArea>

         {/* Bottom Right Explorer Tab (Collapsed) */}
         <div className="absolute bottom-7 left-0 right-0 h-8 border-t border-white/5 bg-[#0f0f0f] flex items-center px-4 cursor-pointer hover:bg-white/5 transition-colors">
            <div className="flex items-center gap-2 text-[10px] font-bold text-gray-500 tracking-widest uppercase">
              <ChevronRight className="size-3.5 rotate-90" /> EXPLORER
            </div>
         </div>
      </div>

      {/* Flash Firmware Confirmation Dialog */}
      <Dialog open={flashDialogOpen} onOpenChange={setFlashDialogOpen}>
        <DialogContent className="bg-[#111] border-white/10 text-gray-200 sm:max-w-md">
          <DialogHeader>
            <DialogTitle className="text-white flex items-center gap-2">
              {toolMissing ? (
                <AlertTriangle className="size-5 text-red-400" />
              ) : (
                <AlertTriangle className="size-5 text-yellow-500" />
              )}
              {toolMissing ? 'Tool Not Found' : 'Flash Firmware'}
            </DialogTitle>
            <DialogDescription className="text-gray-400">
              {toolMissing ? (
                <span>
                  <span className="text-gray-200 font-mono">
                    {detectedBoard === 'espressif' ? 'esptool.py' : 'arduino-cli'}
                  </span>{' '}
                  is required but not installed. Click Install to download and set it up.
                </span>
              ) : detectedBoard === 'unknown' ? (
                <>
                  Could not identify the connected board. Please select the board
                  type manually to continue.
                </>
              ) : (
                <>
                  Detected <span className="text-teal-400 font-medium">{BOARD_INFO[detectedBoard].label}</span> on{' '}
                  <span className="text-gray-200 font-mono">{selectedPort}</span>.
                </>
              )}
            </DialogDescription>
          </DialogHeader>

          {/* Tool missing install panel */}
          {toolMissing && !isInstalling ? (
            <div className="flex flex-col gap-3 py-2">
              <div className="bg-red-500/10 border border-red-500/20 rounded-md p-4 flex flex-col gap-2">
                <div className="text-[13px] text-gray-200 font-medium">
                  {detectedBoard === 'espressif'
                    ? 'esptool.py is needed to flash ESP32/ESP8266 boards.'
                    : 'arduino-cli is needed to upload to Arduino boards.'}
                </div>
                <div className="text-[12px] text-gray-400">
                  {detectedBoard === 'espressif'
                    ? 'This will run: pip3 install esptool'
                    : 'This will download and install arduino-cli to ~/.arduino-cli/bin'}
                </div>
                <div className="flex items-center gap-2 mt-1">
                  <button
                    onClick={async () => {
                      const hw = (window as any).hardware
                      const url = detectedBoard === 'espressif'
                        ? 'https://docs.espressif.com/projects/esptool/en/latest/installation.html'
                        : 'https://arduino.github.io/arduino-cli/latest/installation/'
                      await hw.openUrl(url)
                    }}
                    className="text-[11px] text-blue-400 hover:text-blue-300 underline underline-offset-2"
                  >
                    {detectedBoard === 'espressif' ? 'Download Firmware Tool' : 'Download arduino-cli'}
                  </button>
                </div>
              </div>
            </div>
          ) : (isFlashing || isInstalling) ? (
            /* Live flash/install progress log inside dialog */
            <div className="flex flex-col gap-2 py-2">
              <div className="flex items-center gap-2 text-[11px] font-bold text-gray-500 tracking-widest uppercase">
                {isInstalling ? (
                  <><Loader2 className="size-3.5 animate-spin text-blue-400" /> Installing...</>
                ) : (
                  <><Loader2 className="size-3.5 animate-spin text-cyan-400" /> Flashing...</>
                )}
              </div>
              <div className="bg-black/40 border border-white/5 rounded-md p-3 font-mono text-[12px] text-gray-300 max-h-48 overflow-y-auto leading-relaxed">
                {dialogFlashLogs.length === 0 ? (
                  <span className="text-gray-600 italic">Preparing...</span>
                ) : (
                  dialogFlashLogs.map((entry) => (
                    <div key={entry.id} className="flex gap-2 select-text">
                      <span className="text-gray-600 shrink-0 select-none">
                        {entry.timestamp.toLocaleTimeString('en-US', { hour12: false })}
                      </span>
                      <span className={
                        entry.type === 'error' ? 'text-red-400' :
                        entry.type === 'warn' ? 'text-yellow-400' :
                        entry.type === 'ok' ? 'text-green-400' :
                        entry.type === 'install' ? 'text-blue-400' :
                        'text-cyan-400'
                      }>
                        {entry.message}
                      </span>
                    </div>
                  ))
                )}
              </div>
              <div className="text-[10px] text-gray-600">
                Progress is also logged to the Shell Log below.
              </div>
            </div>
          ) : (
            /* Board type selector (shown when no operation is active) */
            <div className="flex flex-col gap-3 py-2">
              <label className="text-[11px] font-bold text-gray-500 tracking-widest uppercase">
                Board Type
              </label>
              <Select
                value={detectedBoard === 'unknown' ? '' : detectedBoard}
                onValueChange={(val) => { setDetectedBoard(val as BoardType); setToolMissing(false); setToolMissingType(null); setFirmwareUrl('') }}
              >
                <SelectTrigger className="w-full h-9 text-sm bg-black/30 border-white/10">
                  <SelectValue placeholder="Select board type..." />
                </SelectTrigger>
                <SelectContent className="bg-[#111] border-white/10 text-gray-300">
                  <SelectItem value="espressif">Espressif (ESP32 / ESP8266)</SelectItem>
                  <SelectItem value="raspberry-pi">Raspberry Pi Pico (RP2040)</SelectItem>
                  <SelectItem value="arduino">Arduino</SelectItem>
                </SelectContent>
              </Select>

              {/* Info panel for the selected board type */}
              {detectedBoard !== 'unknown' && (
                <div className="bg-white/5 border border-white/5 rounded-md p-3 flex flex-col gap-2 text-[13px]">
                  <p className="text-gray-300">{BOARD_INFO[detectedBoard].description}</p>
                  <div className="flex items-center gap-2 text-gray-500 font-mono text-xs">
                    <span className="text-gray-400">Tool:</span> {BOARD_INFO[detectedBoard].tool}
                  </div>
                  <div className="flex items-center gap-2 text-gray-500 font-mono text-xs">
                    <span className="text-gray-400">Install:</span> {BOARD_INFO[detectedBoard].installHint}
                  </div>
                  {/* Firmware file selector */}
                  <div className="flex flex-col gap-1.5 mt-1">
                    <label className="text-[10px] font-bold text-gray-500 tracking-widest uppercase">
                      Firmware File
                    </label>
                    <div className="flex gap-2">
                      <button
                        onClick={async () => {
                          const hw = (window as any).hardware
                          if (!hw) return
                          const file = await hw.openFirmwareDialog(detectedBoard)
                          if (file) setFirmwareUrl(file)
                        }}
                        className="flex-1 h-8 px-3 text-left text-xs font-mono bg-black/30 border border-white/10 rounded hover:bg-white/5 hover:border-white/20 transition-colors truncate text-gray-300"
                      >
                        {firmwareUrl ? firmwareUrl.split('/').pop() : 'Select .bin or .uf2 file...'}
                      </button>
                      {firmwareUrl && (
                        <span className="text-teal-400 text-xs font-mono self-center max-w-[120px] truncate overflow-hidden">
                          {firmwareUrl.split('/').pop()}
                        </span>
                      )}
                    </div>
                    {!firmwareUrl && (
                      <p className="text-[10px] text-gray-600">
                        Required — select a MicroPython firmware .bin (ESP) or .uf2 (Pico) file
                      </p>
                    )}
                  </div>
                </div>
              )}
            </div>
          )}

          <DialogFooter className="bg-transparent border-t-white/5">
            <DialogClose render={
              <Button variant="ghost" className="text-gray-400 hover:text-white hover:bg-white/10" disabled={isFlashing || isInstalling} />
            }>
              Cancel
            </DialogClose>
            {toolMissing ? (
              <Button
                onClick={handleInstallTool}
                disabled={isInstalling}
                className="bg-blue-600 hover:bg-blue-700 text-white gap-2 font-semibold"
              >
                {isInstalling ? (
                  <><Loader2 className="size-4 animate-spin" /> Installing...</>
                ) : (
                  <>Install Tool</>
                )}
              </Button>
            ) : (
              <Button
                onClick={handleConfirmFlash}
                disabled={detectedBoard === 'unknown' || isFlashing || !firmwareUrl}
                className="bg-[#77a666] hover:bg-[#648c56] text-black font-semibold gap-2 disabled:opacity-40"
              >
                {isFlashing ? (
                  <><Loader2 className="size-4 animate-spin" /> Flashing...</>
                ) : (
                  <>Flash Firmware</>
                )}
              </Button>
            )}
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Toaster position="bottom-right" theme="dark" />
    </div>
  )
}
