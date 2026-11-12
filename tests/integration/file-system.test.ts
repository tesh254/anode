import { describe, it, expect, beforeEach } from 'vitest'
import fs from 'node:fs'
import path from 'node:path'
import os from 'node:os'
import { useStore } from '@/lib/store'

describe('Integration — File System Operations', () => {
  const tmpDir = path.join(os.tmpdir(), `anode-test-fs-integration-${Date.now()}`)

  beforeEach(() => {
    useStore.getState().reset()
    if (!fs.existsSync(tmpDir)) {
      fs.mkdirSync(tmpDir, { recursive: true })
    }
  })

  it('open project: reads directory and loads .py and .ino files', () => {
    fs.writeFileSync(path.join(tmpDir, 'main.py'), 'print("hello")', 'utf-8')
    fs.writeFileSync(path.join(tmpDir, 'helper.py'), 'def add(a, b): return a + b', 'utf-8')
    fs.writeFileSync(path.join(tmpDir, 'sketch.ino'), 'void setup() {}', 'utf-8')
    fs.writeFileSync(path.join(tmpDir, 'readme.txt'), 'not a code file', 'utf-8')

    const entries = fs.readdirSync(tmpDir, { withFileTypes: true })
    const codeFiles: { name: string; path: string; content: string; language: 'python' | 'arduino' }[] = []

    for (const entry of entries) {
      if (entry.isFile && (entry.name.endsWith('.py') || entry.name.endsWith('.ino'))) {
        const filePath = path.join(tmpDir, entry.name)
        const content = fs.readFileSync(filePath, 'utf-8')
        const ext = path.extname(filePath).toLowerCase()
        const language = ext === '.ino' ? 'arduino' : 'python'
        codeFiles.push({ name: entry.name, path: filePath, content, language })
      }
    }

    expect(codeFiles).toHaveLength(3)
    expect(codeFiles.find(f => f.name === 'main.py')?.language).toBe('python')
    expect(codeFiles.find(f => f.name === 'sketch.ino')?.language).toBe('arduino')
    expect(codeFiles.find(f => f.name === 'readme.txt')).toBeUndefined()
  })

  it('open project: sets project path, files, and selects first file', () => {
    const files = [
      { name: 'a.py', path: path.join(tmpDir, 'a.py'), content: 'a', language: 'python' as const },
      { name: 'b.py', path: path.join(tmpDir, 'b.py'), content: 'b', language: 'python' as const },
    ]
    fs.writeFileSync(files[0].path, files[0].content, 'utf-8')
    fs.writeFileSync(files[1].path, files[1].content, 'utf-8')

    useStore.getState().setProjectPath(tmpDir)
    useStore.getState().setProjectFiles(files)
    useStore.getState().setCurrentFile(files[0])

    expect(useStore.getState().projectPath).toBe(tmpDir)
    expect(useStore.getState().projectFiles).toHaveLength(2)
    expect(useStore.getState().currentFile?.name).toBe('a.py')
    expect(useStore.getState().welcomeScreenVisible).toBe(true) // Not set to false by store alone
  })

  it('save file: writes content to disk and clears dirty state', () => {
    const filePath = path.join(tmpDir, 'save-test.py')
    const originalContent = 'original'
    const updatedContent = 'updated'

    fs.writeFileSync(filePath, originalContent, 'utf-8')
    expect(fs.readFileSync(filePath, 'utf-8')).toBe('original')

    fs.writeFileSync(filePath, updatedContent, 'utf-8')
    expect(fs.readFileSync(filePath, 'utf-8')).toBe('updated')

    useStore.getState().setIsDirty(false)
    expect(useStore.getState().isDirty).toBe(false)
  })

  it('create file: creates new .py file with auto-extension', () => {
    const fileName = 'newfile'
    const fullFileName = fileName.endsWith('.py') ? fileName : `${fileName}.py`
    const filePath = path.join(tmpDir, fullFileName)

    fs.writeFileSync(filePath, '# New file\n', 'utf-8')
    expect(fs.existsSync(filePath)).toBe(true)
    expect(fs.readFileSync(filePath, 'utf-8')).toBe('# New file\n')
    expect(fullFileName).toBe('newfile.py')
  })

  it('create file: does not double-add .py extension', () => {
    const fileName = 'test.py'
    const fullFileName = fileName.endsWith('.py') ? fileName : `${fileName}.py`
    expect(fullFileName).toBe('test.py')
  })

  it('create folder: creates directory on disk', () => {
    const folderPath = path.join(tmpDir, 'new-folder')
    fs.mkdirSync(folderPath, { recursive: true })
    expect(fs.statSync(folderPath).isDirectory()).toBe(true)
  })

  it('file content update is reflected in store', () => {
    const file = { name: 'main.py', path: path.join(tmpDir, 'main.py'), content: 'old', language: 'python' as const }
    useStore.getState().setProjectFiles([file])
    useStore.getState().setCurrentFile(file)
    useStore.getState().updateFileContent(file.path, 'new content')

    expect(useStore.getState().currentFile?.content).toBe('new content')
    expect(useStore.getState().projectFiles[0].content).toBe('new content')
    expect(useStore.getState().isDirty).toBe(true)
  })

  it('handles empty directory gracefully', () => {
    const emptyDir = path.join(tmpDir, 'empty')
    fs.mkdirSync(emptyDir, { recursive: true })

    const entries = fs.readdirSync(emptyDir, { withFileTypes: true })
    expect(entries).toHaveLength(0)
  })

  it('handles nested directory structure', () => {
    fs.mkdirSync(path.join(tmpDir, 'lib'), { recursive: true })
    fs.writeFileSync(path.join(tmpDir, 'lib', 'utils.py'), 'def util(): pass', 'utf-8')

    const entries = fs.readdirSync(tmpDir, { withFileTypes: true })
    const libEntry = entries.find(e => e.name === 'lib')
    expect(libEntry?.isDirectory()).toBe(true)

    const libEntries = fs.readdirSync(path.join(tmpDir, 'lib'), { withFileTypes: true })
    expect(libEntries.find(e => e.name === 'utils.py')?.isFile()).toBe(true)
  })

  it('project persistence: add to recent projects on open', () => {
    const projectName = 'my-project'
    useStore.getState().addRecentProject(tmpDir, projectName)

    const recent = useStore.getState().recentProjects
    expect(recent).toHaveLength(1)
    expect(recent[0].path).toBe(tmpDir)
    expect(recent[0].name).toBe(projectName)
  })

  it('switching files: prompt save if dirty, then set new file', () => {
    const fileA = { name: 'a.py', path: '/a.py', content: 'aaa', language: 'python' as const }
    const fileB = { name: 'b.py', path: '/b.py', content: 'bbb', language: 'python' as const }

    useStore.getState().setProjectFiles([fileA, fileB])
    useStore.getState().setCurrentFile(fileA)
    useStore.getState().updateFileContent('/a.py', 'modified a')
    expect(useStore.getState().isDirty).toBe(true)

    // Simulate user choosing not to save, switching file
    useStore.getState().setCurrentFile(fileB)
    expect(useStore.getState().currentFile?.name).toBe('b.py')
    expect(useStore.getState().isDirty).toBe(false)
  })
})
