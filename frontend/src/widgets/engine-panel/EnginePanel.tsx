import { recordingLabel, signLabel } from '@/entities/session'
import type { AppState, Metrics } from '@/shared/api/types'

interface EnginePanelProps {
  state: AppState
  metrics: Metrics
}

/** Панель состояния: факты о жесте и счётчики кадров. */
export function EnginePanel({ state, metrics }: EnginePanelProps) {
  const recording = recordingLabel(state.engine.recording)

  return (
    <div className="panel">
      <h2>Состояние</h2>
      <dl className="facts">
        <dt>Режим</dt>
        <dd>{state.mode_title}</dd>
        <dt>Жест</dt>
        <dd>{signLabel(state.engine.sign)}</dd>
        <dt>Фраза</dt>
        <dd>{state.engine.phrase || '—'}</dd>
        <dt>Слово</dt>
        <dd>{state.engine.streaming_word ?? '—'}</dd>
        <dt>Запись</dt>
        <dd>{recording ?? '—'}</dd>
        <dt>Параметр</dt>
        <dd>{state.engine.parameter.toFixed(2)}</dd>
      </dl>

      <h3>Счётчики кадров</h3>
      <p className="muted">
        принято {metrics.frames_ingested} · устарело {metrics.frames_stale} · отброшено{' '}
        {metrics.hands_rejected}
      </p>
    </div>
  )
}
