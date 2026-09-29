use gesture_core::joint;
use gesture_core::pipeline::{PointSer, RecordingStateSer};
use gesture_core::AppMode;

use super::*;
use crate::clock::ManualClock;
use crate::error::BridgeError;
use crate::state::FrameInput;
/// Прямоугольник ладони вокруг центра: 21 точка, размер 100.
fn hand(cx: f32, cy: f32) -> Vec<PointSer> {
    let x0 = cx - 50.0;
    let y0 = cy - 50.0;
    let mut points = vec![PointSer { x: x0, y: y0 }; joint::COUNT];
    // Запястье и костяшки — по краям, иначе размер ладони нулевой.
    points[joint::WRIST] = PointSer {
        x: x0,
        y: cy + 50.0,
    };
    points[joint::INDEX_MCP] = PointSer { x: x0, y: y0 };
    points[joint::MIDDLE_MCP] = PointSer {
        x: x0 + 25.0,
        y: y0,
    };
    points[joint::RING_MCP] = PointSer {
        x: x0 + 50.0,
        y: y0,
    };
    points[joint::LITTLE_MCP] = PointSer {
        x: x0 + 50.0,
        y: y0 + 25.0,
    };
    points
}

fn bridge() -> (GestureBridge, Arc<ManualClock>) {
    let clock = Arc::new(ManualClock::new());
    (GestureBridge::with_clock(clock.clone()), clock)
}

fn temp_path(tag: &str) -> PathBuf {
    let unique = uuid::Uuid::new_v4();
    std::env::temp_dir().join(format!("gesture-bridge-{tag}-{unique}.json"))
}

// ------------------------------------------------------------- кадры

#[test]
fn empty_frame_ingests_without_events() {
    let (mut bridge, _) = bridge();
    let result = bridge.ingest(&FrameInput::empty(0.0));
    assert!(result.event.is_empty());
    assert!(result.rejected.is_empty());
    assert!(!result.stale);
    assert_eq!(result.state.engine.hands_visible, 0);
    assert_eq!(bridge.metrics().frames_ingested, 1);
}

#[test]
fn good_hand_reaches_engine_and_shows_in_snapshot() {
    let (mut bridge, _) = bridge();
    let result = bridge.ingest(&FrameInput::with_hand(0.0, hand(320.0, 240.0)));
    assert!(result.rejected.is_empty());
    assert_eq!(result.state.engine.hands_visible, 1);
    assert_eq!(result.state.engine.hands[0].len(), joint::COUNT);
}

#[test]
fn broken_hand_is_dropped_but_frame_survives() {
    let (mut bridge, _) = bridge();
    let frame = FrameInput {
        time: 0.0,
        hands: vec![vec![PointSer { x: 1.0, y: 1.0 }; 5], hand(320.0, 240.0)],
    };
    let result = bridge.ingest(&frame);
    assert_eq!(result.rejected.len(), 1);
    assert_eq!(result.rejected[0].hand, 0);
    assert_eq!(result.rejected[0].received, 5);
    assert_eq!(result.rejected[0].expected, 21);
    // Вторая кисть обработана: битая первая не остановила кадр.
    assert_eq!(result.state.engine.hands_visible, 1);
    assert_eq!(bridge.metrics().hands_rejected, 1);
}

#[test]
fn all_broken_hands_leave_frame_empty() {
    let (mut bridge, _) = bridge();
    let frame = FrameInput {
        time: 0.0,
        hands: vec![vec![PointSer { x: 0.0, y: 0.0 }; 4]],
    };
    let result = bridge.ingest(&frame);
    assert_eq!(result.rejected.len(), 1);
    assert_eq!(result.state.engine.hands_visible, 0);
    // Кадр всё равно учтён: поток жив, просто в кадре ничего нет.
    assert_eq!(bridge.metrics().frames_ingested, 1);
}

#[test]
fn stale_frame_is_skipped_and_counted() {
    let (mut bridge, _) = bridge();
    bridge.ingest(&FrameInput::with_hand(1.0, hand(320.0, 240.0)));
    let result = bridge.ingest(&FrameInput::with_hand(0.5, hand(400.0, 240.0)));
    assert!(result.stale);
    assert_eq!(bridge.metrics().frames_ingested, 1);
    assert_eq!(bridge.metrics().frames_stale, 1);
}

#[test]
fn non_finite_time_is_treated_as_stale() {
    let (mut bridge, _) = bridge();
    let frame = FrameInput::with_hand(f32::NAN, hand(320.0, 240.0));
    assert!(bridge.ingest(&frame).stale);
    let frame = FrameInput::with_hand(f32::INFINITY, hand(320.0, 240.0));
    assert!(bridge.ingest(&frame).stale);
    assert_eq!(bridge.metrics().frames_ingested, 0);
}

#[test]
fn tiny_time_jitter_is_accepted() {
    let (mut bridge, _) = bridge();
    bridge.ingest(&FrameInput::with_hand(1.0, hand(320.0, 240.0)));
    // Камера дрожит на доли кадра — это не переупорядоченный кадр.
    let result = bridge.ingest(&FrameInput::with_hand(0.999, hand(320.0, 240.0)));
    assert!(!result.stale);
    assert_eq!(bridge.metrics().frames_stale, 0);
}

#[test]
fn two_hands_are_ingested_together() {
    let (mut bridge, _) = bridge();
    let frame = FrameInput {
        time: 0.0,
        hands: vec![hand(220.0, 240.0), hand(420.0, 240.0)],
    };
    let result = bridge.ingest(&frame);
    assert_eq!(result.state.engine.hands_visible, 2);
}

// ------------------------------------------------------------- команды

#[test]
fn set_mode_switches_and_is_reflected_in_state() {
    let (mut bridge, _) = bridge();
    assert_eq!(bridge.state().mode, AppMode::Control);
    let state = bridge.set_mode(AppMode::Translate);
    assert_eq!(state.mode, AppMode::Translate);
    assert_eq!(state.mode_title, "Перевод");
}

#[test]
fn toggles_flip_flags_once_each() {
    let (mut bridge, _) = bridge();
    assert!(bridge.state().engine.recognition_enabled);
    assert!(!bridge.toggle_recognition().engine.recognition_enabled);
    assert!(bridge.toggle_recognition().engine.recognition_enabled);
    assert!(!bridge.toggle_speech().engine.speech_enabled);
    assert!(bridge.toggle_speech().engine.speech_enabled);
}

#[test]
fn tap_out_of_range_is_harmless() {
    let (mut bridge, _) = bridge();
    let result = bridge.tap(999);
    assert!(result.event.commands.is_empty());
}

#[test]
fn tap_does_not_move_frame_timeline_to_window_clock() {
    // Часы окна далеко впереди шкалы кадров: команда не должна подменять
    // шкалу кадров, иначе следующие кадры станут «устаревшими».
    let clock = Arc::new(ManualClock::starting_at(100.0));
    let mut bridge = GestureBridge::with_clock(clock);
    bridge.ingest(&FrameInput::with_hand(5.0, hand(320.0, 240.0)));
    bridge.tap(0);
    let result = bridge.ingest(&FrameInput::with_hand(5.5, hand(320.0, 240.0)));
    assert!(!result.stale, "кадр после команды должен быть разобран");
}

#[test]
fn start_recording_does_not_reject_later_frames() {
    // Корень проблемы «записать жест никак»: команда брала время у часов
    // окна, и кадры со шкалой кадров уходили в устаревшие.
    let clock = Arc::new(ManualClock::starting_at(100.0));
    let mut bridge = GestureBridge::with_clock(clock);
    bridge.ingest(&FrameInput::with_hand(1.0, hand(320.0, 240.0)));
    bridge.start_recording("да", false);
    let result = bridge.ingest(&FrameInput::with_hand(2.0, hand(320.0, 240.0)));
    assert!(!result.stale, "запись идёт по шкале кадров");
}

#[test]
fn finish_phrase_clears_phrase() {
    let (mut bridge, _) = bridge();
    let result = bridge.finish_phrase();
    assert!(result.state.engine.phrase.is_empty());
}

// ------------------------------------------------------------- словарь

#[test]
fn recording_starts_with_countdown() {
    let (mut bridge, _) = bridge();
    let state = bridge.start_recording("да", false);
    assert!(matches!(
        state.engine.recording,
        RecordingStateSer::Countdown(_)
    ));
}

#[test]
fn cancel_recording_returns_to_idle() {
    let (mut bridge, _) = bridge();
    bridge.start_recording("да", false);
    assert_eq!(
        bridge.cancel_recording().engine.recording,
        RecordingStateSer::Idle
    );
}

#[test]
fn delete_unknown_id_changes_nothing() {
    let (mut bridge, _) = bridge();
    let state = bridge.delete_sign("00000000-0000-0000-0000-000000000000");
    assert!(state.signs.is_empty());
    assert_eq!(state.engine.signs_count, 0);
}

#[test]
fn in_memory_bridge_has_no_store_path() {
    let (bridge, _) = bridge();
    assert!(bridge.store_path().is_none());
    let error = bridge.save().expect_err("нет пути");
    assert!(matches!(error, BridgeError::Unavailable(_)));
}

// --------------------------------------------------------- сохранение

#[test]
fn recorded_sign_survives_restart() {
    let path = temp_path("reload");
    let clock = Arc::new(ManualClock::new());
    let point = hand(320.0, 240.0);

    let recorded = {
        let mut bridge = GestureBridge::with_file_and_clock(path.clone(), clock.clone());
        bridge.start_recording("да", false);
        assert!(matches!(
            bridge.state().engine.recording,
            RecordingStateSer::Countdown(_)
        ));

        // Обратный отсчёт, затем сама запись и остановка кнопкой.
        for time in [1.0, 4.0, 5.0] {
            bridge.ingest(&FrameInput::with_hand(time, point.clone()));
        }
        bridge.stop_recording();
        assert_eq!(
            bridge.state().engine.recording,
            RecordingStateSer::Idle,
            "запись должна завершиться"
        );
        bridge.signs()
    };
    assert_eq!(recorded.len(), 1, "жест должен попасть в словарь");
    assert_eq!(recorded[0].word, "да");
    assert!(!recorded[0].is_dynamic, "поза, а не движение");

    // Перезапуск: новый мост читает тот же файл.
    let reloaded = GestureBridge::with_file_and_clock(path.clone(), clock);
    assert_eq!(reloaded.signs(), recorded, "словарь пережил перезапуск");
    assert_eq!(reloaded.state().engine.signs_count, 1);

    let _ = std::fs::remove_file(&path);
}

#[test]
fn delete_by_id_removes_exactly_that_sign() {
    let path = temp_path("delete");
    let clock = Arc::new(ManualClock::new());
    let point = hand(320.0, 240.0);
    let mut bridge = GestureBridge::with_file_and_clock(path.clone(), clock.clone());

    for (word, time) in [("да", 0.0), ("нет", 6.0), ("пока", 12.0)] {
        bridge.start_recording(word, false);
        for offset in [1.0, 4.0, 5.0] {
            bridge.ingest(&FrameInput::with_hand(time + offset, point.clone()));
        }
        bridge.stop_recording();
    }
    let signs = bridge.signs();
    assert_eq!(signs.len(), 3);

    // Удаляем средний: по позиции это был бы первый или второй.
    let middle = signs[1].id.clone();
    let state = bridge.delete_sign(&middle);
    let left: Vec<String> = state.signs.iter().map(|s| s.word.clone()).collect();
    assert_eq!(left, vec!["да", "пока"]);
    assert!(
        state.engine.notice.is_some(),
        "удаление должно быть объявлено"
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn clear_signs_empties_file_backed_library() {
    let path = temp_path("clear");
    let clock = Arc::new(ManualClock::new());
    let point = hand(320.0, 240.0);
    let mut bridge = GestureBridge::with_file_and_clock(path.clone(), clock.clone());
    bridge.start_recording("да", false);
    for offset in [1.0, 4.0, 5.0] {
        bridge.ingest(&FrameInput::with_hand(offset, point.clone()));
    }
    bridge.stop_recording();
    assert_eq!(bridge.signs().len(), 1);

    let state = bridge.clear_signs().expect("очистка словаря");
    assert!(state.signs.is_empty());

    let reloaded = GestureBridge::with_file_and_clock(path.clone(), clock);
    assert!(reloaded.signs().is_empty(), "очистка сохранена на диск");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn missing_file_is_an_empty_library_not_an_error() {
    let path = temp_path("missing");
    assert!(!path.exists());
    let bridge = GestureBridge::with_file(path.clone());
    assert!(bridge.signs().is_empty());
    assert_eq!(bridge.store_path(), Some(path.as_path()));
}

#[test]
fn clear_signs_on_empty_library_is_noop() {
    let path = temp_path("clear");
    let mut bridge = GestureBridge::with_file(path.clone());
    let state = bridge.clear_signs().expect("очистка пустого словаря");
    assert!(state.signs.is_empty());
    assert!(path.exists(), "пустой словарь сохранён на диск");
    let _ = std::fs::remove_file(&path);
}
