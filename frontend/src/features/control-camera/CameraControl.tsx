import { Chip } from '@/shared/ui'

interface CameraControlProps {
  running: boolean
  busy: boolean
  device: string
  frames: number
  onToggle: () => void
}

/** Кнопка включения камеры и счётчик принятых кадров. */
export function CameraControl({ running, busy, device, frames, onToggle }: CameraControlProps) {
  return (
    <div className="row">
      <Chip on={running} disabled={busy} onClick={onToggle}>
        {running ? 'Камера вкл' : 'Камера выкл'}
      </Chip>
      {running && (
        <span className="muted">
          {device} · кадров {frames}
        </span>
      )}
    </div>
  )
}
