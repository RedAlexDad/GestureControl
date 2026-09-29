import type { Camera } from '@/entities/camera'
import { CameraControl } from '@/features/control-camera'
import type { AppState } from '@/shared/api/types'
import { Notice } from '@/shared/ui'

import { HandOverlay } from './HandOverlay'

interface CameraPanelProps {
  state: AppState
  camera: Camera
}

/** Панель кадра: превью со скелетом и управление камерой. */
export function CameraPanel({ state, camera }: CameraPanelProps) {
  return (
    <div className="panel">
      <h2>Кадр</h2>
      <div className="preview">
        <canvas ref={camera.canvasRef} className="preview__canvas" width={640} height={480} />
        <HandOverlay
          state={state}
          width={camera.settings?.preview.width ?? 640}
          height={camera.settings?.preview.height ?? 480}
        />
        {!camera.status.running && <span className="preview__hint">камера выключена</span>}
      </div>

      <CameraControl
        running={camera.status.running}
        busy={camera.busy}
        device={camera.status.device}
        frames={camera.status.frames}
        onToggle={camera.toggle}
      />

      {camera.settings && !camera.status.running && (
        <p className="muted">
          По настройкам окна: {camera.settings.camera.device} · {camera.settings.camera.width}x
          {camera.settings.camera.height} @ {camera.settings.camera.fps} fps
        </p>
      )}

      {camera.notice && <Notice onClose={camera.dismiss}>{camera.notice}</Notice>}

      <p className="muted">
        Кистей в кадре: {state.engine.hands_visible}. Точки приходят с камеры через детектор,
        скелет рисуется поверх кадра. Распознавание работает, когда кисть видна целиком.
        Ладонь — это жест паузы, поэтому после неё распознавание выключается.
      </p>
    </div>
  )
}
