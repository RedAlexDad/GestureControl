/**
 * Обёртки над командами окна.
 *
 * Здесь единственное место, где живут строковые имена команд: сущности и
 * виджеты зовут осмысленные функции и не знают имён на проводе.
 */

import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

import type {
  AppMode,
  AppState,
  BridgeResult,
  CameraRequest,
  CameraStatus,
  FrameInput,
  GetState,
  Metrics,
  Settings,
} from '@/shared/api/types'

/** Событие окна: ответ на каждый разобранный кадр. */
export const STATE_EVENT = 'gesture://state'

/** Событие окна: камера включилась, остановилась или упала. */
export const CAMERA_EVENT = 'gesture://camera'

/** Настройки приложения, прочитанные окном из `.env` и окружения. */
export function getSettings(): Promise<Settings> {
  return invoke<Settings>('get_settings')
}

/** Показывает состояние и счётчики одним запросом. */
export function getState(): Promise<GetState> {
  return invoke<GetState>('get_state')
}

/** Переключает режим: управление или сурдоперевод. */
export function setMode(mode: AppMode): Promise<AppState> {
  return invoke<AppState>('set_mode', { request: { mode } })
}

/** Включает или выключает распознавание. */
export function setRecognition(enabled: boolean): Promise<AppState> {
  return invoke<AppState>('set_recognition', { request: { enabled } })
}

/** Включает или выключает озвучку. */
export function setSpeech(enabled: boolean): Promise<AppState> {
  return invoke<AppState>('set_speech', { request: { enabled } })
}

/** Останавливает текущую речь. */
export function stopSpeech(): Promise<AppState> {
  return invoke<AppState>('stop_speech')
}

/** Переключает экран демо-интерфейса. */
export function tap(index: number): Promise<BridgeResult> {
  return invoke<BridgeResult>('tap', { request: { index } })
}

/** Произносит накопленную фразу и очищает её. */
export function finishPhrase(): Promise<BridgeResult> {
  return invoke<BridgeResult>('finish_phrase')
}

/** Начинает запись жеста для слова. */
export function startRecording(word: string, isDynamic: boolean): Promise<AppState> {
  return invoke<AppState>('start_recording', { request: { word, is_dynamic: isDynamic } })
}

/** Отменяет запись, если она идёт. */
export function cancelRecording(): Promise<AppState> {
  return invoke<AppState>('cancel_recording')
}

/** Удаляет жест из словаря по идентификатору. */
export function deleteSign(id: string): Promise<AppState> {
  return invoke<AppState>('delete_sign', { request: { id } })
}

/** Очищает словарь целиком. */
export function clearSigns(): Promise<AppState> {
  return invoke<AppState>('clear_signs')
}

/** Отправляет кадр: вход для точек кистей из интерфейса. */
export function pushFrame(frame: FrameInput): Promise<BridgeResult> {
  return invoke<BridgeResult>('push_frame', { frame })
}

/** Счётчики потока кадров без полного состояния. */
export function getMetrics(): Promise<Metrics> {
  return invoke<Metrics>('get_metrics')
}

/** Открывает устройство и запускает фоновое чтение кадров. */
export function startCamera(request: CameraRequest): Promise<CameraStatus> {
  return invoke<CameraStatus>('start_camera', { request })
}

/** Останавливает чтение и освобождает устройство. */
export function stopCamera(): Promise<CameraStatus> {
  return invoke<CameraStatus>('stop_camera')
}

/** Текущий статус камеры без кадра. */
export function cameraStatus(): Promise<CameraStatus> {
  return invoke<CameraStatus>('camera_status')
}

/**
 * Последний кадр превью одним запросом.
 *
 * Ответ — двоичный пакет: ширина и высота по четыре байта, дальше готовый
 * RGBA. Ни base64, ни JSON: интерфейс рисует буфер как есть.
 */
export function cameraFrame(): Promise<ArrayBuffer> {
  return invoke<ArrayBuffer>('camera_frame')
}

/** Подписывается на смену состояния камеры. */
export function onCamera(handler: (status: CameraStatus) => void): Promise<UnlistenFn> {
  return listen<CameraStatus>(CAMERA_EVENT, (event) => handler(event.payload))
}

/** Подписывается на ответы окна по каждому кадру. */
export function onState(handler: (result: BridgeResult) => void): Promise<UnlistenFn> {
  return listen<BridgeResult>(STATE_EVENT, (event) => handler(event.payload))
}

/**
 * Текст ошибки из отказа команды.
 *
 * Окно возвращает `{ message }`, но при падении процесса приходит
 * строка, поэтому приводим оба вида к одному тексту.
 */
export function errorText(error: unknown): string {
  if (typeof error === 'string') return error
  if (error !== null && typeof error === 'object' && 'message' in error) {
    const message = (error as { message: unknown }).message
    if (typeof message === 'string') return message
  }
  return 'неизвестная ошибка'
}

/**
 * Текст ошибки для показа и для журнала окна.
 *
 * Tauri разбирает аргументы команды до входа в обработчик, поэтому отказ
 * вида `missing field` не доходит до кода окна и в журнале не виден: его
 * знает только интерфейс. Пересылаем текст отдельной командой, иначе
 * опечатка в имени поля молча остаётся только в подсказке на экране.
 */
export function reportError(error: unknown): string {
  const text = errorText(error)
  console.error(`команда окна отказала: ${text}`)
  invoke('log_client_error', { message: text }).catch(() => undefined)
  return text
}
