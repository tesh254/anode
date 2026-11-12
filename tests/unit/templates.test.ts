import { describe, it, expect } from 'vitest'
import { TEMPLATES, getTemplateById, getTemplatesByBoard } from '@/lib/templates'

describe('TEMPLATES', () => {
  it('has 11 templates', () => {
    expect(TEMPLATES).toHaveLength(11)
  })

  it('all templates have required fields', () => {
    for (const t of TEMPLATES) {
      expect(t.id).toBeTruthy()
      expect(t.name).toBeTruthy()
      expect(t.description).toBeTruthy()
      expect(['python', 'arduino']).toContain(t.language)
      expect(['espressif', 'm5stack-atom', 'm5stamp-s3', 'raspberry-pi', 'arduino']).toContain(t.board)
      expect(t.code).toBeTruthy()
      expect(t.icon).toBeTruthy()
    }
  })

  it('all template ids are unique', () => {
    const ids = TEMPLATES.map(t => t.id)
    expect(new Set(ids).size).toBe(ids.length)
  })

  it('has at least one template per board type', () => {
    const boards = ['espressif', 'm5stack-atom', 'm5stamp-s3', 'raspberry-pi', 'arduino'] as const
    for (const board of boards) {
      const templates = getTemplatesByBoard(board)
      expect(templates.length).toBeGreaterThan(0)
    }
  })

  it('espressif templates are all python language', () => {
    const espTemplates = getTemplatesByBoard('espressif')
    for (const t of espTemplates) {
      expect(t.language).toBe('python')
    }
  })

  it('arduino blank template uses arduino language', () => {
    const arduinoTemplates = getTemplatesByBoard('arduino')
    const blank = arduinoTemplates.find(t => t.id === 'blank')
    expect(blank?.language).toBe('arduino')
  })

  it('arduino templates include arduino language', () => {
    const arduinoTemplates = getTemplatesByBoard('arduino')
    expect(arduinoTemplates.some(t => t.language === 'arduino')).toBe(true)
  })

  it('blink template exists and is for espressif', () => {
    const blink = getTemplateById('blink')
    expect(blink).toBeDefined()
    expect(blink!.name).toBe('Blink')
    expect(blink!.board).toBe('espressif')
    expect(blink!.language).toBe('python')
    expect(blink!.code).toContain('machine.Pin')
  })

  it('pico-blink template is for raspberry-pi', () => {
    const picoBlink = getTemplateById('pico-blink')
    expect(picoBlink).toBeDefined()
    expect(picoBlink!.board).toBe('raspberry-pi')
    expect(picoBlink!.code).toContain('machine.Pin(25')
  })

  it('arduino-blink template uses arduino language', () => {
    const arduinoBlink = getTemplateById('arduino-blink')
    expect(arduinoBlink).toBeDefined()
    expect(arduinoBlink!.language).toBe('arduino')
    expect(arduinoBlink!.code).toContain('void setup()')
    expect(arduinoBlink!.code).toContain('void loop()')
  })

  it('wifi-connect template has WiFi credentials placeholders', () => {
    const wifi = getTemplateById('wifi-connect')
    expect(wifi).toBeDefined()
    expect(wifi!.code).toContain('your_wifi_ssid')
    expect(wifi!.code).toContain('your_wifi_password')
  })

  it('temperature-sensor template references DS18B20', () => {
    const temp = getTemplateById('temperature-sensor')
    expect(temp).toBeDefined()
    expect(temp!.code).toContain('ds18x20')
  })

  it('oled-display template references SSD1306', () => {
    const oled = getTemplateById('oled-display')
    expect(oled).toBeDefined()
    expect(oled!.code).toContain('ssd1306')
  })

  it('servo-control template references PWM and ADC', () => {
    const servo = getTemplateById('servo-control')
    expect(servo).toBeDefined()
    expect(servo!.code).toContain('PWM')
    expect(servo!.code).toContain('ADC')
  })

  it('pico-wifi template references cyw43', () => {
    const picoWifi = getTemplateById('pico-wifi')
    expect(picoWifi).toBeDefined()
    expect(picoWifi!.code).toContain('cyw43')
  })

  it('m5stack-atom-blink template references GPIO 27', () => {
    const atom = getTemplateById('m5stack-atom-blink')
    expect(atom).toBeDefined()
    expect(atom!.board).toBe('m5stack-atom')
    expect(atom!.code).toContain('Pin(27')
  })

  it('m5stamp-s3-blink template references GPIO 2', () => {
    const stamp = getTemplateById('m5stamp-s3-blink')
    expect(stamp).toBeDefined()
    expect(stamp!.board).toBe('m5stamp-s3')
    expect(stamp!.code).toContain('Pin(2')
  })
})

describe('getTemplateById', () => {
  it('returns template for valid id', () => {
    const result = getTemplateById('blink')
    expect(result).toBeDefined()
    expect(result!.id).toBe('blink')
  })

  it('returns undefined for invalid id', () => {
    expect(getTemplateById('nonexistent')).toBeUndefined()
  })

  it('returns undefined for empty string', () => {
    expect(getTemplateById('')).toBeUndefined()
  })
})

describe('getTemplatesByBoard', () => {
  it('filters espressif templates', () => {
    const results = getTemplatesByBoard('espressif')
    expect(results.length).toBeGreaterThan(0)
    for (const t of results) {
      expect(t.board).toBe('espressif')
    }
  })

  it('m5stack-atom templates include board-specific and espressif templates', () => {
    const results = getTemplatesByBoard('m5stack-atom')
    expect(results.length).toBeGreaterThan(0)
    const atomSpecific = results.filter(t => t.board === 'm5stack-atom')
    const espressifShared = results.filter(t => t.board === 'espressif')
    expect(atomSpecific.length).toBeGreaterThan(0)
    expect(espressifShared.length).toBeGreaterThan(0)
  })

  it('m5stamp-s3 templates include board-specific and espressif templates', () => {
    const results = getTemplatesByBoard('m5stamp-s3')
    expect(results.length).toBeGreaterThan(0)
    const stampSpecific = results.filter(t => t.board === 'm5stamp-s3')
    const espressifShared = results.filter(t => t.board === 'espressif')
    expect(stampSpecific.length).toBeGreaterThan(0)
    expect(espressifShared.length).toBeGreaterThan(0)
  })

  it('filters raspberry-pi templates', () => {
    const results = getTemplatesByBoard('raspberry-pi')
    expect(results.length).toBeGreaterThan(0)
    for (const t of results) {
      expect(t.board).toBe('raspberry-pi')
    }
  })

  it('filters arduino templates', () => {
    const results = getTemplatesByBoard('arduino')
    expect(results.length).toBeGreaterThan(0)
    for (const t of results) {
      expect(t.board).toBe('arduino')
    }
  })

  it('returns blank template even for board with no other templates', () => {
    const results = getTemplatesByBoard('esp8266' as Template['board'])
    expect(results.length).toBeGreaterThanOrEqual(1)
    expect(results[0].id).toBe('blank')
  })

  it('returns new array references (not mutated)', () => {
    const a = getTemplatesByBoard('espressif')
    const b = getTemplatesByBoard('espressif')
    expect(a).not.toBe(b)
    expect(a).toEqual(b)
  })
})
