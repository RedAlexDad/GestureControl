/**
 * Обёртки над командами окна.
 *
 * Здесь единственное место, где живут имена команд: компоненты зовут
 * осмысленные функции и не знают строковых имён, которые имённо они.
 */

import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

import type {
  AppMode,
  AppState,
  BridgeResult,
  CameraFrameResult,
  CameraRequest,
  CameraStatus,
  FrameInput,
  GetState,
  Metrics,
} from './types'

/** Событие окна: ответ на каждый разобранный кадр. */
export const STATE_EVENT = 'gesture://state'

/** Событие окна: камера включилась, остановилась или упала. */
export const CAMERA_EVENT = 'gesture://camera'

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
  return invoke<AppState>('start_recording', { request: { word, isDynamic } })
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

/** Отправляет кадр: единственный вход для точек кистей. */
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
 * Статус и последний кадр одним запросом.
 *
 * Кадр большой, поэтому тянуть его по событию на каждый кадр камеры
 * нельзя: интерфейс сам решает, как часто смотрит.
 */
export function cameraFrame(): Promise<CameraFrameResult> {
  return invoke<CameraFrameResult>('camera_frame')
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
