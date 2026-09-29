import type { ScreenRow } from '@/shared/api/types'
import { Chip } from '@/shared/ui'

interface ScreenSelectProps {
  screens: ScreenRow[]
  index: number
  onTap: (index: number) => void
}

/** Переключение демонстрационного экрана нажатием. */
export function ScreenSelect({ screens, index, onTap }: ScreenSelectProps) {
  return (
    <div className="row">
      {screens.map((screen, order) => (
        <Chip key={screen.title} on={order === index} onClick={() => onTap(order)}>
          {screen.icon} {screen.title}
        </Chip>
      ))}
    </div>
  )
}
