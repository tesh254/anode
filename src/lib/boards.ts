import type { RuntimeEnvironment } from '@/types/interpreter'
import type { BoardType } from '@/lib/store'

// A single source of truth for the boards Anode knows by name. Detection from
// USB descriptors is best-effort: some boards (the M5Stack Atom Lite, the Pico,
// Arduinos) have a unique VID/PID and can be named exactly. Others — every
// ESP32-S3 board that uses the chip's *native* USB — present the identical
// descriptor (303a:1001) regardless of which board it is, so they can only be
// narrowed to a chip family. For those we surface a picker so the user can
// confirm the exact board once; the choice is remembered per physical board.
export interface BoardDef {
  id: string
  /** Friendly name shown in the UI. */
  name: string
  /** Short vendor/series label, e.g. "M5Stack", "Seeed Studio". */
  vendor: string
  /** Silicon family — also groups boards into the same picker list. */
  chip: 'ESP32' | 'ESP32-S3' | 'ESP32-C3' | 'RP2040' | 'AVR'
  runtime: RuntimeEnvironment
  /** Legacy board kind the flash/upload handlers key off. */
  flashBoardType: BoardType
  /** MicroPython firmware variant (micropython.org slug), where applicable. */
  mpyVariant?: string
  /** USB descriptor hints used to auto-detect this board. */
  match?: {
    vid?: string
    pidPrefix?: string
    manufacturerIncludes?: string
    serialIncludes?: string
  }
  /** True for chip-family fallbacks that aren't a specific product. */
  generic?: boolean
}

export const BOARD_CATALOG: BoardDef[] = [
  {
    id: 'm5stack-atom-lite',
    name: 'M5Stack ATOM Lite',
    vendor: 'M5Stack',
    chip: 'ESP32',
    runtime: 'MicroPython',
    flashBoardType: 'm5stack-atom',
    mpyVariant: 'ESP32_GENERIC',
    // CP2104 USB-UART bridge — a unique, board-specific descriptor.
    match: { vid: '10c4', pidPrefix: 'ea60' },
  },
  {
    id: 'm5stamp-s3',
    name: 'M5StampS3',
    vendor: 'M5Stack',
    chip: 'ESP32-S3',
    runtime: 'MicroPython',
    flashBoardType: 'm5stamp-s3',
    mpyVariant: 'ESP32_GENERIC_S3',
    // Enumerates via ESP32-S3 native USB with an 0x80xx product id.
    match: { vid: '303a', pidPrefix: '80' },
  },
  {
    id: 'seeed-xiao-esp32s3',
    name: 'Seeed Studio XIAO ESP32S3 Sense',
    vendor: 'Seeed Studio',
    chip: 'ESP32-S3',
    runtime: 'MicroPython',
    flashBoardType: 'espressif', // auto-detects S3 → ESP32_GENERIC_S3
    mpyVariant: 'ESP32_GENERIC_S3',
    // Native USB-Serial/JTAG (303a:1001) — indistinguishable from a bare
    // ESP32-S3, so this is only ever offered as a pickable candidate.
    match: { vid: '303a', serialIncludes: 'xiao' },
  },
  {
    id: 'esp32-s3-generic',
    name: 'Generic ESP32-S3',
    vendor: 'Espressif',
    chip: 'ESP32-S3',
    runtime: 'MicroPython',
    flashBoardType: 'espressif',
    mpyVariant: 'ESP32_GENERIC_S3',
    match: { vid: '303a' },
    generic: true,
  },
  {
    id: 'esp32-generic',
    name: 'Generic ESP32',
    vendor: 'Espressif',
    chip: 'ESP32',
    runtime: 'MicroPython',
    flashBoardType: 'espressif',
    mpyVariant: 'ESP32_GENERIC',
    match: { vid: '10c4' }, // CP210x bridge, non-Atom
    generic: true,
  },
  {
    id: 'esp32-ch340',
    name: 'Generic ESP32 (CH340)',
    vendor: 'Espressif',
    chip: 'ESP32',
    runtime: 'MicroPython',
    flashBoardType: 'espressif',
    mpyVariant: 'ESP32_GENERIC',
    match: { vid: '1a86' }, // CH340 bridge
    generic: true,
  },
  {
    id: 'rpi-pico',
    name: 'Raspberry Pi Pico',
    vendor: 'Raspberry Pi',
    chip: 'RP2040',
    runtime: 'MicroPython',
    flashBoardType: 'raspberry-pi',
    mpyVariant: 'RPI_PICO',
    match: { vid: '2e8a' },
  },
  {
    id: 'arduino-uno',
    name: 'Arduino Uno',
    vendor: 'Arduino',
    chip: 'AVR',
    runtime: 'Arduino',
    flashBoardType: 'arduino',
    match: { vid: '2341' },
  },
]

export interface BoardPortInput {
  vendorId?: string
  productId?: string
  manufacturer?: string
  serialNumber?: string
  path?: string
}

export function getBoardById(id: string | null | undefined): BoardDef | undefined {
  if (!id) return undefined
  return BOARD_CATALOG.find((b) => b.id === id)
}

/** A stable key for remembering a board override across sessions. */
export function portKey(port: BoardPortInput): string {
  return port.serialNumber?.trim() || port.path || ''
}

export interface BoardMatch {
  /** Best resolved board (override, then auto-detect, then null). */
  board: BoardDef | null
  /** True when a unique, board-specific descriptor identified it. */
  exact: boolean
  /** True when the resolution came from a saved user override. */
  fromOverride: boolean
  /** Boards the user can pick from to refine an ambiguous match. */
  candidates: BoardDef[]
}

// Score how specifically a catalog entry matches a port. A higher score means a
// more distinguishing match (serial > manufacturer > pid > vid). Returns -1 for
// no match.
function scoreMatch(board: BoardDef, port: BoardPortInput): number {
  const m = board.match
  if (!m) return -1
  const vid = (port.vendorId ?? '').toLowerCase()
  const pid = (port.productId ?? '').toLowerCase()
  const mfr = (port.manufacturer ?? '').toLowerCase()
  const serial = (port.serialNumber ?? '').toLowerCase()

  if (m.vid && m.vid.toLowerCase() !== vid) return -1
  let score = m.vid ? 1 : 0
  if (m.pidPrefix) {
    if (!pid.startsWith(m.pidPrefix.toLowerCase())) return -1
    score += 2
  }
  if (m.manufacturerIncludes) {
    if (!mfr.includes(m.manufacturerIncludes.toLowerCase())) return -1
    score += 3
  }
  if (m.serialIncludes) {
    if (!serial.includes(m.serialIncludes.toLowerCase())) return -1
    score += 4
  }
  return score
}

/**
 * Resolve which board is on a port. A saved override always wins. Otherwise the
 * highest-scoring catalog match is used; if that match is only a chip-family
 * fallback (generic), `exact` is false and `candidates` lists the boards in that
 * family so the UI can offer a picker.
 */
export function identifyBoard(
  port: BoardPortInput | undefined,
  overrides: Record<string, string> = {},
): BoardMatch {
  if (!port) return { board: null, exact: false, fromOverride: false, candidates: [] }

  const override = getBoardById(overrides[portKey(port)])

  let best: BoardDef | null = null
  let bestScore = -1
  for (const board of BOARD_CATALOG) {
    const score = scoreMatch(board, port)
    // Prefer specific boards over generics on score ties.
    if (score > bestScore || (score === bestScore && best?.generic && !board.generic)) {
      best = board
      bestScore = score
    }
  }

  const resolved = override ?? best
  const chip = resolved?.chip
  // Offer every named product in the chip family plus a single generic fallback,
  // so the picker never shows two near-identical "Generic …" rows.
  const named = chip ? BOARD_CATALOG.filter((b) => b.chip === chip && !b.generic) : []
  const generic = chip ? BOARD_CATALOG.filter((b) => b.chip === chip && b.generic).slice(0, 1) : []
  const candidates = [...named, ...generic]

  return {
    board: resolved,
    exact: !override && !!best && !best.generic,
    fromOverride: !!override,
    candidates,
  }
}
