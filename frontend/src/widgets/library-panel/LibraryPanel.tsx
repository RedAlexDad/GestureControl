import { SignTable } from '@/features/manage-signs'
import { RecordSign } from '@/features/record-sign'
import type { SignRow } from '@/shared/api/types'

interface LibraryPanelProps {
  signs: SignRow[]
  busy: boolean
  onStart: (word: string, isDynamic: boolean) => void
  onCancel: () => void
  onFinishPhrase: () => void
  onDelete: (id: string) => void
  onClear: () => void
}

/** Панель словаря: запись нового жеста и список своих жестов. */
export function LibraryPanel({
  signs,
  busy,
  onStart,
  onCancel,
  onFinishPhrase,
  onDelete,
  onClear,
}: LibraryPanelProps) {
  return (
    <section className="panel">
      <h2>Словарь жестов</h2>
      <RecordSign busy={busy} onStart={onStart} onCancel={onCancel} onFinishPhrase={onFinishPhrase} />
      <SignTable signs={signs} busy={busy} onDelete={onDelete} onClear={onClear} />
    </section>
  )
}
