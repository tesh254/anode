// Pure, dependency-free helpers for the firmware-flash + board-detection paths.
// Kept free of electron/serialport imports so they're trivially unit-testable.

export interface FlashAttempt {
  baud: string
  /** esptool --before reset mode. */
  before: 'default_reset' | 'no_reset'
}

// Parse the first line of a CircuitPython boot_out.txt, e.g.:
//   "Adafruit CircuitPython 9.0.0 on 2024-...; Seeed XIAO ESP32S3 with ESP32S3"
// → { version: "9.0.0", boardName: "Seeed XIAO ESP32S3" }
export function parseBootOut(text: string): { boardName?: string; version?: string } {
  const firstLine = text.split('\n')[0] ?? ''
  const version = firstLine.match(/CircuitPython\s+(\S+)/)?.[1]
  const boardName = firstLine.match(/;\s*(.+?)\s+with\s/)?.[1]?.trim()
  return { version, boardName }
}

// True when the board can't be reset into the bootloader by esptool's DTR/RTS
// alone — i.e. Espressif native USB (VID 303a) or a macOS native-USB CDC port.
// These boards get the 1200-baud touch before flashing.
export function needsManualReset(vendorId: string | undefined, portPath: string): boolean {
  const vid = (vendorId ?? '').toLowerCase()
  return vid === '303a' || /usbmodem/i.test(portPath ?? '')
}

// esptool connect attempts. After a 1200-baud touch the board is already in
// download mode, so try no_reset first; otherwise let esptool drive the reset.
export function flashAttempts(touched: boolean): FlashAttempt[] {
  return touched
    ? [
        { baud: '115200', before: 'no_reset' },
        { baud: '460800', before: 'default_reset' },
        { baud: '115200', before: 'default_reset' },
      ]
    : [
        { baud: '460800', before: 'default_reset' },
        { baud: '115200', before: 'default_reset' },
        { baud: '115200', before: 'no_reset' },
      ]
}

// Where the RP2040 UF2 bootloader drive (RPI-RP2) mounts, per OS.
export function rp2040MountPoints(platform: string, user: string | undefined): string[] {
  return platform === 'darwin'
    ? ['/Volumes/RPI-RP2']
    : [`/media/${user}/RPI-RP2`, `/run/media/${user}/RPI-RP2`]
}

// Map esptool `flash_id` / chip-detection output to a MicroPython firmware slug.
export function chipTypeFromFlashId(output: string): string {
  if (output.includes('ESP8266')) return 'ESP8266'
  if (output.includes('ESP32-S2')) return 'ESP32_GENERIC_S2'
  if (output.includes('ESP32-S3')) return 'ESP32_GENERIC_S3'
  if (output.includes('ESP32-C3')) return 'ESP32_GENERIC_C3'
  if (output.includes('ESP32-C6')) return 'ESP32_GENERIC_C6'
  return 'ESP32_GENERIC'
}

// Best-effort chip family from the USB vendor id (303a = Espressif native USB → S3).
export function chipTypeFromVid(vendorId: string | undefined): string | null {
  return (vendorId ?? '').toLowerCase() === '303a' ? 'ESP32_GENERIC_S3' : null
}

// Flash offset: S2/S3/C3/C6 and ESP8266 flash at 0x0; the original ESP32 at 0x1000.
export function flashOffset(chipType: string): string {
  return chipType === 'ESP8266' || chipType.includes('C3') || chipType.includes('C6') || chipType.includes('S3')
    ? '0x0'
    : '0x1000'
}

// True when an esptool error message indicates a connection/sync failure (worth
// retrying with a different reset/baud) rather than a missing-tool error.
export function isEsptoolConnectionError(message: string): boolean {
  return (
    message.includes('No serial data received') ||
    message.includes('Failed to connect') ||
    message.includes('chip stopped responding') ||
    message.includes('StopIteration')
  )
}
