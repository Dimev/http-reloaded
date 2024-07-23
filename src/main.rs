use std::{
    fs,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use clap::Parser;
use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode};

const RW_ERR: &str = "Cronch: lock was poissoned";
const VERY_LONG_PATH: &str = "very-long-path-name-intentionally-used-to-get-update-notifications-please-do-not-name-your-files-like-this.rs";
const UPDATE_NOTIFY_SCRIPT: &str = include_str!("update_notify.html");

/// Serve the files
fn serve(path: PathBuf, addr: Option<String>) -> Result<(), anyhow::Error> {
    // stream to notify when an update happens
    let update_notify = Arc::new(Mutex::new(Vec::<TcpStream>::new()));

    let update_notify_cloned = update_notify.clone();
    let mut debouncer = new_debouncer(Duration::from_millis(500), move |res| match res {
        Ok(_) => {
            println!("Files changed, reloading");

            // notify the upate
            let mut stream = update_notify_cloned.lock().expect(RW_ERR);
            stream.retain_mut(
                |s| match s.write_all(b"data: update\n\n").and_then(|_| s.flush()) {
                    Ok(()) => true,
                    Err(_) => false,
                },
            );
        }
        Err(e) => println!("[ERR] While watching files: {:?}", e),
    })?;

    // watch the current dir
    debouncer.watcher().watch(
        if path.is_file() {
            path.parent()
                .expect("File does not have a parent directory")
        } else {
            &path
        },
        RecursiveMode::Recursive,
    )?;

    // listen to incoming requests
    let addr = addr.unwrap_or("127.0.0.1:1111".to_string());
    let listener = TcpListener::bind(&addr)?;
    println!("listening on {}", addr);

    for stream in listener.incoming() {
        if let Err(e) = handle_connection(stream?, &path, &update_notify) {
            println!("[ERR] While responding to request: {:?}", e);
        }
    }

    Ok(())
}

fn handle_connection(
    mut stream: TcpStream,
    path: &PathBuf,
    update_notify: &Arc<Mutex<Vec<TcpStream>>>,
) -> Result<(), anyhow::Error> {
    let reader = BufReader::new(&mut stream);
    let request = reader.lines().next().unwrap_or(Ok("".to_string()))?;

    // trim the request
    let file_path = request
        .trim_start_matches("GET")
        .trim_end_matches("HTTP/1.1")
        .trim()
        .trim_start_matches("/");

    // try and get the file
    let (content, status, mime_type) = if let Some(file) = fs::read(&path.join(file_path)).ok() {
        // get the file content
        (file, "200 OK", get_mime_type(&file_path))
    }
    // try to see if this was an index.html file
    else if let Some(file) = fs::read(&path.join(file_path).join("index.html")).ok() {
        (file, "200 OK", Some("text/html"))
    }
    // if it's the update notifier, set the update stream
    else if file_path == VERY_LONG_PATH {
        // we don't want to wait
        stream.set_nodelay(true)?;

        // send the response
        stream.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\n\r\n",
            )?;
        stream.write_all(b"data: initial\n\n")?;
        stream.flush()?;

        // set the event stream, as we have one now
        update_notify.lock().expect(RW_ERR).push(stream);

        // don't need to send more
        return Ok(());
    }
    // otherwise use the default 404
    else {
        (
            format!(
                "<!DOCTYPE html><h1>404: Not found</h1><p>page {} not found</p>",
                file_path
            )
            .into_bytes(),
            "404 NOT FOUND",
            Some("text/html"),
        )
    };

    // update notify script
    let update_notify = if mime_type == Some("text/html") {
        UPDATE_NOTIFY_SCRIPT
    } else {
        ""
    };

    // send the page back
    let length = content.len() + update_notify.len();
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {length}\r\nCache-Control: no-cache\r\n{}\r\n",
        if let Some(mime) = mime_type {
            format!("Content-Type: {mime}\r\n")
        } else {
            String::new()
        }
    );

    // write response and page content
    stream.write_all(response.as_bytes())?;
    stream.write_all(&content)?;
    stream.write_all(UPDATE_NOTIFY_SCRIPT.as_bytes())?;

    Ok(())
}

/// Get a mime type from a file path
fn get_mime_type<P: AsRef<Path>>(path: &P) -> Option<&str> {
    // see https://developer.mozilla.org/en-US/docs/Web/HTTP/Basics_of_HTTP/MIME_types/Common_types
    match path.as_ref().extension()?.to_str()? {
        "aac" => Some("audio/aac"),
        "abw" => Some("application/x-abiword"),
        "apng" => Some("image/apng"),
        "arc" => Some("application/x-freearc"),
        "avif" => Some("image/avif"),
        "avi" => Some("video/x-msvideo"),
        "azw" => Some("application/vnd.amazon.ebook"),
        "bin" => Some("application/octet-stream"),
        "bmp" => Some("image/bmp"),
        "bz" => Some("application/x-bzip"),
        "bz2" => Some("application/x-bzip2"),
        "cda" => Some("application/x-cdf"),
        "csh" => Some("application/x-csh"),
        "css" => Some("text/css"),
        "csv" => Some("text/csv"),
        "doc" => Some("application/msword"),
        "docx" => Some("application/vnd.openxmlformats-officedocument.wordprocessingml.document"),
        "eot" => Some("application/vnd.ms-fontobject"),
        "epub" => Some("application/epub+zip"),
        "gz" => Some("application/gzip"),
        "gif" => Some("application/gif"),
        "htm" | "html" => Some("text/html"),
        "ico" => Some("image/vnd.microsoft.icon"),
        "ics" => Some("text/calendar"),
        "jar" => Some("application/java-archive"),
        "jpeg" | "jpg" => Some("image/jpeg"),
        "js" => Some("text/javascript"),
        "json" => Some("application/json"),
        "jsonld" => Some("application/ld+json"),
        "mid" | "midi" => Some("audio/midi"),
        "mjs" => Some("text/javascript"),
        "mp3" => Some("audio/mpeg"),
        "mp4" => Some("video/mpeg"),
        "mpeg" => Some("video/mpeg"),
        "mpkg" => Some("application/vnd.apple.installer+xml"),
        "odp" => Some("application/vnd.oasis.opendocument.presentation"),
        "ods" => Some("application/vnd.oasis.opendocument.spreadsheet"),
        "odt" => Some("application/vnd.oasis.opendocument.text"),
        "oga" => Some("audio/ogg"),
        "ogv" => Some("video/ogg"),
        "ogx" => Some("application/ogg"),
        "opus" => Some("audio/opus"),
        "otf" => Some("font/otf"),
        "png" => Some("image/png"),
        "pdf" => Some("application/pdf"),
        "php" => Some("application/x-httpd-php"),
        "ppt" => Some("application/vnd.ms-powerpoint"),
        "pptx" => Some("application/vnd/openxmlformats-officedocument.presentationml.presentation"),
        "rar" => Some("application/vnd.rar"),
        "rtf" => Some("application/rtf"),
        "sh" => Some("application/x-sh"),
        "svg" => Some("image/svg+xml"),
        "tar" => Some("application/x-tar"),
        "tif" | "tiff" => Some("image/tiff"),
        "ts" => Some("video/mp2t"),
        "ttf" => Some("font/ttf"),
        "txt" => Some("text/plain"),
        "vsd" => Some("application/vnd.visio"),
        "wav" => Some("audio/wav"),
        "weba" => Some("audio/webm"),
        "webm" => Some("video/webm"),
        "webp" => Some("image/webp"),
        "woff" => Some("font/woff"),
        "woff2" => Some("font/woff2"),
        "xhtml" => Some("application/xhtml+xml"),
        "xls" => Some("application/vnd.ms-exel"),
        "xlsx" => Some("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
        "xml" => Some("application/xml"),
        "xul" => Some("application/vnd.mozilla.xul+xml"),
        "zip" => Some("application/zip"),
        "3pg" => Some("video/3gpp"),
        "3g2" => Some("video/3ggp2"),
        "7z" => Some("application/x-7z-compressed"),
        // Missing for some reason
        "wasm" => Some("application/wasm"),
        _ => None,
    }
}

#[derive(Parser)]
struct Args {
    /// Optional path to either a directory containing `site.lua`, or a lua file that builds the site
    path: Option<PathBuf>,

    /// Address to serve on, defaults to 127.0.0.1:1111
    #[clap(short, long)]
    address: Option<String>,
}

fn main() -> Result<(), anyhow::Error> {
    let args = Args::parse();
    serve(args.path.unwrap_or(PathBuf::from(".")), args.address)?;
    Ok(())
}
