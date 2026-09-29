import { useState } from 'react'

import type { RecordingState } from '@/shared/api/types'
import { Chip } from '@/shared/ui'

interface RecordSignProps {
  busy: boolean
  recording: RecordingState
  onStart: (word: string, isDynamic: boolean) => void
  onStop: () => void
  onCancel: () => void
  onFinishPhrase: () => void
}

/** Ввод слова и управление записью нового жеста. */
export function RecordSign({
  busy,
  recording,
  onStart,
  onStop,
  onCancel,
  onFinishPhrase,
}: RecordSignProps) {
  const [word, setWord] = useState('')
  const [isDynamic, setIsDynamic] = useState(false)
  const active = recording.kind !== 'Idle'
  const status = recording.kind === 'Countdown' ? `отсчёт ${recording.value}` : 'идёт запись'

  return (
    <div className="row">
      {active ? (
        <>
          <span className="muted">{status}</span>
          <Chip disabled={busy} onClick={onStop}>
            Остановить и сохранить
          </Chip>
          <Chip disabled={busy} onClick={onCancel}>
            Отменить запись
          </Chip>
        </>
      ) : (
        <>
          <input
            className="field"
            value={word}
            placeholder="слово для нового жеста"
            onChange={(event) => setWord(event.target.value)}
          />
          <label className="toggle">
            <input
              type="checkbox"
              checked={isDynamic}
              onChange={(event) => setIsDynamic(event.target.checked)}
            />
            жест по движению
          </label>
          <Chip
            disabled={busy || word.trim() === ''}
            onClick={() => {
              onStart(word, isDynamic)
              setWord('')
            }}
          >
            Записать
          </Chip>
          <Chip disabled={busy} onClick={onFinishPhrase}>
            Озвучить фразу
          </Chip>
        </>
      )}
    </div>
  )
}
