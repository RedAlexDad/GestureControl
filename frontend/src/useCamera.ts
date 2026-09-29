/**
 * Живое превью камеры.
 *
 * Кадры приходят из окна по запросу, а не событием: base64-картинка весит
 * сотни килобайт, и событие на каждый кадр камеры забило бы очередь IPC.
 * Поэтому хук сам решает, как часто смотреть, и рисует на канвасе.
 */

import { useCallback, useEffect, useRef, useState } from 'react'

import * as ipc from './ipc'
import type { CameraFrame, CameraRequest, CameraStatus, Settings } from './types'

/** Как часто тянуть кадр, пока камера включена. */
const POLL_MS = 200

/**
 * Запрос, пока настройки не пришли.
 *
 * Нули означают «возьми умолчание у окна»: так камера стартует даже тогда,
 * когда `get_settings` не ответил.
 */
const REQUEST: CameraRequest = { device: '', width: 0, height: 0, fps: 0 }

const OFF: CameraStatus = { running: false, device: '', frames: 0, error: null }

export interface Camera {
  status: CameraStatus
  notice: string | null
  busy: boolean
  /** Настройки окна: `null`, пока `get_settings` не ответил. */
  settings: Settings | null
  /** Канвас для превью: хуку нужно настоящий элемент, а не ссылка извне. */
  canvasRef: (element: HTMLCanvasElement | null) => void
  toggle: () => void
  stop: () => void
  dismiss: () => void
}

/**
 * Держит состояние камеры и рисует последний кадр.
 *
 * Ошибка не роняет интерфейс: она показывается строкой под кнопкой, потому
 * что камера чаще всего отказывает по понятным причинам — нет устройства,
 * нет прав, устройство занято.
 */
export function useCamera(): Camera {
  const [status, setStatus] = useState<CameraStatus>(OFF)
  const [notice, setNotice] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [settings, setSettings] = useState<Settings | null>(null)
  const canvas = useRef<HTMLCanvasElement | null>(null)
  const image = useRef<ImageData | null>(null)
  const timer = useRef<number | null>(null)

  /** Запрос на включение из настроек окна, а не из констант интерфейса. */
  const request = useCallback((): CameraRequest => {
    const camera = settings?.camera
    if (!camera) return REQUEST
    return {
      device: camera.device,
      width: camera.width,
      height: camera.height,
      fps: camera.fps,
    }
  }, [settings])

  const draw = useCallback((frame: CameraFrame) => {
    const element = canvas.current
    const context = element?.getContext('2d')
    if (!element || !context) return
    if (element.width !== frame.width || element.height !== frame.height) {
      element.width = frame.width
      element.height = frame.height
      image.current = null
    }
    // Буфер переиспользуется между кадрами: новый ImageData на каждом кадре
    // означал бы мусор в куче каждые 200 мс.
    if (!image.current) {
      image.current = context.createImageData(frame.width, frame.height)
    }
    const pixels = image.current.data
    const rgb = decode(frame.rgb)
    // Кадр приходит в RGB24, а канвас ждёт RGBA: разворачиваем на месте.
    const limit = Math.min(rgb.length, (pixels.length / 4) * 3)
    for (let source = 0, target = 0; source + 2 < limit; source += 3, target += 4) {
      pixels[target] = rgb[source] ?? 0
      pixels[target + 1] = rgb[source + 1] ?? 0
      pixels[target + 2] = rgb[source + 2] ?? 0
      pixels[target + 3] = 255
    }
    context.putImageData(image.current, 0, 0)
  }, [])

  const poll = useCallback(async () => {
    try {
      const answer = await ipc.cameraFrame()
      setStatus(answer.status)
      if (answer.frame) draw(answer.frame)
      if (answer.status.error) setNotice(answer.status.error)
    } catch (error: unknown) {
      setNotice(ipc.errorText(error))
    }
  }, [draw])

  const stopLoop = useCallback(() => {
    if (timer.current !== null) {
      window.clearInterval(timer.current)
      timer.current = null
    }
  }, [])

  const start = useCallback(async () => {
    setBusy(true)
    try {
      const answer = await ipc.startCamera(request())
      setStatus(answer)
      setNotice(answer.error)
      await poll()
      stopLoop()
      timer.current = window.setInterval(() => void poll(), POLL_MS)
    } catch (error: unknown) {
      setNotice(ipc.errorText(error))
    } finally {
      setBusy(false)
    }
  }, [poll, request, stopLoop])

  const stop = useCallback(async () => {
    stopLoop()
    setBusy(true)
    try {
      const answer = await ipc.stopCamera()
      setStatus(answer)
      setNotice(null)
      // Старый кадр на канвасе выглядел бы как живая камера: чистим.
      canvas.current?.getContext('2d')?.clearRect(0, 0, 100_000, 100_000)
      image.current = null
    } catch (error: unknown) {
      setNotice(ipc.errorText(error))
    } finally {
      setBusy(false)
    }
  }, [stopLoop])

  useEffect(() => {
    let alive = true
    // Камера могла остаться включённой после перезагрузки окна: спрашиваем
    // статус и продолжаем опрос, если она ещё жива.
    void ipc
      .cameraStatus()
      .then((answer) => {
        if (!alive) return
        setStatus(answer)
        if (answer.running) {
          void poll()
          timer.current = window.setInterval(() => void poll(), POLL_MS)
        }
      })
      .catch((error: unknown) => {
        if (alive) setNotice(ipc.errorText(error))
      })
    // Настройки нужны для показа и для запроса на включение. Их сбой не
    // должен мешать камере: окно подставит умолчания и без них.
    void ipc
      .getSettings()
      .then((answer) => {
        if (alive) setSettings(answer)
      })
      .catch((error: unknown) => {
        if (alive) setNotice(ipc.errorText(error))
      })
    return () => {
      alive = false
      if (timer.current !== null) window.clearInterval(timer.current)
    }
  }, [poll])

  return {
    status,
    notice,
    busy,
    settings,
    canvasRef: (element) => {
      canvas.current = element
    },
    toggle: () => void (status.running ? stop() : start()),
    stop: () => void stop(),
    dismiss: () => setNotice(null),
  }
}

/** Разбирает base64 в байты без копий по одному символу. */
function decode(encoded: string): Uint8Array {
  const binary = window.atob(encoded)
  const bytes = new Uint8Array(binary.length)
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index)
  }
  return bytes
}
