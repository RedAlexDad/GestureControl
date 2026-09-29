//! Распознавание жестов, режимы, команды и снимок состояния.

use super::super::constants::{PARAMETER_MAX, PARAMETER_MIN};
use super::super::events::EngineEvent;
use super::super::GestureEngine;
use super::helpers::{hand_at, open_palm, record_word, shift, show, thumbs_up};
use crate::geometry::Point;
use crate::joint;
use crate::models::{AppMode, Command, Gesture, Sign};

#[test]
fn empty_frame_reports_idle() {
    let mut engine = GestureEngine::new();
    let (snapshot, event) = engine.process_frame(0.0, &[]);
    assert!(snapshot.sign.is_none());
    assert_eq!(snapshot.hands_visible, 0);
    assert!(event.is_empty());
}

#[test]
fn thumbs_up_in_control_confirms() {
    let mut engine = GestureEngine::new();
    let (snapshot, event) = show(&mut engine, &thumbs_up());
    assert_eq!(snapshot.sign, Sign::BuiltIn(Gesture::ThumbsUp));
    assert!(event.commands.contains(&Command::Confirm));
}

#[test]
fn swipe_moves_to_next_screen() {
    let mut engine = GestureEngine::new();
    let hand = thumbs_up();
    engine.process_frame(0.0, std::slice::from_ref(&hand));
    engine.process_frame(0.1, &[shift(&hand, 40.0, 0.0)]);
    let (snapshot, event) = engine.process_frame(0.3, &[shift(&hand, 100.0, 0.0)]);

    assert!(event.commands.contains(&Command::NextScreen));
    assert_eq!(snapshot.screen_index, 1);
}

#[test]
fn open_palm_pauses_recognition() {
    let mut engine = GestureEngine::new();
    let (_, event) = show(&mut engine, &open_palm());
    assert!(event.commands.contains(&Command::Pause));
    assert!(!engine.recognition_enabled());
}

#[test]
fn tap_executes_command_immediately() {
    let mut engine = GestureEngine::new();
    let mut event = EngineEvent::default();
    engine.tap_index(6, 0.0, &mut event);
    assert_eq!(event.commands, vec![Command::NextScreen]);
    assert_eq!(engine.snapshot(&[]).screen_index, 1);

    event = EngineEvent::default();
    engine.tap_index(99, 1.0, &mut event);
    assert!(event.is_empty());
}

#[test]
fn translate_ignores_built_in_gestures() {
    let mut engine = GestureEngine::new();
    engine.set_mode(AppMode::Translate);
    let (snapshot, event) = show(&mut engine, &thumbs_up());
    assert!(snapshot.phrase.is_empty());
    assert!(event.is_empty());
}

#[test]
fn words_speak_one_by_one() {
    let mut engine = GestureEngine::new();
    let hand = thumbs_up();
    record_word(&mut engine, "да", 0.0, &hand);
    engine.set_mode(AppMode::Translate);

    // Первое слово: озвучивается сразу.
    let mut spoken = Vec::new();
    let mut time = 10.0;
    for _ in 0..3 {
        let (_, event) = engine.process_frame(time, std::slice::from_ref(&hand));
        if let Some(word) = event.speak {
            spoken.push(word);
        }
        time += 0.1;
    }
    assert_eq!(spoken, vec!["да".to_string()]);
}

#[test]
fn speech_can_be_switched_off() {
    let mut engine = GestureEngine::new();
    engine.set_speech_enabled(false);
    assert!(!engine.speech_enabled());
    engine.set_speech_enabled(true);
    assert!(engine.speech_enabled());
}

#[test]
fn paused_recognition_ignores_gestures() {
    let mut engine = GestureEngine::new();
    engine.set_recognition_enabled(false);
    let (_, event) = show(&mut engine, &thumbs_up());
    assert!(event.is_empty());
    assert!(engine.snapshot(&[]).sign.is_none());
}

#[test]
fn mode_switch_resets_phrase() {
    let mut engine = GestureEngine::new();
    engine.set_mode(AppMode::Translate);
    assert_eq!(engine.mode(), AppMode::Translate);
    assert!(engine.snapshot(&[]).notice.is_some());

    engine.set_mode(AppMode::Control);
    assert_eq!(engine.mode(), AppMode::Control);
}

#[test]
fn increase_and_decrease_clamp_parameter() {
    let mut engine = GestureEngine::new();
    let mut event = EngineEvent::default();
    for _ in 0..50 {
        engine.tap_index(5, 0.0, &mut event);
    }
    assert!((engine.snapshot(&[]).parameter - PARAMETER_MAX).abs() < 1e-6);

    for _ in 0..50 {
        engine.tap_index(4, 0.0, &mut event);
    }
    let snapshot = engine.snapshot(&[]);
    assert!((snapshot.parameter - PARAMETER_MIN).abs() < 1e-6);
    assert!((snapshot.volume - 0.0).abs() < 1e-6);
}

#[test]
fn snapshot_carries_hands_for_overlay() {
    let mut engine = GestureEngine::new();
    let (snapshot, _) = engine.process_frame(0.0, &[thumbs_up(), hand_at(300.0, 300.0)]);
    assert_eq!(snapshot.hands_visible, 2);
    assert_eq!(snapshot.hands.len(), 2);
    assert_eq!(snapshot.hands[0].len(), joint::COUNT);
}

#[test]
fn broken_points_are_dropped() {
    let mut engine = GestureEngine::new();
    let broken = vec![Point::MISSING; 5];
    let (snapshot, _) = engine.process_points(0.0, &[broken]);
    assert_eq!(snapshot.hands_visible, 0);
}
