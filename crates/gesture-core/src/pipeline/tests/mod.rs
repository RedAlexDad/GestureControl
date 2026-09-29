//! Тесты движка: общие сценарии в `helpers`, распознавание и запись.

mod helpers;
mod recognition;
mod recording;

use super::{GestureEngine, SilentSpeaker};
use crate::library::MemoryStore;

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
