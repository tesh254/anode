/// <reference types="vite/client" />

interface Window {
  hardware: {
    getSerialPorts: () => Promise<any[]>
  }
}
