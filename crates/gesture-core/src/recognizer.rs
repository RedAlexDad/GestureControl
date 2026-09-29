//! Конечный автомат распознавания жестов.
//!
//! Перенос `GestureRecognizer.swift`. Автомат не решает, *что* означает жест:
//! он лишь разбирает поток кадров на эпизоды и передаёт эпизод
//! классификатору, который уже знает про словарь пользователя.
//!
//! Завершившийся эпизод бывает трёх видов:
//!
//! * [`RecognizerMotion::Still`] — рука появилась и держится неподвижно;
//! * [`RecognizerMotion::Flick`] — рука быстро сместилась, это свайп;
//! * [`RecognizerMotion::Slow`] — рука двигалась медленно: жест показан «с историей».
//!
//! В режиме управления `Flick` превращается в свайп-жест, в режиме перевода —
//! в поиск по шаблонам движения. Различие живёт в замыкании-классификаторе,
//! поэтому автомат остаётся общим для обоих режимов.
//!
//! Все пороги заданы долями размера ладони: жест распознаётся одинаково
//! независимо от расстояния до камеры и размера руки.

use crate::geometry::{dist, HandGeometry, Point};
use crate::models::{Gesture, Sign};

/// Смещение центра в долях ладони, выше которого движение считается заметным.
/// Ниже — дрожание руки, выше порога жест уже считается начавшимся.
const STILL_TOLERANCE: f32 = 0.25;

/// Смещение в долях ладони, выше которого движение признаётся потерей цели:
/// ладонь сместилась больше, чем на две с половиной её ширины, — это не жест.
const MAX_MOTION: f32 = 2.5;

/// Минимальное смещение, продлевающее «размазанное» движение. Более мелкие
/// колебания внутри одного жеста не обнуляют окно бездействия.
const MIN_MOTION: f32 = 0.06;

/// Скорость в долях ладони за секунду, выше которой движение считается свайпом.
const FLICK_SPEED: f32 = 0.9;

/// Скорость, выше которой жест считается показанным с историей, а не свайпом.
const SLOW_SPEED: f32 = 0.15;

/// Защита от ложных свайпов в первые мгновения после появления руки: центр
/// смещается не из-за жеста, а из-за неточности детектора.
const APPEAR_GUARD: f32 = 0.2;

/// Сколько секунд рука должна стоять без движения, чтобы жест засчитался.
const STILL_TIME: f32 = 0.12;

/// Сколько секунд удержанный жест остаётся видимым после ухода руки.
const HOLD_TIME: f32 = 0.8;

/// Вид завершившегося движения — вход классификатора.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecognizerMotion {
    /// Рука неподвижна: ищем жест по позе.
    Still,
    /// Быстрое движение: свайп в указанную сторону.
    Flick(Gesture),
    /// Медленное движение: жест показан с историей, ищем по шаблонам динамики.
    Slow,
}

/// Результат обработки одного кадра.
///
/// * `event` — только что распознанный жест. Событие одно: интерфейсу
///   достаточно знать, что жест случился именно сейчас.
/// * `display` — что показывать пользователю. Отличается от `event` во время
///   удержания: жест уже сработал, но остаётся на экране.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GestureResult<S> {
    pub event: Option<S>,
    pub display: Option<S>,
}

impl<S> GestureResult<S> {
    /// Пустой результат: ни события, ни показа.
    pub fn nothing() -> Self {
        GestureResult {
            event: None,
            display: None,
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::joint;

    /// Синтетическая кисть: центр ладони ровно в `(x, y)`, размер ладони 60.
    ///
    /// Точки ладони подобраны симметрично, чтобы `HandGeometry::center`
    /// совпадал с запрошенной точкой, а `size` равнялся 60 — тогда ожидаемые
    /// значения порогов в тестах считаются в уме.
    fn hand_at(x: f32, y: f32) -> HandGeometry {
        let mut p = [Point::MISSING; joint::COUNT];
        p[joint::WRIST] = Point::new(x, y + 60.0);
        p[joint::INDEX_MCP] = Point::new(x - 30.0, y - 10.0);
        p[joint::MIDDLE_MCP] = Point::new(x, y);
        p[joint::RING_MCP] = Point::new(x + 30.0, y - 10.0);
        p[joint::LITTLE_MCP] = Point::new(x, y - 40.0);
        // Остальные точки не участвуют в расчёте автомата, но должны быть
        // валидны, иначе геометрия посчитает руку не найденной.
        for point in p[joint::THUMB_CMC..=joint::LITTLE_TIP].iter_mut() {
            if *point == Point::MISSING {
                *point = Point::new(x, y - 60.0);
            }
        }
        HandGeometry::new(&p).unwrap()
    }

    /// Классификатор, который всегда возвращает один и тот же жест.
    fn always(sign: Gesture) -> impl FnMut(&[HandGeometry], RecognizerMotion) -> Gesture {
        move |_, _| sign
    }

    /// Классификатор, который никогда ничего не узнаёт.
    fn idle(_: &[HandGeometry], _: RecognizerMotion) -> Gesture {
        Gesture::Idle
    }

    /// Показывает руку в точке и возвращает результат обработки кадра.
    fn at<S: Clone + PartialEq>(
        r: &mut GestureRecognizer<S>,
        time: f32,
        x: f32,
        y: f32,
        classify: &mut impl FnMut(&[HandGeometry], RecognizerMotion) -> S,
    ) -> GestureResult<S> {
        r.process(time, &[hand_at(x, y)], classify)
    }

    #[test]
    fn empty_frame_gives_nothing() {
        let mut r = BuiltInRecognizer::new(Gesture::Idle);
        assert_eq!(r.process(0.0, &[], idle), GestureResult::nothing());
    }

    #[test]
    fn still_hand_emits_after_pause() {
        let mut r = BuiltInRecognizer::new(Gesture::Idle);
        let mut classify = always(Gesture::ThumbsUp);

        // Появление: события нет, жест ещё не проверен.
        assert_eq!(at(&mut r, 0.0, 100.0, 100.0, &mut classify).event, None);

        // Покой короче STILL_TIME: рука ещё «ставится».
        assert_eq!(at(&mut r, 0.05, 100.0, 100.0, &mut classify).event, None);

        // Покой длиннее STILL_TIME: жест засчитан.
        let res = at(&mut r, 0.2, 100.0, 100.0, &mut classify);
        assert_eq!(res.event, Some(Gesture::ThumbsUp));
        assert_eq!(res.display, Some(Gesture::ThumbsUp));
    }

    #[test]
    fn hold_keeps_display_after_hand_left() {
        let mut r = BuiltInRecognizer::new(Gesture::Idle);
        let mut classify = always(Gesture::ThumbsUp);

        at(&mut r, 0.0, 100.0, 100.0, &mut classify);
        assert_eq!(
            at(&mut r, 0.2, 100.0, 100.0, &mut classify).event,
            Some(Gesture::ThumbsUp)
        );

        // Рука убрана, но жест ещё держится на экране.
        let res = r.process(0.4, &[], idle);
        assert_eq!(res.event, None);
        assert_eq!(res.display, Some(Gesture::ThumbsUp));

        // Дольше HOLD_TIME — исчезает.
        assert_eq!(r.process(2.0, &[], idle).display, None);
    }

    #[test]
    fn all_four_directions_detected() {
        // Единица смещения и ожидаемое направление свайпа.
        let cases = [
            (1.0_f32, 0.0_f32, Gesture::SwipeRight),
            (-1.0, 0.0, Gesture::SwipeLeft),
            (0.0, -1.0, Gesture::SwipeUp),
            (0.0, 1.0, Gesture::SwipeDown),
        ];
        for (dx, dy, expected) in cases {
            let mut r = BuiltInRecognizer::new(Gesture::Idle);
            let mut seen = None;
            let mut classify = |_: &[HandGeometry], motion: RecognizerMotion| {
                if let RecognizerMotion::Flick(d) = motion {
                    seen = Some(d);
                }
                Gesture::Idle
            };

            // 60 px — размер ладони: 40 px это 0.67 ладони, 100 px — 1.67.
            at(&mut r, 0.0, 200.0, 200.0, &mut classify);
            at(
                &mut r,
                0.1,
                200.0 + 40.0 * dx,
                200.0 + 40.0 * dy,
                &mut classify,
            );
            at(
                &mut r,
                0.3,
                200.0 + 100.0 * dx,
                200.0 + 100.0 * dy,
                &mut classify,
            );

            assert_eq!(seen, Some(expected), "сдвиг ({dx}, {dy})");
        }
    }

    #[test]
    fn flick_is_reported_to_classifier_with_direction() {
        let mut r = BuiltInRecognizer::new(Gesture::Idle);
        let mut seen: Vec<RecognizerMotion> = Vec::new();
        let mut classify = |_: &[HandGeometry], motion: RecognizerMotion| {
            seen.push(motion);
            Gesture::Idle
        };

        at(&mut r, 0.0, 100.0, 100.0, &mut classify);
        at(&mut r, 0.1, 140.0, 100.0, &mut classify);
        at(&mut r, 0.3, 200.0, 100.0, &mut classify);

        assert!(seen.contains(&RecognizerMotion::Flick(Gesture::SwipeRight)));
    }

    #[test]
    fn hold_direction_exposed_for_swipe() {
        let mut r = BuiltInRecognizer::new(Gesture::Idle);
        // Классификатор повторяет увиденное направление свайпа.
        let mut classify = |_: &[HandGeometry], motion: RecognizerMotion| match motion {
            RecognizerMotion::Flick(d) => d,
            _ => Gesture::Idle,
        };

        at(&mut r, 0.0, 100.0, 100.0, &mut classify);
        at(&mut r, 0.1, 140.0, 100.0, &mut classify);
        let res = at(&mut r, 0.3, 200.0, 100.0, &mut classify);

        assert_eq!(res.event, Some(Gesture::SwipeRight));
        assert_eq!(r.hold_direction(), Some(Gesture::SwipeRight));

        // Пока рука на месте, свайп продолжает удерживаться.
        let res = at(&mut r, 0.4, 200.0, 100.0, &mut classify);
        assert_eq!(res.event, None);
        assert_eq!(res.display, Some(Gesture::SwipeRight));
    }

    #[test]
    fn jitter_is_not_a_swipe() {
        let mut r = BuiltInRecognizer::new(Gesture::Idle);
        let mut flicked = false;
        let mut classify = |_: &[HandGeometry], motion: RecognizerMotion| {
            if matches!(motion, RecognizerMotion::Flick(_)) {
                flicked = true;
            }
            Gesture::Idle
        };

        // Дрожание на 2 px (0.03 ладони) каждые 50 мс: слишком медленно.
        let mut time = 0.0;
        for i in 0..20 {
            at(&mut r, time, 100.0 + (i % 2) as f32, 100.0, &mut classify);
            time += 0.05;
        }
        assert!(!flicked, "дрожание не должно считаться свайпом");
    }

    #[test]
    fn flick_guarded_right_after_appearance() {
        let mut r = BuiltInRecognizer::new(Gesture::Idle);
        let mut flicked = false;

        // Быстрое движение в первые 0.2 с — артефакт детектора, не жест.
        {
            let mut classify = |_: &[HandGeometry], motion: RecognizerMotion| {
                if matches!(motion, RecognizerMotion::Flick(_)) {
                    flicked = true;
                }
                Gesture::Idle
            };
            at(&mut r, 0.0, 100.0, 100.0, &mut classify);
            at(&mut r, 0.02, 150.0, 100.0, &mut classify);
            at(&mut r, 0.1, 200.0, 100.0, &mut classify);
        }
        assert!(!flicked, "первые 0.2 с после появления защищены");

        // После защиты тот же жест засчитывается.
        {
            let mut classify = |_: &[HandGeometry], motion: RecognizerMotion| {
                if matches!(motion, RecognizerMotion::Flick(_)) {
                    flicked = true;
                }
                Gesture::Idle
            };
            at(&mut r, 0.3, 280.0, 100.0, &mut classify);
        }
        assert!(flicked, "после APPEAR_GUARD свайп засчитывается");
    }

    #[test]
    fn slow_motion_reported_separately() {
        let mut r = BuiltInRecognizer::new(Gesture::Idle);
        let mut seen: Vec<RecognizerMotion> = Vec::new();
        let mut classify = |_: &[HandGeometry], motion: RecognizerMotion| {
            seen.push(motion);
            Gesture::Idle
        };

        // Сначала заметное движение переводит автомат в Flickering.
        at(&mut r, 0.0, 100.0, 100.0, &mut classify);
        at(&mut r, 0.1, 140.0, 100.0, &mut classify);
        // Затем медленное: 3 px за 0.3 с — скорость выше SLOW_SPEED, но
        // заметного движения нет, поэтому это не свайп.
        at(&mut r, 0.4, 143.0, 100.0, &mut classify);

        assert!(
            seen.contains(&RecognizerMotion::Slow),
            "медленное движение: {seen:?}"
        );
        assert!(
            !seen.iter().any(|m| matches!(m, RecognizerMotion::Flick(_))),
            "медленное движение не должно считаться свайпом: {seen:?}"
        );
    }

    #[test]
    fn marker_result_leaves_nothing_visible() {
        let mut r = BuiltInRecognizer::new(Gesture::Idle);
        let mut classify = idle;

        at(&mut r, 0.0, 100.0, 100.0, &mut classify);
        assert_eq!(
            at(&mut r, 0.3, 100.0, 100.0, &mut classify),
            GestureResult::nothing()
        );
    }

    #[test]
    fn unknown_gesture_retries_after_new_motion() {
        let mut r = BuiltInRecognizer::new(Gesture::Idle);
        // Сначала классификатор молчит, потом узнаёт жест.
        let mut call = 0;
        let mut classify = |_: &[HandGeometry], _: RecognizerMotion| {
            call += 1;
            if call < 2 {
                Gesture::Idle
            } else {
                Gesture::OpenPalm
            }
        };

        at(&mut r, 0.0, 100.0, 100.0, &mut classify);
        // Жест не узнан, но автомат не застрял и попробует ещё раз.
        assert_eq!(at(&mut r, 0.2, 100.0, 100.0, &mut classify).event, None);
        assert_eq!(
            at(&mut r, 0.5, 100.0, 100.0, &mut classify).event,
            Some(Gesture::OpenPalm)
        );
    }

    #[test]
    fn reset_clears_hold() {
        let mut r = BuiltInRecognizer::new(Gesture::Idle);
        let mut classify = always(Gesture::OpenPalm);

        at(&mut r, 0.0, 100.0, 100.0, &mut classify);
        at(&mut r, 0.3, 100.0, 100.0, &mut classify);
        assert_eq!(r.hold_direction(), None);

        r.reset();
        assert_eq!(r.process(0.4, &[], idle), GestureResult::nothing());
    }

    #[test]
    fn custom_signs_use_same_automaton() {
        let word = Sign::custom("id-1", "привет");
        let mut r = SignRecognizer::new(Sign::default());
        let mut classify = |_: &[HandGeometry], _: RecognizerMotion| word.clone();

        at(&mut r, 0.0, 100.0, 100.0, &mut classify);
        let res = at(&mut r, 0.3, 100.0, 100.0, &mut classify);
        assert_eq!(res.event, Some(word));
    }

    #[test]
    fn hold_keeps_custom_sign_visible() {
        let word = Sign::custom("id-1", "пока");
        let mut r = SignRecognizer::new(Sign::default());
        let mut classify = |_: &[HandGeometry], _: RecognizerMotion| word.clone();

        at(&mut r, 0.0, 100.0, 100.0, &mut classify);
        at(&mut r, 0.3, 100.0, 100.0, &mut classify);
        assert_eq!(r.process(0.4, &[], |_, _| Sign::None).display, Some(word));
    }

    #[test]
    fn two_hands_pick_rightmost() {
        let mut r = BuiltInRecognizer::new(Gesture::Idle);
        let mut classify = always(Gesture::Pointing);

        r.process(
            0.0,
            &[hand_at(100.0, 100.0), hand_at(400.0, 100.0)],
            &mut classify,
        );
        r.process(
            0.1,
            &[hand_at(100.0, 100.0), hand_at(440.0, 100.0)],
            &mut classify,
        );
        // Двигается только правая рука — она и должна определять жест.
        let res = r.process(
            0.3,
            &[hand_at(100.0, 100.0), hand_at(500.0, 100.0)],
            &mut classify,
        );
        assert_eq!(res.event, Some(Gesture::Pointing));
    }

    #[test]
    fn synthetic_hand_has_expected_geometry() {
        let hand = hand_at(100.0, 250.0);
        // size = |WRIST - MIDDLE_MCP| = 60.
        assert!((hand.size - 60.0).abs() < 0.01, "size: {}", hand.size);
        // center — среднее по пяти точкам ладони, симметрично относительно (x, y).
        assert!(
            (hand.center.x - 100.0).abs() < 0.01,
            "center.x: {}",
            hand.center.x
        );
        assert!(
            (hand.center.y - 250.0).abs() < 0.01,
            "center.y: {}",
            hand.center.y
        );
    }
}
