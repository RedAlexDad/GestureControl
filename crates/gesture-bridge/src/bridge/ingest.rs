//! Приём кадров от камеры.

use gesture_core::Point;

use crate::clock::Seconds;
use crate::state::FrameInput;

use super::{BridgeResult, GestureBridge, RejectedHand};

/// Допустимое расхождение меток времени между соседними кадрами.
///
/// Камера дрожит на доли кадра, и это нормально; расхождение в десятки
/// миллисекунд означает переупорядоченный кадр, а не дрожание.
const STALE_TOLERANCE: Seconds = 0.001;

impl GestureBridge {
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
}
