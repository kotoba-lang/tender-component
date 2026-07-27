//! Native micro-TCB Component host.
//!
//! The stdin/stdout protocol is deliberately tiny:
//!   1. Clojure sends one run envelope.
//!   2. each imported WIT function yields one provider-call envelope;
//!      Clojure validates and invokes the admitted provider, then responds.
//!   3. the host emits one terminal result or error envelope.
//!
//! No WASI linker is installed.  Therefore a Component receives only the
//! separately named aiueos imports carried in its admitted envelope.

use anyhow::{Context, Result, anyhow, bail};
use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, BufWriter, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use wasmtime::component::{Component, Linker, Resource, ResourceType, Val};
use wasmtime::{Config, Engine, Store, StoreLimits, StoreLimitsBuilder};

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Ability {
    target: String,
    operation: String,
    #[serde(rename = "max-bytes")]
    max_bytes: u64,
    #[serde(rename = "max-items")]
    max_items: u64,
    #[serde(rename = "deadline-ms")]
    deadline_ms: u64,
    #[serde(rename = "audit-id")]
    audit_id: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Import {
    name: String,
    ability: Ability,
}

#[derive(Debug, Clone, Deserialize)]
struct Run {
    #[serde(rename = "type")]
    kind: String,
    component: String,
    #[serde(rename = "capability-mode", default = "default_capability_mode")]
    capability_mode: String,
    imports: Vec<Import>,
    fuel: u64,
    #[serde(rename = "memory-pages")]
    memory_pages: u64,
}

fn default_capability_mode() -> String {
    "function".into()
}

struct Protocol {
    input: BufReader<io::Stdin>,
    output: BufWriter<io::Stdout>,
}

struct State {
    protocol: Option<Arc<Mutex<Protocol>>>,
    limits: StoreLimits,
}

fn allowed_binding(name: &str) -> Option<(&'static str, &'static str, &'static str)> {
    match name {
        "aiueos-clock-now" => Some(("kotoba:application/clock@1.0.0", "now", "clock/now")),
        "aiueos-log-append" => Some(("kotoba:application/log@1.0.0", "append", "log/append")),
        _ => None,
    }
}

fn validate_ability(name: &str, ability: &Ability) -> Result<()> {
    let Some((_, _, expected_operation)) = allowed_binding(name) else {
        bail!("unrecognized aiueos Component import: {name}");
    };
    if ability.operation != expected_operation {
        bail!("import {name} is bound to an invalid operation");
    }
    if ability.target.is_empty()
        || ability.audit_id.is_empty()
        || ability.max_bytes == 0
        || ability.max_items == 0
        || ability.deadline_ms == 0
    {
        bail!("import {name} has an unbounded or incomplete ability");
    }
    Ok(())
}

fn send(protocol: &mut Protocol, value: &Value) -> Result<()> {
    serde_json::to_writer(&mut protocol.output, value)?;
    protocol.output.write_all(b"\n")?;
    protocol.output.flush()?;
    Ok(())
}

fn provider_call(state: &mut State, name: &str, ability: &Ability, value: i64) -> Result<i64> {
    // The descriptor is captured while linking, never supplied by the guest.
    validate_ability(name, ability)?;
    let protocol = state
        .protocol
        .as_ref()
        .ok_or_else(|| anyhow!("resident provider-free host has no provider protocol"))?;
    let mut protocol = protocol
        .lock()
        .map_err(|_| anyhow!("protocol lock poisoned"))?;
    send(
        &mut protocol,
        &json!({
            "type": "provider-call",
            "import": name,
            "ability": ability,
            "payload": { "value": value }
        }),
    )?;
    let mut response = String::new();
    if protocol.input.read_line(&mut response)? == 0 {
        bail!("provider closed protocol before responding");
    }
    let response: Value = serde_json::from_str(&response).context("invalid provider response")?;
    if response.get("type") != Some(&Value::String("provider-result".into())) {
        bail!("expected provider-result response");
    }
    if response.get("import") != Some(&Value::String(name.into())) {
        bail!("provider response import does not match request");
    }
    response
        .get("value")
        .and_then(Value::as_i64)
        .ok_or_else(|| anyhow!("provider response must contain an i64 value"))
}

// With Wasmtime's deliberately minimal feature set its error type does not
// implement `std::error::Error`. Keep that boundary explicit rather than
// enabling unrelated Wasmtime features merely to use `anyhow` conversion.
fn wasmtime_result<T>(result: std::result::Result<T, wasmtime::Error>, context: &str) -> Result<T> {
    result.map_err(|error| anyhow!("{context}: {error}"))
}

fn run(request: Run, protocol: Option<Arc<Mutex<Protocol>>>) -> Result<i64> {
    if request.kind != "run" {
        bail!("first protocol envelope must be a run request");
    }
    let mut imports = BTreeMap::new();
    for import in request.imports {
        validate_ability(&import.name, &import.ability)?;
        if imports.insert(import.name, import.ability).is_some() {
            bail!("duplicate Component import");
        }
    }

    let mut config = Config::new();
    config.wasm_component_model(true);
    config.consume_fuel(true);
    let engine = wasmtime_result(Engine::new(&config), "cannot create Component engine")?;
    let component = wasmtime_result(
        Component::from_file(&engine, &request.component),
        "cannot compile admitted Component",
    )?;
    let limits = StoreLimitsBuilder::new()
        .memory_size(request.memory_pages.saturating_mul(65536) as usize)
        .build();
    let mut store = Store::new(&engine, State { protocol, limits });
    store.limiter(|state| &mut state.limits);
    wasmtime_result(store.set_fuel(request.fuel), "cannot set Component fuel")?;

    let mut linker = Linker::<State>::new(&engine);
    for (name, ability) in imports {
        let import_name = name.clone();
        let (interface, function, _) = allowed_binding(&name)
            .ok_or_else(|| anyhow!("unrecognized aiueos Component import: {name}"))?;
        let mut instance = wasmtime_result(
            linker.instance(interface),
            "cannot create admitted Component interface",
        )?;
        if request.capability_mode == "linear-resource" {
            let resource_name = format!("{function}-capability");
            wasmtime_result(
                instance.resource(&resource_name, ResourceType::host::<()>(), |_, _| Ok(())),
                "cannot bind linear capability resource",
            )?;
            let issue_name = format!("issue-{function}");
            wasmtime_result(
                instance.func_wrap(&issue_name, move |_cx, (): ()| {
                    Ok((Resource::<()>::new_own(1),))
                }),
                "cannot bind linear capability issuer",
            )?;
            let execute_name = format!("execute-{function}");
            wasmtime_result(
                instance.func_wrap(
                    &execute_name,
                    move |mut cx, (_cap, value): (Resource<()>, i64)| {
                        provider_call(cx.data_mut(), &import_name, &ability, value)
                            .map(|result| (result,))
                            .map_err(|error| wasmtime::Error::msg(error.to_string()))
                    },
                ),
                "cannot bind linear capability consumer",
            )?;
        } else {
            wasmtime_result(
                instance.func_wrap(function, move |mut cx, (value,): (i64,)| {
                    provider_call(cx.data_mut(), &import_name, &ability, value)
                        .map(|result| (result,))
                        .map_err(|error| wasmtime::Error::msg(error.to_string()))
                }),
                "cannot bind admitted Component import",
            )?;
        }
    }
    let instance = wasmtime_result(
        linker.instantiate(&mut store, &component),
        "Component imports did not match the admitted bindings",
    )?;
    let function = instance
        .get_func(&mut store, "main")
        .ok_or_else(|| anyhow!("Component does not export main"))?;
    let mut results = [Val::S64(0)];
    wasmtime_result(
        function.call(&mut store, &[], &mut results),
        "Component main failed",
    )?;
    match results.into_iter().next() {
        Some(Val::S64(value)) => Ok(value),
        _ => bail!("Component main must return s64"),
    }
}

struct ResidentConfig {
    bind: SocketAddr,
    component: PathBuf,
    component_cid: String,
    component_sha256: String,
    expected_result: i64,
    fuel: u64,
    memory_pages: u64,
    node: String,
    receipt_log: PathBuf,
    signing_key: SigningKey,
}

fn required_env(name: &str) -> Result<String> {
    let value = env::var(name).with_context(|| format!("{name} is required"))?;
    if value.is_empty() {
        bail!("{name} must not be empty");
    }
    Ok(value)
}

fn parse_positive_env(name: &str) -> Result<u64> {
    let value = required_env(name)?;
    let parsed = value
        .parse::<u64>()
        .with_context(|| format!("{name} must be a positive integer"))?;
    if parsed == 0 {
        bail!("{name} must be positive");
    }
    Ok(parsed)
}

fn sha256_file(path: &Path) -> Result<String> {
    let bytes =
        fs::read(path).with_context(|| format!("cannot read Component {}", path.display()))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn read_signing_key(path: &Path) -> Result<SigningKey> {
    let encoded = fs::read_to_string(path)
        .with_context(|| format!("cannot read receipt signing key {}", path.display()))?;
    let bytes = hex::decode(encoded.trim()).context("receipt signing key is not hex")?;
    let seed: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow!("receipt signing key must contain exactly 32 bytes"))?;
    Ok(SigningKey::from_bytes(&seed))
}

fn resident_config() -> Result<ResidentConfig> {
    let bind = env::var("KOTOTAMA_BIND_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:18901".into())
        .parse::<SocketAddr>()
        .context("KOTOTAMA_BIND_ADDR must be a socket address")?;
    if !bind.ip().is_loopback() {
        bail!("resident Component host must bind a loopback address");
    }
    let component = PathBuf::from(required_env("KOTOTAMA_COMPONENT_PATH")?);
    if !component.is_absolute() || !component.is_file() {
        bail!("KOTOTAMA_COMPONENT_PATH must be an absolute file");
    }
    let expected_sha256 = required_env("KOTOTAMA_COMPONENT_SHA256")?;
    if expected_sha256.len() != 64 || !expected_sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("KOTOTAMA_COMPONENT_SHA256 must be lowercase SHA-256 hex");
    }
    let actual_sha256 = sha256_file(&component)?;
    if actual_sha256 != expected_sha256 {
        bail!("resident Component bytes do not match KOTOTAMA_COMPONENT_SHA256");
    }
    let seed_path = PathBuf::from(required_env("KOTOTAMA_RECEIPT_SEED_PATH")?);
    let receipt_log = PathBuf::from(required_env("KOTOTAMA_RECEIPT_LOG")?);
    if !seed_path.is_absolute() || !receipt_log.is_absolute() {
        bail!("receipt key and log paths must be absolute");
    }
    Ok(ResidentConfig {
        bind,
        component,
        component_cid: required_env("KOTOTAMA_COMPONENT_CID")?,
        component_sha256: expected_sha256,
        expected_result: required_env("KOTOTAMA_EXPECTED_RESULT")?
            .parse::<i64>()
            .context("KOTOTAMA_EXPECTED_RESULT must be i64")?,
        fuel: parse_positive_env("KOTOTAMA_FUEL")?,
        memory_pages: parse_positive_env("KOTOTAMA_MEMORY_PAGES")?,
        node: required_env("KOTOTAMA_NODE")?,
        receipt_log,
        signing_key: read_signing_key(&seed_path)?,
    })
}

fn now_ms() -> Result<u128> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before Unix epoch")?
        .as_millis())
}

fn receipt_envelope(body: Value, signing_key: &SigningKey) -> Result<Value> {
    let canonical = serde_json::to_vec(&body)?;
    let signature = signing_key.sign(&canonical);
    let payload = String::from_utf8(canonical)
        .map_err(|_| anyhow!("canonical receipt payload is not UTF-8"))?;
    Ok(json!({
        "algorithm": "ed25519",
        "body": body,
        "payload": payload,
        "signature": hex::encode(signature.to_bytes())
    }))
}

fn persist_receipt(path: &Path, receipt: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("cannot open receipt log {}", path.display()))?;
    serde_json::to_writer(&mut file, receipt)?;
    file.write_all(b"\n")?;
    file.flush()?;
    file.sync_data()?;
    Ok(())
}

fn execute_resident(config: &ResidentConfig) -> Result<Value> {
    let started_at_ms = now_ms()?;
    let result = run(
        Run {
            kind: "run".into(),
            component: config.component.to_string_lossy().into_owned(),
            capability_mode: "function".into(),
            imports: vec![],
            fuel: config.fuel,
            memory_pages: config.memory_pages,
        },
        None,
    )?;
    let finished_at_ms = now_ms()?;
    if result != config.expected_result {
        bail!("Component result does not match admitted expected result");
    }
    let public_key = hex::encode(config.signing_key.verifying_key().to_bytes());
    let body = json!({
        "ambient-wasi": false,
        "component-cid": config.component_cid,
        "component-sha256": config.component_sha256,
        "expected-result": config.expected_result,
        "finished-at-ms": finished_at_ms,
        "format": "kototama.component-execution-receipt/v1",
        "fuel": config.fuel,
        "memory-pages": config.memory_pages,
        "node": config.node,
        "receipt-public-key": public_key,
        "result": result,
        "runtime": "tender-component/0.1.0+wasmtime-42.0.1",
        "started-at-ms": started_at_ms,
        "status": "ok"
    });
    let receipt = receipt_envelope(body, &config.signing_key)?;
    persist_receipt(&config.receipt_log, &receipt)?;
    Ok(receipt)
}

fn read_http_request(stream: &mut TcpStream) -> Result<(String, String)> {
    stream.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
    let mut bytes = Vec::new();
    let mut one = [0u8; 1];
    while bytes.len() < 16 * 1024 {
        if stream.read(&mut one)? == 0 {
            break;
        }
        bytes.push(one[0]);
        if bytes.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    if !bytes.ends_with(b"\r\n\r\n") {
        bail!("HTTP header is incomplete or too large");
    }
    let text = std::str::from_utf8(&bytes).context("HTTP header is not UTF-8")?;
    let mut lines = text.split("\r\n");
    let mut request = lines.next().unwrap_or_default().split_whitespace();
    let method = request.next().unwrap_or_default().to_owned();
    let path = request.next().unwrap_or_default().to_owned();
    if request.next() != Some("HTTP/1.1") {
        bail!("only HTTP/1.1 is accepted");
    }
    for line in lines {
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("content-length:") {
            let length = line.split_once(':').map(|(_, v)| v.trim()).unwrap_or("");
            if length != "0" {
                bail!("request bodies are not accepted");
            }
        }
    }
    Ok((method, path))
}

fn write_http_json(stream: &mut TcpStream, status: &str, body: &Value) -> Result<()> {
    let bytes = serde_json::to_vec(body)?;
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        bytes.len()
    )?;
    stream.write_all(&bytes)?;
    stream.flush()?;
    Ok(())
}

fn health(config: &ResidentConfig) -> Value {
    json!({
        "ambient-wasi": false,
        "component-cid": config.component_cid,
        "component-sha256": config.component_sha256,
        "node": config.node,
        "ready": true,
        "receipt-public-key": hex::encode(config.signing_key.verifying_key().to_bytes()),
        "runtime": "tender-component/0.1.0+wasmtime-42.0.1"
    })
}

fn handle_http(mut stream: TcpStream, config: &ResidentConfig) -> Result<()> {
    let request = read_http_request(&mut stream);
    match request {
        Ok((method, path)) if method == "GET" && path == "/healthz" => {
            write_http_json(&mut stream, "200 OK", &health(config))
        }
        Ok((method, path)) if method == "POST" && path == "/v1/run" => {
            match execute_resident(config) {
                Ok(receipt) => write_http_json(&mut stream, "200 OK", &receipt),
                Err(error) => write_http_json(
                    &mut stream,
                    "500 Internal Server Error",
                    &json!({"error": error.to_string(), "ok": false}),
                ),
            }
        }
        Ok(_) => write_http_json(
            &mut stream,
            "404 Not Found",
            &json!({"error": "not found", "ok": false}),
        ),
        Err(error) => write_http_json(
            &mut stream,
            "400 Bad Request",
            &json!({"error": error.to_string(), "ok": false}),
        ),
    }
}

fn serve_resident() -> Result<()> {
    let config = resident_config()?;
    // Compile and execute once before advertising readiness.
    execute_resident(&config).context("resident Component startup canary failed")?;
    let listener =
        TcpListener::bind(config.bind).with_context(|| format!("cannot bind {}", config.bind))?;
    eprintln!(
        "{}",
        json!({
            "bind": config.bind.to_string(),
            "component-cid": config.component_cid,
            "node": config.node,
            "ready": true
        })
    );
    for connection in listener.incoming() {
        match connection {
            Ok(stream) => {
                if let Err(error) = handle_http(stream, &config) {
                    eprintln!("resident request failed: {error:#}");
                }
            }
            Err(error) => eprintln!("resident accept failed: {error}"),
        }
    }
    Ok(())
}

fn main() {
    if env::args().nth(1).as_deref() == Some("--serve") {
        if let Err(error) = serve_resident() {
            eprintln!("resident Component host failed: {error:#}");
            std::process::exit(1);
        }
        return;
    }
    let protocol = Arc::new(Mutex::new(Protocol {
        input: BufReader::new(io::stdin()),
        output: BufWriter::new(io::stdout()),
    }));
    let outcome = (|| -> Result<i64> {
        let mut line = String::new();
        {
            let mut locked = protocol
                .lock()
                .map_err(|_| anyhow!("protocol lock poisoned"))?;
            if locked.input.read_line(&mut line)? == 0 {
                bail!("missing run envelope");
            }
        }
        run(
            serde_json::from_str(&line).context("invalid run envelope")?,
            Some(protocol.clone()),
        )
    })();
    let terminal = match outcome {
        Ok(value) => json!({ "type": "result", "value": value }),
        Err(error) => json!({ "type": "error", "message": format!("{error:#}") }),
    };
    if let Ok(mut locked) = protocol.lock() {
        let _ = send(&mut locked, &terminal);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::Verifier;

    #[test]
    fn receipt_signature_covers_exact_body() {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let body = json!({"format": "kototama.component-execution-receipt/v1",
                          "result": 6419002});
        let receipt = receipt_envelope(body.clone(), &key).unwrap();
        let payload = receipt.get("payload").unwrap().as_str().unwrap();
        assert_eq!(serde_json::from_str::<Value>(payload).unwrap(), body);
        let signature_bytes: [u8; 64] =
            hex::decode(receipt.get("signature").unwrap().as_str().unwrap())
                .unwrap()
                .try_into()
                .unwrap();
        let signature = ed25519_dalek::Signature::from_bytes(&signature_bytes);
        key.verifying_key()
            .verify(payload.as_bytes(), &signature)
            .unwrap();
        assert!(
            key.verifying_key()
                .verify(
                    &serde_json::to_vec(&json!({"result": 0})).unwrap(),
                    &signature
                )
                .is_err()
        );
    }

    #[test]
    fn public_bind_is_rejected() {
        let public = "0.0.0.0:18901".parse::<SocketAddr>().unwrap();
        let loopback = "127.0.0.1:18901".parse::<SocketAddr>().unwrap();
        assert!(!public.ip().is_loopback());
        assert!(loopback.ip().is_loopback());
    }
}
