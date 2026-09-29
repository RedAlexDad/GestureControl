import type { AppMode } from '@/shared/api/types'
import { FlagToggles } from '@/features/toggle-flags'
import { ModeSwitch } from '@/features/switch-mode'

interface TopBarProps {
  mode: AppMode
  recognitionEnabled: boolean
  speechEnabled: boolean
  onMode: (mode: AppMode) => void
  onRecognition: (enabled: boolean) => void
  onSpeech: (enabled: boolean) => void
  onStopSpeech: () => void
}

/** Верхняя панель: режим и флаги распознавания. */
export function TopBar({
  mode,
  recognitionEnabled,
  speechEnabled,
  onMode,
  onRecognition,
  onSpeech,
  onStopSpeech,
}: TopBarProps) {
  return (
    <header className="bar">
      <ModeSwitch mode={mode} onChange={onMode} />
      <FlagToggles
        recognitionEnabled={recognitionEnabled}
        speechEnabled={speechEnabled}
        onRecognition={onRecognition}
        onSpeech={onSpeech}
        onStopSpeech={onStopSpeech}
      />
    </header>
  )
}
