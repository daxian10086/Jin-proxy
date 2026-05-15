// HTTP Routes & WebSocket API endpoints

// 閻庣數鎳撶花?Python: jindx/routes.py



use std::sync::Arc;

use std::time::{SystemTime, UNIX_EPOCH};



use actix_web::{web, HttpRequest, HttpResponse};



use futures_util::StreamExt;

use log::{error, info, warn};

use serde_json::{json, Value};

use tokio::sync::Mutex;



use crate::modules::cache;

use crate::modules::codex::handle_codex_rpc;

use crate::modules::config::{CONFIG, DEEPSEEK_BASE};

use crate::modules::protocol::{self, chat_to_responses, get_session_id, make_id, map_model, maybe_map_model, responses_to_chat, sse_event};

use crate::modules::stats::{self, decrement_active_streams, increment_active_streams, log_error, record_codex_request, record_error, record_upstream_error};

use crate::modules::web_fetch;



// 闁冲厜鍋撻柍鍏夊亾 闁稿繐褰夐棅?HTTP 閻庡箍鍨洪崺娑氱博?闁冲厜鍋撻柍鍏夊亾



lazy_static::lazy_static! {

    static ref HTTP_CLIENT: Mutex<Option<reqwest::Client>> = Mutex::new(None);

}



pub async fn get_http_client() -> reqwest::Client {

    let mut guard = HTTP_CLIENT.lock().await;

    if guard.is_none() {

        let client = reqwest::Client::builder()

            .timeout(std::time::Duration::from_secs(300))

            .pool_max_idle_per_host(20)

            .build()

            .expect("Failed to create HTTP client");

        *guard = Some(client);

    }

    guard.as_ref().unwrap().clone()

}



fn get_auth_headers() -> Vec<(String, String)> {

    vec![

        ("Authorization".into(), format!("Bearer {}", CONFIG.get_str("deepseek_key"))),

        ("Content-Type".into(), "application/json".into()),

    ]

}



fn get_client_auth_headers(req: &HttpRequest) -> Vec<(String, String)> {

    let client_auth = req.headers().get("Authorization")

        .and_then(|v| v.to_str().ok())

        .unwrap_or("");

    let key = if !client_auth.is_empty() {

        client_auth.to_string()

    } else {

        format!("Bearer {}", CONFIG.get_str("deepseek_key"))

    };

    vec![

        ("Authorization".into(), key),

        ("Content-Type".into(), "application/json".into()),

    ]

}



fn get_upstream() -> String {

    format!("{}/v1/chat/completions", CONFIG.get_str("deepseek_base"))

}



// 闁冲厜鍋撻柍鍏夊亾 Chat Completions 闁冲厜鍋撻柍鍏夊亾



pub async fn chat_completions(req: HttpRequest, body: web::Json<Value>) -> HttpResponse {

    record_codex_request();

    let mut body = body.into_inner();

    body["model"] = json!(maybe_map_model(body.get("model").and_then(|v| v.as_str()).unwrap_or("")));



    let stream = body.get("stream").and_then(|v| v.as_bool()).unwrap_or(false);

    let auth_headers = get_client_auth_headers(&req);



    if stream {

        return stream_chat(body, &auth_headers).await;

    }



    let client = get_http_client().await;

    match client

        .post(&get_upstream())

        .json(&body)

        .headers(reqwest::header::HeaderMap::from_iter(

            auth_headers.iter().map(|(k, v)| {

                (reqwest::header::HeaderName::from_bytes(k.as_bytes()).unwrap(),

                 reqwest::header::HeaderValue::from_str(v).unwrap())

            })

        ))

        .send()

        .await

    {

        Ok(resp) => {

            if resp.status() != 200 {

                let status_code = resp.status().as_u16();

                let detail = resp.text().await.unwrap_or_default();

                return HttpResponse::build(actix_web::http::StatusCode::from_u16(status_code).unwrap_or(actix_web::http::StatusCode::BAD_GATEWAY))

                    .body(detail);

            }

            match resp.json::<Value>().await {

                Ok(data) => HttpResponse::Ok().json(data),

                Err(e) => {

                    error!("Failed to parse upstream JSON: {}", e);

                    HttpResponse::InternalServerError().body("Upstream response parse error")

                }

            }

        }

        Err(e) => {

            error!("Chat completions upstream error: {}", e);

            if e.is_timeout() {

                HttpResponse::GatewayTimeout().body("Upstream timeout")

            } else {

                HttpResponse::BadGateway().body(format!("{}", e))

            }

        }

    }

}



async fn stream_chat(body: Value, auth_headers: &[(String, String)]) -> HttpResponse {

    let client = get_http_client().await;

    let mut headers_map = reqwest::header::HeaderMap::new();

    for (k, v) in auth_headers {

        headers_map.insert(

            reqwest::header::HeaderName::from_bytes(k.as_bytes()).unwrap(),

            reqwest::header::HeaderValue::from_str(v).unwrap(),

        );

    }



    match client

        .post(&get_upstream())

        .json(&body)

        .headers(headers_map)

        .send()

        .await

    {

        Ok(resp) => {

            if resp.status() != 200 {

                let status = resp.status().as_u16();

                let body_str = resp.text().await.unwrap_or_default();

                error!("DeepSeek chat/stream {}: {}", status, &body_str[..body_str.len().min(2000)]);

                return HttpResponse::build(

                    actix_web::http::StatusCode::from_u16(status).unwrap_or(actix_web::http::StatusCode::BAD_GATEWAY)

                ).body(body_str);

            }

            let byte_stream = resp.bytes_stream();

            let stream = byte_stream.map(|item| {

                item.map(|bytes| actix_web::web::Bytes::from(bytes.to_vec()))

                    .map_err(|e| actix_web::error::ErrorBadGateway(format!("{}", e)))

            });

            HttpResponse::Ok()

                .content_type("text/event-stream")

                .insert_header(("Cache-Control", "no-cache"))

                .insert_header(("Connection", "keep-alive"))

                .streaming(stream)

        }

        Err(e) => {

            error!("Chat stream connect error: {}", e);

            let error_bytes = actix_web::web::Bytes::from(format!(

                "data: {}\n\n",

                json!({"error": {"message": format!("{}", e), "type": "upstream_error"}})

            ));

            return HttpResponse::BadGateway().content_type("text/event-stream").body(error_bytes);



        }

    }

}



// 闁冲厜鍋撻柍鍏夊亾 Responses HTTP 闁冲厜鍋撻柍鍏夊亾



pub async fn responses_http(req: HttpRequest, body: web::Json<Value>) -> HttpResponse {

    record_codex_request();

    let body = body.into_inner();

    let stream = body.get("stream").and_then(|v| v.as_bool()).unwrap_or(false);

    let model = map_model(body.get("model").and_then(|v| v.as_str()).unwrap_or(""));



    // 閻犲搫鐤囩换鍐矚妤﹁法缈婚柛?

    let inp = body.get("input");

    if inp.map(|v| match v {

        Value::String(s) => s.is_empty(),

        Value::Array(a) => a.is_empty(),

        _ => true,

    }).unwrap_or(true) {

        info!("HTTP skip empty-input request");

        return HttpResponse::Ok().json(json!({

            "id": make_id("resp"),

            "object": "response",

            "created_at": SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),

            "status": "completed",

            "model": model,

            "output": [],

            "output_text": "",

            "usage": {"input_tokens": 0, "output_tokens": 0, "total_tokens": 0},

        }));

    }



    if stream {

        return stream_responses_sse(body).await;

    }



    let chat_request = responses_to_chat(&body, true);

    let client = get_http_client().await;

    let auth_headers = get_auth_headers();

    let mut headers_map = reqwest::header::HeaderMap::new();

    for (k, v) in &auth_headers {

        headers_map.insert(

            reqwest::header::HeaderName::from_bytes(k.as_bytes()).unwrap(),

            reqwest::header::HeaderValue::from_str(v).unwrap(),

        );

    }



    match client.post(&get_upstream()).json(&chat_request).headers(headers_map).send().await {

        Ok(resp) => {

            if resp.status() != 200 {

                let status_code = resp.status().as_u16();

                let body_str = resp.text().await.unwrap_or_default();

                record_error(status_code);

                record_upstream_error(&body_str[..body_str.len().min(2000)]);

                log_error(&format!("DeepSeek non-stream {}: {}", status_code, &body_str[..body_str.len().min(200)]));

                return HttpResponse::build(

                    actix_web::http::StatusCode::from_u16(status_code).unwrap_or(actix_web::http::StatusCode::BAD_GATEWAY)

                ).body(body_str);

            }

            match resp.json::<Value>().await {

                Ok(chat_data) => {

                    let reasoning = chat_data["choices"][0]["message"]["reasoning_content"]

                        .as_str()

                        .unwrap_or("");

                    if !reasoning.is_empty() {

                        cache::cache_reasoning("codex", &get_session_id(&body), reasoning);

                    }

                    HttpResponse::Ok().json(chat_to_responses(&chat_data, &model))

                }

                Err(e) => HttpResponse::InternalServerError().body(format!("Parse error: {}", e)),

            }

        }

        Err(e) => {

            error!("Responses HTTP upstream error: {}", e);

            if e.is_timeout() {

                HttpResponse::GatewayTimeout().body("Upstream timeout")

            } else {

                HttpResponse::BadGateway().body(format!("{}", e))

            }

        }

    }

}



// 闁冲厜鍋撻柍鍏夊亾 SSE 婵炵繝绀佺槐?闁冲厜鍋撻柍鍏夊亾



async fn stream_responses_sse(body: Value) -> HttpResponse {

    increment_active_streams();

    let model = map_model(body.get("model").and_then(|v| v.as_str()).unwrap_or(""));



    let (tx, mut rx) = tokio::sync::mpsc::channel::<Result<actix_web::web::Bytes, std::io::Error>>(100);



    tokio::spawn(async move {

        let result = stream_responses_sse_inner(body, model, &tx).await;

        if let Err(e) = result {

            error!("SSE stream error: {}", e);

        }

        decrement_active_streams();

    });



    let stream = async_stream::stream! {

        while let Some(item) = rx.recv().await {

            yield item;

        }

    };



    HttpResponse::Ok()

        .content_type("text/event-stream")

        .insert_header(("Cache-Control", "no-cache"))

        .insert_header(("Connection", "keep-alive"))

        .insert_header(("X-Accel-Buffering", "no"))

        .streaming(stream)

}



async fn stream_responses_sse_inner(

    body: Value,

    model: String,

    tx: &tokio::sync::mpsc::Sender<Result<actix_web::web::Bytes, std::io::Error>>,

) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {

    let resp_id = make_id("resp");

    let msg_id = make_id("msg");

    let output_index = 0;

    let mut content_index = 0;

    let mut sent_text_parts = false;

    let mut usage = json!({});

    let mut tool_calls_by_index: std::collections::BTreeMap<usize, Value> = std::collections::BTreeMap::new();



    let now_ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();



    // 缂佹柨顑呭畵鍡涘矗閹达腹鍋撴担绋跨仴濠殿喖顑勭花銊︾?

    let _ = tx.send(Ok(actix_web::web::Bytes::from(format!(

        "event: response.created\ndata: {}\n\n",

        sse_event("response.created", json!({"response": {"id": resp_id, "object": "response", "created_at": now_ts, "status": "in_progress", "model": model, "output": []}}))

    )))).await;



    let _ = tx.send(Ok(actix_web::web::Bytes::from(format!(

        "event: response.in_progress\ndata: {}\n\n",

        sse_event("response.in_progress", json!({"response": {"id": resp_id, "object": "response", "status": "in_progress", "model": model}}))

    )))).await;



    let _ = tx.send(Ok(actix_web::web::Bytes::from(format!(

        "event: response.output_item.added\ndata: {}\n\n",

        sse_event("response.output_item.added", json!({"output_index": output_index, "item": {"id": msg_id, "type": "message", "role": "assistant", "status": "in_progress", "content": []}}))

    )))).await;



    let mut chat_request = responses_to_chat(&body, false);

    chat_request["stream"] = json!(true);



    let client = get_http_client().await;

    let auth_headers = get_auth_headers();

    let mut headers_map = reqwest::header::HeaderMap::new();

    for (k, v) in &auth_headers {

        headers_map.insert(

            reqwest::header::HeaderName::from_bytes(k.as_bytes()).unwrap(),

            reqwest::header::HeaderValue::from_str(v).unwrap(),

        );

    }



    let resp = match client

        .post(&get_upstream())

        .json(&chat_request)

        .headers(headers_map)

        .send()

        .await

    {

        Ok(r) => r,

        Err(e) => {

            record_error(500);

            log_error(&format!("SSE stream error: {}", e));

            let _ = tx.send(Ok(actix_web::web::Bytes::from(format!(

                "data: {}\n\n",

                json!({"type": "error", "error": {"message": format!("{}", e)}})

            )))).await;

            return Ok(());

        }

    };



    if resp.status() != 200 {

        let status_code = resp.status().as_u16();

        let body_str = resp.text().await.unwrap_or_default();

        record_error(status_code);

        record_upstream_error(&body_str[..body_str.len().min(2000)]);

        let _ = tx.send(Ok(actix_web::web::Bytes::from(format!(

            "data: {}\n\n",

            json!({"type": "error", "error": {"message": &body_str[..body_str.len().min(1000)], "code": status_code}})

        )))).await;

        return Ok(());

    }



    let mut byte_stream = resp.bytes_stream();

    let mut content_buf = String::new();

    let mut reasoning_buf = String::new();

    let mut buffer = String::new();



    while let Some(chunk_result) = byte_stream.next().await {

        let chunk = match chunk_result {

            Ok(c) => c,

            Err(e) => {

                warn!("SSE stream read error at eof: {}", e);

                break;

            }

        };

        buffer.push_str(&String::from_utf8_lossy(&chunk));



        while let Some(pos) = buffer.find('\n') {

            let line = buffer[..pos].to_string();

            buffer = buffer[pos + 1..].to_string();



            if !line.starts_with("data: ") {

                continue;

            }

            let data_str = &line[6..];

            if data_str == "[DONE]" {

                break;

            }



            let delta: Value = match serde_json::from_str(data_str) {

                Ok(d) => d,

                Err(_) => continue,

            };



            let choices = delta.get("choices").and_then(|v| v.as_array());

            if choices.is_none() || choices.unwrap().is_empty() {

                continue;

            }

            let d = &choices.unwrap()[0].get("delta").unwrap_or(&serde_json::Value::Null);

            let content_delta = d.get("content").and_then(|v| v.as_str()).unwrap_or("");

            let reasoning_delta = d.get("reasoning_content").and_then(|v| v.as_str()).unwrap_or("");



            if let Some(u) = delta.get("usage") {

                usage = u.clone();

            }



            if !reasoning_delta.is_empty() {

                if reasoning_buf.is_empty() && content_buf.is_empty() {

                    let _ = tx.send(Ok(actix_web::web::Bytes::from(format!(

                        "event: response.content_part.added\ndata: {}\n\n",

                        sse_event("response.content_part.added", json!({

                            "item_id": msg_id, "output_index": output_index, "content_index": content_index,

                            "part": {"type": "reasoning_text", "text": "", "summary": []}

                        }))

                    )))).await;

                    sent_text_parts = true;

                }

                reasoning_buf.push_str(reasoning_delta);

                let _ = tx.send(Ok(actix_web::web::Bytes::from(format!(

                    "event: response.reasoning_text.delta\ndata: {}\n\n",

                    sse_event("response.reasoning_text.delta", json!({

                        "item_id": msg_id, "output_index": output_index, "content_index": content_index,

                        "delta": reasoning_delta

                    }))

                )))).await;

            }



            if !content_delta.is_empty() {

                if content_buf.is_empty() && reasoning_buf.is_empty() && !sent_text_parts {

                    let _ = tx.send(Ok(actix_web::web::Bytes::from(format!(

                        "event: response.content_part.added\ndata: {}\n\n",

                        sse_event("response.content_part.added", json!({

                            "item_id": msg_id, "output_index": output_index, "content_index": content_index,

                            "part": {"type": "output_text", "text": ""}

                        }))

                    )))).await;

                    sent_text_parts = true;

                } else if !reasoning_buf.is_empty() && content_buf.is_empty() {

                    content_index += 1;

                    let _ = tx.send(Ok(actix_web::web::Bytes::from(format!(

                        "event: response.content_part.added\ndata: {}\n\n",

                        sse_event("response.content_part.added", json!({

                            "item_id": msg_id, "output_index": output_index, "content_index": content_index,

                            "part": {"type": "output_text", "text": ""}

                        }))

                    )))).await;

                }

                content_buf.push_str(content_delta);

                let _ = tx.send(Ok(actix_web::web::Bytes::from(format!(

                    "event: response.output_text.delta\ndata: {}\n\n",

                    sse_event("response.output_text.delta", json!({

                        "item_id": msg_id, "output_index": output_index, "content_index": content_index,

                        "delta": content_delta

                    }))

                )))).await;

            }



            // 缂侀硸鍨宠ⅶ tool call deltas

            if let Some(tc_deltas) = d.get("tool_calls").and_then(|v| v.as_array()) {

                for tc in tc_deltas {

                    let idx = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

                    let entry = tool_calls_by_index.entry(idx).or_insert(json!({

                        "id": "", "name": "", "arguments": "",

                    }));

                    if let Some(id) = tc.get("id").and_then(|v| v.as_str()) { entry["id"] = json!(id); }

                    if let Some(func) = tc.get("function") {

                        if let Some(name) = func.get("name").and_then(|v| v.as_str()) { entry["name"] = json!(name); }

                        if let Some(args) = func.get("arguments").and_then(|v| v.as_str()) {

                            entry["arguments"] = json!(format!("{}{}", entry["arguments"].as_str().unwrap_or(""), args));

                        }

                    }

                }

            }

        }

    }



    // 闁告瑦鍨块埀顑跨劍濞撳墎绱掗崼婊呯殤濞?

    let display_text = if !content_buf.is_empty() { &content_buf } else { &reasoning_buf };

    let final_content = if display_text.is_empty() {

        vec![]

    } else {

        vec![json!({"type": "output_text", "text": display_text, "annotations": []})]

    };



    if sent_text_parts {

        let part_type = if !content_buf.is_empty() { "output_text" } else { "reasoning_text" };

        let _ = tx.send(Ok(actix_web::web::Bytes::from(format!(

            "event: response.content_part.done\ndata: {}\n\n",

            sse_event("response.content_part.done", json!({

                "item_id": msg_id, "output_index": output_index, "content_index": content_index,

                "part": {"type": part_type, "text": display_text}

            }))

        )))).await;

    }



    let _ = tx.send(Ok(actix_web::web::Bytes::from(format!(

        "event: response.output_item.done\ndata: {}\n\n",

        sse_event("response.output_item.done", json!({

            "output_index": output_index,

            "item": {"id": msg_id, "type": "message", "role": "assistant", "status": "completed", "content": final_content}

        }))

    )))).await;



    let mut all_output_items: Vec<Value> = vec![

        json!({"id": msg_id, "type": "message", "role": "assistant", "status": "completed", "content": final_content})

    ];



    for (ti, tc) in tool_calls_by_index.iter() {

        let tc_id = if tc["id"].as_str().map(|s| !s.is_empty()).unwrap_or(false) {

            tc["id"].as_str().unwrap().to_string()

        } else {

            make_id("call")

        };

        let tc_name = tc["name"].as_str().unwrap_or("");

        let tc_args = tc["arguments"].as_str().unwrap_or("{}");

        let tc_out_idx = output_index + ti + 1;



        let _ = tx.send(Ok(actix_web::web::Bytes::from(format!(

            "event: response.output_item.added\ndata: {}\n\n",

            sse_event("response.output_item.added", json!({

                "output_index": tc_out_idx,

                "item": {"id": tc_id, "type": "function_call", "name": tc_name, "call_id": tc_id, "status": "in_progress", "arguments": ""}

            }))

        )))).await;

        let _ = tx.send(Ok(actix_web::web::Bytes::from(format!(

            "event: response.function_call_arguments.delta\ndata: {}\n\n",

            sse_event("response.function_call_arguments.delta", json!({"item_id": tc_id, "output_index": tc_out_idx, "delta": tc_args}))

        )))).await;

        let _ = tx.send(Ok(actix_web::web::Bytes::from(format!(

            "event: response.function_call_arguments.done\ndata: {}\n\n",

            sse_event("response.function_call_arguments.done", json!({"item_id": tc_id, "output_index": tc_out_idx, "arguments": tc_args}))

        )))).await;

        let _ = tx.send(Ok(actix_web::web::Bytes::from(format!(

            "event: response.output_item.done\ndata: {}\n\n",

            sse_event("response.output_item.done", json!({

                "output_index": tc_out_idx,

                "item": {"id": tc_id, "type": "function_call", "name": tc_name, "call_id": tc_id, "status": "completed", "arguments": tc_args}

            }))

        )))).await;



        all_output_items.push(json!({

            "id": tc_id, "type": "function_call", "call_id": tc_id,

            "name": tc_name, "arguments": tc_args, "status": "completed",

        }));

    }



    let _ = tx.send(Ok(actix_web::web::Bytes::from(format!(

        "event: response.completed\ndata: {}\n\n",

        sse_event("response.completed", json!({

            "response": {

                "id": resp_id, "object": "response", "created_at": now_ts,

                "status": "completed", "model": model,

                "output": all_output_items,

                "output_text": display_text,

                "usage": {

                    "input_tokens": usage.get("prompt_tokens").and_then(|v| v.as_i64()).unwrap_or(0),

                    "output_tokens": usage.get("completion_tokens").and_then(|v| v.as_i64()).unwrap_or(0),

                    "total_tokens": usage.get("total_tokens").and_then(|v| v.as_i64()).unwrap_or(0),

                },

            }

        }))

    )))).await;

    let _ = tx.send(Ok(actix_web::web::Bytes::from("data: [DONE]\n\n"))).await;



    if !reasoning_buf.is_empty() {

        cache::cache_reasoning("codex", &get_session_id(&body), &reasoning_buf);

    }



    Ok(())

}



// 闁冲厜鍋撻柍鍏夊亾 Responses Compact 闁冲厜鍋撻柍鍏夊亾



pub async fn responses_compact(req: HttpRequest, body: web::Json<Value>) -> HttpResponse {

    let body = body.into_inner();

    let inp = body.get("input").and_then(|v| v.as_array()).cloned().unwrap_or_default();

    let instructions = body.get("instructions")

        .or(body.get("system_message"))

        .and_then(|v| v.as_str())

        .unwrap_or("")

        .to_string();

    let model_name = map_model(body.get("model").and_then(|v| v.as_str()).unwrap_or(""));



    let conv_text = format_conversation_for_compact(&inp);



    let summary_messages = json!([

        {"role": "system", "content": instructions},

        {"role": "user", "content": format!(

            "Please compress and summarize the following conversation history. Keep all key information, decisions and code changes, but remove redundant intermediate steps and repetitive content. Output the summary in English:\n\n{}",

            conv_text

        )},

    ]);



    let mut chat_request = json!({

        "model": model_name,

        "messages": summary_messages,

        "stream": false,

    });

    if let Some(max_tokens) = body.get("max_output_tokens").and_then(|v| v.as_i64()) {

        chat_request["max_tokens"] = json!(max_tokens);

    }



    let client = get_http_client().await;

    let auth_headers = get_auth_headers();

    let mut headers_map = reqwest::header::HeaderMap::new();

    for (k, v) in &auth_headers {

        headers_map.insert(

            reqwest::header::HeaderName::from_bytes(k.as_bytes()).unwrap(),

            reqwest::header::HeaderValue::from_str(v).unwrap(),

        );

    }



    match client.post(&get_upstream()).json(&chat_request).headers(headers_map).send().await {

        Ok(resp) => {

            if resp.status() != 200 {

                let fallback = if inp.len() > 20 { inp[inp.len()-20..].to_vec() } else { inp };

                return HttpResponse::Ok().json(json!({"output": fallback, "compacted_input": fallback}));

            }

            let chat_data: Value = match resp.json().await {

                Ok(d) => d,

                Err(_) => {

                    let fallback = if inp.len() > 20 { inp[inp.len()-20..].to_vec() } else { inp };

                    return HttpResponse::Ok().json(json!({"output": fallback, "compacted_input": fallback}));

                }

            };

            let summary_text = chat_data["choices"][0]["message"]["content"]

                .as_str()

                .unwrap_or("");



            let mut compacted = Vec::new();

            if !instructions.is_empty() {

                compacted.push(json!({"type": "message", "role": "developer", "content": [{"type": "input_text", "text": instructions}]}));

            }

            compacted.push(json!({"type": "message", "role": "developer", "content": [{"type": "input_text", "text": format!("[Conversation History Summary]\n{}", summary_text)}]}));



            let keep_tail = 6.min(inp.len());

            if keep_tail > 0 {

                compacted.extend(inp[inp.len()-keep_tail..].to_vec());

            }

            info!("Compact done: {} items -> {} items", inp.len(), compacted.len());

            HttpResponse::Ok().json(json!({"output": compacted, "compacted_input": compacted}))

        }

        Err(_) => {

            let fallback = if inp.len() > 20 { inp[inp.len()-20..].to_vec() } else { inp };

            HttpResponse::Ok().json(json!({"output": fallback, "compacted_input": fallback}))

        }

    }

}



fn format_conversation_for_compact(inp: &[Value]) -> String {

    let mut lines = Vec::new();

    for item in inp {

        let role = item.get("role").and_then(|v| v.as_str()).unwrap_or("");

        let itype = item.get("type").and_then(|v| v.as_str()).unwrap_or("");

        let content = item.get("content");



        if itype == "function_call" {

            let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");

            let args = item.get("arguments").and_then(|v| v.as_str()).unwrap_or("");

            let args_short = if args.len() > 200 { format!("{}...", &args[..200]) } else { args.to_string() };

            lines.push(format!("[Tool Call] {}({})", name, args_short));

        } else if itype == "function_call_output" {

            let call_id = item.get("call_id").and_then(|v| v.as_str()).unwrap_or("");

            let output = item.get("output").map(|v| v.to_string()).unwrap_or_default();

            let output_short = if output.len() > 300 { format!("{}...", &output[..300]) } else { output };

            lines.push(format!("[Tool Result {}] {}", call_id, output_short));

        } else if !role.is_empty() || itype == "message" {

            let role_label = match role {

                "user" => "User", "assistant" => "Assistant",

                "developer" => "System", "system" => "System",

                "tool" => "Tool",

                _ => role,

            };

            let content_str = match content {

                Some(Value::Array(parts)) => {

                    parts.iter().map(|p| {

                        let pt = p.get("type").and_then(|v| v.as_str()).unwrap_or("");

                        let t = p.get("text").and_then(|v| v.as_str()).unwrap_or("");

                        if pt == "reasoning_text" {

                            if t.len() > 200 { format!("[Think: {}...]", &t[..200]) }

                            else { format!("[Think: {}]", t) }

                        } else {

                            if t.len() > 500 { format!("{}...", &t[..500]) } else { t.to_string() }

                        }

                    }).collect::<Vec<_>>().join("\n")

                }

                Some(Value::String(s)) => {

                    if s.len() > 800 { format!("{}...", &s[..800]) } else { s.clone() }

                }

                _ => String::new(),

            };

            lines.push(format!("[{}] {}", role_label, content_str));

        }

    }

    lines.join("\n\n")

}



// 闁冲厜鍋撻柍鍏夊亾 婵☆垪鈧磭鈧兘宕氬Δ鍕┾偓?& 闁稿鍎遍幃宥呂涢埀顒勫蓟?闁冲厜鍋撻柍鍏夊亾



pub async fn list_models() -> HttpResponse {

    let default_model = CONFIG.get_str("default_model");

    HttpResponse::Ok().json(json!({

        "object": "list",

        "data": [

            {"id": "gpt-5.5", "object": "model", "created": 1750000000, "owned_by": "system"},

            {"id": "gpt-5", "object": "model", "created": 1750000000, "owned_by": "system"},

            {"id": default_model, "object": "model", "created": 1750000000, "owned_by": "deepseek"},

            {"id": "deepseek-v4-flash", "object": "model", "created": 1750000000, "owned_by": "deepseek"},

        ],

    }))

}



pub async fn health() -> HttpResponse {

    let client = get_http_client().await;

    let auth_headers = get_auth_headers();

    let mut headers_map = reqwest::header::HeaderMap::new();

    for (k, v) in &auth_headers {

        headers_map.insert(

            reqwest::header::HeaderName::from_bytes(k.as_bytes()).unwrap(),

            reqwest::header::HeaderValue::from_str(v).unwrap(),

        );

    }



    let upstream = match client

        .get(format!("{}/v1/models", CONFIG.get_str("deepseek_base")))

        .headers(headers_map)

        .send()

        .await

    {

        Ok(r) => if r.status().as_u16() < 500 { "ok" } else { "error" },

        Err(_) => "unreachable",

    };



    HttpResponse::Ok().json(json!({

        "status": "ok",

        "target": get_upstream(),

        "upstream": upstream,

        "cache": {"backend": "file"},

    }))

}// 閺夆晞妫勬慨鐐哄礆?routes.rs 闁哄牜鍋勯悢顒勬晬鐏炶棄褰嗙€殿喒鍋?HTTP 閻庡箍鍨洪崺娑氱博椤栨瑤绨板〒?main.rs 濞?Claude handler 濞达綀娉曢弫?





