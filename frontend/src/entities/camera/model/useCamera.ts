/**
 * Живое превью камеры.
 *
 * Кадр приходит двоичным пакетом: ширина и высота по четыре байта, дальше
 * готовый RGBA. Хук рисует буфер как есть и не тратит время на base64 и
 * JSON. Статус спрашивается редко: он меняется не каждый кадр.
 */

import { useCallback, useEffect, useRef, useState } from 'react'

import * as ipc from '@/shared/api/ipc'
import type { CameraRequest, CameraStatus, Settings } from '@/shared/api/types'

/**
 * Минимальная пауза между запросами кадра.
 *
 * Запросы идут друг за другом: следующий начинается сразу после ответа на
 * предыдущий. Сервер всегда отдаёт самый свежий кадр, поэтому ожидание
 * между запросами — это чистая задержка картинки.
 */
const POLL_MS = 16

/** Как часто обновлять статус камеры: счётчик кадров не самоцель. */
const STATUS_MS = 400

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
  /** Канвас для превью: хуку нужен настоящий элемент, а не ссылка извне. */
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
  const timer = useRef<number | null>(null)
  // Занятость опроса живёт в ref, а не в состоянии: состояние перерисовало
  // бы кнопку камеры двадцать раз в секунду, и она мигала бы.
  const polling = useRef(false)
  // Идёт ли цикл превью: им управляют запуск и остановка камеры.
  const running = useRef(false)

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

  /** Рисует двоичный пакет кадра на канвасе. */
  const draw = useCallback((packet: ArrayBuffer) => {
    if (packet.byteLength <= 8) return
    const view = new DataView(packet)
    const width = view.getUint32(0, true)
    const height = view.getUint32(4, true)
    if (width === 0 || height === 0) return
    const element = canvas.current
    const context = element?.getContext('2d')
    if (!element || !context) return
    const pixels = new Uint8ClampedArray(packet, 8)
    if (pixels.length < width * height * 4) return
    if (element.width !== width || element.height !== height) {
      element.width = width
      element.height = height
    }
    // Кадр уже в RGBA: канвас принимает буфер как есть, без попиксельного
    // цикла в JavaScript.
    context.putImageData(new ImageData(pixels, width, height), 0, 0)
  }, [])

  const poll = useCallback(async () => {
    // Пропущенный тик лучше второго запроса в полёте: ответы приходят
    // по порядку, и очередь из них только оттягивает картинку.
    if (polling.current) return
    polling.current = true
    try {
      draw(await ipc.cameraFrame())
    } catch (error: unknown) {
      setNotice(ipc.reportError(error))
    } finally {
      polling.current = false
    }
  }, [draw])

  const pump = useCallback(async () => {
    if (!running.current) return
    await poll()
    if (!running.current) return
    timer.current = window.setTimeout(() => void pump(), POLL_MS)
  }, [poll])

  const startLoop = useCallback(() => {
    running.current = true
    void pump()
  }, [pump])

  const stopLoop = useCallback(() => {
    running.current = false
    if (timer.current !== null) {
      window.clearTimeout(timer.current)
      timer.current = null
    }
  }, [])

  const start = useCallback(async () => {
    setBusy(true)
    try {
      const answer = await ipc.startCamera(request())
      setStatus(answer)
      setNotice(answer.error)
      startLoop()
    } catch (error: unknown) {
      setNotice(ipc.reportError(error))
    } finally {
      setBusy(false)
    }
  }, [request, startLoop])

  const stop = useCallback(async () => {
    stopLoop()
    setBusy(true)
    try {
      const answer = await ipc.stopCamera()
      setStatus(answer)
      setNotice(null)
      // Старый кадр на канвасе выглядел бы как живая камера: чистим.
      canvas.current?.getContext('2d')?.clearRect(0, 0, 100_000, 100_000)
    } catch (error: unknown) {
      setNotice(ipc.reportError(error))
    } finally {
      setBusy(false)
    }
  }, [stopLoop])

  useEffect(() => {
    let alive = true
    // Статус меняется не каждый кадр, поэтому спрашиваем его редко: так
    // приложение не перерисовывается на каждом принятом кадре.
    const refresh = () => {
      void ipc
        .cameraStatus()
        .then((answer) => {
          if (alive) setStatus((previous) => (same(previous, answer) ? previous : answer))
        })
        .catch((error: unknown) => {
          if (alive) setNotice(ipc.reportError(error))
        })
    }
    refresh()
    const statusTimer = window.setInterval(refresh, STATUS_MS)

    // Камера могла остаться включённой после перезагрузки окна: если она
    // ещё жива, продолжаем тянуть кадры.
    void ipc
      .cameraStatus()
      .then((answer) => {
        if (alive && answer.running) startLoop()
      })
      .catch(() => undefined)

    // Настройки нужны для показа и для запроса на включение. Их сбой не
    // должен мешать камере: окно подставит умолчания и без них.
    void ipc
      .getSettings()
      .then((answer) => {
        if (alive) setSettings(answer)
      })
      .catch((error: unknown) => {
        if (alive) setNotice(ipc.reportError(error))
      })

    return () => {
      alive = false
      window.clearInterval(statusTimer)
      stopLoop()
    }
  }, [startLoop, stopLoop])

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

/** Совпадают ли два статуса по полям, которые видит пользователь. */
function same(a: CameraStatus, b: CameraStatus): boolean {
  return (
    a.running === b.running &&
    a.device === b.device &&
    a.error === b.error &&
    a.frames === b.frames
  )
}
