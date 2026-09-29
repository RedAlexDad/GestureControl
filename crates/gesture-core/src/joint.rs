//! Индексы 21 ключевой точки кисти.
//!
//! Порядок совпадает с порядком, в котором точки отдаёт детектор рук
//! (см. `gesture_vision`): запястье, затем по 4 точки на каждый палец
//! от основания к кончику.

/// Запястье.
pub const WRIST: usize = 0;

pub const THUMB_CMC: usize = 1;
pub const THUMB_MP: usize = 2;
pub const THUMB_IP: usize = 3;
pub const THUMB_TIP: usize = 4;

pub const INDEX_MCP: usize = 5;
pub const INDEX_PIP: usize = 6;
pub const INDEX_DIP: usize = 7;
pub const INDEX_TIP: usize = 8;

pub const MIDDLE_MCP: usize = 9;
pub const MIDDLE_PIP: usize = 10;
pub const MIDDLE_DIP: usize = 11;
pub const MIDDLE_TIP: usize = 12;

pub const RING_MCP: usize = 13;
pub const RING_PIP: usize = 14;
pub const RING_DIP: usize = 15;
pub const RING_TIP: usize = 16;

pub const LITTLE_MCP: usize = 17;
pub const LITTLE_PIP: usize = 18;
pub const LITTLE_DIP: usize = 19;
pub const LITTLE_TIP: usize = 20;

/// Число ключевых точек кисти.
pub const COUNT: usize = 21;

/// Пять точек, по которым считается размер и центр ладони.
pub const PALM: [usize; 5] = [WRIST, INDEX_MCP, MIDDLE_MCP, RING_MCP, LITTLE_MCP];

/// Точки, образующие скелет кисти: пары индексов для отрисовки и расчёта DTW.
pub const BONES: [(usize, usize); 21] = [
    (0, 1),
    (1, 2),
    (2, 3),
    (3, 4), // большой
    (0, 5),
    (5, 6),
    (6, 7),
    (7, 8), // указательный
    (9, 10),
    (10, 11),
    (11, 12), // средний
    (13, 14),
    (14, 15),
    (15, 16), // безымянный
    (0, 17),
    (17, 18),
    (18, 19),
    (19, 20), // мизинец
    (5, 9),
    (9, 13),
    (13, 17), // ладонь
];

/// Кончики пальцев — рисуются крупнее остальных точек.
pub const TIPS: [usize; 5] = [THUMB_TIP, INDEX_TIP, MIDDLE_TIP, RING_TIP, LITTLE_TIP];
