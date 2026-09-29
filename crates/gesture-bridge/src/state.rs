//! Сериализуемые структуры для интерфейса.
//!
//! Ядро отдаёт своё состояние, но интерфейсу нужен один пакет на
//! отрисовку: снимок движка, список жестов словаря и подписи экранов.
//! Всё здесь — чистые данные без логики, их безопасно отдавать в JSON.

use gesture_core::pipeline::{EngineSnapshot, PointSer};
use gesture_core::{AppMode, CustomSign, DemoScreen, DEMO_SCREENS};
use serde::{Deserialize, Serialize};

/// Кадр с камеры, пришедший в мост.
///
/// Точки уже в пикселях кадра: нормализация из единичных координат —
/// забота слоя зрения, а не интерфейса.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FrameInput {
    /// Метка времени кадра в секундах.
    pub time: f32,
    /// Кисти в кадре: у каждой 21 точка.
    pub hands: Vec<Vec<PointSer>>,
}

impl FrameInput {
    /// Кадр без рук.
    pub fn empty(time: f32) -> Self {
        Self {
            time,
            hands: Vec::new(),
        }
    }

    /// Кадр с одной кистью из точек.
    pub fn with_hand(time: f32, hand: Vec<PointSer>) -> Self {
        Self {
            time,
            hands: vec![hand],
        }
    }
}

/// Строка списка жестов в интерфейсе.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignRow {
    /// Идентификатор жеста: по нему интерфейс удаляет нужную строку.
    pub id: String,
    /// Слово без оформления: так оно лежит в словаре.
    pub word: String,
    /// Подпись для показа: слово в кавычках с пометкой «свой жест».
    pub title: String,
    /// Сколько кистей участвует в жесте.
    pub hand_count: usize,
    /// Жест записан по движению, а не по позе.
    pub is_dynamic: bool,
}

impl From<&CustomSign> for SignRow {
    fn from(sign: &CustomSign) -> Self {
        Self {
            id: sign.id.to_string(),
            word: sign.word.clone(),
            title: sign.title(),
            hand_count: sign.hand_count(),
            is_dynamic: !sign.sequences.is_empty(),
        }
    }
}

/// Экран демо-интерфейса в сериализуемом виде.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenRow {
    pub title: String,
    pub icon: String,
}

impl From<DemoScreen> for ScreenRow {
    fn from(screen: DemoScreen) -> Self {
        Self {
            title: screen.title.to_string(),
            icon: screen.icon.to_string(),
        }
    }
}

/// Полное состояние приложения: один ответ на запрос снимка.
///
/// Интерфейс запрашивает состояние целиком, а не собирает его из
/// отдельных вызовов, поэтому перерисовка не зависит от порядка ответов.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppState {
    /// Снимок движка: режим, фраза, запись, скелет кистей.
    pub engine: EngineSnapshot,
    /// Жесты пользовательского словаря.
    pub signs: Vec<SignRow>,
    /// Экраны, переключаемые жестами.
    pub screens: Vec<ScreenRow>,
    /// Какой режим выбран: дублирует `engine.mode` для быстрого чтения.
    pub mode: AppMode,
    /// Название режима на русском.
    pub mode_title: String,
    /// Есть ли во что переключаться дальше.
    pub has_more_screens: bool,
}

impl AppState {
    /// Собирает состояние из снимка движка и его жестов.
    pub fn new(engine: EngineSnapshot, signs: Vec<SignRow>) -> Self {
        let mode = engine.mode;
        let screen_index = engine.screen_index;
        let total = DEMO_SCREENS.len();
        Self {
            signs,
            screens: DEMO_SCREENS.iter().copied().map(ScreenRow::from).collect(),
            mode,
            mode_title: mode.title().to_string(),
            has_more_screens: total > 0 && screen_index + 1 < total,
            engine,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gesture_core::{Point, RecordingStateSer, Sign};

    fn snapshot(index: usize) -> EngineSnapshot {
        EngineSnapshot {
            mode: AppMode::Control,
            recognition_enabled: true,
            speech_enabled: true,
            sign: Sign::None,
            streaming_word: None,
            phrase: String::new(),
            recording: RecordingStateSer::Idle,
            notice: None,
            signs_count: 0,
            screen_index: index,
            zoom: 1.0,
            volume: 0.0,
            parameter: 1.0,
            hands_visible: 0,
            hands: Vec::new(),
        }
    }

    #[test]
    fn frame_input_defaults_to_no_hands() {
        let frame = FrameInput::default();
        assert!(frame.hands.is_empty());
        assert_eq!(frame.time, 0.0);
    }

    #[test]
    fn frame_input_wraps_single_hand() {
        let frame = FrameInput::with_hand(1.5, vec![PointSer { x: 1.0, y: 2.0 }]);
        assert_eq!(frame.time, 1.5);
        assert_eq!(frame.hands.len(), 1);
        assert_eq!(frame.hands[0][0], PointSer { x: 1.0, y: 2.0 });
    }

    #[test]
    fn app_state_carries_screens_and_mode_title() {
        let state = AppState::new(snapshot(0), Vec::new());
        assert_eq!(state.screens.len(), DEMO_SCREENS.len());
        assert_eq!(state.screens[0].title, "Главная");
        assert_eq!(state.mode_title, "Управление");
        assert!(
            state.has_more_screens,
            "после первого экрана есть следующий"
        );
    }

    #[test]
    fn app_state_knows_when_last_screen_reached() {
        let last = DEMO_SCREENS.len() - 1;
        let state = AppState::new(snapshot(last), Vec::new());
        assert!(!state.has_more_screens);
    }

    #[test]
    fn sign_row_reads_title_and_kind() {
        let mut sign = CustomSign::new("да");
        assert_eq!(
            SignRow::from(&sign),
            SignRow {
                id: sign.id.to_string(),
                word: "да".to_string(),
                title: "Свой жест «да»".to_string(),
                hand_count: sign.hand_count(),
                is_dynamic: false,
            }
        );
        sign.sequences.push(vec![vec![0.0; 4]]);
        assert!(SignRow::from(&sign).is_dynamic);
    }

    #[test]
    fn app_state_survives_json_round_trip() {
        let state = AppState::new(
            snapshot(1),
            vec![SignRow {
                id: "abc".to_string(),
                word: "нет".to_string(),
                title: "Свой жест «нет»".to_string(),
                hand_count: 1,
                is_dynamic: false,
            }],
        );
        let json = serde_json::to_string(&state).expect("state serializes");
        let back: AppState = serde_json::from_str(&json).expect("state deserializes");
        assert_eq!(state, back);
    }

    #[test]
    fn frame_input_survives_json_round_trip() {
        let frame = FrameInput::with_hand(0.5, vec![PointSer { x: -1.0, y: 0.0 }]);
        let json = serde_json::to_string(&frame).expect("frame serializes");
        let back: FrameInput = serde_json::from_str(&json).expect("frame deserializes");
        assert_eq!(frame, back);
    }

    #[test]
    fn missing_point_keeps_its_sentinel_after_round_trip() {
        let frame = FrameInput::with_hand(
            0.0,
            (0..gesture_core::joint::COUNT)
                .map(|_| PointSer::from(Point::MISSING))
                .collect(),
        );
        let json = serde_json::to_string(&frame).expect("frame serializes");
        let back: FrameInput = serde_json::from_str(&json).expect("frame deserializes");
        assert_eq!(back.hands[0][0], PointSer::from(Point::MISSING));
    }
}
