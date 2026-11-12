import { useState } from 'react'
import { useTheme } from 'next-themes'
import Editor from '@monaco-editor/react'
import { useStore } from '@/lib/store'
import { TEMPLATES, getTemplatesByBoard, Template } from '@/lib/templates'
import { 
  FolderOpen, 
  Plus, 
  Clock, 
  FileCode2, 
  ChevronRight, 
  X, 
  Zap, 
  Wifi, 
  Thermometer, 
  Monitor, 
  Settings,
  MonitorPlay,
  Sun,
  Moon,
} from 'lucide-react'

interface WelcomeScreenProps {
  onOpenProject: (path: string) => void
  onCreateFromTemplate: (template: Template, projectName: string, projectPath: string) => void
  onOpenSettings: () => void
}

const BOARD_LABELS: Record<string, string> = {
  'espressif': 'ESP32 / ESP8266',
  'raspberry-pi': 'Raspberry Pi Pico',
  'arduino': 'Arduino',
}

const TEMPLATE_ICONS: Record<string, React.ReactNode> = {
  'Zap': <Zap className="size-5" />,
  'Wifi': <Wifi className="size-5" />,
  'Thermometer': <Thermometer className="size-5" />,
  'Monitor': <Monitor className="size-5" />,
  'Settings': <Settings className="size-5" />,
  'MonitorPlay': <MonitorPlay className="size-5" />,
}

export function WelcomeScreen({ onOpenProject, onCreateFromTemplate, onOpenSettings }: WelcomeScreenProps) {
  const { theme, setTheme } = useTheme()
  const { recentProjects, removeRecentProject } = useStore()
  const [showTemplates, setShowTemplates] = useState(false)
  const [selectedBoard, setSelectedBoard] = useState<string | null>(null)
  const [previewTemplate, setPreviewTemplate] = useState<Template | null>(null)
  const [showNameDialog, setShowNameDialog] = useState(false)
  const [pendingTemplate, setPendingTemplate] = useState<Template | null>(null)
  const [projectName, setProjectName] = useState('')

  const handleOpenFolder = async () => {
    const hw = (window as unknown as { hardware: { openFolder: () => Promise<string | null> } }).hardware
    if (hw?.openFolder) {
      const path = await hw.openFolder()
      if (path) {
        onOpenProject(path)
      }
    }
  }

  const handleNewProject = () => {
    setShowTemplates(true)
    setSelectedBoard('espressif')
  }

  const handleCreateProject = async (template: Template) => {
    setPendingTemplate(template)
    setProjectName(template.name.replace(/\s+/g, '-').toLowerCase() + '-project')
    setShowNameDialog(true)
  }

  const handleConfirmCreate = async () => {
    if (!pendingTemplate || !projectName.trim()) return

    const hw = (window as unknown as { hardware: { openFolder: () => Promise<string | null> } }).hardware
    if (!hw?.openFolder) return

    const basePath = await hw.openFolder()
    if (!basePath) return

    const finalPath = `${basePath}/${projectName.trim()}`
    onCreateFromTemplate(pendingTemplate, projectName.trim(), finalPath)
    
    setShowNameDialog(false)
    setShowTemplates(false)
    setPendingTemplate(null)
    setProjectName('')
  }

  const filteredTemplates = selectedBoard ? getTemplatesByBoard(selectedBoard as Template['board']) : TEMPLATES

  // Show name dialog when creating a project
  if (showNameDialog && pendingTemplate) {
    return (
      <div className="fixed inset-0 bg-black/50 dark:bg-black/70 flex items-center justify-center z-[60]">
        <div className="w-full max-w-md bg-white dark:bg-[#111] border border-gray-200 dark:border-white/10 rounded-xl p-6">
          <h2 className="text-lg font-semibold text-gray-900 dark:text-white mb-4">Name Your Project</h2>
          
          <div className="mb-4">
            <label className="block text-xs text-gray-600 dark:text-gray-400 mb-2">Project Name</label>
            <input
              type="text"
              value={projectName}
              onChange={(e) => setProjectName(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && handleConfirmCreate()}
              className="w-full bg-gray-100 dark:bg-black/30 border border-gray-300 dark:border-white/10 rounded-md px-3 py-2 text-gray-900 dark:text-white text-sm focus:outline-none focus:border-green-500"
              placeholder="my-project"
              autoFocus
            />
          </div>

          <div className="flex justify-end gap-3">
            <button
              onClick={() => { setShowNameDialog(false); setPendingTemplate(null); setProjectName('') }}
              className="px-4 py-2 text-gray-600 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white transition-colors"
            >
              Cancel
            </button>
            <button
              onClick={handleConfirmCreate}
              disabled={!projectName.trim()}
              className="bg-green-600 hover:bg-green-700 disabled:opacity-50 disabled:cursor-not-allowed text-white px-4 py-2 rounded-md font-medium transition-colors"
            >
              Create Project
            </button>
          </div>
        </div>
      </div>
    )
  }

  if (showTemplates) {
    return (
      <div className="fixed inset-0 bg-white dark:bg-[#0a0a0a] flex items-center justify-center z-50 p-8">
        <div className="w-full max-w-4xl h-full max-h-[calc(100vh-4rem)] flex flex-col bg-white dark:bg-[#111] border border-gray-200 dark:border-white/10 rounded-xl overflow-hidden">
          {/* Header */}
          <div className="flex items-center justify-between px-6 py-4 border-b border-gray-200 dark:border-white/5 shrink-0">
            <div>
              <h1 className="text-xl font-semibold text-gray-900 dark:text-white">New Project</h1>
              <p className="text-sm text-gray-500 dark:text-gray-400 mt-1">Select a template to get started</p>
            </div>
            <button
              onClick={() => { setShowTemplates(false); setPreviewTemplate(null) }}
              className="p-2 text-gray-500 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white hover:bg-gray-100 dark:hover:bg-white/10 rounded-md transition-colors"
            >
              <X className="size-5" />
            </button>
          </div>

          <div className="flex flex-1 min-h-0">
            {/* Sidebar - Board selection */}
            <div className="w-56 border-r border-gray-200 dark:border-white/5 p-4 shrink-0 overflow-y-auto">
              <div className="text-[10px] font-bold text-gray-500 dark:text-gray-500 tracking-widest uppercase mb-3">
                Board Type
              </div>
              <div className="flex flex-col gap-1">
                {Object.entries(BOARD_LABELS).map(([board, label]) => (
                  <button
                    key={board}
                    onClick={() => { setSelectedBoard(board); setPreviewTemplate(null) }}
                    className={`w-full text-left px-3 py-2 rounded-md text-sm transition-colors ${
                      selectedBoard === board 
                        ? 'bg-green-500/20 dark:bg-green-600/20 text-green-600 dark:text-green-400' 
                        : 'text-gray-600 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white hover:bg-gray-100 dark:hover:bg-white/5'
                    }`}
                  >
                    {label}
                  </button>
                ))}
              </div>
            </div>

            {/* Main content - Template list */}
            <div className="flex-1 flex min-w-0">
              {/* Templates */}
              <div className="w-80 border-r border-gray-200 dark:border-white/5 p-4 overflow-y-auto shrink-0">
                <div className="text-[10px] font-bold text-gray-500 dark:text-gray-500 tracking-widest uppercase mb-3">
                  Templates
                </div>
                <div className="flex flex-col gap-2">
                  {filteredTemplates.map((template) => (
                    <button
                      key={template.id}
                      onClick={() => setPreviewTemplate(template)}
                      onDoubleClick={() => handleCreateProject(template)}
                      className={`w-full text-left p-3 rounded-md transition-colors ${
                        previewTemplate?.id === template.id
                          ? 'bg-gray-100 dark:bg-white/10 border border-gray-200 dark:border-white/20'
                          : 'hover:bg-gray-100 dark:hover:bg-white/5 border border-transparent'
                      }`}
                    >
                      <div className="flex items-center gap-3">
                        <div className={`shrink-0 ${
                          template.board === 'espressif' ? 'text-cyan-500 dark:text-cyan-400' :
                          template.board === 'raspberry-pi' ? 'text-orange-500 dark:text-orange-400' :
                          'text-blue-500 dark:text-blue-400'
                        }`}>
                          {TEMPLATE_ICONS[template.icon] || <Zap className="size-5" />}
                        </div>
                        <div className="min-w-0">
                          <div className="text-sm font-medium text-gray-900 dark:text-gray-200 truncate">{template.name}</div>
                          <div className="text-xs text-gray-500 dark:text-gray-500 truncate">{template.description}</div>
                        </div>
                      </div>
                    </button>
                  ))}
                </div>
              </div>

              {/* Preview */}
              <div className="flex-1 p-4 flex flex-col">
                <div className="text-[10px] font-bold text-gray-500 dark:text-gray-500 tracking-widest uppercase mb-3 shrink-0">
                  Preview
                </div>
                {previewTemplate ? (
                  <div className="flex-1 flex flex-col min-h-0">
                    <div className="mb-3 shrink-0">
                      <div className="text-lg font-medium text-gray-900 dark:text-white">{previewTemplate.name}</div>
                      <div className="text-sm text-gray-500 dark:text-gray-400 mt-1">{previewTemplate.description}</div>
                    </div>
                    <div className="flex-1 min-h-0 rounded-md overflow-hidden border border-gray-200 dark:border-white/5">
                      <Editor
                        height="100%"
                        language={previewTemplate.language === 'arduino' ? 'cpp' : 'python'}
                        value={previewTemplate.code}
                        theme="vs-dark"
                        options={{
                          readOnly: true,
                          minimap: { enabled: false },
                          fontSize: 12,
                          fontFamily: "'JetBrains Mono', 'Fira Code', 'Menlo', monospace",
                          lineNumbers: 'off',
                          scrollBeyondLastLine: false,
                          wordWrap: 'on',
                          renderLineHighlight: 'none',
                          padding: { top: 8, bottom: 8 },
                        }}
                      />
                    </div>
                    <div className="mt-4 shrink-0 flex justify-end">
                      <button
                        onClick={() => previewTemplate && handleCreateProject(previewTemplate)}
                        className="bg-green-600 hover:bg-green-700 text-white px-6 py-2.5 rounded-md font-medium flex items-center gap-2 transition-colors"
                      >
                        <Plus className="size-4" />
                        Create Project
                      </button>
                    </div>
                  </div>
                ) : (
                  <div className="flex-1 flex items-center justify-center text-gray-500 dark:text-gray-500">
                    Select a template to preview
                  </div>
                )}
              </div>
            </div>
          </div>
        </div>
      </div>
    )
  }

  return (
    <div className="fixed inset-0 bg-white dark:bg-[#0a0a0a] flex flex-col items-center justify-center">
      {/* Logo/Title */}
      <div className="text-center mb-12">
        <div className="flex items-center justify-center gap-3 mb-4">
          <img src="/app-logo-green.svg" alt="Anode" className="h-12 dark:hidden" />
          <img src="/app-logo-black.svg" alt="Anode" className="h-12 hidden dark:block" />
        </div>
        <p className="text-gray-500 dark:text-gray-400 mt-2">MicroPython & Arduino IDE</p>
      </div>

      {/* Main actions */}
      <div className="flex flex-col gap-3 w-80">
        <button
          onClick={handleNewProject}
          className="w-full flex items-center gap-3 px-4 py-3 bg-gray-100 dark:bg-white/5 hover:bg-gray-200 dark:hover:bg-white/10 border border-gray-200 dark:border-white/10 rounded-md transition-colors text-left group"
        >
          <Plus className="size-5 text-green-600 dark:text-green-500" />
          <span className="text-gray-900 dark:text-gray-200 font-medium">New Project</span>
          <ChevronRight className="size-4 text-gray-400 ml-auto group-hover:text-gray-600 dark:group-hover:text-gray-300 transition-colors" />
        </button>

        <button
          onClick={handleOpenFolder}
          className="w-full flex items-center gap-3 px-4 py-3 bg-gray-100 dark:bg-white/5 hover:bg-gray-200 dark:hover:bg-white/10 border border-gray-200 dark:border-white/10 rounded-md transition-colors text-left group"
        >
          <FolderOpen className="size-5 text-blue-600 dark:text-blue-400" />
          <span className="text-gray-900 dark:text-gray-200 font-medium">Open Project Folder</span>
          <ChevronRight className="size-4 text-gray-400 ml-auto group-hover:text-gray-600 dark:group-hover:text-gray-300 transition-colors" />
        </button>
      </div>

      {/* Recent projects */}
      {recentProjects.length > 0 && (
        <div className="mt-16 w-80">
          <div className="flex items-center gap-2 mb-4">
            <Clock className="size-4 text-gray-400 dark:text-gray-500" />
            <span className="text-[10px] font-bold text-gray-500 dark:text-gray-500 tracking-widest uppercase">Recent Projects</span>
          </div>
          <div className="flex flex-col gap-1">
            {recentProjects.map((project) => (
              <div
                key={project.path}
                className="group flex items-center gap-3 px-3 py-2.5 rounded-md hover:bg-gray-100 dark:hover:bg-white/5 transition-colors cursor-pointer"
                onClick={() => onOpenProject(project.path)}
              >
                <FileCode2 className="size-4 text-gray-400 dark:text-gray-500 shrink-0" />
                <div className="flex-1 min-w-0">
                  <div className="text-sm text-gray-900 dark:text-gray-200 truncate">{project.name}</div>
                  <div className="text-xs text-gray-500 dark:text-gray-500 truncate">{project.path}</div>
                </div>
                <button
                  onClick={(e) => { e.stopPropagation(); removeRecentProject(project.path) }}
                  className="opacity-0 group-hover:opacity-100 p-1 hover:bg-gray-200 dark:hover:bg-white/10 rounded transition-all"
                >
                  <X className="size-3 text-gray-400" />
                </button>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Settings and Theme toggle button */}
      <button
        onClick={onOpenSettings}
        className="absolute top-4 right-4 p-2 text-gray-500 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white hover:bg-gray-100 dark:hover:bg-white/10 rounded-md transition-colors"
      >
        <Settings className="size-5" />
      </button>

      <button
        onClick={() => setTheme(theme === 'dark' ? 'light' : 'dark')}
        className="absolute bottom-4 right-4 p-2 text-gray-500 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white hover:bg-gray-100 dark:hover:bg-white/10 rounded-md transition-colors"
      >
        {theme === 'dark' ? <Sun className="size-5" /> : <Moon className="size-5" />}
      </button>
    </div>
  )
}
