/**
 * Контракт IPC: зеркало структур из `gesture-bridge` и `src-tauri`.
 *
 * Имена полей повторяют Rust как есть, без camelCase. Rust сериализует
 * без `rename_all`, поэтому любая переделка здесь должна начинаться с
 * правки структур на стороне окна, иначе поля разъедутся.
 */

/** Режим приложения. Соответствует `gesture_core::AppMode`. */
export type AppMode = 'Control' | 'Translate'

/** Встроенный жест. Соответствует `gesture_core::Gesture`. */
export type Gesture =
  | 'Idle'
  | 'ThumbsUp'
  | 'OpenPalm'
  | 'Pointing'
  | 'Fist'
  | 'Victory'
  | 'Ok'
  | 'CallMe'
  | 'SwipeRight'
  | 'SwipeLeft'
  | 'SwipeUp'
  | 'SwipeDown'

/** Команда интерфейса. Соответствует `gesture_core::Command`. */
export type Command =
  | 'Confirm'
  | 'Pause'
  | 'Select'
  | 'NextScreen'
  | 'PreviousScreen'
  | 'Increase'
  | 'Decrease'

/**
 * Текущий жест. `BuiltIn` и `Custom` — варианты с полями, поэтому
 * serde отдаёт их объектами вида `{ BuiltIn: 'Fist' }`.
 */
export type Sign =
  | 'None'
  | { BuiltIn: Gesture }
  | { Custom: { id: string; word: string } }

/**
 * Состояние записи. У варианта `kind` поле `value` есть не всегда,
 * поэтому оно необязательное.
 */
export type RecordingState =
  | { kind: 'Idle' }
  | { kind: 'Countdown'; value: number }
  | { kind: 'Recording'; value: number }

/** Точка кисти в пикселях кадра. */
export interface PointSer {
  x: number
  y: number
}

/**
 * Запрос на включение камеры. Нулевое поле означает «взять умолчание»,
 * поэтому отдельно передавать 640x480 не нужно. Соответствует
 * `gesture_control_lib::camera::CameraRequest`.
 */
export interface CameraRequest {
  device: string
  width: number
  height: number
  fps: number
}

/** Состояние камеры. Соответствует `camera::CameraStatus`. */
export interface CameraStatus {
  running: boolean
  device: string
  frames: number
  error: string | null
}

/**
 * Кадр превью: пиксели RGB24 в base64. Соответствует `camera::CameraFrame`.
 *
 * `rgb` — не data-URI: канвас ждёт сырые байты, а не строку, поэтому
 * разбор base64 живёт в `useCamera`.
 */
export interface CameraFrame {
  width: number
  height: number
  rgb: string
}

/** Статус и кадр одним ответом. Соответствует `camera::CameraFrameResult`. */
export interface CameraFrameResult {
  status: CameraStatus
  frame: CameraFrame | null
}

/** Снимок движка: всё, что нужно нарисовать. */
export interface EngineSnapshot {
  mode: AppMode
  recognition_enabled: boolean
  speech_enabled: boolean
  sign: Sign
  streaming_word: string | null
  phrase: string
  recording: RecordingState
  notice: string | null
  signs_count: number
  screen_index: number
  zoom: number
  volume: number
  parameter: number
  hands_visible: number
  hands: PointSer[][]
}

/** Строка списка пользовательских жестов. */
export interface SignRow {
  id: string
  /** Слово без оформления, как в словаре. */
  word: string
  /** Подпись для показа. */
  title: string
  hand_count: number
  is_dynamic: boolean
}

/** Экран демо-интерфейса. */
export interface ScreenRow {
  title: string
  icon: string
}

/** Полное состояние приложения. */
export interface AppState {
  engine: EngineSnapshot
  signs: SignRow[]
  screens: ScreenRow[]
  mode: AppMode
  mode_title: string
  has_more_screens: boolean
}

/** Ответ на один кадр: состояние плюс события. */
export interface BridgeResult {
  state: AppState
  event: {
    speak: string | null
    commands: Command[]
  }
  rejected: RejectedHand[]
  stale: boolean
}

/** Кисть, отброшенная из-за неверного числа точек. */
export interface RejectedHand {
  hand: number
  received: number
  expected: number
}

/** Счётчики потока кадров. */
export interface Metrics {
  frames_ingested: number
  frames_stale: number
  hands_rejected: number
}

/** Ответ `get_state`. */
export interface GetState {
  state: AppState
  metrics: Metrics
}

/** Кадр с камеры: метка времени в секундах и кисти по 21 точке. */
export interface FrameInput {
  time: number
  hands: PointSer[][]
}

/** Число точек, которые ждёт ядро у одной кисти. */
export const JOINTS = 21
