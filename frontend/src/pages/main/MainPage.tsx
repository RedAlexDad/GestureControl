import { useBridge } from '@/entities/session'
import { useCamera } from '@/entities/camera'
import { Notice } from '@/shared/ui'
import { CameraPanel } from '@/widgets/camera-panel'
import { EnginePanel } from '@/widgets/engine-panel'
import { LibraryPanel } from '@/widgets/library-panel'
import { Subtitles } from '@/widgets/subtitles'
import { TopBar } from '@/widgets/top-bar'

/** Главный экран: камера, состояние и словарь. */
export function MainPage() {
  const bridge = useBridge()
  const camera = useCamera()
  const state = bridge.state

  if (!state) {
    return (
      <main className="app app--waiting">
        <p>{bridge.notice ?? 'загружаю состояние окна…'}</p>
      </main>
    )
  }

  return (
    <main className="app">
      <TopBar
        mode={state.mode}
        recognitionEnabled={state.engine.recognition_enabled}
        speechEnabled={state.engine.speech_enabled}
        onMode={(mode) => void bridge.setMode(mode)}
        onRecognition={(enabled) => void bridge.setRecognition(enabled)}
        onSpeech={(enabled) => void bridge.setSpeech(enabled)}
        onStopSpeech={() => void bridge.stopSpeech()}
      />

      <Subtitles
        mode={state.mode}
        sign={state.engine.sign}
        phrase={state.engine.phrase}
        word={state.engine.streaming_word}
        recognitionEnabled={state.engine.recognition_enabled}
      />

      {bridge.notice && <Notice onClose={bridge.dismiss}>{bridge.notice}</Notice>}

      <section className="grid">
        <CameraPanel state={state} camera={camera} />
        <EnginePanel
          state={state}
          metrics={bridge.metrics}
          onTap={(index) => void bridge.tap(index)}
        />
      </section>

      <LibraryPanel
        signs={state.signs}
        busy={bridge.busy}
        recording={state.engine.recording}
        onStart={(word, isDynamic) => void bridge.startRecording(word, isDynamic)}
        onStop={() => void bridge.stopRecording()}
        onCancel={() => void bridge.cancelRecording()}
        onFinishPhrase={() => void bridge.finishPhrase()}
        onDelete={(id) => void bridge.deleteSign(id)}
        onClear={() => void bridge.clearSigns()}
      />
    </main>
  )
}
