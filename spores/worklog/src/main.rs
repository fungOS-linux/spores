//! The worklog receiver: normalize producer payloads and keep the log.
//!
//! This runs as a native process, not a Spore, because it owns a socket and a
//! file and no Spore is allowed to. When Soil grows an ingest capability this
//! becomes that capability's body and the CLI stays as the operator's handle on
//! it.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::ExitCode;
use std::time::Duration;

use worklog::{
    Append, Event, Store, from_github, from_marbles, iso8601_utc, now_ms, snapshot_lua,
    verify_signature,
};

const USAGE: &str = "\
worklog — the fungos.worklog/event@1 receiver and reader

  worklog ingest   --source marbles|github --log PATH --secret-file PATH
                   [--signature VALUE] [--at MS]
                   read one payload from stdin
  worklog ingest-batch --source marbles|github --log PATH --allow-unsigned
                   [--at MS]                read one payload per line
  worklog serve    --bind ADDR --log PATH
                   --marbles-secret-file PATH --github-secret-file PATH
  worklog tail     --log PATH [--limit N] [--all]
  worklog render-lua --log PATH [--limit N] [--all] [--now MS]
  worklog verify   --secret-file PATH --header VALUE   body on stdin

`--all` includes noise kinds (lease heartbeats) that the default view drops.
`--allow-unsigned` is for replaying a payload you already have; the receiver
never uses it. The log is append-only JSONL; `tail` reads it newest first.";

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("worklog: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    let Some((command, rest)) = args.split_first() else {
        return Err(USAGE.to_string());
    };
    match command.as_str() {
        "ingest" => ingest(rest),
        "ingest-batch" => ingest_batch(rest),
        "serve" => serve(rest),
        "tail" => tail(rest),
        "render-lua" => render_lua(rest),
        "verify" => verify(rest),
        "--help" | "-h" | "help" => {
            println!("{USAGE}");
            Ok(())
        }
        other => Err(format!("unknown command {other:?}\n\n{USAGE}")),
    }
}

fn option(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|argument| argument == name)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

fn required(args: &[String], name: &str) -> Result<String, String> {
    option(args, name).ok_or_else(|| format!("missing {name}\n\n{USAGE}"))
}

fn number(args: &[String], name: &str, default: i64) -> Result<i64, String> {
    match option(args, name) {
        Some(raw) => raw
            .parse()
            .map_err(|_| format!("{name} expects an integer, got {raw:?}")),
        None => Ok(default),
    }
}

fn has(args: &[String], name: &str) -> bool {
    args.iter().any(|argument| argument == name)
}

fn secret(path: &str) -> Result<Vec<u8>, String> {
    let text = std::fs::read_to_string(path).map_err(|error| format!("read {path}: {error}"))?;
    let trimmed = text.trim_end_matches(['\n', '\r']).to_string();
    if trimmed.is_empty() {
        return Err(format!(
            "{path} is empty; a webhook with no secret is unauthenticated"
        ));
    }
    Ok(trimmed.into_bytes())
}

fn read_stdin() -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    std::io::stdin()
        .read_to_end(&mut body)
        .map_err(|error| format!("read stdin: {error}"))?;
    Ok(body)
}

/// Verify (when a secret is configured) and normalize one payload.
fn accept(
    source: &str,
    body: &[u8],
    signature: Option<&str>,
    secret: Option<&[u8]>,
    now: i64,
) -> Result<Event, Rejected> {
    if let Some(secret) = secret {
        let header = signature.ok_or(Rejected::new(401, "missing signature header"))?;
        if !verify_signature(secret, body, header) {
            return Err(Rejected::new(401, "signature does not match the body"));
        }
    }
    match source {
        "marbles" => from_marbles(body).map_err(|error| Rejected::new(400, &error)),
        "github" => from_github(body, now).map_err(|error| Rejected::new(400, &error)),
        other => Err(Rejected::new(400, &format!("unknown source {other:?}"))),
    }
}

/// A refusal with the status a webhook sender should see.
#[derive(Debug)]
struct Rejected {
    status: u16,
    reason: String,
}

impl Rejected {
    fn new(status: u16, reason: &str) -> Self {
        Self {
            status,
            reason: reason.to_string(),
        }
    }
}

fn ingest(args: &[String]) -> Result<(), String> {
    let source = required(args, "--source")?;
    let log = required(args, "--log")?;
    let secret = match option(args, "--secret-file") {
        Some(path) => Some(secret(&path)?),
        // Reading one delivery by hand is legitimate; accepting an unauthenticated
        // one by accident is not. The flag is the difference.
        None if has(args, "--allow-unsigned") => None,
        None => {
            return Err(
                "ingest needs --secret-file (or --allow-unsigned when replaying a known payload)\n\n"
                    .to_string()
                    + USAGE,
            );
        }
    };
    let now = match option(args, "--at") {
        Some(raw) => raw
            .parse()
            .map_err(|_| format!("--at expects milliseconds, got {raw:?}"))?,
        None => now_ms(),
    };

    let body = read_stdin()?;
    let signature = option(args, "--signature");
    let event = accept(&source, &body, signature.as_deref(), secret.as_deref(), now)
        .map_err(|rejected| rejected.reason)?;

    let mut store = Store::open(&log)?;
    match store.append_at(event.clone(), now)? {
        Append::Written(hlc) => println!(
            "{} {} {} (hlc {}:{})",
            iso8601_utc(event.at),
            event.kind.as_str(),
            event.summary,
            hlc.physical,
            hlc.counter
        ),
        Append::Duplicate => println!("duplicate {}", event.id),
    }
    Ok(())
}

fn tail(args: &[String]) -> Result<(), String> {
    let log = required(args, "--log")?;
    let limit = number(args, "--limit", 40)?;
    let store = Store::open(&log)?;
    for event in store.recent(usize::try_from(limit.max(0)).unwrap_or(0))? {
        if event.kind.is_noise() && !has(args, "--all") {
            continue;
        }
        println!(
            "{}  {:<10} {:<8} {:<12} {}",
            iso8601_utc(event.at),
            event.kind.as_str(),
            event.source,
            event.actor,
            event.summary
        );
    }
    Ok(())
}

fn render_lua(args: &[String]) -> Result<(), String> {
    let log = required(args, "--log")?;
    let limit = number(args, "--limit", 60)?;
    let now = number(args, "--now", now_ms())?;
    let store = Store::open(&log)?;
    let events = store.scan()?;
    let snapshot = snapshot_lua(
        &events,
        usize::try_from(limit.max(0)).unwrap_or(0),
        now,
        has(args, "--all"),
    );
    print!("{snapshot}");
    Ok(())
}

fn verify(args: &[String]) -> Result<(), String> {
    let secret = secret(&required(args, "--secret-file")?)?;
    let header = required(args, "--header")?;
    let body = read_stdin()?;
    if verify_signature(&secret, &body, &header) {
        println!("ok");
        Ok(())
    } else {
        Err("signature does not match the body".to_string())
    }
}

// ---- the receiver -------------------------------------------------------

/// The largest body either producer is known to send, with room to spare. A
/// webhook is a notification, not an upload.
const MAX_BODY: usize = 1 << 20;
const MAX_HEADERS: usize = 64;
const MAX_LINE: u64 = 8 << 10;

struct Routes {
    marbles: Option<Vec<u8>>,
    github: Option<Vec<u8>>,
}

/// Read many envelopes in one process.
///
/// A webhook delivers one event at a time; catching up after an outage delivers
/// thousands, and one process per row would re-read the whole log each time —
/// quadratic in exactly the case that matters. This is the reconciliation leg.
///
/// There is no per-line signature to check here: the input is a payload this
/// operator already holds, so `--allow-unsigned` is required and explicit.
fn ingest_batch(args: &[String]) -> Result<(), String> {
    let source = required(args, "--source")?;
    let log = required(args, "--log")?;
    if !has(args, "--allow-unsigned") {
        return Err(
            "ingest-batch needs --allow-unsigned: a batch has no per-line signature\n\n"
                .to_string()
                + USAGE,
        );
    }
    let now = match option(args, "--at") {
        Some(raw) => raw
            .parse()
            .map_err(|_| format!("--at expects milliseconds, got {raw:?}"))?,
        None => now_ms(),
    };

    let body = read_stdin()?;
    let mut store = Store::open(&log)?;
    let (mut written, mut duplicate, mut rejected) = (0_u32, 0_u32, 0_u32);
    for (index, line) in body.split(|byte| *byte == b'\n').enumerate() {
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let event = match accept(&source, line, None, None, now) {
            Ok(event) => event,
            Err(rejection) => {
                rejected += 1;
                eprintln!("worklog: line {}: {}", index + 1, rejection.reason);
                continue;
            }
        };
        match store.append_at(event, now)? {
            Append::Written(_) => written += 1,
            Append::Duplicate => duplicate += 1,
        }
    }
    println!("worklog: {written} written, {duplicate} duplicate, {rejected} rejected");
    if rejected > 0 {
        return Err(format!("{rejected} line(s) could not be normalized"));
    }
    Ok(())
}

fn serve(args: &[String]) -> Result<(), String> {
    let bind = option(args, "--bind").unwrap_or_else(|| "127.0.0.1:7420".to_string());
    let log = required(args, "--log")?;
    let routes = Routes {
        marbles: match option(args, "--marbles-secret-file") {
            Some(path) => Some(secret(&path)?),
            None => None,
        },
        github: match option(args, "--github-secret-file") {
            Some(path) => Some(secret(&path)?),
            None => None,
        },
    };
    if routes.marbles.is_none() && routes.github.is_none() {
        // Refusing to start beats starting a receiver that would accept
        // unsigned payloads as if they were authenticated.
        return Err(
            "no secret configured: pass --marbles-secret-file or --github-secret-file".to_string(),
        );
    }

    let listener = TcpListener::bind(&bind).map_err(|error| format!("bind {bind}: {error}"))?;
    println!("worklog: listening on {bind}, appending to {log}");

    let mut store = Store::open(&log)?;
    for connection in listener.incoming() {
        match connection {
            Ok(stream) => {
                if let Err(error) = handle(stream, &mut store, &routes) {
                    eprintln!("worklog: {error}");
                }
            }
            Err(error) => eprintln!("worklog: accept: {error}"),
        }
    }
    Ok(())
}

fn handle(mut stream: TcpStream, store: &mut Store, routes: &Routes) -> Result<(), String> {
    // A sender that connects and stalls must not stop the next delivery.
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .map_err(|error| format!("set_read_timeout: {error}"))?;
    let mut reader = BufReader::new(stream.try_clone().map_err(|error| error.to_string())?);

    let mut line = String::new();
    reader
        .by_ref()
        .take(MAX_LINE)
        .read_line(&mut line)
        .map_err(|error| format!("read request line: {error}"))?;
    let mut parts = line.split_whitespace();
    let (Some(method), Some(path)) = (parts.next(), parts.next()) else {
        return respond(&mut stream, 400, "malformed request line");
    };
    let source = match path {
        "/ingest/marbles" => "marbles",
        "/ingest/github" => "github",
        _ => return respond(&mut stream, 404, "unknown ingestion route"),
    };
    if method != "POST" {
        return respond(&mut stream, 405, "only POST is accepted");
    }

    let mut content_length = None;
    let mut signature = None;
    for _ in 0..MAX_HEADERS {
        let mut header = String::new();
        reader
            .by_ref()
            .take(MAX_LINE)
            .read_line(&mut header)
            .map_err(|error| format!("read header: {error}"))?;
        let header = header.trim_end_matches(['\n', '\r']);
        if header.is_empty() {
            break;
        }
        let Some((name, value)) = header.split_once(':') else {
            return respond(&mut stream, 400, "malformed header");
        };
        let value = value.trim();
        // Both channels sign the raw body with HMAC-SHA256; only the header
        // name differs, so the value is collected the same way for both.
        if name.eq_ignore_ascii_case("x-marbles-signature")
            || name.eq_ignore_ascii_case("x-hub-signature-256")
        {
            signature = Some(value.to_string());
        }
        if name.eq_ignore_ascii_case("content-length") {
            if content_length.is_some() {
                return respond(&mut stream, 400, "duplicate content-length");
            }
            content_length = value.parse::<usize>().ok();
        }
    }

    let Some(length) = content_length else {
        return respond(&mut stream, 400, "content-length is required");
    };
    if length > MAX_BODY {
        return respond(&mut stream, 413, "body is larger than the receiver accepts");
    }
    let mut body = vec![0_u8; length];
    reader
        .read_exact(&mut body)
        .map_err(|error| format!("read body: {error}"))?;

    let secret = match source {
        "marbles" => routes.marbles.as_deref(),
        _ => routes.github.as_deref(),
    };
    let Some(secret) = secret else {
        // Configured without a secret for this source: refuse rather than
        // accept an unauthenticated payload the operator did not intend to allow.
        return respond(&mut stream, 503, "no secret configured for this route");
    };

    let now = now_ms();
    let event = match accept(source, &body, signature.as_deref(), Some(secret), now) {
        Ok(event) => event,
        Err(rejected) => return respond(&mut stream, rejected.status, &rejected.reason),
    };

    match store.append_at(event.clone(), now) {
        Ok(Append::Written(_)) => {
            println!(
                "{} {:<10} {:<8} {}",
                iso8601_utc(event.at),
                event.kind.as_str(),
                event.source,
                event.summary
            );
            respond(
                &mut stream,
                200,
                &format!("{{\"accepted\":\"{}\"}}", event.id),
            )
        }
        Ok(Append::Duplicate) => respond(&mut stream, 200, "{\"duplicate\":true}"),
        Err(error) => {
            eprintln!("worklog: append failed: {error}");
            respond(&mut stream, 503, "the log could not be written")
        }
    }
}

fn respond(stream: &mut TcpStream, status: u16, body: &str) -> Result<(), String> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        503 => "Service Unavailable",
        _ => "Error",
    };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .and_then(|()| stream.flush())
        .map_err(|error| format!("respond: {error}"))
}
