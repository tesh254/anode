import { useEffect, useRef, useState, useCallback } from "react"
import Editor, { OnMount } from "@monaco-editor/react"
import { Button } from "@/components/ui/button"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Badge } from "@/components/ui/badge"
import {
  Dialog, DialogContent, DialogHeader, DialogTitle,
  DialogDescription, DialogFooter, DialogClose,
} from "@/components/ui/dialog"
import { BottomPanel } from "@/components/bottom-panel"
import { WelcomeScreen } from "@/components/welcome-screen"
import { CommandPalette } from "@/components/command-palette"
import { DeviceFileTree } from "@/components/device-file-tree"
import { EditorTabs } from "@/components/editor-tabs"
import { Toaster, toast } from "sonner"
import { useStore, detectBoardType, getPreferredPort, isLikelyHardwarePort, detectExecutionProfiles, selectBestProfile, resolveProfileOnDetect, circuitPythonHint, type BoardType, type OpenTab } from "@/lib/store"
import { useAutoConnect } from "@/lib/use-auto-connect"
import { identifyBoard, portKey } from "@/lib/boards"
import { getWorkspaceLayout } from "@/types/interpreter"
import { Template } from "@/lib/templates"
import {
  FileCode2, Play, Settings, Upload, Square,
  Search, Files, Plus, RotateCw, Terminal,
  AlertTriangle, Loader2, Save, FolderPlus, FilePlus, Cpu, Zap, Hammer,
} from "lucide-react"

// Types
interface PortInfo {
  path: string
  manufacturer?: string
  serialNumber?: string
  vendorId?: string
  productId?: string
}

interface FileEntry {
  name: string
  path: string
  isDirectory: boolean
  isFile: boolean
}

interface HardwareApi {
  getSerialPorts: () => Promise<PortInfo[]>
  detectCircuitPython?: () => Promise<{ present: boolean; mountPath?: string; boardName?: string; version?: string }>
  readDirectory: (path: string) => Promise<FileEntry[]>
  readFile: (path: string) => Promise<{ content: string; language: 'python' | 'arduino' }>
  writeFile: (path: string, content: string) => Promise<{ success: boolean; error?: string }>
  createFile: (dirPath: string, fileName: string, content: string) => Promise<{ success: boolean; path?: string }>
  createDirectory: (dirPath: string) => Promise<{ success: boolean }>
  runCode: (opts: { portPath: string; code: string }) => Promise<{ success: boolean; error?: string; viaRepl?: boolean }>
  uploadCode: (opts: { portPath: string; fileName: string; code: string }) => Promise<{ success: boolean; error?: string }>
  circuitPythonDeploy?: (opts: { mountPath: string; fileName: string; code: string }) => Promise<{ success: boolean; error?: string; path?: string }>
  stopCode: (opts: { portPath: string }) => Promise<{ success: boolean; error?: string }>
  flashFirmware: (opts: { portPath: string; boardType: string; firmwareUrl: string; vendorId?: string; productId?: string }) => Promise<{ success: boolean; error?: string; hint?: string }>
  checkTool: (tool: string) => Promise<{ available: boolean }>
  installTool: (tool: 'espressif' | 'arduino') => Promise<{ success: boolean; error?: string; hint?: string }>
  openFolder?: () => Promise<string | null>
  replConnect?: (portPath: string) => Promise<{ success: boolean; error?: string }>
  replDisconnect?: () => Promise<{ success: boolean }>
  saveReplExport?: (content: string, defaultName?: string) => Promise<{ success: boolean; canceled?: boolean; error?: string; filePath?: string }>
  deviceFsLs?: (portPath: string, remotePath: string) => Promise<{ success: boolean; entries: { name: string; path: string; isDirectory: boolean; size: number }[]; error?: string }>
  deviceFsCat?: (portPath: string, remotePath: string) => Promise<{ success: boolean; content: string; error?: string }>
  deviceFsPut?: (portPath: string, localFilePath: string, remotePath: string) => Promise<{ success: boolean; error?: string }>
  deviceFsRm?: (portPath: string, remotePath: string) => Promise<{ success: boolean; error?: string }>
  arduinoCompile?: (opts: { sketchPath: string; fqbn: string }) => Promise<{ success: boolean; error?: string; stdout?: string }>
  arduinoUpload?: (opts: { portPath: string; sketchPath: string; fqbn: string }) => Promise<{ success: boolean; error?: string; stdout?: string }>
  onFlashProgress?: (cb: (_event: unknown, message: string) => void) => () => void
  onInstallProgress?: (cb: (_event: unknown, data: { tool: string; message: string }) => void) => () => void
}

const BOARD_INFO: Record<string, {
  label: string
  tool: string
  installHint: string
  description: string
}> = {
  espressif: {
    label: 'Espressif (ESP32 / ESP8266)',
    tool: 'esptool.py',
    installHint: 'python3 -m pip install esptool',
    description: 'This will use esptool.py to write MicroPython firmware to the flash memory of your ESP board.',
  },
  'm5stack-atom': {
    label: 'M5Stack Atom Light (ESP32 PICO)',
    tool: 'esptool.py',
    installHint: 'python3 -m pip install esptool',
    description: 'This will flash MicroPython ESP32_GENERIC firmware to your M5Stack Atom Light using esptool.py.',
  },
  'm5stamp-s3': {
    label: 'M5StampS3 (ESP32-S3)',
    tool: 'esptool.py',
    installHint: 'python3 -m pip install esptool',
    description: 'This will flash MicroPython ESP32_GENERIC_S3 firmware to your M5StampS3 using esptool.py.',
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
  }
}

function getPortSelectionError(port: PortInfo | undefined): string | null {
  if (!port) return 'No board port selected.'
  if (isLikelyHardwarePort(port)) return null
  return `Selected port ${port.path} looks like macOS system/Bluetooth port, not microcontroller. Choose /dev/cu.usbmodem* or /dev/cu.usbserial* port instead.`
}

interface ContextMenuState {
  visible: boolean
  x: number
  y: number
  type: 'file' | 'directory' | 'editor'
  targetPath?: string
}

export default function App() {
  const {
    projectPath, projectFiles, currentFile, isDirty,
    editorState, ports, selectedPort, detectedBoard,
    availableProfiles, activeProfile, workspaceMode, circuitPythonDrive,
    openTabs,
    flashDialogOpen, isFlashing, isInstalling, toolMissing, toolMissingType,
    logEntries, dialogFlashLogs,
    replConnected, bottomMode,
    connectionDropped,
    welcomeScreenVisible, settingsOpen,
    setProjectPath, setProjectFiles, setCurrentFile, updateFileContent, setIsDirty,
    setEditorState, setPorts, setSelectedPort, setDetectedBoard,
    setAvailableProfiles, setActiveProfile, selectProfile, setCircuitPythonDrive,
    openTab, closeTab, setActiveTab, markTabDirty,
    boardOverrides, setBoardOverride,
    setLastPortPath, setConnectionDropped,
    setFlashDialogOpen, setIsFlashing, setIsInstalling, setToolMissing,
    addLogEntry, clearLogEntries, clearDialogFlashLogs,
    setReplConnected, setBottomMode,
    setWelcomeScreenVisible, setSettingsOpen, addRecentProject,
  } = useStore()

  const editorRef = useRef<Parameters<OnMount>[0] | null>(null)
  const monacoRef = useRef<Parameters<OnMount>[1] | null>(null)
  const selectedPortRef = useRef(selectedPort)
  const [contextMenu, setContextMenu] = useState<ContextMenuState>({ visible: false, x: 0, y: 0, type: 'editor' })
  const [commandPaletteOpen, setCommandPaletteOpen] = useState(false)
  const [createDialogOpen, setCreateDialogOpen] = useState<'file' | 'folder' | null>(null)
  const [newItemName, setNewItemName] = useState('')
  const selectedPortInfo = ports.find((port) => port.path === selectedPort)
  // Resolve the friendly board (saved override → auto-detect → chip family).
  const boardMatch = identifyBoard(selectedPortInfo, boardOverrides)
  const board = boardMatch.board
  const boardFlashType = board?.flashBoardType
  // Friendly name when the port is resolved; fall back to the legacy family
  // label while the port list is still loading or only the kind is known.
  const boardLabel = board?.name ?? (detectedBoard !== 'unknown' ? BOARD_INFO[detectedBoard]?.label : undefined)
  const deployableFiles = projectFiles.filter((file) => file.language === 'python' || file.language === 'arduino')
  const workspaceLayout = getWorkspaceLayout(workspaceMode)
  const { attemptAutoConnect, checkConnectionDrop } = useAutoConnect()

  useEffect(() => { selectedPortRef.current = selectedPort }, [selectedPort])

  // Keep the legacy flash/upload board kind in sync with the resolved board so
  // a manual board choice (override) drives which firmware gets flashed.
  useEffect(() => {
    if (boardFlashType && boardFlashType !== detectedBoard) setDetectedBoard(boardFlashType)
  }, [boardFlashType, detectedBoard, setDetectedBoard])

  const getHardware = useCallback(() => {
    return (window as unknown as { hardware?: HardwareApi }).hardware
  }, [])

  // Cmd+K for command palette
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault()
        setCommandPaletteOpen(true)
      }
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [])

  const refreshPorts = useCallback(async () => {
    const hw = getHardware()
    if (!hw) return
    try {
      const availablePorts = await hw.getSerialPorts()
      setPorts(availablePorts)

      // Probe for a mounted CIRCUITPY drive so detection can route CircuitPython
      // boards to the right runtime (they look identical to MicroPython over USB).
      const cpDrive = (await hw.detectCircuitPython?.()) ?? null
      setCircuitPythonDrive(cpDrive)

      checkConnectionDrop(availablePorts)

      if (!selectedPortRef.current && availablePorts.length > 0) {
        attemptAutoConnect(availablePorts)
      }

      if (selectedPortRef.current) {
        const currentPort = availablePorts.find((p) => p.path === selectedPortRef.current)
        if (currentPort) {
          setDetectedBoard(detectBoardType(currentPort))
          const profiles = detectExecutionProfiles({ ...currentPort, circuitPython: circuitPythonHint() })
          setAvailableProfiles(profiles)
          // Keep the runtime/workspace in step with what's actually on this port
          // (board swaps, a CIRCUITPY drive mounting, fresh-launch restore). See
          // resolveProfileOnDetect for the rules.
          const current = useStore.getState().activeProfile
          const next = resolveProfileOnDetect({
            current,
            profiles,
            savedId: useStore.getState().lastProfileId,
            manual: useStore.getState().profileManuallySet,
          })
          if (next && next.id !== current?.id) setActiveProfile(next)
          if (connectionDropped) {
            addLogEntry({ type: 'ok', message: `Reconnected to ${currentPort.path}` })
            setConnectionDropped(false)
          }
        } else {
          setDetectedBoard('unknown')
          const fallback = getPreferredPort(availablePorts)
          if (fallback) {
            setSelectedPort(fallback.path)
            setLastPortPath(fallback.path)
            setDetectedBoard(detectBoardType(fallback))
            const profiles = detectExecutionProfiles({ ...fallback, circuitPython: circuitPythonHint() })
            setAvailableProfiles(profiles)
            const best = selectBestProfile(profiles)
            if (best) setActiveProfile(best)
          } else {
            setSelectedPort('')
          }
        }
      }
    } catch (error) {
      console.error("Failed to fetch ports", error)
    }
  }, [getHardware, setDetectedBoard, setPorts, setSelectedPort, setAvailableProfiles, setActiveProfile, setLastPortPath, setConnectionDropped, addLogEntry, attemptAutoConnect, checkConnectionDrop, connectionDropped])

  // Poll serial ports and auto-detect board
  useEffect(() => {
    refreshPorts()
    const interval = setInterval(refreshPorts, 3000)
    return () => clearInterval(interval)
  }, [refreshPorts])

  // Listen for flash/install progress
  useEffect(() => {
    const hw = getHardware()
    if (!hw) return

    const flashCleanup = hw.onFlashProgress?.((_event: unknown, message: string) => {
      const type = message.startsWith("[ERROR]") ? "error"
        : message.startsWith("[WARN]") ? "warn"
        : message.startsWith("[OK]") ? "ok"
        : "flash"
      const cleanMsg = message.replace(/^\[(ERROR|WARN|OK)\]\s*/, "")
      addLogEntry({ type, message: cleanMsg })
    })

    const installCleanup = hw.onInstallProgress?.((_event: unknown, data: { tool: string; message: string }) => {
      const type = data.message.startsWith("[ERROR]") ? "error"
        : data.message.startsWith("[WARN]") ? "warn"
        : data.message.startsWith("[OK]") ? "ok"
        : "install"
      const cleanMsg = data.message.replace(/^\[(ERROR|WARN|OK)\]\s*/, "")
      addLogEntry({ type, message: cleanMsg })
    })

    return () => { flashCleanup?.(); installCleanup?.() }
  }, [addLogEntry])

  // Close context menu on click outside
  useEffect(() => {
    const handleClick = () => setContextMenu(prev => ({ ...prev, visible: false }))
    window.addEventListener('click', handleClick)
    return () => window.removeEventListener('click', handleClick)
  }, [])

  const handleEditorMount: OnMount = (editor, monaco) => {
    editorRef.current = editor
    monacoRef.current = monaco
    editor.onDidChangeCursorPosition((e) => {
      setEditorState({ line: e.position.lineNumber, column: e.position.column })
    })
  }

  const handleOpenProject = async (path: string) => {
    const hw = getHardware()
    if (!hw) return

    try {
      const entries = await hw.readDirectory(path)
      const files: typeof projectFiles = []
      
      for (const entry of entries) {
        if (entry.isFile && (entry.name.endsWith('.py') || entry.name.endsWith('.ino'))) {
          const result = await hw.readFile(entry.path)
          if (result.content) {
            files.push({
              name: entry.name,
              path: entry.path,
              content: result.content,
              language: result.language,
            })
          }
        }
      }

      setProjectPath(path)
      setProjectFiles(files)
      
      const projectName = path.split('/').pop() || 'Project'
      addRecentProject(path, projectName)
      
      if (files.length > 0) {
        setCurrentFile(files[0])
      }
      
      setWelcomeScreenVisible(false)
      toast.success(`Opened ${projectName}`)
    } catch (error) {
      toast.error('Failed to open project folder')
      console.error(error)
    }
  }

  const handleCreateFromTemplate = async (template: Template, projectName: string, projectPath: string) => {
    const hw = (window as unknown as { 
      hardware: { 
        createFile: (dir: string, name: string, content: string) => Promise<{ success: boolean; path?: string }>
      } 
    }).hardware
    if (!hw) return

    try {
      const mainFileName = template.language === 'arduino' ? 'sketch.ino' : 'main.py'
      await hw.createFile(projectPath, mainFileName, template.code)
      
      setProjectPath(projectPath)
      setProjectFiles([{
        name: mainFileName,
        path: `${projectPath}/${mainFileName}`,
        content: template.code,
        language: template.language,
      }])
      setCurrentFile({
        name: mainFileName,
        path: `${projectPath}/${mainFileName}`,
        content: template.code,
        language: template.language,
      })
      
      addRecentProject(projectPath, projectName)
      setWelcomeScreenVisible(false)
      toast.success(`Created ${projectName}`)
    } catch (error) {
      toast.error('Failed to create project')
      console.error(error)
    }
  }

  const saveFileToDisk = useCallback(async (
    file: typeof projectFiles[number],
    options?: { silent?: boolean; toastLabel?: string },
  ) => {
    const hw = getHardware()
    if (!hw) return false

    try {
      const result = await hw.writeFile(file.path, file.content)
      if (!result.success) {
        const message = result.error || `Failed to save ${file.name}`
        addLogEntry({ type: 'error', message })
        toast.error(message)
        return false
      }

      if (!options?.silent) {
        toast.success(options?.toastLabel || `${file.name} saved`)
      }
      return true
    } catch (error) {
      const message = error instanceof Error ? error.message : `Failed to save ${file.name}`
      addLogEntry({ type: 'error', message })
      toast.error(message)
      console.error(error)
      return false
    }
  }, [addLogEntry, getHardware, projectFiles])

  const handleSaveFile = useCallback(async () => {
    if (!currentFile) return
    const saved = await saveFileToDisk(currentFile, { toastLabel: 'File saved' })
    if (saved) {
      setIsDirty(false)
    }
  }, [currentFile, saveFileToDisk, setIsDirty])

  const handleTabSelect = useCallback((tab: OpenTab) => {
    setActiveTab(tab.path)
    const file = projectFiles.find((f) => f.path === tab.path)
    if (file) {
      setCurrentFile(file)
    }
  }, [projectFiles, setActiveTab, setCurrentFile])

  const handleTabClose = useCallback((tab: OpenTab) => {
    if (tab.isDirty) {
      const shouldSave = window.confirm(`Save changes to ${tab.name}?`)
      if (shouldSave) {
        const file = projectFiles.find((f) => f.path === tab.path)
        if (file) void saveFileToDisk(file, { silent: false })
      }
    }
    closeTab(tab.path)
    const remaining = openTabs.filter((t) => t.path !== tab.path)
    if (remaining.length > 0) {
      const next = remaining[remaining.length - 1]
      const file = projectFiles.find((f) => f.path === next.path)
      if (file) setCurrentFile(file)
    } else {
      setCurrentFile(null)
    }
  }, [closeTab, openTabs, projectFiles, saveFileToDisk, setCurrentFile])

  const handleSaveAllFiles = useCallback(async (options?: { silent?: boolean }) => {
    if (projectFiles.length === 0) return false
    addLogEntry({ type: 'info', message: `Saving ${projectFiles.length} project file${projectFiles.length === 1 ? '' : 's'} locally...` })

    for (const file of projectFiles) {
      const saved = await saveFileToDisk(file, { silent: true })
      if (!saved) {
        return false
      }
    }

    setIsDirty(false)
    addLogEntry({ type: 'ok', message: `Saved ${projectFiles.length} file${projectFiles.length === 1 ? '' : 's'} locally.` })
    if (!options?.silent) {
      toast.success(`Saved ${projectFiles.length} file${projectFiles.length === 1 ? '' : 's'}`)
    }
    return true
  }, [addLogEntry, projectFiles, saveFileToDisk, setIsDirty])

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 's') {
        e.preventDefault()
        handleSaveFile()
      }
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [handleSaveFile])

  // Copy files onto the mounted CIRCUITPY drive. This is CircuitPython's native
  // deploy/run path — writing a file triggers the board's auto-reload, which
  // re-runs code.py; output is read over the serial REPL. No serial port handoff.
  const deployToCircuitPython = async (files: { name: string; content: string }[]): Promise<boolean> => {
    const hw = getHardware()
    const mountPath = circuitPythonDrive?.mountPath
    if (!hw?.circuitPythonDeploy || !mountPath) {
      addLogEntry({ type: 'error', message: 'CIRCUITPY drive not mounted — connect the board (it should appear as a USB drive).' })
      toast.error('No CIRCUITPY drive detected')
      return false
    }
    for (const file of files) {
      addLogEntry({ type: 'flash', message: `Copying ${file.name} to CIRCUITPY...` })
      const result = await hw.circuitPythonDeploy({ mountPath, fileName: file.name, code: file.content })
      if (!result.success) {
        addLogEntry({ type: 'error', message: result.error || `Copy failed for ${file.name}` })
        toast.error(result.error || `Copy failed for ${file.name}`)
        return false
      }
    }
    return true
  }

  const handleRun = async () => {
    if (!currentFile) {
      toast.error('No file open')
      return
    }
    const hw = getHardware()
    if (!hw) return

    // CircuitPython: deploy to the drive instead of running over serial. The
    // board auto-reloads and runs code.py; output streams to the serial REPL.
    if (workspaceMode === 'circuitpython') {
      const saved = await saveFileToDisk(currentFile, { silent: true })
      if (!saved) return
      setBottomMode(replConnected ? 'repl' : 'shell')
      addLogEntry({ type: 'info', message: `Deploying ${currentFile.name} to CIRCUITPY (auto-runs on save)...` })
      const ok = await deployToCircuitPython([{ name: currentFile.name, content: currentFile.content }])
      if (ok) {
        setIsDirty(false)
        if (!/^(code|main)\.py$/i.test(currentFile.name)) {
          addLogEntry({ type: 'warn', message: `CircuitPython only auto-runs code.py / main.py — rename ${currentFile.name} to run it directly.` })
        }
        addLogEntry({ type: 'ok', message: `${currentFile.name} copied; board auto-reloaded. Connect the REPL to see output.` })
        toast.success('Deployed to CircuitPython')
      }
      return
    }

    if (!selectedPort) {
      toast.error('No board connected')
      return
    }

    const saved = await saveFileToDisk(currentFile, { silent: true })
    if (!saved) return

    setBottomMode(replConnected ? 'repl' : 'shell')
    addLogEntry({ type: 'info', message: `Running ${currentFile.name} on ${selectedPort}...` })

    try {
      const result = await hw.runCode({ portPath: selectedPort, code: currentFile.content })
      // If the main process ran the code through the live REPL, output streams
      // into the REPL panel — surface that view regardless of renderer state.
      if (result.viaRepl) setBottomMode('repl')
      if (result.success) {
        setIsDirty(false)
        addLogEntry({ type: 'ok', message: `${currentFile.name} executed on board.` })
        toast.success('Code executed')
      } else {
        addLogEntry({ type: 'error', message: result.error || 'Run failed' })
        toast.error(result.error || 'Run failed')
      }
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error)
      addLogEntry({ type: 'error', message: `Run failed: ${msg}` })
      toast.error('Run failed')
    }
  }

  const handleUpload = async () => {
    if (!currentFile || deployableFiles.length === 0) {
      toast.error('No file open')
      return
    }
    const hw = getHardware()
    if (!hw) return

    // CircuitPython: copy the project files onto the CIRCUITPY drive.
    if (workspaceMode === 'circuitpython') {
      const saved = await handleSaveAllFiles({ silent: true })
      if (!saved) return
      setBottomMode('shell')
      addLogEntry({ type: 'info', message: `Copying ${deployableFiles.length} file${deployableFiles.length === 1 ? '' : 's'} to CIRCUITPY...` })
      const ok = await deployToCircuitPython(deployableFiles.map((f) => ({ name: f.name, content: f.content })))
      if (ok) {
        setIsDirty(false)
        addLogEntry({ type: 'ok', message: `Deployment complete. ${deployableFiles.length} file${deployableFiles.length === 1 ? '' : 's'} copied to CIRCUITPY.` })
        toast.success(`Uploaded ${deployableFiles.length} file${deployableFiles.length === 1 ? '' : 's'}`)
      }
      return
    }

    if (!selectedPort) {
      toast.error('No board connected')
      return
    }

    const saved = await handleSaveAllFiles({ silent: true })
    if (!saved) return

    setBottomMode('shell')
    addLogEntry({ type: 'info', message: `Deploying ${deployableFiles.length} file${deployableFiles.length === 1 ? '' : 's'} to ${selectedPort}...` })

    try {
      for (const [index, file] of deployableFiles.entries()) {
        addLogEntry({ type: 'flash', message: `Uploading ${file.name} (${index + 1}/${deployableFiles.length})...` })
        const result = await hw.uploadCode({
          portPath: selectedPort,
          fileName: file.name,
          code: file.content,
        })
        if (!result.success) {
          addLogEntry({ type: 'error', message: result.error || `Upload failed for ${file.name}` })
          toast.error(result.error || `Upload failed for ${file.name}`)
          return
        }
      }
      setIsDirty(false)
      addLogEntry({ type: 'ok', message: `Deployment complete. ${deployableFiles.length} file${deployableFiles.length === 1 ? '' : 's'} synced to board.` })
      toast.success(`Uploaded ${deployableFiles.length} file${deployableFiles.length === 1 ? '' : 's'}`)
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error)
      addLogEntry({ type: 'error', message: `Upload failed: ${msg}` })
      toast.error('Upload failed')
    }
  }

  const handleStop = async () => {
    if (!selectedPort) {
      toast.error('No board connected')
      return
    }
    const hw = getHardware()
    if (!hw) return

    setBottomMode('shell')
    addLogEntry({ type: 'warn', message: 'Stopping execution...' })

    try {
      await hw.stopCode({ portPath: selectedPort })
      toast.warning('Stopped')
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error)
      addLogEntry({ type: 'error', message: `Stop failed: ${msg}` })
    }
  }

  const handleArduinoCompile = async () => {
    if (!projectPath) {
      toast.error('No project open')
      return
    }
    const hw = getHardware()
    if (!hw?.arduinoCompile) {
      toast.error('Arduino compile not available')
      return
    }
    const fqbn = activeProfile?.fqbn || 'arduino:avr:uno'
    setBottomMode('shell')
    addLogEntry({ type: 'info', message: `Compiling with arduino-cli (${fqbn})...` })
    try {
      const result = await hw.arduinoCompile({ sketchPath: projectPath, fqbn })
      if (result.success) {
        addLogEntry({ type: 'ok', message: 'Compile successful' })
        toast.success('Compiled successfully')
      } else {
        addLogEntry({ type: 'error', message: result.error || 'Compile failed' })
        toast.error('Compile failed')
      }
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error)
      addLogEntry({ type: 'error', message: `Compile failed: ${msg}` })
      toast.error('Compile failed')
    }
  }

  const handleArduinoUpload = async () => {
    if (!projectPath || !selectedPort) {
      toast.error('No project or board connected')
      return
    }
    const hw = getHardware()
    if (!hw?.arduinoUpload) {
      toast.error('Arduino upload not available')
      return
    }
    const fqbn = activeProfile?.fqbn || 'arduino:avr:uno'
    setBottomMode('shell')
    addLogEntry({ type: 'info', message: `Uploading to ${selectedPort} (${fqbn})...` })
    try {
      const result = await hw.arduinoUpload({ portPath: selectedPort, sketchPath: projectPath, fqbn })
      if (result.success) {
        addLogEntry({ type: 'ok', message: 'Upload successful' })
        toast.success('Uploaded successfully')
      } else {
        addLogEntry({ type: 'error', message: result.error || 'Upload failed' })
        toast.error('Upload failed')
      }
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error)
      addLogEntry({ type: 'error', message: `Upload failed: ${msg}` })
      toast.error('Upload failed')
    }
  }

  const handleFlashFirmware = () => {
    if (!selectedPort) {
      toast.error("Please select a board first.")
      return
    }
    const port = ports.find(p => p.path === selectedPort)
    const invalidSelection = getPortSelectionError(port)
    if (invalidSelection) {
      addLogEntry({ type: 'error', message: invalidSelection })
      toast.error('Invalid board port selected')
      toast.info(invalidSelection)
      return
    }
    // If the port isn't in the current list (stale), still pass its path so the
    // path-based fallback in detectBoardType can identify the board family.
    const board = detectBoardType(port ?? { vendorId: '', manufacturer: '', productId: '', path: selectedPort })
    setDetectedBoard(board)
    if (board === 'unknown') {
      addLogEntry({ type: 'warn', message: `Could not auto-detect board on ${selectedPort}. Select manually in the dialog.` })
    } else {
      addLogEntry({ type: 'info', message: `Detected ${BOARD_INFO[board]?.label ?? board} on ${selectedPort}` })
    }
    setFlashDialogOpen(true)
  }

  const handleConfirmFlash = async () => {
    if (detectedBoard === 'unknown') return
    const port = ports.find((entry) => entry.path === selectedPort)
    const invalidSelection = getPortSelectionError(port)
    if (invalidSelection) {
      addLogEntry({ type: 'error', message: invalidSelection })
      toast.error('Invalid board port selected')
      toast.info(invalidSelection)
      setFlashDialogOpen(false)
      return
    }
    const hw = getHardware()
    if (!hw) return

    // Auto-disconnect REPL if open on this port — esptool cannot share the serial port
    if (replConnected) {
      addLogEntry({ type: 'info', message: 'Disconnecting REPL to free serial port for flashing...' })
      toast.info('Closing REPL to free the serial port for flashing')
      try {
        if (hw.replDisconnect) {
          await hw.replDisconnect()
        }
        setReplConnected(false)
        // Give the OS a moment to release the port handle
        await new Promise((r) => setTimeout(r, 400))
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e)
        addLogEntry({ type: 'warn', message: `REPL disconnect warning: ${msg}` })
      }
    }

    const isEspressifFamily = detectedBoard === 'espressif' || detectedBoard === 'm5stack-atom' || detectedBoard === 'm5stamp-s3'
    const toolName = isEspressifFamily ? 'esptool.py' : detectedBoard === 'arduino' ? 'arduino-cli' : null
    if (toolName) {
      const check = await hw.checkTool(toolName)
      if (!check.available) {
        setToolMissing(true, isEspressifFamily ? 'espressif' : 'arduino')
        toast.error(`${toolName} not found. Click "Install" to install it.`)
        return
      }
    }

    setToolMissing(false)
    clearDialogFlashLogs()
    setIsFlashing(true)
    setBottomMode('shell')
    // Get out of the way so progress is watchable directly in the shell.
    setFlashDialogOpen(false)

    try {
      const result = await hw.flashFirmware({
        portPath: selectedPort,
        boardType: detectedBoard,
        firmwareUrl: 'auto',
        vendorId: selectedPortInfo?.vendorId,
        productId: selectedPortInfo?.productId,
      })
      if (result.success) {
        addLogEntry({ type: 'ok', message: 'Firmware flashed successfully!' })
        toast.success("Firmware flashed successfully!")
      } else {
        addLogEntry({ type: 'error', message: result.error || 'Flash failed.' })
        toast.error(result.error || "Flash failed.")
        if (result.hint) toast.info(result.hint)
      }
    } catch (error: unknown) {
      const errorMsg = error instanceof Error ? error.message : String(error)
      addLogEntry({ type: 'error', message: `Flash failed: ${errorMsg}` })
      toast.error(`Flash failed: ${errorMsg}`)
    } finally {
      setIsFlashing(false)
      setFlashDialogOpen(false)
    }
  }

  const handleInstallTool = async () => {
    if (!toolMissingType) return
    const hw = getHardware()
    if (!hw) return

    setIsInstalling(true)
    setBottomMode('shell')
    clearDialogFlashLogs()
    addLogEntry({ type: 'info', message: `Installing ${toolMissingType === 'espressif' ? 'esptool' : 'arduino-cli'}...` })

    try {
      const result = await hw.installTool(toolMissingType)
      if (result.success) {
        addLogEntry({ type: 'ok', message: 'Tool installed successfully!' })
        setToolMissing(false)
        setIsInstalling(false)
        await handleConfirmFlash()
      } else {
        addLogEntry({ type: 'error', message: result.error || 'Installation failed.' })
        if (result.hint) addLogEntry({ type: 'warn', message: result.hint })
        setIsInstalling(false)
      }
    } catch (error: unknown) {
      const errorMsg = error instanceof Error ? error.message : String(error)
      addLogEntry({ type: 'error', message: `Installation failed: ${errorMsg}` })
      setIsInstalling(false)
    }
  }

  const handleFileClick = async (file: typeof projectFiles[0]) => {
    if (isDirty && currentFile) {
      const shouldSave = window.confirm('Save changes to ' + currentFile.name + '?')
      if (shouldSave) await handleSaveFile()
    }
    setCurrentFile(file)
    openTab({
      path: file.path,
      name: file.name,
      isDirty: false,
      scope: 'local',
      language: file.language,
    })
  }

  const handleContextMenu = (e: React.MouseEvent, type: ContextMenuState['type'], path?: string) => {
    e.preventDefault()
    setContextMenu({
      visible: true,
      x: e.clientX,
      y: e.clientY,
      type,
      targetPath: path,
    })
  }

  const handleCreateFile = async () => {
    if (!projectPath || !newItemName.trim()) return
    const hw = getHardware()
    if (!hw) return

    const fileName = newItemName.trim().endsWith('.py') ? newItemName.trim() : `${newItemName.trim()}.py`
    
    try {
      const result = await hw.createFile(projectPath, fileName, '# New file\n')
      if (result.success && result.path) {
        const newFile = {
          name: fileName,
          path: result.path,
          content: '# New file\n',
          language: 'python' as const,
        }
        setProjectFiles([...projectFiles, newFile])
        setCurrentFile(newFile)
        toast.success(`Created ${fileName}`)
      }
    } catch (error) {
      toast.error('Failed to create file')
      console.error(error)
    }
    setCreateDialogOpen(null)
    setNewItemName('')
  }

  const handleCreateFolder = async () => {
    if (!projectPath || !newItemName.trim()) return
    const hw = getHardware()
    if (!hw) return

    const folderPath = `${projectPath}/${newItemName.trim()}`
    
    try {
      const result = await hw.createDirectory(folderPath)
      if (result.success) {
        toast.success(`Created folder ${newItemName.trim()}`)
      }
    } catch (error) {
      toast.error('Failed to create folder')
      console.error(error)
    }
    setCreateDialogOpen(null)
    setNewItemName('')
  }

  // Show welcome screen if no project is open
  if (welcomeScreenVisible) {
    return (
      <>
        <WelcomeScreen
          onOpenProject={handleOpenProject}
          onCreateFromTemplate={handleCreateFromTemplate}
          onOpenSettings={() => { setSettingsOpen(true); setWelcomeScreenVisible(false) }}
        />
        <SettingsDialog open={settingsOpen} onClose={() => setSettingsOpen(false)} />
        <Toaster position="bottom-right" theme="dark" />
      </>
    )
  }

  return (
    <div className="flex flex-col h-screen w-full bg-white dark:bg-[#0a0a0a] text-gray-700 dark:text-gray-300 font-sans overflow-hidden select-none">
      {/* Top Header */}
      <div className="h-14 flex items-center justify-between px-4 border-b border-gray-200 dark:border-white/5 shrink-0 bg-gray-100 dark:bg-[#0f0f0f]">
        <div className="flex items-center gap-4">
          <img src="/negative-icon-dark.svg" alt="Anode" className="h-7 dark:hidden" />
          <img src="/negative-icon-dark.svg" alt="Anode" className="h-7 hidden dark:block opacity-80" />
          <div className="h-6 w-px bg-gray-300 dark:bg-gray-700" />
          <div className="flex items-center gap-1.5">
            <Button onClick={() => void handleSaveAllFiles()} disabled={projectFiles.length === 0} variant="outline" className="gap-1.5 font-medium h-8 px-3 rounded-md text-sm border-gray-300 dark:border-white/10 bg-white/80 dark:bg-white/5">
              <Save className="size-3.5" /> Save All
            </Button>
            {workspaceLayout.showRunButton && (
              <Button onClick={handleRun} disabled={!currentFile} className="bg-green-600 hover:bg-green-700 text-white gap-1.5 font-medium h-8 px-3 rounded-md text-sm disabled:opacity-50">
                <Play className="size-3.5" /> Run
              </Button>
            )}
            {workspaceLayout.showCompileButton && (
              <Button onClick={handleArduinoCompile} disabled={!projectPath} className="bg-amber-600 hover:bg-amber-700 text-white gap-1.5 font-medium h-8 px-3 rounded-md text-sm disabled:opacity-50">
                <Hammer className="size-3.5" /> Compile
              </Button>
            )}
            {workspaceLayout.showUploadButton && (
              <Button onClick={workspaceMode === 'arduino' ? handleArduinoUpload : handleUpload} disabled={!currentFile} className="bg-blue-600 hover:bg-blue-700 text-white gap-1.5 font-medium h-8 px-3 rounded-md text-sm disabled:opacity-50">
                <Upload className="size-3.5" /> {workspaceMode === 'arduino' ? 'Upload' : 'Upload'}
              </Button>
            )}
            <Button onClick={handleStop} className="bg-red-500 hover:bg-red-600 text-white gap-1.5 font-medium h-8 px-3 rounded-md text-sm">
              <Square className="size-3.5 fill-current" /> Stop
            </Button>
          </div>
        </div>
        <div className="flex items-center gap-2">
          {isDirty && <Badge variant="outline" className="text-yellow-600 dark:text-yellow-500 border-yellow-500/50 text-[10px]">Modified</Badge>}
          {currentFile && (
            <span className="text-xs text-gray-500 dark:text-gray-400">{currentFile.name}</span>
          )}
          <Button onClick={() => setSettingsOpen(true)} title="Settings" variant="ghost" size="icon" className="text-gray-500 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white hover:bg-gray-200 dark:hover:bg-white/10 rounded-md">
            <Settings className="size-4" />
          </Button>
        </div>
      </div>

      {/* Connection dropped warning banner */}
      {connectionDropped && selectedPort && (
        <div className="bg-yellow-500/10 border-b border-yellow-500/20 text-yellow-600 dark:text-yellow-500 px-4 py-1.5 text-xs flex items-center gap-2 shrink-0">
          <AlertTriangle className="size-3.5 shrink-0" />
          <span>Connection lost on {selectedPort}. Waiting for reconnect...</span>
        </div>
      )}

      {/* Main Content Row */}
      <div className="flex flex-1 min-h-0">
        {/* Left Activity Bar */}
        <div className="w-14 flex flex-col items-center py-4 border-r border-gray-200 dark:border-white/5 bg-gray-50 dark:bg-[#0a0a0a] shrink-0 gap-6">
          <Files className="size-6 text-green-600 dark:text-green-500" />
          <Search className="size-6 text-green-600 dark:text-green-500 opacity-60 hover:opacity-100 cursor-pointer transition-opacity" />
          <div className="mt-auto flex flex-col gap-6 items-center pb-2">
            <Settings 
              className="size-6 text-green-600 dark:text-green-500 opacity-60 hover:opacity-100 cursor-pointer transition-opacity" 
              onClick={() => setSettingsOpen(true)}
            />
          </div>
        </div>

        {/* Center Area */}
        <div className="flex-1 flex flex-col min-w-0 relative">
          {/* Editor Tabs */}
          <EditorTabs onTabSelect={handleTabSelect} onTabClose={handleTabClose} />

          {/* Editor Area */}
          <div className="flex-1 relative bg-white dark:bg-[#0a0a0a]">
            <Editor
              height="100%"
              language={currentFile?.language || 'python'}
              theme="vs-dark"
              value={currentFile?.content || ''}
              onChange={(val) => {
                if (currentFile && val !== undefined) {
                  updateFileContent(currentFile.path, val)
                  markTabDirty(currentFile.path)
                }
              }}
              onMount={handleEditorMount}
              options={{
                minimap: { enabled: false },
                fontSize: 14,
                fontFamily: "'JetBrains Mono', 'Fira Code', 'Menlo', monospace",
                smoothScrolling: true,
                cursorBlinking: "smooth",
                lineNumbersMinChars: 3,
                renderLineHighlight: "line",
                scrollbar: { verticalScrollbarSize: 10, horizontalScrollbarSize: 10 },
                wordWrap: 'on',
              }}
            />
          </div>

          {/* Combined Shell + REPL panel */}
          <BottomPanel
            logEntries={logEntries}
            onClearLogs={clearLogEntries}
          />

          {/* Bottom Status Bar */}
          <div className="h-7 border-t border-gray-200 dark:border-white/5 bg-gray-100 dark:bg-[#0a0a0a] flex items-center justify-between px-3 text-[11px] text-gray-500 dark:text-gray-400 font-mono tracking-wide shrink-0 z-10">
            <div className="flex items-center gap-4">
              <span className="text-gray-600 dark:text-gray-300">{selectedPort ? 'connected' : 'disconnected'}</span>
              <span className="text-gray-300 dark:text-gray-600">|</span>
              <span title={board ? `${board.vendor} · ${board.chip}` : undefined}>
                {boardLabel ?? 'No Board'}
              </span>
            </div>
            <div className="flex items-center gap-4">
              <span>{currentFile?.name || 'No file'}</span>
              <span className="text-gray-300 dark:text-gray-600">|</span>
              <span>Ln {editorState.line}, Col {editorState.column}</span>
              <span className="text-gray-300 dark:text-gray-600">|</span>
              <span>UTF-8</span>
            </div>
          </div>
        </div>

        {/* Right Sidebar */}
        <div className="w-72 flex flex-col bg-gray-100 dark:bg-[#0f0f0f] shrink-0 relative">
          {/* Content */}
          <ScrollArea className="flex-1">
            <div className="p-4 flex flex-col gap-6">
              {/* Project Files */}
              <div>
                <div className="flex items-center justify-between mb-3">
                  <div className="text-[11px] font-bold text-gray-500 dark:text-gray-500 tracking-widest uppercase">
                    {projectPath?.split('/').pop() || 'PROJECT'}/
                  </div>
                  <div className="relative group">
                    <button
                      onClick={() => setCreateDialogOpen(createDialogOpen ? null : 'file')}
                      className="p-1 hover:bg-gray-200 dark:hover:bg-white/10 rounded transition-colors"
                    >
                      <Plus className="size-4 text-gray-500 dark:text-gray-400" />
                    </button>
                    {/* Dropdown */}
                    <div className="hidden group-hover:block absolute right-0 top-full mt-1 bg-white dark:bg-[#1a1a1a] border border-gray-200 dark:border-white/10 rounded-md shadow-lg py-1 z-50 min-w-[140px]">
                      <button
                        onClick={() => { setCreateDialogOpen('file'); setNewItemName('') }}
                        className="w-full px-3 py-1.5 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-white/10 flex items-center gap-2"
                      >
                        <FilePlus className="size-4" /> New File
                      </button>
                      <button
                        onClick={() => { setCreateDialogOpen('folder'); setNewItemName('') }}
                        className="w-full px-3 py-1.5 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-white/10 flex items-center gap-2"
                      >
                        <FolderPlus className="size-4" /> New Folder
                      </button>
                    </div>
                  </div>
                </div>
                <div className="flex flex-col">
                  {projectFiles.length === 0 ? (
                    <div className="text-sm text-gray-500 dark:text-gray-500 italic">No files in project</div>
                  ) : (
                    projectFiles.map((file) => (
                      <div
                        key={file.path}
                        onClick={() => handleFileClick(file)}
                        onContextMenu={(e) => handleContextMenu(e, 'file', file.path)}
                        className={`group flex items-center gap-2 px-3 py-2 rounded-sm text-[13px] cursor-pointer transition-colors ${
                          currentFile?.path === file.path
                            ? 'bg-gray-200 dark:bg-white/10 text-gray-900 dark:text-white border border-gray-300 dark:border-white/10'
                            : 'text-gray-600 dark:text-gray-400 hover:bg-gray-200 dark:hover:bg-white/5 hover:text-gray-900 dark:hover:text-gray-200'
                        }`}
                      >
                        <FileCode2 className="size-4 text-teal-600 dark:text-teal-500 shrink-0" />
                        <span className="truncate flex-1">{file.name}</span>
                        {/* Hover actions */}
                        <div className="hidden group-hover:flex items-center gap-1 shrink-0">
                          <button
                            onClick={(e) => { e.stopPropagation(); setCreateDialogOpen('file'); setNewItemName('') }}
                            className="p-1 hover:bg-gray-300 dark:hover:bg-white/20 rounded"
                            title="New file"
                          >
                            <FilePlus className="size-3 text-gray-500 dark:text-gray-400" />
                          </button>
                          <button
                            onClick={(e) => { e.stopPropagation(); setCreateDialogOpen('folder'); setNewItemName('') }}
                            className="p-1 hover:bg-gray-300 dark:hover:bg-white/20 rounded"
                            title="New folder"
                          >
                            <FolderPlus className="size-3 text-gray-500 dark:text-gray-400" />
                          </button>
                        </div>
                      </div>
                    ))
                  )}
                </div>
              </div>

              {/* Board Panel */}
              <div className="bg-gray-200/50 dark:bg-white/5 border border-gray-200 dark:border-white/5 rounded-xl p-4 flex flex-col gap-3 shadow-sm">
                <div className="flex items-center gap-2 text-[10px] font-bold text-gray-500 dark:text-gray-500 tracking-widest uppercase">
                  <div className={`size-2 rounded-full ${selectedPort ? 'bg-green-500 dark:bg-green-500 shadow-[0_0_8px_rgba(34,197,94,0.5)]' : 'bg-gray-400 dark:bg-gray-500'}`} /> BOARD
                </div>
                <div className="flex items-start justify-between gap-3">
                  <div>
                    <div className="text-teal-600 dark:text-teal-500 font-medium text-base tracking-wide">
                      {boardLabel ?? 'No Board Connected'}
                    </div>
                    <div className="mt-1 text-xs text-gray-500 dark:text-gray-400">
                      {board
                        ? `${board.vendor} · ${board.chip}${boardMatch.exact ? '' : boardMatch.fromOverride ? ' · pinned' : ' · auto-detected, confirm below'}`
                        : selectedPort ? `Ready on ${selectedPort}` : 'Connect board to detect port and firmware target'}
                    </div>
                  </div>
                  <button
                    onClick={refreshPorts}
                    className="p-2 rounded-md border border-gray-300 dark:border-white/10 hover:bg-gray-100 dark:hover:bg-white/10 transition-colors"
                    title="Refresh ports"
                  >
                    <RotateCw className="size-4 text-gray-500 dark:text-gray-300" />
                  </button>
                </div>

                {/* Board Selector — lets the user pin the exact board when USB
                    descriptors can't tell ESP32-S3 variants apart. */}
                {selectedPort && boardMatch.candidates.length > 1 && (
                  <div>
                    <div className="text-[10px] font-bold text-gray-500 dark:text-gray-500 tracking-widest uppercase mb-1.5">
                      Board
                    </div>
                    <Select
                      value={board?.id ?? ''}
                      onValueChange={(val) => {
                        if (val && selectedPortInfo) setBoardOverride(portKey(selectedPortInfo), val)
                      }}
                    >
                      <SelectTrigger className="w-full h-8 text-xs bg-white dark:bg-black/20 border-gray-300 dark:border-white/10">
                        <SelectValue placeholder="Select board..." />
                      </SelectTrigger>
                      <SelectContent className="bg-white dark:bg-[#111] border-gray-200 dark:border-white/10 text-gray-900 dark:text-gray-300">
                        {boardMatch.candidates.map((b) => (
                          <SelectItem key={b.id} value={b.id}>
                            {b.name}{b.generic ? '' : ' ★'}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                    {!boardMatch.exact && !boardMatch.fromOverride && (
                      <div className="mt-1 text-[10px] text-amber-600 dark:text-amber-500/80">
                        Same chip family — pick your exact board so firmware and pins match.
                      </div>
                    )}
                  </div>
                )}

                {/* Execution Profile Selector */}
                {availableProfiles.length > 0 && (
                  <div>
                    <div className="text-[10px] font-bold text-gray-500 dark:text-gray-500 tracking-widest uppercase mb-1.5">
                      Runtime Profile
                    </div>
                    <Select
                      value={activeProfile?.id ?? ''}
                      onValueChange={(val) => { if (val) selectProfile(val) }}
                    >
                      <SelectTrigger className="w-full h-8 text-xs bg-white dark:bg-black/20 border-gray-300 dark:border-white/10">
                        <SelectValue placeholder="Select runtime profile..." />
                      </SelectTrigger>
                      <SelectContent className="bg-white dark:bg-[#111] border-gray-200 dark:border-white/10 text-gray-900 dark:text-gray-300">
                        {availableProfiles.map((profile) => (
                          <SelectItem key={profile.id} value={profile.id}>
                            {profile.boardName}
                            {profile.confidence === 'high' && !profile.isGeneric ? ' ★' : ''}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                    {activeProfile && (
                      <div className="mt-1.5 text-[10px] text-gray-500 dark:text-gray-500 flex items-center gap-1.5">
                        <span className={`px-1.5 py-0.5 rounded ${activeProfile.runtime === 'Arduino' ? 'bg-blue-500/20 text-blue-500' : 'text-teal-500 bg-teal-500/20'}`}>
                          {activeProfile.runtime}
                        </span>
                        <span>{activeProfile.variant}</span>
                      </div>
                    )}
                  </div>
                )}

                <div className="grid grid-cols-2 gap-2 text-xs">
                  <div className="rounded-lg bg-white/80 dark:bg-black/20 border border-gray-200 dark:border-white/10 p-3">
                    <div className="flex items-center gap-2 text-gray-500 dark:text-gray-400 uppercase tracking-wide text-[10px]">
                      <Cpu className="size-3.5" /> Interface
                    </div>
                    <div className="mt-1 font-medium text-gray-800 dark:text-gray-100">
                      {selectedPortInfo?.manufacturer || 'Unknown USB bridge'}
                    </div>
                    <div className="mt-1 text-gray-500 dark:text-gray-400 font-mono">
                      {selectedPortInfo?.vendorId || '----'}:{selectedPortInfo?.productId || '----'}
                    </div>
                  </div>
                  <div className="rounded-lg bg-white/80 dark:bg-black/20 border border-gray-200 dark:border-white/10 p-3">
                    <div className="flex items-center gap-2 text-gray-500 dark:text-gray-400 uppercase tracking-wide text-[10px]">
                      <Zap className="size-3.5" /> Deploy
                    </div>
                    <div className="mt-1 font-medium text-gray-800 dark:text-gray-100">
                      {deployableFiles.length} file{deployableFiles.length === 1 ? '' : 's'}
                    </div>
                    <div className="mt-1 text-gray-500 dark:text-gray-400">
                      {workspaceMode === 'arduino' ? 'Compile then upload' : 'Save local first, then sync'}
                    </div>
                  </div>
                </div>

                <Select value={selectedPort} onValueChange={(val) => {
                  setSelectedPort(val || "")
                  setLastPortPath(val || "")
                  const port = ports.find(p => p.path === val)
                  if (port) {
                    setDetectedBoard(detectBoardType(port))
                    const profiles = detectExecutionProfiles({ ...port, circuitPython: circuitPythonHint() })
                    setAvailableProfiles(profiles)
                    const best = selectBestProfile(profiles)
                    if (best) setActiveProfile(best)
                  }
                }}>
                  <SelectTrigger className="w-full h-8 text-xs bg-white dark:bg-black/20 border-gray-300 dark:border-white/10">
                    <SelectValue placeholder="Select COM Port" />
                  </SelectTrigger>
                  <SelectContent className="bg-white dark:bg-[#111] border-gray-200 dark:border-white/10 text-gray-900 dark:text-gray-300">
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

                <Button onClick={handleFlashFirmware} className="w-full bg-[#77a666] hover:bg-[#648c56] text-black font-semibold gap-2">
                  <Upload className="size-4" /> Flash Firmware
                </Button>

                {workspaceLayout.showUploadButton && (
                  <Button onClick={workspaceMode === 'arduino' ? handleArduinoUpload : handleUpload} className="w-full bg-blue-600 hover:bg-blue-700 text-white font-semibold gap-2" disabled={!selectedPort || deployableFiles.length === 0}>
                    <Upload className="size-4" /> {workspaceMode === 'arduino' ? 'Upload to Board' : 'Upload Project'}
                  </Button>
                )}

                <Button
                  onClick={() => setBottomMode(bottomMode === 'repl' ? 'hidden' : 'repl')}
                  variant="ghost"
                  className={`w-full gap-2 ${bottomMode === 'repl' ? 'text-teal-400' : 'text-gray-400'}`}
                >
                  <Terminal className="size-4" /> {bottomMode === 'repl' ? 'Hide REPL' : 'Show REPL'}
                </Button>
              </div>

              {/* Device File Tree — only for MicroPython/CircuitPython mode */}
              {workspaceLayout.showDeviceFileTree && (
                <DeviceFileTree className="rounded-xl border border-gray-200 dark:border-white/5 overflow-hidden max-h-[300px]" />
              )}
            </div>
          </ScrollArea>
        </div>
      </div>

      {/* Context Menu */}
      {contextMenu.visible && (
        <div
          className="fixed bg-[#1a1a1a] border border-white/10 rounded-md shadow-lg py-1 z-50 min-w-[180px]"
          style={{ left: contextMenu.x, top: contextMenu.y }}
          onClick={(e) => e.stopPropagation()}
        >
          {contextMenu.type === 'file' && (
            <>
              <button
                onClick={() => { handleSaveFile(); setContextMenu(prev => ({ ...prev, visible: false })) }}
                className="w-full px-3 py-1.5 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-white/10 flex items-center gap-2"
              >
                <Save className="size-4" /> Save
              </button>
              <button
                onClick={() => { setWelcomeScreenVisible(true); setContextMenu(prev => ({ ...prev, visible: false })) }}
                className="w-full px-3 py-1.5 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-white/10 flex items-center gap-2"
              >
                <Plus className="size-4" /> New Project
              </button>
              <button
                onClick={() => { setContextMenu(prev => ({ ...prev, visible: false })); setCommandPaletteOpen(true) }}
                className="w-full px-3 py-1.5 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-white/10 flex items-center gap-2"
              >
                <Search className="size-4" /> Command Palette
              </button>
              <div className="border-t border-gray-200 dark:border-white/5 my-1" />
              <button
                onClick={() => { setContextMenu(prev => ({ ...prev, visible: false })); window.location.reload() }}
                className="w-full px-3 py-1.5 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-white/10 flex items-center gap-2"
              >
                <RotateCw className="size-4" /> Reload Project
              </button>
              <button
                onClick={() => { setSettingsOpen(true); setContextMenu(prev => ({ ...prev, visible: false })) }}
                className="w-full px-3 py-1.5 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-white/10 flex items-center gap-2"
              >
                <Settings className="size-4" /> Settings
              </button>
              <div className="border-t border-gray-200 dark:border-white/5 my-1" />
              <button
                onClick={() => { setBottomMode(bottomMode === 'shell' ? 'hidden' : 'shell'); setContextMenu(prev => ({ ...prev, visible: false })) }}
                className="w-full px-3 py-1.5 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-white/10 flex items-center gap-2"
              >
                <Terminal className="size-4" /> {bottomMode === 'shell' ? 'Hide' : 'Show'} Shell
              </button>
              <button
                onClick={() => { setBottomMode(bottomMode === 'repl' ? 'hidden' : 'repl'); setContextMenu(prev => ({ ...prev, visible: false })) }}
                className="w-full px-3 py-1.5 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-white/10 flex items-center gap-2"
              >
                <Terminal className="size-4" /> {bottomMode === 'repl' ? 'Hide' : 'Show'} REPL
              </button>
            </>
          )}
        </div>
      )}

      {/* Flash Dialog */}
      <FlashDialog
        open={flashDialogOpen}
        onClose={() => setFlashDialogOpen(false)}
        detectedBoard={detectedBoard}
        isFlashing={isFlashing}
        isInstalling={isInstalling}
        toolMissing={toolMissing}
        toolMissingType={toolMissingType}
        dialogFlashLogs={dialogFlashLogs}
        selectedPort={selectedPort}
        onConfirmFlash={handleConfirmFlash}
        onInstallTool={handleInstallTool}
      />

      {/* Settings Dialog */}
      <SettingsDialog open={settingsOpen} onClose={() => setSettingsOpen(false)} />

      {/* Create File/Folder Dialog */}
      <CreateDialog
        open={createDialogOpen !== null}
        type={createDialogOpen}
        name={newItemName}
        onNameChange={setNewItemName}
        onClose={() => { setCreateDialogOpen(null); setNewItemName('') }}
        onConfirm={() => {
          if (createDialogOpen === 'file') {
            handleCreateFile()
          } else if (createDialogOpen === 'folder') {
            handleCreateFolder()
          }
        }}
      />

      {/* Command Palette */}
      <CommandPalette
        open={commandPaletteOpen}
        onClose={() => setCommandPaletteOpen(false)}
        onOpenProject={handleOpenProject}
        onCreateFromTemplate={handleCreateFromTemplate}
        onSaveFile={handleSaveFile}
        onSaveAllFiles={handleSaveAllFiles}
        onRun={handleRun}
        onUpload={handleUpload}
        onStop={handleStop}
        onOpenFlash={handleFlashFirmware}
      />

      <Toaster position="bottom-right" theme="dark" />
    </div>
  )
}

// Create File/Folder Dialog Component
function CreateDialog({
  open,
  type,
  name,
  onNameChange,
  onClose,
  onConfirm,
}: {
  open: boolean
  type: 'file' | 'folder' | null
  name: string
  onNameChange: (name: string) => void
  onClose: () => void
  onConfirm: () => void
}) {
  if (!open || !type) return null

  return (
    <Dialog open={open} onOpenChange={onClose}>
      <DialogContent className="bg-white dark:bg-[#111] border-gray-200 dark:border-white/10 text-gray-900 dark:text-white sm:max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            {type === 'file' ? <FilePlus className="size-5" /> : <FolderPlus className="size-5" />}
            Create New {type === 'file' ? 'File' : 'Folder'}
          </DialogTitle>
          <DialogDescription className="text-gray-500 dark:text-gray-400">
            Enter a name for your new {type === 'file' ? 'file' : 'folder'}.
            {type === 'file' && ' A .py extension will be added automatically if not specified.'}
          </DialogDescription>
        </DialogHeader>
        
        <div className="py-2">
          <input
            type="text"
            value={name}
            onChange={(e) => onNameChange(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && name.trim() && onConfirm()}
            placeholder={type === 'file' ? 'filename.py' : 'folder-name'}
            className="w-full bg-gray-100 dark:bg-black/30 border border-gray-300 dark:border-white/10 rounded-md px-3 py-2 text-gray-900 dark:text-white text-sm focus:outline-none focus:border-green-500"
            autoFocus
          />
        </div>

        <DialogFooter>
          <Button variant="ghost" onClick={onClose} className="text-gray-600 dark:text-gray-400">
            Cancel
          </Button>
          <Button
            onClick={() => { onConfirm(); }}
            disabled={!name.trim()}
            className="bg-green-600 hover:bg-green-700 text-white"
          >
            Create
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

// Flash Dialog Component
function FlashDialog({
  open, onClose, detectedBoard, isFlashing, isInstalling, toolMissing,
  dialogFlashLogs, selectedPort, onConfirmFlash, onInstallTool,
}: {
  open: boolean
  onClose: () => void
  detectedBoard: string
  isFlashing: boolean
  isInstalling: boolean
  toolMissing: boolean
  toolMissingType?: 'espressif' | 'arduino' | null
  dialogFlashLogs: { id: string; timestamp: Date; type: string; message: string }[]
  selectedPort: string
  onConfirmFlash: () => void
  onInstallTool: () => void
}) {
  return (
    <Dialog open={open} onOpenChange={onClose}>
      <DialogContent className="bg-[#111] border-white/10 text-gray-200 sm:max-w-md max-h-[90vh] overflow-y-auto">
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
                  {(detectedBoard === 'espressif' || detectedBoard === 'm5stack-atom' || detectedBoard === 'm5stamp-s3') ? 'esptool.py' : 'arduino-cli'}
                </span>{' '}

              is required but not installed.
              </span>
            ) : detectedBoard === 'unknown' ? (
              <>Could not identify the connected board. Please select manually.</>
            ) : (
              <>
                Detected <span className="text-teal-400 font-medium">{BOARD_INFO[detectedBoard]?.label}</span> on{' '}
                <span className="text-gray-200 font-mono">{selectedPort}</span>.
              </>
            )}
          </DialogDescription>
        </DialogHeader>

        {toolMissing && !isInstalling ? (
          <div className="flex flex-col gap-3 py-2">
            <div className="bg-red-500/10 border border-red-500/20 rounded-md p-4 flex flex-col gap-2">
              <div className="text-[13px] text-gray-200 font-medium">
                {(detectedBoard === 'espressif' || detectedBoard === 'm5stack-atom' || detectedBoard === 'm5stamp-s3')
                  ? 'esptool.py is needed to flash ESP32/ESP8266 boards.'
                  : 'arduino-cli is needed to upload to Arduino boards.'}
              </div>
              <div className="text-[12px] text-gray-400">
                {(detectedBoard === 'espressif' || detectedBoard === 'm5stack-atom' || detectedBoard === 'm5stamp-s3')
                  ? 'This will run: python3 -m pip install esptool'
                  : 'This will download and install arduino-cli'}
              </div>
            </div>
          </div>
        ) : (isFlashing || isInstalling) ? (
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
                  <div key={entry.id} className="flex gap-2">
                    <span className="text-gray-600 shrink-0 select-none">
                      {entry.timestamp.toLocaleTimeString('en-US', { hour12: false })}
                    </span>
                    <span className={`select-text break-words whitespace-pre-wrap min-w-0 ${
                      entry.type === 'error' ? 'text-red-400' :
                      entry.type === 'warn' ? 'text-yellow-400' :
                      entry.type === 'ok' ? 'text-green-400' :
                      entry.type === 'install' ? 'text-blue-400' :
                      'text-cyan-400'
                    }`}>
                      {entry.message}
                    </span>
                  </div>
                ))
              )}
            </div>
            {(detectedBoard === 'espressif' || detectedBoard === 'm5stack-atom' || detectedBoard === 'm5stamp-s3') && (
              <div className="flex items-start gap-2 bg-yellow-500/10 text-yellow-500 border border-yellow-500/20 p-2.5 rounded-sm text-xs leading-relaxed">
                <AlertTriangle className="size-4 shrink-0 mt-0.5" />
                <div>
                  <span className="font-semibold">Connection tip:</span> Try holding the <span className="font-bold">BOOT</span> button on your board.
                </div>
              </div>
            )}
          </div>
        ) : (
          <div className="flex flex-col gap-3 py-2">
            <label className="text-[11px] font-bold text-gray-500 tracking-widest uppercase">Board Type</label>
            <Select
              value={detectedBoard === 'unknown' ? '' : detectedBoard}
              onValueChange={(val) => useStore.getState().setDetectedBoard(val as BoardType)}
            >
              <SelectTrigger className="w-full h-9 text-sm bg-black/30 border-white/10">
                <SelectValue placeholder="Select board type..." />
              </SelectTrigger>
              <SelectContent className="bg-[#111] border-white/10 text-gray-300">
                <SelectItem value="espressif">Espressif (ESP32 / ESP8266)</SelectItem>
                <SelectItem value="m5stack-atom">M5Stack Atom Light (ESP32 PICO)</SelectItem>
                <SelectItem value="m5stamp-s3">M5StampS3 (ESP32-S3)</SelectItem>
                <SelectItem value="raspberry-pi">Raspberry Pi Pico (RP2040)</SelectItem>
                <SelectItem value="arduino">Arduino</SelectItem>
              </SelectContent>
            </Select>
            {detectedBoard !== 'unknown' && (
              <div className="bg-white/5 border border-white/5 rounded-md p-3 flex flex-col gap-2 text-[13px]">
                <p className="text-gray-300">{BOARD_INFO[detectedBoard]?.description}</p>
                <div className="flex items-center gap-2 text-gray-500 font-mono text-xs">
                  <span className="text-gray-400">Tool:</span> {BOARD_INFO[detectedBoard]?.tool}
                </div>
              </div>
            )}
          </div>
        )}

        <DialogFooter className="bg-transparent border-t-white/5">
          <DialogClose render={
            <Button variant="ghost" className="text-gray-400 hover:text-white hover:bg-white/10" disabled={isFlashing || isInstalling}>
              Cancel
            </Button>
          }>
            Cancel
          </DialogClose>
          {toolMissing ? (
            <Button onClick={onInstallTool} disabled={isInstalling} className="bg-blue-600 hover:bg-blue-700 text-white gap-2 font-semibold">
              {isInstalling ? <><Loader2 className="size-4 animate-spin" /> Installing...</> : 'Install Tool'}
            </Button>
          ) : (
            <Button
              onClick={onConfirmFlash}
              disabled={detectedBoard === 'unknown' || isFlashing}
              className="bg-[#77a666] hover:bg-[#648c56] text-black font-semibold gap-2 disabled:opacity-40"
            >
              {isFlashing ? <><Loader2 className="size-4 animate-spin" /> Flashing...</> : 'Flash Firmware'}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

// Settings Dialog Component
function SettingsDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const [activeTab, setActiveTab] = useState<'general' | 'board' | 'editor'>('general')
  
  return (
    <Dialog open={open} onOpenChange={onClose}>
      <DialogContent className="bg-[#111] border-white/10 text-gray-200 sm:max-w-lg">
        <DialogHeader>
          <DialogTitle className="text-white flex items-center gap-2">
            <Settings className="size-5" /> Settings
          </DialogTitle>
        </DialogHeader>
        
        <div className="flex gap-4 py-4">
          {/* Tabs */}
          <div className="w-40 shrink-0">
            <div className="flex flex-col gap-1">
              {(['general', 'board', 'editor'] as const).map((tab) => (
                <button
                  key={tab}
                  onClick={() => setActiveTab(tab)}
                  className={`px-3 py-2 text-sm text-left rounded-md transition-colors capitalize ${
                    activeTab === tab ? 'bg-white/10 text-white' : 'text-gray-400 hover:text-white hover:bg-white/5'
                  }`}
                >
                  {tab}
                </button>
              ))}
            </div>
          </div>
          
          {/* Content */}
          <div className="flex-1 min-h-[300px]">
            {activeTab === 'general' && (
              <div className="flex flex-col gap-4">
                <div>
                  <h3 className="text-sm font-medium text-white mb-2">Application</h3>
                  <div className="space-y-2">
                    <label className="flex items-center gap-2 text-sm text-gray-400">
                      <input type="checkbox" className="rounded border-gray-600 bg-black/30" />
                      Check for updates on startup
                    </label>
                    <label className="flex items-center gap-2 text-sm text-gray-400">
                      <input type="checkbox" className="rounded border-gray-600 bg-black/30" />
                      Show welcome screen on startup
                    </label>
                  </div>
                </div>
                <div>
                  <h3 className="text-sm font-medium text-white mb-2">Recent Projects</h3>
                  <Button 
                    variant="outline" 
                    className="text-gray-400 border-white/10 hover:bg-white/10"
                    onClick={() => {
                      useStore.getState().recentProjects.forEach(p => useStore.getState().removeRecentProject(p.path))
                      toast.success('Recent projects cleared')
                    }}
                  >
                    Clear Recent Projects
                  </Button>
                </div>
              </div>
            )}
            
            {activeTab === 'board' && (
              <div className="flex flex-col gap-4">
                <div>
                  <h3 className="text-sm font-medium text-white mb-2">Board Connection</h3>
                  <div className="space-y-3">
                    <div>
                      <label className="text-xs text-gray-500 mb-1 block">Baud Rate</label>
                      <Select defaultValue="460800">
                        <SelectTrigger className="w-full h-9 text-sm bg-black/30 border-white/10">
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent className="bg-[#111] border-white/10">
                          <SelectItem value="9600">9600</SelectItem>
                          <SelectItem value="115200">115200</SelectItem>
                          <SelectItem value="460800">460800</SelectItem>
                          <SelectItem value="921600">921600</SelectItem>
                        </SelectContent>
                      </Select>
                    </div>
                  </div>
                </div>
              </div>
            )}
            
            {activeTab === 'editor' && (
              <div className="flex flex-col gap-4">
                <div>
                  <h3 className="text-sm font-medium text-white mb-2">Font Size</h3>
                  <Select defaultValue="14">
                    <SelectTrigger className="w-full h-9 text-sm bg-black/30 border-white/10">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent className="bg-[#111] border-white/10">
                      <SelectItem value="12">12px</SelectItem>
                      <SelectItem value="14">14px</SelectItem>
                      <SelectItem value="16">16px</SelectItem>
                      <SelectItem value="18">18px</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
                <div>
                  <h3 className="text-sm font-medium text-white mb-2">Tab Size</h3>
                  <Select defaultValue="4">
                    <SelectTrigger className="w-full h-9 text-sm bg-black/30 border-white/10">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent className="bg-[#111] border-white/10">
                      <SelectItem value="2">2 spaces</SelectItem>
                      <SelectItem value="4">4 spaces</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
                <label className="flex items-center gap-2 text-sm text-gray-400">
                  <input type="checkbox" defaultChecked className="rounded border-gray-600 bg-black/30" />
                  Word wrap
                </label>
              </div>
            )}
          </div>
        </div>
        
        <DialogFooter className="bg-transparent border-t-white/5">
          <Button onClick={onClose} className="bg-green-600 hover:bg-green-700 text-white">
            Done
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
