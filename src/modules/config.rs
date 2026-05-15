// 杩愯鏃堕厤缃鐞嗭紝绾跨▼瀹夊叏銆?

// 瀵瑰簲 Python: jindx/config.py



use std::collections::HashMap;

use std::fs;

use std::io::Write;

use std::path::{Path, PathBuf};

use std::sync::RwLock;



use lazy_static::lazy_static;

use log::{debug, error, info, warn};

use serde::{Deserialize, Serialize};

use serde_json::Value;



// 鈹€鈹€ 骞冲彴榛樿閰嶇疆璺緞 鈹€鈹€



fn default_config_path() -> PathBuf {

    if let Ok(p) = std::env::var("PROXY_CONFIG_FILE") {

        return PathBuf::from(p);

    }

    if cfg!(windows) {

        if let Ok(appdata) = std::env::var("APPDATA") {

            return PathBuf::from(appdata).join("proxy-config.json");

        }

        return dirs_next().unwrap_or_else(|| PathBuf::from(".")).join("proxy-config.json");

    }

    if cfg!(target_os = "macos") {

        return dirs_next()

            .unwrap_or_else(|| PathBuf::from("."))

            .join("Library")

            .join("Application Support")

            .join("proxy-config.json");

    }

    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {

        return PathBuf::from(xdg).join("proxy-config.json");

    }

    dirs_next()

        .unwrap_or_else(|| PathBuf::from("."))

        .join(".config")

        .join("proxy-config.json")

}



fn dirs_next() -> Option<PathBuf> {

    if let Ok(home) = std::env::var("HOME") {

        return Some(PathBuf::from(home));

    }

    if let Ok(userprofile) = std::env::var("USERPROFILE") {

        return Some(PathBuf::from(userprofile));

    }

    None

}



// 鈹€鈹€ 鐜鍙橀噺榛樿鍊?鈹€鈹€



fn env_str(key: &str, default: &str) -> String {

    std::env::var(key).unwrap_or_else(|_| default.to_string())

}



fn env_int(key: &str, default: i64) -> i64 {

    std::env::var(key)

        .ok()

        .and_then(|v| v.parse().ok())

        .unwrap_or(default)

}



// 鈹€鈹€ 鍏紑閰嶇疆甯搁噺 鈹€鈹€



lazy_static! {

    pub static ref DEEPSEEK_BASE: String = env_str("DEEPSEEK_BASE", "https://api.deepseek.com");

    pub static ref DEEPSEEK_KEY: String = env_str("DEEPSEEK_KEY", "sk-your-deepseek-api-key");

    pub static ref PROXY_PORT: u16 = env_int("PROXY_PORT", 8080) as u16;

    pub static ref CONNECT_PORT: u16 = env_int("CONNECT_PORT", 8443) as u16;

    pub static ref DEFAULT_MODEL: String = env_str("DEFAULT_MODEL", "deepseek-v4-pro");

    pub static ref TLS_PORT: u16 = env_int("TLS_PORT", 8444) as u16;

    pub static ref ADMIN_PORT: u16 = env_int("ADMIN_PORT", 8090) as u16;

    pub static ref REASONING_CACHE_MAX: usize = env_int("REASONING_CACHE_MAX", 10) as usize;

    pub static ref REASONING_CACHE_TTL: i64 = env_int("REASONING_CACHE_TTL", 600);

    pub static ref DEFAULT_REASONING_EFFORT: Option<String> = std::env::var("DEFAULT_REASONING_EFFORT").ok();

    pub static ref MAX_POSITION_EMBEDDINGS: i64 = env_int("MAX_POSITION_EMBEDDINGS", 1000000);

    pub static ref TOOL_USE_ENFORCEMENT: String = env_str(

        "TOOL_USE_ENFORCEMENT",

        "You MUST use the provided tools to accomplish the user's task. Never respond with just text explaining what you would do 鈥?actually call the tools. If tools are available, use them to take real actions: run commands, read/write files, search the web. Do NOT ask the user for confirmation before using tools. Just do it.",

    );

    pub static ref CONFIG_FILE_PATH: PathBuf = default_config_path();

}



// 鈹€鈹€ TLS 璇佷功璺緞 鈹€鈹€



lazy_static! {

    pub static ref CERT_DIR: PathBuf = {

        // 褰撴墦鍖呬负 exe 鏃讹紝璇佷功鐩綍鏀惧湪 exe 鍚岀洰褰曚笅

        if let Ok(exe) = std::env::current_exe() {

            let exe_dir = exe.parent().unwrap_or(Path::new("."));

            let cert_dir = exe_dir.join("certs");

            if cert_dir.exists() {

                return cert_dir;

            }

        }

        // 鍚﹀垯浠庨」鐩牴鐩綍鐨?certs/ 璇诲彇

        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("certs")

    };

    pub static ref CERT_FILE: PathBuf = CERT_DIR.join("tls.crt");

    pub static ref KEY_FILE: PathBuf = CERT_DIR.join("tls.key");

}



// 鈹€鈹€ 榛樿閰嶇疆 鈹€鈹€



fn default_config_map() -> HashMap<String, Value> {

    let mut m = HashMap::new();

    m.insert("deepseek_key".into(), Value::String(DEEPSEEK_KEY.clone()));

    m.insert("deepseek_base".into(), Value::String(DEEPSEEK_BASE.clone()));

    m.insert("default_model".into(), Value::String(DEFAULT_MODEL.clone()));

    m.insert("model_mapping".into(), serde_json::json!({"gpt-5.5": "deepseek-v4-pro", "gpt-5": "deepseek-v4-pro"}));

    m.insert("reasoning_effort".into(), Value::Null);

    m.insert("max_position_embeddings".into(), serde_json::json!(*MAX_POSITION_EMBEDDINGS));

    m.insert("max_output_tokens".into(), serde_json::json!(16384));

    m.insert("temperature".into(), Value::Null);

    m.insert("top_p".into(), Value::Null);

    m.insert("tool_use_enforcement".into(), serde_json::json!(true));

    m.insert("tool_use_prompt".into(), Value::String(TOOL_USE_ENFORCEMENT.clone()));

    m.insert("web_fetch_max_urls".into(), serde_json::json!(5));

    m.insert("web_fetch_timeout".into(), serde_json::json!(10));

    m.insert("web_fetch_max_body".into(), serde_json::json!(80000));

    m.insert("enable_reasoning_cache".into(), serde_json::json!(true));

    m.insert("reasoning_cache_ttl".into(), serde_json::json!(600));

    // Claude

    m.insert("claude_deepseek_key".into(), Value::String("".into()));

    m.insert("claude_deepseek_base".into(), Value::String("https://api.deepseek.com".into()));

    m.insert("claude_default_model".into(), Value::String("deepseek-v4-pro".into()));

    m.insert("claude_reasoning_effort".into(), Value::Null);

    m.insert("claude_max_position_embeddings".into(), serde_json::json!(1000000));

    m.insert("claude_max_output_tokens".into(), serde_json::json!(16384));

    m.insert("claude_temperature".into(), Value::Null);

    m.insert("claude_top_p".into(), Value::Null);

    m.insert("claude_strip_thinking".into(), serde_json::json!(true));

    m.insert("claude_skip_dangerous_mode".into(), serde_json::json!(true));

    m.insert("claude_deepseek_thinking_enabled".into(), serde_json::json!(false));

    m

}



// ---- RuntimeConfig ----



pub struct RuntimeConfig {

    config: RwLock<HashMap<String, Value>>,

}



impl RuntimeConfig {

    pub fn new() -> Self {

        let cfg = Self::load_from_file();

        Self {

            config: RwLock::new(cfg),

        }

    }



    fn load_from_file() -> HashMap<String, Value> {

        let mut cfg = default_config_map();

        let path = CONFIG_FILE_PATH.as_path();

        if path.exists() {

            match fs::read_to_string(path) {

                Ok(text) => match serde_json::from_str::<HashMap<String, Value>>(&text) {

                    Ok(saved) => {

                        for (k, v) in saved {

                            cfg.insert(k, v);

                        }

                    }

                    Err(e) => warn!("Failed to parse config file: {}", e),

                },

                Err(e) => warn!("Failed to read config file: {}", e),

            }

        }

        cfg

    }



    fn save_to_file(cfg: &HashMap<String, Value>) {

        let path = CONFIG_FILE_PATH.as_path();

        if let Some(parent) = path.parent() {

            let _ = fs::create_dir_all(parent);

        }

        match serde_json::to_string_pretty(cfg) {

            Ok(json) => {

                if let Err(e) = fs::write(path, json) {

                    error!("Failed to save config: {}", e);

                }

            }

            Err(e) => error!("Failed to serialize config: {}", e),

        }

    }



    pub fn get_str(&self, key: &str) -> String {

        let cfg = self.config.read().unwrap();

        cfg.get(key)

            .and_then(|v| v.as_str())

            .map(|s| s.to_string())

            .unwrap_or_default()

    }



    pub fn get_bool(&self, key: &str) -> bool {

        let cfg = self.config.read().unwrap();

        cfg.get(key)

            .and_then(|v| v.as_bool())

            .unwrap_or(false)

    }



    pub fn get_int(&self, key: &str, default: i64) -> i64 {

        let cfg = self.config.read().unwrap();

        cfg.get(key)

            .and_then(|v| v.as_i64())

            .unwrap_or(default)

    }



    pub fn get_opt_str(&self, key: &str) -> Option<String> {

        let cfg = self.config.read().unwrap();

        cfg.get(key)

            .and_then(|v| v.as_str())

            .map(|s| s.to_string())

    }



    pub fn get_model_mapping(&self) -> HashMap<String, String> {

        let cfg = self.config.read().unwrap();

        cfg.get("model_mapping")

            .and_then(|v| v.as_object())

            .map(|obj| {

                obj.iter()

                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))

                    .collect()

            })

            .unwrap_or_default()

    }



    pub fn config_dict(&self) -> HashMap<String, Value> {

        self.config.read().unwrap().clone()

    }



    pub fn update(&self, updates: &HashMap<String, Value>) {

        let allowed: Vec<&str> = vec![

            "deepseek_key", "deepseek_base", "default_model",

            "model_mapping", "reasoning_effort", "max_position_embeddings",

            "max_output_tokens", "temperature", "top_p",

            "tool_use_enforcement", "tool_use_prompt",

            "web_fetch_max_urls", "web_fetch_timeout", "web_fetch_max_body",

            "enable_reasoning_cache", "reasoning_cache_ttl",

            "claude_deepseek_key", "claude_deepseek_base", "claude_default_model",

            "claude_reasoning_effort", "claude_max_position_embeddings",

            "claude_max_output_tokens", "claude_temperature", "claude_top_p",

            "claude_strip_thinking", "claude_skip_dangerous_mode",

            "claude_deepseek_thinking_enabled",

        ];



        {

            let mut cfg = self.config.write().unwrap();

            for key in &allowed {

                if let Some(val) = updates.get(*key) {

                    cfg.insert(key.to_string(), val.clone());

                }

            }

            Self::save_to_file(&cfg);

        }

        let filtered: HashMap<_, _> = updates.iter()

            .filter(|(k, _)| allowed.contains(&k.as_str()))

            .collect();

        info!("Config updated: {:?}", filtered);

    }



    pub fn reload(&self) {

        let new_cfg = Self::load_from_file();

        let mut cfg = self.config.write().unwrap();

        *cfg = new_cfg;

    }



    // Claude-specific helpers

    pub fn get_claude_str(&self, key: &str) -> String {

        let claude_key = format!("claude_{}", key);

        let val = self.get_str(&claude_key);

        if val.is_empty() {

            self.get_str(key)

        } else {

            val

        }

    }



    pub fn get_claude_bool(&self, key: &str) -> bool {

        let claude_key = format!("claude_{}", key);

        self.get_bool(&claude_key)

    }



    pub fn get_claude_opt_str(&self, key: &str) -> Option<String> {

        let claude_key = format!("claude_{}", key);

        self.get_opt_str(&claude_key).or_else(|| self.get_opt_str(key))

    }



    pub fn get_claude_int(&self, key: &str, default: i64) -> i64 {

        let claude_key = format!("claude_{}", key);

        let val = self.get_int(&claude_key, -1);

        if val == -1 {

            self.get_int(key, default)

        } else {

            val

        }

    }

}



// 鈹€鈹€ 鍏ㄥ眬鍗曚緥 鈹€鈹€



lazy_static! {

    pub static ref CONFIG: RuntimeConfig = RuntimeConfig::new();

}



// 鈹€鈹€ Codex 閰嶇疆鑷姩鐢熸垚 鈹€鈹€



const CODEX_CONFIG_TOML: &str = r#"# 姝ゆ枃浠剁敱 JinDx Proxy 鍚姩鏃惰嚜鍔ㄧ敓鎴愶紝璇峰嬁鎵嬪姩缂栬緫銆?

# 濡傞渶淇敼妯″瀷鎴栧弬鏁帮紝璇烽€氳繃绠＄悊闈㈡澘 http://127.0.0.1:{admin_port} 鎿嶄綔銆?



model = "gpt-5.5"

model_reasoning_effort = "xhigh"

model_provider = "openai_http"



[model_providers.openai_http]

name = "JinDx Proxy (DeepSeek)"

wire_api = "responses"

requires_openai_auth = false

supports_websockets = true

base_url = "http://127.0.0.1:{proxy_port}"



{projects_section}



[tui.model_availability_nux]

"gpt-5.5" = 4



[features]

terminal_resize_reflow = true

"#;




pub fn write_codex_config_toml(_force: bool) {
    let target_dir = dirs_next().unwrap_or_else(|| PathBuf::from(".")).join(".codex");
    let _ = fs::create_dir_all(&target_dir);
    let target_file = target_dir.join("config.toml");

    let home_path = dirs_next().unwrap_or_else(|| PathBuf::from(".")).to_string_lossy().replace("\\", "/");

    let mut projects_section = format!("[projects.\"{}\"]\ntrust_level = \"trusted\"", home_path);
    if cfg!(windows) {
        projects_section.push_str("\n\n[projects.\"C:/\"]\ntrust_level = \"trusted\"");
        projects_section.push_str("\n\n[projects.\"D:/\"]\ntrust_level = \"trusted\"");
    }

    let content = CODEX_CONFIG_TOML
        .replace("{admin_port}", &ADMIN_PORT.to_string())
        .replace("{proxy_port}", &PROXY_PORT.to_string())
        .replace("{projects_section}", &projects_section);

    if let Err(e) = fs::write(&target_file, content) {
        error!("Failed to write Codex config: {}", e);
    } else {
        info!("Codex config initialized at {:?}", target_file);
    }
}
pub fn clear_codex_config_toml() {

    let target = dirs_next()

        .unwrap_or_else(|| PathBuf::from("."))

        .join(".codex")

        .join("config.toml");

    if target.exists() {

        let _ = fs::remove_file(&target);

        info!("Codex config removed: {:?}", target);

    }

}



// 鈹€鈹€ Claude 閰嶇疆鑷姩鐢熸垚 鈹€鈹€



pub fn write_claude_settings_json(force: bool) {

    let target_dir = dirs_next()

        .unwrap_or_else(|| PathBuf::from("."))

        .join(".claude");

    let _ = fs::create_dir_all(&target_dir);

    let settings_path = target_dir.join("settings.json");



    if !force && settings_path.exists() {

        debug!("Claude settings already exists, skip writing: {:?}", settings_path);

        return;

    }



    let default_model = CONFIG.get_str("default_model");

    let claude_default = CONFIG.get_claude_str("default_model");

    let model = if claude_default.is_empty() { &default_model } else { &claude_default };



    let settings = serde_json::json!({

        "env": {

            "ANTHROPIC_AUTH_TOKEN": "proxy-placeholder",

            "ANTHROPIC_BASE_URL": format!("http://127.0.0.1:{}", *PROXY_PORT),

            "ANTHROPIC_DEFAULT_HAIKU_MODEL": model,

            "ANTHROPIC_DEFAULT_OPUS_MODEL": model,

            "ANTHROPIC_DEFAULT_SONNET_MODEL": model,

            "ANTHROPIC_MODEL": model,

        },

        "model": "sonnet",

        "skipDangerousModePermissionPrompt": CONFIG.get_claude_bool("skip_dangerous_mode"),

        "theme": "auto",

    });



    if let Err(e) = fs::write(&settings_path, serde_json::to_string_pretty(&settings).unwrap() + "\n") {

        error!("Failed to write Claude settings: {}", e);

    } else {

        info!("Claude settings written to {:?}", settings_path);

    }



    if cfg!(windows) {

        ensure_claude_hosts_hijack();

    }

}



fn ensure_claude_hosts_hijack() {

    let hosts_path = Path::new(r"C:\Windows\System32\drivers\etc\hosts");

    let entries = vec![("127.0.0.1", "api.anthropic.com")];

    let existing = fs::read_to_string(hosts_path).unwrap_or_default();

    let mut changed = false;

    let mut to_add = Vec::new();

    for (ip, domain) in &entries {

        let line = format!("{} {}", ip, domain);

        if !existing.lines().any(|l| l.trim() == line.trim()) {
            to_add.push(line);

            changed = true;

        }

    }

    if !changed {

        debug!("Claude hosts hijack already in place");

        return;

    }

    match fs::OpenOptions::new().append(true).open(hosts_path) {

        Ok(mut f) => {

            for entry in &to_add {

                let _ = writeln!(f, "{}", entry);

            }

            info!("Claude hosts hijack written: {}", to_add.join(", "));

        }

        Err(e) => {

            warn!(

                "Cannot write hosts file (need admin): {}. Add manually: {}",

                e,

                to_add.join(", ")

            );

        }

    }

}



pub fn clear_claude_settings_json() {

    let target = dirs_next()

        .unwrap_or_else(|| PathBuf::from("."))

        .join(".claude")

        .join("settings.json");

    if target.exists() {

        let data: Value = fs::read_to_string(&target)

            .ok()

            .and_then(|s| serde_json::from_str(&s).ok())

            .unwrap_or(serde_json::json!({}));

        let clean = serde_json::json!({

            "model": "sonnet",

            "theme": "auto",

        });

        let _ = fs::write(&target, serde_json::to_string_pretty(&clean).unwrap() + "\n");

        info!("Claude proxy config removed from {:?}", target);

    }

}



pub fn get_proxy_status() -> Value {

    let codex_config = dirs_next()

        .unwrap_or_else(|| PathBuf::from("."))

        .join(".codex")

        .join("config.toml");

    let codex_enabled = codex_config.exists();



    let claude_settings = dirs_next()

        .unwrap_or_else(|| PathBuf::from("."))

        .join(".claude")

        .join("settings.json");

    let mut claude_enabled = false;

    if claude_settings.exists() {

        if let Ok(text) = fs::read_to_string(&claude_settings) {

            if let Ok(data) = serde_json::from_str::<Value>(&text) {

                if let Some(env) = data.get("env") {

                    if let Some(base) = env.get("ANTHROPIC_BASE_URL") {

                        claude_enabled = base.as_str().map(|s| s.starts_with("http://127.0.0.1")).unwrap_or(false);

                    }

                }

            }

        }

    }



    serde_json::json!({

        "codex_enabled": codex_enabled,

        "claude_enabled": claude_enabled,

    })

}
