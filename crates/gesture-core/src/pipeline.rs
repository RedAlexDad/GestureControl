//! Движок приложения: режимы, запись жестов, перевод фраз и очередь команд.
//!
//! Перенос `GestureViewModel` из `GestureViewModel.swift`.
//!
//! Движок не знает ни о камере, ни об окне: на вход подаются точки кистей,
//! на выход — снимок состояния и список событий (озвучивание текста, команды
//! интерфейса). Так его можно полностью проверить тестами и переиспользовать
//! в консольном режиме.
//!
//! Оба режима работают через один автомат [`SignRecognizer`]: различие в том,
//! что считается результатом, задаёт замыкание-классификатор
//! [`classify`]. В режиме управления это встроенные жесты, в режиме перевода —
//! ближайший жест из словаря.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::features::HandFeatures;
use crate::geometry::{HandGeometry, Point};
use crate::library::{CustomSign, SignLibrary, SignStore};
use crate::models::{AppMode, Command, RecordingState, Sign, DEMO_SCREENS};
use crate::motion::MotionFeatures;
use crate::recognizer::{GestureResult, RecognizerMotion, SignRecognizer};

/// Порог близости для статичных жестов: среднее расстояние между
/// соответствующими точками в размерах ладони.
pub const STATIC_THRESHOLD: f32 = 0.30;

/// Порог расстояния DTW для динамических жестов.
pub const DYNAMIC_THRESHOLD: f32 = 0.70;

/// Как долго ждём подтверждения жеста, прежде чем отправить слово в фразу.
pub const STREAM_WINDOW: f32 = 0.9;

/// Длительность записи одного примера жеста.
pub const RECORD_DURATION: f32 = 1.5;

/// Длительность обратного отсчёта перед записью.
pub const RECORD_COUNTDOWN: f32 = 3.0;

/// Пауза перед началом записи: рука входит в кадр не мгновенно, и первые
/// кадры динамики не относятся к жесту.
pub const RECORD_LEAD_IN: f32 = 0.2;

/// Минимум кадров с движением, иначе запись считается пустой.
pub const MIN_MOTION_FRAMES: usize = 4;

/// Сколько секунд кадров уходит в буфер движения.
/// 2 секунды при 30 fps — достаточно, чтобы поймать и задержать жест.
pub const MOTION_BUFFER: f32 = 2.0;

/// Частота кадров, на которую рассчитан буфер движения.
pub const ASSUMED_FPS: f32 = 30.0;

/// Пауза между соседними словами в очереди озвучки.
pub const SPEAK_GAP: f32 = 0.25;

/// Пауза между соседними командами интерфейса.
pub const COMMAND_GAP: f32 = 0.4;

/// Сколько живёт уведомление.
pub const NOTICE_DURATION: f32 = 2.5;

/// Границы и шаг параметра, который меняют жесты увеличения/уменьшения.
pub const PARAMETER_MIN: f32 = 0.5;
pub const PARAMETER_MAX: f32 = 3.0;
pub const PARAMETER_STEP: f32 = 0.1;

/// Озвучивание текста. Позволяет не тянуть синтез речи в ядро.
///
/// Трейт требует `Send + Sync`: синтезатор живёт в общем состоянии окна и
/// должен выдерживать вызов и из потока кадров, и из потока интерфейса.
pub trait Speaker: Send + Sync {
    /// Произносит текст. Вызывается из потока кадров, поэтому
    /// реализация обязана возвращаться быстро.
    fn speak(&self, text: &str);

    /// Прерывает текущую речь.
    fn stop(&self);
}

/// Заглушка: приложение работает без звука.
#[derive(Debug, Default)]
pub struct SilentSpeaker;

impl Speaker for SilentSpeaker {
    fn speak(&self, _text: &str) {}
    fn stop(&self) {}
}

/// Событие, которое движок просит выполнить оболочке.
///
/// Сериализуется, потому что это полезная нагрузка IPC: оболочка
/// пересылает её в интерфейс как есть.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EngineEvent {
    /// Текст для озвучки.
    pub speak: Option<String>,
    /// Команды интерфейса в порядке выполнения.
    pub commands: Vec<Command>,
}

impl EngineEvent {
    pub fn is_empty(&self) -> bool {
        self.speak.is_none() && self.commands.is_empty()
    }
}

/// Снимок состояния движка: всё, что нужно нарисовать в интерфейсе.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineSnapshot {
    pub mode: AppMode,
    /// Включено ли распознавание (кнопка «Пауза»).
    pub recognition_enabled: bool,
    /// Включена ли озвучка.
    pub speech_enabled: bool,
    /// Текущий жест: событие либо удерживаемый.
    pub sign: Sign,
    /// Слово, добавленное в фразу последним.
    pub streaming_word: Option<String>,
    /// Накопленная фраза.
    pub phrase: String,
    pub recording: RecordingStateSer,
    /// Текст последнего уведомления.
    pub notice: Option<String>,
    /// Сколько слов в словаре.
    pub signs_count: usize,
    /// Индекс текущего экрана демо-интерфейса.
    pub screen_index: usize,
    /// Масштаб демо-интерфейса: производная от `parameter`.
    pub zoom: f32,
    /// Громкость: производная от `parameter`.
    pub volume: f32,
    /// Параметр, меняемый жестами увеличения/уменьшения.
    pub parameter: f32,
    /// Сколько кистей в кадре.
    pub hands_visible: usize,
    /// Координаты точек кистей для отрисовки скелета.
    pub hands: Vec<Vec<PointSer>>,
}

/// Состояние записи в сериализуемом виде.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum RecordingStateSer {
    Idle,
    Countdown(u8),
    Recording(f32),
}

impl From<RecordingState> for RecordingStateSer {
    fn from(value: RecordingState) -> Self {
        match value {
            RecordingState::Idle => RecordingStateSer::Idle,
            RecordingState::Countdown(n) => RecordingStateSer::Countdown(n),
            RecordingState::Recording(p) => RecordingStateSer::Recording(p),
        }
    }
}

/// Точка в сериализуемом виде.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PointSer {
    pub x: f32,
    pub y: f32,
}

impl From<Point> for PointSer {
    fn from(p: Point) -> Self {
        PointSer { x: p.x, y: p.y }
    }
}

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

    // ----------------------------------------------------------------- запись

    /// Начинает запись примера жеста для слова.
    ///
    /// Отказ регистрируется как уведомление, а не ошибка: интерфейс показывает
    /// его в поле подсказки.
    pub fn begin_recording(&mut self, word: &str, is_dynamic: bool, time: f32) {
        if word.trim().is_empty() {
            self.notice = Some("Введите слово для жеста".to_string());
            self.notice_until = f32::INFINITY;
            return;
        }
        self.recognizer.reset();
        self.current = Sign::None;
        self.pending = Some(PendingRecording {
            word: word.trim().to_string(),
            is_dynamic,
            poses: Vec::new(),
            motion: Vec::new(),
        });
        self.countdown_until = time + RECORD_COUNTDOWN;
        self.recording = RecordingState::Countdown(RECORD_COUNTDOWN as u8);
    }

    /// Отменяет запись.
    pub fn cancel_recording(&mut self) {
        self.recording = RecordingState::Idle;
        self.pending = None;
    }

    /// Состояние записи.
    pub fn recording(&self) -> RecordingState {
        self.recording
    }

    /// Длительность записи для интерфейса.
    pub fn record_duration(&self) -> f32 {
        RECORD_DURATION
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

    /// Завершает запись и добавляет жест в словарь.
    fn finish_recording(&mut self, time: f32) {
        let Some(pending) = self.pending.take() else {
            self.recording = RecordingState::Idle;
            return;
        };
        self.recording = RecordingState::Idle;

        if pending.is_dynamic {
            if pending.motion.len() < MIN_MOTION_FRAMES {
                self.notice_soon("Жест не распознан: нет движения", time);
                return;
            }
            let word = pending.word.clone();
            self.library.add_sequence(&word, pending.motion);
            self.persist();
            self.notice_soon(format!("Жест «{}» сохранён", word), time);
            return;
        }

        let Some(poses) = average_frames(&pending.poses) else {
            self.notice_soon("Рука не найдена", time);
            return;
        };
        let word = pending.word.clone();
        self.library.add(&word, poses);
        self.persist();
        self.notice_soon(format!("Жест «{}» сохранён", word), time);
    }

    // ------------------------------------------------------------ обработка

    /// Обрабатывает очередной кадр.
    ///
    /// * `time` — монотонное время в секундах.
    /// * `hands` — геометрия кистей текущего кадра.
    pub fn process_frame(
        &mut self,
        time: f32,
        hands: &[HandGeometry],
    ) -> (EngineSnapshot, EngineEvent) {
        self.push_motion(hands);

        // Пока идёт запись, распознавание не выполняется: иначе жест,
        // который пользователь записывает, тут же сработал бы как команда.
        if self.advance_recording(time, hands) {
            return (self.snapshot(hands), EngineEvent::default());
        }

        if time >= self.notice_until {
            self.notice = None;
        }

        let mut event = EngineEvent::default();

        if self.recognition_enabled {
            let result = self.recognize(time, hands);
            self.current = result.display.unwrap_or(Sign::None);
            if let Some(sign) = result.event {
                match self.mode {
                    AppMode::Control => self.queue_command(&sign, time),
                    AppMode::Translate => self.push_stream(&sign, time),
                }
            }
        } else {
            // При выключенном распознавании движение не копится: иначе
            // жест, показанный во время паузы, сработал бы сразу после неё.
            self.motion_frames.clear();
            self.last_center = None;
        }

        self.pump_commands(time, &mut event);
        self.advance_speech(time, &mut event);

        (self.snapshot(hands), event)
    }

    /// Обрабатывает кадр в координатах детектора: битые кисти отбрасываются.
    pub fn process_points(
        &mut self,
        time: f32,
        hands: &[Vec<Point>],
    ) -> (EngineSnapshot, EngineEvent) {
        let geometries: Vec<HandGeometry> =
            hands.iter().filter_map(|p| HandGeometry::new(p)).collect();
        self.process_frame(time, &geometries)
    }

    /// Кладёт очередной кадр в буфер динамики.
    fn push_motion(&mut self, hands: &[HandGeometry]) {
        let Some(first) = hands.first() else {
            self.last_center = None;
            return;
        };
        let size = first.size.max(1.0);
        let previous = self.last_center.unwrap_or(first.center);
        let dx = (first.center.x - previous.x) / size;
        let dy = (first.center.y - previous.y) / size;
        if let Some(shape) = HandFeatures::sign_vector(hands) {
            self.motion_frames
                .push_back(MotionFeatures::frame(&shape, dx, dy));
        }
        self.last_center = Some(first.center);

        let max_frames = (MOTION_BUFFER * ASSUMED_FPS) as usize;
        while self.motion_frames.len() > max_frames {
            self.motion_frames.pop_front();
        }
    }

    /// Ведёт обратный отсчёт и запись. `true` — кадр занят записью.
    fn advance_recording(&mut self, time: f32, hands: &[HandGeometry]) -> bool {
        match self.recording {
            RecordingState::Countdown(visible) => {
                if time >= self.countdown_until {
                    // Запись начинается только с рукой в кадре: считать
                    // нечего, и пользователь должен увидеть причину отмены.
                    if hands.is_empty() {
                        self.notice_soon("Рука пропала. Запись прервана", time);
                        self.cancel_recording();
                        return true;
                    }
                    self.recording = RecordingState::Recording(0.0);
                    self.record_started = time;
                    if let Some(pending) = &mut self.pending {
                        pending.poses.clear();
                        pending.motion.clear();
                    }
                    // Кадр, на котором началась запись, тоже часть жеста.
                    self.collect(time, hands);
                } else {
                    // Число на экране не должно мигать: округляем вниз,
                    // чтобы «3» сменилось «2» один раз, а не каждый кадр.
                    let remaining = ((self.countdown_until - time).ceil() as u8)
                        .clamp(1, RECORD_COUNTDOWN as u8);
                    if remaining != visible {
                        self.recording = RecordingState::Countdown(remaining);
                    }
                }
                true
            }

            RecordingState::Recording(_) => {
                if hands.is_empty() {
                    self.notice_soon("Рука пропала. Запись прервана", time);
                    self.cancel_recording();
                    return true;
                }
                self.collect(time, hands);
                let elapsed = time - self.record_started;
                if elapsed >= RECORD_DURATION {
                    self.finish_recording(time);
                } else {
                    self.recording =
                        RecordingState::Recording((elapsed / RECORD_DURATION).clamp(0.0, 1.0));
                }
                true
            }

            RecordingState::Idle => false,
        }
    }

    /// Добавляет кадр к незавершённой записи.
    fn collect(&mut self, time: f32, hands: &[HandGeometry]) {
        let Some(pending) = &mut self.pending else {
            return;
        };
        if let Some(shape) = HandFeatures::sign_vector(hands) {
            pending.poses.push(shape);
        }
        // Динамику начинаем копить не сразу: первые кадры рука ещё входит
        // в кадр, и их смещение — артефакт появления, а не часть жеста.
        if time - self.record_started > RECORD_LEAD_IN {
            if let Some(frame) = self.motion_frames.back() {
                pending.motion.push(frame.clone());
            }
        }
    }

    /// Прокидывает кадр через автомат с замыканием-классификатором.
    fn recognize(&mut self, time: f32, hands: &[HandGeometry]) -> GestureResult<Sign> {
        let mode = self.mode;
        let library = &self.library;
        let stream: Vec<Vec<f32>> = self.motion_frames.iter().cloned().collect();
        self.recognizer.process(time, hands, |hands, motion| {
            classify(library, mode, &stream, hands, motion)
        })
    }

    /// Режим управления: встроенный жест превращается в команду.
    fn queue_command(&mut self, sign: &Sign, time: f32) {
        let Some(gesture) = built_in(sign) else {
            return;
        };
        if let Some(command) = gesture.command() {
            self.command_queue.push_back(command);
            self.next_command_at = self.next_command_at.max(time);
        }
    }

    /// Режим перевода: слово уходит в фразу, если жест подтверждён
    /// непрерывной последовательностью.
    fn push_stream(&mut self, sign: &Sign, time: f32) {
        let Some(word) = sign.word() else {
            return;
        };
        // Пауза длиннее окна означает новое слово, а не продолжение.
        if time - self.last_gesture > STREAM_WINDOW {
            self.streaming.clear();
            self.speakable = 0;
        }
        self.last_gesture = time;
        self.streaming.push(word.to_string());
    }

    /// Выдаёт не более одной команды за `COMMAND_GAP`: серия одинаковых
    /// жестов не должна прокрутить список экранов за один кадр.
    fn pump_commands(&mut self, time: f32, event: &mut EngineEvent) {
        if time < self.next_command_at {
            return;
        }
        if let Some(command) = self.command_queue.pop_front() {
            self.next_command_at = time + COMMAND_GAP;
            self.execute_command(command, event);
        }
    }

    fn execute_command(&mut self, command: Command, event: &mut EngineEvent) {
        match command {
            Command::Confirm | Command::Select => {}
            Command::Pause => {
                self.set_recognition_enabled(!self.recognition_enabled);
            }
            Command::NextScreen => {
                self.screen_index = (self.screen_index + 1) % DEMO_SCREENS.len();
            }
            Command::PreviousScreen => {
                self.screen_index = if self.screen_index == 0 {
                    DEMO_SCREENS.len() - 1
                } else {
                    self.screen_index - 1
                };
            }
            Command::Increase => {
                self.parameter = (self.parameter + PARAMETER_STEP).min(PARAMETER_MAX);
            }
            Command::Decrease => {
                self.parameter = (self.parameter - PARAMETER_STEP).max(PARAMETER_MIN);
            }
        }
        event.commands.push(command);
    }

    /// Озвучивает накопленную фразу.
    ///
    /// Слова произносятся по одному с паузой, как в оригинале, поэтому
    /// вызывается каждый кадр — очередь продвигается сама.
    fn advance_speech(&mut self, time: f32, event: &mut EngineEvent) {
        if !self.speech_enabled || time < self.next_spoke_at {
            return;
        }
        if self.speakable >= self.streaming.len() {
            return;
        }
        let word = self.streaming[self.speakable].clone();
        self.speakable += 1;
        self.speaker.speak(&word);
        self.next_spoke_at = time + SPEAK_GAP;
        event.speak = Some(word);
    }

    /// Снимок состояния для интерфейса.
    pub fn snapshot(&self, hands: &[HandGeometry]) -> EngineSnapshot {
        let span = PARAMETER_MAX - PARAMETER_MIN;
        EngineSnapshot {
            mode: self.mode,
            recognition_enabled: self.recognition_enabled,
            speech_enabled: self.speech_enabled,
            sign: self.current.clone(),
            streaming_word: self.streaming.last().cloned(),
            phrase: self.current_phrase(),
            recording: self.recording.into(),
            notice: self.notice.clone(),
            signs_count: self.library.len(),
            screen_index: self.screen_index,
            zoom: self.parameter,
            volume: ((self.parameter - PARAMETER_MIN) / span).clamp(0.0, 1.0),
            parameter: self.parameter,
            hands_visible: hands.len(),
            hands: hands
                .iter()
                .map(|h| h.p.iter().map(|&p| p.into()).collect())
                .collect(),
        }
    }

    /// Нажатие на элемент демо-интерфейса.
    ///
    /// Команда выполняется сразу: палец нажал один раз — и ровно одно
    /// действие произошло, без задержки и без повторов.
    pub fn tap_index(&mut self, index: usize, time: f32, event: &mut EngineEvent) {
        let Some(command) = tap_command(index) else {
            return;
        };
        self.command_queue.push_back(command);
        self.next_command_at = time;
        self.pump_commands(time, event);
    }

    /// Останавливает озвучку.
    pub fn stop_speech(&mut self) {
        self.speaker.stop();
    }

    /// Озвучивает накопленную фразу целиком, игнорируя паузу между словами.
    pub fn finish_phrase(&mut self, time: f32, event: &mut EngineEvent) {
        if !self.speech_enabled {
            return;
        }
        if self.speakable < self.streaming.len() {
            let word = self.streaming[self.speakable].clone();
            self.speakable += 1;
            self.speaker.speak(&word);
            self.next_spoke_at = time + SPEAK_GAP;
            event.speak = Some(word);
        }
    }
}

impl Default for GestureEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Команда элемента демо-интерфейса.
fn tap_command(index: usize) -> Option<Command> {
    Some(match index {
        0 => Command::Confirm,
        1 => Command::Pause,
        2 => Command::Select,
        3 => Command::PreviousScreen,
        4 => Command::Decrease,
        5 => Command::Increase,
        6 => Command::NextScreen,
        _ => return None,
    })
}

/// Классификатор: встроенные жесты в режиме «Управление»,
/// словарь пользователя — в режиме «Перевод».
///
/// Возвращает `Sign::None`, когда жест не узнан: для автомата это сигнал
/// «попробовать ещё раз», а не ложное срабатывание.
fn classify(
    library: &SignLibrary,
    mode: AppMode,
    stream: &[Vec<f32>],
    hands: &[HandGeometry],
    motion: RecognizerMotion,
) -> Sign {
    match mode {
        AppMode::Control => match motion {
            RecognizerMotion::Still => hands
                .first()
                .map(|h| h.static_gesture())
                .filter(|g| *g != crate::models::Gesture::Idle)
                .map(Sign::BuiltIn)
                .unwrap_or(Sign::None),
            // Свайп в управлении — это и есть жест, независимо от позы.
            RecognizerMotion::Flick(direction) => Sign::BuiltIn(direction),
            // Медленное движение в управлении ничего не значит: там нет
            // пользовательских шаблонов.
            RecognizerMotion::Slow => Sign::None,
        },
        AppMode::Translate => match motion {
            RecognizerMotion::Still => library
                .classify_pose(hands, STATIC_THRESHOLD)
                .map(|s| Sign::custom(&s.id.to_string(), &s.word))
                .unwrap_or(Sign::None),
            // И быстрый, и медленный жест ищем по динамике: в переводе
            // важно движение, а не его скорость.
            RecognizerMotion::Flick(_) | RecognizerMotion::Slow => library
                .classify_motion(stream, DYNAMIC_THRESHOLD)
                .map(|s| Sign::custom(&s.id.to_string(), &s.word))
                .unwrap_or(Sign::None),
        },
    }
}

/// Встроенный жест из знака, если знак встроенный.
fn built_in(sign: &Sign) -> Option<crate::models::Gesture> {
    match sign {
        Sign::BuiltIn(gesture) => Some(*gesture),
        _ => None,
    }
}

/// Усредняет покадровые векторы, сгруппированные по числу рук.
///
/// Жест выполняется либо одной, либо двумя руками; смешанные кадры
/// отбрасываются, иначе среднее получилось бы бессмысленным.
fn average_frames(frames: &[Vec<f32>]) -> Option<Vec<Vec<f32>>> {
    let mut groups: std::collections::BTreeMap<usize, Vec<&Vec<f32>>> =
        std::collections::BTreeMap::new();
    for frame in frames {
        groups.entry(frame.len()).or_default().push(frame);
    }
    let hand_count = groups
        .iter()
        .max_by_key(|(_, v)| v.len())
        .map(|(k, _)| *k)?;
    let group = groups.get(&hand_count)?;
    let count = group.len();
    let width = group[0].len();
    let mut mean = vec![0.0f32; width];
    for frame in group {
        for (i, value) in frame.iter().enumerate() {
            mean[i] += value / count as f32;
        }
    }
    Some(vec![mean])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::joint;
    use crate::library::MemoryStore;
    use crate::models::Gesture;

    /// Движок обязан быть пригоден для общего состояния окна: его держат
    /// и поток кадров, и поток интерфейса. Проверка компилируется только
    /// при выполнении этих границ, поэтому регрессия ловится сборкой.
    #[test]
    fn engine_is_usable_from_several_threads() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<GestureEngine>();
        assert_send_sync::<MemoryStore>();
        assert_send_sync::<SilentSpeaker>();
    }

    /// Кисть с центром ладони в `(x, y)`, размер ладони 60.
    fn hand_at(x: f32, y: f32) -> HandGeometry {
        let mut p = [Point::MISSING; joint::COUNT];
        p[joint::WRIST] = Point::new(x, y + 60.0);
        p[joint::INDEX_MCP] = Point::new(x - 30.0, y - 10.0);
        p[joint::MIDDLE_MCP] = Point::new(x, y);
        p[joint::RING_MCP] = Point::new(x + 30.0, y - 10.0);
        p[joint::LITTLE_MCP] = Point::new(x, y - 40.0);
        for point in p[joint::THUMB_CMC..=joint::LITTLE_TIP].iter_mut() {
            if *point == Point::MISSING {
                *point = Point::new(x, y - 60.0);
            }
        }
        HandGeometry::new(&p).unwrap()
    }

    /// Раскладка «большой палец вверх»: кулак с высоко поднятым большим.
    fn thumbs_up() -> HandGeometry {
        let mut p = [Point::MISSING; joint::COUNT];
        p[joint::WRIST] = Point::new(100.0, 200.0);
        p[joint::INDEX_MCP] = Point::new(80.0, 150.0);
        p[joint::MIDDLE_MCP] = Point::new(100.0, 140.0);
        p[joint::RING_MCP] = Point::new(120.0, 150.0);
        p[joint::LITTLE_MCP] = Point::new(135.0, 165.0);
        for (mcp, pip, dip, tip) in [
            (
                joint::INDEX_MCP,
                joint::INDEX_PIP,
                joint::INDEX_DIP,
                joint::INDEX_TIP,
            ),
            (
                joint::MIDDLE_MCP,
                joint::MIDDLE_PIP,
                joint::MIDDLE_DIP,
                joint::MIDDLE_TIP,
            ),
            (
                joint::RING_MCP,
                joint::RING_PIP,
                joint::RING_DIP,
                joint::RING_TIP,
            ),
            (
                joint::LITTLE_MCP,
                joint::LITTLE_PIP,
                joint::LITTLE_DIP,
                joint::LITTLE_TIP,
            ),
        ] {
            let base = p[mcp];
            p[pip] = Point::new(base.x - 8.0, base.y - 5.0);
            p[dip] = Point::new(base.x - 2.0, base.y + 5.0);
            p[tip] = Point::new(base.x + 4.0, base.y + 12.0);
        }
        p[joint::THUMB_CMC] = Point::new(88.0, 185.0);
        p[joint::THUMB_MP] = Point::new(70.0, 155.0);
        p[joint::THUMB_IP] = Point::new(52.0, 130.0);
        p[joint::THUMB_TIP] = Point::new(40.0, 110.0);
        HandGeometry::new(&p).unwrap()
    }

    /// Кисть, смещённая на `dx`, с сохранением позы.
    fn shift(hand: &HandGeometry, dx: f32, dy: f32) -> HandGeometry {
        let mut p = hand.p;
        for point in p.iter_mut() {
            *point = Point::new(point.x + dx, point.y + dy);
        }
        HandGeometry::new(&p).unwrap()
    }

    /// Проводит жест в кадрах: тишина, появление, удержание.
    ///
    /// События всех кадров сливаются: команда или слово приходят на
    /// промежуточном кадре, когда автомат переходит в удержание, а не на
    /// последнем.
    fn show_from(
        engine: &mut GestureEngine,
        hand: &HandGeometry,
        start: f32,
    ) -> (EngineSnapshot, EngineEvent) {
        let mut snapshot = engine.snapshot(&[]);
        let mut merged = EngineEvent::default();
        for offset in [0.0_f32, 0.2, 0.5] {
            let (snap, event) = engine.process_frame(start + offset, std::slice::from_ref(hand));
            snapshot = snap;
            merged.commands.extend(event.commands);
            if merged.speak.is_none() {
                merged.speak = event.speak;
            }
        }
        (snapshot, merged)
    }

    /// Проводит жест с нуля времени.
    fn show(engine: &mut GestureEngine, hand: &HandGeometry) -> (EngineSnapshot, EngineEvent) {
        show_from(engine, hand, 0.0)
    }

    /// Прогоняет запись слова до конца: обратный отсчёт, сбор кадров и
    /// сохранение в словарь.
    fn record_word(engine: &mut GestureEngine, word: &str, start: f32, hand: &HandGeometry) {
        let hands = std::slice::from_ref(hand);
        engine.begin_recording(word, false, start);
        assert!(matches!(engine.recording(), RecordingState::Countdown(_)));

        // Обратный отсчёт.
        engine.process_frame(start + 1.0, hands);
        assert!(matches!(engine.recording(), RecordingState::Countdown(_)));

        // Три секунды обратного отсчёта прошли — идёт запись.
        engine.process_frame(start + 4.0, hands);
        assert!(
            matches!(engine.recording(), RecordingState::Recording(_)),
            "запись должна начаться"
        );

        // Оставшаяся длительность записи.
        engine.process_frame(start + 5.5, hands);
        assert_eq!(
            engine.recording(),
            RecordingState::Idle,
            "запись должна завершиться"
        );
    }

    #[test]
    fn empty_frame_reports_idle() {
        let mut engine = GestureEngine::new();
        let (snapshot, event) = engine.process_frame(0.0, &[]);
        assert!(snapshot.sign.is_none());
        assert_eq!(snapshot.hands_visible, 0);
        assert!(event.is_empty());
    }

    #[test]
    fn thumbs_up_in_control_confirms() {
        let mut engine = GestureEngine::new();
        let (snapshot, event) = show(&mut engine, &thumbs_up());
        assert_eq!(snapshot.sign, Sign::BuiltIn(Gesture::ThumbsUp));
        assert!(event.commands.contains(&Command::Confirm));
    }

    #[test]
    fn swipe_moves_to_next_screen() {
        let mut engine = GestureEngine::new();
        let hand = thumbs_up();
        engine.process_frame(0.0, std::slice::from_ref(&hand));
        engine.process_frame(0.1, &[shift(&hand, 40.0, 0.0)]);
        let (snapshot, event) = engine.process_frame(0.3, &[shift(&hand, 100.0, 0.0)]);

        assert!(event.commands.contains(&Command::NextScreen));
        assert_eq!(snapshot.screen_index, 1);
    }

    #[test]
    fn open_palm_pauses_recognition() {
        let mut engine = GestureEngine::new();
        let (snapshot, _) = show(&mut engine, &thumbs_up());
        assert!(snapshot.recognition_enabled);
        // Тот же автомат, но жест «пауза» выключает распознавание.
        let mut engine = GestureEngine::new();
        let open = HandGeometry::new(&open_palm_points()).unwrap();
        let (_, event) = show(&mut engine, &open);
        assert!(event.commands.contains(&Command::Pause));
        assert!(!engine.recognition_enabled());
    }

    #[test]
    fn tap_executes_command_immediately() {
        let mut engine = GestureEngine::new();
        let mut event = EngineEvent::default();
        engine.tap_index(6, 0.0, &mut event);
        assert_eq!(event.commands, vec![Command::NextScreen]);
        assert_eq!(engine.snapshot(&[]).screen_index, 1);

        event = EngineEvent::default();
        engine.tap_index(99, 1.0, &mut event);
        assert!(event.is_empty());
    }

    #[test]
    fn translate_ignores_built_in_gestures() {
        let mut engine = GestureEngine::new();
        engine.set_mode(AppMode::Translate);
        let (snapshot, event) = show(&mut engine, &thumbs_up());
        assert!(snapshot.phrase.is_empty());
        assert!(event.is_empty());
    }

    /// Записывает жест в словарь и возвращает слово, которое он даёт.
    fn record_and_match(word: &str, hand: &HandGeometry) -> String {
        let mut engine = GestureEngine::new();
        record_word(&mut engine, word, 0.0, hand);
        assert_eq!(engine.library().len(), 1, "жест должен попасть в словарь");

        engine.set_mode(AppMode::Translate);
        let (snapshot, _) = show_from(&mut engine, hand, 10.0);
        snapshot.phrase
    }

    #[test]
    fn recorded_static_gesture_is_recognized() {
        assert_eq!(record_and_match("привет", &thumbs_up()), "ПРИВЕТ");
    }

    #[test]
    fn two_different_words_stay_apart() {
        let mut engine = GestureEngine::new();
        let hand = thumbs_up();
        // Один жест записан под двумя словами — слов в словаре станет два,
        // иначе словарь склеил бы их в один.
        record_word(&mut engine, "да", 0.0, &hand);
        record_word(&mut engine, "нет", 10.0, &hand);
        assert_eq!(engine.library().len(), 2);
    }

    #[test]
    fn recording_aborts_when_hand_leaves() {
        let mut engine = GestureEngine::new();
        let hand = thumbs_up();
        engine.begin_recording("слово", false, 0.0);
        engine.process_frame(1.0, std::slice::from_ref(&hand));
        engine.process_frame(4.0, std::slice::from_ref(&hand));
        assert!(matches!(engine.recording(), RecordingState::Recording(_)));

        engine.process_frame(4.5, &[]);
        assert_eq!(engine.recording(), RecordingState::Idle);
        assert_eq!(
            engine.library().len(),
            0,
            "прерванная запись не сохраняется"
        );
        assert!(engine.snapshot(&[]).notice.is_some());
    }

    #[test]
    fn recording_without_hand_does_not_start() {
        let mut engine = GestureEngine::new();
        engine.begin_recording("слово", false, 0.0);
        engine.process_frame(1.0, &[]);
        engine.process_frame(4.0, &[]);
        assert_eq!(engine.recording(), RecordingState::Idle);
        assert_eq!(engine.library().len(), 0);
    }

    #[test]
    fn empty_word_is_rejected() {
        let mut engine = GestureEngine::new();
        engine.begin_recording("   ", false, 0.0);
        assert_eq!(engine.recording(), RecordingState::Idle);
        assert!(engine.snapshot(&[]).notice.is_some());
    }

    #[test]
    fn words_speak_one_by_one() {
        let mut engine = GestureEngine::new();
        let hand = thumbs_up();
        record_word(&mut engine, "да", 0.0, &hand);
        engine.set_mode(AppMode::Translate);

        // Первое слово: озвучивается сразу.
        let mut spoken = Vec::new();
        let mut time = 10.0;
        for _ in 0..3 {
            let (_, event) = engine.process_frame(time, std::slice::from_ref(&hand));
            if let Some(word) = event.speak {
                spoken.push(word);
            }
            time += 0.1;
        }
        assert_eq!(spoken, vec!["да".to_string()]);
    }

    #[test]
    fn speech_can_be_switched_off() {
        let mut engine = GestureEngine::new();
        engine.set_speech_enabled(false);
        assert!(!engine.speech_enabled());
        engine.set_speech_enabled(true);
        assert!(engine.speech_enabled());
    }

    #[test]
    fn paused_recognition_ignores_gestures() {
        let mut engine = GestureEngine::new();
        engine.set_recognition_enabled(false);
        let (_, event) = show(&mut engine, &thumbs_up());
        assert!(event.is_empty());
        assert!(engine.snapshot(&[]).sign.is_none());
    }

    #[test]
    fn mode_switch_resets_phrase() {
        let mut engine = GestureEngine::new();
        engine.set_mode(AppMode::Translate);
        assert_eq!(engine.mode(), AppMode::Translate);
        assert!(engine.snapshot(&[]).notice.is_some());

        engine.set_mode(AppMode::Control);
        assert_eq!(engine.mode(), AppMode::Control);
    }

    #[test]
    fn increase_and_decrease_clamp_parameter() {
        let mut engine = GestureEngine::new();
        let mut event = EngineEvent::default();
        for _ in 0..50 {
            engine.tap_index(5, 0.0, &mut event);
        }
        assert!((engine.snapshot(&[]).parameter - PARAMETER_MAX).abs() < 1e-6);

        for _ in 0..50 {
            engine.tap_index(4, 0.0, &mut event);
        }
        let snapshot = engine.snapshot(&[]);
        assert!((snapshot.parameter - PARAMETER_MIN).abs() < 1e-6);
        assert!((snapshot.volume - 0.0).abs() < 1e-6);
    }

    #[test]
    fn snapshot_carries_hands_for_overlay() {
        let mut engine = GestureEngine::new();
        let (snapshot, _) = engine.process_frame(0.0, &[thumbs_up(), hand_at(300.0, 300.0)]);
        assert_eq!(snapshot.hands_visible, 2);
        assert_eq!(snapshot.hands.len(), 2);
        assert_eq!(snapshot.hands[0].len(), joint::COUNT);
    }

    #[test]
    fn broken_points_are_dropped() {
        let mut engine = GestureEngine::new();
        let broken = vec![Point::MISSING; 5];
        let (snapshot, _) = engine.process_points(0.0, &[broken]);
        assert_eq!(snapshot.hands_visible, 0);
    }

    #[test]
    fn delete_removes_sign() {
        let mut engine = GestureEngine::new();
        record_word(&mut engine, "да", 0.0, &thumbs_up());
        assert_eq!(engine.library().len(), 1);

        engine.delete_sign(0);
        assert_eq!(engine.library().len(), 0);
    }

    /// Раскладка «открытая ладонь» для проверки жеста паузы.
    fn open_palm_points() -> [Point; joint::COUNT] {
        let mut p = [Point::MISSING; joint::COUNT];
        p[joint::WRIST] = Point::new(100.0, 200.0);
        p[joint::INDEX_MCP] = Point::new(75.0, 155.0);
        p[joint::MIDDLE_MCP] = Point::new(100.0, 145.0);
        p[joint::RING_MCP] = Point::new(125.0, 155.0);
        p[joint::LITTLE_MCP] = Point::new(148.0, 170.0);
        for (mcp, pip, dip, tip) in [
            (
                joint::INDEX_MCP,
                joint::INDEX_PIP,
                joint::INDEX_DIP,
                joint::INDEX_TIP,
            ),
            (
                joint::MIDDLE_MCP,
                joint::MIDDLE_PIP,
                joint::MIDDLE_DIP,
                joint::MIDDLE_TIP,
            ),
            (
                joint::RING_MCP,
                joint::RING_PIP,
                joint::RING_DIP,
                joint::RING_TIP,
            ),
            (
                joint::LITTLE_MCP,
                joint::LITTLE_PIP,
                joint::LITTLE_DIP,
                joint::LITTLE_TIP,
            ),
        ] {
            let base = p[mcp];
            p[pip] = Point::new(base.x, base.y - 15.0);
            p[dip] = Point::new(base.x, base.y - 25.0);
            p[tip] = Point::new(base.x, base.y - 35.0);
        }
        p[joint::THUMB_CMC] = Point::new(80.0, 195.0);
        p[joint::THUMB_MP] = Point::new(62.0, 190.0);
        p[joint::THUMB_IP] = Point::new(48.0, 183.0);
        p[joint::THUMB_TIP] = Point::new(38.0, 175.0);
        p
    }
}
