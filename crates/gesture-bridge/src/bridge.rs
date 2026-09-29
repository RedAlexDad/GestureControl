//! Мост между ядром и интерфейсом.
//!
//! Ядро (`gesture-core`) ничего не знает про окно, JSON и потоки, поэтому
//! повторить его логику здесь нельзя. Мост не переигрывает решение ядра,
//! а только переводит его на язык интерфейса: принимает кадр, отдаёт готовое
//! состояние и события.
//!
//! ```text
//!   камера → FrameInput → GestureBridge → AppState + EngineEvent → UI
//!                        │
//!                        └─ GestureEngine (gesture-core)
//! ```
//!
//! Благодаря такой прослойке вся логика приложения проверяется обычным
//! `cargo test`, а окно Tauri получает тонкий слой команд без тестов.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gesture_core::pipeline::EngineEvent;
use gesture_core::{AppMode, GestureEngine, JsonFileStore, Point, SilentSpeaker};

use crate::clock::{MonotonicClock, Seconds, SharedClock};
use crate::error::BridgeError;
use crate::state::{AppState, FrameInput, SignRow};

/// Допустимое расхождение меток времени между соседними кадрами.
///
/// Камера дрожит на доли кадра, и это нормально; расхождение в десятки
/// миллисекунд означает переупорядоченный кадр, а не дрожание.
const STALE_TOLERANCE: Seconds = 0.001;

/// Кисть, отброшенная из-за неверного числа точек.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RejectedHand {
    /// Номер кисти в кадре, начиная с нуля.
    pub hand: usize,
    /// Сколько точек пришло.
    pub received: usize,
    /// Сколько точек ожидает ядро.
    pub expected: usize,
}

/// Что показать интерфейсу после одной операции.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BridgeResult {
    /// Состояние для отрисовки.
    pub state: AppState,
    /// События: речь, команды, уведомления.
    pub event: EngineEvent,
    /// Кисти, отброшенные из-за неверного числа точек.
    pub rejected: Vec<RejectedHand>,
    /// Кадр пришёл с устаревшей меткой и не разобран.
    pub stale: bool,
}

impl BridgeResult {
    /// Ответ без событий и отбракованных кистей.
    fn quiet(state: AppState) -> Self {
        Self {
            state,
            event: EngineEvent::default(),
            rejected: Vec::new(),
            stale: false,
        }
    }
}

/// Счётчики потока кадров.
///
/// Нужен, чтобы отличить «жест не распознан» от «камера не поставляет
/// кадры» или «слой зрения отдаёт битые точки».
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BridgeMetrics {
    /// Сколько кадров дошло до ядра.
    pub frames_ingested: u64,
    /// Сколько кадров отброшено из-за неверной метки времени.
    pub frames_stale: u64,
    /// Сколько кистей отброшено из-за неверного числа точек.
    pub hands_rejected: u64,
}

/// Мост: один источник правды для интерфейса.
#[derive(Debug)]
pub struct GestureBridge {
    engine: GestureEngine,
    clock: SharedClock,
    /// Путь к словарю, если он должен переживать перезапуск.
    store_path: Option<PathBuf>,
    /// Метка последнего принятого кадра.
    last_time: Seconds,
    metrics: BridgeMetrics,
}

impl GestureBridge {
    /// Мост в памяти: словарь живёт до конца сеанса.
    pub fn new() -> Self {
        Self::with_clock(Arc::new(MonotonicClock::new()))
    }

    /// Мост с заданным источником времени.
    pub fn with_clock(clock: SharedClock) -> Self {
        Self::build(GestureEngine::new(), clock, None)
    }

    /// Мост со словарём в JSON-файле.
    ///
    /// Отсутствующий файл считается пустым словарём, а не ошибкой: первый
    /// запуск приложения обязан работать.
    pub fn with_file(path: impl Into<PathBuf>) -> Self {
        Self::with_file_and_clock(path, Arc::new(MonotonicClock::new()))
    }

    /// Мост со словарём в файле и заданным источником времени.
    pub fn with_file_and_clock(path: impl Into<PathBuf>, clock: SharedClock) -> Self {
        let path = path.into();
        let store = JsonFileStore::new(path.clone());
        let engine = GestureEngine::load(Box::new(store), Box::new(SilentSpeaker));
        Self::build(engine, clock, Some(path))
    }

    fn build(engine: GestureEngine, clock: SharedClock, store_path: Option<PathBuf>) -> Self {
        Self {
            engine,
            clock,
            store_path,
            last_time: f32::NEG_INFINITY,
            metrics: BridgeMetrics::default(),
        }
    }

    // ---------------------------------------------------------------- кадры

    /// Разбирает очередной кадр камеры.
    ///
    /// Кисти с неверным числом точек отбрасываются по одной: один битый
    /// детектор не должен останавливать весь кадр. Кадр с устаревшей меткой
    /// времени игнорируется целиком, иначе буфер динамики и обратный отсчёт
    /// откатились бы назад.
    pub fn ingest(&mut self, frame: &FrameInput) -> BridgeResult {
        if !frame.time.is_finite() {
            self.metrics.frames_stale += 1;
            return BridgeResult {
                stale: true,
                ..BridgeResult::quiet(self.state())
            };
        }
        if frame.time + STALE_TOLERANCE < self.last_time {
            self.metrics.frames_stale += 1;
            return BridgeResult {
                stale: true,
                ..BridgeResult::quiet(self.state())
            };
        }

        let mut hands = Vec::with_capacity(frame.hands.len());
        let mut rejected = Vec::new();
        for (index, hand) in frame.hands.iter().enumerate() {
            if hand.len() == gesture_core::joint::COUNT {
                hands.push(hand.iter().map(|p| Point::new(p.x, p.y)).collect());
            } else {
                rejected.push(RejectedHand {
                    hand: index,
                    received: hand.len(),
                    expected: gesture_core::joint::COUNT,
                });
            }
        }

        self.metrics.hands_rejected += rejected.len() as u64;
        self.metrics.frames_ingested += 1;
        self.last_time = frame.time.max(self.last_time);

        let (engine, event) = self.engine.process_points(frame.time, &hands);
        BridgeResult {
            state: self.wrap(engine),
            event,
            rejected,
            stale: false,
        }
    }

    /// Снимок состояния для отрисовки без разбора кадра.
    pub fn state(&self) -> AppState {
        self.wrap(self.engine.snapshot(&[]))
    }

    // -------------------------------------------------------------- команды

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
        let time = self.clock.now();
        self.last_time = self.last_time.max(time);
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
        let time = self.clock.now();
        self.last_time = self.last_time.max(time);
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

    // -------------------------------------------------------------- словарь

    /// Начинает запись нового жеста.
    pub fn start_recording(&mut self, word: &str, is_dynamic: bool) -> AppState {
        let time = self.clock.now();
        self.last_time = self.last_time.max(time);
        self.engine.begin_recording(word, is_dynamic, time);
        self.state()
    }

    /// Отменяет запись жеста.
    pub fn cancel_recording(&mut self) -> AppState {
        self.engine.cancel_recording();
        self.state()
    }

    /// Удаляет жест по идентификатору.
    ///
    /// Идентификатор, а не позиция: после удаления строки все позиции
    /// сдвигаются, и нажатие второй кнопки удалило бы не тот жест.
    pub fn delete_sign(&mut self, id: &str) -> AppState {
        self.engine.delete_sign_id(id);
        self.state()
    }

    /// Жесты словаря для списка в интерфейсе.
    pub fn signs(&self) -> Vec<SignRow> {
        self.engine.signs().iter().map(SignRow::from).collect()
    }

    /// Сбрасывает словарь и сохраняет пустой.
    pub fn clear_signs(&mut self) -> Result<AppState, BridgeError> {
        while !self.engine.signs().is_empty() {
            self.engine.delete_sign(0);
        }
        self.save()?;
        Ok(self.state())
    }

    /// Принудительно записывает словарь на диск.
    ///
    /// Движок сохраняет сам после записи и удаления, поэтому это нужно
    /// только при снятии с диска: интерфейсу «сохранить» без всякой причины.
    pub fn save(&self) -> Result<(), BridgeError> {
        let Some(path) = &self.store_path else {
            return Err(BridgeError::Unavailable("путь к словарю не задан"));
        };
        self.engine
            .library()
            .save(&JsonFileStore::new(path.as_path()))
            .map_err(BridgeError::from)
    }

    /// Путь к файлу словаря, если он задан.
    pub fn store_path(&self) -> Option<&Path> {
        self.store_path.as_deref()
    }

    /// Счётчики потока кадров.
    pub fn metrics(&self) -> BridgeMetrics {
        self.metrics
    }

    /// Текущее время по часам моста.
    pub fn now(&self) -> Seconds {
        self.clock.now()
    }

    fn wrap(&self, engine: gesture_core::EngineSnapshot) -> AppState {
        AppState::new(engine, self.signs())
    }
}

impl Default for GestureBridge {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::ManualClock;
    use gesture_core::joint;
    use gesture_core::pipeline::{PointSer, RecordingStateSer};

    /// Прямоугольник ладони вокруг центра: 21 точка, размер 100.
    fn hand(cx: f32, cy: f32) -> Vec<PointSer> {
        let x0 = cx - 50.0;
        let y0 = cy - 50.0;
        let mut points = vec![PointSer { x: x0, y: y0 }; joint::COUNT];
        // Запястье и костяшки — по краям, иначе размер ладони нулевой.
        points[joint::WRIST] = PointSer {
            x: x0,
            y: cy + 50.0,
        };
        points[joint::INDEX_MCP] = PointSer { x: x0, y: y0 };
        points[joint::MIDDLE_MCP] = PointSer {
            x: x0 + 25.0,
            y: y0,
        };
        points[joint::RING_MCP] = PointSer {
            x: x0 + 50.0,
            y: y0,
        };
        points[joint::LITTLE_MCP] = PointSer {
            x: x0 + 50.0,
            y: y0 + 25.0,
        };
        points
    }

    fn bridge() -> (GestureBridge, Arc<ManualClock>) {
        let clock = Arc::new(ManualClock::new());
        (GestureBridge::with_clock(clock.clone()), clock)
    }

    fn temp_path(tag: &str) -> PathBuf {
        let unique = uuid::Uuid::new_v4();
        std::env::temp_dir().join(format!("gesture-bridge-{tag}-{unique}.json"))
    }

    // ------------------------------------------------------------- кадры

    #[test]
    fn empty_frame_ingests_without_events() {
        let (mut bridge, _) = bridge();
        let result = bridge.ingest(&FrameInput::empty(0.0));
        assert!(result.event.is_empty());
        assert!(result.rejected.is_empty());
        assert!(!result.stale);
        assert_eq!(result.state.engine.hands_visible, 0);
        assert_eq!(bridge.metrics().frames_ingested, 1);
    }

    #[test]
    fn good_hand_reaches_engine_and_shows_in_snapshot() {
        let (mut bridge, _) = bridge();
        let result = bridge.ingest(&FrameInput::with_hand(0.0, hand(320.0, 240.0)));
        assert!(result.rejected.is_empty());
        assert_eq!(result.state.engine.hands_visible, 1);
        assert_eq!(result.state.engine.hands[0].len(), joint::COUNT);
    }

    #[test]
    fn broken_hand_is_dropped_but_frame_survives() {
        let (mut bridge, _) = bridge();
        let frame = FrameInput {
            time: 0.0,
            hands: vec![vec![PointSer { x: 1.0, y: 1.0 }; 5], hand(320.0, 240.0)],
        };
        let result = bridge.ingest(&frame);
        assert_eq!(result.rejected.len(), 1);
        assert_eq!(result.rejected[0].hand, 0);
        assert_eq!(result.rejected[0].received, 5);
        assert_eq!(result.rejected[0].expected, 21);
        // Вторая кисть обработана: битая первая не остановила кадр.
        assert_eq!(result.state.engine.hands_visible, 1);
        assert_eq!(bridge.metrics().hands_rejected, 1);
    }

    #[test]
    fn all_broken_hands_leave_frame_empty() {
        let (mut bridge, _) = bridge();
        let frame = FrameInput {
            time: 0.0,
            hands: vec![vec![PointSer { x: 0.0, y: 0.0 }; 4]],
        };
        let result = bridge.ingest(&frame);
        assert_eq!(result.rejected.len(), 1);
        assert_eq!(result.state.engine.hands_visible, 0);
        // Кадр всё равно учтён: поток жив, просто в кадре ничего нет.
        assert_eq!(bridge.metrics().frames_ingested, 1);
    }

    #[test]
    fn stale_frame_is_skipped_and_counted() {
        let (mut bridge, _) = bridge();
        bridge.ingest(&FrameInput::with_hand(1.0, hand(320.0, 240.0)));
        let result = bridge.ingest(&FrameInput::with_hand(0.5, hand(400.0, 240.0)));
        assert!(result.stale);
        assert_eq!(bridge.metrics().frames_ingested, 1);
        assert_eq!(bridge.metrics().frames_stale, 1);
    }

    #[test]
    fn non_finite_time_is_treated_as_stale() {
        let (mut bridge, _) = bridge();
        let frame = FrameInput::with_hand(f32::NAN, hand(320.0, 240.0));
        assert!(bridge.ingest(&frame).stale);
        let frame = FrameInput::with_hand(f32::INFINITY, hand(320.0, 240.0));
        assert!(bridge.ingest(&frame).stale);
        assert_eq!(bridge.metrics().frames_ingested, 0);
    }

    #[test]
    fn tiny_time_jitter_is_accepted() {
        let (mut bridge, _) = bridge();
        bridge.ingest(&FrameInput::with_hand(1.0, hand(320.0, 240.0)));
        // Камера дрожит на доли кадра — это не переупорядоченный кадр.
        let result = bridge.ingest(&FrameInput::with_hand(0.999, hand(320.0, 240.0)));
        assert!(!result.stale);
        assert_eq!(bridge.metrics().frames_stale, 0);
    }

    #[test]
    fn two_hands_are_ingested_together() {
        let (mut bridge, _) = bridge();
        let frame = FrameInput {
            time: 0.0,
            hands: vec![hand(220.0, 240.0), hand(420.0, 240.0)],
        };
        let result = bridge.ingest(&frame);
        assert_eq!(result.state.engine.hands_visible, 2);
    }

    // ------------------------------------------------------------- команды

    #[test]
    fn set_mode_switches_and_is_reflected_in_state() {
        let (mut bridge, _) = bridge();
        assert_eq!(bridge.state().mode, AppMode::Control);
        let state = bridge.set_mode(AppMode::Translate);
        assert_eq!(state.mode, AppMode::Translate);
        assert_eq!(state.mode_title, "Перевод");
    }

    #[test]
    fn toggles_flip_flags_once_each() {
        let (mut bridge, _) = bridge();
        assert!(bridge.state().engine.recognition_enabled);
        assert!(!bridge.toggle_recognition().engine.recognition_enabled);
        assert!(bridge.toggle_recognition().engine.recognition_enabled);
        assert!(!bridge.toggle_speech().engine.speech_enabled);
        assert!(bridge.toggle_speech().engine.speech_enabled);
    }

    #[test]
    fn tap_out_of_range_is_harmless() {
        let (mut bridge, _) = bridge();
        let result = bridge.tap(999);
        assert!(result.event.commands.is_empty());
    }

    #[test]
    fn tap_keeps_frame_clock_monotonic() {
        let (mut bridge, clock) = bridge();
        bridge.ingest(&FrameInput::with_hand(5.0, hand(320.0, 240.0)));
        bridge.tap(0);
        // Кадр с прежней меткой не откатывается после команды с часов.
        let result = bridge.ingest(&FrameInput::with_hand(1.0, hand(320.0, 240.0)));
        assert!(result.stale);
        assert_eq!(bridge.now(), clock.current());
    }

    #[test]
    fn finish_phrase_clears_phrase() {
        let (mut bridge, _) = bridge();
        let result = bridge.finish_phrase();
        assert!(result.state.engine.phrase.is_empty());
    }

    // ------------------------------------------------------------- словарь

    #[test]
    fn recording_starts_with_countdown() {
        let (mut bridge, _) = bridge();
        let state = bridge.start_recording("да", false);
        assert!(matches!(
            state.engine.recording,
            RecordingStateSer::Countdown(_)
        ));
    }

    #[test]
    fn cancel_recording_returns_to_idle() {
        let (mut bridge, _) = bridge();
        bridge.start_recording("да", false);
        assert_eq!(
            bridge.cancel_recording().engine.recording,
            RecordingStateSer::Idle
        );
    }

    #[test]
    fn delete_unknown_id_changes_nothing() {
        let (mut bridge, _) = bridge();
        let state = bridge.delete_sign("00000000-0000-0000-0000-000000000000");
        assert!(state.signs.is_empty());
        assert_eq!(state.engine.signs_count, 0);
    }

    #[test]
    fn in_memory_bridge_has_no_store_path() {
        let (bridge, _) = bridge();
        assert!(bridge.store_path().is_none());
        let error = bridge.save().expect_err("нет пути");
        assert!(matches!(error, BridgeError::Unavailable(_)));
    }

    // --------------------------------------------------------- сохранение

    #[test]
    fn recorded_sign_survives_restart() {
        let path = temp_path("reload");
        let clock = Arc::new(ManualClock::new());
        let point = hand(320.0, 240.0);

        let recorded = {
            let mut bridge = GestureBridge::with_file_and_clock(path.clone(), clock.clone());
            bridge.start_recording("да", false);
            assert!(matches!(
                bridge.state().engine.recording,
                RecordingStateSer::Countdown(_)
            ));

            // Обратный отсчёт, затем сама запись.
            for time in [1.0, 4.0, 5.5] {
                bridge.ingest(&FrameInput::with_hand(time, point.clone()));
            }
            assert_eq!(
                bridge.state().engine.recording,
                RecordingStateSer::Idle,
                "запись должна завершиться"
            );
            bridge.signs()
        };
        assert_eq!(recorded.len(), 1, "жест должен попасть в словарь");
        assert_eq!(recorded[0].word, "да");
        assert!(!recorded[0].is_dynamic, "поза, а не движение");

        // Перезапуск: новый мост читает тот же файл.
        let reloaded = GestureBridge::with_file_and_clock(path.clone(), clock);
        assert_eq!(reloaded.signs(), recorded, "словарь пережил перезапуск");
        assert_eq!(reloaded.state().engine.signs_count, 1);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn delete_by_id_removes_exactly_that_sign() {
        let path = temp_path("delete");
        let clock = Arc::new(ManualClock::new());
        let point = hand(320.0, 240.0);
        let mut bridge = GestureBridge::with_file_and_clock(path.clone(), clock.clone());

        for (word, time) in [("да", 0.0), ("нет", 6.0), ("пока", 12.0)] {
            bridge.start_recording(word, false);
            for offset in [1.0, 4.0, 5.5] {
                bridge.ingest(&FrameInput::with_hand(time + offset, point.clone()));
            }
        }
        let signs = bridge.signs();
        assert_eq!(signs.len(), 3);

        // Удаляем средний: по позиции это был бы первый или второй.
        let middle = signs[1].id.clone();
        let state = bridge.delete_sign(&middle);
        let left: Vec<String> = state.signs.iter().map(|s| s.word.clone()).collect();
        assert_eq!(left, vec!["да", "пока"]);
        assert!(
            state.engine.notice.is_some(),
            "удаление должно быть объявлено"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn clear_signs_empties_file_backed_library() {
        let path = temp_path("clear");
        let clock = Arc::new(ManualClock::new());
        let point = hand(320.0, 240.0);
        let mut bridge = GestureBridge::with_file_and_clock(path.clone(), clock.clone());
        bridge.start_recording("да", false);
        for offset in [1.0, 4.0, 5.5] {
            bridge.ingest(&FrameInput::with_hand(offset, point.clone()));
        }
        assert_eq!(bridge.signs().len(), 1);

        let state = bridge.clear_signs().expect("очистка словаря");
        assert!(state.signs.is_empty());

        let reloaded = GestureBridge::with_file_and_clock(path.clone(), clock);
        assert!(reloaded.signs().is_empty(), "очистка сохранена на диск");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_file_is_an_empty_library_not_an_error() {
        let path = temp_path("missing");
        assert!(!path.exists());
        let bridge = GestureBridge::with_file(path.clone());
        assert!(bridge.signs().is_empty());
        assert_eq!(bridge.store_path(), Some(path.as_path()));
    }

    #[test]
    fn clear_signs_on_empty_library_is_noop() {
        let path = temp_path("clear");
        let mut bridge = GestureBridge::with_file(path.clone());
        let state = bridge.clear_signs().expect("очистка пустого словаря");
        assert!(state.signs.is_empty());
        assert!(path.exists(), "пустой словарь сохранён на диск");
        let _ = std::fs::remove_file(&path);
    }
}
