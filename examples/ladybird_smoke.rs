use elaris::ladybird::{Event, Ladybird};
use std::cell::Cell;
use std::env;
use std::path::PathBuf;
use std::rc::Rc;
use std::thread;
use std::time::{Duration, Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let resource_dir = PathBuf::from(args.next().ok_or("missing Ladybird resource directory")?);
    let profile_dir = PathBuf::from(args.next().ok_or("missing profile directory")?);
    let html_file = PathBuf::from(args.next().ok_or("missing local HTML test page")?);
    let file_url = format!("file://{}", html_file.canonicalize()?.display());
    let saw_title = Rc::new(Cell::new(false));
    let saw_frame = Rc::new(Cell::new(false));
    let callback_title = Rc::clone(&saw_title);
    let callback_frame = Rc::clone(&saw_frame);
    let mut browser = Ladybird::new(&resource_dir, &profile_dir, move |event| match event {
        Event::Title(title) if title == "Elaris Bridge Smoke" => callback_title.set(true),
        Event::ReadyToPaint => callback_frame.set(true),
        _ => {}
    })?;
    let mut view = browser.create_view()?;
    view.load(&file_url)?;
    view.resize(900, 650, 2.0)?;

    // The smoke page's background is rgb(0, 128, 255); its bottom-right pixel shows it.
    let mut saw_pixels = false;
    let deadline = Instant::now() + Duration::from_secs(30);
    while !saw_title.get() || !saw_pixels {
        view.pump()?;
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
    drop(view);
    let close_deadline = Instant::now() + Duration::from_millis(100);
    while Instant::now() < close_deadline {
        browser.pump()?;
        thread::sleep(Duration::from_millis(5));
    }
    drop(browser);
    drop(Ladybird::new(&resource_dir, &profile_dir, |_| {})?);
    Ok(())
}
