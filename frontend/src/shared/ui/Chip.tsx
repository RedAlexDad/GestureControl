import type { ReactNode } from 'react'

interface ChipProps {
  /** Включённое состояние: подсвечивает кнопку. */
  on?: boolean
  disabled?: boolean
  onClick?: (() => void) | undefined
  children: ReactNode
}

/** Кнопка-«чип»: единый вид для переключателей режима и флагов. */
export function Chip({ on = false, disabled = false, onClick, children }: ChipProps) {
  return (
    <button
      type="button"
      className={on ? 'chip chip--on' : 'chip'}
      disabled={disabled}
      onClick={onClick}
    >
      {children}
    </button>
  )
}
