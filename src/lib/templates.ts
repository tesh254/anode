export interface Template {
  id: string
  name: string
  description: string
  language: 'python' | 'arduino'
  board: 'espressif' | 'm5stack-atom' | 'm5stamp-s3' | 'raspberry-pi' | 'arduino'
  code: string
  icon: string
}

export const TEMPLATES: Template[] = [
  {
    id: 'blank',
    name: 'Blank Project',
    description: 'Start from scratch with an empty file',
    language: 'python',
    board: 'espressif',
    icon: 'FileCode2',
    code: `# main.py\n`,
  },
  {
    id: 'blink',
    name: 'Blink',
    description: 'LED blink example - the hello world of microcontrollers',
    language: 'python',
    board: 'espressif',
    icon: 'Zap',
    code: `import machine
import time

# Configure the built-in LED (GPIO 2 on most ESP32 boards)
led = machine.Pin(2, machine.Pin.OUT)

print("Starting blink example...")
print("LED will blink every 500ms")

while True:
    led.value(1)  # LED on
    time.sleep(0.5)
    led.value(0)  # LED off
    time.sleep(0.5)
`,
  },
  {
    id: 'wifi-connect',
    name: 'WiFi Connect',
    description: 'Connect to WiFi and display IP address',
    language: 'python',
    board: 'espressif',
    icon: 'Wifi',
    code: `import network
import time

# WiFi credentials
SSID = "your_wifi_ssid"
PASSWORD = "your_wifi_password"

def connect_wifi():
    wlan = network.WLAN(network.STA_IF)
    wlan.active(True)
    
    if not wlan.isconnected():
        print(f"Connecting to {SSID}...")
        wlan.connect(SSID, PASSWORD)
        
        # Wait for connection with timeout
        timeout = 30
        while not wlan.isconnected() and timeout > 0:
            time.sleep(1)
            timeout -= 1
            print(f"Waiting... {timeout}s remaining")
    
    if wlan.isconnected():
        print(f"Connected! IP: {wlan.ifconfig()[0]}")
        return True
    else:
        print("Failed to connect")
        return False

# Run
connect_wifi()
`,
  },
  {
    id: 'temperature-sensor',
    name: 'Temperature Sensor',
    description: 'Read temperature from DS18B20 sensor',
    language: 'python',
    board: 'espressif',
    icon: 'Thermometer',
    code: `import machine
import onewire
import ds18x20
import time

# Data pin connected to DS18B20 sensor
SENSOR_PIN = 4

# Initialize the sensor
dat = machine.Pin(SENSOR_PIN)
ds = ds18x20.DS18X20(onewire.OneWire(dat))

# Find all sensors on the bus
sensors = ds.scan()
print(f"Found {len(sensors)} sensor(s)")

if len(sensors) == 0:
    print("No sensors found!")
else:
    print("Reading temperature...")
    while True:
        ds.convert_temp()
        time.sleep_ms(750)  # Wait for conversion
        
        for sensor in sensors:
            temp = ds.read_temp(sensor)
            print(f"Temperature: {temp:.2f}°C")
        time.sleep(2)
`,
  },
  {
    id: 'oled-display',
    name: 'OLED Display',
    description: 'Display text on SSD1306 I2C OLED display',
    language: 'python',
    board: 'espressif',
    icon: 'Monitor',
    code: `from machine import Pin, I2C
import ssd1306
import time

# I2C pins for most ESP32 boards
# SCL = GPIO 22, SDA = GPIO 21
i2c = I2C(0, scl=Pin(22), sda=Pin(21), freq=400000)

# Create OLED display object (128x64 pixels)
display = ssd1306.SSD1306_I2C(128, 64, i2c)

# Clear display
display.fill(0)
display.text("Hello, Anode!", 0, 0, 1)
display.text("MicroPython OLED", 0, 16, 1)
display.text("Demo", 0, 32, 1)
display.show()

print("OLED display initialized")
time.sleep(2)

# Scroll demo
for i in range(0, 64, 4):
    display.fill(0)
    display.text("Scrolling", -i + 32, 28, 1)
    display.show()
    time.sleep(0.05)

print("Demo complete!")
`,
  },
  {
    id: 'servo-control',
    name: 'Servo Control',
    description: 'Control a servo motor with potentiometer',
    language: 'python',
    board: 'espressif',
    icon: 'Settings',
    code: `from machine import Pin, PWM, ADC
import time

# Servo connected to GPIO 15
SERVO_PIN = 15

# Potentiometer on ADC pin 34
POT_PIN = 34

# Initialize PWM for servo
servo = PWM(Pin(SERVO_PIN), freq=50)

# Initialize ADC for potentiometer
pot = ADC(Pin(POT_PIN))
pot.atten(ADC.ATTN_11DB)  # Full range: 0-4095 for 3.3V

def angle_to_duty(angle):
    # Convert angle (0-180) to duty cycle
    # Most servos: 1ms = 0°, 1.5ms = 90°, 2ms = 180°
    min_duty = 1638  # ~1ms for 0°
    max_duty = 8192  # ~2ms for 180°
    return int(min_duty + (max_duty - min_duty) * angle / 180)

print("Servo control started")
print("Adjust potentiometer to control servo position")

while True:
    # Read potentiometer value
    pot_value = pot.read()
    
    # Map pot value (0-4095) to angle (0-180)
    angle = int(pot_value * 180 / 4095)
    
    # Set servo position
    servo.duty(angle_to_duty(angle))
    
    print(f"Angle: {angle}°")
    time.sleep(0.1)
`,
  },
  {
    id: 'm5stack-atom-blink',
    name: 'Atom Blink',
    description: 'Blink the LED on M5Stack Atom Light (GPIO 27)',
    language: 'python',
    board: 'm5stack-atom',
    icon: 'Zap',
    code: `from machine import Pin
import time

# M5Stack Atom Light LED is on GPIO 27
led = Pin(27, Pin.OUT)

print("M5Stack Atom Light Blink")
while True:
    led.value(1)
    time.sleep(0.5)
    led.value(0)
    time.sleep(0.5)
`,
  },
  {
    id: 'm5stamp-s3-blink',
    name: 'StampS3 Blink',
    description: 'Blink the LED on M5StampS3 (GPIO 2)',
    language: 'python',
    board: 'm5stamp-s3',
    icon: 'Zap',
    code: `from machine import Pin
import time

# M5StampS3 LED is on GPIO 2
led = Pin(2, Pin.OUT)

print("M5StampS3 Blink")
while True:
    led.value(1)
    time.sleep(0.5)
    led.value(0)
    time.sleep(0.5)
`,
  },
  {
    id: 'pico-blink',
    name: 'Pico Blink',
    description: 'LED blink example for Raspberry Pi Pico',
    language: 'python',
    board: 'raspberry-pi',
    icon: 'Zap',
    code: `import machine
import time

# LED on Pico is on GPIO 25 (built-in)
led = machine.Pin(25, machine.Pin.OUT)

print("Pico Blink Example")
print("LED connected to GPIO 25")

while True:
    led.value(1)  # On
    time.sleep(0.5)
    led.value(0)  # Off
    time.sleep(0.5)
`,
  },
  {
    id: 'pico-wifi',
    name: 'Pico W WiFi',
    description: 'Connect Pico W to WiFi using CYW43',
    language: 'python',
    board: 'raspberry-pi',
    icon: 'Wifi',
    code: `import network
import time
import cyw43

# WiFi credentials
SSID = "your_wifi_ssid"
PASSWORD = "your_wifi_password"

# Initialize WiFi
cyw43.os_init()

def connect_wifi():
    wlan = network.WLAN(network.STA_IF)
    wlan.active(True)
    
    print(f"Connecting to {SSID}...")
    wlan.connect(SSID, PASSWORD)
    
    timeout = 30
    while not wlan.isconnected() and timeout > 0:
        time.sleep(1)
        timeout -= 1
        print(f"Waiting... {timeout}s")
    
    if wlan.isconnected():
        print(f"Connected! IP: {wlan.ifconfig()[0]}")
    else:
        print("Failed to connect")

connect_wifi()
`,
  },
  {
    id: 'arduino-blink',
    name: 'Arduino Blink',
    description: 'LED blink for Arduino (uses Arduino framework)',
    language: 'arduino',
    board: 'arduino',
    icon: 'Zap',
    code: `void setup() {
  // Initialize built-in LED pin as output
  pinMode(LED_BUILTIN, OUTPUT);
  Serial.begin(115200);
  Serial.println("Arduino Blink Started");
}

void loop() {
  digitalWrite(LED_BUILTIN, HIGH);  // Turn LED on
  Serial.println("LED ON");
  delay(1000);                       // Wait 1 second
  
  digitalWrite(LED_BUILTIN, LOW);   // Turn LED off
  Serial.println("LED OFF");
  delay(1000);                       // Wait 1 second
}
`,
  },
]

export function getTemplateById(id: string): Template | undefined {
  return TEMPLATES.find(t => t.id === id)
}

export function getTemplatesByBoard(board: Template['board']): Template[] {
  const blank = TEMPLATES.find(t => t.id === 'blank')!
  const isArduino = board === 'arduino'
  const blankForBoard: Template = { ...blank, board, language: isArduino ? 'arduino' : 'python', code: isArduino ? '// sketch.ino\n' : `# main.py\n` }
  const boardTemplates = TEMPLATES.filter(t => t.board === board && t.id !== 'blank')
  // M5Stack boards also show generic ESP32 templates (excluding other M5-specific ones)
  const espressifExtras = (board === 'm5stack-atom' || board === 'm5stamp-s3')
    ? TEMPLATES.filter(t => t.board === 'espressif' && t.id !== 'blank')
    : []
  return [blankForBoard, ...boardTemplates, ...espressifExtras]
}
