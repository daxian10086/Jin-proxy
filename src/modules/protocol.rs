// OpenAI Responses API ? DeepSeek Chat Completions 鍗忚缈昏瘧銆?

// 瀵瑰簲 Python: jindx/protocol.py



use std::collections::HashMap;



use crate::modules::cache;

use crate::modules::config::{self, CONFIG};

use crate::modules::web_fetch;

use log::info;

use serde_json::{json, Value};

use uuid::Uuid;



// 鈹€鈹€ ID 鐢熸垚 鈹€鈹€



pub fn make_id(prefix: &str) -> String {

    let hex = Uuid::new_v4().to_string().replace("-", "");

    format!("{}_{}", prefix, &hex[..24])

}



// 鈹€鈹€ 瑙掕壊姝ｈ鍖?鈹€鈹€



fn normalize_role(role: &str) -> &str {

    if role == "developer" {

        "system"

    } else {

        role

    }

}



// 鈹€鈹€ 宸ュ叿娑堟伅鎺掑簭淇 鈹€鈹€



fn fix_tool_message_ordering(messages: &mut Vec<Value>) {

    if messages.is_empty() {

        return;

    }

    let mut fixed = Vec::new();

    let mut skip = std::collections::HashSet::new();



    for i in 0..messages.len() {

        if skip.contains(&i) {

            continue;

        }

        let msg = &messages[i];

        if msg.get("role").and_then(|r| r.as_str()) == Some("assistant")

            && msg.get("tool_calls").is_some()

        {

            fixed.push(msg.clone());

            let call_ids: std::collections::HashSet<String> = msg["tool_calls"]

                .as_array()

                .map(|arr| arr.iter().filter_map(|tc| tc["id"].as_str().map(|s| s.to_string())).collect())

                .unwrap_or_default();

            for j in (i + 1)..messages.len() {

                let mj = &messages[j];

                if mj.get("role").and_then(|r| r.as_str()) == Some("tool")

                    && mj.get("tool_call_id")

                        .and_then(|id| id.as_str())

                        .map(|id| call_ids.contains(id))

                        .unwrap_or(false)

                {

                    fixed.push(mj.clone());

                    skip.insert(j);

                }

            }

        } else if msg.get("role").and_then(|r| r.as_str()) == Some("tool") {

            // 瀛ょ珛 tool 娑堟伅闄勫姞鍒版渶鍚庝竴涓惈 tool_calls 鐨?assistant

            let mut insert_at = fixed.len();

            for (fi, fm) in fixed.iter().enumerate().rev() {

                if fm.get("role").and_then(|r| r.as_str()) == Some("assistant")

                    && fm.get("tool_calls").is_some()

                {

                    insert_at = fi + 1;

                    while insert_at < fixed.len()

                        && fixed[insert_at].get("role").and_then(|r| r.as_str()) == Some("tool")

                    {

                        insert_at += 1;

                    }

                    break;

                }

            }

            fixed.insert(insert_at, msg.clone());

        } else {

            fixed.push(msg.clone());

        }

    }

    *messages = fixed;

}



// 鈹€鈹€ 纭繚 assistant 鎺ㄧ悊鍐呭 鈹€鈹€



pub fn ensure_assistant_reasoning(messages: &mut Vec<Value>, cached_reasoning: &[String]) {

    if cached_reasoning.is_empty() {

        return;

    }

    let mut cache_idx = 0;

    let mut cache_used = std::collections::HashSet::new();

    let mut turn_start = 0;



    for i in 0..messages.len() {

        if messages[i].get("role").and_then(|r| r.as_str()) == Some("assistant") {

            if messages[i].get("reasoning_content").is_some() {

                continue;

            }

            // 鍦ㄥ悓鍥炲悎鐨勫墠涓€涓?assistant 娑堟伅涓煡鎵?reasoning

            for j in (turn_start..i).rev() {

                if messages[j].get("role").and_then(|r| r.as_str()) == Some("assistant") {

                    if let Some(rc) = messages[j].get("reasoning_content").and_then(|v| v.as_str()) {

                        messages[i]["reasoning_content"] = json!(rc);

                        break;

                    }

                }

            }

            if messages[i].get("reasoning_content").is_none() {

                while cache_idx < cached_reasoning.len() {

                    if !cache_used.contains(&cache_idx) {

                        messages[i]["reasoning_content"] = json!(cached_reasoning[cache_idx]);

                        cache_used.insert(cache_idx);

                        break;

                    }

                    cache_idx += 1;

                }

            }

        } else if messages[i].get("role").and_then(|r| r.as_str()) != Some("tool") {

            turn_start = i + 1;

        }

    }

}



// 鈹€鈹€ 浠?Responses API 璇锋眰浣撲腑鎻愬彇娑堟伅鍒楄〃 鈹€鈹€



fn extract_message_items(data: &Value) -> Vec<Value> {

    let mut results = Vec::new();



    if let Some(instructions) = data.get("instructions").and_then(|v| v.as_str()) {

        if !instructions.is_empty() {

            results.push(json!({"role": "system", "content": instructions}));

        }

    }



    let inp = data.get("input");

    match inp {

        Some(Value::String(s)) => {

            results.push(json!({"role": "user", "content": s}));

        }

        Some(Value::Array(arr)) => {

            let mut pending_tool_calls: Vec<Value> = Vec::new();



            let flush_pending = |results: &mut Vec<Value>, pending: &mut Vec<Value>| {

                if !pending.is_empty() {

                    results.push(json!({

                        "role": "assistant",

                        "content": null,

                        "tool_calls": pending.clone(),

                    }));

                    pending.clear();

                }

            };



            for item in arr {

                let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");

                match item_type {

                    "message" => {

                        let role = normalize_role(item.get("role").and_then(|v| v.as_str()).unwrap_or("user"));

                        let content = item.get("content");

                        match content {

                            Some(Value::Array(parts)) => {

                                let mut text_parts = Vec::new();

                                let mut msg_tool_calls = Vec::new();

                                for part in parts {

                                    let pt = part.get("type").and_then(|v| v.as_str()).unwrap_or("");

                                    match pt {

                                        "input_text" | "text" | "output_text" => {

                                            if let Some(t) = part.get("text").and_then(|v| v.as_str()) {

                                                text_parts.push(t.to_string());

                                            }

                                        }

                                        "reasoning_text" => {

                                            if let Some(t) = part.get("text").and_then(|v| v.as_str()) {

                                                text_parts.push(t.to_string());

                                            }

                                        }

                                        "input_image" => text_parts.push("[image]".into()),

                                        "input_file" => {

                                            text_parts.push(format!(

                                                "[file: {}]",

                                                part.get("filename").and_then(|v| v.as_str()).unwrap_or("")

                                            ));

                                        }

                                        "function_call" => {

                                            msg_tool_calls.push(json!({

                                                "id": part.get("id").or(part.get("call_id")).and_then(|v| v.as_str()).unwrap_or(""),

                                                "type": "function",

                                                "function": {

                                                    "name": part.get("name").and_then(|v| v.as_str()).unwrap_or(""),

                                                    "arguments": part.get("arguments").and_then(|v| v.as_str()).unwrap_or(""),

                                                },

                                            }));

                                        }

                                        _ => {}

                                    }

                                }

                                let content_str = if text_parts.is_empty() {

                                    content.map(|v| v.to_string()).unwrap_or_default()

                                } else {

                                    text_parts.join("\n")

                                };

                                if !msg_tool_calls.is_empty() {

                                    flush_pending(&mut results, &mut pending_tool_calls);

                                    results.push(json!({

                                        "role": role,

                                        "content": if content_str.is_empty() { Value::Null } else { json!(content_str) },

                                        "tool_calls": msg_tool_calls,

                                    }));

                                } else {

                                    flush_pending(&mut results, &mut pending_tool_calls);

                                    results.push(json!({"role": role, "content": content_str}));

                                }

                            }

                            _ => {

                                flush_pending(&mut results, &mut pending_tool_calls);

                                let c = content.map(|v| v.as_str().unwrap_or("").to_string()).unwrap_or_default();

                                results.push(json!({"role": role, "content": c}));

                            }

                        }

                    }

                    "function_call" => {

                        pending_tool_calls.push(json!({

                            "id": item.get("call_id").or(item.get("id")).and_then(|v| v.as_str()).unwrap_or(""),

                            "type": "function",

                            "function": {

                                "name": item.get("name").and_then(|v| v.as_str()).unwrap_or(""),

                                "arguments": item.get("arguments").and_then(|v| v.as_str()).unwrap_or(""),

                            },

                        }));

                    }

                    "function_call_output" => {

                        flush_pending(&mut results, &mut pending_tool_calls);

                        let call_id = item.get("call_id").and_then(|v| v.as_str()).unwrap_or("");

                        let output = item.get("output");

                        let output_str = match output {

                            Some(Value::Object(obj)) => serde_json::to_string(obj).unwrap_or_default(),

                            Some(Value::String(s)) => s.clone(),

                            Some(v) => v.to_string(),

                            None => String::new(),

                        };

                        results.push(json!({

                            "role": "tool",

                            "tool_call_id": call_id,

                            "content": output_str,

                        }));

                    }

                    _ => {

                        flush_pending(&mut results, &mut pending_tool_calls);

                        if let Some(role) = item.get("role").and_then(|v| v.as_str()) {

                            let r = normalize_role(role);

                            let content = item.get("content");

                            let content_str = match content {

                                Some(Value::Array(parts)) => {

                                    let texts: Vec<String> = parts

                                        .iter()

                                        .filter_map(|p| {

                                            if matches!(p.get("type").and_then(|v| v.as_str()), Some("input_text" | "text" | "output_text")) {

                                                p.get("text").and_then(|v| v.as_str()).map(|s| s.to_string())

                                            } else {

                                                None

                                            }

                                        })

                                        .collect();

                                    if texts.is_empty() {

                                        content.map(|v| v.to_string()).unwrap_or_default()

                                    } else {

                                        texts.join("\n")

                                    }

                                }

                                _ => content.and_then(|v| v.as_str()).unwrap_or("").to_string(),

                            };

                            results.push(json!({"role": r, "content": content_str}));

                        } else if let Some(text) = item.get("text").and_then(|v| v.as_str()) {

                            results.push(json!({"role": "user", "content": text}));

                        }

                    }

                }

            }

            flush_pending(&mut results, &mut pending_tool_calls);

        }

        _ => {}

    }



    results

}



// 鈹€鈹€ 宸ュ叿杞崲 鈹€鈹€



fn convert_tools(tools: &[Value]) -> Vec<Value> {

    let mut result = Vec::new();

    for tool in tools {

        if let Some(func) = tool.get("function") {

            let name = func.get("name").and_then(|v| v.as_str()).unwrap_or("");

            if name.is_empty() {

                continue;

            }

            let mut func_obj = func.clone();

            let params = func_obj.get("parameters");

            if !params.map(|p| p.is_object() && p.get("type").and_then(|v| v.as_str()) == Some("object")).unwrap_or(false) {

                func_obj["parameters"] = json!({"type": "object", "properties": {}});

            }

            result.push(json!({"type": "function", "function": func_obj}));

        } else {

            let name = tool.get("name").or(tool.get("type")).and_then(|v| v.as_str()).unwrap_or("");

            if name.is_empty() {

                continue;

            }

            let params = tool.get("parameters").cloned().unwrap_or(json!({"type": "object", "properties": {}}));

            result.push(json!({

                "type": "function",

                "function": {

                    "name": name,

                    "description": tool.get("description").and_then(|v| v.as_str()).unwrap_or(""),

                    "parameters": params,

                },

            }));

        }

    }

    result

}



// 鈹€鈹€ 妯″瀷鏄犲皠 鈹€鈹€



pub fn map_model(name: &str) -> String {

    if name.is_empty() {

        return CONFIG.get_str("default_model");

    }

    let low = name.to_lowercase();

    if low.contains("deepseek") {

        return name.to_string();

    }

    let mapping = CONFIG.get_model_mapping();

    if let Some(mapped) = mapping.get(name) {

        return mapped.clone();

    }

    if let Some(mapped) = mapping.get(&low) {

        return mapped.clone();

    }

    CONFIG.get_str("default_model")

}



// 鈹€鈹€ Responses 鈫?Chat 杞崲 鈹€鈹€



pub fn responses_to_chat(data: &Value, prefetch_urls: bool) -> Value {

    let mut messages = extract_message_items(data);



    if messages.is_empty() {

        if let Some(prompt) = data.get("prompt").and_then(|v| v.as_str()) {

            messages.push(json!({"role": "user", "content": prompt}));

        }

    }

    if messages.is_empty() {

        messages.push(json!({"role": "user", "content": ""}));

    }



    fix_tool_message_ordering(&mut messages);



    let session_id = get_session_id(data);

    let cached_reasoning = cache::get_cached_reasoning("codex", &session_id);

    if !cached_reasoning.is_empty() {

        let mut reasoning_idx = 0;

        for msg in messages.iter_mut() {

            if msg.get("role").and_then(|r| r.as_str()) == Some("assistant") {

                if reasoning_idx < cached_reasoning.len() {

                    msg["reasoning_content"] = json!(cached_reasoning[reasoning_idx]);

                }

                reasoning_idx += 1;

            }

        }

        info!("Attached reasoning_content to assistant messages (used {} entries)", reasoning_idx.min(cached_reasoning.len()));

    }



    ensure_assistant_reasoning(&mut messages, &cached_reasoning);



    // tool-use 鎻愮ず浠呴杞敞鍏?

    let tools = data.get("tools").and_then(|v| v.as_array());

    let has_history = messages.iter().any(|m| {

        matches!(m.get("role").and_then(|r| r.as_str()), Some("assistant" | "tool"))

    });

    if tools.is_some() && !has_history && CONFIG.get_bool("tool_use_enforcement") {

        let prompt = CONFIG.get_str("tool_use_prompt");

        if !prompt.is_empty() {

            if let Some(first) = messages.first_mut() {

                if first.get("role").and_then(|r| r.as_str()) == Some("system") {

                    let content = first["content"].as_str().unwrap_or("");

                    if !content.contains(&prompt) {

                        first["content"] = json!(format!("{}\n\n{}", prompt, content));

                    }

                } else {

                    messages.insert(0, json!({"role": "system", "content": prompt}));

                }

            }

            info!("Injected tool-use enforcement prompt (first turn)");

        }

    }



    // URL 棰勫彇

    if prefetch_urls && !has_history && web_fetch::has_urls_in_messages(&messages) {

        // 娉ㄦ剰锛歱refetch_urls_into_messages 鏄悓姝ュ嚱鏁帮紝鍦?executor 涓繍琛?

        web_fetch::prefetch_urls_into_messages(&mut messages);

        info!("Pre-fetched URLs into message context (first turn)");

    }



    let model = map_model(data.get("model").and_then(|v| v.as_str()).unwrap_or(""));

    let mut chat = json!({

        "model": model,

        "messages": messages,

        "stream": data.get("stream").and_then(|v| v.as_bool()).unwrap_or(false),

    });



    if let Some(max_tokens) = data.get("max_output_tokens").and_then(|v| v.as_i64()) {

        chat["max_tokens"] = json!(max_tokens);

    } else {

        let cfg_tokens = CONFIG.get_int("max_output_tokens", 16384);

        if cfg_tokens > 0 {

            chat["max_tokens"] = json!(cfg_tokens);

        }

    }



    let cfg_ctx = CONFIG.get_int("max_position_embeddings", 1000000);

    if cfg_ctx > 0 {

        chat["max_position_embeddings"] = json!(cfg_ctx);

    }



    if let Some(temp) = data.get("temperature") {

        chat["temperature"] = temp.clone();

    } else if let Some(cfg_temp) = CONFIG.get_opt_str("temperature") {

        if let Ok(t) = cfg_temp.parse::<f64>() {

            chat["temperature"] = json!(t);

        }

    }



    if let Some(top_p) = data.get("top_p") {

        chat["top_p"] = top_p.clone();

    } else if let Some(cfg_top_p) = CONFIG.get_opt_str("top_p") {

        if let Ok(p) = cfg_top_p.parse::<f64>() {

            chat["top_p"] = json!(p);

        }

    }



    if let Some(stop) = data.get("stop") {

        chat["stop"] = stop.clone();

    }



    // reasoning

    if let Some(reasoning) = data.get("reasoning") {

        match reasoning {

            Value::Object(obj) => {

                if let Some(effort) = obj.get("effort").and_then(|v| v.as_str()) {

                    chat["reasoning_effort"] = json!(effort);

                }

            }

            Value::String(s) => {

                chat["reasoning_effort"] = json!(s);

            }

            _ => {}

        }

    }

    if chat.get("reasoning_effort").is_none() {

        if let Some(ref re) = *config::DEFAULT_REASONING_EFFORT {

            chat["reasoning_effort"] = json!(re);

        } else if let Some(cfg_re) = CONFIG.get_opt_str("reasoning_effort") {

            chat["reasoning_effort"] = json!(cfg_re);

        }

    }



    if let Some(tools) = tools {

        let tool_names: Vec<String> = tools

            .iter()

            .map(|t| {

                t.get("function")

                    .and_then(|f| f.get("name"))

                    .or(t.get("name").or(t.get("type")))

                    .and_then(|v| v.as_str())

                    .unwrap_or("?")

                    .to_string()

            })

            .collect();

        info!("Tool names ({}): {:?}", tools.len(), tool_names);



        let converted = convert_tools(tools);

        chat["tools"] = json!(converted);

        chat["tool_choice"] = data.get("tool_choice").cloned().unwrap_or(json!("auto"));

    }



    // thinking 榛樿鍏抽棴

    if !chat.as_object().unwrap().contains_key("thinking") {

        chat["thinking"] = json!({"type": "disabled"});

        chat.as_object_mut().unwrap().remove("reasoning_effort");

    }



    chat

}



// 鈹€鈹€ Chat 鈫?Responses 杞崲 鈹€鈹€



pub fn chat_to_responses(chat_data: &Value, model: &str) -> Value {

    let resp_id = make_id("resp");

    let choice = chat_data["choices"]

        .as_array()

        .and_then(|arr| arr.first())

        .unwrap_or(&Value::Null);

    let message = choice.get("message").unwrap_or(&Value::Null);

    let content = message.get("content").and_then(|v| v.as_str()).unwrap_or("");

    let reasoning = message.get("reasoning_content").and_then(|v| v.as_str()).unwrap_or("");

    let tool_calls = message.get("tool_calls").and_then(|v| v.as_array());



    let usage = chat_data.get("usage").unwrap_or(&Value::Null);

    let msg_id = make_id("msg");



    let mut output_content = Vec::new();



    if !reasoning.is_empty() {

        output_content.push(json!({

            "type": "reasoning_text",

            "text": reasoning,

            "summary": [],

        }));

    }



    if !content.is_empty() {

        output_content.push(json!({

            "type": "output_text",

            "text": content,

            "annotations": [],

        }));

    } else if reasoning.is_empty() {

        output_content.push(json!({

            "type": "output_text",

            "text": "",

            "annotations": [],

        }));

    }



    if let Some(tc_arr) = tool_calls {

        for tc in tc_arr {

            let func = tc.get("function").unwrap_or(&Value::Null);

            output_content.push(json!({

                "id": tc.get("id").and_then(|v| v.as_str()).unwrap_or(""),

                "type": "function_call",

                "call_id": tc.get("id").and_then(|v| v.as_str()).unwrap_or(""),

                "name": func.get("name").and_then(|v| v.as_str()).unwrap_or(""),

                "arguments": func.get("arguments").and_then(|v| v.as_str()).unwrap_or(""),

            }));

        }

    }



    let output_item = json!({

        "id": msg_id,

        "type": "message",

        "status": "completed",

        "role": "assistant",

        "content": output_content,

    });



    json!({

        "id": resp_id,

        "object": "response",

        "created_at": chat_data.get("created").and_then(|v| v.as_i64()).unwrap_or(0),

        "status": "completed",

        "model": model,

        "output": [output_item],

        "output_text": content,

        "usage": {

            "input_tokens": usage.get("prompt_tokens").and_then(|v| v.as_i64()).unwrap_or(0),

            "output_tokens": usage.get("completion_tokens").and_then(|v| v.as_i64()).unwrap_or(0),

            "total_tokens": usage.get("total_tokens").and_then(|v| v.as_i64()).unwrap_or(0),

            "input_token_details": {

                "cached_tokens": usage.get("prompt_tokens_details").and_then(|v| v.get("cached_tokens")).and_then(|v| v.as_i64()).unwrap_or(0),

            },

            "output_token_details": {

                "reasoning_tokens": usage.get("completion_tokens_details").and_then(|v| v.get("reasoning_tokens")).and_then(|v| v.as_i64()).unwrap_or(0),

            },

        },

        "incomplete_details": null,

        "instructions": null,

        "metadata": {},

    })

}



// 鈹€鈹€ SSE 浜嬩欢鏋勫缓 鈹€鈹€

// 娉ㄦ剰锛氳繖閲岃繑鍥炲師濮?JSON 瀛楃涓诧紝渚?SSE 鍙戝皠鍣ㄤ娇鐢?



pub fn sse_event_json(event_type: &str, data: &Value) -> String {

    let mut payload = data.clone();

    if let Some(obj) = payload.as_object_mut() {

        obj.insert("type".into(), json!(event_type));

    }

    serde_json::to_string(&payload).unwrap_or_default()

}



pub fn sse_event(event_type: &str, kwargs: Value) -> String {

    let mut payload = kwargs;

    if let Some(obj) = payload.as_object_mut() {

        obj.insert("type".into(), json!(event_type));

    }

    serde_json::to_string(&payload).unwrap_or_default()

}



// 鈹€鈹€ Session ID 鎻愬彇 鈹€鈹€



pub fn get_session_id(data: &Value) -> String {

    if let Some(sid) = data.get("prompt_cache_key").and_then(|v| v.as_str()) {

        return sid.to_string();

    }

    if let Some(sid) = data.get("conversation_id").or(data.get("session_id")).and_then(|v| v.as_str()) {

        return sid.to_string();

    }

    if let Some(meta) = data.get("metadata") {

        if let Some(sid) = meta.get("session_id")

            .or(meta.get("conversation_id"))

            .or(meta.get("thread_id"))

            .and_then(|v| v.as_str())

        {

            return sid.to_string();

        }

    }



    let instructions = data.get("instructions").and_then(|v| v.as_str()).unwrap_or("");

    let inp = data.get("input");

    let mut first_user_msg = String::new();

    if let Some(Value::Array(arr)) = inp {

        for item in arr {

            if let Some(role) = item.get("role").and_then(|v| v.as_str()) {

                if role == "user" {

                    first_user_msg = item.get("content").map(|v| v.to_string()).unwrap_or_default();

                    break;

                }

            } else if item.get("type").and_then(|v| v.as_str()) == Some("message")

                && item.get("role").and_then(|v| v.as_str()) == Some("user")

            {

                first_user_msg = item.get("content").map(|v| v.to_string()).unwrap_or_default();

                break;

            }

        }

    } else if let Some(Value::String(s)) = inp {

        first_user_msg = s.clone();

    }



    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();

    hasher.update(instructions.as_bytes());

    let inst_hash = format!("{:x}", hasher.finalize());

    let inst_hash = &inst_hash[..8];



    let seed = format!("{}||{}||{}", inst_hash, first_user_msg, Uuid::new_v4().to_string().replace("-", ""));

    let seed = &seed[..seed.len().min(1000)];



    let mut hasher2 = Sha256::new();

    hasher2.update(seed.as_bytes());

    format!("{:x}", hasher2.finalize())[..16].to_string()

}



// 鈹€鈹€ 妯″瀷鐩存帴鏄犲皠锛堜笉鍥炶惤榛樿锛?鈹€鈹€



pub fn maybe_map_model(name: &str) -> String {

    if name.is_empty() {

        return CONFIG.get_str("default_model");

    }

    let low = name.to_lowercase();

    if low.contains("deepseek") {

        return name.to_string();

    }

    let mapping = CONFIG.get_model_mapping();

    if let Some(mapped) = mapping.get(name) {

        return mapped.clone();

    }

    if let Some(mapped) = mapping.get(&low) {

        return mapped.clone();

    }

    name.to_string()

}// 鍦?protocol.rs 鏈熬闇€瑕佹坊鍔犲嚱鏁颁緵 claude 妯″潡浣跨敤

// 杩藉姞鍒?protocol.rs



// pub pub fn ensure_assistant_reasoning(messages: &mut Vec<Value>, cached_reasoning: &[String]) {

//     ensure_assistant_reasoning(messages, cached_reasoning);

// }

