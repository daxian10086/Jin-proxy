// 闁规亽鍔庨幃濠勭磽閹惧磭鎽犻柨娑欑濠€浼村捶閻楀牊鐎ù鐘哄煐鐎垫梹绋婇崨顓烆嚙 + 闁告劕鎳庨悺銊╁礉閻樼儵鍋撻悢绮瑰亾?

// 閻庣數鎳撶花?Python: jindx/cache.py



use std::collections::{HashMap, VecDeque};

use std::fs;

use std::path::PathBuf;

use std::sync::RwLock;

use std::time::{SystemTime, UNIX_EPOCH};



use lazy_static::lazy_static;

use log::{debug, info};

use serde::{Deserialize, Serialize};

use serde_json::{json, Value};



use crate::modules::config::CONFIG;

use crate::modules::stats;



pub type Source = &'static str; // "codex" or "claude"



// 闁冲厜鍋撻柍鍏夊亾 缂傚倹鎸搁悺銊╁级閿涘嫭锟?闁冲厜鍋撻柍鍏夊亾



#[derive(Debug, Clone, Serialize, Deserialize)]

struct CacheEntry {

    text: String,

    ts: u64,

}



// 闁冲厜鍋撻柍鍏夊亾 闁哄倸娲ｅ▎銏㈢磽閹惧磭鎽犻悹渚灠锟?闁冲厜鍋撻柍鍏夊亾



fn file_cache_dir() -> PathBuf {

    crate::modules::config::CONFIG_FILE_PATH

        .parent()

        .unwrap_or(std::path::Path::new("."))

        .join("reasoning_cache")

}



fn file_cache_path(source: Source, session_id: &str) -> PathBuf {

    file_cache_dir().join(format!("{}_{}.json", source, session_id))

}



fn now_ts() -> u64 {

    SystemTime::now()

        .duration_since(UNIX_EPOCH)

        .unwrap()

        .as_secs()

}



// 闁冲厜鍋撻柍鍏夊亾 闁哄倸娲ｅ▎銏㈢磽閹惧磭鎽犻悹鍥嚙锟?闁冲厜鍋撻柍鍏夊亾



fn cache_file_get(source: Source, session_id: &str, ttl: i64) -> Vec<String> {

    let path = file_cache_path(source, session_id);

    if !path.exists() {

        return vec![];

    }

    let entries: Vec<CacheEntry> = fs::read_to_string(&path)

        .ok()

        .and_then(|s| serde_json::from_str(&s).ok())

        .unwrap_or_default();



    let now = now_ts();

    let ttl_u = ttl as u64;

    let valid: Vec<CacheEntry> = entries

        .into_iter()

        .filter(|e| now - e.ts < ttl_u)

        .collect();



    if valid.is_empty() {

        let _ = fs::remove_file(&path);

        return vec![];

    }



    let _ = fs::write(&path, serde_json::to_string(&valid).unwrap_or_default());

    valid.into_iter().map(|e| e.text).collect()

}



fn cache_file_set(source: Source, session_id: &str, reasoning_text: &str, ttl: i64) {

    let path = file_cache_path(source, session_id);

    let _ = fs::create_dir_all(path.parent().unwrap());



    let mut entries: Vec<CacheEntry> = fs::read_to_string(&path)

        .ok()

        .and_then(|s| serde_json::from_str(&s).ok())

        .unwrap_or_default();



    entries.push(CacheEntry {

        text: reasoning_text.to_string(),

        ts: now_ts(),

    });



    while entries.len() > *crate::modules::config::REASONING_CACHE_MAX {

        entries.remove(0);

    }



    let _ = fs::write(&path, serde_json::to_string(&entries).unwrap_or_default());

}



// 闁冲厜鍋撻柍鍏夊亾 闁告劕鎳庨悺銊х磽閹惧磭锟?闁冲厜鍋撻柍鍏夊亾



lazy_static! {

    static ref MEMORY_CACHE: RwLock<HashMap<String, VecDeque<CacheEntry>>> = RwLock::new(HashMap::new());

}



fn full_key(source: Source, session_id: &str) -> String {

    format!("{}:{}", source, session_id)

}



fn cache_memory_get(full_key: &str, ttl: i64) -> Vec<String> {
    let cache = MEMORY_CACHE.read().unwrap();
    if let Some(entries) = cache.get(full_key) {
        let now = now_ts();
        let ttl_u = ttl as u64;
        let valid: Vec<String> = entries
            .iter()
            .filter(|e| now - e.ts < ttl_u)
            .map(|e| e.text.clone())
            .collect();
        if valid.is_empty() {
            return vec![];
        }
        return valid;
    }
    vec![]
}




fn cache_memory_set(full_key: &str, reasoning_text: &str) {

    let entry = CacheEntry {

        text: reasoning_text.to_string(),

        ts: now_ts(),

    };

    let mut cache = MEMORY_CACHE.write().unwrap();

    let entries = cache.entry(full_key.to_string()).or_default();

    entries.push_back(entry);

    while entries.len() > *crate::modules::config::REASONING_CACHE_MAX {

        entries.pop_front();

    }

    while cache.len() > 1000 {

        let first_key = cache.keys().next().cloned();

        if let Some(k) = first_key {

            cache.remove(&k);

        }

    }

}



// 闁冲厜鍋撻柍鍏夊亾 闁稿浚鍓欑槐?API 闁冲厜鍋撻柍鍏夊亾



pub fn get_cached_reasoning(source: Source, session_id: &str) -> Vec<String> {

    if !CONFIG.get_bool("enable_reasoning_cache") {

        return vec![];

    }

    let cache_ttl = CONFIG.get_int("reasoning_cache_ttl", 600);



    // 闁哄倸娲ｅ▎銏″濡搫锟?

    let result = cache_file_get(source, session_id, cache_ttl);

    stats::record_cache(!result.is_empty());

    if !result.is_empty() {

        return result;

    }



    // 闁告劕鎳庨悺銊╁礂濠婂啰锟?

    let result = cache_memory_get(&full_key(source, session_id), cache_ttl);

    stats::record_cache(!result.is_empty());

    result

}



pub fn cache_reasoning(source: Source, session_id: &str, reasoning_text: &str) {

    if reasoning_text.trim().is_empty() {

        return;

    }

    if !CONFIG.get_bool("enable_reasoning_cache") {

        return;

    }



    let cache_ttl = CONFIG.get_int("reasoning_cache_ttl", 600);



    // 闁哄倸娲ｅ▎銏ゅ箰娴ｉ鐣介柛?

    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {

        cache_file_set(source, session_id, reasoning_text, cache_ttl);

    }));



    // 闁告艾鏈鐐哄礃濞嗗繐鐓傞柛鎰噹閻°劑宕濋悩鐑╁亾閻旀椿鍤㈤柛?

    cache_memory_set(&full_key(source, session_id), reasoning_text);

}



pub fn get_memory_sessions_count() -> usize {

    MEMORY_CACHE.read().unwrap().len()

}



fn cleanup_expired_memory_entries() {

    let cache_ttl = CONFIG.get_int("reasoning_cache_ttl", 600) as u64;

    let now = now_ts();

    let mut cache = MEMORY_CACHE.write().unwrap();

    cache.retain(|_, entries| {

        entries.retain(|e| now - e.ts < cache_ttl);

        !entries.is_empty()

    });

}



// 闁冲厜鍋撻柍鍏夊亾 闁告艾楠歌ぐ鏉戙€掗崨顖涘€炲ù鐘侯嚙锟?闁冲厜鍋撻柍鍏夊亾



pub async fn memory_cache_cleanup_loop() {

    loop {

        tokio::time::sleep(tokio::time::Duration::from_secs(300)).await;

        cleanup_expired_memory_entries();

    }

}



// 闁冲厜鍋撻柍鍏夊亾 Redis 闁绘鍩栭埀顑跨劍閻擄紕锟?闁冲厜鍋撻柍鍏夊亾



pub fn is_redis_available() -> bool {

    false

}



pub fn get_redis_info() -> Value {

    json!({"status": "disabled", "fallback": "file"})

}



pub fn get_redis_session_count() -> usize {

    0

}



// 闁冲厜鍋撻柍鍏夊亾 缂傚倹鎸搁悺銊﹀緞瑜嶉惃顒佺┍閳╁啩锟?闁冲厜鍋撻柍鍏夊亾



pub fn get_cache_size_info() -> Value {

    let cache_dir = file_cache_dir();

    let mut file_count = 0u64;

    let mut total_size = 0u64;

    if cache_dir.exists() {

        if let Ok(entries) = fs::read_dir(&cache_dir) {

            for entry in entries.flatten() {

                if entry.path().extension().map(|e| e == "json").unwrap_or(false) {

                    file_count += 1;

                    if let Ok(meta) = entry.metadata() {

                        total_size += meta.len();

                    }

                }

            }

        }

    }



    let mem_count = MEMORY_CACHE.read().unwrap().len();



    fn fmt_size(size: u64) -> String {

        if size < 1024 {

            format!("{}B", size)

        } else if size < 1024 * 1024 {

            format!("{:.1}KB", size as f64 / 1024.0)

        } else {

            format!("{:.1}MB", size as f64 / 1024.0 / 1024.0)

        }

    }



    json!({

        "file_count": file_count,

        "file_size": total_size,

        "file_size_str": fmt_size(total_size),

        "memory_count": mem_count,

    })

}



pub fn clear_cache(source: &str) -> usize {

    let cache_dir = file_cache_dir();

    let mut deleted = 0usize;

    if cache_dir.exists() {

        if let Ok(entries) = fs::read_dir(&cache_dir) {

            for entry in entries.flatten() {

                let path = entry.path();

                if path.extension().map(|e| e == "json").unwrap_or(false) {

                    if source.is_empty()

                        || path

                            .file_stem()

                            .and_then(|s| s.to_str())

                            .map(|s| s.starts_with(&format!("{}_", source)))

                            .unwrap_or(false)

                    {

                        if fs::remove_file(&path).is_ok() {

                            deleted += 1;

                        }

                    }

                }

            }

        }

    }



    let mut cache = MEMORY_CACHE.write().unwrap();

    if source.is_empty() {

        cache.clear();

    } else {

        let prefix = format!("{}:", source);

        cache.retain(|k, _| !k.starts_with(&prefix));

    }



    info!("Cache cleared: {} files deleted, source={}", deleted, if source.is_empty() { "all" } else { source });

    deleted

}



