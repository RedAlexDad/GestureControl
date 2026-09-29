/** Подписи состояния сессии для показа. */

import type { Gesture, RecordingState, Sign } from '@/shared/api/types'

/** Русские названия встроенных жестов. */
const GESTURE_TITLES: Record<Gesture, string> = {
  Idle: 'нет',
  ThumbsUp: 'Лайк',
  OpenPalm: 'Ладонь',
  Pointing: 'Указательный палец',
  Fist: 'Кулак',
  Victory: 'Victory',
  Ok: 'Ок',
  CallMe: 'Позвони мне',
  SwipeRight: 'Свайп вправо',
  SwipeLeft: 'Свайп влево',
  SwipeUp: 'Свайп вверх',
  SwipeDown: 'Свайп вниз',
}

/** Подпись текущего жеста. */
export function signLabel(sign: Sign): string {
  if (sign === 'None') return 'нет'
  if ('BuiltIn' in sign) return GESTURE_TITLES[sign.BuiltIn]
  return `свой: ${sign.Custom.word}`
}

/** Текст состояния записи с обратным отсчётом. */
export function recordingLabel(recording: RecordingState): string | null {
  if (recording.kind === 'Idle') return null
  if (recording.kind === 'Countdown') return `отсчёт ${recording.value}`
  return 'идёт запись'
}
