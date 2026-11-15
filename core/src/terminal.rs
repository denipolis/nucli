#[derive(Clone)]
pub enum Feed {
    Start { cols: usize, rows: usize },
    Bytes { local: bool, data: Vec<u8> },
    Size { cols: usize, rows: usize },
    Flush,
}
