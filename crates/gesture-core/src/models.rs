//! Типы жестов, команд и режимов приложения.
//!
//! Перенос `GestureModels.swift`. Строковые подписи хранятся здесь же, чтобы
//! интерфейс не зависел от локализации Rust-части.

use serde::{Deserialize, Serialize};

/// Режим работы приложения.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppMode {
    /// Бесконтактное управление встроенными жестами.
    Control,
    /// Сурдоперевод по жестам из словаря пользователя.
    Translate,
}

impl AppMode {
    pub const ALL: [AppMode; 2] = [AppMode::Control, AppMode::Translate];

    pub fn title(self) -> &'static str {
        match self {
            AppMode::Control => "Управление",
            AppMode::Translate => "Перевод",
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            AppMode::Control => "control",
            AppMode::Translate => "translate",
        }
    }

    pub fn from_slug(s: &str) -> Option<Self> {
        match s {
            "control" => Some(AppMode::Control),
            "translate" => Some(AppMode::Translate),
            _ => None,
        }
    }
}

/// Команды управления, в которые преобразуются распознанные жесты.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    Confirm,
    Pause,
    Select,
    NextScreen,
    PreviousScreen,
    Increase,
    Decrease,
}

impl Command {
    pub fn title(self) -> &'static str {
        match self {
            Command::Confirm => "Подтверждение",
            Command::Pause => "Пауза / продолжить",
            Command::Select => "Выбор",
            Command::NextScreen => "Следующий экран",
            Command::PreviousScreen => "Предыдущий экран",
            Command::Increase => "Увеличение параметра",
            Command::Decrease => "Уменьшение параметра",
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            Command::Confirm => "confirm",
            Command::Pause => "pause",
            Command::Select => "select",
            Command::NextScreen => "next_screen",
            Command::PreviousScreen => "previous_screen",
            Command::Increase => "increase",
            Command::Decrease => "decrease",
        }
    }
}

/// Встроенные жесты для режима «Управление».
/// В режиме «Перевод» используются только жесты, которым обучил пользователь.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Gesture {
    /// Жест не распознан. Значение по умолчанию: автомат считает
    /// `Idle` признаком «ничего не произошло».
    #[default]
    Idle,
    // Статические жесты
    ThumbsUp,
    OpenPalm,
    Pointing,
    Fist,
    Victory,
    Ok,
    CallMe,
    // Динамические жесты
    SwipeRight,
    SwipeLeft,
    SwipeUp,
    SwipeDown,
}

impl Gesture {
    /// Порядок для интерфейса: как в `CaseIterable` оригинала.
    pub const ALL: [Gesture; 12] = [
        Gesture::Idle,
        Gesture::ThumbsUp,
        Gesture::OpenPalm,
        Gesture::Pointing,
        Gesture::Fist,
        Gesture::Victory,
        Gesture::Ok,
        Gesture::CallMe,
        Gesture::SwipeRight,
        Gesture::SwipeLeft,
        Gesture::SwipeUp,
        Gesture::SwipeDown,
    ];

    pub fn slug(self) -> &'static str {
        match self {
            Gesture::Idle => "idle",
            Gesture::ThumbsUp => "thumbs_up",
            Gesture::OpenPalm => "open_palm",
            Gesture::Pointing => "pointing",
            Gesture::Fist => "fist",
            Gesture::Victory => "victory",
            Gesture::Ok => "ok",
            Gesture::CallMe => "call_me",
            Gesture::SwipeRight => "swipe_right",
            Gesture::SwipeLeft => "swipe_left",
            Gesture::SwipeUp => "swipe_up",
            Gesture::SwipeDown => "swipe_down",
        }
    }

    pub fn from_slug(s: &str) -> Option<Self> {
        Gesture::ALL.into_iter().find(|g| g.slug() == s)
    }

    pub fn title(self) -> &'static str {
        match self {
            Gesture::Idle => "Жест не распознан",
            Gesture::ThumbsUp => "Большой палец вверх",
            Gesture::OpenPalm => "Открытая ладонь",
            Gesture::Pointing => "Указательный палец",
            Gesture::Fist => "Кулак",
            Gesture::Victory => "Два пальца (V)",
            Gesture::Ok => "Жест «Окей»",
            Gesture::CallMe => "Большой палец и мизинец",
            Gesture::SwipeRight => "Движение руки вправо",
            Gesture::SwipeLeft => "Движение руки влево",
            Gesture::SwipeUp => "Движение руки вверх",
            Gesture::SwipeDown => "Движение руки вниз",
        }
    }

    pub fn emoji(self) -> &'static str {
        match self {
            Gesture::Idle => "❔",
            Gesture::ThumbsUp => "👍",
            Gesture::OpenPalm => "✋",
            Gesture::Pointing => "☝️",
            Gesture::Fist => "✊",
            Gesture::Victory => "✌️",
            Gesture::Ok => "👌",
            Gesture::CallMe => "🤙",
            Gesture::SwipeRight => "➡️",
            Gesture::SwipeLeft => "⬅️",
            Gesture::SwipeUp => "⬆️",
            Gesture::SwipeDown => "⬇️",
        }
    }

    /// Подсказка, как правильно выполнить жест.
    pub fn hint(self) -> &'static str {
        match self {
            Gesture::Idle => "",
            Gesture::ThumbsUp => "Кулак, большой палец направлен вверх",
            Gesture::OpenPalm => "Все пальцы выпрямлены, ладонь к камере",
            Gesture::Pointing => "Выпрямлен только указательный палец",
            Gesture::Fist => "Все пальцы сжаты, большой прижат",
            Gesture::Victory => "Выпрямлены указательный и средний пальцы",
            Gesture::Ok => "Большой и указательный — кольцо, остальные выпрямлены",
            Gesture::CallMe => "Выпрямлены только большой палец и мизинец",
            Gesture::SwipeRight => "Быстро проведите рукой вправо",
            Gesture::SwipeLeft => "Быстро проведите рукой влево",
            Gesture::SwipeUp => "Быстро поднимите руку вверх",
            Gesture::SwipeDown => "Быстро опустите руку вниз",
        }
    }

    /// Режим «Управление»: жест → команда.
    pub fn command(self) -> Option<Command> {
        match self {
            Gesture::ThumbsUp => Some(Command::Confirm),
            Gesture::OpenPalm => Some(Command::Pause),
            Gesture::Pointing => Some(Command::Select),
            Gesture::SwipeRight => Some(Command::NextScreen),
            Gesture::SwipeLeft => Some(Command::PreviousScreen),
            Gesture::SwipeUp => Some(Command::Increase),
            Gesture::SwipeDown => Some(Command::Decrease),
            _ => None,
        }
    }

    pub fn is_dynamic(self) -> bool {
        matches!(
            self,
            Gesture::SwipeRight | Gesture::SwipeLeft | Gesture::SwipeUp | Gesture::SwipeDown
        )
    }
}

/// Результат распознавания: встроенный жест или жест из словаря пользователя.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Sign {
    #[default]
    None,
    BuiltIn(Gesture),
    Custom {
        id: String,
        word: String,
    },
}

impl Sign {
    pub fn custom(id: &str, word: &str) -> Self {
        Sign::Custom {
            id: id.to_string(),
            word: word.to_string(),
        }
    }

    pub fn is_none(&self) -> bool {
        matches!(self, Sign::None)
    }

    pub fn emoji(&self) -> &'static str {
        match self {
            Sign::None => Gesture::Idle.emoji(),
            Sign::BuiltIn(g) => g.emoji(),
            Sign::Custom { .. } => "🤟",
        }
    }

    pub fn title(&self) -> String {
        match self {
            Sign::None => Gesture::Idle.title().to_string(),
            Sign::BuiltIn(g) => g.title().to_string(),
            Sign::Custom { word, .. } => format!("Свой жест «{word}»"),
        }
    }

    /// Слово для режима «Перевод».
    /// Встроенные жесты в переводе не используются, поэтому слова у них нет.
    pub fn word(&self) -> Option<&str> {
        match self {
            Sign::Custom { word, .. } => Some(word),
            _ => None,
        }
    }

    /// Идентификатор записи словаря, если жест пользовательский.
    pub fn custom_id(&self) -> Option<&str> {
        match self {
            Sign::Custom { id, .. } => Some(id),
            _ => None,
        }
    }
}

/// Состояние записи нового жеста.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum RecordingState {
    #[default]
    Idle,
    /// Обратный отсчёт перед записью: 3, 2, 1.
    Countdown(u8),
    /// Идёт запись, значение — прогресс 0…1.
    Recording(f32),
}

impl RecordingState {
    pub fn is_idle(&self) -> bool {
        matches!(self, RecordingState::Idle)
    }
}

/// Экран демо-интерфейса управления.
///
/// Подписи хранятся статическими строками и не десериализуются: это
/// описание встроенного интерфейса, а не данные извне.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DemoScreen {
    pub title: &'static str,
    pub icon: &'static str,
}

/// Экраны, переключаемые жестами в режиме «Управление».
pub const DEMO_SCREENS: [DemoScreen; 4] = [
    DemoScreen {
        title: "Главная",
        icon: "house.fill",
    },
    DemoScreen {
        title: "Музыка",
        icon: "music.note",
    },
    DemoScreen {
        title: "Фото",
        icon: "photo.fill",
    },
    DemoScreen {
        title: "Настройки",
        icon: "gearshape.fill",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_mapping_matches_original() {
        assert_eq!(Gesture::ThumbsUp.command(), Some(Command::Confirm));
        assert_eq!(Gesture::OpenPalm.command(), Some(Command::Pause));
        assert_eq!(Gesture::Pointing.command(), Some(Command::Select));
        assert_eq!(Gesture::SwipeRight.command(), Some(Command::NextScreen));
        assert_eq!(Gesture::SwipeLeft.command(), Some(Command::PreviousScreen));
        assert_eq!(Gesture::SwipeUp.command(), Some(Command::Increase));
        assert_eq!(Gesture::SwipeDown.command(), Some(Command::Decrease));
        // У остальных жестов команды нет.
        assert_eq!(Gesture::Fist.command(), None);
        assert_eq!(Gesture::Idle.command(), None);
    }

    #[test]
    fn slugs_round_trip() {
        for g in Gesture::ALL {
            assert_eq!(Gesture::from_slug(g.slug()), Some(g));
        }
        for m in AppMode::ALL {
            assert_eq!(AppMode::from_slug(m.slug()), Some(m));
        }
    }

    #[test]
    fn built_in_signs_have_no_word() {
        assert_eq!(Sign::BuiltIn(Gesture::Fist).word(), None);
        assert_eq!(Sign::None.word(), None);
        assert_eq!(Sign::custom("id-1", "привет").word(), Some("привет"));
    }
}
