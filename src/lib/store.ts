import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import {
  type ExecutionProfile,
  type ProfileDetectionInput,
  type WorkspaceMode,
  type RuntimeEnvironment,
  type WorkspaceLayout,
  KNOWN_BOARD_SIGNATURES,
  GENERIC_PROFILES,
  runtimeToWorkspaceMode,
  getWorkspaceLayout,
} from '@/types/interpreter'

export interface ProjectFile {
  name: string
  path: string
  content: string
  language: 'python' | 'arduino'
}

export interface LogEntry {
  id: string
  timestamp: Date
  type: 'info' | 'warn' | 'error' | 'ok' | 'install' | 'flash'
  message: string
}

export type BoardType = 'espressif' | 'm5stack-atom' | 'm5stamp-s3' | 'raspberry-pi' | 'arduino' | 'unknown'

export interface PortInfo {
  path: string
  manufacturer?: string
  vendorId?: string
  productId?: string
  serialNumber?: string
}

// Result of the main-process CIRCUITPY-drive probe (`detect-circuitpython`).
export interface CircuitPythonDrive {
  present: boolean
  mountPath?: string
  boardName?: string
  version?: string
}

export interface EditorState {
  line: number
  column: number
}

export interface OpenTab {
  path: string
  name: string
  isDirty: boolean
  scope: 'local' | 'remote'
  language: 'python' | 'arduino'
}

export interface DeviceFileEntry {
  name: string
  path: string
  isDirectory: boolean
  size: number
}

interface AppState {
  // Project state
  projectPath: string | null
  projectFiles: ProjectFile[]
  currentFile: ProjectFile | null
  isDirty: boolean

  // Editor state
  editorState: EditorState

  // Connection state
  ports: PortInfo[]
  selectedPort: string
  detectedBoard: BoardType

  // Execution profile state (multi-interpreter selection engine)
  availableProfiles: ExecutionProfile[]
  activeProfile: ExecutionProfile | null
  workspaceMode: WorkspaceMode
  // True when the active profile was chosen explicitly by the user (vs auto-
  // detected). A manual choice survives re-detection on the same board; an auto
  // choice tracks the best detection. See resolveProfileOnDetect.
  profileManuallySet: boolean
  // Latest CIRCUITPY-drive probe result; feeds CircuitPython auto-detection.
  circuitPythonDrive: CircuitPythonDrive | null

  // Multi-tab editor state
  openTabs: OpenTab[]
  activeTabPath: string | null

  // Device file tree state
  deviceFiles: DeviceFileEntry[]
  deviceFilesLoading: boolean
  deviceConnected: boolean

  // Auto-connect state
  autoConnect: boolean
  lastPortPath: string
  lastProfileId: string
  connectionDropped: boolean

  // Manual board choices, keyed by a port's serial number (or path). Lets the
  // user pin the exact board when USB descriptors can't disambiguate it.
  boardOverrides: Record<string, string>

  // Flash state
  flashDialogOpen: boolean
  isFlashing: boolean
  isInstalling: boolean
  toolMissing: boolean
  toolMissingType: 'espressif' | 'arduino' | null

  // Log state
  logEntries: LogEntry[]
  dialogFlashLogs: LogEntry[]
  shellLogVisible: boolean

  // REPL state
  replConnected: boolean
  // True while a connect attempt is in flight. The device file tree watches this
  // to stop probing the port with `mpremote ls`, so it can't steal the port out
  // from under the connecting REPL.
  replConnecting: boolean
  replOutput: string
  replVisible: boolean

  // Combined bottom panel: 'shell' | 'repl' | 'hidden'
  bottomMode: 'shell' | 'repl' | 'hidden'

  // UI state
  welcomeScreenVisible: boolean
  settingsOpen: boolean

  // Recent projects
  recentProjects: { path: string; name: string; lastOpened: number }[]

  // Actions
  setProjectPath: (path: string | null) => void
  setProjectFiles: (files: ProjectFile[]) => void
  setCurrentFile: (file: ProjectFile | null) => void
  updateFileContent: (path: string, content: string) => void
  setIsDirty: (dirty: boolean) => void

  setEditorState: (state: Partial<EditorState>) => void

  setPorts: (ports: AppState['ports']) => void
  setSelectedPort: (port: string) => void
  setDetectedBoard: (board: BoardType) => void

  // Execution profile actions
  setAvailableProfiles: (profiles: ExecutionProfile[]) => void
  setActiveProfile: (profile: ExecutionProfile | null) => void
  selectProfile: (profileId: string) => void
  setWorkspaceMode: (mode: WorkspaceMode) => void
  setCircuitPythonDrive: (drive: CircuitPythonDrive | null) => void
  refreshProfiles: (portPath: string) => void

  // Multi-tab actions
  openTab: (tab: OpenTab) => void
  closeTab: (path: string) => void
  setActiveTab: (path: string | null) => void
  markTabClean: (path: string) => void
  markTabDirty: (path: string) => void

  // Device file tree actions
  setDeviceFiles: (files: DeviceFileEntry[]) => void
  setDeviceFilesLoading: (loading: boolean) => void
  setDeviceConnected: (connected: boolean) => void

  // Auto-connect actions
  setAutoConnect: (enabled: boolean) => void
  setLastPortPath: (path: string) => void
  setLastProfileId: (id: string) => void
  setConnectionDropped: (dropped: boolean) => void
  setBoardOverride: (portKey: string, boardId: string) => void

  setFlashDialogOpen: (open: boolean) => void
  setIsFlashing: (flashing: boolean) => void
  setIsInstalling: (installing: boolean) => void
  setToolMissing: (missing: boolean, type?: 'espressif' | 'arduino' | null) => void

  addLogEntry: (entry: Omit<LogEntry, 'id' | 'timestamp'>) => void
  clearLogEntries: () => void
  clearDialogFlashLogs: () => void
  setShellLogVisible: (visible: boolean) => void

  setReplConnected: (connected: boolean) => void
  setReplConnecting: (connecting: boolean) => void
  appendReplOutput: (data: string) => void
  clearReplOutput: () => void
  setReplVisible: (visible: boolean) => void

  setBottomMode: (mode: 'shell' | 'repl' | 'hidden') => void

  setWelcomeScreenVisible: (visible: boolean) => void
  setSettingsOpen: (open: boolean) => void

  addRecentProject: (path: string, name: string) => void
  removeRecentProject: (path: string) => void

  reset: () => void
}

const initialState = {
  projectPath: null,
  projectFiles: [],
  currentFile: null,
  isDirty: false,
  editorState: { line: 1, column: 1 },
  ports: [],
  selectedPort: '',
  detectedBoard: 'unknown' as BoardType,
  availableProfiles: [] as ExecutionProfile[],
  activeProfile: null as ExecutionProfile | null,
  profileManuallySet: false,
  circuitPythonDrive: null as CircuitPythonDrive | null,
  workspaceMode: 'micropython' as WorkspaceMode,
  openTabs: [] as OpenTab[],
  activeTabPath: null as string | null,
  deviceFiles: [] as DeviceFileEntry[],
  deviceFilesLoading: false,
  deviceConnected: false,
  autoConnect: true,
  lastPortPath: '',
  lastProfileId: '',
  connectionDropped: false,
  boardOverrides: {} as Record<string, string>,
  flashDialogOpen: false,
  isFlashing: false,
  isInstalling: false,
  toolMissing: false,
  toolMissingType: null,
  logEntries: [],
  dialogFlashLogs: [],
  shellLogVisible: true,
  replConnected: false,
  replConnecting: false,
  replOutput: '',
  replVisible: false,
  bottomMode: 'shell' as 'shell' | 'repl' | 'hidden',
  welcomeScreenVisible: true,
  settingsOpen: false,
  recentProjects: [],
}

export const useStore = create<AppState>()(
  persist(
    (set, get) => ({
      ...initialState,

      setProjectPath: (path) => set({ projectPath: path }),

      setProjectFiles: (files) => set({ projectFiles: files }),

      setCurrentFile: (file) => set({ currentFile: file, isDirty: false }),

      updateFileContent: (path, content) => {
        const { projectFiles, currentFile } = get()
        const updatedFiles = projectFiles.map(f =>
          f.path === path ? { ...f, content } : f
        )
        const updatedCurrent = currentFile?.path === path
          ? { ...currentFile, content }
          : currentFile
        set({ projectFiles: updatedFiles, currentFile: updatedCurrent, isDirty: true })
      },

      setIsDirty: (dirty) => set({ isDirty: dirty }),

      setEditorState: (state) => set((s) => ({
        editorState: { ...s.editorState, ...state }
      })),

      setPorts: (ports) => set({ ports }),
      setSelectedPort: (port) => set({ selectedPort: port }),
      setDetectedBoard: (board) => set({ detectedBoard: board }),

      setAvailableProfiles: (profiles) => set({ availableProfiles: profiles }),

      setActiveProfile: (profile) => {
        if (profile) {
          // Auto-selection (detection) — does not pin the choice.
          set({
            activeProfile: profile,
            workspaceMode: runtimeToWorkspaceMode(profile.runtime),
            lastProfileId: profile.id,
            profileManuallySet: false,
          })
        } else {
          set({ activeProfile: null, profileManuallySet: false })
        }
      },

      selectProfile: (profileId) => {
        const { availableProfiles } = get()
        const profile = availableProfiles.find((p) => p.id === profileId)
        if (profile) {
          // Explicit user choice — pin it so re-detection won't override it.
          set({
            activeProfile: profile,
            workspaceMode: runtimeToWorkspaceMode(profile.runtime),
            lastProfileId: profile.id,
            profileManuallySet: true,
          })
        }
      },

      setWorkspaceMode: (mode) => set({ workspaceMode: mode }),

      setCircuitPythonDrive: (drive) => set({ circuitPythonDrive: drive }),

      refreshProfiles: (portPath) => {
        const profiles = detectExecutionProfiles({ path: portPath, vendorId: '', manufacturer: '', productId: '' })
        set({ availableProfiles: profiles })
      },

      openTab: (tab) => {
        const { openTabs } = get()
        if (!openTabs.some((t) => t.path === tab.path)) {
          set({ openTabs: [...openTabs, tab] })
        }
        set({ activeTabPath: tab.path })
      },

      closeTab: (path) => {
        const { openTabs, activeTabPath } = get()
        const filtered = openTabs.filter((t) => t.path !== path)
        const newActive = activeTabPath === path
          ? (filtered.length > 0 ? filtered[filtered.length - 1].path : null)
          : activeTabPath
        set({ openTabs: filtered, activeTabPath: newActive })
      },

      setActiveTab: (path) => set({ activeTabPath: path }),

      markTabClean: (path) => {
        const { openTabs } = get()
        set({ openTabs: openTabs.map((t) => t.path === path ? { ...t, isDirty: false } : t) })
      },

      markTabDirty: (path) => {
        const { openTabs } = get()
        set({ openTabs: openTabs.map((t) => t.path === path ? { ...t, isDirty: true } : t) })
      },

      setDeviceFiles: (files) => set({ deviceFiles: files }),
      setDeviceFilesLoading: (loading) => set({ deviceFilesLoading: loading }),
      setDeviceConnected: (connected) => set({ deviceConnected: connected }),

      setAutoConnect: (enabled) => set({ autoConnect: enabled }),
      setLastPortPath: (path) => set({ lastPortPath: path }),
      setLastProfileId: (id) => set({ lastProfileId: id }),
      setConnectionDropped: (dropped) => set({ connectionDropped: dropped }),
      setBoardOverride: (portKey, boardId) =>
        set((state) => ({ boardOverrides: { ...state.boardOverrides, [portKey]: boardId } })),

      setFlashDialogOpen: (open) => set({ flashDialogOpen: open }),
      setIsFlashing: (flashing) => set({ isFlashing: flashing }),
      setIsInstalling: (installing) => set({ isInstalling: installing }),
      setToolMissing: (missing, type) => set({ toolMissing: missing, toolMissingType: type ?? null }),

      addLogEntry: (entry) => {
        const { logEntries, dialogFlashLogs } = get()
        const newEntry: LogEntry = {
          ...entry,
          id: String(Date.now() + Math.random()),
          timestamp: new Date(),
        }
        set({
          logEntries: [...logEntries, newEntry],
          dialogFlashLogs: [...dialogFlashLogs, newEntry],
        })
      },

      clearLogEntries: () => set({ logEntries: [] }),
      clearDialogFlashLogs: () => set({ dialogFlashLogs: [] }),
      setShellLogVisible: (visible) => set({ shellLogVisible: visible }),

      setReplConnected: (connected) => set({ replConnected: connected }),
      setReplConnecting: (connecting) => set({ replConnecting: connecting }),
      appendReplOutput: (data) => set((s) => ({ replOutput: s.replOutput + data })),
      clearReplOutput: () => set({ replOutput: '' }),
      setReplVisible: (visible) => set({ replVisible: visible }),
      setBottomMode: (mode) => set({ bottomMode: mode }),

      setWelcomeScreenVisible: (visible) => set({ welcomeScreenVisible: visible }),
      setSettingsOpen: (open) => set({ settingsOpen: open }),

      addRecentProject: (path, name) => {
        const { recentProjects } = get()
        const filtered = recentProjects.filter(p => p.path !== path)
        const updated = [
          { path, name, lastOpened: Date.now() },
          ...filtered
        ].slice(0, 10) // Keep only 10 most recent
        set({ recentProjects: updated })
      },

      removeRecentProject: (path) => {
        const { recentProjects } = get()
        set({ recentProjects: recentProjects.filter(p => p.path !== path) })
      },

      reset: () => set(initialState),
    }),
    {
      name: 'anode-storage',
      partialize: (state) => ({
        recentProjects: state.recentProjects,
        shellLogVisible: state.shellLogVisible,
        selectedPort: state.selectedPort,
        replVisible: state.replVisible,
        bottomMode: state.bottomMode,
        autoConnect: state.autoConnect,
        lastPortPath: state.lastPortPath,
        lastProfileId: state.lastProfileId,
        boardOverrides: state.boardOverrides,
      }),
    }
  )
)

// Detect board type from port info
export function detectBoardType(port: { vendorId?: string; manufacturer?: string; productId?: string; path?: string }): BoardType {
  const vid = port.vendorId?.toLowerCase() ?? ''
  const mfr = port.manufacturer?.toLowerCase() ?? ''
  const pid = port.productId?.toLowerCase() ?? ''
  const path = port.path?.toLowerCase() ?? ''

  // M5Stack Atom Light: CP2104 USB-UART bridge (VID 10c4, PID ea60) on ESP32 PICO-D4
  if (vid === '10c4' && pid === 'ea60') return 'm5stack-atom'
  // M5StampS3: ESP32-S3 native USB (VID 303a, PID 80??)
  if (vid === '303a' && pid.startsWith('80')) return 'm5stamp-s3'
  if (vid === '10c4' || vid === '1a86' || vid === '303a' || mfr.includes('silicon labs') || mfr.includes('espressif')) {
    return 'espressif'
  }
  if (vid === '2e8a' || mfr.includes('raspberry') || mfr.includes('rp2040') || pid === '0005') {
    return 'raspberry-pi'
  }
  if (vid === '2341' || vid === '2a03' || mfr.includes('arduino') || mfr.includes('genuino')) {
    return 'arduino'
  }

  // Path-based fallback when VID/PID aren't populated (some macOS drivers omit them)
  // CP210x on macOS: /dev/tty.usbserial-* or /dev/cu.usbserial-*
  if (/usbserial-[a-z0-9]+$/i.test(path)) return 'espressif'
  // CH340 on macOS: /dev/tty.usbmod* or /dev/cu.usbmod* (NOT usbmodem — that's ambiguous)
  if (/usbmod[a-z0-9]*$/i.test(path) && !/usbmodem/i.test(path)) return 'espressif'
  // macOS usbmodem — ambiguous: could be Arduino OR ESP32-S3 native USB CDC
  // If VID wasn't matched above, we can't distinguish. Default to espressif
  // since ESP32-S3 native USB is the more common cause of usbmodem on modern boards.
  // Arduino typically uses usbmodem with VID 2341 which is already matched above.
  if (/usbmodem/i.test(path)) return 'espressif'
  // Linux ACM devices — ambiguous (Arduino, Pico, ESP32-S3). Default to arduino
  if (/ttyacm\d+$/i.test(path)) return 'arduino'
  // Raspberry Pi Pico mount on macOS/Linux often shows as ttyACM* or usbmodem* with "RPI" in path
  if (/rp2040|pico|rpi-rp2/i.test(path)) return 'raspberry-pi'

  return 'unknown'
}

export function isLikelyHardwarePort(port: PortInfo): boolean {
  const path = port.path.toLowerCase()
  const manufacturer = port.manufacturer?.toLowerCase() ?? ''

  if (detectBoardType(port) !== 'unknown') return true

  if (
    path.includes('debug-console') ||
    path.includes('bluetooth') ||
    path.includes('incoming-port') ||
    path.includes('airpods') ||
    path.includes('redmibuds') ||
    manufacturer.includes('bluetooth')
  ) {
    return false
  }

  return (
    path.includes('usb') ||
    path.includes('acm') ||
    path.includes('serial') ||
    path.includes('modem') ||
    path.includes('uart')
  )
}

export function getPreferredPort(ports: PortInfo[]): PortInfo | null {
  if (ports.length === 0) return null

  const likelyPorts = ports.filter(isLikelyHardwarePort)
  if (likelyPorts.length === 0) return null

  const knownBoard = likelyPorts.find((port) => detectBoardType(port) !== 'unknown')
  if (knownBoard) return knownBoard

  const cuPort = likelyPorts.find((port) => port.path.startsWith('/dev/cu.'))
  return cuPort ?? likelyPorts[0] ?? null
}

// The CircuitPython detection hint from the latest CIRCUITPY-drive probe, in the
// shape detectExecutionProfiles expects. Read at every detection call site so a
// mounted CIRCUITPY drive steers detection toward CircuitPython.
export function circuitPythonHint(): ProfileDetectionInput['circuitPython'] {
  const drive = useStore.getState().circuitPythonDrive
  return drive?.present ? { boardName: drive.boardName, version: drive.version } : undefined
}

export function detectExecutionProfiles(input: ProfileDetectionInput): ExecutionProfile[] {
  const vid = (input.vendorId ?? '').toLowerCase()
  const pid = (input.productId ?? '').toLowerCase()
  const mfr = (input.manufacturer ?? '').toLowerCase()
  const portPath = input.path ?? ''

  const profiles: ExecutionProfile[] = []
  const seen = new Set<string>()

  const addProfile = (sig: KnownBoardSignatureLike, confidence: 'high' | 'low') => {
    const id = `${sig.runtime.toLowerCase()}-${sig.variant.toLowerCase().replace(/[^a-z0-9]/g, '-')}-${portPath}`
    if (seen.has(id)) return
    seen.add(id)
    profiles.push({
      id,
      runtime: sig.runtime,
      variant: sig.variant,
      boardName: sig.boardName,
      portPath,
      fqbn: sig.fqbn,
      isGeneric: false,
      confidence,
    })
  }

  // A mounted CIRCUITPY drive is the strongest signal — add it first so it wins
  // selectBestProfile over any USB-descriptor match (e.g. a bare ESP32-S3 that
  // would otherwise be guessed as MicroPython).
  if (input.circuitPython) {
    const name = input.circuitPython.boardName
    addProfile(
      {
        runtime: 'CircuitPython',
        variant: name ?? 'CircuitPython',
        boardName: name ? `${name} (CircuitPython)` : 'CircuitPython board',
      },
      'high',
    )
  }

  for (const sig of KNOWN_BOARD_SIGNATURES) {
    if (sig.vid !== vid) continue
    if (sig.pidPrefix && !pid.startsWith(sig.pidPrefix)) continue
    addProfile(sig, 'high')
  }

  if (mfr.includes('arduino') || mfr.includes('genuino')) {
    addProfile(
      { runtime: 'Arduino', variant: 'AVR', boardName: 'Arduino (by manufacturer)', fqbn: 'arduino:avr:uno' },
      'high',
    )
  }
  if (mfr.includes('raspberry') || mfr.includes('rp2040') || /pico|rp2040|rpi-rp2/i.test(portPath)) {
    addProfile({ runtime: 'MicroPython', variant: 'RP2040', boardName: 'Raspberry Pi Pico (by path)' }, 'high')
  }

  for (const generic of GENERIC_PROFILES) {
    const id = `${generic.id}-${portPath}`
    if (seen.has(id)) continue
    seen.add(id)
    profiles.push({ ...generic, portPath })
  }

  return profiles
}

interface KnownBoardSignatureLike {
  runtime: RuntimeEnvironment
  variant: string
  boardName: string
  fqbn?: string
}

export function getActiveWorkspaceLayout(): WorkspaceLayout {
  const mode = useStore.getState().workspaceMode
  return getWorkspaceLayout(mode)
}

// Decide which profile should be active after a port/board (re)detection, given
// the currently-active one and the freshly-detected profiles. Pure so it can be
// unit-tested as the board-swap/runtime-follow backbone. Rules:
//  - no active profile (fresh launch) → restore the saved one, else the best.
//  - a MANUAL pick is preserved as long as it's still valid for this port; only a
//    genuine board swap (its id no longer in the detected set) replaces it.
//  - an AUTO-selected profile simply tracks the current best detection, so it
//    follows board swaps and stronger signals (e.g. a CIRCUITPY drive mounting,
//    which makes CircuitPython the best match).
export function resolveProfileOnDetect(opts: {
  current: ExecutionProfile | null
  profiles: ExecutionProfile[]
  savedId?: string | null
  manual?: boolean
}): ExecutionProfile | null {
  const { current, profiles, savedId, manual } = opts
  const best = selectBestProfile(profiles)
  if (!current) {
    const saved = savedId ? profiles.find((p) => p.id === savedId) : undefined
    return saved ?? best
  }
  if (manual) {
    const stillValid = profiles.some((p) => p.id === current.id)
    return stillValid ? current : (best ?? current)
  }
  return best ?? current
}

export function selectBestProfile(profiles: ExecutionProfile[]): ExecutionProfile | null {
  if (profiles.length === 0) return null
  const high = profiles.filter((p) => p.confidence === 'high' && !p.isGeneric)
  if (high.length > 0) return high[0]
  const nonGeneric = profiles.filter((p) => !p.isGeneric)
  if (nonGeneric.length > 0) return nonGeneric[0]
  return profiles[0]
}
