import { useEffect, useRef, useCallback } from 'react'
import { useStore, detectBoardType, getPreferredPort, detectExecutionProfiles, selectBestProfile, circuitPythonHint, isLikelyHardwarePort, type PortInfo } from '@/lib/store'

export function useAutoConnect() {
  const {
    selectedPort,
    autoConnect,
    lastPortPath,
    lastProfileId,
    availableProfiles,
    activeProfile,
    connectionDropped,
    setSelectedPort,
    setDetectedBoard,
    setAvailableProfiles,
    setActiveProfile,
    setLastPortPath,
    setLastProfileId,
    setConnectionDropped,
    addLogEntry,
  } = useStore()

  const lastPortRef = useRef(selectedPort)
  const wasConnectedRef = useRef(false)

  useEffect(() => { lastPortRef.current = selectedPort }, [selectedPort])
  useEffect(() => { wasConnectedRef.current = !!selectedPort }, [selectedPort])

  const attemptAutoConnect = useCallback((availablePorts: PortInfo[]) => {
    if (!autoConnect) return
    if (availablePorts.length === 0) return

    if (lastPortPath) {
      const match = availablePorts.find((p) => p.path === lastPortPath && isLikelyHardwarePort(p))
      if (match) {
        const needsPort = selectedPort !== match.path
        // Profiles aren't persisted, so on a fresh launch the port is restored
        // but the runtime profile is empty — repopulate it here too, not only
        // when the port itself changes.
        const needsProfiles = availableProfiles.length === 0 || !activeProfile
        if (needsPort || needsProfiles) {
          if (needsPort) setSelectedPort(match.path)
          setDetectedBoard(detectBoardType(match))
          const profiles = detectExecutionProfiles({ ...match, circuitPython: circuitPythonHint() })
          setAvailableProfiles(profiles)

          if (lastProfileId) {
            const saved = profiles.find((p) => p.id === lastProfileId)
            if (saved) {
              setActiveProfile(saved)
            } else {
              const best = selectBestProfile(profiles)
              if (best) setActiveProfile(best)
            }
          } else {
            const best = selectBestProfile(profiles)
            if (best) setActiveProfile(best)
          }

          if (needsPort && connectionDropped) {
            addLogEntry({ type: 'ok', message: `Reconnected to ${match.path}` })
            setConnectionDropped(false)
          }
        }
        return
      }
    }

    if (!selectedPort) {
      const preferred = getPreferredPort(availablePorts)
      if (preferred) {
        setSelectedPort(preferred.path)
        setDetectedBoard(detectBoardType(preferred))
        setLastPortPath(preferred.path)
        const profiles = detectExecutionProfiles({ ...preferred, circuitPython: circuitPythonHint() })
        setAvailableProfiles(profiles)
        const best = selectBestProfile(profiles)
        if (best) {
          setActiveProfile(best)
          setLastProfileId(best.id)
        }
      }
    }
  }, [autoConnect, lastPortPath, lastProfileId, selectedPort, availableProfiles, activeProfile, connectionDropped, setSelectedPort, setDetectedBoard, setAvailableProfiles, setActiveProfile, setLastPortPath, setLastProfileId, setConnectionDropped, addLogEntry])

  const checkConnectionDrop = useCallback((availablePorts: PortInfo[]) => {
    if (!selectedPort || !wasConnectedRef.current) return
    const stillPresent = availablePorts.some((p) => p.path === selectedPort)
    if (!stillPresent && !connectionDropped) {
      setConnectionDropped(true)
      addLogEntry({ type: 'warn', message: `Connection lost on ${selectedPort}. Waiting for reconnect...` })
    }
  }, [selectedPort, connectionDropped, setConnectionDropped, addLogEntry])

  return { attemptAutoConnect, checkConnectionDrop }
}
