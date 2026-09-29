//! Классификатор жестов и свободные помощники движка.

use crate::geometry::HandGeometry;
use crate::library::SignLibrary;
use crate::models::{AppMode, Command, Sign};
use crate::recognizer::RecognizerMotion;

use super::constants::{DYNAMIC_THRESHOLD, STATIC_THRESHOLD};

/// Команда элемента демо-интерфейса.
pub(super) fn tap_command(index: usize) -> Option<Command> {
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
pub(super) fn classify(
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
pub(super) fn built_in(sign: &Sign) -> Option<crate::models::Gesture> {
    match sign {
        Sign::BuiltIn(gesture) => Some(*gesture),
        _ => None,
    }
}

/// Усредняет покадровые векторы, сгруппированные по числу рук.
///
/// Жест выполняется либо одной, либо двумя руками; смешанные кадры
/// отбрасываются, иначе среднее получилось бы бессмысленным.
pub(super) fn average_frames(frames: &[Vec<f32>]) -> Option<Vec<Vec<f32>>> {
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
