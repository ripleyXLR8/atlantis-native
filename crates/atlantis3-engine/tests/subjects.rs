//! The subject menu against the original's own code
//! (`tests/golden/subjects_layout.txt`, emulated): the subject keys'
//! effect on the table (`OP` / `TABLE`), the cells drawn (`DRAW`), the
//! first row (`YSTART`) and the hit test at points around every cell
//! (`HITROW`), for 1 to 16 subjects at 640, 800 and 1024 pixels wide.
//!
//! Section A needs `scenar\defnames.con` (`ATLANTIS3_DATA`).

use atlantis2_engine::script::{Script, Subject, SubjectTable};
use atlantis3_data::A3Files;
use atlantis3_engine::subjects::Layout;

const GOLDEN: &str = include_str!("golden/subjects_layout.txt");

fn field<'a>(l: &'a str, key: &str) -> &'a str {
    l.split(' ')
        .find_map(|w| w.strip_prefix(key))
        .unwrap_or_else(|| panic!("{key} in {l}"))
}

/// The harness's list for `n` subjects: entry i = {i % 12, 0, flag 0 for
/// odd i}, the rest of the table freshly reset.
fn table(n: usize) -> (SubjectTable, i32) {
    let mut t = SubjectTable::new();
    t.reset_a3();
    let p = 9;
    for i in 0..n {
        t.set_entry(
            p,
            i,
            Subject {
                value: (i % 12) as i32,
                index: 0,
                flag: u8::from(i % 2 == 0),
            },
        );
    }
    t.set_count(p, n as i32);
    (t, p)
}

#[test]
fn layout_and_hit_test_as_the_original() {
    let mut draws = 0;
    let mut hits = 0;
    for l in GOLDEN.lines() {
        let w: Vec<&str> = l.split(' ').collect();
        match w[0] {
            "DRAW" => {
                let (width, n, i): (usize, usize, usize) = (
                    w[1].parse().unwrap(),
                    w[2].parse().unwrap(),
                    w[3].parse().unwrap(),
                );
                let lay = Layout::for_width(width);
                let (t, p) = table(n);
                let c = lay.cells(&t.list(p))[i];
                let got = format!(
                    "spr={} x={} y={} alpha={:?} ring=#{} rx={} ry={} ralpha=1.0",
                    c.value, c.x, c.y, c.alpha, lay.ring, c.ring.0, c.ring.1
                );
                assert_eq!(got, w[4..].join(" "), "{l}");
                draws += 1;
            }
            "YSTART" => {
                let lay = Layout::for_width(w[1].parse().unwrap());
                assert_eq!(
                    lay.ystart(w[2].parse().unwrap()),
                    w[3].parse::<i32>().unwrap()
                );
            }
            "HITROW" => {
                let lay = Layout::for_width(w[1].parse().unwrap());
                let n: usize = w[2].parse().unwrap();
                let (t, p) = table(n);
                let y: i32 = field(l, "y=").parse().unwrap();
                for cell in &w[6..] {
                    let (x, want) = cell.split_once(':').unwrap();
                    let got = lay.pick(&t, p, (x.parse().unwrap(), y));
                    assert_eq!(got, want.parse::<i32>().unwrap(), "{l} at x={x}");
                    hits += 1;
                }
            }
            _ => {}
        }
    }
    assert_eq!(draws, 408);
    assert!(hits > 20_000, "{hits}");
}

#[test]
fn subject_keys_as_the_original() {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let files = A3Files::open_dir(&dir).unwrap().0;
    let defnames = files.read("scenar/defnames.con").unwrap();
    let mut s = Script::new_a3(&defnames).unwrap();
    s.subjects.reset_a3();
    // A chapter in memory: the key search is bounded by its size.
    assert!(s.set_chapter_text(&[b' '; 64]));
    struct NoHost;
    impl atlantis2_engine::script::Host for NoHost {
        fn read_chapter(&mut self, _: i32) -> Option<Vec<u8>> {
            None
        }
    }
    let mut checked = 0;
    for l in GOLDEN.lines() {
        if let Some(op) = l.strip_prefix("OP ") {
            let (key, rest) = op.split_once(' ').unwrap();
            s.run_command(&mut NoHost, format!("({key},{rest}").as_bytes());
        } else if let Some(t) = l.strip_prefix("TABLE ") {
            let id: i32 = t
                .split_once('(')
                .unwrap()
                .1
                .split_once(')')
                .unwrap()
                .0
                .parse()
                .unwrap();
            let got: Vec<String> = s
                .subjects
                .list(id)
                .iter()
                .map(|e| format!("{}/{}/{}", e.value, e.index, e.flag))
                .collect();
            let want = field(t, "entries=");
            assert_eq!(got.join(","), want, "{l}");
            assert_eq!(s.subjects.count(id).to_string(), field(t, "count="));
            checked += 1;
        }
    }
    assert_eq!(checked, 13);
}
