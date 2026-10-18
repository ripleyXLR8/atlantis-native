//! A CD folder that lacks some of the four discs' archives is refused with
//! their numbers, so that the player is told which discs to copy.
use std::sync::Arc;

use atlantis_data::{Game, GameError, StdDir};

#[test]
fn cd_folder_with_disc_1_only() {
    let Ok(dir) = std::env::var("ATLANTIS_DATA") else {
        eprintln!("ATLANTIS_DATA not set: skipped");
        return;
    };
    let tmp = std::env::temp_dir().join(format!("atlantis-missing-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::copy(format!("{dir}/atlantis.exe"), tmp.join("atlantis.exe")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(format!("{dir}/BIGCD1.BIG"), tmp.join("BIGCD1.BIG")).unwrap();
    #[cfg(not(unix))]
    return;
    match Game::open(Arc::new(StdDir::new(&tmp))) {
        Err(GameError::MissingArchives(n)) => assert_eq!(n, vec![2, 3, 4]),
        Err(e) => panic!("unexpected error {e}"),
        Ok(_) => panic!("opened without discs 2..4"),
    }
    std::fs::remove_file(tmp.join("BIGCD1.BIG")).unwrap();
    assert!(matches!(
        Game::open(Arc::new(StdDir::new(&tmp))),
        Err(GameError::NoArchives)
    ));
    let _ = std::fs::remove_dir_all(&tmp);
}
