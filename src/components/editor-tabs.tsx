import { X, Circle } from 'lucide-react'
import { useStore, type OpenTab } from '@/lib/store'
import { cn } from '@/lib/utils'

interface EditorTabsProps {
  className?: string
  onTabSelect: (tab: OpenTab) => void
  onTabClose: (tab: OpenTab) => void
}

export function EditorTabs({ className, onTabSelect, onTabClose }: EditorTabsProps) {
  const { openTabs, activeTabPath } = useStore()

  if (openTabs.length === 0) return null

  return (
    <div className={cn('flex items-center bg-gray-100 dark:bg-[#0f0f0f] border-b border-gray-200 dark:border-white/5 overflow-x-auto shrink-0', className)}>
      {openTabs.map((tab) => {
        const isActive = tab.path === activeTabPath
        return (
          <div
            key={tab.path}
            onClick={() => onTabSelect(tab)}
            className={cn(
              'group flex items-center gap-2 px-3 py-1.5 text-[12px] cursor-pointer border-r border-gray-200 dark:border-white/5 transition-colors whitespace-nowrap',
              isActive
                ? 'bg-white dark:bg-[#1a1a1a] text-gray-900 dark:text-white border-b-2 border-b-green-500'
                : 'text-gray-500 dark:text-gray-400 hover:bg-gray-200/50 dark:hover:bg-white/5',
            )}
          >
            {tab.scope === 'remote' && (
              <span className="size-1.5 rounded-full bg-blue-400" title="Remote file" />
            )}
            <span className="truncate max-w-[120px]">{tab.name}</span>
            {tab.isDirty ? (
              <Circle className="size-2 fill-current text-yellow-500" />
            ) : (
              <button
                onClick={(e) => { e.stopPropagation(); onTabClose(tab) }}
                className="opacity-0 group-hover:opacity-100 p-0.5 hover:bg-gray-300 dark:hover:bg-white/20 rounded transition-opacity"
              >
                <X className="size-3" />
              </button>
            )}
          </div>
        )
      })}
    </div>
  )
}
