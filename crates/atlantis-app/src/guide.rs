//! What to tell the player about a folder that is not a game folder, from
//! the names of the files in it (the desktop and the web build share it).

use atlantis_data::GameError;

/// The page of the player's guide on getting the game files.
pub const HELP_URL: &str =
    "https://github.com/ripleyXLR8/atlantis-native/blob/main/docs/GUIDE.md#getting-the-game-files";

/// What to tell the player about the folder `shown` whose files are
/// `names` (lower case), which `Game::open` refused with `err`.
pub fn explain(names: &[String], shown: &str, err: &GameError) -> String {
    let has = |name: &str| names.iter().any(|n| n == name);
    let any_ext = |exts: &[&str]| {
        names
            .iter()
            .any(|n| exts.iter().any(|e| n.ends_with(&format!(".{e}"))))
    };
    match err {
        GameError::MissingArchives(missing) if any_ext(&["iso", "bin", "img", "mdf", "nrg"]) => {
            let list: Vec<String> = missing.iter().map(|n| format!("disc {n}")).collect();
            format!(
                "The disc images in {shown} hold the game, but not all of its four discs. \
                 Add the images of: {}.",
                list.join(", ")
            )
        }
        GameError::MissingArchives(missing) => {
            let list: Vec<String> = missing
                .iter()
                .map(|n| format!("\u{2022} BIGCD{n}.BIG, from disc {n}"))
                .collect();
            format!(
                "The folder {shown} holds the game, but not all of it.\n\n\
                 The CD release spreads its files over four discs, one BIGCD file each. \
                 Copy these files into the folder:\n{}\n\n\
                 (Installing from the CD only copies a few files: the game then reads the \
                 rest from the disc, which Atlantis Native does not do.)",
                list.join("\n")
            )
        }
        GameError::NoArchives => format!(
            "The folder {shown} holds atlantis.exe but none of the game's archives \
             (BIGCD1.BIG to BIGCD4.BIG).\n\n\
             If you installed the game from its CD, the installation only holds a few \
             files: copy BIGCD1.BIG to BIGCD4.BIG from the four discs into this folder \
             (each disc holds one)."
        ),
        GameError::UnknownExecutable(size) => format!(
            "The folder {shown} holds an atlantis.exe ({size} bytes) that Atlantis Native \
             does not know: an edition of the game that is not supported yet.\n\n\
             Supported: the English CD (1997 and 2001), the French CD (1997), the German \
             CD (1997), the Dutch CD (1997), the Swedish CD (1997), the CD with Cryo's \
             patch 1.01, and GOG.com (English, French, German, Spanish, Italian).\n\n\
             You can help support your edition: report it to the project. The report \
             opens on the project's GitHub page, filled in with the files' identifiers \
             (sizes and checksums, no personal data); nothing is sent until you submit it \
             there."
        ),
        GameError::NoExecutable => {
            if any_ext(&["iso", "bin", "cue", "mdf", "mds", "nrg", "img", "chd"]) {
                format!(
                    "The folder {shown} holds disc images that Atlantis Native cannot read \
                     (it reads .iso, .bin, .img, .mdf and .nrg images of the game's CDs, \
                     not compressed ones).\n\n\
                     Open each image (double-click it on Windows 10 and later, or use \
                     7-Zip) and copy atlantis.exe from disc 1 and BIGCD1.BIG to \
                     BIGCD4.BIG from the four discs into one folder, then choose that \
                     folder."
                )
            } else if names
                .iter()
                .any(|n| n.starts_with("setup_atlantis") || n.ends_with(".pkg"))
            {
                format!(
                    "The folder {shown} holds the GOG.com installer, not the installed \
                     game.\n\n\
                     Run the installer first, then choose the folder where it installed \
                     the game."
                )
            } else if has("bigcd1.big") || names.iter().any(|n| n.starts_with("bigcd")) {
                format!(
                    "The folder {shown} holds game archives but not atlantis.exe.\n\n\
                     Copy atlantis.exe from the first disc of the CD into this folder: \
                     Atlantis Native reads the game's tables from it (it never runs it)."
                )
            } else if any_ext(&["zip", "7z", "rar"]) {
                format!(
                    "The folder {shown} holds archives (zip, 7z, rar).\n\n\
                     Extract them first (disc images inside them can then be read as \
                     they are), then choose the folder that holds the extracted files."
                )
            } else {
                format!(
                    "The folder {shown} does not hold Atlantis: The Lost Tales.\n\n\
                     Choose the folder that holds atlantis.exe: the game's installation \
                     folder from GOG.com, or a folder where you copied atlantis.exe and \
                     BIGCD1.BIG to BIGCD4.BIG from the CD."
                )
            }
        }
    }
}

/// The project's Buy Me a Coffee page.
pub const SUPPORT_URL: &str = "https://buymeacoffee.com/ripleyxlr8";

/// The page that opens a new issue on the project.
pub const NEW_ISSUE_URL: &str = "https://github.com/ripleyXLR8/atlantis-native/issues/new";

/// What identifies an edition Atlantis Native does not know, for a report
/// to the project: the executable's and the archives' identifiers and the
/// names of the files at the top of the game folder (never its path, nor
/// anything about the player).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EditionReport {
    pub exe_size: u64,
    pub pe_timestamp: Option<u32>,
    pub exe_md5: String,
    /// The md5 of the first 5000 bytes (ScummVM's detection key).
    pub exe_head_md5: String,
    /// Each BIGCD archive: its name, size and md5 of its first 5000 bytes.
    pub archives: Vec<(String, u64, String)>,
    pub from_images: bool,
    pub files: Vec<String>,
}

fn md5_hex(data: &[u8]) -> String {
    use md5::{Digest, Md5};
    Md5::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

impl EditionReport {
    /// The identifiers of the game in `src`; `None` without atlantis.exe.
    pub fn collect(
        src: &dyn atlantis_data::FileSource,
        files: &[String],
        from_images: bool,
    ) -> Option<EditionReport> {
        let exe = src.read("atlantis.exe")?;
        let archives = (1..=4)
            .filter_map(|n| {
                let name = format!("BIGCD{n}.BIG");
                let size = src.size(&name)?;
                let head = src.read_at(&name, 0, 5000.min(size as usize))?;
                Some((name, size, md5_hex(&head)))
            })
            .collect();
        let mut files: Vec<String> = files.to_vec();
        files.sort();
        files.truncate(60);
        Some(EditionReport {
            exe_size: exe.len() as u64,
            pe_timestamp: atlantis_data::exe::pe_timestamp(&exe),
            exe_md5: md5_hex(&exe),
            exe_head_md5: md5_hex(&exe[..exe.len().min(5000)]),
            archives,
            from_images,
            files,
        })
    }

    /// The issue's title and body (Markdown).
    pub fn issue(&self, platform: &str, version: &str) -> (String, String) {
        let title = format!("New edition: atlantis.exe of {} bytes", self.exe_size);
        let mut body = String::from(
            "Atlantis Native did not recognise this edition of the game.\n\n\
             **Where does this copy come from?** (CD with its language and publisher, \
             DVD, download, patch...): \n\n\
             ## Identifiers (filled in by Atlantis Native)\n\n",
        );
        body.push_str(&format!(
            "| | |\n|---|---|\n| atlantis.exe size | {} |\n| PE timestamp | {} |\n\
             | md5 | `{}` |\n| md5 of the first 5000 bytes | `{}` |\n| read from disc images | {} |\n",
            self.exe_size,
            self.pe_timestamp.map_or("none".to_string(), |t| format!("0x{t:08x}")),
            self.exe_md5,
            self.exe_head_md5,
            if self.from_images { "yes" } else { "no" },
        ));
        for (name, size, head) in &self.archives {
            body.push_str(&format!("| {name} | {size} bytes, head md5 `{head}` |\n"));
        }
        body.push_str(&format!(
            "\nFiles in the folder: {}\n\n{version} on {platform}\n",
            self.files.join(", ")
        ));
        (title, body)
    }

    /// The link that opens the issue, filled in, on the project's page.
    pub fn url(&self, platform: &str, version: &str) -> String {
        let (title, body) = self.issue(platform, version);
        format!(
            "{NEW_ISSUE_URL}?labels=new%20edition&title={}&body={}",
            percent_encode(&title),
            percent_encode(&body)
        )
    }
}

/// `s` encoded for a URL's query (everything but the unreserved
/// characters).
pub fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlantis_data::MemFiles;

    #[test]
    fn report_of_an_unknown_edition() {
        let mut files = MemFiles::new();
        files.insert("atlantis.exe", b"MZ not a real exe".to_vec());
        files.insert("BIGCD1.BIG", vec![1u8; 9000]);
        let names = vec!["ATLANTIS.EXE".to_string(), "BIGCD1.BIG".to_string()];
        let r = EditionReport::collect(&files, &names, false).unwrap();
        assert_eq!(r.exe_size, 17);
        assert_eq!(r.pe_timestamp, None);
        assert_eq!(r.archives.len(), 1);
        assert_eq!(r.archives[0].1, 9000);
        let url = r.url("windows", "Atlantis Native 0.1.0");
        assert!(url.starts_with(NEW_ISSUE_URL));
        assert!(url.contains("title=New%20edition%3A%20atlantis.exe%20of%2017%20bytes"));
        assert!(!url.contains(' ') && !url.contains('\n'));
        let (_, body) = r.issue("windows", "v");
        assert!(body.contains(&r.exe_md5) && body.contains("BIGCD1.BIG"));
        assert_eq!(percent_encode("a b/é"), "a%20b%2F%C3%A9");
    }

    /// The report of a real edition read from its disc images
    /// (`ATLANTIS_ISO_UNKNOWN`: a folder of images of an edition not
    /// supported, skipped when the edition opens): every identifier, and a
    /// link short enough for GitHub.
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn report_from_disc_images() {
        let Ok(dir) = std::env::var("ATLANTIS_ISO_UNKNOWN") else {
            eprintln!("ATLANTIS_ISO_UNKNOWN not set: skipped");
            return;
        };
        let set = atlantis_data::disc_image::DiscSet::open_folder(dir.as_ref()).unwrap();
        let set = std::sync::Arc::new(set);
        if let Ok(g) = atlantis_data::Game::open(set.clone()) {
            eprintln!(
                "ATLANTIS_ISO_UNKNOWN holds a supported edition ({}): skipped",
                g.edition()
            );
            return;
        }
        let names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_ascii_lowercase())
            .collect();
        let r = EditionReport::collect(&*set, &names, true).unwrap();
        assert_eq!(r.archives.len(), 4);
        let url = r.url("linux", "Atlantis Native test");
        eprintln!("{}\n{} characters", r.issue("linux", "test").1, url.len());
        assert!(url.len() < 8000);
    }
}
