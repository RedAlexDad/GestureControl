//! Поиск и разбор файла `.env`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::{ENV_FILE, ENV_PATH_KEY, ENV_SEARCH_DEPTH};

/// Разобранный файл `.env`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct EnvFile {
    /// Значения по ключу.
    values: HashMap<String, String>,
    /// Откуда прочитали: попадает в журнал, чтобы было видно, какой файл
    /// победил, если настроек на диске несколько.
    path: Option<PathBuf>,
}

impl EnvFile {
    /// Ищет `.env` вверх от текущего каталога.
    pub fn discover() -> EnvFile {
        match Self::locate() {
            Some(path) => Self::load(&path).unwrap_or_default(),
            // Отсутствие файла — обычное дело: работать можно и на одних
            // умолчаниях, поэтому ошибка чтения не поднимается наружу.
            None => EnvFile::default(),
        }
    }

    /// Путь к файлу настроек или `None`, если файла нет.
    fn locate() -> Option<PathBuf> {
        if let Some(path) = std::env::var_os(ENV_PATH_KEY).map(PathBuf::from) {
            return path.is_file().then_some(path);
        }
        let mut dir = std::env::current_dir().ok()?;
        for _ in 0..=ENV_SEARCH_DEPTH {
            let candidate = dir.join(ENV_FILE);
            if candidate.is_file() {
                return Some(candidate);
            }
            if !dir.pop() {
                break;
            }
        }
        None
    }

    /// Читает файл настроек.
    pub fn load(path: &Path) -> std::io::Result<EnvFile> {
        let text = std::fs::read_to_string(path)?;
        let mut file = EnvFile::parse(&text);
        file.path = Some(path.to_path_buf());
        Ok(file)
    }

    /// Разбирает содержимое `.env` в пары ключ и значение.
    pub fn parse(text: &str) -> EnvFile {
        let mut values = HashMap::new();
        for (number, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            // `export КЛЮЧ=значение` встречается в привычных оболочках и
            // в файле выглядит ошибкой, хотя смысл тот же.
            let line = line.strip_prefix("export ").unwrap_or(line).trim();
            let Some((key, value)) = line.split_once('=') else {
                tracing::warn!(".env:{}: нет знака равенства, строка пропущена", number + 1);
                continue;
            };
            let key = key.trim();
            if key.is_empty() {
                tracing::warn!(".env:{}: пустой ключ, строка пропущена", number + 1);
                continue;
            }
            values.insert(key.to_string(), unquote(value.trim()));
        }
        EnvFile { values, path: None }
    }

    /// Значение по ключу, если файл его содержит.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    /// Путь, откуда прочитали файл.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
}

/// Снимает кавычки со значения, если они есть.
///
/// Кавычки нужны для значений с пробелами, поэтому проверяем именно их, а не
/// пытаемся угадать по содержимому.
fn unquote(value: &str) -> String {
    for quote in ['"', '\''] {
        if value.len() >= 2 && value.starts_with(quote) && value.ends_with(quote) {
            return value[1..value.len() - 1].to_string();
        }
    }
    value.to_string()
}
