// Codex CLI 适配：RPC 模拟、模型目录、分析桩。

// 对应 Python: jindx/codex.py



use actix_web::{web, HttpRequest, HttpResponse};

use log::debug;

use serde_json::{json, Value};



use crate::modules::config::CONFIG;



// ── Codex RPC 处理 ──



pub fn handle_codex_rpc(method: &str, params: &Value) -> Option<Value> {

    match method {

        // 速率限制

        "account/rateLimits/read" => Some(json!({

            "method": "account/rateLimits/updated",

            "params": {

                "rateLimits": [],

                "rateLimitsByLimitId": {},

            },

        })),

        // 配置要求

        "config/requirements/read" => Some(json!({

            "method": "config/requirements/updated",

            "params": {"requirements": []},

        })),

        // 模型提供商能力

        "modelProvider/capabilities/read" => Some(json!({

            "method": "modelProvider/capabilities/updated",

            "params": {

                "providers": [{

                    "id": "openai",

                    "capabilities": {

                        "supports_tools": true,

                        "supports_images": true,

                        "supports_streaming": true,

                        "supports_reasoning": true,

                        "max_context_tokens": 131072,

                        "max_output_tokens": 16384,

                    },

                }],

            },

        })),

        // 实验性功能

        "experimentalFeatures/list" => Some(json!({

            "method": "experimentalFeatures/updated",

            "params": {"features": []},

        })),

        // 账户读取

        "account/read" => Some(json!({

            "method": "account/updated",

            "params": {

                "account": {

                    "id": "proxy-user",

                    "email": "proxy@localhost",

                    "plan_type": "plus",

                    "entitled": true,

                },

                "entitlements": {"codex": true, "codex_plus": true},

            },

        })),

        // 模型列表

        "model/list" => {

            let model_name = CONFIG.get_str("default_model");

            Some(json!({

                "method": "model/updated",

                "params": {

                    "models": [{

                        "id": model_name,

                        "name": model_name,

                        "capabilities": {

                            "supports_tools": true,

                            "supports_images": true,

                            "supports_streaming": true,

                            "supports_reasoning": true,

                        },

                    }],

                },

            }))

        }

        // 账户登录

        m if m.starts_with("account/login") => Some(json!({

            "method": "account/login/completed",

            "params": {"status": "authenticated", "account": {"id": "proxy-user"}},

        })),

        // 账户已更新 — 忽略

        "account/updated" => None,

        // MCP/Skills/Device — 返回空列表

        m if m.starts_with("mcpServer/") || m.starts_with("skills/") || m.starts_with("device/") =>

            Some(json!({"method": m.replace("read", "updated").replace("list", "updated"), "params": {}})),

        // Catch-all

        m if m.contains("/read") || m.contains("/list") =>

            Some(json!({"method": m.replace("/read", "/updated").replace("/list", "/updated"), "params": {}})),

        _ => None,

    }

}



// ── Codex 模型目录 ──



fn make_model_entry(

    slug: &str,

    display_name: &str,

    description: &str,

    priority: u32,

    speed_tiers: Vec<&str>,

    reasoning_level: &str,

    reasoning_levels: Option<Vec<Value>>,

) -> Value {

    let rl = reasoning_levels.unwrap_or_else(|| {

        vec![

            json!({"effort": "low", "description": "Fast responses with lighter reasoning"}),

            json!({"effort": "medium", "description": "Balances speed and reasoning depth"}),

            json!({"effort": "high", "description": "Greater reasoning depth for complex problems"}),

        ]

    });



    json!({

        "slug": slug,

        "display_name": display_name,

        "description": description,

        "default_reasoning_level": reasoning_level,

        "default_reasoning_summary": "none",

        "default_verbosity": "low",

        "supported_reasoning_levels": rl,

        "support_verbosity": true,

        "supports_reasoning_summaries": true,

        "supports_image_detail_original": true,

        "supports_parallel_tool_calls": true,

        "supports_search_tool": true,

        "context_window": 272000,

        "max_context_window": 272000,

        "effective_context_window_percent": 95,

        "input_modalities": ["text", "image"],

        "shell_type": "shell_command",

        "visibility": "list",

        "supported_in_api": true,

        "priority": priority,

        "additional_speed_tiers": speed_tiers,

        "apply_patch_tool_type": "freeform",

        "web_search_tool_type": "text_and_image",

        "experimental_supported_tools": [],

        "truncation_policy": {"mode": "tokens", "limit": 10000},

        "upgrade": null,

        "availability_nux": null,

        "base_instructions": "You are Codex, a coding agent.",

        "model_messages": {

            "instructions_template": "You are Codex, a coding agent. You and the user share one workspace, and your job is to collaborate with them until their goal is genuinely handled.",

        },

    })

}



pub async fn codex_models(_req: HttpRequest) -> HttpResponse {

    let default_model = CONFIG.get_str("default_model");

    HttpResponse::Ok().json(json!({

        "models": [

            make_model_entry(

                "gpt-5.5",

                "GPT-5.5",

                "Frontier model for complex coding, research, and real-world work.",

                0,

                vec!["fast"],

                "medium",

                None,

            ),

            make_model_entry(

                "gpt-5",

                "GPT-5",

                "Fast model for everyday tasks",

                1,

                vec![],

                "low",

                Some(vec![

                    json!({"effort": "low", "description": "Fast responses with lighter reasoning"}),

                    json!({"effort": "medium", "description": "Balances speed and reasoning depth"}),

                ]),

            ),

            make_model_entry(

                &default_model,

                &default_model,

                "DeepSeek V4 Pro via JinDX proxy",

                2,

                vec![],

                "medium",

                None,

            ),

        ],

        "default": "gpt-5.5",

    }))

}



pub async fn codex_analytics() -> HttpResponse {

    HttpResponse::Ok().json(json!({"status": "ok"}))

}



pub async fn codex_plugins() -> HttpResponse {

    HttpResponse::Ok().json(json!([]))

}



pub async fn codex_wham() -> HttpResponse {

    HttpResponse::Ok().json(json!({"status": "ok"}))

}



pub async fn codex_backend_fallback(req: HttpRequest) -> HttpResponse {

    let path = req.match_info().get("path").unwrap_or("");

    debug!("Codex backend fallback: /backend-api/{}", path);

    HttpResponse::Ok().json(json!({"status": "ok"}))

}