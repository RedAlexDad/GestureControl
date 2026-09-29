import { SignTable } from '@/features/manage-signs'
import { RecordSign } from '@/features/record-sign'
import type { RecordingState, SignRow } from '@/shared/api/types'

interface LibraryPanelProps {
  signs: SignRow[]
  busy: boolean
  recording: RecordingState
  onStart: (word: string, isDynamic: boolean) => void
  onStop: () => void
  onCancel: () => void
  onFinishPhrase: () => void
  onDelete: (id: string) => void
  onClear: () => void
}

/** Панель словаря: запись нового жеста и список своих жестов. */
export function LibraryPanel({
  signs,
  busy,
  recording,
  onStart,
  onStop,
  onCancel,
  onFinishPhrase,
  onDelete,
  onClear,
}: LibraryPanelProps) {
  return (
    <section className="panel">
      <h2>Словарь жестов</h2>
      <RecordSign
        busy={busy}
        recording={recording}
        onStart={onStart}
        onStop={onStop}
        onCancel={onCancel}
        onFinishPhrase={onFinishPhrase}
      />
      <SignTable signs={signs} busy={busy} onDelete={onDelete} onClear={onClear} />
    </section>
  )
}
