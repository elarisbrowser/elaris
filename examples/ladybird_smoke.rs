use elaris::downloads::{Status as DownloadStatus, Update as DownloadUpdate};
use elaris::ladybird::{Event, Ladybird};
use std::cell::Cell;
use std::env;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let resource_dir = PathBuf::from(args.next().ok_or("missing Ladybird resource directory")?);
    let profile_dir = isolated_profile(PathBuf::from(
        args.next().ok_or("missing profile directory")?,
    ))?;
    let html_file = PathBuf::from(args.next().ok_or("missing local HTML test page")?);
    let file_url = format!("file://{}", html_file.canonicalize()?.display());
    std::fs::create_dir_all(&profile_dir)?;
    let profile_dir = profile_dir.canonicalize()?;
    ensure_outside_user_downloads(&profile_dir)?;
    let download_dir = profile_dir.join("isolated-downloads");
    ensure_outside_user_downloads(&download_dir)?;
    // A file left by an earlier run would make this run save as "rust-ffi (1).txt".
    if download_dir.exists() {
        std::fs::remove_dir_all(&download_dir)?;
    }
    std::fs::create_dir_all(&download_dir)?;
    let download_dir = download_dir.canonicalize()?;
    ensure_outside_user_downloads(&download_dir)?;
    let server = LocalDownloads::start()?;
    let saw_title = Rc::new(Cell::new(false));
    let saw_frame = Rc::new(Cell::new(false));
    let download_updates = Rc::new(std::cell::RefCell::new(Vec::<DownloadUpdate>::new()));
    let callback_title = Rc::clone(&saw_title);
    let callback_frame = Rc::clone(&saw_frame);
    let callback_downloads = Rc::clone(&download_updates);
    let mut browser = Ladybird::new(
        Some(&resource_dir),
        &profile_dir,
        move |_, event| match event {
            Event::Title(title) if title == "Elaris Bridge Smoke" => callback_title.set(true),
            Event::ReadyToPaint => callback_frame.set(true),
            Event::Download(download) => callback_downloads.borrow_mut().push(download),
            _ => {}
        },
    )?;
    browser.set_download_directory(&download_dir)?;
    let mut view = browser.create_view(1, 60.0)?;
    view.load(&file_url)?;
    view.resize(900, 650, 2.0)?;

    // The smoke page's background is rgb(0, 128, 255); its bottom-right pixel shows it.
    let mut saw_pixels = false;
    let deadline = Instant::now() + Duration::from_secs(30);
    while !saw_title.get() || !saw_pixels {
        browser.pump()?;
        if saw_frame.take()
            && let Some(frame) = view.frame()?
            && (frame.width, frame.height, frame.scale) == (900, 650, 2.0)
        {
            let rgba = view.copy_frame(&frame)?;
            saw_pixels = rgba[rgba.len() - 4..] == [0, 128, 255, 255];
        }
        if Instant::now() >= deadline {
            return Err("timed out waiting for the title and a 900x650 frame of the page".into());
        }
        thread::sleep(Duration::from_millis(5));
    }

    let small_id = browser.start_download(&server.url("/small"), Some("../rust-ffi.txt"))?;
    let small = wait_for_download(
        &mut browser,
        &download_updates,
        small_id,
        DownloadStatus::Completed,
    )?;
    let small_path = small
        .destination
        .as_ref()
        .ok_or("completed Rust FFI download has no destination")?;
    if small.filename != "rust-ffi.txt" || small_path.parent() != Some(download_dir.as_path()) {
        return Err("Rust FFI download did not use its sanitized isolated destination".into());
    }

    let slow_id = browser.start_download(&server.url("/slow"), Some("cancel-me.bin"))?;
    let mut saw_progress = false;
    let progress_deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < progress_deadline {
        browser.pump()?;
        if let Some(update) = latest_download(&download_updates, slow_id)
            && update.status == DownloadStatus::Downloading
            && update.bytes_downloaded > 0
        {
            saw_progress = true;
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    if !saw_progress {
        return Err("Rust FFI download did not report progress".into());
    }
    let slow =
        latest_download(&download_updates, slow_id).ok_or("download event was not reported")?;
    browser.cancel_download(slow.native_id())?;
    let cancelled = wait_for_download(
        &mut browser,
        &download_updates,
        slow_id,
        DownloadStatus::Cancelled,
    )?;
    if cancelled
        .destination
        .as_deref()
        .is_none_or(|path| path.parent() != Some(download_dir.as_path()))
    {
        return Err("cancelled Rust FFI download escaped its isolated directory".into());
    }

    drop(view);
    let close_deadline = Instant::now() + Duration::from_millis(100);
    while Instant::now() < close_deadline {
        browser.pump()?;
        thread::sleep(Duration::from_millis(5));
    }
    drop(browser);
    drop(Ladybird::new(Some(&resource_dir), &profile_dir, |_, _| {})?);
    drop(server);
    Ok(())
}

fn isolated_profile(path: PathBuf) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let path = if path.is_absolute() {
        path
    } else {
        env::current_dir()?.join(path)
    };
    ensure_outside_user_downloads(&path)?;
    Ok(path)
}

fn ensure_outside_user_downloads(path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let Some(home) = env::var_os("HOME") else {
        return Ok(());
    };
    let user_downloads = PathBuf::from(home).join("Downloads");
    let canonical_downloads = user_downloads
        .canonicalize()
        .unwrap_or_else(|_| user_downloads.clone());
    let mut existing = path;
    while !existing.exists() {
        existing = existing
            .parent()
            .ok_or("Rust FFI smoke path has no existing parent")?;
    }
    let resolved = existing.canonicalize()?.join(path.strip_prefix(existing)?);
    if resolved.starts_with(canonical_downloads) {
        return Err("Rust FFI smoke path must be outside the user's Downloads directory".into());
    }
    Ok(())
}

fn latest_download(
    updates: &std::cell::RefCell<Vec<DownloadUpdate>>,
    native_id: u64,
) -> Option<DownloadUpdate> {
    updates
        .borrow()
        .iter()
        .rev()
        .find(|update| update.native_id() == native_id)
        .cloned()
}

fn wait_for_download(
    browser: &mut Ladybird,
    updates: &std::cell::RefCell<Vec<DownloadUpdate>>,
    native_id: u64,
    status: DownloadStatus,
) -> Result<DownloadUpdate, Box<dyn std::error::Error>> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        browser.pump()?;
        if let Some(update) = latest_download(updates, native_id)
            && update.status == status
        {
            return Ok(update);
        }
        if Instant::now() >= deadline {
            return Err(format!("timed out waiting for download status {status:?}").into());
        }
        thread::sleep(Duration::from_millis(5));
    }
}

struct LocalDownloads {
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
    base_url: String,
}

impl LocalDownloads {
    fn start() -> Result<Self, Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let stop = Arc::new(AtomicBool::new(false));
        let should_stop = Arc::clone(&stop);
        let thread = thread::spawn(move || {
            while !should_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => serve_download(stream),
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            stop,
            thread: Some(thread),
            base_url: format!("http://{address}"),
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }
}

impl Drop for LocalDownloads {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve_download(mut stream: TcpStream) {
    let mut request = [0; 1024];
    let _ = stream.read(&mut request);
    let path = String::from_utf8_lossy(&request);
    let body = if path.contains("/slow ") {
        let header = b"HTTP/1.1 200 OK\r\nContent-Length: 16777216\r\nConnection: close\r\n\r\n";
        if stream.write_all(header).is_err() {
            return;
        }
        let chunk = [b'x'; 16 * 1024];
        for _ in 0..1024 {
            if stream.write_all(&chunk).is_err() {
                return;
            }
            thread::sleep(Duration::from_millis(4));
        }
        return;
    } else {
        b"Rust FFI download fixture\n".as_slice()
    };
    let header = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(body);
}
