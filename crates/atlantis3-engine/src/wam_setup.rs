//! The show / hide commands `CHANGEWAM` runs after loading a place
//! (`Game.exe` @ 0x0044f0c0): one function per place, each a list of
//! `RunOneCommand("(...)", 0)` calls chosen by story flags. The flags are
//! read at their addresses in the flag array (@ 0x0077c4ec, 4 bytes each);
//! the commands are the executable's strings, case as written.
//!
//! Ported: the funeral chamber (`cfuner.wam`, @ 0x0044f460) and the Hoggar
//! (`hoggar.wam`, @ 0x0044f4e0). `couloir`, `rhea` and `tourmagi` call an
//! empty function. The others (`interdim` @ 0x0044f9c0, `jardin`
//! @ 0x0044f9d0, `ldmorts` @ 0x0044fd60, `mmarchd` @ 0x004501c0, `paleo`
//! @ 0x004502b0, `pmagi` @ 0x00450510) are not ported yet: logged.

/// The commands for place `wam` (any case, with `.wam`) under `flags`.
pub fn commands(wam: &str, flags: &[i32]) -> Vec<String> {
    let f = |n: usize| flags.get(n).copied().unwrap_or(0);
    let mut out: Vec<&'static str> = Vec::new();
    match wam.to_ascii_lowercase().as_str() {
        "cfuner.wam" => cfuner(&f, &mut out),
        "hoggar.wam" => hoggar(&f, &mut out),
        "couloir.wam" | "rhea.wam" | "tourmagi.wam" => {}
        other => log::warn!("{other}: its CHANGEWAM commands are not ported yet"),
    }
    out.into_iter().map(String::from).collect()
}

/// @ 0x0044f460 (flags 46 @ 0x0077c5a4, 45 @ 0x0077c5a0).
fn cfuner(f: &dyn Fn(usize) -> i32, out: &mut Vec<&'static str>) {
    out.push(if f(46) == 1 {
        "(hideobjet=CHF_OISEAUBOIS)"
    } else {
        "(showobjet=CHF_OISEAUBOIS)"
    });
    if f(45) == 1 {
        out.extend([
            "(hideobjet=CHF_COUVERCLE01)",
            "(showobjet=CHF_COUVERCLE02)",
            "(showperso=CHF_PSYCHOP)",
        ]);
    } else {
        out.extend([
            "(showobjet=CHF_COUVERCLE01)",
            "(hideobjet=CHF_COUVERCLE02)",
            "(hideperso=CHF_PSYCHOP)",
        ]);
    }
}

/// @ 0x0044f4e0.
fn hoggar(f: &dyn Fn(usize) -> i32, out: &mut Vec<&'static str>) {
    let pick = |c: bool, a: &'static str, b: &'static str| if c { a } else { b };
    out.push(pick(
        f(416) == 0,
        "(hideobjet=HOG_DISQUE_TOUPIE)",
        "(showobjet=HOG_DISQUE_TOUPIE)",
    ));
    if f(13) == 0 {
        out.push("(showobjet=CMP_MENBRANE)");
        let stars = [
            ("(showobjet=HOG_ETOILE01)", "(hideobjet=HOG_ETOILE01)"),
            ("(showobjet=HOG_ETOILE02)", "(hideobjet=HOG_ETOILE02)"),
            ("(showobjet=HOG_ETOILE03)", "(hideobjet=HOG_ETOILE03)"),
            ("(showobjet=HOG_ETOILE04)", "(hideobjet=HOG_ETOILE04)"),
            ("(showobjet=HOG_ETOILE05)", "(hideobjet=HOG_ETOILE05)"),
        ];
        for (k, (show, hide)) in stars.into_iter().enumerate() {
            out.push(pick(f(420 + k) == 1, show, hide));
        }
    } else {
        out.extend([
            "(hideobjet=CMP_MENBRANE)",
            "(hideobjet=HOG_ETOILE01)",
            "(hideobjet=HOG_ETOILE02)",
            "(hideobjet=HOG_ETOILE03)",
            "(hideobjet=HOG_ETOILE04)",
            "(hideobjet=HOG_ETOILE05)",
        ]);
    }
    if f(15) == 1 && f(10) == 0 {
        out.extend([
            "(ShowPerso=HOG_BOSS)",
            "(ShowPerso=HOG_GARDIEN03)",
            "(ShowPerso=HOG_TOUAREGBLESSE)",
        ]);
    } else {
        out.extend([
            "(HidePerso=HOG_BOSS)",
            "(HidePerso=HOG_GARDIEN03)",
            "(HidePerso=HOG_TOUAREGBLESSE)",
        ]);
    }
    if f(9) == 1 && f(10) == 0 {
        out.extend(["(ShowPerso=CMP_BOSS)", "(ShowPerso=CMP_GARDIEN03)"]);
    } else {
        out.extend(["(HidePerso=CMP_BOSS)", "(HidePerso=CMP_GARDIEN03)"]);
    }
    if f(25) == 1 || f(26) == 1 || f(27) == 1 {
        out.extend(["(ShowPerso=CMP_BOSS)", "(ShowPerso=CMP_GARDIEN03)"]);
    }
    if f(28) == 1 {
        out.extend(["(HidePerso=CMP_BOSS)", "(ShowPerso=CMP_BOSS2)"]);
    } else {
        out.push("(HidePerso=CMP_BOSS2)");
    }
    out.push(pick(
        f(10) == 1 && f(27) == 0,
        "(ShowPerso=GRO_GARDIEN03)",
        "(HidePerso=GRO_GARDIEN03)",
    ));
    out.push(pick(
        f(6) == 1 && f(8) == 0,
        "(showobjet=HOG_OBJ_CRANE)",
        "(hideobjet=HOG_OBJ_CRANE)",
    ));
    if f(425) == 1 {
        out.extend(["(ShowPerso=CMP_ATLANTE)", "(HidePerso=CMP_ATLANTEFIGE)"]);
    } else {
        out.extend(["(ShowPerso=CMP_ATLANTEFIGE)", "(HidePerso=CMP_ATLANTE)"]);
    }
    out.push(pick(
        f(21) == 1 && f(22) == 0,
        "(showobjet=CMP_CLOCHE)",
        "(hideobjet=CMP_CLOCHE)",
    ));
    out.push(pick(
        f(22) == 1 && f(23) == 0,
        "(showobjet=CMP_TELECOMMANDE)",
        "(hideobjet=CMP_TELECOMMANDE)",
    ));
    // The five portals' objects (@ 0x0044f7f0..0x0044f8f9).
    const H: [&str; 2] = ["(ShowObjet=CMP_HOGGAR)", "(HideObjet=CMP_HOGGAR)"];
    const B: [&str; 2] = ["(ShowObjet=CMP_BAGDAD)", "(HideObjet=CMP_BAGDAD)"];
    const BF: [&str; 2] = ["(ShowObjet=CMP_BAGDADFIN)", "(HideObjet=CMP_BAGDADFIN)"];
    const P: [&str; 2] = ["(ShowObjet=CMP_PALEO)", "(HideObjet=CMP_PALEO)"];
    const PF: [&str; 2] = ["(ShowObjet=CMP_PALEOFIN)", "(HideObjet=CMP_PALEOFIN)"];
    let shown = if f(400) == 1 {
        [1, 0, 1, 1, 1]
    } else if f(401) == 1 {
        [1, 1, 0, 1, 1]
    } else if f(402) == 1 {
        [1, 1, 1, 0, 1]
    } else if f(403) == 1 {
        [1, 1, 1, 1, 0]
    } else if f(404) == 1 {
        [0, 1, 1, 1, 1]
    } else {
        [1, 1, 1, 1, 1]
    };
    for (k, set) in [H, B, BF, P, PF].into_iter().enumerate() {
        out.push(set[shown[k]]);
    }
    out.push(pick(
        f(30) == 1,
        "(HidePerso=HOG_GARDIEN04)",
        "(ShowPerso=HOG_GARDIEN04)",
    ));
    out.push(pick(
        f(31) == 1,
        "(HideObjet=CMP_TORCHE_ELECTRIQUE)",
        "(ShowObjet=CMP_TORCHE_ELECTRIQUE)",
    ));
    out.push(pick(
        f(32) == 1,
        "(HideObjet=CMP_CLEF1)",
        "(ShowObjet=CMP_CLEF1)",
    ));
    out.push(pick(
        f(34) == 1,
        "(HideObjet=CMP_COUTEAU)",
        "(ShowObjet=CMP_COUTEAU)",
    ));
    if f(36) == 1 {
        out.extend(["(HidePerso=HOG_TOUAREGBL)", "(ShowPerso=HOG_TOUAREGLI)"]);
    } else {
        out.extend(["(ShowPerso=HOG_TOUAREGBL)", "(HidePerso=HOG_TOUAREGLI)"]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_portals_follow_their_flags() {
        let mut flags = vec![0; 0x800];
        let c = commands("Hoggar.wam", &flags);
        assert_eq!(c.len(), 30);
        assert!(c.contains(&"(HideObjet=CMP_HOGGAR)".to_string()));
        flags[402] = 1;
        let c = commands("hoggar.wam", &flags);
        assert!(c.contains(&"(ShowObjet=CMP_PALEO)".to_string()));
        assert!(c.contains(&"(HideObjet=CMP_BAGDAD)".to_string()));
        assert_eq!(commands("CFuner.wam", &flags).len(), 4);
    }
}
