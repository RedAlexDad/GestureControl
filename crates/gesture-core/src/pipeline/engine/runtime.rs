//! Обработка кадров: распознавание, команды, озвучка и снимок состояния.

use crate::features::HandFeatures;
use crate::geometry::{HandGeometry, Point};
use crate::models::{AppMode, Command, Sign, DEMO_SCREENS};
use crate::motion::MotionFeatures;
use crate::recognizer::GestureResult;

use super::super::classify::{built_in, classify, tap_command};
use super::super::constants::{
    ASSUMED_FPS, COMMAND_GAP, MOTION_BUFFER, PARAMETER_MAX, PARAMETER_MIN, PARAMETER_STEP,
    SPEAK_GAP, STREAM_WINDOW,
};
use super::super::events::{EngineEvent, EngineSnapshot};
use super::GestureEngine;

impl GestureEngine {
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
