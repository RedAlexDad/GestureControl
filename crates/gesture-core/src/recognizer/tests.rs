use super::*;
use crate::geometry::{HandGeometry, Point};
use crate::joint;
use crate::models::Sign;

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
