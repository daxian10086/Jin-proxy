// 绠＄悊 API 鍜?Web 绠＄悊闈㈡澘銆?// 瀵瑰簲 Python: jindx/admin.py

use actix_web::{web, HttpRequest, HttpResponse};
use serde_json::{json, Value};

use crate::modules::cache;
use crate::modules::config::{self, CONFIG, PROXY_PORT, ADMIN_PORT, TLS_PORT, CONNECT_PORT};
use crate::modules::stats;

// -- Admin 璁よ瘉 --

fn get_admin_token() -> String {
    let key = CONFIG.get_claude_str("deepseek_key");
    if !key.is_empty() && key != "sk-your-deepseek-api-key" {
        return key;
    }
    let key = CONFIG.get_str("deepseek_key");
    if key == "sk-your-deepseek-api-key" {
        return String::new();
    }
    key
}

fn check_auth(req: &HttpRequest) -> bool {
    let token = get_admin_token();
    if token.is_empty() {
        return true;
    }
    let auth = req.headers().get("Authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    auth == format!("Bearer {}", token)
}

// -- 绠＄悊 API 绔偣 --

pub async fn admin_health(req: HttpRequest) -> HttpResponse {
    let client = reqwest::Client::new();
    let deepseek_base = CONFIG.get_str("deepseek_base");
    let key = CONFIG.get_str("deepseek_key");
    let ds_ok = match client
        .get(format!("{}/v1/models", deepseek_base))
        .header("Authorization", format!("Bearer {}", key))
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
    {
        Ok(r) => r.status().as_u16() < 500,
        Err(_) => false,
    };

    HttpResponse::Ok().json(json!({
        "status": "ok",
        "deepseek": if ds_ok { "connected" } else { "unreachable" },
        "redis": if cache::is_redis_available() { "connected" } else { "disabled" },
    }))
}

pub async fn admin_page() -> HttpResponse {
    let html = get_admin_html();
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(html)
}

pub async fn admin_get_config(req: HttpRequest) -> HttpResponse {
    if !check_auth(&req) {
        return HttpResponse::Unauthorized().json(json!({"error": "Unauthorized"}));
    }
    let mut cfg = CONFIG.config_dict();
    cfg.insert("PROXY_PORT".into(), json!(*PROXY_PORT));
    cfg.insert("ADMIN_PORT".into(), json!(*ADMIN_PORT));
    cfg.insert("TLS_PORT".into(), json!(*TLS_PORT));
    cfg.insert("CONNECT_PORT".into(), json!(*CONNECT_PORT));
    HttpResponse::Ok().json(cfg)
}

pub async fn admin_set_config(req: HttpRequest, body: web::Json<Value>) -> HttpResponse {
    if !check_auth(&req) {
        return HttpResponse::Unauthorized().json(json!({"error": "Unauthorized"}));
    }
    let updates: std::collections::HashMap<String, Value> = body
        .as_object()
        .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();
    CONFIG.update(&updates);
    HttpResponse::Ok().json(json!({"status": "ok", "config": CONFIG.config_dict()}))
}

pub async fn admin_stats(req: HttpRequest) -> HttpResponse {
    if !check_auth(&req) { return HttpResponse::Unauthorized().json(json!({"error": "Unauthorized"})); }
    HttpResponse::Ok().json(stats::get_stats())
}

pub async fn admin_sessions(req: HttpRequest) -> HttpResponse {
    if !check_auth(&req) { return HttpResponse::Unauthorized().json(json!({"error": "Unauthorized"})); }
    HttpResponse::Ok().json(json!({"memory_sessions": cache::get_memory_sessions_count(), "redis_sessions": 0}))
}

pub async fn admin_logs(req: HttpRequest, query: web::Query<std::collections::HashMap<String, String>>) -> HttpResponse {
    if !check_auth(&req) { return HttpResponse::Unauthorized().json(json!({"error": "Unauthorized"})); }
    let limit: usize = query.get("limit").and_then(|v| v.parse().ok()).unwrap_or(50);
    HttpResponse::Ok().json(json!({"logs": stats::get_logs(limit)}))
}

pub async fn admin_cache_info(req: HttpRequest) -> HttpResponse {
    if !check_auth(&req) { return HttpResponse::Unauthorized().json(json!({"error": "Unauthorized"})); }
    HttpResponse::Ok().json(json!({"cache": cache::get_cache_size_info()}))
}

pub async fn admin_cache_clear(req: HttpRequest, body: web::Json<Value>) -> HttpResponse {
    if !check_auth(&req) { return HttpResponse::Unauthorized().json(json!({"error": "Unauthorized"})); }
    let source = body.get("source").and_then(|v| v.as_str()).unwrap_or("");
    let deleted = cache::clear_cache(source);
    HttpResponse::Ok().json(json!({"status": "ok", "deleted": deleted}))
}

pub async fn admin_proxy_status(req: HttpRequest) -> HttpResponse {
    if !check_auth(&req) { return HttpResponse::Unauthorized().json(json!({"error": "Unauthorized"})); }
    HttpResponse::Ok().json(config::get_proxy_status())
}

pub async fn admin_proxy_toggle(req: HttpRequest, body: web::Json<Value>) -> HttpResponse {
    if !check_auth(&req) { return HttpResponse::Unauthorized().json(json!({"error": "Unauthorized"})); }
    if let Some(enabled) = body.get("codex").and_then(|v| v.as_bool()) {
        if enabled { config::write_codex_config_toml(true); }
        else { config::clear_codex_config_toml(); }
    }
    if let Some(enabled) = body.get("claude").and_then(|v| v.as_bool()) {
        if enabled { config::write_claude_settings_json(true); }
        else { config::clear_claude_settings_json(); }
    }
    HttpResponse::Ok().json(config::get_proxy_status())
}

// -- 鍐呭祵绠＄悊闈㈡澘 HTML (simplified) --

fn get_admin_html() -> String {
    format!(r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>JinDX Proxy Manager</title>
<style>
:root {{ --bg: #f6f8fa; --fg: #1f2328; --border: #d0d7de; --accent: #0969da; --danger: #cf222e; --green: #1a7f37; --input-bg: #fff; --card: #fff; --muted: #656d76; }}
* {{ box-sizing: border-box; margin: 0; padding: 0; }}
body {{ font: 14px/1.6 -apple-system, BlinkMacSystemFont, sans-serif; background: var(--bg); color: var(--fg); min-height: 100vh; }}
#topbar {{ display: flex; justify-content: space-between; align-items: center; padding: 12px 24px; border-bottom: 1px solid var(--border); background: var(--card); }}
#topbar h1 {{ font-size: 20px; color: var(--accent); display: flex; align-items: center; gap: 10px; }}
#topbar h1 .dot {{ width: 8px; height: 8px; border-radius: 50%; background: var(--green); }}
#main {{ display: flex; gap: 20px; padding: 20px 24px; max-width: 1400px; margin: 0 auto; }}
#left {{ flex: 1; min-width: 0; }}
#right {{ width: 420px; flex-shrink: 0; }}
@media (max-width: 900px) {{ #main {{ flex-direction: column; }} #right {{ width: 100%; }} }}
.card {{ background: var(--card); border: 1px solid var(--border); border-radius: 6px; padding: 16px; margin-bottom: 16px; }}
.card h2 {{ font-size: 15px; margin-bottom: 14px; padding-bottom: 8px; border-bottom: 1px solid var(--border); color: var(--accent); }}
.row {{ display: flex; gap: 12px; align-items: center; margin-bottom: 10px; flex-wrap: wrap; }}
.row label {{ min-width: 150px; font-weight: 500; font-size: 13px; }}
.row input, .row select {{ flex: 1; min-width: 180px; background: var(--input-bg); border: 1px solid var(--border); border-radius: 4px; color: var(--fg); padding: 6px 10px; font-size: 13px; }}
.btn {{ padding: 8px 20px; border: 1px solid var(--border); border-radius: 6px; font-size: 13px; font-weight: 600; cursor: pointer; }}
.btn-primary {{ background: #238636; color: #fff; border-color: #238636; }}
.btn-primary:hover {{ background: #2ea043; }}
.btn-secondary {{ background: var(--input-bg); color: var(--fg); }}
#toast {{ position: fixed; top: 16px; right: 16px; padding: 10px 18px; border-radius: 6px; font-size: 13px; font-weight: 500; opacity: 0; transition: opacity 0.25s; z-index: 999; pointer-events: none; }}
#toast.show {{ opacity: 1; }}
#toast.ok {{ background: #238636; color: #fff; }}
#toast.err {{ background: var(--danger); color: #fff; }}
.stat-grid {{ display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }}
.stat-item {{ background: var(--bg); border: 1px solid var(--border); border-radius: 4px; padding: 12px; text-align: center; }}
.stat-value {{ font-size: 24px; font-weight: 700; color: var(--accent); }}
.stat-label {{ font-size: 11px; color: var(--muted); margin-top: 2px; }}
.env-box {{ background: var(--bg); border: 1px solid var(--border); border-radius: 4px; padding: 10px 12px; font-family: monospace; font-size: 12px; line-height: 1.8; color: var(--fg); white-space: pre-wrap; word-break: break-all; }}
</style>
</head>
<body>
<div id="topbar">
  <h1><span class="dot" id="status-dot"></span>JinDX Proxy Manager</h1>
  <span style="font-size:12px;color:var(--muted);">Port: {admin_port}</span>
</div>
<div id="toast"></div>
<div id="main">
<div id="left">
  <div class="card"><h2>Upstream API</h2>
    <div class="row"><label>API Key</label><input id="deepseek_key" type="password" placeholder="sk-..." autocomplete="off"></div>
    <div class="row"><label>Base URL</label><input id="deepseek_base" type="text" placeholder="https://api.deepseek.com"></div>
    <div class="row"><label>Default Model</label><input id="default_model" type="text" placeholder="deepseek-v4-pro"></div>
  </div>
  <div class="card"><h2>Model Mapping</h2>
    <div id="model-rows"></div>
    <button onclick="addModelRow('','')" style="margin-top:6px;font-size:12px;background:none;border:1px dashed var(--border);border-radius:4px;color:var(--accent);cursor:pointer;padding:4px 12px;">+ Add Mapping</button>
  </div>
  <div style="display:flex;gap:10px;margin-top:16px;">
    <button class="btn btn-primary" onclick="saveConfig()">Save Config</button>
    <button class="btn btn-secondary" onclick="loadConfig()">Reload</button>
  </div>
</div>
<div id="right">
  <div class="card"><h2>Status</h2>
    <div class="stat-grid">
      <div class="stat-item"><div class="stat-value" id="stat-uptime">--</div><div class="stat-label">Uptime</div></div>
      <div class="stat-item"><div class="stat-value" id="stat-requests">0</div><div class="stat-label">Requests</div></div>
      <div class="stat-item"><div class="stat-value" id="stat-streams">0</div><div class="stat-label">Active Streams</div></div>
      <div class="stat-item"><div class="stat-value" id="stat-error-rate">0%</div><div class="stat-label">Error Rate</div></div>
    </div>
  </div>
  <div class="card"><h2>Terminal Env</h2>
    <button class="btn btn-primary" onclick="copyEnv('codex')" style="font-size:11px;padding:4px 10px;">Copy Codex CLI</button>
    <button class="btn btn-primary" onclick="copyEnv('claude')" style="font-size:11px;padding:4px 10px;margin-left:4px;">Copy Claude Code</button>
    <div id="env-display" class="env-box" style="margin-top:10px;">Click a button above to see env vars</div>
  </div>
</div>
</div>
<script>
var _configCache={{}};
async function loadConfig(){{
  try{{
    var r=await fetch('/admin/config'), cfg=await r.json();
    _configCache=cfg;
    document.getElementById('deepseek_key').value=cfg.deepseek_key||'';
    document.getElementById('deepseek_base').value=cfg.deepseek_base||'';
    document.getElementById('default_model').value=cfg.default_model||'';
    setModelMapping(cfg.model_mapping);
    updateEnvDisplay();
    toast('Config loaded',true);
  }}catch(e){{ toast('Load failed: '+e,false); }}
}}
async function saveConfig(){{
  var cfg={{
    deepseek_key:document.getElementById('deepseek_key').value.trim(),
    deepseek_base:document.getElementById('deepseek_base').value.trim(),
    default_model:document.getElementById('default_model').value.trim(),
    model_mapping:getModelMapping(),
    enable_reasoning_cache:true, reasoning_cache_ttl:600,
    max_output_tokens:16384, max_position_embeddings:1000000,
    web_fetch_max_urls:5, web_fetch_timeout:10, web_fetch_max_body:80000,
    tool_use_enforcement:true,
  }};
  try{{
    var r=await fetch('/admin/config',{{method:'POST',headers:{{'Content-Type':'application/json'}},body:JSON.stringify(cfg)}});
    if(r.ok){{ toast('Saved & applied',true); loadConfig(); }}
    else{{ var e=await r.json(); toast('Save failed: '+(e.detail||r.status),false); }}
  }}catch(e){{ toast('Save failed: '+e,false); }}
}}
function addModelRow(k,v){{
  var d=document.createElement('div'); d.style.cssText='display:flex;gap:8px;align-items:center;margin-bottom:6px;';
  var ki=document.createElement('input'); ki.placeholder='OpenAI model'; ki.value=k||'';
  var vi=document.createElement('input'); vi.placeholder='DeepSeek model'; vi.value=v||'';
  var del=document.createElement('button'); del.textContent='X'; del.onclick=function(){{d.remove();}};
  del.style.cssText='background:none;border:1px solid var(--border);border-radius:4px;color:var(--danger);cursor:pointer;padding:4px 10px;';
  d.append(ki,vi,del); document.getElementById('model-rows').appendChild(d);
}}
function getModelMapping(){{
  var m={{}}; document.querySelectorAll('#model-rows>div').forEach(function(r){{
    var i=r.querySelectorAll('input');
    if(i[0].value.trim()&&i[1].value.trim()) m[i[0].value.trim()]=i[1].value.trim();
  }}); return m;
}}
function setModelMapping(map){{
  document.getElementById('model-rows').innerHTML='';
  if(map&&Object.keys(map).length) Object.entries(map).forEach(function(e){{addModelRow(e[0],e[1]);}});
}}
function toast(msg,ok){{
  var t=document.getElementById('toast'); t.textContent=msg; t.className=(ok?'ok':'err')+' show';
  setTimeout(function(){{t.classList.remove('show');}},2200);
}}
function updateEnvDisplay(){{
  var key=_configCache.deepseek_key||'sk-your-key';
  var proxyPort=_configCache.PROXY_PORT||'8080';
  document.getElementById('env-display').textContent='Codex CLI:+$env:OPENAI_BASE_URL="http://127.0.0.1:'+proxyPort+'" - $env:OPENAI_API_KEY="'+key+'" - codex - Claude Code: - $env:ANTHROPIC_BASE_URL="http://127.0.0.1:'+proxyPort+'" - $env:ANTHROPIC_API_KEY="'+key+'" - claude';
}}
async function copyEnv(mode){{
  var key=_configCache.deepseek_key||'sk-your-key';
  var proxyPort=_configCache.PROXY_PORT||'8080';
  var text=mode==='codex'?('$env:OPENAI_BASE_URL="http://127.0.0.1:'+proxyPort+'"+$env:OPENAI_API_KEY="'+key+'" - codex'):('$env:ANTHROPIC_BASE_URL="http://127.0.0.1:'+proxyPort+'"+$env:ANTHROPIC_API_KEY="'+key+'" - claude');
  try{{ await navigator.clipboard.writeText(text); toast('Copied to clipboard',true); }}
  catch(e){{ document.getElementById('env-display').textContent=text; toast('Please copy text above',false); }}
}}
function fmtUptime(sec){{
  if(sec<60) return sec+'s';
  if(sec<3600) return Math.floor(sec/60)+'m';
  if(sec<86400) return Math.floor(sec/3600)+'h '+Math.floor((sec%3600)/60)+'m';
  return Math.floor(sec/86400)+'d '+Math.floor((sec%86400)/3600)+'h';
}}
async function refreshStats(){{
  try{{
    var r=await fetch('/admin/stats'), s=await r.json();
    document.getElementById('stat-uptime').textContent=fmtUptime(s.uptime);
    document.getElementById('stat-requests').textContent=s.total_requests;
    document.getElementById('stat-streams').textContent=s.active_streams;
    document.getElementById('stat-error-rate').textContent=s.error_rate+'%';
  }}catch(e){{}}
}}
async function init(){{
  await loadConfig();
  refreshStats();
  setInterval(refreshStats,5000);
}}
init();
</script>
</body>
</html>"#, admin_port = *ADMIN_PORT)
}
