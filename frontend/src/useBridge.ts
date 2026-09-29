/** Хранение состояния окна и подписка на его события. */

import { useCallback, useEffect, useRef, useState } from 'react'

import * as ipc from './ipc'
import type { AppState, Metrics } from './types'

const EMPTY_METRICS: Metrics = {
  frames_ingested: 0,
  frames_stale: 0,
  hands_rejected: 0,
}

export interface Bridge {
  state: AppState | null
  metrics: Metrics
  notice: string | null
  busy: boolean
  setMode: (mode: 'Control' | 'Translate') => Promise<void>
  setRecognition: (enabled: boolean) => Promise<void>
  setSpeech: (enabled: boolean) => Promise<void>
  stopSpeech: () => Promise<void>
  tap: (index: number) => Promise<void>
  finishPhrase: () => Promise<void>
  startRecording: (word: string, isDynamic: boolean) => Promise<void>
  cancelRecording: () => Promise<void>
  deleteSign: (id: string) => Promise<void>
  clearSigns: () => Promise<void>
  pushFrame: (frame: { time: number; hands: { x: number; y: number }[][] }) => Promise<void>
  dismiss: () => void
}

/**
 * Держит снимок состояния и переживает ошибки команд.
 *
 * Ошибка не роняет интерфейс: она показывается строкой и гасит кнопку,
 * чтобы пользователь мог повторить действие.
 */
export function useBridge(): Bridge {
  const [state, setState] = useState<AppState | null>(null)
  const [metrics, setMetrics] = useState<Metrics>(EMPTY_METRICS)
  const [notice, setNotice] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const clock = useRef(0)

  useEffect(() => {
    let alive = true

    void ipc
      .getState()
      .then((answer) => {
        if (!alive) return
        setState(answer.state)
        setMetrics(answer.metrics)
      })
      .catch((error: unknown) => {
        if (alive) setNotice(ipc.errorText(error))
      })

    const unlisten = ipc.onState((result) => {
      if (!alive) return
      setState(result.state)
      if (result.rejected.length > 0) {
        setNotice(
          `кисть отброшена: ${result.rejected[0]?.received ?? 0} точек вместо ` +
            `${result.rejected[0]?.expected ?? 0}`,
        )
      }
    })

    return () => {
      alive = false
      void unlisten.then((off) => off())
    }
  }, [])

  const run = useCallback(
    async (action: () => Promise<{ state?: AppState }>) => {
      setBusy(true)
      try {
        const answer = await action()
        if (answer.state) setState(answer.state)
        setNotice(null)
      } catch (error: unknown) {
        setNotice(ipc.errorText(error))
      } finally {
        setBusy(false)
      }
    },
    [],
  )

  const engineOnly = useCallback(
    async (action: () => Promise<AppState>) => {
      await run(async () => ({ state: await action() }))
    },
    [run],
  )

  const pushFrame = useCallback(
    async (frame: { time: number; hands: { x: number; y: number }[][] }) => {
      clock.current += 1 / 30
      try {
        const result = await ipc.pushFrame({ ...frame, time: clock.current })
        setState(result.state)
        setNotice(result.stale ? 'кадр устарел и был отброшен' : null)
      } catch (error: unknown) {
        setNotice(ipc.errorText(error))
      }
    },
    [],
  )

  return {
    state,
    metrics,
    notice,
    busy,
    setMode: (mode) => engineOnly(() => ipc.setMode(mode)),
    setRecognition: (enabled) => engineOnly(() => ipc.setRecognition(enabled)),
    setSpeech: (enabled) => engineOnly(() => ipc.setSpeech(enabled)),
    stopSpeech: () => engineOnly(() => ipc.stopSpeech()),
    tap: (index) => run(async () => ({ state: (await ipc.tap(index)).state })),
    finishPhrase: () => run(async () => ({ state: (await ipc.finishPhrase()).state })),
    startRecording: (word, isDynamic) =>
      engineOnly(() => ipc.startRecording(word, isDynamic)),
    cancelRecording: () => engineOnly(() => ipc.cancelRecording()),
    deleteSign: (id) => engineOnly(() => ipc.deleteSign(id)),
    clearSigns: () => engineOnly(() => ipc.clearSigns()),
    pushFrame,
    dismiss: () => setNotice(null),
  }
}
