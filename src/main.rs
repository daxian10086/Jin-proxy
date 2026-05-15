mod modules;

use actix_cors::Cors;
use actix_web::{web, App, HttpServer, HttpRequest, HttpResponse};
use log::{info, LevelFilter};
use modules::admin;
use modules::cache;
use modules::codex;
use modules::config::{self, write_codex_config_toml, write_claude_settings_json, ADMIN_PORT, PROXY_PORT, TLS_PORT, CONNECT_PORT, CONFIG};
use modules::routes;
use modules::tunnel;
use serde_json::{json, Value};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::Builder::new()
        .filter_level(LevelFilter::Info)
        .filter_module("rustls", LevelFilter::Warn)
        .filter_module("hyper", LevelFilter::Warn)
        .filter_module("reqwest", LevelFilter::Warn)
        .init();

    info!("Starting JinDX Proxy (Rust)");
    tunnel::ensure_certs();
    write_codex_config_toml(false);
    write_claude_settings_json(false);
    // 妫€鏌?API Key 鏄惁閰嶇疆
    let key = CONFIG.get_str("deepseek_key");
    if key.is_empty() || key == "sk-your-deepseek-api-key" {
        eprintln!("========================================================");
        eprintln!("  WARNING: DEEPSEEK_KEY not configured!");
        eprintln!("  Set it before starting:");
        eprintln!("    $env:DEEPSEEK_KEY=\"sk-your-key\"");
        eprintln!(r"    .\jin-proxy.exe");
        eprintln!("========================================================");
        eprintln!();
    }

    let proxy_port = *PROXY_PORT;
    tokio::spawn(async move { tunnel::run_connect_server(proxy_port).await; });
    tokio::spawn(async move { cache::memory_cache_cleanup_loop().await; });

    println!();
    println!("========================================================");
    println!("  JinDX Proxy (Rust) | Started");
    println!("    http://127.0.0.1:{}        Admin Panel", *ADMIN_PORT);
    println!("    http://127.0.0.1:{}        API Proxy", *PROXY_PORT);
    println!("    https://127.0.0.1:{}       TLS Proxy", *TLS_PORT);
    println!("========================================================");
    println!();

    let admin_port = *ADMIN_PORT;
    let proxy_port = *PROXY_PORT;

    let admin_server = HttpServer::new(|| {
        let cors = Cors::default().allow_any_origin().allow_any_method().allow_any_header();
        App::new().wrap(cors).app_data(web::JsonConfig::default().limit(2 * 1024 * 1024))
            .route("/admin/health", web::get().to(admin::admin_health))
            .route("/admin", web::get().to(admin::admin_page))
            .route("/admin/config", web::get().to(admin::admin_get_config))
            .route("/admin/config", web::post().to(admin::admin_set_config))
            .route("/admin/stats", web::get().to(admin::admin_stats))
            .route("/admin/sessions", web::get().to(admin::admin_sessions))
            .route("/admin/logs", web::get().to(admin::admin_logs))
            .route("/admin/cache-info", web::get().to(admin::admin_cache_info))
            .route("/admin/cache-clear", web::post().to(admin::admin_cache_clear))
            .route("/admin/proxy-status", web::get().to(admin::admin_proxy_status))
            .route("/admin/proxy-status", web::post().to(admin::admin_proxy_toggle))
            .route("/health", web::get().to(admin::admin_health))
            .route("/", web::get().to(admin::admin_page))
            .route("/config", web::get().to(admin::admin_get_config))
            .route("/config", web::post().to(admin::admin_set_config))
            .route("/stats", web::get().to(admin::admin_stats))
            .route("/sessions", web::get().to(admin::admin_sessions))
            .route("/logs", web::get().to(admin::admin_logs))
            .route("/cache-info", web::get().to(admin::admin_cache_info))
            .route("/cache-clear", web::post().to(admin::admin_cache_clear))
            .route("/proxy-status", web::get().to(admin::admin_proxy_status))
            .route("/proxy-status", web::post().to(admin::admin_proxy_toggle))
    })
    .bind(format!("0.0.0.0:{}", admin_port))?;

    let proxy_server = HttpServer::new(move || {
        let cors = Cors::default().allow_any_origin().allow_any_method().allow_any_header();
        App::new().wrap(cors).app_data(web::JsonConfig::default().limit(2 * 1024 * 1024))
            .route("/v1/chat/completions", web::post().to(routes::chat_completions))
            .route("/chat/completions", web::post().to(routes::chat_completions))
            .route("/v1/responses", web::post().to(routes::responses_http))
            .route("/responses", web::post().to(routes::responses_http))
            .route("/backend-api/codex/responses", web::post().to(routes::responses_http))
            .route("/v1/backend-api/codex/responses", web::post().to(routes::responses_http))
            .route("/v1/responses/compact", web::post().to(routes::responses_compact))
            .route("/responses/compact", web::post().to(routes::responses_compact))
            .route("/v1/models", web::get().to(routes::list_models))
            .route("/models", web::get().to(routes::list_models))
            .route("/health", web::get().to(routes::health))
            .route("/backend-api/codex/models", web::get().to(codex::codex_models))
            .route("/backend-api/models", web::get().to(codex::codex_models))
            .route("/v1/backend-api/codex/models", web::get().to(codex::codex_models))
            .route("/backend-api/codex/analytics-events/events", web::post().to(codex::codex_analytics))
            .route("/backend-api/analytics-events/events", web::post().to(codex::codex_analytics))
            .route("/v1/backend-api/codex/analytics-events/events", web::post().to(codex::codex_analytics))
            .route("/backend-api/plugins/featured", web::get().to(codex::codex_plugins))
            .route("/backend-api/wham/apps", web::post().to(codex::codex_wham))
            .route("/v1/backend-api/wham/apps", web::post().to(codex::codex_wham))
            .route("/backend-api/{path:.*}", web::to(codex::codex_backend_fallback))
            .route("/v1/backend-api/{path:.*}", web::to(codex::codex_backend_fallback))
            .route("/v1/messages", web::post().to(claude_messages_handler))
            .route("/messages", web::post().to(claude_messages_handler))
            .route("/v1/models/claude", web::get().to(claude_models_handler))
    })
    .bind(format!("0.0.0.0:{}", proxy_port))?;

    info!("API proxy listening on http://0.0.0.0:{}", proxy_port);

    let admin_handle = admin_server.run();
    let proxy_handle = proxy_server.run();
    let _ = tokio::join!(admin_handle, proxy_handle);
    Ok(())
}

async fn claude_messages_handler(req: HttpRequest, body: web::Json<Value>) -> HttpResponse {
    let body = body.into_inner();
    modules::stats::record_claude_request();
    let stream = body.get("stream").and_then(|v| v.as_bool()).unwrap_or(false);
    let (chat_request, session_id) = modules::claude::anthropic_to_chat(&body);
    let client = crate::modules::routes::get_http_client().await;

    let base = CONFIG.get_claude_str("deepseek_base");
    let upstream = format!("{}/v1/chat/completions", base);

    let key = {
        let k = CONFIG.get_claude_str("deepseek_key");
        if k.is_empty() { CONFIG.get_str("deepseek_key") } else { k }
    };
    let mut headers_map = reqwest::header::HeaderMap::new();
    headers_map.insert(
        reqwest::header::AUTHORIZATION,
        reqwest::header::HeaderValue::from_str(&format!("Bearer {}", key)).unwrap(),
    );
    headers_map.insert(
        reqwest::header::CONTENT_TYPE,
        reqwest::header::HeaderValue::from_static("application/json"),
    );

    if stream {
        return match client.post(&upstream).json(&chat_request).headers(headers_map).send().await {
            Ok(resp) => {
                if resp.status() != 200 {
                    let status_code = resp.status().as_u16();
                    let body_str = resp.text().await.unwrap_or_default();
                    modules::stats::record_error(status_code);
                    return HttpResponse::BadGateway().body(body_str);
                }
                let byte_stream = resp.bytes_stream();
                let stream = futures_util::StreamExt::map(byte_stream, |item| {
                    item.map(|b| actix_web::web::Bytes::from(b.to_vec()))
                        .map_err(|e| actix_web::error::ErrorBadGateway(format!("{}", e)))
                });
                HttpResponse::Ok()
                    .content_type("text/event-stream")
                    .insert_header(("Cache-Control", "no-cache"))
                    .insert_header(("Connection", "keep-alive"))
                    .streaming(stream)
            }
            Err(e) => {
                if e.is_timeout() { HttpResponse::GatewayTimeout().body("Upstream timeout") }
                else { HttpResponse::BadGateway().body(format!("{}", e)) }
            }
        };
    }

    match client.post(&upstream).json(&chat_request).headers(headers_map).send().await {
        Ok(resp) => {
            if resp.status() != 200 {
                let status_code = resp.status().as_u16();
                let body_str = resp.text().await.unwrap_or_default();
                return HttpResponse::build(
                    actix_web::http::StatusCode::from_u16(status_code).unwrap_or(actix_web::http::StatusCode::BAD_GATEWAY)
                ).body(body_str);
            }
            match resp.json::<Value>().await {
                Ok(chat_data) => {
                    let (claude_response, reasoning_text) = modules::claude::chat_to_anthropic(
                        &chat_data,
                        chat_request["model"].as_str().unwrap_or("deepseek-v4-pro"),
                    );
                    if !reasoning_text.is_empty() {
                        let t = &reasoning_text[..reasoning_text.len().min(8000)];
                        modules::cache::cache_reasoning("claude", &session_id, t);
                        modules::cache::cache_reasoning("claude", "recent", t);
                    }
                    HttpResponse::Ok().json(claude_response)
                }
                Err(e) => HttpResponse::InternalServerError().body(format!("Parse error: {}", e)),
            }
        }
        Err(e) => {
            if e.is_timeout() { HttpResponse::GatewayTimeout().body("Upstream timeout") }
            else { HttpResponse::BadGateway().body(format!("{}", e)) }
        }
    }
}

async fn claude_models_handler() -> HttpResponse {
    let model = CONFIG.get_claude_str("default_model");
    let model = if model.is_empty() { "deepseek-v4-pro".to_string() } else { model };
    HttpResponse::Ok().json(json!({
        "data": [{"id": model, "object": "model", "created": 1750000000, "owned_by": "deepseek"}],
    }))
}
