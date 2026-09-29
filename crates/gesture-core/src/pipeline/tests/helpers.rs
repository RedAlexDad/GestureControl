//! Общие кисти и сценарии для тестов движка.

use super::super::{EngineEvent, EngineSnapshot, GestureEngine};
use crate::geometry::{HandGeometry, Point};
use crate::joint;
use crate::models::RecordingState;

/// Кисть с центром ладони в `(x, y)`, размер ладони 60.
pub(super) fn hand_at(x: f32, y: f32) -> HandGeometry {
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
pub(super) fn thumbs_up() -> HandGeometry {
    let mut p = [Point::MISSING; joint::COUNT];
    p[joint::WRIST] = Point::new(100.0, 200.0);
    p[joint::INDEX_MCP] = Point::new(80.0, 150.0);
    p[joint::MIDDLE_MCP] = Point::new(100.0, 140.0);
    p[joint::RING_MCP] = Point::new(120.0, 150.0);
    p[joint::LITTLE_MCP] = Point::new(135.0, 165.0);
    for (mcp, pip, dip, tip) in FINGERS {
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

/// Раскладка «открытая ладонь» для проверки жеста паузы.
pub(super) fn open_palm() -> HandGeometry {
    let mut p = [Point::MISSING; joint::COUNT];
    p[joint::WRIST] = Point::new(100.0, 200.0);
    p[joint::INDEX_MCP] = Point::new(75.0, 155.0);
    p[joint::MIDDLE_MCP] = Point::new(100.0, 145.0);
    p[joint::RING_MCP] = Point::new(125.0, 155.0);
    p[joint::LITTLE_MCP] = Point::new(148.0, 170.0);
    for (mcp, pip, dip, tip) in FINGERS {
        let base = p[mcp];
        p[pip] = Point::new(base.x, base.y - 15.0);
        p[dip] = Point::new(base.x, base.y - 25.0);
        p[tip] = Point::new(base.x, base.y - 35.0);
    }
    p[joint::THUMB_CMC] = Point::new(80.0, 195.0);
    p[joint::THUMB_MP] = Point::new(62.0, 190.0);
    p[joint::THUMB_IP] = Point::new(48.0, 183.0);
    p[joint::THUMB_TIP] = Point::new(38.0, 175.0);
    HandGeometry::new(&p).unwrap()
}

/// Суставы четырёх пальцев: проксимальный, средний, дистальный, концевой.
const FINGERS: [(usize, usize, usize, usize); 4] = [
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
];

/// Кисть, смещённая на `dx`, с сохранением позы.
pub(super) fn shift(hand: &HandGeometry, dx: f32, dy: f32) -> HandGeometry {
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
pub(super) fn show_from(
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
pub(super) fn show(
    engine: &mut GestureEngine,
    hand: &HandGeometry,
) -> (EngineSnapshot, EngineEvent) {
    show_from(engine, hand, 0.0)
}

/// Прогоняет запись слова до конца: обратный отсчёт, сбор кадров и
/// сохранение в словарь.
pub(super) fn record_word(engine: &mut GestureEngine, word: &str, start: f32, hand: &HandGeometry) {
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
