// Anthropic Messages API ? DeepSeek Chat Completions 閸楀繗顔呯紙鏄忕槯閵?

// 鐎电懓绨?Python: jindx/claude.py



use std::collections::HashMap;



use actix_web::{HttpRequest, HttpResponse};

use log::{info, warn};

use serde_json::{json, Value};



use crate::modules::cache;
use crate::modules::config::CONFIG;


use crate::modules::protocol;

use crate::modules::stats;

use uuid::Uuid;



const MAX_TOKENS_DEFAULT: i64 = 16384;

const MAX_POS_DEFAULT: i64 = 1000000;



fn make_claude_id(prefix: &str) -> String {

    let hex = Uuid::new_v4().to_string().replace("-", "");

    format!("{}_{}", prefix, &hex[..24])

}



fn cfg_str(key: &str, default: &str) -> String {
    let v = CONFIG.get_claude_str(key);
    if v.is_empty() { default.to_string() } else { v }

}



fn cfg_bool(key: &str) -> bool {

    CONFIG.get_claude_bool(key)

}



fn cfg_int(key: &str, default: i64) -> i64 {

    CONFIG.get_claude_int(key, default)

}



fn get_upstream() -> String {

    let base = cfg_str("deepseek_base", "https://api.deepseek.com");

    format!("{}/v1/chat/completions", base)

}



fn get_auth_headers() -> Vec<(String, String)> {

    let key = cfg_str("deepseek_key", "");

    let key = if key.is_empty() { CONFIG.get_str("deepseek_key") } else { key };

    vec![

        ("Authorization".into(), format!("Bearer {}", key)),

        ("Content-Type".into(), "application/json".into()),

    ]

}



// 閳光偓閳光偓 Anthropic content 閳?Chat message 閳光偓閳光偓



fn anthropic_content_to_chat_message(role: &str, content: &Value) -> Vec<Value> {

    match content {

        Value::String(s) => vec![json!({"role": role, "content": s})],

        Value::Array(parts) => {

            let mut results = Vec::new();

            let mut text_parts = Vec::new();

            let mut thinking_parts = Vec::new();

            let mut tool_calls: Vec<Value> = Vec::new();



            for part in parts {

                let tp = part.get("type").and_then(|v| v.as_str()).unwrap_or("");

                match tp {

                    "text" => {

                        if let Some(t) = part.get("text").and_then(|v| v.as_str()) {

                            text_parts.push(t.to_string());

                        }

                    }

                    "thinking" => {

                        if let Some(t) = part.get("thinking").and_then(|v| v.as_str()) {

                            if !t.is_empty() {

                                thinking_parts.push(t.to_string());

                            }

                        }

                    }

                    "redacted_thinking" => {}

                    "tool_use" => {

                        tool_calls.push(json!({

                            "id": part.get("id").and_then(|v| v.as_str()).unwrap_or(""),

                            "type": "function",

                            "function": {

                                "name": part.get("name").and_then(|v| v.as_str()).unwrap_or(""),

                                "arguments": serde_json::to_string(part.get("input").unwrap_or(&serde_json::Value::Null)).unwrap_or("{}".into()),

                            },

                        }));

                    }

                    "tool_result" => {

                        let content_str = match part.get("content") {

                            Some(Value::Array(inner)) => {

                                inner.iter()

                                    .filter_map(|c| c.get("text").and_then(|v| v.as_str()).or_else(|| c.as_str()))

                                    .collect::<Vec<_>>()

                                    .join("")

                            }

                            Some(Value::String(s)) => s.clone(),

                            Some(v) => serde_json::to_string(v).unwrap_or_default(),

                            None => String::new(),

                        };

                        results.push(json!({

                            "role": "tool",

                            "tool_call_id": part.get("tool_use_id").and_then(|v| v.as_str()).unwrap_or(""),

                            "content": content_str,

                        }));

                    }

                    _ => {}

                }

            }



            let mut final_msg = json!({"role": role, "content": text_parts.join("")});

            if !tool_calls.is_empty() {

                final_msg["content"] = json!(final_msg["content"].as_str().unwrap_or(""));

                final_msg["tool_calls"] = json!(tool_calls);

            }

            if !thinking_parts.is_empty() && role == "assistant" {

                final_msg["reasoning_content"] = json!(thinking_parts.join("\n"));

            }

            results.push(final_msg);

            results

        }

        _ => vec![json!({"role": role, "content": ""})],

    }

}



fn anthropic_tools_to_chat(tools: &[Value]) -> Vec<Value> {

    tools.iter().map(|tool| {

        let schema = tool.get("input_schema").unwrap_or(&serde_json::Value::Null);

        let mut params = json!({"type": schema.get("type").and_then(|v| v.as_str()).unwrap_or("object")});

        for key in &["properties", "required", "additionalProperties", "enum",

                       "oneOf", "anyOf", "allOf", "items", "minItems", "maxItems",

                       "minProperties", "maxProperties", "uniqueItems"] {

            if let Some(val) = schema.get(key) {

                params[key] = val.clone();

            }

        }

        json!({

            "type": "function",

            "function": {

                "name": tool.get("name").and_then(|v| v.as_str()).unwrap_or(""),

                "description": tool.get("description").and_then(|v| v.as_str()).unwrap_or(""),

                "parameters": params,

            },

        })

    }).collect()

}



// 閳光偓閳光偓 Claude session key 閳光偓閳光偓



fn claude_session_key(messages: &[Value]) -> String {

    use sha2::{Digest, Sha256};

    let short: Vec<&Value> = messages.iter()

        .filter(|m| matches!(m.get("role").and_then(|v| v.as_str()), Some("user" | "assistant")))

        .take(6)

        .collect();

    if short.is_empty() {

        return "claude_default".into();

    }

    let seed = serde_json::to_string(&short).unwrap_or_default();

    let seed = &seed[..seed.len().min(200)];

    let mut hasher = Sha256::new();

    hasher.update(seed.as_bytes());

    let h = format!("{:x}", hasher.finalize());

    format!("claude_{}", &h[..8])

}



// 閳光偓閳光偓 Anthropic 閳?Chat 鏉烆剚宕?閳光偓閳光偓



pub fn anthropic_to_chat(request_body: &Value) -> (Value, String) {

    let mut messages: Vec<Value> = Vec::new();



    if let Some(system) = request_body.get("system") {

        let system_text = match system {

            Value::String(s) => s.clone(),

            Value::Array(arr) => {

                arr.iter()

                    .map(|c| c.get("text").and_then(|v| v.as_str()).unwrap_or(""))

                    .collect::<Vec<_>>()

                    .join("")

            }

            _ => String::new(),

        };

        if !system_text.is_empty() {

            messages.push(json!({"role": "system", "content": system_text}));

        }

    }



    for msg in request_body.get("messages").and_then(|v| v.as_array()).unwrap_or(&vec![]) {

        let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("user");

        let converted = anthropic_content_to_chat_message(role, msg.get("content").unwrap_or(&json!("")));

        messages.extend(converted);

    }



    let session_id = claude_session_key(&messages);

    let thinking_enabled = cfg_bool("deepseek_thinking_enabled");



    if thinking_enabled {

        let cached = cache::get_cached_reasoning("claude", &session_id);



        if !cached.is_empty() {

            let mut idx = 0;

            for msg in messages.iter_mut() {

                if msg.get("role").and_then(|v| v.as_str()) == Some("assistant") {

                    if idx < cached.len() {

                        msg["reasoning_content"] = json!(cached[idx]);

                        idx += 1;

                    }

                }

            }

            if idx > 0 {

                info!("Claude injected {} cached reasoning entries (session {})", idx, session_id);

            }

        }



        if cached.is_empty() {

            let cached_global = cache::get_cached_reasoning("claude", "recent");

            if !cached_global.is_empty() {

                for msg in messages.iter_mut() {

                    if msg.get("role").and_then(|v| v.as_str()) == Some("assistant")

                        && msg.get("reasoning_content").is_none()

                    {

                        msg["reasoning_content"] = json!(cached_global[0]);

                        break;

                    }

                }

            }

        }



        let all_cached = if cached.is_empty() {

            cache::get_cached_reasoning("claude", "recent")

        } else {

            cached

        };

        protocol::ensure_assistant_reasoning(&mut messages, &all_cached);

    }



    let model = cfg_str("default_model", "deepseek-v4-pro");

    let mut chat = json!({

        "model": model,

        "messages": messages,

        "stream": request_body.get("stream").and_then(|v| v.as_bool()).unwrap_or(false),

    });



    let max_tokens = request_body.get("max_tokens").and_then(|v| v.as_i64()).unwrap_or(0);

    let max_tokens = if max_tokens > 0 { max_tokens } else { cfg_int("max_output_tokens", MAX_TOKENS_DEFAULT) };

    if max_tokens > 0 {

        chat["max_tokens"] = json!(max_tokens);

    }

    let ctx = cfg_int("max_position_embeddings", MAX_POS_DEFAULT);

    if ctx > 0 {

        chat["max_position_embeddings"] = json!(ctx);

    }



    if let Some(temp) = request_body.get("temperature") {

        chat["temperature"] = temp.clone();

    } else if let Some(cfg_temp) = CONFIG.get_claude_opt_str("temperature") {

        if let Ok(t) = cfg_temp.parse::<f64>() {

            chat["temperature"] = json!(t);

        }

    }



    if let Some(top_p) = request_body.get("top_p") {

        chat["top_p"] = top_p.clone();

    } else if let Some(cfg_top_p) = CONFIG.get_claude_opt_str("top_p") {

        if let Ok(p) = cfg_top_p.parse::<f64>() {

            chat["top_p"] = json!(p);

        }

    }



    if !thinking_enabled {

        chat["thinking"] = json!({"type": "disabled"});

    }



    if let Some(tools) = request_body.get("tools").and_then(|v| v.as_array()) {

        chat["tools"] = json!(anthropic_tools_to_chat(tools));

        chat["tool_choice"] = json!("auto");

    }



    (chat, session_id)

}



// 閳光偓閳光偓 闂団偓鐟曚礁婀?protocol 濡€虫健娑擃叀鐨熼悽銊ф畱鏉堝懎濮崙鑺ユ殶 閳光偓閳光偓

// 濞夈劍鍓伴敍姘崇箹娑擃亝藟閹恒儱鍤遍弫鎵暏娴?claude 濡€虫健鐠嬪啰鏁?protocol 娑擃厾娈?ensure_assistant_reasoning

// 閻㈠彉绨?Rust 濡€虫健缁崵绮洪惃鍕閸掕绱濋幋鎴滄粦閸?protocol 娑擃厼鍙曞鈧銈堢窡閸?

pub mod bridge {

    use super::*;



    pub fn ensure_assistant_reasoning(messages: &mut Vec<Value>, cached: &[String]) {

        // 鐠嬪啰鏁?protocol 濡€虫健娑擃厾娈戦柅鏄忕帆

        super::_ensure_reasoning(messages, cached);

    }

}



fn _ensure_reasoning(messages: &mut Vec<Value>, cached_reasoning: &[String]) {

    if cached_reasoning.is_empty() {

        return;

    }

    let mut cache_idx = 0;

    let mut cache_used = std::collections::HashSet::new();



    for i in 0..messages.len() {

        if messages[i].get("role").and_then(|v| v.as_str()) == Some("assistant") {

            if messages[i].get("reasoning_content").is_some() {

                continue;

            }

            while cache_idx < cached_reasoning.len() {

                if !cache_used.contains(&cache_idx) {

                    messages[i]["reasoning_content"] = json!(cached_reasoning[cache_idx]);

                    cache_used.insert(cache_idx);

                    break;

                }

                cache_idx += 1;

            }

        }

    }

}



// 閳光偓閳光偓 Chat 閳?Anthropic 閸濆秴绨?閳光偓閳光偓



const FINISH_MAP: &[(&str, &str)] = &[

    ("stop", "end_turn"),

    ("length", "max_tokens"),

    ("tool_calls", "tool_use"),

    ("content_filter", "end_turn"),

];



fn ds_finish_to_claude(fs: &str) -> &str {

    FINISH_MAP.iter()

        .find(|(k, _)| *k == fs)

        .map(|(_, v)| *v)

        .unwrap_or("end_turn")

}



fn ensure_tool_use_id(tid: &str) -> String {

    if tid.starts_with("toolu_") {

        tid.to_string()

    } else {

        make_claude_id("toolu")

    }

}



pub fn chat_to_anthropic(chat_response: &Value, upstream_model: &str) -> (Value, String) {

    let msg_id = make_claude_id("msg");

    let mut blocks = Vec::new();

    let mut has_tc = false;

    let mut reasoning_text = String::new();

    let mut finish = "stop";



    if let Some(choices) = chat_response["choices"].as_array() {

        if let Some(ch) = choices.first() {

            finish = ch.get("finish_reason").and_then(|v| v.as_str()).unwrap_or("stop");



            if let Some(msg) = ch.get("message") {

                let ds_text = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");

                reasoning_text = msg.get("reasoning_content").and_then(|v| v.as_str()).unwrap_or("").to_string();



                let strip = cfg_bool("strip_thinking");

                if !reasoning_text.is_empty() && !strip {

                    blocks.push(json!({"type": "text", "text": reasoning_text}));

                }

                if !ds_text.is_empty() {

                    blocks.push(json!({"type": "text", "text": ds_text}));

                }



                if let Some(tc_arr) = msg.get("tool_calls").and_then(|v| v.as_array()) {

                    has_tc = true;

                    for tc in tc_arr {

                        let func = tc.get("function").unwrap_or(&serde_json::Value::Null);

                        let args_str = func.get("arguments").and_then(|v| v.as_str()).unwrap_or("{}");

                        let inp: Value = serde_json::from_str(args_str).unwrap_or(json!({"_raw": args_str}));

                        blocks.push(json!({

                            "type": "tool_use",

                            "id": ensure_tool_use_id(tc.get("id").and_then(|v| v.as_str()).unwrap_or("")),

                            "name": func.get("name").and_then(|v| v.as_str()).unwrap_or(""),

                            "input": inp,

                        }));

                    }

                }

            }

        }

    }



    if blocks.is_empty() {

        blocks = vec![json!({"type": "text", "text": ""})];

    }



    let usage = chat_response.get("usage").unwrap_or(&serde_json::Value::Null);



    let response = json!({

        "id": msg_id,

        "type": "message",

        "role": "assistant",

        "model": upstream_model,

        "content": blocks,

        "stop_reason": if has_tc { "tool_use" } else { ds_finish_to_claude(finish) },

        "stop_sequence": null,

        "usage": {

            "input_tokens": usage.get("prompt_tokens").and_then(|v| v.as_i64()).unwrap_or(0),

            "output_tokens": usage.get("completion_tokens").and_then(|v| v.as_i64()).unwrap_or(0),

        },

    });



    (response, reasoning_text)

}



// 閳光偓閳光偓 閻劋绨?protocol 濡€虫健瀵洜鏁ら惃鍕窡閸斺晛鍤遍弫?閳光偓閳光偓

// 鐏?_ensure_assistant_reasoning 濡椼儲甯存稉鍝勫彆瀵偓閸戣姤鏆?

pub fn ensure_assistant_reasoning_for_protocol(messages: &mut Vec<Value>, cached: &[String]) {

    _ensure_reasoning(messages, cached);

}

// 閸忣剙绱戠紒?main.rs 娴ｈ法鏁ら惃鍕窡閸斺晛鍤遍弫?
