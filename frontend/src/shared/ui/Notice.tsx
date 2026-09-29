import type { ReactNode } from 'react'

interface NoticeProps {
  children: ReactNode
  /** Закрыть подсказку: без обработчика кнопка не показывается. */
  onClose?: (() => void) | undefined
}

/** Строка уведомления с необязательной кнопкой закрытия. */
export function Notice({ children, onClose }: NoticeProps) {
  return (
    <p className="notice" role="status">
      {children}
      {onClose && (
        <button type="button" className="notice__close" onClick={onClose}>
          скрыть
        </button>
      )}
    </p>
  )
}
