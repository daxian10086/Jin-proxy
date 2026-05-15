// 缂傚啯鍨块妴澶愬箮閹惧啿绲块柨娑欑摋RL 婵☆偀鍋撴繛鏉戭儍閳ь兛绶氶。鈺呭矗閺嵮勫濞寸媴绲块幃濠勭棯瑜庢慨鍕矗閺嶃儮鍋?

// 閻庣數鎳撶花?Python: jindx/web_fetch.py



use std::collections::{HashMap, HashSet};

use std::net::IpAddr;

use std::str::FromStr;

use std::time::Duration;



use ipnet::IpNet;

use lazy_static::lazy_static;

use log::{info, warn};

use regex::Regex;

use scraper::{Html, Selector};

use serde_json::{json, Value};

use url::Url;



use crate::modules::config::CONFIG;



// 闁冲厜鍋撻柍鍏夊亾 SSRF 闂傚啫寮舵慨銏ゆ晬濮橆剙鏁剁紓?IP 婵炲牏鏁荤划锕傚触瀹ュ懎绀?闁冲厜鍋撻柍鍏夊亾



lazy_static! {

    static ref PRIVATE_NETWORKS: Vec<IpNet> = vec![

        "10.0.0.0/8".parse().unwrap(),

        "172.16.0.0/12".parse().unwrap(),

        "192.168.0.0/16".parse().unwrap(),

        "169.254.0.0/16".parse().unwrap(),

        "127.0.0.0/8".parse().unwrap(),

        "::1/128".parse().unwrap(),

        "fc00::/7".parse().unwrap(),

    ];

}



fn is_private_url(url_str: &str) -> bool {

    if let Ok(parsed) = Url::parse(url_str) {

        if let Some(host) = parsed.host_str() {

            if let Ok(addr) = IpAddr::from_str(host) {

                return PRIVATE_NETWORKS.iter().any(|net| net.contains(&addr));

            }

            return false; // hostname 闂?IP 闁革附婢樺鍐晬鐏炵偓鏉归悶?

        }

    }

    true // 闁归攱甯炵划椋庣矚?hostname

}



// 闁冲厜鍋撻柍鍏夊亾 web_fetch 鐎规悶鍎遍崣璺ㄢ偓瑙勭煯缁?闁冲厜鍋撻柍鍏夊亾



pub fn web_fetch_tool_definition() -> Value {

    json!({

        "type": "function",

        "function": {

            "name": "web_fetch",

            "description": "Fetch content from a URL over HTTP/HTTPS. Use this instead of curl, wget, or other shell-based HTTP tools. Returns HTTP status and response body. Supports GET, HEAD, POST, PUT, DELETE, PATCH, OPTIONS methods.",

            "parameters": {

                "type": "object",

                "properties": {

                    "url": {"type": "string", "description": "The URL to fetch (http:// or https://)"},

                    "method": {"type": "string", "enum": ["GET", "HEAD", "POST", "PUT", "DELETE", "PATCH", "OPTIONS"], "description": "HTTP method (default: GET)"},

                    "headers": {"type": "object", "description": "Optional HTTP headers as key-value pairs"},

                    "body": {"type": "string", "description": "Request body for POST/PUT/PATCH requests"},

                },

                "required": ["url"],

            },

        },

    })

}



pub const WEB_FETCH_HINT: &str = "A web_fetch tool is available for HTTP/HTTPS requests. Use it instead of curl, wget, or shell-based HTTP tools. The tool accepts: url (required), method (GET default), headers (optional), body (optional).";



// 闁冲厜鍋撻柍鍏夊亾 URL 闁圭粯鍔曡ぐ?闁冲厜鍋撻柍鍏夊亾



pub fn extract_urls_from_text(text: &str) -> Vec<String> {

    let re = Regex::new(r#"https?://[^\s<>"{}|\\^\[\]]+"#).unwrap();

    re.find_iter(text).map(|m| m.as_str().to_string()).collect()

}



pub fn has_urls_in_messages(messages: &[Value]) -> bool {

    for msg in messages {

        if let Some(content) = msg.get("content") {

            match content {

                Value::String(s) => {

                    if s.contains("http://") || s.contains("https://") {

                        return true;

                    }

                }

                Value::Array(parts) => {

                    for part in parts {

                        if let Some(text) = part.get("text").and_then(|v| v.as_str()) {

                            if text.contains("http://") || text.contains("https://") {

                                return true;

                            }

                        }

                    }

                }

                _ => {}

            }

        }

    }

    false

}



fn extract_urls_from_messages(messages: &[Value]) -> Vec<String> {

    let mut all_urls = Vec::new();

    for msg in messages {

        if let Some(role) = msg.get("role").and_then(|v| v.as_str()) {

            if role == "user" || role == "system" {

                if let Some(content) = msg.get("content").and_then(|v| v.as_str()) {

                    all_urls.extend(extract_urls_from_text(content));

                }

            }

        }

    }



    let mut seen = HashSet::new();

    let mut urls = Vec::new();

    for u in all_urls {

        if seen.contains(&u) {

            continue;

        }

        if u.contains("127.0.0.1") || u.contains("localhost") || u.contains("0.0.0.0") || u.contains("::1") {

            continue;

        }

        if is_private_url(&u) {

            warn!("Skipping private/internal URL: {}", u);

            continue;

        }

        seen.insert(u.clone());

        urls.push(u);

    }

    urls

}



// 闁冲厜鍋撻柍鍏夊亾 闁告艾鏈?URL 闁硅埖鎸歌ぐ?闁冲厜鍋撻柍鍏夊亾



fn fetch_url_sync(url: &str, fetch_timeout: u64, max_body: usize) -> Option<String> {

    let client = reqwest::blocking::Client::builder()

        .timeout(Duration::from_secs(fetch_timeout))

        .user_agent("Mozilla/5.0 (compatible; ChatProxy/1.0)")

        .build()

        .ok()?;



    match client.get(url).send() {

        Ok(resp) => {

            let ct = resp.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();

            let text = resp.text().unwrap_or_default();

            let mut processed = if ct.contains("html") {

                // 缂佺姭鍋撻柛妤佹礈濞?HTML 闁哄秴娲ㄩ鐑藉礈閵壯岀€?

                let re_script = Regex::new(r"(?is)<script[^>]*>.*?</script>").unwrap();

                let re_style = Regex::new(r"(?is)<style[^>]*>.*?</style>").unwrap();

                let re_tag = Regex::new(r"<[^>]+>").unwrap();

                let re_ws = Regex::new(r"\s+").unwrap();

                let t = re_script.replace_all(&text, "");

                let t = re_style.replace_all(&t, "");

                let t = re_tag.replace_all(&t, " ");

                re_ws.replace_all(&t, " ").trim().to_string()

            } else {

                text

            };



            if processed.len() > max_body {

                processed = format!("{}...[truncated, {} chars]", &processed[..max_body], processed.len() - max_body);

            }

            Some(processed)

        }

        Err(e) => {

            warn!("Pre-fetch failed for {}: {}", url, e);

            None

        }

    }

}



fn inject_fetched_context(messages: &mut Vec<Value>, fetched: &HashMap<String, String>) {

    if fetched.is_empty() {

        return;

    }

    let context: String = fetched.iter()

        .map(|(url, content)| format!("[Web content from {}]\n{}", url, content))

        .collect::<Vec<_>>()

        .join("\n\n---\n\n");



    let context = format!("\n\n[Pre-fetched web content 闁?use this directly, no need to call web_fetch]\n\n{}", context);



    for msg in messages.iter_mut().rev() {

        if msg.get("role").and_then(|v| v.as_str()) == Some("user") {

            if let Some(content) = msg.get("content").and_then(|v| v.as_str()) {

                msg["content"] = json!(format!("{}{}", content, context));

                return;

            }

        }

    }

    messages.push(json!({"role": "user", "content": context}));

}



// 闁冲厜鍋撻柍鍏夊亾 闁稿浚鍓欑槐鎴烇紣閸曨偄绲块柛鎴ｅГ閺?闁冲厜鍋撻柍鍏夊亾



pub fn prefetch_urls_into_messages(messages: &mut Vec<Value>) {

    let urls = extract_urls_from_messages(messages);

    if urls.is_empty() {

        return;

    }



    let max_urls = CONFIG.get_int("web_fetch_max_urls", 5) as usize;

    let fetch_timeout = CONFIG.get_int("web_fetch_timeout", 10) as u64;

    let max_body = CONFIG.get_int("web_fetch_max_body", 80000) as usize;



    let mut fetched = HashMap::new();

    for url in urls.iter().take(max_urls) {

        if let Some(content) = fetch_url_sync(url, fetch_timeout, max_body) {

            info!("Pre-fetched {} -> {} chars", url, content.len());

            fetched.insert(url.clone(), content);

        }

    }



    inject_fetched_context(messages, &fetched);

}



// 闁冲厜鍋撻柍鍏夊亾 鐎殿喖鍊归?web_fetch 闁圭瑳鍡╂斀 闁冲厜鍋撻柍鍏夊亾



pub async fn execute_web_fetch(args: &Value, http_client: &reqwest::Client) -> String {

    let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("");

    if url.is_empty() {

        return "Error: no URL provided".to_string();

    }

    if is_private_url(url) {

        return format!("Error: requests to internal/private addresses are blocked ({})", url);

    }



    let method = args.get("method").and_then(|v| v.as_str()).unwrap_or("GET").to_uppercase();

    let headers_val = args.get("headers").cloned().unwrap_or(json!({}));

    let req_body = args.get("body").and_then(|v| v.as_str());



    let max_body = CONFIG.get_int("web_fetch_max_body", 80000) as usize;



    if method == "GET" {

        // 閻忓繑绻嗛惁?Jina AI 濞寸媴绲块幃?

        let jina_url = format!("https://r.jina.ai/{}", url);

        let resp = http_client

            .get(&jina_url)

            .header("Accept", "text/plain")

            .header("X-Return-Format", "markdown")

            .timeout(Duration::from_secs(20))

            .send()

            .await;



        if let Ok(r) = resp {

            if r.status().is_success() {

                let text = r.text().await.unwrap_or_default();

                if text.len() <= max_body {

                    return text;

                }

                return format!("{}...[truncated, {} chars]", &text[..max_body], text.len() - max_body);

            }

        }

    }



    raw_fetch(http_client, url, &method, &headers_val, req_body, max_body).await

}



async fn raw_fetch(

    client: &reqwest::Client,

    url: &str,

    method: &str,

    headers_val: &Value,

    req_body: Option<&str>,

    max_body: usize,

) -> String {

    let mut req = match method {

        "GET" => client.get(url),

        "HEAD" => client.head(url),

        "POST" => client.post(url),

        "PUT" => client.put(url),

        "DELETE" => client.delete(url),

        "PATCH" => client.patch(url),

        "OPTIONS" => client.request(reqwest::Method::OPTIONS, url),

        _ => client.get(url),

    };



    req = req.header("User-Agent", "Mozilla/5.0 (compatible; ChatProxy/1.0)");

    if let Some(obj) = headers_val.as_object() {

        for (k, v) in obj {

            if let Some(val) = v.as_str() {

                req = req.header(k.as_str(), val);

            }

        }

    }



    if let Some(body) = req_body {

        if matches!(method, "POST" | "PUT" | "PATCH") {

            req = req.body(body.to_string());

        }

    }



    match req.timeout(Duration::from_secs(20)).send().await {

        Ok(resp) => {

            let status = resp.status();

            let ct = resp.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();



            if method == "HEAD" || method == "OPTIONS" {

                let hdrs: Vec<String> = resp.headers().iter()

                    .map(|(k, v)| format!("{}: {}", k, v.to_str().unwrap_or("")))

                    .collect();

                return format!("HTTP {} {}\n{}", status.as_u16(), status.canonical_reason().unwrap_or(""), hdrs.join("\n"));

            }



            if ct.contains("image") || ct.contains("audio") || ct.contains("video") || ct.contains("octet-stream") {

                return format!("HTTP {}\nContent-Type: {}\n(binary content, not shown)", status.as_u16(), ct);

            }



            let text = resp.text().await.unwrap_or_default();

            if text.len() <= max_body {

                return format!("HTTP {}\n\n{}", status.as_u16(), text);

            }

            format!("HTTP {}\n\n{}...[truncated, {} chars]", status.as_u16(), &text[..max_body], text.len() - max_body)

        }

        Err(e) => format!("Fetch error: {}", e),

    }

}
