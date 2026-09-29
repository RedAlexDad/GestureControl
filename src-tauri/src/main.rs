// Точка входа приложения. Вся логика — в библиотеке, чтобы её можно было
// переиспользовать на мобильных платформах.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    gesture_control_lib::run()
}
