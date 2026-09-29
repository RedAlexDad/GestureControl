import type { SignRow } from '@/shared/api/types'
import { Chip } from '@/shared/ui'

interface SignTableProps {
  signs: SignRow[]
  busy: boolean
  onDelete: (id: string) => void
  onClear: () => void
}

/** Список записанных жестов с удалением и полной очисткой. */
export function SignTable({ signs, busy, onDelete, onClear }: SignTableProps) {
  return (
    <>
      <div className="row">
        <Chip disabled={busy || signs.length === 0} onClick={onClear}>
          Очистить словарь
        </Chip>
      </div>

      {signs.length === 0 ? (
        <p className="muted">В словаре пока нет своих жестов.</p>
      ) : (
        <table className="table">
          <thead>
            <tr>
              <th>Подпись</th>
              <th>Слово</th>
              <th>Кистей</th>
              <th>Вид</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {signs.map((sign) => (
              <tr key={sign.id}>
                <td>{sign.title}</td>
                <td>{sign.word}</td>
                <td>{sign.hand_count}</td>
                <td>{sign.is_dynamic ? 'движение' : 'поза'}</td>
                <td>
                  <button
                    type="button"
                    className="danger"
                    disabled={busy}
                    onClick={() => onDelete(sign.id)}
                  >
                    удалить
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </>
  )
}
