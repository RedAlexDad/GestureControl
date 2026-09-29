import { useMemo, useState } from 'react'

import { brokenHand, fist, frame, openPalm, partialHand } from './demoFrames'
import { useBridge } from './useBridge'
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

/** Скелет кистей на кадре: пропущенные точки не рисуем. */
function Hands({ state }: { state: AppState }) {
  const paths = useMemo(
    () =>
      state.engine.hands.map((hand, index) => ({
        index,
        points: hand.filter((point) => point.x >= 0),
      })),
    [state.engine.hands],
  )

  return (
    <svg className="stage" viewBox="0 0 640 480" role="img" aria-label="Кадр с камеры">
      <rect className="stage__back" width="640" height="480" />
      {paths.map(({ index, points }) => (
        <g key={index} className="skeleton">
          {points.map((point, order) => (
            <circle key={order} cx={point.x} cy={point.y} r="5" />
          ))}
        </g>
      ))}
    </svg>
  )
}

export default function App() {
  const bridge = useBridge()
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

  const send = (hands: { x: number; y: number }[][]) => {
    void bridge.pushFrame(frame(0, hands))
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
          <Hands state={state} />
          <p className="muted">
            Кистей в кадре: {state.engine.hands_visible}. Слой зрения на Rust ещё не выбран,
            поэтому точки ниже отправляются вручную.
          </p>
          <div className="row">
            <button type="button" onClick={() => send([openPalm(320, 320, 0)])}>
              Ладонь
            </button>
            <button type="button" onClick={() => send([fist(320, 320, 0)])}>
              Кулак
            </button>
            <button type="button" onClick={() => send([partialHand(320, 320, 0)])}>
              Часть точек
            </button>
            <button type="button" onClick={() => send([brokenHand()])}>
              Сломанная кисть
            </button>
            <button type="button" onClick={() => send([])}>
              Пустой кадр
            </button>
          </div>
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
