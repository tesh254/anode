import { useState, useEffect, useRef, useMemo } from 'react'
import { useStore } from '@/lib/store'
import { TEMPLATES, Template } from '@/lib/templates'
import {
  Search, FileCode2, FolderOpen, Plus, Settings, Terminal,
  Upload, Square, Play, BookOpen
} from 'lucide-react'
import { toast } from 'sonner'

interface CommandItem {
  id: string
  title: string
  subtitle?: string
  icon: React.ReactNode
  category: 'action' | 'file' | 'project' | 'template'
  action: () => void
}

interface CommandPaletteProps {
  open: boolean
  onClose: () => void
  onOpenProject: (path: string) => void
  onCreateFromTemplate: (template: Template, projectName: string, projectPath: string) => void
  onSaveFile: () => void
  onSaveAllFiles: () => void
  onRun: () => void
  onUpload: () => void
  onStop: () => void
  onOpenFlash: () => void
}

export function CommandPalette({
  open,
  onClose,
  onOpenProject,
  onCreateFromTemplate,
  onSaveFile,
  onSaveAllFiles,
  onRun,
  onUpload,
  onStop,
  onOpenFlash,
}: CommandPaletteProps) {
  const {
    projectFiles,
    recentProjects,
    bottomMode,
    setBottomMode,
    setWelcomeScreenVisible,
    setSettingsOpen,
    setCurrentFile,
  } = useStore()

  const [query, setQuery] = useState('')
  const [selectedIndex, setSelectedIndex] = useState(0)
  const inputRef = useRef<HTMLInputElement>(null)
  const listRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (open) {
      setQuery('')
      setSelectedIndex(0)
      setTimeout(() => inputRef.current?.focus(), 50)
    }
  }, [open])

  const handleOpenFolder = async () => {
    const hw = (window as unknown as { hardware: { openFolder: () => Promise<string | null> } }).hardware
    if (!hw?.openFolder) return
    const path = await hw.openFolder()
    if (path) {
      toast.success('Opening folder...')
      onOpenProject(path)
    }
  }

  const commands = useMemo<CommandItem[]>(() => {
    const items: CommandItem[] = []

    // Actions
    items.push({
      id: 'new-project',
      title: 'New Project',
      subtitle: 'Create a new project from a template',
      icon: <Plus className="size-4" />,
      category: 'action',
      action: () => { onClose(); setWelcomeScreenVisible(true) },
    })
    items.push({
      id: 'open-folder',
      title: 'Open Folder',
      subtitle: 'Open an existing project folder',
      icon: <FolderOpen className="size-4" />,
      category: 'action',
      action: () => { onClose(); handleOpenFolder() },
    })
    items.push({
      id: 'settings',
      title: 'Open Settings',
      subtitle: 'Configure application settings',
      icon: <Settings className="size-4" />,
      category: 'action',
      action: () => { onClose(); setSettingsOpen(true) },
    })
    items.push({
      id: 'toggle-shell',
      title: bottomMode === 'shell' ? 'Hide Shell Log' : 'Show Shell Log',
      subtitle: 'Toggle the shell log panel',
      icon: <Terminal className="size-4" />,
      category: 'action',
      action: () => { onClose(); setBottomMode(bottomMode === 'shell' ? 'hidden' : 'shell') },
    })
    items.push({
      id: 'toggle-repl',
      title: bottomMode === 'repl' ? 'Hide REPL' : 'Show REPL',
      subtitle: 'Toggle the interactive REPL panel',
      icon: <Terminal className="size-4" />,
      category: 'action',
      action: () => { onClose(); setBottomMode(bottomMode === 'repl' ? 'hidden' : 'repl') },
    })
    items.push({
      id: 'save-file',
      title: 'Save File',
      subtitle: 'Save the current file (Cmd+S)',
      icon: <FileCode2 className="size-4" />,
      category: 'action',
      action: () => { onClose(); onSaveFile() },
    })
    items.push({
      id: 'save-all-files',
      title: 'Save All Files',
      subtitle: 'Write all project files to disk before deploy',
      icon: <FileCode2 className="size-4" />,
      category: 'action',
      action: () => { onClose(); onSaveAllFiles() },
    })
    items.push({
      id: 'run-code',
      title: 'Run',
      subtitle: 'Save current file, then run on connected board',
      icon: <Play className="size-4" />,
      category: 'action',
      action: () => { onClose(); void onRun() },
    })
    items.push({
      id: 'upload-firmware',
      title: 'Flash Firmware',
      subtitle: 'Open firmware flash dialog for connected board',
      icon: <Upload className="size-4" />,
      category: 'action',
      action: () => { onClose(); onOpenFlash() },
    })
    items.push({
      id: 'upload-project',
      title: 'Upload Project',
      subtitle: 'Save all files, then sync project to connected board',
      icon: <Upload className="size-4" />,
      category: 'action',
      action: () => { onClose(); void onUpload() },
    })
    items.push({
      id: 'stop-execution',
      title: 'Stop',
      subtitle: 'Stop the currently running code',
      icon: <Square className="size-4" />,
      category: 'action',
      action: () => { onClose(); void onStop() },
    })

    // Recent projects
    recentProjects.forEach((project) => {
      items.push({
        id: `project-${project.path}`,
        title: project.name,
        subtitle: project.path,
        icon: <FolderOpen className="size-4" />,
        category: 'project',
        action: () => { onClose(); onOpenProject(project.path) },
      })
    })

    // Project files
    projectFiles.forEach((file) => {
      items.push({
        id: `file-${file.path}`,
        title: file.name,
        subtitle: file.path,
        icon: <FileCode2 className="size-4" />,
        category: 'file',
        action: () => { onClose(); setCurrentFile(file) },
      })
    })

    // Templates
    TEMPLATES.slice(0, 5).forEach((template) => {
      items.push({
        id: `template-${template.id}`,
        title: `New: ${template.name}`,
        subtitle: template.description,
        icon: <BookOpen className="size-4" />,
        category: 'template',
        action: () => {
          onClose()
          const name = template.name.replace(/\s+/g, '-').toLowerCase() + '-project'
          onCreateFromTemplate(template, name, `/tmp/${name}`)
        },
      })
    })

    return items
  }, [projectFiles, recentProjects, bottomMode, onOpenProject, onCreateFromTemplate, onClose, setBottomMode, setWelcomeScreenVisible, setSettingsOpen, setCurrentFile, onSaveFile, onSaveAllFiles, onRun, onOpenFlash, onUpload, onStop])

  const filteredCommands = useMemo(() => {
    if (!query.trim()) return commands
    const lower = query.toLowerCase()
    return commands.filter(cmd =>
      cmd.title.toLowerCase().includes(lower) ||
      cmd.subtitle?.toLowerCase().includes(lower)
    )
  }, [commands, query])

  useEffect(() => {
    setSelectedIndex(0)
  }, [query])

  useEffect(() => {
    if (listRef.current) {
      const selected = listRef.current.children[selectedIndex] as HTMLElement
      selected?.scrollIntoView({ block: 'nearest' })
    }
  }, [selectedIndex])

  const handleKeyDown = (e: React.KeyboardEvent) => {
    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault()
        setSelectedIndex(i => Math.min(i + 1, filteredCommands.length - 1))
        break
      case 'ArrowUp':
        e.preventDefault()
        setSelectedIndex(i => Math.max(i - 1, 0))
        break
      case 'Enter':
        e.preventDefault()
        if (filteredCommands[selectedIndex]) {
          filteredCommands[selectedIndex].action()
        }
        break
      case 'Escape':
        e.preventDefault()
        onClose()
        break
    }
  }

  if (!open) return null

  return (
    <div className="fixed inset-0 z-[100] flex items-start justify-center pt-[15vh]">
      {/* Backdrop */}
      <div className="absolute inset-0 bg-black/50" onClick={onClose} />
      
      {/* Palette */}
      <div className="relative w-full max-w-xl bg-[#1a1a1a] border border-white/10 rounded-xl shadow-2xl overflow-hidden">
        {/* Search input */}
        <div className="flex items-center gap-3 px-4 py-3 border-b border-white/10">
          <Search className="size-5 text-gray-500 shrink-0" />
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Search commands, files, projects..."
            className="flex-1 bg-transparent text-white placeholder-gray-500 outline-none text-sm"
          />
          <kbd className="px-2 py-0.5 text-[10px] text-gray-500 bg-white/5 rounded border border-white/10">
            ESC
          </kbd>
        </div>

        {/* Results */}
        <div ref={listRef} className="max-h-80 overflow-y-auto py-2">
          {filteredCommands.length === 0 ? (
            <div className="px-4 py-8 text-center text-gray-500 text-sm">
              No results found
            </div>
          ) : (
            filteredCommands.map((cmd, index) => (
              <button
                key={cmd.id}
                onClick={() => cmd.action()}
                onMouseEnter={() => setSelectedIndex(index)}
                className={`w-full flex items-center gap-3 px-4 py-2.5 text-left transition-colors ${
                  index === selectedIndex ? 'bg-white/10' : 'hover:bg-white/5'
                }`}
              >
                <div className={`shrink-0 ${
                  cmd.category === 'action' ? 'text-blue-400' :
                  cmd.category === 'file' ? 'text-teal-400' :
                  cmd.category === 'project' ? 'text-yellow-400' :
                  'text-green-400'
                }`}>
                  {cmd.icon}
                </div>
                <div className="flex-1 min-w-0">
                  <div className="text-sm text-white truncate">{cmd.title}</div>
                  {cmd.subtitle && (
                    <div className="text-xs text-gray-500 truncate">{cmd.subtitle}</div>
                  )}
                </div>
                {index === selectedIndex && (
                  <kbd className="shrink-0 px-2 py-0.5 text-[10px] text-gray-500 bg-white/5 rounded border border-white/10">
                    ↵
                  </kbd>
                )}
              </button>
            ))
          )}
        </div>

        {/* Footer */}
        <div className="flex items-center gap-4 px-4 py-2 border-t border-white/5 text-[10px] text-gray-600">
          <span className="flex items-center gap-1">
            <kbd className="px-1.5 py-0.5 bg-white/5 rounded border border-white/10">↑↓</kbd>
            navigate
          </span>
          <span className="flex items-center gap-1">
            <kbd className="px-1.5 py-0.5 bg-white/5 rounded border border-white/10">↵</kbd>
            select
          </span>
          <span className="flex items-center gap-1">
            <kbd className="px-1.5 py-0.5 bg-white/5 rounded border border-white/10">esc</kbd>
            close
          </span>
        </div>
      </div>
    </div>
  )
}
