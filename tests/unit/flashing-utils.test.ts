import { describe, it, expect } from 'vitest'
import {
  parseBootOut,
  needsManualReset,
  flashAttempts,
  rp2040MountPoints,
  chipTypeFromFlashId,
  chipTypeFromVid,
  flashOffset,
  isEsptoolConnectionError,
} from '../../electron/main/utils/flashing'

describe('parseBootOut', () => {
  it('extracts board name and version from a real boot_out.txt first line', () => {
    const text = 'Adafruit CircuitPython 9.0.0 on 2024-03-19; Seeed XIAO ESP32S3 with ESP32S3\nBoard ID:seeed_xiao_esp32s3'
    expect(parseBootOut(text)).toEqual({ version: '9.0.0', boardName: 'Seeed XIAO ESP32S3' })
  })

  it('handles an RP2040 boot_out line', () => {
    const text = 'Adafruit CircuitPython 8.2.10 on 2024-02-14; Raspberry Pi Pico with rp2040'
    const { version, boardName } = parseBootOut(text)
    expect(version).toBe('8.2.10')
    expect(boardName).toBe('Raspberry Pi Pico')
  })

  it('returns undefineds for unrecognized content rather than throwing', () => {
    expect(parseBootOut('garbage')).toEqual({ version: undefined, boardName: undefined })
    expect(parseBootOut('')).toEqual({ version: undefined, boardName: undefined })
  })
})

describe('needsManualReset', () => {
  it('is true for Espressif native USB (VID 303a)', () => {
    expect(needsManualReset('303a', '/dev/cu.usbserial-10')).toBe(true)
    expect(needsManualReset('303A', '/dev/whatever')).toBe(true)
  })

  it('is true for a macOS native-USB CDC port path', () => {
    expect(needsManualReset('1a86', '/dev/tty.usbmodem21101')).toBe(true)
  })

  it('is false for a plain UART-bridge board (CP210x/CH340 on a usbserial path)', () => {
    expect(needsManualReset('10c4', '/dev/cu.usbserial-1410')).toBe(false)
    expect(needsManualReset(undefined, '/dev/ttyUSB0')).toBe(false)
  })
})

describe('flashAttempts', () => {
  it('tries no_reset first after a 1200-baud touch (board already in download mode)', () => {
    const attempts = flashAttempts(true)
    expect(attempts[0]).toEqual({ baud: '115200', before: 'no_reset' })
    expect(attempts.map((a) => a.before)).toContain('default_reset')
  })

  it('tries default_reset first for auto-reset boards', () => {
    const attempts = flashAttempts(false)
    expect(attempts[0].before).toBe('default_reset')
    expect(attempts[attempts.length - 1].before).toBe('no_reset')
  })
})

describe('rp2040MountPoints', () => {
  it('returns the macOS volume', () => {
    expect(rp2040MountPoints('darwin', 'eric')).toEqual(['/Volumes/RPI-RP2'])
  })

  it('returns both Linux media roots for the user', () => {
    expect(rp2040MountPoints('linux', 'eric')).toEqual(['/media/eric/RPI-RP2', '/run/media/eric/RPI-RP2'])
  })
})

describe('chipTypeFromFlashId', () => {
  it.each([
    ['Chip is ESP8266', 'ESP8266'],
    ['Detecting chip type... ESP32-S2', 'ESP32_GENERIC_S2'],
    ['Detecting chip type... ESP32-S3', 'ESP32_GENERIC_S3'],
    ['Detecting chip type... ESP32-C3', 'ESP32_GENERIC_C3'],
    ['Detecting chip type... ESP32-C6', 'ESP32_GENERIC_C6'],
    ['Chip is ESP32-D0WD', 'ESP32_GENERIC'],
    ['nonsense', 'ESP32_GENERIC'],
  ])('maps %s → %s', (output, expected) => {
    expect(chipTypeFromFlashId(output)).toBe(expected)
  })
})

describe('chipTypeFromVid', () => {
  it('maps Espressif native USB (303a) to the S3 generic firmware', () => {
    expect(chipTypeFromVid('303a')).toBe('ESP32_GENERIC_S3')
    expect(chipTypeFromVid('303A')).toBe('ESP32_GENERIC_S3')
  })
  it('returns null for non-Espressif VIDs', () => {
    expect(chipTypeFromVid('10c4')).toBeNull()
    expect(chipTypeFromVid(undefined)).toBeNull()
  })
})

describe('flashOffset', () => {
  it('flashes S3/C3/C6 and ESP8266 at 0x0', () => {
    for (const c of ['ESP8266', 'ESP32_GENERIC_S3', 'ESP32_GENERIC_C3', 'ESP32_GENERIC_C6']) {
      expect(flashOffset(c)).toBe('0x0')
    }
  })
  it('flashes the original ESP32 and the S2 at 0x1000', () => {
    expect(flashOffset('ESP32_GENERIC')).toBe('0x1000')
    expect(flashOffset('ESP32')).toBe('0x1000')
    expect(flashOffset('ESP32_GENERIC_S2')).toBe('0x1000')
  })
})

describe('isEsptoolConnectionError', () => {
  it('detects connection/sync failures worth retrying', () => {
    expect(isEsptoolConnectionError('No serial data received')).toBe(true)
    expect(isEsptoolConnectionError('Failed to connect to ESP32-S3')).toBe(true)
    expect(isEsptoolConnectionError('chip stopped responding')).toBe(true)
    expect(isEsptoolConnectionError('StopIteration')).toBe(true)
  })
  it('does not flag unrelated errors (e.g. missing tool)', () => {
    expect(isEsptoolConnectionError('No module named esptool')).toBe(false)
  })
})
