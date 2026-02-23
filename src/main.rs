use std::env;
use std::fs::File;
use std::path::Path;
use std::collections::HashMap;
use std::io::{self, BufRead, Write};

fn parse_shell_config<P: AsRef<Path>>(path: P) -> io::Result<HashMap<String, String>> {
    let file = File::open(path)?;
    let reader = io::BufReader::new(file);
    let mut map = HashMap::new();

    let mut current_line = String::new();

    for line in reader.lines() {
        let l = line?;
        let trimmed = l.trim();

        // Пропускаем пустые строки
        if trimmed.is_empty() {
            continue;
        }

        // Пропускаем строки-комментарии, начинающиеся с '#'
        if trimmed.starts_with('#') {
            continue;
        }

        // Если строка заканчивается на \, убираем его и копим данные
        if trimmed.ends_with('\\') {
            current_line.push_str(&trimmed[..trimmed.len() - 1].trim());
            continue;
        } else {
            current_line.push_str(trimmed);
        }

        // Парсим накопленную строку вида key="value" или key=value
        if let Some(pos) = current_line.find('=') {
            let key = current_line[..pos].trim().to_string();
            let mut value = current_line[pos + 1..].trim().to_string();

            // Убираем кавычки, если они есть
            if (value.starts_with('"') && value.ends_with('"')) || 
               (value.starts_with('\'') && value.ends_with('\'')) {
                value.pop();
                value.remove(0);
            }

            map.insert(key, value);
        }

        current_line.clear();
    }

    Ok(map)
}

struct EngineData {
    id: String,
    name: String,
    description: String,
    prefix: String,
    profiles: Vec<HashMap<String, String>>,
}

/// Возвращает метаданные для engine по его id.
fn get_engine_metadata(id: &str) -> Option<(String, String, String)> {
    match id {
        "bhyve" => Some((
            "bhyve".to_string(),
            "Native FreeBSD hypervisor".to_string(),
            "b".to_string(),
        )),
        "xen" => Some((
            "xen".to_string(),
            "XEN type-1 hypervisor".to_string(),
            "x".to_string(),
        )),
        "qemu" => Some((
            "qemu".to_string(),
            "QEMU hypervisor".to_string(),
            "q".to_string(),
        )),
        _ => None,
    }
}

fn php_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Спецификация ключа из CIX_PROFILES_DATA: имя и флаг конвертации в байты.
struct KeySpec {
    /// Имя параметра для поиска в профиле и вывода в PHP
    name: String,
    /// Конвертировать значение в байты (human-readable -> bytes)
    convert_to_bytes: bool,
}

/// Парсит элемент CIX_PROFILES_DATA, например "imgsize:bytes" или "vm_profile".
fn parse_key_spec(s: &str) -> KeySpec {
    let s = s.trim();
    if let Some((name, modifier)) = s.split_once(':') {
        let name = name.trim();
        let convert_to_bytes = modifier.trim().eq_ignore_ascii_case("bytes");
        KeySpec {
            name: name.to_string(),
            convert_to_bytes,
        }
    } else {
        KeySpec {
            name: s.to_string(),
            convert_to_bytes: false,
        }
    }
}

/// Проверяет, что строка — целое число без постфиксов (g, m, t, k и т.п.).
fn is_plain_number(s: &str) -> bool {
    let s = s.trim();
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

/// Конвертирует human-readable значение (100m, 1g, 20t) в байты.
/// Если значение уже число без постфиксов — возвращает как есть.
/// Использует бинарные единицы (1024).
fn human_to_bytes(s: &str) -> String {
    let s = s.trim();
    if s.is_empty() {
        return s.to_string();
    }

    // Если число без постфиксов — уже в байтах
    if is_plain_number(s) {
        return s.to_string();
    }

    let mut it = s.chars().peekable();
    let mut num_str = String::new();
    while it.peek().map_or(false, |c| c.is_ascii_digit()) {
        if let Some(c) = it.next() {
            num_str.push(c);
        }
    }
    let num: u64 = match num_str.parse() {
        Ok(n) => n,
        Err(_) => return s.to_string(), // не удалось распарсить — оставляем как есть
    };

    let suffix = it.collect::<String>().to_lowercase();
    let factor: u64 = match suffix.as_str() {
        "k" => 1024,
        "m" => 1024_u64.pow(2),
        "g" => 1024_u64.pow(3),
        "t" => 1024_u64.pow(4),
        _ => return s.to_string(), // неизвестный постфикс — оставляем как есть
    };

    (num * factor).to_string()
}

fn main() {
    // Парсим аргументы командной строки
    let args: Vec<String> = env::args().collect();
    if args.len() < 5 {
        eprintln!("Usage: {} -c <engines> -o <output_path>", args[0]);
        eprintln!("Example: {} -c \"xen bhyve qemu\" -o out.php", args[0]);
        std::process::exit(1);
    }

    let mut capabilities: Option<String> = None;
    let mut output_path: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-c" => {
                if i + 1 < args.len() {
                    capabilities = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    eprintln!("Error: -c requires a value");
                    std::process::exit(1);
                }
            }
            "-o" => {
                if i + 1 < args.len() {
                    output_path = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    eprintln!("Error: -o requires a value");
                    std::process::exit(1);
                }
            }
            _ => {
                eprintln!("Unknown argument: {}", args[i]);
                std::process::exit(1);
            }
        }
    }

    let capabilities_str = match capabilities {
        Some(s) => s,
        None => {
            eprintln!("Error: -c (capabilities) is required");
            std::process::exit(1);
        }
    };

    let output_path = match output_path {
        Some(s) => s,
        None => {
            eprintln!("Error: -o (output path) is required");
            std::process::exit(1);
        }
    };

    // Парсим список engine из -c
    let engine_ids: Vec<String> = capabilities_str
        .split_whitespace()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if engine_ids.is_empty() {
        eprintln!("Error: No engines specified in -c");
        std::process::exit(1);
    }

    // Создаем список engines динамически на основе указанных в -c
    let mut engines: Vec<EngineData> = Vec::new();
    for engine_id in &engine_ids {
        match get_engine_metadata(engine_id) {
            Some((name, description, prefix)) => {
                engines.push(EngineData {
                    id: engine_id.clone(),
                    name,
                    description,
                    prefix,
                    profiles: Vec::new(),
                });
            }
            None => {
                eprintln!("Warning: Unknown engine '{}', skipping", engine_id);
            }
        }
    }

    if engines.is_empty() {
        eprintln!("Error: No valid engines found");
        std::process::exit(1);
    }

    // Получаем список ключей для извлечения из CIX_PROFILES_DATA
    let keys_env = match env::var("CIX_PROFILES_DATA") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("No such CIX_PROFILES_DATA");
            std::process::exit(1);
        }
    };

    let key_specs: Vec<KeySpec> = keys_env
        .split(',')
        .map(|s| parse_key_spec(s))
        .filter(|k| !k.name.is_empty())
        .collect();

    if key_specs.is_empty() {
        eprintln!("CIX_PROFILES_DATA is empty");
        std::process::exit(1);
    }

    // Опциональные переменные окружения: VM_CPUS_MAX, VM_CPUS_MIN, VM_RAM_MAX, VM_RAM_MIN, IMGSIZE_MAX, IMGSIZE_MIN
    let vm_cpus_max: Option<String> = env::var("VM_CPUS_MAX").ok().map(|v| v.trim().to_string()).filter(|s| !s.is_empty());
    let vm_cpus_min: Option<String> = env::var("VM_CPUS_MIN").ok().map(|v| v.trim().to_string()).filter(|s| !s.is_empty());

    let vm_ram_max: Option<String> = env::var("VM_RAM_MAX")
        .ok()
        .map(|v| human_to_bytes(v.trim()))
        .filter(|s| !s.is_empty());
    let vm_ram_min: Option<String> = env::var("VM_RAM_MIN")
        .ok()
        .map(|v| human_to_bytes(v.trim()))
        .filter(|s| !s.is_empty());
    let imgsize_max: Option<String> = env::var("IMGSIZE_MAX")
        .ok()
        .map(|v| human_to_bytes(v.trim()))
        .filter(|s| !s.is_empty());
    let imgsize_min: Option<String> = env::var("IMGSIZE_MIN")
        .ok()
        .map(|v| human_to_bytes(v.trim()))
        .filter(|s| !s.is_empty());


    // Загружаем профили из CIX_PROFILES и распределяем по гипервизорам
    let profiles_env = match env::var("CIX_PROFILES") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("No such CIX_PROFILES");
            std::process::exit(1);
        }
    };

    for profile_path in profiles_env.split_whitespace() {
        match parse_shell_config(profile_path) {
            Ok(config) => {
                for engine in &mut engines {
                    let active_key = format!("{}_active", engine.id);
                    if let Some(active) = config.get(&active_key) {
                        if active == "1" {
                            engine.profiles.push(config.clone());
                        }
                    }
                }
            }
            Err(_) => {
                eprintln!("Ошибка доступа к файлу: {}", profile_path);
            }
        }
    }

    // Генерируем PHP-файл конфигурации
    let mut file = match File::create(&output_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Cannot create output file {}: {}", &output_path, e);
            std::process::exit(1);
        }
    };

    if writeln!(file, "<?php").is_err() {
        std::process::exit(1);
    }
    if writeln!(file, "ClonOS::$engines=[").is_err() {
        std::process::exit(1);
    }

    let engines_len = engines.len();
    for (ei, engine) in engines.into_iter().enumerate() {
        if writeln!(file, "\t\"{}\"=>[", engine.id).is_err() {
            std::process::exit(1);
        }
        if writeln!(
            file,
            "\t\t\"name\"=>\"{}\",",
            php_escape(&engine.name)
        )
        .is_err()
        {
            std::process::exit(1);
        }
        if writeln!(
            file,
            "\t\t\"description\"=>\"{}\",",
            php_escape(&engine.description)
        )
        .is_err()
        {
            std::process::exit(1);
        }
        if writeln!(
            file,
            "\t\t\"prefix\"=>\"{}\",",
            php_escape(&engine.prefix)
        )
        .is_err()
        {
            std::process::exit(1);
        }

        // count: количество профилей в data
        let profiles_len = engine.profiles.len();
        if writeln!(
            file,
            "\t\t\"count\"=>\"{}\",",
            profiles_len
        )
        .is_err()
        {
            std::process::exit(1);
        }

        // Опциональные лимиты из переменных окружения
        if let Some(ref v) = vm_cpus_max {
            if writeln!(file, "\t\t\"vm_cpus_max\"=>\"{}\",", php_escape(v)).is_err() {
                std::process::exit(1);
            }
        }
        if let Some(ref v) = vm_cpus_min {
            if writeln!(file, "\t\t\"vm_cpus_min\"=>\"{}\",", php_escape(v)).is_err() {
                std::process::exit(1);
            }
        }
        if let Some(ref v) = vm_ram_max {
            if writeln!(file, "\t\t\"vm_ram_max\"=>\"{}\",", php_escape(v)).is_err() {
                std::process::exit(1);
            }
        }
        if let Some(ref v) = vm_ram_min {
            if writeln!(file, "\t\t\"vm_ram_min\"=>\"{}\",", php_escape(v)).is_err() {
                std::process::exit(1);
            }
        }
        if let Some(ref v) = imgsize_max {
            if writeln!(file, "\t\t\"imgsize_max\"=>\"{}\",", php_escape(v)).is_err() {
                std::process::exit(1);
            }
        }
        if let Some(ref v) = imgsize_min {
            if writeln!(file, "\t\t\"imgsize_min\"=>\"{}\",", php_escape(v)).is_err() {
                std::process::exit(1);
            }
        }

        if engine.profiles.is_empty() {
            if writeln!(file, "\t\t\"data\"=>[],").is_err() {
                std::process::exit(1);
            }
        } else {
            if writeln!(file, "\t\t\"data\"=>[").is_err() {
                std::process::exit(1);
            }

            for (pi, profile) in engine.profiles.iter().enumerate() {
                let profile_name = format!("p{}", pi);
                if write!(file, "\t\t\t\"{}\"=>[", profile_name).is_err() {
                    std::process::exit(1);
                }

                // Собираем все найденные параметры профиля
                let mut profile_params: Vec<(String, String)> = Vec::new();
                for spec in &key_specs {
                    if let Some(value) = profile.get(&spec.name) {
                        let out_value = if spec.convert_to_bytes {
                            human_to_bytes(value)
                        } else {
                            value.clone()
                        };
                        profile_params.push((spec.name.clone(), out_value));
                    }
                }

                // Сортируем параметры по имени в алфавитном порядке
                profile_params.sort_by(|a, b| a.0.cmp(&b.0));

                // Выводим отсортированные параметры
                let mut first = true;
                for (key, value) in &profile_params {
                    if !first {
                        if write!(file, ",").is_err() {
                            std::process::exit(1);
                        }
                    }
                    first = false;
                    if write!(
                        file,
                        "\"{}\"=>\"{}\"",
                        php_escape(key),
                        php_escape(value)
                    )
                    .is_err()
                    {
                        std::process::exit(1);
                    }
                }

                // запятая только между элементами
                if pi + 1 < profiles_len {
                    if writeln!(file, "],").is_err() {
                        std::process::exit(1);
                    }
                } else {
                    if writeln!(file, "]").is_err() {
                        std::process::exit(1);
                    }
                }
            }

            if writeln!(file, "\t\t],").is_err() {
                std::process::exit(1);
            }
        }

        // Запятая после последнего engine не нужна
        if ei + 1 < engines_len {
            if writeln!(file, "\t],").is_err() {
                std::process::exit(1);
            }
        } else {
            if writeln!(file, "\t]").is_err() {
                std::process::exit(1);
            }
        }
    }

    if writeln!(file, "];").is_err() {
        std::process::exit(1);
    }
}
