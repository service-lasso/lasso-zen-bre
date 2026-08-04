use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};

const SERVICE_ID: &str = "zen-bre";
const ENGINE_VERSION: &str = "1.0.0-beta.11";

#[derive(Debug, Clone)]
struct Config {
    host: String,
    port: u16,
    decisions_dir: PathBuf,
}

impl Config {
    fn from_args() -> Result<Self, String> {
        let mut host = env::var("ZEN_BRE_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
        let mut port = env::var("ZEN_BRE_PORT")
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(18089);
        let mut decisions_dir = env::var("ZEN_BRE_DECISIONS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("decisions"));

        let mut args = env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--host" => {
                    host = args
                        .next()
                        .ok_or_else(|| "--host requires a value".to_string())?;
                }
                "--port" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--port requires a value".to_string())?;
                    port = value
                        .parse::<u16>()
                        .map_err(|_| "--port must be a TCP port".to_string())?;
                }
                "--decisions-dir" => {
                    decisions_dir = PathBuf::from(
                        args.next()
                            .ok_or_else(|| "--decisions-dir requires a value".to_string())?,
                    );
                }
                "--help" | "-h" => {
                    println!(
                        "Usage: lasso-zen-bre [--host 127.0.0.1] [--port 18089] [--decisions-dir ./decisions]"
                    );
                    std::process::exit(0);
                }
                other => return Err(format!("unsupported argument: {other}")),
            }
        }

        Ok(Self {
            host,
            port,
            decisions_dir,
        })
    }
}

fn main() {
    let config = Config::from_args().unwrap_or_else(|error| {
        eprintln!("configuration_error: {error}");
        std::process::exit(2);
    });

    if let Err(error) = fs::create_dir_all(&config.decisions_dir) {
        eprintln!("decisions_dir_error: {error}");
        std::process::exit(2);
    }

    let listener = TcpListener::bind((config.host.as_str(), config.port)).unwrap_or_else(|error| {
        eprintln!("bind_error: {error}");
        std::process::exit(2);
    });

    println!(
        "lasso-zen-bre listening on http://{}:{}",
        config.host, config.port
    );

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => handle_connection(stream, &config),
            Err(error) => eprintln!("accept_error: {error}"),
        }
    }
}

fn handle_connection(mut stream: TcpStream, config: &Config) {
    let mut buffer = [0; 8192];
    let bytes_read = match stream.read(&mut buffer) {
        Ok(size) => size,
        Err(error) => {
            eprintln!("read_error: {error}");
            return;
        }
    };

    let request = String::from_utf8_lossy(&buffer[..bytes_read]);
    let request_line = request.lines().next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let path = parts.next().unwrap_or("/");

    let (status, body) = route(method, path, config);
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );

    if let Err(error) = stream.write_all(response.as_bytes()) {
        eprintln!("write_error: {error}");
    }
}

fn route(method: &str, path: &str, config: &Config) -> (&'static str, String) {
    match (method, path) {
        ("GET", "/health/live") => ("200 OK", live_json()),
        ("GET", "/health/ready") => ("200 OK", ready_json(&config.decisions_dir)),
        ("GET", "/version") => ("200 OK", version_json()),
        ("GET", "/v1/decisions") => ("200 OK", decisions_json(&config.decisions_dir)),
        ("POST", "/v1/decisions/reload") => ("200 OK", ready_json(&config.decisions_dir)),
        ("POST", route) if route.starts_with("/v1/decisions/") && route.ends_with("/evaluate") => (
            "501 Not Implemented",
            json_object(&[
                ("service", SERVICE_ID),
                ("status", "not_implemented"),
                ("code", "engine_registry_pending"),
            ]),
        ),
        _ => (
            "404 Not Found",
            json_object(&[("service", SERVICE_ID), ("error", "not_found")]),
        ),
    }
}

fn live_json() -> String {
    json_object(&[("service", SERVICE_ID), ("status", "ok")])
}

fn ready_json(decisions_dir: &Path) -> String {
    let decisions = discover_decisions(decisions_dir);
    format!(
        "{{\"service\":\"{}\",\"status\":\"ready\",\"engine\":\"zen-engine\",\"engineVersion\":\"{}\",\"decisionCount\":{},\"decisions\":{}}}",
        SERVICE_ID,
        ENGINE_VERSION,
        decisions.len(),
        json_array(&decisions)
    )
}

fn version_json() -> String {
    format!(
        "{{\"service\":\"{}\",\"wrapperVersion\":\"{}\",\"engine\":\"zen-engine\",\"engineVersion\":\"{}\",\"arbitraryPrecision\":true}}",
        SERVICE_ID,
        env!("CARGO_PKG_VERSION"),
        ENGINE_VERSION
    )
}

fn decisions_json(decisions_dir: &Path) -> String {
    let decisions = discover_decisions(decisions_dir);
    format!(
        "{{\"service\":\"{}\",\"valuePolicy\":\"metadata_only\",\"decisions\":{}}}",
        SERVICE_ID,
        json_array(&decisions)
    )
}

fn discover_decisions(decisions_dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(decisions_dir) else {
        return Vec::new();
    };

    let mut ids = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file()
            && path.extension().and_then(|value| value.to_str()) == Some("json")
            && let Some(id) = decision_id_for_path(&path)
        {
            ids.push(id);
        }
    }
    ids.sort();
    ids
}

fn decision_id_for_path(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    let valid = stem
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'));
    if valid && !stem.contains("..") && !stem.is_empty() {
        Some(stem.to_string())
    } else {
        None
    }
}

fn json_object(fields: &[(&str, &str)]) -> String {
    let rendered = fields
        .iter()
        .map(|(key, value)| format!("\"{}\":\"{}\"", escape_json(key), escape_json(value)))
        .collect::<Vec<_>>()
        .join(",");
    format!("{{{rendered}}}")
}

fn json_array(values: &[String]) -> String {
    let rendered = values
        .iter()
        .map(|value| format!("\"{}\"", escape_json(value)))
        .collect::<Vec<_>>()
        .join(",");
    format!("[{rendered}]")
}

fn escape_json(value: &str) -> String {
    value
        .chars()
        .flat_map(|ch| match ch {
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\n' => "\\n".chars().collect::<Vec<_>>(),
            '\r' => "\\r".chars().collect::<Vec<_>>(),
            '\t' => "\\t".chars().collect::<Vec<_>>(),
            other => vec![other],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_path_like_decision_ids() {
        assert_eq!(
            decision_id_for_path(Path::new("decisions/valid-decision.json")),
            Some("valid-decision".to_string())
        );
        assert_eq!(
            decision_id_for_path(Path::new("decisions/..bad.json")),
            None
        );
    }

    #[test]
    fn version_reports_arbitrary_precision_pin() {
        let body = version_json();
        assert!(body.contains("\"engineVersion\":\"1.0.0-beta.11\""));
        assert!(body.contains("\"arbitraryPrecision\":true"));
    }
}
