import { app } from 'electron'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { existsSync } from 'node:fs'

const __dirname = path.dirname(fileURLToPath(import.meta.url))

export type BundledBinary = 'mpremote' | 'esptool' | 'arduino-cli' | 'python'

export interface PlatformArch {
  platform: string
  arch: string
}

export function getPlatformArch(): PlatformArch {
  return { platform: process.platform, arch: process.arch }
}

export function getPlatformArchKey(): string {
  return `${process.platform}-${process.arch}`
}

export function getBinaryExeName(binaryName: string): string {
  return process.platform === 'win32' ? `${binaryName}.exe` : binaryName
}

export function getBundledBinPath(binaryName: string): string {
  const isProd = app.isPackaged
  const exeName = getBinaryExeName(binaryName)

  if (isProd) {
    return path.join(process.resourcesPath, 'bin', exeName)
  }

  const platformArch = getPlatformArchKey()
  return path.resolve(__dirname, '..', '..', '..', 'resources', 'bin', platformArch, exeName)
}

export function getBundledPythonPath(): string {
  const dir = process.platform === 'win32' ? 'python' : 'python'
  const isProd = app.isPackaged
  if (isProd) {
    return path.join(process.resourcesPath, 'bin', dir)
  }
  const platformArch = getPlatformArchKey()
  return path.resolve(__dirname, '..', '..', '..', 'resources', 'bin', platformArch, dir)
}

export function getPythonExecutablePath(): string {
  const pythonDir = getBundledPythonPath()
  if (process.platform === 'win32') {
    return path.join(pythonDir, 'python.exe')
  }
  return path.join(pythonDir, 'bin', 'python3')
}

export function getMpremoteInvocation(): { cmd: string; args: string[] } {
  const isProd = app.isPackaged
  if (isProd) {
    const python = getPythonExecutablePath()
    return { cmd: python, args: ['-m', 'mpremote'] }
  }
  return { cmd: 'python3', args: ['-m', 'mpremote'] }
}

export function getEsptoolInvocation(): { cmd: string; args: string[] } {
  const isProd = app.isPackaged
  if (isProd) {
    const python = getPythonExecutablePath()
    return { cmd: python, args: ['-m', 'esptool'] }
  }
  return { cmd: 'python3', args: ['-m', 'esptool'] }
}

export function getArduinoCliPath(): string {
  return getBundledBinPath('arduino-cli')
}

export function isBundledBinaryAvailable(binary: BundledBinary): boolean {
  switch (binary) {
    case 'mpremote':
    case 'esptool': {
      const isProd = app.isPackaged
      if (isProd) return existsSync(getPythonExecutablePath())
      return false
    }
    case 'arduino-cli': {
      return existsSync(getArduinoCliPath())
    }
    case 'python': {
      return existsSync(getPythonExecutablePath())
    }
  }
}

export function resolveToolCommand(binary: BundledBinary): { cmd: string; args: string[] } | null {
  switch (binary) {
    case 'mpremote':
      return getMpremoteInvocation()
    case 'esptool':
      return getEsptoolInvocation()
    case 'arduino-cli': {
      const p = getArduinoCliPath()
      return existsSync(p) ? { cmd: p, args: [] } : null
    }
    case 'python': {
      const p = getPythonExecutablePath()
      return existsSync(p) ? { cmd: p, args: [] } : null
    }
  }
}
