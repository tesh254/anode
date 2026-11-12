import { describe, it, expect } from 'vitest'
import { resolveProfileOnDetect, detectExecutionProfiles, selectBestProfile } from '@/lib/store'
import type { ExecutionProfile } from '@/types/interpreter'

// Helpers to build profiles the way detectExecutionProfiles would, for the
// board-swap / runtime-follow backbone (resolveProfileOnDetect).
const profilesFor = (port: { vendorId?: string; productId?: string; manufacturer?: string; path?: string; circuitPython?: { boardName?: string } }) =>
  detectExecutionProfiles({ path: '/dev/cu.usbmodem1', ...port })

const arduinoUno = profilesFor({ vendorId: '2341', productId: '0043' })
const bareS3 = profilesFor({ vendorId: '303a', productId: '1001' })
const circuitPyS3 = profilesFor({ vendorId: '303a', productId: '1001', circuitPython: { boardName: 'XIAO ESP32S3' } })

const pick = (profiles: ExecutionProfile[], runtime: string) =>
  profiles.find((p) => p.runtime === runtime && !p.isGeneric) ?? profiles.find((p) => p.runtime === runtime)!

describe('resolveProfileOnDetect — board connection / swap backbone', () => {
  it('fresh launch (no active profile) restores the saved profile when present', () => {
    const saved = selectBestProfile(arduinoUno)!
    const next = resolveProfileOnDetect({ current: null, profiles: arduinoUno, savedId: saved.id })
    expect(next?.id).toBe(saved.id)
  })

  it('fresh launch with no saved id picks the best match', () => {
    const next = resolveProfileOnDetect({ current: null, profiles: arduinoUno, savedId: null })
    expect(next?.id).toBe(selectBestProfile(arduinoUno)!.id)
  })

  it('follows a real board swap: Arduino → ESP32-S3 on the same port', () => {
    const current = selectBestProfile(arduinoUno)! // Arduino active
    // Board physically swapped — now a bare ESP32-S3 is detected on the port.
    const next = resolveProfileOnDetect({ current, profiles: bareS3, savedId: current.id })
    expect(next?.id).not.toBe(current.id)
    expect(next?.runtime).toBe('MicroPython') // bare S3 → MicroPython best guess
  })

  it('preserves a MANUAL pick across re-detection on the same board', () => {
    // User manually pinned the CircuitPython profile on a bare S3 (manual: true).
    const manual = bareS3.find((p) => p.runtime === 'CircuitPython')!
    const next = resolveProfileOnDetect({ current: manual, profiles: bareS3, savedId: manual.id, manual: true })
    expect(next?.id).toBe(manual.id) // unchanged — still valid on this port
  })

  it('replaces a MANUAL pick only when the board genuinely swaps away', () => {
    // A board-specific (non-generic) manual pick: the S3 MicroPython profile.
    const manual = bareS3.find((p) => p.runtime === 'MicroPython' && !p.isGeneric)!
    // Board physically swapped to an Arduino — that S3 profile id is gone.
    const next = resolveProfileOnDetect({ current: manual, profiles: arduinoUno, savedId: manual.id, manual: true })
    expect(next?.runtime).toBe('Arduino')
  })

  it('an AUTO profile follows a newly-mounted CIRCUITPY drive (best detection wins)', () => {
    const current = pick(bareS3, 'MicroPython') // auto-detected MicroPython guess
    // Same port, but now a CIRCUITPY drive is detected → high-confidence CP profile.
    const next = resolveProfileOnDetect({ current, profiles: circuitPyS3, savedId: current.id, manual: false })
    expect(next?.runtime).toBe('CircuitPython')
    expect(next?.confidence).toBe('high')
  })

  it('an AUTO profile re-detecting the same board stays on the same best (no churn)', () => {
    const current = pick(arduinoUno, 'Arduino')
    const next = resolveProfileOnDetect({ current, profiles: arduinoUno, savedId: current.id, manual: false })
    expect(next?.id).toBe(current.id)
  })

  it('keeps the current profile when the detected set is empty', () => {
    const current = selectBestProfile(arduinoUno)!
    const next = resolveProfileOnDetect({ current, profiles: [], savedId: current.id, manual: false })
    expect(next?.id).toBe(current.id)
  })
})
