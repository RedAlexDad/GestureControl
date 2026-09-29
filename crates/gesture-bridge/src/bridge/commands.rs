//! Команды интерфейса: режим, флаги, тап, завершение фразы.

use gesture_core::pipeline::EngineEvent;
use gesture_core::AppMode;

use crate::state::AppState;

use super::{BridgeResult, GestureBridge};

impl GestureBridge {
    /// Переключает режим работы.
    pub fn set_mode(&mut self, mode: AppMode) -> AppState {
        self.engine.set_mode(mode);
        self.state()
    }

    /// Включает или выключает распознавание.
    pub fn set_recognition(&mut self, enabled: bool) -> AppState {
        self.engine.set_recognition_enabled(enabled);
        self.state()
    }

    /// Переключает распознавание.
    pub fn toggle_recognition(&mut self) -> AppState {
        let enabled = !self.engine.recognition_enabled();
        self.set_recognition(enabled)
    }

    /// Включает или выключает озвучку.
    pub fn set_speech(&mut self, enabled: bool) -> AppState {
        self.engine.set_speech_enabled(enabled);
        self.state()
    }

    /// Переключает озвучку.
    pub fn toggle_speech(&mut self) -> AppState {
        let enabled = !self.engine.speech_enabled();
        self.set_speech(enabled)
    }

    /// Опрашивает элемент интерфейса: экран, поле, кнопку.
    ///
    /// Номер — позиция элемента в текущем экране, а не идентификатор:
    /// список перерисовывается каждым кадром, и индекс короче для IPC.
    pub fn tap(&mut self, index: usize) -> BridgeResult {
        let time = self.frame_time();
        let mut event = EngineEvent::default();
        self.engine.tap_index(index, time, &mut event);
        BridgeResult {
            state: self.state(),
            event,
            rejected: Vec::new(),
            stale: false,
        }
    }

    /// Завершает накопленную фразу жестом.
    pub fn finish_phrase(&mut self) -> BridgeResult {
        let time = self.frame_time();
        let mut event = EngineEvent::default();
        self.engine.finish_phrase(time, &mut event);
        BridgeResult {
            state: self.state(),
            event,
            rejected: Vec::new(),
            stale: false,
        }
    }

    /// Останавливает текущую озвучку.
    pub fn stop_speech(&mut self) -> AppState {
        self.engine.stop_speech();
        self.state()
    }
}
