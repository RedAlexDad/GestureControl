//! Движок распознавания, фраз и команд.

mod recording;
mod runtime;

use std::collections::VecDeque;

use crate::geometry::Point;
use crate::library::{CustomSign, SignLibrary, SignStore};
use crate::models::{AppMode, Command, RecordingState, Sign};
use crate::recognizer::SignRecognizer;

use super::constants::NOTICE_DURATION;
use super::speaker::{SilentSpeaker, Speaker};

/// Данные ещё не завершённой записи.
///
/// Поза и динамика копятся раздельно: статичный жест усредняется в один
/// вектор, а для динамичного важна вся последовательность кадров.
struct PendingRecording {
    word: String,
    is_dynamic: bool,
    /// Векторы позы по кадрам.
    poses: Vec<Vec<f32>>,
    /// Кадры со смещением — только динамика.
    motion: Vec<Vec<f32>>,
}

/// Движок распознавания, фраз и команд.
pub struct GestureEngine {
    mode: AppMode,
    library: SignLibrary,
    recognizer: SignRecognizer,
    speaker: Box<dyn Speaker>,
    store: Option<Box<dyn SignStore>>,

    recognition_enabled: bool,
    speech_enabled: bool,

    /// Жест для отображения: событие либо удерживаемый автоматом.
    current: Sign,
    /// Слова текущей фразы.
    streaming: Vec<String>,
    /// Момент последнего подтверждённого жеста.
    last_gesture: f32,
    /// Сколько слов уже озвучено.
    speakable: usize,

    recording: RecordingState,
    pending: Option<PendingRecording>,
    countdown_until: f32,
    record_started: f32,

    /// Скользящее окно кадров для распознавания динамики.
    motion_frames: VecDeque<Vec<f32>>,
    /// Центр ведущей кисти в прошлом кадре: от него считается смещение.
    last_center: Option<Point>,

    notice: Option<String>,
    notice_until: f32,
    next_spoke_at: f32,
    next_command_at: f32,
    command_queue: VecDeque<Command>,

    screen_index: usize,
    parameter: f32,
}

impl std::fmt::Debug for GestureEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GestureEngine")
            .field("mode", &self.mode)
            .field("signs", &self.library.len())
            .field("recognition_enabled", &self.recognition_enabled)
            .field("speech_enabled", &self.speech_enabled)
            .field("recording", &self.recording)
            .field("sign", &self.current)
            .field("phrase", &self.streaming.join(" "))
            .field("screen_index", &self.screen_index)
            .field("parameter", &self.parameter)
            .finish_non_exhaustive()
    }
}

impl GestureEngine {
    /// Создаёт движок с пустым словарём и без звука.
    pub fn new() -> Self {
        Self::with_parts(
            SignLibrary::from_signs(Vec::new()),
            Box::new(SilentSpeaker),
            None,
        )
    }

    /// Создаёт движок с заданным словарём, озвучкой и хранилищем.
    pub fn with_parts(
        library: SignLibrary,
        speaker: Box<dyn Speaker>,
        store: Option<Box<dyn SignStore>>,
    ) -> Self {
        GestureEngine {
            mode: AppMode::Control,
            library,
            recognizer: SignRecognizer::new(Sign::default()),
            speaker,
            store,
            recognition_enabled: true,
            speech_enabled: true,
            current: Sign::None,
            streaming: Vec::new(),
            last_gesture: f32::NEG_INFINITY,
            speakable: 0,
            recording: RecordingState::Idle,
            pending: None,
            countdown_until: 0.0,
            record_started: 0.0,
            motion_frames: VecDeque::new(),
            last_center: None,
            notice: None,
            notice_until: 0.0,
            next_spoke_at: 0.0,
            next_command_at: 0.0,
            command_queue: VecDeque::new(),
            screen_index: 0,
            parameter: 1.0,
        }
    }

    /// Загружает движок из хранилища.
    pub fn load(store: Box<dyn SignStore>, speaker: Box<dyn Speaker>) -> Self {
        let library = SignLibrary::load(store.as_ref());
        Self::with_parts(library, speaker, Some(store))
    }

    // ---------------------------------------------------------------- режимы

    pub fn mode(&self) -> AppMode {
        self.mode
    }

    /// Переключает режим и сбрасывает состояние распознавания.
    ///
    /// Сброс обязателен: поза, набранная в одном режиме, не должна тут же
    /// сработать как жест другого режима.
    pub fn set_mode(&mut self, mode: AppMode) {
        if self.mode == mode {
            return;
        }
        self.mode = mode;
        self.recognizer.reset();
        self.current = Sign::None;
        self.streaming.clear();
        self.speakable = 0;
        self.motion_frames.clear();
        self.last_center = None;
        self.cancel_recording();
        self.notice = Some(format!("Режим: {}", mode.title()));
        self.notice_until = f32::INFINITY;
    }

    /// Включает или выключает распознавание.
    pub fn set_recognition_enabled(&mut self, enabled: bool) {
        if self.recognition_enabled == enabled {
            return;
        }
        self.recognition_enabled = enabled;
        self.recognizer.reset();
        self.current = Sign::None;
        self.streaming.clear();
        self.speakable = 0;
        self.motion_frames.clear();
        self.last_center = None;
        self.notice = Some(if enabled {
            "Распознавание включено".to_string()
        } else {
            "Распознавание приостановлено".to_string()
        });
        self.notice_until = f32::INFINITY;
    }

    pub fn recognition_enabled(&self) -> bool {
        self.recognition_enabled
    }

    /// Включает или выключает озвучку.
    pub fn set_speech_enabled(&mut self, enabled: bool) {
        self.speech_enabled = enabled;
        if !enabled {
            self.speaker.stop();
        }
    }

    pub fn speech_enabled(&self) -> bool {
        self.speech_enabled
    }

    // -------------------------------------------------------------- словарь

    pub fn library(&self) -> &SignLibrary {
        &self.library
    }

    pub fn signs(&self) -> Vec<CustomSign> {
        self.library.snapshot()
    }

    /// Удаляет жест по индексу и сохраняет словарь.
    pub fn delete_sign(&mut self, index: usize) {
        if let Some(removed) = self.library.delete(index) {
            self.notice = Some(format!("Жест «{}» удалён", removed.word));
            self.notice_until = f32::INFINITY;
            self.persist();
        }
    }

    /// Удаляет жест по идентификатору.
    pub fn delete_sign_id(&mut self, id: &str) {
        if let Ok(uuid) = id.parse() {
            if let Some(removed) = self.library.delete_id(uuid) {
                self.notice = Some(format!("Жест «{}» удалён", removed.word));
                self.notice_until = f32::INFINITY;
                self.persist();
            }
        }
    }

    /// Фраза, готовая к показу жестом: верхний регистр и разделители.
    pub fn current_phrase(&self) -> String {
        self.streaming.join(" ").to_uppercase()
    }

    fn persist(&self) {
        if let Some(store) = &self.store {
            // Ошибка записи не должна ломать сеанс: жест уже в памяти.
            let _ = self.library.save(store.as_ref());
        }
    }

    fn notice_soon(&mut self, text: impl Into<String>, time: f32) {
        self.notice = Some(text.into());
        self.notice_until = time + NOTICE_DURATION;
    }
}

impl Default for GestureEngine {
    fn default() -> Self {
        Self::new()
    }
}
