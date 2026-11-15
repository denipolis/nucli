pub mod keys;

use std::sync::Arc;
use std::sync::mpsc::{Sender, channel};

use alacritty_terminal::event::EventListener;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Term;
use alacritty_terminal::vte::ansi::Processor;

#[derive(Clone)]
pub enum Feed {
    Start { cols: usize, rows: usize },
    Bytes { local: bool, data: Vec<u8> },
    Size { cols: usize, rows: usize },
    Flush,
}

pub fn spawn_mirror<T: EventListener + Send + 'static>(
    term: Arc<FairMutex<Term<T>>>,
    mut resize: impl FnMut(&mut Term<T>, usize, usize) + Send + 'static,
    mut fed: impl FnMut(bool) + Send + 'static,
) -> Sender<Feed> {
    let (feeds, received) = channel::<Feed>();
    std::thread::spawn(move || {
        let mut reader: Processor = Processor::new();
        let mut local: Processor = Processor::new();
        while let Ok(feed) = received.recv() {
            let output = matches!(feed, Feed::Bytes { local: false, .. });
            let mut term = term.lock();
            match feed {
                Feed::Start { cols, rows } => {
                    resize(&mut term, cols, rows);
                    reader = Processor::new();
                    local = Processor::new();
                    reader.advance(&mut *term, b"\x1bc");
                }
                Feed::Bytes { local: true, data } => local.advance(&mut *term, &data),
                Feed::Bytes { local: false, data } => reader.advance(&mut *term, &data),
                Feed::Size { cols, rows } => resize(&mut term, cols, rows),
                Feed::Flush => reader.stop_sync(&mut *term),
            }
            drop(term);
            fed(output);
        }
    });
    feeds
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::grid::Dimensions;
    use alacritty_terminal::index::{Column, Line, Point};
    use alacritty_terminal::term::Config;
    use alacritty_terminal::term::test::TermSize;

    #[test]
    fn mirror_rebuilds_the_screen_from_feeds() {
        let term = Arc::new(FairMutex::new(Term::new(Config::default(), &TermSize::new(10, 3), VoidListener)));
        let (done, fed) = channel();
        let feeds = spawn_mirror(
            term.clone(),
            |term, cols, rows| term.resize(TermSize::new(cols, rows)),
            move |output| done.send(output).unwrap(),
        );
        feeds.send(Feed::Bytes { local: false, data: b"stale".to_vec() }).unwrap();
        feeds.send(Feed::Start { cols: 20, rows: 4 }).unwrap();
        feeds.send(Feed::Bytes { local: false, data: b"hi\r\nthere".to_vec() }).unwrap();
        feeds.send(Feed::Bytes { local: true, data: b"!".to_vec() }).unwrap();
        assert_eq!(fed.iter().take(4).collect::<Vec<_>>(), [true, false, true, false]);
        let term = term.lock();
        assert_eq!((term.columns(), term.screen_lines()), (20, 4));
        let row = |line: i32| -> String {
            (0..20).map(|column| term.grid()[Point::new(Line(line), Column(column))].c).collect::<String>()
        };
        assert_eq!(row(0).trim_end(), "hi");
        assert_eq!(row(1).trim_end(), "there!");
    }
}
