use super::*;
use crate::features::HandFeatures;
use crate::geometry::{HandGeometry, Point};
use crate::joint;
use crate::motion::MotionFeatures;
use uuid::Uuid;

/// Пустой словарь для теста.
fn empty_library() -> SignLibrary {
    SignLibrary::from_signs(Vec::new())
}

/// Кисть с заданным положением большого пальца.
fn hand(thumb_up: bool) -> HandGeometry {
    let mut p = [Point::MISSING; joint::COUNT];
    p[joint::WRIST] = Point::new(200.0, 300.0);
    p[joint::INDEX_MCP] = Point::new(175.0, 255.0);
    p[joint::MIDDLE_MCP] = Point::new(200.0, 245.0);
    p[joint::RING_MCP] = Point::new(225.0, 255.0);
    p[joint::LITTLE_MCP] = Point::new(248.0, 270.0);
    for point in p[joint::THUMB_CMC..=joint::LITTLE_TIP].iter_mut() {
        if *point == Point::MISSING {
            *point = Point::new(200.0, 220.0);
        }
    }
    // Большой палец: поднят или прижат к ладони.
    p[joint::THUMB_TIP] = Point::new(
        if thumb_up { 150.0 } else { 190.0 },
        if thumb_up { 200.0 } else { 265.0 },
    );
    HandGeometry::new(&p).unwrap()
}

/// Поза с заданным положением большого пальца: у одного слова
/// палец поднят, у другого опущен.
fn pose(thumb_up: bool) -> Vec<f32> {
    HandFeatures::hand_vector(&hand(thumb_up)).unwrap()
}

/// Последовательность кадров, смещающихся в заданном направлении.
fn stream(dx: f32, frames: usize) -> Vec<Vec<f32>> {
    (0..frames)
        .map(|i| MotionFeatures::frame(&pose(true), dx * i as f32, 0.0))
        .collect()
}

#[test]
fn add_creates_sign_and_merges_by_case_insensitive_word() {
    let mut lib = empty_library();
    let first = lib.add("Привет", vec![pose(true)]);
    assert_eq!(lib.len(), 1);

    let second = lib.add("привет", vec![pose(true)]);
    assert_eq!(first, second, "регистр не влияет на объединение");
    assert_eq!(lib.len(), 1);
    assert_eq!(lib.get(0).unwrap().samples.len(), 2);
}

#[test]
fn classify_pose_matches_learned_word() {
    let mut lib = empty_library();
    lib.add("да", vec![pose(true)]);
    lib.add("нет", vec![pose(false)]);

    assert_eq!(lib.classify_pose(&[hand(true)], 0.4).unwrap().word, "да");
    assert_eq!(lib.classify_pose(&[hand(false)], 0.4).unwrap().word, "нет");
}

#[test]
fn close_samples_of_one_word_are_not_ambiguous() {
    let mut lib = empty_library();
    // Два похожих примера одного жеста: до правки второй пример выглядел
    // как конкурирующее слово, и жест отвергался как неоднозначный.
    lib.add("да", vec![pose(true), pose(true)]);
    assert_eq!(lib.classify_pose(&[hand(true)], 0.4).unwrap().word, "да");
}

#[test]
fn classify_pose_rejects_unrelated_pose() {
    let mut lib = empty_library();
    lib.add("да", vec![pose(true)]);

    // Совершенно другая поза: пальцы сжаты в кулак и опущены к
    // запястью, большой палец лежит поперёк ладони. Отличается сразу
    // десяток точек, а не одна — иначе среднее расстояние по 20 точкам
    // осталось бы в пределах порога.
    let mut p = [Point::MISSING; joint::COUNT];
    p[joint::WRIST] = Point::new(200.0, 300.0);
    p[joint::INDEX_MCP] = Point::new(175.0, 255.0);
    p[joint::MIDDLE_MCP] = Point::new(200.0, 245.0);
    p[joint::RING_MCP] = Point::new(225.0, 255.0);
    p[joint::LITTLE_MCP] = Point::new(248.0, 270.0);
    p[joint::THUMB_CMC] = Point::new(183.0, 292.0);
    for (mcp, pip, dip, tip) in [
        (
            joint::INDEX_MCP,
            joint::INDEX_PIP,
            joint::INDEX_DIP,
            joint::INDEX_TIP,
        ),
        (
            joint::MIDDLE_MCP,
            joint::MIDDLE_PIP,
            joint::MIDDLE_DIP,
            joint::MIDDLE_TIP,
        ),
        (
            joint::RING_MCP,
            joint::RING_PIP,
            joint::RING_DIP,
            joint::RING_TIP,
        ),
        (
            joint::LITTLE_MCP,
            joint::LITTLE_PIP,
            joint::LITTLE_DIP,
            joint::LITTLE_TIP,
        ),
    ] {
        let base = p[mcp];
        p[pip] = Point::new(base.x - 3.0, base.y + 17.0);
        p[dip] = Point::new(base.x + 3.0, base.y + 30.0);
        p[tip] = Point::new(base.x + 9.0, base.y + 41.0);
    }
    p[joint::THUMB_MP] = Point::new(172.0, 288.0);
    p[joint::THUMB_IP] = Point::new(163.0, 278.0);
    p[joint::THUMB_TIP] = Point::new(155.0, 268.0);
    let hand = HandGeometry::new(&p).unwrap();

    assert!(
        lib.classify_pose(&[hand], 0.3).is_none(),
        "далёкая поза не должна узнаваться"
    );
}

#[test]
fn classify_motion_matches_direction() {
    let mut lib = empty_library();
    lib.add_sequence("вправо", stream(2.0, 14));
    lib.add_sequence("влево", stream(-2.0, 14));

    assert_eq!(
        lib.classify_motion(&stream(2.0, 14), 0.6).unwrap().word,
        "вправо"
    );
    assert_eq!(
        lib.classify_motion(&stream(-2.0, 14), 0.6).unwrap().word,
        "влево"
    );
}

#[test]
fn classify_motion_ignores_still_stream() {
    let mut lib = empty_library();
    lib.add_sequence("вправо", stream(2.0, 14));
    let still: Vec<Vec<f32>> = (0..14)
        .map(|_| MotionFeatures::frame(&pose(true), 0.0, 0.0))
        .collect();
    assert!(lib.classify_motion(&still, 0.6).is_none());
}

#[test]
fn delete_removes_sign() {
    let mut lib = empty_library();
    let id = lib.add("да", vec![pose(true)]);
    assert!(lib.delete_id(id).is_some());
    assert!(lib.is_empty());
    assert!(
        lib.delete(0).is_none(),
        "повторное удаление ничего не делает"
    );
}

#[test]
fn memory_store_round_trip() {
    let store = MemoryStore::default();
    let mut lib = empty_library();
    lib.add("да", vec![pose(true)]);
    lib.save(&store).unwrap();

    let restored = SignLibrary::load(&store);
    assert_eq!(restored.len(), 1);
    assert_eq!(restored.get(0).unwrap().word, "да");
}

#[test]
fn json_store_round_trip_preserves_id() {
    let dir = std::env::temp_dir().join(format!("gctrl-test-{}", Uuid::new_v4()));
    let path = dir.join("signs.json");
    let store = JsonFileStore::new(&path);

    // Отсутствующий файл — пустой словарь, а не ошибка.
    assert!(SignLibrary::load(&store).is_empty());

    let mut lib = empty_library();
    let id = lib.add("да", vec![pose(true)]);
    lib.add_sequence("вправо", stream(2.0, 12));
    lib.save(&store).unwrap();

    let restored = SignLibrary::load(&store);
    assert_eq!(restored.len(), 2);
    assert_eq!(restored.get(0).unwrap().id, id, "идентификатор сохраняется");
    assert_eq!(restored.get(1).unwrap().sequences.len(), 1);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn json_store_creates_missing_directory() {
    let dir = std::env::temp_dir().join(format!("gctrl-test-{}", Uuid::new_v4()));
    let store = JsonFileStore::new(dir.join("nested").join("signs.json"));
    let mut lib = empty_library();
    lib.add("да", vec![pose(true)]);
    lib.save(&store).expect("каталог создаётся автоматически");
    assert!(store.path().exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn hand_count_reported_from_samples() {
    let mut lib = empty_library();
    lib.add("да", vec![pose(true)]);
    assert_eq!(lib.get(0).unwrap().hand_count(), 1);
}

#[test]
fn stats_report_sample_counts() {
    let mut lib = empty_library();
    lib.add("да", vec![pose(true), pose(true)]);
    lib.add_sequence("вправо", stream(2.0, 12));
    assert_eq!(
        lib.stats(),
        vec![("да".to_string(), 2, 0), ("вправо".to_string(), 0, 1)]
    );
}
