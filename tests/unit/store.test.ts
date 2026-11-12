import { describe, it, expect, beforeEach } from 'vitest'
import { useStore, detectBoardType, getPreferredPort, isLikelyHardwarePort } from '@/lib/store'
import type { BoardType } from '@/lib/store'

describe('useStore', () => {
  beforeEach(() => {
    useStore.getState().reset()
  })

  describe('initial state', () => {
    it('has correct default values', () => {
      const state = useStore.getState()
      expect(state.projectPath).toBeNull()
      expect(state.projectFiles).toEqual([])
      expect(state.currentFile).toBeNull()
      expect(state.isDirty).toBe(false)
      expect(state.editorState).toEqual({ line: 1, column: 1 })
      expect(state.ports).toEqual([])
      expect(state.selectedPort).toBe('')
      expect(state.detectedBoard).toBe('unknown')
      expect(state.flashDialogOpen).toBe(false)
      expect(state.isFlashing).toBe(false)
      expect(state.isInstalling).toBe(false)
      expect(state.toolMissing).toBe(false)
      expect(state.toolMissingType).toBeNull()
      expect(state.logEntries).toEqual([])
      expect(state.dialogFlashLogs).toEqual([])
      expect(state.shellLogVisible).toBe(true)
      expect(state.welcomeScreenVisible).toBe(true)
      expect(state.settingsOpen).toBe(false)
      expect(state.recentProjects).toEqual([])
    })
  })

  describe('project state', () => {
    it('setProjectPath sets the path', () => {
      useStore.getState().setProjectPath('/tmp/my-project')
      expect(useStore.getState().projectPath).toBe('/tmp/my-project')
    })

    it('setProjectPath can clear to null', () => {
      useStore.getState().setProjectPath('/tmp/x')
      useStore.getState().setProjectPath(null)
      expect(useStore.getState().projectPath).toBeNull()
    })

    it('setProjectFiles sets the files array', () => {
      const files = [
        { name: 'main.py', path: '/tmp/main.py', content: 'print("hi")', language: 'python' as const },
      ]
      useStore.getState().setProjectFiles(files)
      expect(useStore.getState().projectFiles).toEqual(files)
    })

    it('setCurrentFile sets file and resets isDirty', () => {
      useStore.getState().setIsDirty(true)
      const file = { name: 'main.py', path: '/tmp/main.py', content: 'x', language: 'python' as const }
      useStore.getState().setCurrentFile(file)
      expect(useStore.getState().currentFile).toEqual(file)
      expect(useStore.getState().isDirty).toBe(false)
    })

    it('setCurrentFile can clear to null', () => {
      const file = { name: 'a.py', path: '/a.py', content: '', language: 'python' as const }
      useStore.getState().setCurrentFile(file)
      useStore.getState().setCurrentFile(null)
      expect(useStore.getState().currentFile).toBeNull()
    })

    it('updateFileContent updates matching file in projectFiles', () => {
      const files = [
        { name: 'a.py', path: '/a.py', content: 'old', language: 'python' as const },
        { name: 'b.py', path: '/b.py', content: 'keep', language: 'python' as const },
      ]
      useStore.getState().setProjectFiles(files)
      useStore.getState().setCurrentFile(files[0])
      useStore.getState().updateFileContent('/a.py', 'new content')
      expect(useStore.getState().projectFiles[0].content).toBe('new content')
      expect(useStore.getState().projectFiles[1].content).toBe('keep')
    })

    it('updateFileContent updates currentFile if path matches', () => {
      const file = { name: 'a.py', path: '/a.py', content: 'old', language: 'python' as const }
      useStore.getState().setProjectFiles([file])
      useStore.getState().setCurrentFile(file)
      useStore.getState().updateFileContent('/a.py', 'updated')
      expect(useStore.getState().currentFile?.content).toBe('updated')
    })

    it('updateFileContent does not update currentFile if path differs', () => {
      const fileA = { name: 'a.py', path: '/a.py', content: 'a', language: 'python' as const }
      const fileB = { name: 'b.py', path: '/b.py', content: 'b', language: 'python' as const }
      useStore.getState().setProjectFiles([fileA, fileB])
      useStore.getState().setCurrentFile(fileA)
      useStore.getState().updateFileContent('/b.py', 'b-updated')
      expect(useStore.getState().currentFile?.content).toBe('a')
    })

    it('updateFileContent sets isDirty to true', () => {
      const file = { name: 'a.py', path: '/a.py', content: 'old', language: 'python' as const }
      useStore.getState().setProjectFiles([file])
      useStore.getState().setCurrentFile(file)
      useStore.getState().updateFileContent('/a.py', 'new')
      expect(useStore.getState().isDirty).toBe(true)
    })

    it('setIsDirty toggles dirty state', () => {
      useStore.getState().setIsDirty(true)
      expect(useStore.getState().isDirty).toBe(true)
      useStore.getState().setIsDirty(false)
      expect(useStore.getState().isDirty).toBe(false)
    })
  })

  describe('editor state', () => {
    it('setEditorState updates line and column', () => {
      useStore.getState().setEditorState({ line: 10, column: 5 })
      expect(useStore.getState().editorState).toEqual({ line: 10, column: 5 })
    })

    it('setEditorState merges partial updates', () => {
      useStore.getState().setEditorState({ line: 10, column: 5 })
      useStore.getState().setEditorState({ column: 20 })
      expect(useStore.getState().editorState).toEqual({ line: 10, column: 20 })
    })
  })

  describe('connection state', () => {
    it('setPorts sets available ports', () => {
      const ports = [{ path: '/dev/ttyUSB0', manufacturer: 'Espressif', vendorId: '10c4' }]
      useStore.getState().setPorts(ports)
      expect(useStore.getState().ports).toEqual(ports)
    })

    it('setSelectedPort sets the port', () => {
      useStore.getState().setSelectedPort('/dev/ttyUSB0')
      expect(useStore.getState().selectedPort).toBe('/dev/ttyUSB0')
    })

    it('setDetectedBoard sets the board type', () => {
      useStore.getState().setDetectedBoard('espressif')
      expect(useStore.getState().detectedBoard).toBe('espressif')
    })

    it('setDetectedBoard supports m5stack-atom', () => {
      useStore.getState().setDetectedBoard('m5stack-atom')
      expect(useStore.getState().detectedBoard).toBe('m5stack-atom')
    })

    it('setDetectedBoard supports m5stamp-s3', () => {
      useStore.getState().setDetectedBoard('m5stamp-s3')
      expect(useStore.getState().detectedBoard).toBe('m5stamp-s3')
    })
  })

  describe('flash state', () => {
    it('setFlashDialogOpen toggles the dialog', () => {
      useStore.getState().setFlashDialogOpen(true)
      expect(useStore.getState().flashDialogOpen).toBe(true)
      useStore.getState().setFlashDialogOpen(false)
      expect(useStore.getState().flashDialogOpen).toBe(false)
    })

    it('setIsFlashing toggles flashing state', () => {
      useStore.getState().setIsFlashing(true)
      expect(useStore.getState().isFlashing).toBe(true)
    })

    it('setIsInstalling toggles installing state', () => {
      useStore.getState().setIsInstalling(true)
      expect(useStore.getState().isInstalling).toBe(true)
    })

    it('setToolMissing sets missing state and type', () => {
      useStore.getState().setToolMissing(true, 'espressif')
      expect(useStore.getState().toolMissing).toBe(true)
      expect(useStore.getState().toolMissingType).toBe('espressif')
    })

    it('setToolMissing defaults type to null when not provided', () => {
      useStore.getState().setToolMissing(true)
      expect(useStore.getState().toolMissingType).toBeNull()
    })

    it('setToolMissing can clear missing state', () => {
      useStore.getState().setToolMissing(true, 'arduino')
      useStore.getState().setToolMissing(false)
      expect(useStore.getState().toolMissing).toBe(false)
      expect(useStore.getState().toolMissingType).toBeNull()
    })
  })

  describe('log state', () => {
    it('addLogEntry appends entry with id and timestamp', () => {
      useStore.getState().addLogEntry({ type: 'info', message: 'hello' })
      const entries = useStore.getState().logEntries
      expect(entries).toHaveLength(1)
      expect(entries[0].type).toBe('info')
      expect(entries[0].message).toBe('hello')
      expect(entries[0].id).toBeDefined()
      expect(entries[0].timestamp).toBeInstanceOf(Date)
    })

    it('addLogEntry adds to both logEntries and dialogFlashLogs', () => {
      useStore.getState().addLogEntry({ type: 'flash', message: 'flashing...' })
      expect(useStore.getState().logEntries).toHaveLength(1)
      expect(useStore.getState().dialogFlashLogs).toHaveLength(1)
    })

    it('addLogEntry appends multiple entries in order', () => {
      useStore.getState().addLogEntry({ type: 'info', message: 'first' })
      useStore.getState().addLogEntry({ type: 'error', message: 'second' })
      const entries = useStore.getState().logEntries
      expect(entries).toHaveLength(2)
      expect(entries[0].message).toBe('first')
      expect(entries[1].message).toBe('second')
    })

    it('clearLogEntries clears only main log', () => {
      useStore.getState().addLogEntry({ type: 'info', message: 'test' })
      useStore.getState().clearLogEntries()
      expect(useStore.getState().logEntries).toEqual([])
      expect(useStore.getState().dialogFlashLogs).toHaveLength(1)
    })

    it('clearDialogFlashLogs clears only dialog logs', () => {
      useStore.getState().addLogEntry({ type: 'info', message: 'test' })
      useStore.getState().clearDialogFlashLogs()
      expect(useStore.getState().dialogFlashLogs).toEqual([])
      expect(useStore.getState().logEntries).toHaveLength(1)
    })

    it('setShellLogVisible toggles visibility', () => {
      useStore.getState().setShellLogVisible(false)
      expect(useStore.getState().shellLogVisible).toBe(false)
      useStore.getState().setShellLogVisible(true)
      expect(useStore.getState().shellLogVisible).toBe(true)
    })
  })

  describe('UI state', () => {
    it('setWelcomeScreenVisible toggles welcome screen', () => {
      useStore.getState().setWelcomeScreenVisible(false)
      expect(useStore.getState().welcomeScreenVisible).toBe(false)
    })

    it('setSettingsOpen toggles settings', () => {
      useStore.getState().setSettingsOpen(true)
      expect(useStore.getState().settingsOpen).toBe(true)
    })
  })

  describe('recent projects', () => {
    it('addRecentProject adds a project', () => {
      useStore.getState().addRecentProject('/tmp/proj1', 'Proj1')
      const projects = useStore.getState().recentProjects
      expect(projects).toHaveLength(1)
      expect(projects[0].path).toBe('/tmp/proj1')
      expect(projects[0].name).toBe('Proj1')
      expect(projects[0].lastOpened).toBeTypeOf('number')
    })

    it('addRecentProject moves existing project to top', () => {
      useStore.getState().addRecentProject('/tmp/a', 'A')
      useStore.getState().addRecentProject('/tmp/b', 'B')
      useStore.getState().addRecentProject('/tmp/a', 'A')
      const projects = useStore.getState().recentProjects
      expect(projects).toHaveLength(2)
      expect(projects[0].path).toBe('/tmp/a')
    })

    it('addRecentProject caps at 10 entries', () => {
      for (let i = 0; i < 15; i++) {
        useStore.getState().addRecentProject(`/tmp/p${i}`, `P${i}`)
      }
      expect(useStore.getState().recentProjects).toHaveLength(10)
    })

    it('addRecentProject updates name for existing path', () => {
      useStore.getState().addRecentProject('/tmp/x', 'OldName')
      useStore.getState().addRecentProject('/tmp/x', 'NewName')
      const project = useStore.getState().recentProjects.find(p => p.path === '/tmp/x')
      expect(project?.name).toBe('NewName')
    })

    it('removeRecentProject removes by path', () => {
      useStore.getState().addRecentProject('/tmp/a', 'A')
      useStore.getState().addRecentProject('/tmp/b', 'B')
      useStore.getState().removeRecentProject('/tmp/a')
      const projects = useStore.getState().recentProjects
      expect(projects).toHaveLength(1)
      expect(projects[0].path).toBe('/tmp/b')
    })

    it('removeRecentProject is no-op for non-existent path', () => {
      useStore.getState().addRecentProject('/tmp/a', 'A')
      useStore.getState().removeRecentProject('/tmp/nonexistent')
      expect(useStore.getState().recentProjects).toHaveLength(1)
    })
  })

  describe('reset', () => {
    it('resets all state to initial values', () => {
      const store = useStore.getState()
      store.setProjectPath('/tmp/x')
      store.setProjectFiles([{ name: 'a.py', path: '/a.py', content: 'x', language: 'python' }])
      store.setCurrentFile({ name: 'a.py', path: '/a.py', content: 'x', language: 'python' })
      store.setIsDirty(true)
      store.setEditorState({ line: 50, column: 20 })
      store.setSelectedPort('/dev/ttyUSB0')
      store.setDetectedBoard('espressif')
      store.setFlashDialogOpen(true)
      store.addLogEntry({ type: 'info', message: 'test' })
      store.setSettingsOpen(true)

      store.reset()

      const after = useStore.getState()
      expect(after.projectPath).toBeNull()
      expect(after.projectFiles).toEqual([])
      expect(after.currentFile).toBeNull()
      expect(after.isDirty).toBe(false)
      expect(after.editorState).toEqual({ line: 1, column: 1 })
      expect(after.selectedPort).toBe('')
      expect(after.detectedBoard).toBe('unknown')
      expect(after.flashDialogOpen).toBe(false)
      expect(after.logEntries).toEqual([])
      expect(after.settingsOpen).toBe(false)
    })
  })
})

describe('detectBoardType', () => {
  it('detects espressif by vendorId 10c4', () => {
    expect(detectBoardType({ vendorId: '10c4' })).toBe('espressif')
  })

  it('detects m5stack-atom by vendorId 10c4 + productId ea60', () => {
    expect(detectBoardType({ vendorId: '10c4', productId: 'ea60' })).toBe('m5stack-atom')
  })

  it('detects m5stamp-s3 by vendorId 303a + productId starting with 80', () => {
    expect(detectBoardType({ vendorId: '303a', productId: '8010' })).toBe('m5stamp-s3')
  })

  it('detects m5stamp-s3 by vendorId 303a + productId 80', () => {
    expect(detectBoardType({ vendorId: '303a', productId: '80' })).toBe('m5stamp-s3')
  })

  it('does not detect m5stamp-s3 for vendorId 303a with non-80 productId', () => {
    expect(detectBoardType({ vendorId: '303a', productId: '0001' })).toBe('espressif')
  })

  it('detects espressif by vendorId 1a86', () => {
    expect(detectBoardType({ vendorId: '1a86' })).toBe('espressif')
  })

  it('detects espressif by vendorId 303a', () => {
    expect(detectBoardType({ vendorId: '303a' })).toBe('espressif')
  })

  it('detects espressif by manufacturer containing "silicon labs"', () => {
    expect(detectBoardType({ manufacturer: 'Silicon Labs CP210x' })).toBe('espressif')
  })

  it('detects espressif by manufacturer containing "espressif"', () => {
    expect(detectBoardType({ manufacturer: 'Espressif Systems' })).toBe('espressif')
  })

  it('detects raspberry-pi by vendorId 2e8a', () => {
    expect(detectBoardType({ vendorId: '2e8a' })).toBe('raspberry-pi')
  })

  it('detects raspberry-pi by manufacturer containing "raspberry"', () => {
    expect(detectBoardType({ manufacturer: 'Raspberry Pi Foundation' })).toBe('raspberry-pi')
  })

  it('detects raspberry-pi by manufacturer containing "rp2040"', () => {
    expect(detectBoardType({ manufacturer: 'RP2040 USB Device' })).toBe('raspberry-pi')
  })

  it('detects raspberry-pi by productId 0005', () => {
    expect(detectBoardType({ productId: '0005' })).toBe('raspberry-pi')
  })

  it('detects arduino by vendorId 2341', () => {
    expect(detectBoardType({ vendorId: '2341' })).toBe('arduino')
  })

  it('detects arduino by vendorId 2a03', () => {
    expect(detectBoardType({ vendorId: '2a03' })).toBe('arduino')
  })

  it('detects arduino by manufacturer containing "arduino"', () => {
    expect(detectBoardType({ manufacturer: 'Arduino LLC' })).toBe('arduino')
  })

  it('detects arduino by manufacturer containing "genuino"', () => {
    expect(detectBoardType({ manufacturer: 'Genuino Plc' })).toBe('arduino')
  })

  it('returns unknown for unrecognized ports', () => {
    expect(detectBoardType({ vendorId: 'abcd', manufacturer: 'Unknown' })).toBe('unknown')
  })

  it('returns unknown for empty port info', () => {
    expect(detectBoardType({})).toBe('unknown')
  })

  it('is case-insensitive for vendorId', () => {
    expect(detectBoardType({ vendorId: '10C4' })).toBe('espressif')
  })

  it('is case-insensitive for manufacturer', () => {
    expect(detectBoardType({ manufacturer: 'ESPRESSIF SYSTEMS' })).toBe('espressif')
  })

  it('prioritizes m5stack-atom over raspberry-pi when VID+PID match', () => {
    expect(detectBoardType({ vendorId: '10c4', productId: 'ea60', manufacturer: 'Raspberry Pi' })).toBe('m5stack-atom')
  })

  it('prioritizes espressif over raspberry-pi when only VID matches', () => {
    expect(detectBoardType({ vendorId: '10c4', manufacturer: 'Raspberry Pi' })).toBe('espressif')
  })

  it('detects board with only vendorId set', () => {
    expect(detectBoardType({ vendorId: '2e8a', manufacturer: undefined, productId: undefined })).toBe('raspberry-pi')
  })

  it('handles undefined fields gracefully', () => {
    const result: BoardType = detectBoardType({ vendorId: undefined, manufacturer: undefined, productId: undefined })
    expect(result).toBe('unknown')
  })
})

describe('hardware port selection', () => {
  it('rejects macOS debug console port', () => {
    expect(isLikelyHardwarePort({ path: '/dev/tty.debug-console' })).toBe(false)
  })

  it('accepts usb modem port', () => {
    expect(isLikelyHardwarePort({ path: '/dev/cu.usbmodem11301' })).toBe(true)
  })

  it('prefers known hardware port over debug console', () => {
    const port = getPreferredPort([
      { path: '/dev/tty.debug-console' },
      { path: '/dev/cu.usbmodem11301', manufacturer: 'Espressif', vendorId: '303a', productId: '8132' },
    ])

    expect(port?.path).toBe('/dev/cu.usbmodem11301')
  })
})
