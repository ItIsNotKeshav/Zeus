//! A box-tree check before the `mp4` crate reads an MP4's header.
//!
//! `mp4` 0.14 loops forever on a box inside a container whose size is 0, or
//! smaller than its own header: it seeks back to that box's start and reads
//! it again (a 64-byte file of `ftyp` and `moov { trak { size 0 } }` hangs
//! it). Anyone can send such a file, so every header goes through
//! [`read_header`], which walks the boxes the crate walks and refuses the
//! ones that would not move it forward.

use std::io::{Read, Seek, SeekFrom};

/// Deeper than any box the crate reads, so the walk always ends.
const MAX_DEPTH: usize = 16;

/// Reads the MP4 header of the `size` bytes in `reader`, after checking
/// that no box would stall the reader.
pub fn read_header<R: Read + Seek>(mut reader: R, size: u64) -> Result<mp4::Mp4Reader<R>, String> {
    let start = reader
        .stream_position()
        .map_err(|error| error.to_string())?;
    check(&mut reader, start, size)?;
    reader
        .seek(SeekFrom::Start(start))
        .map_err(|error| error.to_string())?;
    mp4::Mp4Reader::read_header(reader, size).map_err(|error| error.to_string())
}

/// Checks the boxes from `start` to `size`.
fn check<R: Read + Seek>(reader: &mut R, start: u64, size: u64) -> Result<(), String> {
    walk(reader, start, size, size, None, 0)
}

/// Walks the boxes from `at` to `end`, and those inside the containers the
/// crate reads box by box. A box `parent` holds may not be empty or end
/// past the file; the top level keeps the crate's own rules, which stop at
/// a box of size 0. A box that cannot be read is left for the crate to
/// report.
fn walk<R: Read + Seek>(
    reader: &mut R,
    mut at: u64,
    end: u64,
    file: u64,
    parent: Option<[u8; 4]>,
    depth: usize,
) -> Result<(), String> {
    while at < end {
        let Some((kind, size, header)) = read_box(reader, at)? else {
            return Ok(());
        };
        let name = String::from_utf8_lossy(&kind).into_owned();
        if size == 0 {
            return match parent {
                None => Ok(()),
                Some(_) => Err(format!("its {name} box has no size")),
            };
        }
        if size < header {
            return Err(format!("its {name} box is smaller than its header"));
        }
        let box_end = at
            .checked_add(size)
            .ok_or_else(|| format!("its {name} box is too large"))?;
        if parent.is_some() && box_end > file {
            return Err(format!("its {name} box runs past the end of the file"));
        }
        if depth < MAX_DEPTH
            && let Some(skip) = children_at(reader, kind, parent, at + header)?
        {
            walk(
                reader,
                at + header + skip,
                box_end,
                file,
                Some(kind),
                depth + 1,
            )?;
        }
        at = box_end;
    }
    Ok(())
}

/// The type, size, and header length of the box at `at`, with a size of 0
/// for one that runs to the end of the file. `None` when it cannot be read.
fn read_box<R: Read + Seek>(
    reader: &mut R,
    at: u64,
) -> Result<Option<([u8; 4], u64, u64)>, String> {
    reader
        .seek(SeekFrom::Start(at))
        .map_err(|error| error.to_string())?;
    let mut head = [0; 8];
    if reader.read_exact(&mut head).is_err() {
        return Ok(None);
    }
    let kind = [head[4], head[5], head[6], head[7]];
    match u32::from_be_bytes([head[0], head[1], head[2], head[3]]) {
        1 => {
            let mut large = [0; 8];
            if reader.read_exact(&mut large).is_err() {
                return Ok(None);
            }
            Ok(Some((kind, u64::from_be_bytes(large), 16)))
        }
        size => Ok(Some((kind, u64::from(size), 8))),
    }
}

/// How far into a box's content its child boxes start, for the boxes the
/// crate reads child by child, or `None` for the others.
fn children_at<R: Read + Seek>(
    reader: &mut R,
    kind: [u8; 4],
    parent: Option<[u8; 4]>,
    content: u64,
) -> Result<Option<u64>, String> {
    if parent == Some(*b"ilst") {
        // Title, year, cover, and summary items hold `data` boxes.
        return Ok(matches!(&kind, b"\xa9nam" | b"\xa9day" | b"covr" | b"desc").then_some(0));
    }
    Ok(match &kind {
        b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl" | b"dinf" | b"edts" | b"udta" | b"mvex"
        | b"moof" | b"traf" | b"ilst" => Some(0),
        // Version and flags, then the entries.
        b"dref" => Some(8),
        // Version and flags, which some files leave out and start with
        // `hdlr` instead, as the crate allows.
        b"meta" => {
            reader
                .seek(SeekFrom::Start(content))
                .map_err(|error| error.to_string())?;
            let mut head = [0; 8];
            if reader.read_exact(&mut head).is_err() {
                return Ok(None);
            }
            if head[..4] == [0; 4] {
                Some(4)
            } else if &head[4..] == b"hdlr" {
                Some(0)
            } else {
                None
            }
        }
        _ => None,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Cursor;
    use std::time::Duration;

    /// A box of `kind` holding `content`.
    pub(crate) fn mp4_box(kind: &[u8; 4], content: &[u8]) -> Vec<u8> {
        let mut bytes = ((content.len() + 8) as u32).to_be_bytes().to_vec();
        bytes.extend_from_slice(kind);
        bytes.extend_from_slice(content);
        bytes
    }

    fn ftyp() -> Vec<u8> {
        mp4_box(b"ftyp", b"isom\0\0\x02\0isomiso2")
    }

    /// The 64-byte file that hangs `mp4` 0.14: a track holding a box of
    /// size 0.
    pub(crate) fn zero_size_box() -> Vec<u8> {
        let mut bytes = ftyp();
        let mut empty = 0u32.to_be_bytes().to_vec();
        empty.extend_from_slice(b"free");
        empty.extend_from_slice(&[0; 16]);
        bytes.extend(mp4_box(b"moov", &mp4_box(b"trak", &empty)));
        bytes
    }

    /// The same box of size 0, deep in the metadata.
    fn zero_size_metadata() -> Vec<u8> {
        let mut hdlr = vec![0; 8];
        hdlr.extend_from_slice(b"mdir");
        hdlr.extend_from_slice(&[0; 13]);
        let mut empty = 0u32.to_be_bytes().to_vec();
        empty.extend_from_slice(b"data");
        let mut meta = vec![0; 4];
        meta.extend(mp4_box(b"hdlr", &hdlr));
        meta.extend(mp4_box(b"ilst", &mp4_box(b"\xa9nam", &empty)));
        let mut bytes = ftyp();
        bytes.extend(mp4_box(
            b"moov",
            &mp4_box(b"udta", &mp4_box(b"meta", &meta)),
        ));
        bytes
    }

    /// Runs `read` on its own thread, failing the test if it hangs.
    fn within_a_second<T: Send + 'static>(read: impl FnOnce() -> T + Send + 'static) -> T {
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(read());
        });
        receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("the reader returns instead of hanging")
    }

    #[test]
    fn a_box_of_size_zero_inside_a_track_is_refused() {
        let bytes = zero_size_box();
        assert_eq!(bytes.len(), 64);
        let error = within_a_second(move || {
            let size = bytes.len() as u64;
            read_header(Cursor::new(bytes), size).err()
        });
        assert_eq!(error.as_deref(), Some("its free box has no size"));
    }

    #[test]
    fn a_box_of_size_zero_in_the_metadata_is_refused() {
        let bytes = zero_size_metadata();
        let error = within_a_second(move || {
            let size = bytes.len() as u64;
            read_header(Cursor::new(bytes), size).err()
        });
        assert_eq!(error.as_deref(), Some("its data box has no size"));
    }

    #[test]
    fn a_box_smaller_than_its_header_is_refused() {
        let mut small = 4u32.to_be_bytes().to_vec();
        small.extend_from_slice(b"free");
        let mut bytes = ftyp();
        bytes.extend(mp4_box(b"moov", &small));
        let size = bytes.len() as u64;
        let error = read_header(Cursor::new(bytes), size).err();
        assert_eq!(
            error.as_deref(),
            Some("its free box is smaller than its header")
        );
    }

    #[test]
    fn a_box_running_past_the_file_is_refused() {
        let mut long = 4096u32.to_be_bytes().to_vec();
        long.extend_from_slice(b"free");
        let mut bytes = ftyp();
        bytes.extend(mp4_box(b"moov", &long));
        let size = bytes.len() as u64;
        let error = read_header(Cursor::new(bytes), size).err();
        assert_eq!(
            error.as_deref(),
            Some("its free box runs past the end of the file")
        );
    }

    #[test]
    fn a_real_video_reads() {
        let bytes = include_bytes!("../tests/fixtures/video/sample.mp4");
        let mp4 = read_header(Cursor::new(&bytes[..]), bytes.len() as u64).expect("reads");
        assert!(!mp4.tracks().is_empty());
    }
}
