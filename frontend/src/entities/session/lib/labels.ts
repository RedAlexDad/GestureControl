/** Подписи состояния сессии для показа. */

import type { RecordingState, Sign } from '@/shared/api/types'

/** Подпись текущего жеста. */
export function signLabel(sign: Sign): string {
  if (sign === 'None') return 'нет'
  if ('BuiltIn' in sign) return sign.BuiltIn
  return `свой: ${sign.Custom.word}`
}

/** Текст состояния записи с обратным отсчётом. */
export function recordingLabel(recording: RecordingState): string | null {
  if (recording.kind === 'Idle') return null
  if (recording.kind === 'Countdown') return `отсчёт ${recording.value}`
  return `запись ${Math.round(recording.value * 100)}%`
}
