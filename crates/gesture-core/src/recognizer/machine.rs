//! Состояния и переходы конечного автомата.

use crate::geometry::{dist, HandGeometry, Point};
use crate::models::{Gesture, Sign};

use super::constants::{
    APPEAR_GUARD, FLICK_SPEED, HOLD_TIME, MAX_MOTION, MIN_MOTION, SLOW_SPEED, STILL_TIME,
    STILL_TOLERANCE,
};
use super::{GestureResult, RecognizerMotion};

/// Внутреннее состояние автомата.
#[derive(Debug, Clone, PartialEq)]
enum State<S> {
    /// Руки нет, ждём появления.
    Idle,
    /// Рука только появилась: движение ещё не началось, жест не проверен.
    Appeared {
        /// Момент последнего замеченного движения.
        last_motion: f32,
    },
    /// Видно движение, но оно пока не признано ни свайпом, ни завершённым
    /// жестом: пользователь «ведёт» руку.
    Flickering { last_motion: f32 },
    /// Жест распознан и удерживается, пока рука не ушла.
    Hold {
        /// Что показывать пользователю.
        sign: S,
        /// Направление свайпа: его выполняет интерфейс, пока рука не убрана.
        direction: Option<Gesture>,
        /// Момент распознавания — от него отсчитывается `HOLD_TIME`.
        started: f32,
    },
    /// Жест устарел: руки нет дольше `HOLD_TIME`.
    Stale,
}

/// Конечный автомат жестов, обобщённый на тип результата.
///
/// `S` — это `Gesture` в режиме управления и `Sign` в режиме перевода.
#[derive(Debug, Clone)]
pub struct GestureRecognizer<S> {
    state: State<S>,
    /// Значение, означающее «жеста нет».
    marker: S,
    /// Предыдущая геометрия ведущей руки: по ней считается движение.
    last_hand: Option<HandGeometry>,
    last_time: f32,
    /// Момент появления текущей руки.
    appearance: f32,
}

impl<S: Clone + PartialEq> GestureRecognizer<S> {
    /// Создаёт автомат. `marker` — значение, означающее «жеста нет».
    pub fn new(marker: S) -> Self {
        GestureRecognizer {
            state: State::Idle,
            marker,
            last_hand: None,
            last_time: 0.0,
            appearance: 0.0,
        }
    }

    /// Сбрасывает автомат: текущий жест теряется, рука считается исчезнувшей.
    pub fn reset(&mut self) {
        self.state = State::Idle;
        self.last_hand = None;
        self.last_time = 0.0;
        self.appearance = 0.0;
    }

    /// Направление удерживаемого свайпа, если автомат держит свайп.
    pub fn hold_direction(&self) -> Option<Gesture> {
        match &self.state {
            State::Hold { direction, .. } => *direction,
            _ => None,
        }
    }

    /// Обрабатывает очередной кадр.
    ///
    /// `classify` вызывается только в момент, когда движение признано
    /// жестом: пока рука просто шевелится, сравнение с шаблонами динамики
    /// не выполняется.
    pub fn process<F>(
        &mut self,
        time: f32,
        hands: &[HandGeometry],
        mut classify: F,
    ) -> GestureResult<S>
    where
        F: FnMut(&[HandGeometry], RecognizerMotion) -> S,
    {
        // Ведущей считаем самую правую руку: так жест остаётся стабильным,
        // когда в кадре видны обе кисти.
        match hands
            .iter()
            .max_by(|a, b| a.center.x.total_cmp(&b.center.x))
        {
            None => self.on_no_hand(time),
            Some(hand) => {
                let hand = hand.clone();
                let result = self.on_hand(time, &hand, hands, &mut classify);
                self.last_hand = Some(hand);
                self.last_time = time;
                result
            }
        }
    }

    /// Рук нет: удержание стареет, остальные состояния сбрасываются.
    fn on_no_hand(&mut self, time: f32) -> GestureResult<S> {
        self.last_hand = None;
        self.last_time = time;

        match &self.state {
            State::Hold { sign, started, .. } => {
                if time - started > HOLD_TIME {
                    self.state = State::Stale;
                    GestureResult::nothing()
                } else {
                    GestureResult {
                        event: None,
                        display: Some(sign.clone()),
                    }
                }
            }
            _ => {
                self.state = State::Idle;
                GestureResult::nothing()
            }
        }
    }

    /// Рука видна: разбираем движение.
    fn on_hand<F>(
        &mut self,
        time: f32,
        hand: &HandGeometry,
        hands: &[HandGeometry],
        classify: &mut F,
    ) -> GestureResult<S>
    where
        F: FnMut(&[HandGeometry], RecognizerMotion) -> S,
    {
        let previous = match &self.last_hand {
            None => {
                // Появление: запоминаем положение, но ещё ничего не судим.
                self.appearance = time;
                self.state = State::Appeared { last_motion: time };
                return GestureResult::nothing();
            }
            Some(previous) => previous,
        };

        let palm = hand.size.max(1.0);
        let motion = dist(previous.center, hand.center) / palm;
        let elapsed = (time - self.last_time).max(f32::EPSILON);
        let speed = motion / elapsed;

        match self.state.clone() {
            // Жест удерживается: рука может двигаться, но новых событий нет.
            State::Hold { sign, .. } => GestureResult {
                event: None,
                display: Some(sign),
            },

            State::Appeared { last_motion } => {
                if motion > STILL_TOLERANCE {
                    // Пользователь начал двигать рукой — ждём, что получится.
                    self.state = State::Flickering { last_motion: time };
                    GestureResult::nothing()
                } else if time - last_motion > STILL_TIME {
                    self.emit(time, classify(hands, RecognizerMotion::Still), None)
                } else {
                    GestureResult::nothing()
                }
            }

            State::Flickering { last_motion } => {
                let flick = time - self.appearance > APPEAR_GUARD
                    && motion > STILL_TOLERANCE
                    && motion < MAX_MOTION
                    && speed > FLICK_SPEED;
                if flick {
                    let direction = direction_of(previous.center, hand.center);
                    self.emit(
                        time,
                        classify(hands, RecognizerMotion::Flick(direction)),
                        Some(direction),
                    )
                } else if motion > MIN_MOTION {
                    // Жест ещё «живёт»: продлеваем окно бездействия.
                    self.state = State::Flickering { last_motion: time };
                    GestureResult::nothing()
                } else if speed > SLOW_SPEED && time - last_motion > STILL_TIME {
                    self.emit(time, classify(hands, RecognizerMotion::Slow), None)
                } else {
                    GestureResult::nothing()
                }
            }

            // Обработано в `on_no_hand` и при первом появлении.
            State::Idle | State::Stale => {
                self.appearance = time;
                self.state = State::Appeared { last_motion: time };
                GestureResult::nothing()
            }
        }
    }

    /// Запоминает распознанный жест и переводит автомат в удержание.
    ///
    /// Если классификатор не узнал жест, возвращаемся к ожиданию покоя:
    /// автомат попробует ещё раз, когда движение окончательно прекратится.
    fn emit(&mut self, time: f32, sign: S, direction: Option<Gesture>) -> GestureResult<S> {
        if sign == self.marker {
            self.state = State::Appeared { last_motion: time };
            return GestureResult::nothing();
        }
        self.state = State::Hold {
            sign: sign.clone(),
            direction,
            started: time,
        };
        GestureResult {
            event: Some(sign.clone()),
            display: Some(sign),
        }
    }
}

/// Направление свайпа по смещению центра ладони.
fn direction_of(from: Point, to: Point) -> Gesture {
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    if dx.abs() >= dy.abs() {
        if dx > 0.0 {
            Gesture::SwipeRight
        } else {
            Gesture::SwipeLeft
        }
    } else if dy > 0.0 {
        Gesture::SwipeDown
    } else {
        Gesture::SwipeUp
    }
}

/// Автомат для встроенных жестов управления.
pub type BuiltInRecognizer = GestureRecognizer<Gesture>;

/// Автомат для пользовательских жестов сурдоперевода.
pub type SignRecognizer = GestureRecognizer<Sign>;
