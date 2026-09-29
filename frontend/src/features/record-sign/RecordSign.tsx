import { useState } from 'react'

import { Chip } from '@/shared/ui'

interface RecordSignProps {
  busy: boolean
  onStart: (word: string, isDynamic: boolean) => void
  onCancel: () => void
  onFinishPhrase: () => void
}

/** Ввод слова и управление записью нового жеста. */
export function RecordSign({ busy, onStart, onCancel, onFinishPhrase }: RecordSignProps) {
  const [word, setWord] = useState('')
  const [isDynamic, setIsDynamic] = useState(false)

  return (
    <div className="row">
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
      <Chip disabled={busy} onClick={onCancel}>
        Отменить запись
      </Chip>
      <Chip disabled={busy} onClick={onFinishPhrase}>
        Озвучить фразу
      </Chip>
    </div>
  )
}
