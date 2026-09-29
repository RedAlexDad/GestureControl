import { Chip } from '@/shared/ui'

interface FlagTogglesProps {
  recognitionEnabled: boolean
  speechEnabled: boolean
  onRecognition: (enabled: boolean) => void
  onSpeech: (enabled: boolean) => void
  onStopSpeech: () => void
}

/** Флаги распознавания и озвучки плюс остановка речи. */
export function FlagToggles({
  recognitionEnabled,
  speechEnabled,
  onRecognition,
  onSpeech,
  onStopSpeech,
}: FlagTogglesProps) {
  return (
    <div className="bar__flags">
      <Chip on={recognitionEnabled} onClick={() => onRecognition(!recognitionEnabled)}>
        {recognitionEnabled ? 'Распознавание вкл' : 'Распознавание выкл'}
      </Chip>
      <Chip on={speechEnabled} onClick={() => onSpeech(!speechEnabled)}>
        {speechEnabled ? 'Озвучка вкл' : 'Озвучка выкл'}
      </Chip>
      <Chip onClick={onStopSpeech}>Стоп речь</Chip>
    </div>
  )
}
