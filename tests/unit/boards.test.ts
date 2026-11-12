import { describe, it, expect } from 'vitest'
import { identifyBoard, getBoardById, portKey, BOARD_CATALOG } from '@/lib/boards'

describe('board catalog', () => {
  it('has unique ids', () => {
    const ids = BOARD_CATALOG.map((b) => b.id)
    expect(new Set(ids).size).toBe(ids.length)
  })

  it('every board maps to a known flash board type', () => {
    const valid = ['espressif', 'm5stack-atom', 'm5stamp-s3', 'raspberry-pi', 'arduino']
    for (const b of BOARD_CATALOG) expect(valid).toContain(b.flashBoardType)
  })
})

describe('identifyBoard — exact matches', () => {
  it('identifies the M5Stack ATOM Lite by CP2104 descriptor', () => {
    const m = identifyBoard({ vendorId: '10c4', productId: 'ea60' })
    expect(m.board?.id).toBe('m5stack-atom-lite')
    expect(m.exact).toBe(true)
  })

  it('identifies the M5StampS3 by 303a:80xx', () => {
    const m = identifyBoard({ vendorId: '303a', productId: '8010' })
    expect(m.board?.id).toBe('m5stamp-s3')
    expect(m.exact).toBe(true)
  })

  it('identifies the Raspberry Pi Pico by 2e8a', () => {
    expect(identifyBoard({ vendorId: '2e8a', productId: '0005' }).board?.id).toBe('rpi-pico')
  })

  it('identifies an Arduino Uno by 2341', () => {
    expect(identifyBoard({ vendorId: '2341' }).board?.id).toBe('arduino-uno')
  })
})

describe('identifyBoard — ambiguous ESP32-S3', () => {
  it('falls back to generic ESP32-S3 for native USB-Serial/JTAG (303a:1001)', () => {
    const m = identifyBoard({ vendorId: '303a', productId: '1001' })
    expect(m.board?.id).toBe('esp32-s3-generic')
    expect(m.exact).toBe(false)
  })

  it('offers all ESP32-S3 boards as pickable candidates', () => {
    const m = identifyBoard({ vendorId: '303a', productId: '1001' })
    const ids = m.candidates.map((c) => c.id)
    expect(ids).toContain('m5stamp-s3')
    expect(ids).toContain('seeed-xiao-esp32s3')
    expect(ids).toContain('esp32-s3-generic')
  })

  it('detects the XIAO when the serial number advertises it', () => {
    const m = identifyBoard({ vendorId: '303a', productId: '1001', serialNumber: 'XIAO-ESP32S3-1234' })
    expect(m.board?.id).toBe('seeed-xiao-esp32s3')
    expect(m.exact).toBe(true)
  })
})

describe('identifyBoard — user override wins', () => {
  it('pins the XIAO over the auto-detected generic', () => {
    const port = { vendorId: '303a', productId: '1001', serialNumber: 'abc123' }
    const overrides = { [portKey(port)]: 'seeed-xiao-esp32s3' }
    const m = identifyBoard(port, overrides)
    expect(m.board?.id).toBe('seeed-xiao-esp32s3')
    expect(m.fromOverride).toBe(true)
    expect(m.exact).toBe(false) // exact is reserved for descriptor-based matches
  })

  it('keys the override by serial number, falling back to path', () => {
    expect(portKey({ serialNumber: 'sn-1', path: '/dev/x' })).toBe('sn-1')
    expect(portKey({ path: '/dev/x' })).toBe('/dev/x')
  })
})

describe('getBoardById', () => {
  it('returns the board for a known id and undefined otherwise', () => {
    expect(getBoardById('m5stamp-s3')?.name).toBe('M5StampS3')
    expect(getBoardById('nope')).toBeUndefined()
    expect(getBoardById(null)).toBeUndefined()
  })
})
