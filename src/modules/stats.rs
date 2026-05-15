// 缁熻璁℃暟鍜岄敊璇棩蹇楃幆褰㈢紦鍐插尯锛岀嚎绋嬪畨鍏ㄣ€?
// 瀵瑰簲 Python: jindx/stats.py



use std::collections::{HashMap, VecDeque};

use std::sync::RwLock;

use std::time::{SystemTime, UNIX_EPOCH};



use lazy_static::lazy_static;

use regex::Regex;

use serde_json::json;

use serde_json::Value;



// 鈹€鈹€ 缁熻鏁版嵁 鈹€鈹€



#[derive(Debug, Clone)]

struct StatsInner {

    start_time: u64,

    total_requests: u64,

    codex_requests: u64,

    claude_requests: u64,

    active_streams: i64,

    errors_by_code: HashMap<String, u64>,

    cache_hits: u64,

    cache_misses: u64,

    upstream_errors: HashMap<String, u64>,

}



impl StatsInner {

    fn new() -> Self {

        Self {

            start_time: SystemTime::now()

                .duration_since(UNIX_EPOCH)

                .unwrap()

                .as_secs(),

            total_requests: 0,

            codex_requests: 0,

            claude_requests: 0,

            active_streams: 0,

            errors_by_code: HashMap::new(),

            cache_hits: 0,

            cache_misses: 0,

            upstream_errors: HashMap::new(),

        }

    }

}



lazy_static! {

    static ref STATS: RwLock<StatsInner> = RwLock::new(StatsInner::new());

    static ref LOG_BUFFER: RwLock<VecDeque<LogEntry>> = RwLock::new(VecDeque::new());

}



const MAX_LOG_BUFFER: usize = 200;



#[derive(Debug, Clone)]

struct LogEntry {

    ts: u64,

    msg: String,

}



fn now_ts() -> u64 {

    SystemTime::now()

        .duration_since(UNIX_EPOCH)

        .unwrap()

        .as_secs()

}



// 鈹€鈹€ 鏁忔劅鏁版嵁杩囨护 鈹€鈹€



lazy_static! {

    static ref SENSITIVE_PATTERNS: Vec<Regex> = vec![

        Regex::new(r"sk-[a-zA-Z0-9_-]{20,}").unwrap(),

        Regex::new(r"Bearer\s+\S+").unwrap(),

    ];

}



pub fn sanitize_log(msg: &str) -> String {

    let mut result = msg.to_string();

    for pattern in SENSITIVE_PATTERNS.iter() {

        result = pattern.replace_all(&result, "<REDACTED>").to_string();

    }

    result

}



// 鈹€鈹€ 璁板綍鍑芥暟 鈹€鈹€



pub fn record_request() {

    let mut s = STATS.write().unwrap();

    s.total_requests += 1;

}



pub fn record_codex_request() {

    let mut s = STATS.write().unwrap();

    s.total_requests += 1;

    s.codex_requests += 1;

}



pub fn record_claude_request() {

    let mut s = STATS.write().unwrap();

    s.total_requests += 1;

    s.claude_requests += 1;

}



pub fn record_error(code: u16) {

    let mut s = STATS.write().unwrap();

    let key = code.to_string();

    *s.errors_by_code.entry(key).or_insert(0) += 1;

}



pub fn record_upstream_error(msg: &str) {

    let mut s = STATS.write().unwrap();

    let short = &msg[..msg.len().min(120)];

    *s.upstream_errors.entry(short.to_string()).or_insert(0) += 1;

}



pub fn record_cache(hit: bool) {

    let mut s = STATS.write().unwrap();

    if hit {

        s.cache_hits += 1;

    } else {

        s.cache_misses += 1;

    }

}



pub fn log_error(msg: &str) {

    let entry = LogEntry {

        ts: now_ts(),

        msg: sanitize_log(&msg[..msg.len().min(500)]),

    };

    let mut buf = LOG_BUFFER.write().unwrap();

    buf.push_back(entry);

    while buf.len() > MAX_LOG_BUFFER {

        buf.pop_front();

    }

}



// 鈹€鈹€ 鏌ヨ鍑芥暟 鈹€鈹€



pub fn get_stats() -> Value {

    let s = STATS.read().unwrap();

    let total = s.total_requests.max(1);

    let errors: u64 = s.errors_by_code.values().sum();

    let error_rate = (errors as f64 / total as f64 * 100.0 * 10.0).round() / 10.0;

    let cache_total = (s.cache_hits + s.cache_misses).max(1);

    let cache_hit_rate =

        (s.cache_hits as f64 / cache_total as f64 * 100.0 * 10.0).round() / 10.0;



    let mut top_errors: Vec<(String, u64)> = s.errors_by_code.iter()

        .map(|(k, v)| (k.clone(), *v))

        .collect();

    top_errors.sort_by(|a, b| b.1.cmp(&a.1));

    top_errors.truncate(10);



    let mut top_upstream: Vec<(String, u64)> = s.upstream_errors.iter()

        .map(|(k, v)| (k.clone(), *v))

        .collect();

    top_upstream.sort_by(|a, b| b.1.cmp(&a.1));

    top_upstream.truncate(5);



    json!({

        "uptime": now_ts() - s.start_time,

        "total_requests": s.total_requests,

        "codex_requests": s.codex_requests,

        "claude_requests": s.claude_requests,

        "active_streams": s.active_streams,

        "errors_by_code": top_errors.into_iter().collect::<HashMap<_, _>>(),

        "error_rate": error_rate,

        "cache_hits": s.cache_hits,

        "cache_misses": s.cache_misses,

        "cache_hit_rate": cache_hit_rate,

        "top_upstream_errors": top_upstream.into_iter()

            .map(|(msg, count)| json!({"msg": msg, "count": count}))

            .collect::<Vec<_>>(),

    })

}



pub fn get_logs(limit: usize) -> Vec<Value> {

    let buf = LOG_BUFFER.read().unwrap();

    let len = buf.len();

    let start = if len > limit { len - limit } else { 0 };

    let mut result: Vec<Value> = buf.iter()

        .skip(start)

        .rev()

        .map(|e| json!({"ts": e.ts, "msg": e.msg}))

        .collect();

    result.truncate(limit);

    result

}



pub fn increment_active_streams() {

    STATS.write().unwrap().active_streams += 1;

}



pub fn decrement_active_streams() {

    STATS.write().unwrap().active_streams -= 1;

}



// 鈹€鈹€ 鏃ュ織杩囨护鍣?鈹€鈹€



pub struct SensitiveDataFilter;



impl log::Log for SensitiveDataFilter {

    fn enabled(&self, _: &log::Metadata) -> bool {

        true

    }



    fn log(&self, record: &log::Record) {

        let msg = sanitize_log(&format!("{}", record.args()));

        eprintln!("[{}] {}", record.level(), msg);

    }



    fn flush(&self) {}

}