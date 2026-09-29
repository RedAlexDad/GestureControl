//! Запись примеров жестов в словарь.

use crate::features::HandFeatures;
use crate::geometry::HandGeometry;
use crate::models::{RecordingState, Sign};

use super::super::classify::average_frames;
use super::super::constants::{
    MIN_MOTION_FRAMES, RECORD_COUNTDOWN, RECORD_DURATION, RECORD_LEAD_IN,
};
use super::{GestureEngine, PendingRecording};

impl GestureEngine {
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

    /// Ведёт обратный отсчёт и запись. `true` — кадр занят записью.
    pub(super) fn advance_recording(&mut self, time: f32, hands: &[HandGeometry]) -> bool {
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
}
