//! Jumping between source and PDF with the `.synctex.gz` file a compile
//! writes, without needing the `synctex` program (Tectonic doesn't ship one).
//!
//! The file lists, page by page, every box and glyph run TeX placed, each
//! tagged with the input file and line it came from:
//!
//! ```text
//! Input:1:C:\proj\main.tex          <- tag 1
//! {1                                <- page 1
//! (1,3:8799518,8865054:22609920,647495,0   <- hbox from tag 1 line 3: x,y:W,H,D
//! x1,3:9330359,8865054              <- a kern at x,y
//! }1
//! ```
//!
//! Positions are in TeX scaled points from the page's top-left corner.
//! Everything this module returns is in PDF points (1/72 inch), also from
//! the top-left, which is what the PDF view draws in.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Scaled points per PDF point (65536 sp per TeX point, 72.27 / 72).
const SP_PER_BP: f64 = 65781.76;

/// A place on a page, in PDF points from the top-left corner.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct Spot {
    /// 1-based page number.
    pub page: u32,
    /// Left edge.
    pub x: f64,
    /// Top edge.
    pub y: f64,
    /// Width (0 for a single point).
    pub width: f64,
    /// Height (0 for a single point).
    pub height: f64,
}

/// A line of a source file.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SourceLine {
    /// The input file as the compile named it.
    pub file: PathBuf,
    /// 1-based line.
    pub line: u32,
}

#[derive(Debug, Clone, Copy)]
struct Record {
    kind: u8,
    tag: u32,
    line: u32,
    x: i64,
    y: i64,
    w: i64,
    h: i64,
    d: i64,
    // the printed line this sits in: the outermost horizontal box inside the
    // nearest vertical one (a paragraph's line), as an index into the page
    line_box: Option<usize>,
}

impl Record {
    fn has_box(&self) -> bool {
        matches!(self.kind, b'(' | b'[' | b'h' | b'v') && self.w > 0
    }
}

/// A parsed SyncTeX file.
#[derive(Debug, Default)]
pub struct Synctex {
    inputs: HashMap<u32, PathBuf>,
    // sp -> bp, with the file's Unit and Magnification folded in
    scale: f64,
    x_offset: f64,
    y_offset: f64,
    pages: Vec<(u32, Vec<Record>)>,
}

/// Why a SyncTeX file couldn't be read.
#[derive(Debug, thiserror::Error)]
pub enum SynctexError {
    /// The file couldn't be read or unpacked.
    #[error("couldn't read the SyncTeX file: {0}")]
    Io(#[from] std::io::Error),
    /// It isn't SyncTeX.
    #[error("not a SyncTeX file")]
    Format,
}

fn num(s: &str) -> Option<i64> {
    s.trim().parse().ok()
}

// "tag,line[,column]:x,y[:W[,H,D]]" after the record's kind character
fn parse_record(kind: u8, rest: &str) -> Option<Record> {
    let mut parts = rest.split(':');
    let mut tl = parts.next()?.split(',');
    let tag = tl.next()?.trim().parse().ok()?;
    let line = tl.next()?.trim().parse().ok()?;
    let mut xy = parts.next()?.split(',');
    let x = num(xy.next()?)?;
    let y = num(xy.next()?)?;
    let (mut w, mut h, mut d) = (0, 0, 0);
    if let Some(size) = parts.next() {
        let mut s = size.split(',');
        w = s.next().and_then(num).unwrap_or(0);
        h = s.next().and_then(num).unwrap_or(0);
        d = s.next().and_then(num).unwrap_or(0);
    }
    Some(Record {
        kind,
        tag,
        line,
        x,
        y,
        w,
        h,
        d,
        line_box: None,
    })
}

impl Synctex {
    /// Reads `name.synctex.gz` (or an uncompressed `name.synctex`).
    pub fn load(path: &Path) -> Result<Self, SynctexError> {
        let raw = std::fs::read(path)?;
        let text = if raw.starts_with(&[0x1f, 0x8b]) {
            let mut s = String::new();
            flate2::read::GzDecoder::new(&raw[..]).read_to_string(&mut s)?;
            s
        } else {
            String::from_utf8_lossy(&raw).into_owned()
        };
        Self::parse(&text)
    }

    /// Parses the text of a SyncTeX file.
    pub fn parse(text: &str) -> Result<Self, SynctexError> {
        if !text.starts_with("SyncTeX Version:") {
            return Err(SynctexError::Format);
        }
        let mut s = Synctex::default();
        let (mut unit, mut mag) = (1.0, 1000.0);
        let mut current: Option<(u32, Vec<Record>)> = None;
        // open boxes on the current page: (kind, index into the page's records)
        let mut open: Vec<(u8, usize)> = Vec::new();
        for line in text.lines() {
            let Some(&first) = line.as_bytes().first() else {
                continue;
            };
            if let Some(rest) = line.strip_prefix("Input:") {
                if let Some((tag, path)) = rest.split_once(':') {
                    if let Ok(tag) = tag.parse() {
                        s.inputs.insert(tag, PathBuf::from(path));
                    }
                }
            } else if let Some(v) = line.strip_prefix("Unit:") {
                unit = v.trim().parse().unwrap_or(1.0);
            } else if let Some(v) = line.strip_prefix("Magnification:") {
                mag = v.trim().parse().unwrap_or(1000.0);
            } else if let Some(v) = line.strip_prefix("X Offset:") {
                s.x_offset = v.trim().parse().unwrap_or(0.0);
            } else if let Some(v) = line.strip_prefix("Y Offset:") {
                s.y_offset = v.trim().parse().unwrap_or(0.0);
            } else if first == b'{' {
                if let Ok(n) = line[1..].trim().parse() {
                    current = Some((n, Vec::new()));
                    open.clear();
                }
            } else if first == b'}' {
                if let Some(page) = current.take() {
                    s.pages.push(page);
                }
            } else if let Some((_, recs)) = current.as_mut() {
                match first {
                    b')' | b']' => {
                        open.pop();
                    }
                    b'(' | b'[' | b'h' | b'v' | b'x' | b'k' | b'g' | b'$' => {
                        if let Some(mut r) = parse_record(first, &line[1..]) {
                            let index = recs.len();
                            // the hbox just inside the innermost open vbox
                            let after_vbox = open
                                .iter()
                                .rposition(|(k, _)| *k == b'[')
                                .map(|p| p + 1)
                                .unwrap_or(0);
                            r.line_box = open[after_vbox..]
                                .iter()
                                .find(|(k, _)| *k == b'(')
                                .map(|(_, i)| *i)
                                .or((first == b'(').then_some(index));
                            recs.push(r);
                            if matches!(first, b'(' | b'[') {
                                open.push((first, index));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        if let Some(page) = current.take() {
            s.pages.push(page);
        }
        s.scale = unit * (mag / 1000.0) / SP_PER_BP;
        Ok(s)
    }

    fn bp_x(&self, sp: i64) -> f64 {
        (sp as f64 + self.x_offset) * self.scale
    }

    fn bp_y(&self, sp: i64) -> f64 {
        (sp as f64 + self.y_offset) * self.scale
    }

    fn bp(&self, sp: i64) -> f64 {
        sp as f64 * self.scale
    }

    // the input tag for a source file: the one whose path ends with the most
    // of the file's components, compared without regard to case on Windows
    fn tag_for(&self, file: &Path) -> Option<u32> {
        let parts = |p: &Path| -> Vec<String> {
            p.to_string_lossy()
                .replace('\\', "/")
                .split('/')
                .filter(|c| !c.is_empty() && *c != ".")
                .map(|c| {
                    if cfg!(windows) {
                        c.to_lowercase()
                    } else {
                        c.to_string()
                    }
                })
                .collect()
        };
        let want = parts(file);
        self.inputs
            .iter()
            .filter_map(|(tag, path)| {
                let have = parts(path);
                let common = want
                    .iter()
                    .rev()
                    .zip(have.iter().rev())
                    .take_while(|(a, b)| a == b)
                    .count();
                // at least the file name, and all of the shorter path
                (common > 0 && (common == want.len() || common == have.len()))
                    .then_some((common, *tag))
            })
            .max()
            .map(|(_, tag)| tag)
    }

    /// Where a source line ended up in the PDF: one box per page it's on,
    /// first page first. Lines that produced nothing (a comment, a blank
    /// line) go to the nearest line after them that did.
    pub fn forward(&self, file: &Path, line: u32) -> Vec<Spot> {
        let Some(tag) = self.tag_for(file) else {
            return Vec::new();
        };
        let lines: Vec<u32> = self
            .pages
            .iter()
            .flat_map(|(_, recs)| recs.iter())
            .filter(|r| r.tag == tag)
            .map(|r| r.line)
            .collect();
        let target = lines
            .iter()
            .copied()
            .filter(|l| *l >= line)
            .min()
            .or_else(|| lines.iter().copied().filter(|l| *l < line).max());
        let Some(target) = target else {
            return Vec::new();
        };

        let mut out = Vec::new();
        for (page, recs) in &self.pages {
            let hits: Vec<&Record> = recs
                .iter()
                .filter(|r| r.tag == tag && r.line == target)
                .collect();
            if hits.is_empty() {
                continue;
            }
            // the printed lines the source line produced; records outside any
            // line (rare) count as points on their baseline
            let mut boxes: Vec<&Record> = hits
                .iter()
                .filter_map(|r| r.line_box.map(|i| &recs[i]))
                .collect();
            boxes.sort_by_key(|r| (r.y, r.x));
            boxes.dedup_by_key(|r| (r.x, r.y, r.w));
            let (x0, y0, x1, y1) = if boxes.is_empty() {
                let xs = hits.iter().map(|r| self.bp_x(r.x));
                let ys = hits.iter().map(|r| self.bp_y(r.y));
                let (minx, maxx) = xs.fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(v), b.max(v)));
                let (miny, maxy) = ys.fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(v), b.max(v)));
                // a point has no height: give it a line's worth above the baseline
                (minx, miny - 10.0, maxx.max(minx + 1.0), maxy + 2.0)
            } else {
                boxes.iter().fold(
                    (f64::MAX, f64::MAX, f64::MIN, f64::MIN),
                    |(a, b, c, d), r| {
                        (
                            a.min(self.bp_x(r.x)),
                            b.min(self.bp_y(r.y) - self.bp(r.h)),
                            c.max(self.bp_x(r.x) + self.bp(r.w)),
                            d.max(self.bp_y(r.y) + self.bp(r.d)),
                        )
                    },
                )
            };
            out.push(Spot {
                page: *page,
                x: x0,
                y: y0,
                width: x1 - x0,
                height: y1 - y0,
            });
        }
        out
    }

    /// The source line behind a point on a page (PDF points from the
    /// top-left), for a double-click in the PDF view.
    pub fn backward(&self, page: u32, x: f64, y: f64) -> Option<SourceLine> {
        let recs = &self.pages.iter().find(|(n, _)| *n == page)?.1;
        let inside = |r: &Record| {
            let (rx, ry) = (self.bp_x(r.x), self.bp_y(r.y));
            x >= rx && x <= rx + self.bp(r.w) && y >= ry - self.bp(r.h) && y <= ry + self.bp(r.d)
        };
        // the smallest horizontal box around the point is the line clicked
        let line_box = recs
            .iter()
            .filter(|r| r.has_box() && r.kind != b'[' && r.kind != b'v' && inside(r))
            .min_by(|a, b| (a.w * (a.h + a.d)).cmp(&(b.w * (b.h + b.d))));
        let pick = match line_box {
            Some(b) => {
                // within it, the glyph run nearest the click says which source
                // line it came from (a paragraph spans many)
                let top = self.bp_y(b.y) - self.bp(b.h);
                let bottom = self.bp_y(b.y) + self.bp(b.d);
                recs.iter()
                    .filter(|r| !r.has_box())
                    .filter(|r| {
                        let ry = self.bp_y(r.y);
                        ry >= top - 0.5 && ry <= bottom + 0.5
                    })
                    .min_by(|a, b| {
                        let da = (self.bp_x(a.x) - x).abs();
                        let db = (self.bp_x(b.x) - x).abs();
                        da.total_cmp(&db)
                    })
                    .unwrap_or(b)
            }
            None => recs.iter().min_by(|a, b| {
                let d = |r: &Record| (self.bp_x(r.x) - x).powi(2) + (self.bp_y(r.y) - y).powi(2);
                d(a).total_cmp(&d(b))
            })?,
        };
        Some(SourceLine {
            file: self.inputs.get(&pick.tag)?.clone(),
            line: pick.line,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Synctex {
        let text = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.synctex"),
        )
        .unwrap();
        Synctex::parse(&text).unwrap()
    }

    fn root(s: &Synctex) -> PathBuf {
        s.inputs[&1].parent().unwrap().to_path_buf()
    }

    // the fixture was made on Windows, and Unix paths don't split on `\`
    fn named(p: &Path, name: &str) -> bool {
        p.to_string_lossy()
            .rsplit(['/', '\\'])
            .next()
            .is_some_and(|last| last == name)
    }

    #[test]
    fn forward_matches_the_synctex_program() {
        // `synctex view -i 3:1:chapters/one.tex` on the real file gave
        // page 1, h 133.77, v 182.43 (baseline), W 343.71, H 8.86
        let s = sample();
        let spots = s.forward(&root(&s).join("chapters").join("one.tex"), 3);
        assert_eq!(spots.len(), 1, "{spots:?}");
        let sp = spots[0];
        assert_eq!(sp.page, 1);
        assert!((sp.x - 133.77).abs() < 0.5, "{sp:?}");
        assert!((sp.width - 343.71).abs() < 0.5, "{sp:?}");
        // the box ends just under the baseline the program reports
        assert!(sp.y < 182.43 && sp.y + sp.height >= 182.0, "{sp:?}");
    }

    #[test]
    fn forward_finds_files_by_relative_name_and_skips_blank_lines() {
        let s = sample();
        let a = s.forward(Path::new("chapters/one.tex"), 3);
        assert_eq!(a.len(), 1);
        // past the end of the file: the last line that made text answers
        let b = s.forward(Path::new("chapters/one.tex"), 40);
        assert_eq!(b, a);
        // a blank line ends its paragraph, so it lands on that paragraph
        let blank = s.forward(Path::new("chapters/one.tex"), 2);
        assert_eq!(blank, s.forward(Path::new("chapters/one.tex"), 1));
        assert!(s.forward(Path::new("nope.tex"), 1).is_empty());
    }

    #[test]
    fn backward_matches_the_synctex_program() {
        // `synctex edit -o 1:200:140` -> main.tex line 3 (the section title);
        // `-o 1:150:105` -> main.tex line 9
        let s = sample();
        let hit = s.backward(1, 200.0, 140.0).unwrap();
        assert!(named(&hit.file, "main.tex"), "{hit:?}");
        assert_eq!(hit.line, 3);
        let near_top = s.backward(1, 150.0, 105.0).unwrap();
        assert!(named(&near_top.file, "main.tex"));
        assert!(s.backward(9, 1.0, 1.0).is_none());
    }

    #[test]
    fn round_trip_lands_on_the_same_line() {
        let s = sample();
        let file = root(&s).join("chapters").join("one.tex");
        for line in [1, 3] {
            let spot = s.forward(&file, line)[0];
            let back = s
                .backward(
                    spot.page,
                    spot.x + spot.width / 4.0,
                    spot.y + spot.height * 0.8,
                )
                .unwrap();
            assert!(named(&back.file, "one.tex"), "{back:?}");
            assert_eq!(back.line, line);
        }
    }

    #[test]
    fn rejects_other_files() {
        assert!(matches!(Synctex::parse("hello"), Err(SynctexError::Format)));
    }
}
