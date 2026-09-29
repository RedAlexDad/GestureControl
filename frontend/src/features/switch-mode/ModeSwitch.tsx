import type { AppMode } from '@/shared/api/types'
import { Chip } from '@/shared/ui'

const MODES: AppMode[] = ['Control', 'Translate']

const TITLES: Record<AppMode, string> = {
  Control: 'Управление',
  Translate: 'Перевод',
}

interface ModeSwitchProps {
  mode: AppMode
  onChange: (mode: AppMode) => void
}

/** Переключение режима: управление или сурдоперевод. */
export function ModeSwitch({ mode, onChange }: ModeSwitchProps) {
  return (
    <div className="bar__modes">
      {MODES.map((item) => (
        <Chip key={item} on={mode === item} onClick={() => onChange(item)}>
          {TITLES[item]}
        </Chip>
      ))}
    </div>
  )
}
