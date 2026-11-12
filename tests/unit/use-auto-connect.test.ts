import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderHook, act } from '@testing-library/react'
import { useAutoConnect } from '@/lib/use-auto-connect'
import { useStore, detectExecutionProfiles } from '@/lib/store'

vi.mock('@monaco-editor/react', () => ({
  default: () => null,
}))

vi.mock('next-themes', () => ({
  useTheme: () => ({ theme: 'dark', setTheme: vi.fn() }),
}))

describe('useAutoConnect', () => {
  beforeEach(() => {
    useStore.getState().reset()
  })

  it('returns attemptAutoConnect and checkConnectionDrop functions', () => {
    const { result } = renderHook(() => useAutoConnect())
    expect(typeof result.current.attemptAutoConnect).toBe('function')
    expect(typeof result.current.checkConnectionDrop).toBe('function')
  })

  it('attemptAutoConnect does nothing when autoConnect is disabled', () => {
    useStore.getState().setAutoConnect(false)
    const { result } = renderHook(() => useAutoConnect())
    const ports = [{ path: '/dev/cu.usbserial-1410', vendorId: '10c4', productId: 'ea60' }]
    act(() => result.current.attemptAutoConnect(ports))
    expect(useStore.getState().selectedPort).toBe('')
  })

  it('attemptAutoConnect does nothing when no ports available', () => {
    const { result } = renderHook(() => useAutoConnect())
    act(() => result.current.attemptAutoConnect([]))
    expect(useStore.getState().selectedPort).toBe('')
  })

  it('attemptAutoConnect reconnects to lastPortPath when port reappears', () => {
    useStore.getState().setLastPortPath('/dev/cu.usbserial-1410')
    useStore.getState().setConnectionDropped(true)
    const { result } = renderHook(() => useAutoConnect())
    const ports = [{ path: '/dev/cu.usbserial-1410', vendorId: '10c4', productId: 'ea60' }]
    act(() => result.current.attemptAutoConnect(ports))
    expect(useStore.getState().selectedPort).toBe('/dev/cu.usbserial-1410')
    expect(useStore.getState().connectionDropped).toBe(false)
  })

  it('attemptAutoConnect selects preferred port when no lastPortPath and no current selection', () => {
    useStore.getState().setSelectedPort('')
    useStore.getState().setLastPortPath('')
    const { result } = renderHook(() => useAutoConnect())
    const ports = [{ path: '/dev/cu.usbserial-1410', vendorId: '10c4', productId: 'ea60' }]
    act(() => result.current.attemptAutoConnect(ports))
    expect(useStore.getState().selectedPort).toBe('/dev/cu.usbserial-1410')
    expect(useStore.getState().lastPortPath).toBe('/dev/cu.usbserial-1410')
  })

  it('attemptAutoConnect sets available profiles when connecting', () => {
    useStore.getState().setSelectedPort('')
    useStore.getState().setLastPortPath('')
    const { result } = renderHook(() => useAutoConnect())
    const ports = [{ path: '/dev/cu.usbserial-1410', vendorId: '10c4', productId: 'ea60' }]
    act(() => result.current.attemptAutoConnect(ports))
    expect(useStore.getState().availableProfiles.length).toBeGreaterThan(0)
    expect(useStore.getState().activeProfile).not.toBeNull()
  })

  it('attemptAutoConnect restores lastProfileId if it matches a profile', () => {
    useStore.getState().setLastPortPath('/dev/cu.usbserial-1410')
    const profiles = detectExecutionProfiles({ vendorId: '10c4', productId: 'ea60', path: '/dev/cu.usbserial-1410' })
    const best = profiles.find(p => p.confidence === 'high')
    useStore.getState().setLastProfileId(best?.id ?? '')
    const { result } = renderHook(() => useAutoConnect())
    const ports = [{ path: '/dev/cu.usbserial-1410', vendorId: '10c4', productId: 'ea60' }]
    act(() => result.current.attemptAutoConnect(ports))
    expect(useStore.getState().activeProfile?.id).toBe(best?.id)
  })

  it('checkConnectionDrop does nothing when no port is selected', () => {
    const { result } = renderHook(() => useAutoConnect())
    act(() => result.current.checkConnectionDrop([{ path: '/dev/other' }]))
    expect(useStore.getState().connectionDropped).toBe(false)
  })

  it('checkConnectionDrop sets connectionDropped when selected port disappears', () => {
    useStore.getState().setSelectedPort('/dev/cu.usbserial-1410')
    const { result } = renderHook(() => useAutoConnect())
    act(() => result.current.checkConnectionDrop([]))
    expect(useStore.getState().connectionDropped).toBe(true)
  })

  it('checkConnectionDrop does not set dropped when port is still present', () => {
    useStore.getState().setSelectedPort('/dev/cu.usbserial-1410')
    const { result } = renderHook(() => useAutoConnect())
    act(() => result.current.checkConnectionDrop([{ path: '/dev/cu.usbserial-1410' }]))
    expect(useStore.getState().connectionDropped).toBe(false)
  })

  it('checkConnectionDrop does not re-set dropped if already dropped', () => {
    useStore.getState().setSelectedPort('/dev/cu.usbserial-1410')
    useStore.getState().setConnectionDropped(true)
    const { result } = renderHook(() => useAutoConnect())
    act(() => result.current.checkConnectionDrop([]))
    expect(useStore.getState().connectionDropped).toBe(true)
  })

  it('attemptAutoConnect logs reconnect message when connectionDropped is true', () => {
    useStore.getState().setLastPortPath('/dev/cu.usbserial-1410')
    useStore.getState().setConnectionDropped(true)
    const { result } = renderHook(() => useAutoConnect())
    const ports = [{ path: '/dev/cu.usbserial-1410', vendorId: '10c4', productId: 'ea60' }]
    act(() => result.current.attemptAutoConnect(ports))
    const logs = useStore.getState().logEntries
    expect(logs.some(l => l.message.includes('Reconnected'))).toBe(true)
  })

  it('attemptAutoConnect does not change port if already selected and present', () => {
    useStore.getState().setSelectedPort('/dev/cu.usbserial-1410')
    useStore.getState().setLastPortPath('/dev/cu.usbserial-1410')
    const { result } = renderHook(() => useAutoConnect())
    const ports = [{ path: '/dev/cu.usbserial-1410', vendorId: '10c4', productId: 'ea60' }]
    act(() => result.current.attemptAutoConnect(ports))
    expect(useStore.getState().selectedPort).toBe('/dev/cu.usbserial-1410')
  })

  it('attemptAutoConnect falls back to best profile when lastProfileId does not match', () => {
    useStore.getState().setLastPortPath('/dev/cu.usbserial-1410')
    useStore.getState().setLastProfileId('nonexistent-profile')
    const { result } = renderHook(() => useAutoConnect())
    const ports = [{ path: '/dev/cu.usbserial-1410', vendorId: '10c4', productId: 'ea60' }]
    act(() => result.current.attemptAutoConnect(ports))
    expect(useStore.getState().activeProfile).not.toBeNull()
    expect(useStore.getState().activeProfile?.confidence).toBe('high')
  })
})
