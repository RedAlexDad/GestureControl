import { useMemo, useState } from 'react'

import { BONES, isVisible, TIPS } from './skeleton'
import { useBridge } from './useBridge'
import { useCamera } from './useCamera'
import type { AppState, RecordingState, Sign } from './types'

/** Подпись текущего жеста для показа. */
function signLabel(sign: Sign): string {
  if (sign === 'None') return 'нет'
  if ('BuiltIn' in sign) return sign.BuiltIn
  return `свой: ${sign.Custom.word}`
}

/** Текст состояния записи с обратным отсчётом. */
function recordingLabel(recording: RecordingState): string | null {
  if (recording.kind === 'Idle') return null
  if (recording.kind === 'Countdown') return `отсчёт ${recording.value}`
  return `запись ${Math.round(recording.value * 100)}%`
}

/** Скелет кистей поверх кадра: пропущенные точки не рисуем. */
function Hands({ state, width, height }: { state: AppState; width: number; height: number }) {
  const paths = useMemo(
    () =>
      state.engine.hands.map((hand, index) => ({
        index,
        bones: BONES.flatMap(([from, to], order) => {
          const a = hand[from]
          const b = hand[to]
          if (!isVisible(a) || !isVisible(b)) return []
          return [{ key: `${from}-${to}-${order}`, from: a, to: b }]
        }),
        tips: TIPS.flatMap((tip) => {
          const point = hand[tip]
          return isVisible(point) ? [{ key: tip, point }] : []
        }),
        points: hand.flatMap((point, order) =>
          isVisible(point) ? [{ key: order, point }] : [],
        ),
      })),
    [state.engine.hands],
  )

  return (
    <svg
      className="preview__skeleton"
      viewBox={`0 0 ${width} ${height}`}
      preserveAspectRatio="none"
      role="img"
      aria-label="Скелет кисти поверх кадра"
    >
      {paths.map(({ index, bones, tips, points }) => (
        <g key={index} className="skeleton">
          {bones.map(({ key, from, to }) => (
            <line key={key} x1={from.x} y1={from.y} x2={to.x} y2={to.y} />
          ))}
          {points.map(({ key, point }) => (
            <circle key={key} cx={point.x} cy={point.y} r="4" />
          ))}
          {tips.map(({ key, point }) => (
            <circle key={`tip-${key}`} cx={point.x} cy={point.y} r="6" />
          ))}
        </g>
      ))}
    </svg>
  )
}

export default function App() {
  const bridge = useBridge()
  const camera = useCamera()
  const [word, setWord] = useState('')
  const [isDynamic, setIsDynamic] = useState(false)

  const state = bridge.state
  const recording = state ? recordingLabel(state.engine.recording) : null

  if (!state) {
    return (
      <main className="app app--waiting">
        <p>{bridge.notice ?? 'загружаю состояние окна…'}</p>
      </main>
    )
  }

  return (
    <main className="app">
      <header className="bar">
        <div className="bar__modes">
          {(['Control', 'Translate'] as const).map((mode) => (
            <button
              key={mode}
              type="button"
              className={state.mode === mode ? 'chip chip--on' : 'chip'}
              onClick={() => void bridge.setMode(mode)}
            >
              {mode === 'Control' ? 'Управление' : 'Перевод'}
            </button>
          ))}
        </div>

        <div className="bar__flags">
          <button
            type="button"
            className={state.engine.recognition_enabled ? 'chip chip--on' : 'chip'}
            onClick={() => void bridge.setRecognition(!state.engine.recognition_enabled)}
          >
            {state.engine.recognition_enabled ? 'Распознавание вкл' : 'Распознавание выкл'}
          </button>
          <button
            type="button"
            className={state.engine.speech_enabled ? 'chip chip--on' : 'chip'}
            onClick={() => void bridge.setSpeech(!state.engine.speech_enabled)}
          >
            {state.engine.speech_enabled ? 'Озвучка вкл' : 'Озвучка выкл'}
          </button>
          <button type="button" className="chip" onClick={() => void bridge.stopSpeech()}>
            Стоп речь
          </button>
        </div>
      </header>

      {bridge.notice && (
        <p className="notice" role="status">
          {bridge.notice}
          <button type="button" className="notice__close" onClick={bridge.dismiss}>
            скрыть
          </button>
        </p>
      )}

      <section className="grid">
        <div className="panel">
          <h2>Кадр</h2>
          <div className="preview">
            <canvas ref={camera.canvasRef} className="preview__canvas" width={640} height={480} />
            <Hands
              state={state}
              width={camera.settings?.preview.width ?? 640}
              height={camera.settings?.preview.height ?? 480}
            />
            {!camera.status.running && <span className="preview__hint">камера выключена</span>}
          </div>
          <div className="row">
            <button
              type="button"
              className={camera.status.running ? 'chip chip--on' : 'chip'}
              disabled={camera.busy}
              onClick={camera.toggle}
            >
              {camera.status.running ? 'Камера вкл' : 'Камера выкл'}
            </button>
            {camera.status.running && (
              <span className="muted">
                {camera.status.device} · кадров {camera.status.frames}
              </span>
            )}
          </div>
          {camera.settings && !camera.status.running && (
            <p className="muted">
              По настройкам окна: {camera.settings.camera.device} · {camera.settings.camera.width}x
              {camera.settings.camera.height} @ {camera.settings.camera.fps} fps
            </p>
          )}
          {camera.notice && (
            <p className="notice" role="status">
              {camera.notice}
              <button type="button" className="notice__close" onClick={camera.dismiss}>
                скрыть
              </button>
            </p>
          )}
          <p className="muted">
            Кистей в кадре: {state.engine.hands_visible}. Точки приходят с камеры через
            детектор, скелет рисуется поверх кадра. Распознавание работает, когда кисть видна
            целиком. Ладонь — это жест паузы, поэтому после неё распознавание выключается.
          </p>
        </div>

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

          <h3>Экраны</h3>
          <div className="row">
            {state.screens.map((screen, index) => (
              <button
                key={screen.title}
                type="button"
                className={index === state.engine.screen_index ? 'chip chip--on' : 'chip'}
                onClick={() => void bridge.tap(index)}
              >
                {screen.icon} {screen.title}
              </button>
            ))}
          </div>

          <h3>Счётчики кадров</h3>
          <p className="muted">
            принято {bridge.metrics.frames_ingested} · устарело {bridge.metrics.frames_stale} ·
            отброшено {bridge.metrics.hands_rejected}
          </p>
        </div>
      </section>

      <section className="panel">
        <h2>Словарь жестов</h2>
        <div className="row">
          <input
            className="field"
            value={word}
            placeholder="слово для нового жеста"
            onChange={(event) => setWord(event.target.value)}
          />
          <label className="toggle">
            <input
              type="checkbox"
              checked={isDynamic}
              onChange={(event) => setIsDynamic(event.target.checked)}
            />
            жест по движению
          </label>
          <button
            type="button"
            disabled={bridge.busy || word.trim() === ''}
            onClick={() => {
              void bridge.startRecording(word, isDynamic)
              setWord('')
            }}
          >
            Записать
          </button>
          <button type="button" disabled={bridge.busy} onClick={() => void bridge.cancelRecording()}>
            Отменить запись
          </button>
          <button type="button" disabled={bridge.busy} onClick={() => void bridge.finishPhrase()}>
            Озвучить фразу
          </button>
          <button
            type="button"
            className="danger"
            disabled={bridge.busy || state.signs.length === 0}
            onClick={() => void bridge.clearSigns()}
          >
            Очистить словарь
          </button>
        </div>

        {state.signs.length === 0 ? (
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
              {state.signs.map((sign) => (
                <tr key={sign.id}>
                  <td>{sign.title}</td>
                  <td>{sign.word}</td>
                  <td>{sign.hand_count}</td>
                  <td>{sign.is_dynamic ? 'движение' : 'поза'}</td>
                  <td>
                    <button
                      type="button"
                      className="danger"
                      disabled={bridge.busy}
                      onClick={() => void bridge.deleteSign(sign.id)}
                    >
                      удалить
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>
    </main>
  )
}
