import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('electron', () => ({
  app: {
    isPackaged: false,
  },
}))

vi.mock('node:fs', async (importOriginal) => {
  const actual = await importOriginal<typeof import('node:fs')>()
  return {
    ...actual,
    existsSync: vi.fn(() => false),
  }
})

import {
  getPlatformArch,
  getPlatformArchKey,
  getBinaryExeName,
  getBundledBinPath,
  getBundledPythonPath,
  getPythonExecutablePath,
  getMpremoteInvocation,
  getEsptoolInvocation,
  getArduinoCliPath,
  isBundledBinaryAvailable,
  resolveToolCommand,
} from '../../electron/main/utils/paths'

describe('getPlatformArch', () => {
  it('returns platform and arch', () => {
    const result = getPlatformArch()
    expect(result.platform).toBe(process.platform)
    expect(result.arch).toBe(process.arch)
  })
})

describe('getPlatformArchKey', () => {
  it('returns platform-arch string', () => {
    const key = getPlatformArchKey()
    expect(key).toBe(`${process.platform}-${process.arch}`)
  })
})

describe('getBinaryExeName', () => {
  it('appends .exe on win32', () => {
    if (process.platform === 'win32') {
      expect(getBinaryExeName('python')).toBe('python.exe')
    } else {
      expect(getBinaryExeName('python')).toBe('python')
    }
  })

  it('does not append extension on non-win32', () => {
    if (process.platform !== 'win32') {
      expect(getBinaryExeName('arduino-cli')).toBe('arduino-cli')
      expect(getBinaryExeName('mpremote')).toBe('mpremote')
    }
  })
})

describe('getBundledBinPath (dev mode)', () => {
  it('returns a path containing resources/bin', () => {
    const p = getBundledBinPath('arduino-cli')
    expect(p).toContain('resources')
    expect(p).toContain('bin')
  })

  it('includes the binary name', () => {
    const p = getBundledBinPath('mpremote')
    const exeName = getBinaryExeName('mpremote')
    expect(p).toContain(exeName)
  })

  it('includes platform-arch key in dev mode', () => {
    const p = getBundledBinPath('esptool')
    expect(p).toContain(getPlatformArchKey())
  })
})

describe('getBundledPythonPath (dev mode)', () => {
  it('returns a path containing python and resources', () => {
    const p = getBundledPythonPath()
    expect(p).toContain('python')
    expect(p).toContain('resources')
  })
})

describe('getPythonExecutablePath (dev mode)', () => {
  it('returns python3 on non-win32', () => {
    const p = getPythonExecutablePath()
    if (process.platform !== 'win32') {
      expect(p).toContain('python3')
    } else {
      expect(p).toContain('python.exe')
    }
  })
})

describe('getMpremoteInvocation (dev mode)', () => {
  it('returns python3 -m mpremote in dev', () => {
    const { cmd, args } = getMpremoteInvocation()
    expect(cmd).toBe('python3')
    expect(args).toEqual(['-m', 'mpremote'])
  })
})

describe('getEsptoolInvocation (dev mode)', () => {
  it('returns python3 -m esptool in dev', () => {
    const { cmd, args } = getEsptoolInvocation()
    expect(cmd).toBe('python3')
    expect(args).toEqual(['-m', 'esptool'])
  })
})

describe('getArduinoCliPath', () => {
  it('returns a path containing arduino-cli', () => {
    const p = getArduinoCliPath()
    expect(p).toContain('arduino-cli')
  })
})

describe('isBundledBinaryAvailable', () => {
  beforeEach(() => {
    vi.doMock('node:fs', () => ({ existsSync: vi.fn(() => false) }))
  })

  it('returns false for mpremote in dev mode (no bundled python)', () => {
    expect(isBundledBinaryAvailable('mpremote')).toBe(false)
  })

  it('returns false for esptool in dev mode', () => {
    expect(isBundledBinaryAvailable('esptool')).toBe(false)
  })

  it('returns false for arduino-cli when not on disk', () => {
    expect(isBundledBinaryAvailable('arduino-cli')).toBe(false)
  })

  it('returns false for python in dev mode', () => {
    expect(isBundledBinaryAvailable('python')).toBe(false)
  })
})

describe('resolveToolCommand', () => {
  it('returns mpremote invocation', () => {
    const result = resolveToolCommand('mpremote')
    expect(result).not.toBeNull()
    expect(result?.cmd).toBe('python3')
    expect(result?.args).toEqual(['-m', 'mpremote'])
  })

  it('returns esptool invocation', () => {
    const result = resolveToolCommand('esptool')
    expect(result).not.toBeNull()
    expect(result?.cmd).toBe('python3')
  })

  it('returns null for arduino-cli when not on disk', () => {
    const result = resolveToolCommand('arduino-cli')
    expect(result).toBeNull()
  })

  it('returns null for python when not on disk', () => {
    const result = resolveToolCommand('python')
    expect(result).toBeNull()
  })
})
