//! Запись жестов в словарь: обучение, прерывание, удаление.

use super::super::GestureEngine;
use super::helpers::{record_word, show_from, thumbs_up};
use crate::geometry::HandGeometry;
use crate::models::{AppMode, RecordingState};

/// Записывает жест в словарь и возвращает слово, которое он даёт.
fn record_and_match(word: &str, hand: &HandGeometry) -> String {
    let mut engine = GestureEngine::new();
    record_word(&mut engine, word, 0.0, hand);
    assert_eq!(engine.library().len(), 1, "жест должен попасть в словарь");

    engine.set_mode(AppMode::Translate);
    let (snapshot, _) = show_from(&mut engine, hand, 10.0);
    snapshot.phrase
}

#[test]
fn recorded_static_gesture_is_recognized() {
    assert_eq!(record_and_match("привет", &thumbs_up()), "ПРИВЕТ");
}

#[test]
fn two_different_words_stay_apart() {
    let mut engine = GestureEngine::new();
    let hand = thumbs_up();
    // Один жест записан под двумя словами — слов в словаре станет два,
    // иначе словарь склеил бы их в один.
    record_word(&mut engine, "да", 0.0, &hand);
    record_word(&mut engine, "нет", 10.0, &hand);
    assert_eq!(engine.library().len(), 2);
}

#[test]
fn recording_aborts_when_hand_leaves() {
    let mut engine = GestureEngine::new();
    let hand = thumbs_up();
    engine.begin_recording("слово", false, 0.0);
    engine.process_frame(1.0, std::slice::from_ref(&hand));
    engine.process_frame(4.0, std::slice::from_ref(&hand));
    assert!(matches!(engine.recording(), RecordingState::Recording(_)));

    engine.process_frame(4.5, &[]);
    assert_eq!(engine.recording(), RecordingState::Idle);
    assert_eq!(
        engine.library().len(),
        0,
        "прерванная запись не сохраняется"
    );
    assert!(engine.snapshot(&[]).notice.is_some());
}

#[test]
fn recording_waits_for_manual_stop() {
    let mut engine = GestureEngine::new();
    let hand = thumbs_up();
    engine.begin_recording("слово", false, 0.0);
    engine.process_frame(4.0, std::slice::from_ref(&hand));
    assert!(matches!(engine.recording(), RecordingState::Recording(_)));

    // Прошло больше старых полутора секунд — запись всё ещё идёт: её
    // останавливает пользователь, а не таймер.
    engine.process_frame(9.0, std::slice::from_ref(&hand));
    assert!(matches!(engine.recording(), RecordingState::Recording(_)));

    engine.stop_recording(9.5);
    assert_eq!(engine.recording(), RecordingState::Idle);
    assert_eq!(engine.library().len(), 1, "жест сохранён по кнопке");
}

#[test]
fn recording_without_hand_does_not_start() {
    let mut engine = GestureEngine::new();
    engine.begin_recording("слово", false, 0.0);
    engine.process_frame(1.0, &[]);
    engine.process_frame(4.0, &[]);
    assert_eq!(engine.recording(), RecordingState::Idle);
    assert_eq!(engine.library().len(), 0);
}

#[test]
fn empty_word_is_rejected() {
    let mut engine = GestureEngine::new();
    engine.begin_recording("   ", false, 0.0);
    assert_eq!(engine.recording(), RecordingState::Idle);
    assert!(engine.snapshot(&[]).notice.is_some());
}

#[test]
fn delete_removes_sign() {
    let mut engine = GestureEngine::new();
    record_word(&mut engine, "да", 0.0, &thumbs_up());
    assert_eq!(engine.library().len(), 1);

    engine.delete_sign(0);
    assert_eq!(engine.library().len(), 0);
}
