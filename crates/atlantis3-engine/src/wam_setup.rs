//! The show / hide commands `CHANGEWAM` runs after loading a place
//! (`Game.exe` @ 0x0044f0c0): one function per place, each a list of
//! `RunOneCommand("(...)", 0)` calls chosen by story flags. The flags are
//! read at their addresses in the flag array (@ 0x0077c4ec, 4 bytes each);
//! the commands are the executable's strings, case as written.
//!
//! Every place: the funeral chamber (`cfuner.wam`, @ 0x0044f460), the
//! Hoggar (`hoggar.wam`, @ 0x0044f4e0), the gardens (`jardin.wam`,
//! @ 0x0044f9d0), the Land of the Dead (`ldmorts.wam`, @ 0x0044fd60), the
//! market (`mmarchd.wam`, @ 0x004501c0), the Palaeolithic (`paleo.wam`,
//! @ 0x004502b0) and the magus's palace (`pmagi.wam`, @ 0x00450510).
//! `couloir`, `rhea` and `tourmagi` call an empty function (0x0045fda0);
//! `interdim`'s (@ 0x0044f9c0) only sets the interdimension flag
//! (0x0077d2f0), which `CHANGEWAM` sets anyway (0x0044b910): no command.

/// The commands for place `wam` (any case, with `.wam`) under `flags`.
pub fn commands(wam: &str, flags: &[i32]) -> Vec<String> {
    let f = |n: usize| flags.get(n).copied().unwrap_or(0);
    let mut out: Vec<&'static str> = Vec::new();
    match wam.to_ascii_lowercase().as_str() {
        "cfuner.wam" => cfuner(&f, &mut out),
        "hoggar.wam" => hoggar(&f, &mut out),
        "jardin.wam" => jardin(&f, &mut out),
        "ldmorts.wam" => ldmorts(&f, &mut out),
        "mmarchd.wam" => mmarchd(&f, &mut out),
        "paleo.wam" => paleo(&f, &mut out),
        "pmagi.wam" => pmagi(&f, &mut out),
        "couloir.wam" | "rhea.wam" | "tourmagi.wam" | "interdim.wam" => {}
        other => log::warn!("{other}: no CHANGEWAM commands (not a game place)"),
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

/// @ 0x0044fd60.
fn ldmorts(f: &dyn Fn(usize) -> i32, out: &mut Vec<&'static str>) {
    let mut pv: &'static str;
    if f(85) == 1 {
        pv = "(hideobjet=LDM_PLUMES)";
    } else {
        pv = "(showobjet=LDM_PLUMES)";
    }
    out.push(pv);
    out.push("(HideObjet=LDM_FRESQUE1)");
    out.push("(HideObjet=LDM_FRESQUE2)");
    out.push("(HideObjet=LDM_FRESQUE3)");
    if f(80) == 1 {
        out.push("(ShowObjet=LDM_FRESQUE1)");
    }
    if f(44) == 1 {
        out.push("(HideObjet=LDM_FRESQUE1)");
        out.push("(ShowObjet=LDM_FRESQUE2)");
    }
    if f(54) == 1 {
        out.push("(ShowObjet=LDM_ANKHPR)");
        out.push("(HideObjet=LDM_FRESQUE2)");
        out.push("(ShowObjet=LDM_FRESQUE3)");
    }
    if (f(87) == 1) && (f(69) == 0) {
        out.push("(showobjet=LDM_BALANCE_PR)");
        pv = "(hideobjet=LDM_BALANCEPE)";
    } else {
        out.push("(showobjet=LDM_BALANCEPE)");
        pv = "(hideobjet=LDM_BALANCE_PR)";
    }
    out.push(pv);
    if f(560) == 0 {
        out.push("(showperso=LDM_PECHEUR01)");
        pv = "(showperso=LDM_PECHEUR02)";
    } else {
        out.push("(hideperso=LDM_PECHEUR01)");
        pv = "(hideperso=LDM_PECHEUR02)";
    }
    out.push(pv);
    if (f(87) == 1) && (f(560) == 0) {
        pv = "(showobjet=LDM_POISSON2)";
    } else {
        pv = "(hideobjet=LDM_POISSON2)";
    }
    out.push(pv);
    if f(66) == 1 {
        pv = "(hideobjet=LDM_ANKH)";
    } else {
        pv = "(showobjet=LDM_ANKH)";
    }
    out.push(pv);
    if f(54) == 1 {
        pv = "(showobjet=LDM_ANKHPR)";
    } else {
        pv = "(hideobjet=LDM_ANKHPR)";
    }
    out.push(pv);
    if f(85) == 1 {
        out.push("(hideobjet=LDM_PLUMES)");
        if f(535) == 1 {
            out.push("(showobjet=LDM_PLUME0,1)");
            out.push("(hideobjet=COF_PLUME0)");
        } else {
            out.push("(showobjet=COF_PLUME0)");
        }
        if f(536) == 1 {
            out.push("(showobjet=LDM_PLUME1,1)");
            out.push("(hideobjet=COF_PLUME1)");
        } else {
            out.push("(showobjet=COF_PLUME1)");
        }
        if f(537) == 1 {
            out.push("(showobjet=LDM_PLUME2,1)");
            out.push("(hideobjet=COF_PLUME2)");
        } else {
            out.push("(showobjet=COF_PLUME2)");
        }
        if f(538) == 1 {
            out.push("(showobjet=LDM_PLUME3,1)");
            out.push("(hideobjet=COF_PLUME3)");
        } else {
            out.push("(showobjet=COF_PLUME3)");
        }
        if f(539) == 1 {
            out.push("(showobjet=LDM_PLUME4,1)");
            out.push("(hideobjet=COF_PLUME4)");
        } else {
            out.push("(showobjet=COF_PLUME4)");
        }
        if f(540) == 1 {
            out.push("(showobjet=LDM_PLUME5,1)");
            out.push("(hideobjet=COF_PLUME5)");
        } else {
            out.push("(showobjet=COF_PLUME5)");
        }
        if f(541) == 1 {
            out.push("(showobjet=LDM_PLUME6,1)");
            out.push("(hideobjet=COF_PLUME6)");
        } else {
            out.push("(showobjet=COF_PLUME6)");
        }
        if f(542) == 1 {
            out.push("(showobjet=LDM_PLUME7,1)");
            out.push("(hideobjet=COF_PLUME7)");
        } else {
            out.push("(showobjet=COF_PLUME7)");
        }
        if f(543) == 1 {
            out.push("(showobjet=LDM_PLUME8,1)");
            out.push("(hideobjet=COF_PLUME8)");
        } else {
            out.push("(showobjet=COF_PLUME8)");
        }
        if f(544) != 1 {
            out.push("(showobjet=COF_PLUME9)");
            out.push("(showperso=CHF_PSYCHOP)");
            return;
        }
        pv = "(showobjet=LDM_PLUME9,1)";
    } else {
        if f(76) == 1 {
            pv = "(showobjet=LDM_PLUMES)";
        } else {
            pv = "(hideobjet=LDM_PLUMES)";
        }
        out.push(pv);
        out.push("(hideobjet=COF_PLUME0)");
        out.push("(hideobjet=COF_PLUME1)");
        out.push("(hideobjet=COF_PLUME2)");
        out.push("(hideobjet=COF_PLUME3)");
        out.push("(hideobjet=COF_PLUME4)");
        out.push("(hideobjet=COF_PLUME5)");
        out.push("(hideobjet=COF_PLUME6)");
        out.push("(hideobjet=COF_PLUME7)");
        pv = "(hideobjet=COF_PLUME8)";
    }
    out.push(pv);
    out.push("(hideobjet=COF_PLUME9)");
    out.push("(showperso=CHF_PSYCHOP)");
}

/// @ 0x004501c0.
fn mmarchd(f: &dyn Fn(usize) -> i32, out: &mut Vec<&'static str>) {
    let mut pv: &'static str;
    if f(119) == 0 {
        out.push("(ShowPerso=MMA_CHAT)");
        out.push("(ShowObjet=MMA_COUVERCLE)");
        pv = "(HideObjet=MMA_COUVERCLEOUVERT)";
    } else {
        out.push("(HidePerso=MMA_CHAT)");
        out.push("(HideObjet=MMA_COUVERCLE)");
        pv = "(ShowObjet=MMA_COUVERCLEOUVERT)";
    }
    out.push(pv);
    if f(123) == 0 {
        pv = "(ShowObjet=MMA_PIERRE)";
    } else {
        pv = "(HideObjet=MMA_PIERRE)";
    }
    out.push(pv);
    if f(121) == 0 {
        pv = "(ShowObjet=MMA_ECHELLE)";
    } else {
        pv = "(HideObjet=MMA_ECHELLE)";
    }
    out.push(pv);
    if f(124) == 0 {
        pv = "(HideObjet=MMA_ECHELLEBALCON)";
    } else {
        pv = "(ShowObjet=MMA_ECHELLEBALCON)";
    }
    out.push(pv);
    if f(889) == 0 {
        out.push("(ShowObjet=MMA_VASE)");
        out.push("(HideObjet=MMA_VASEBRISE)");
        return;
    }
    out.push("(HideObjet=MMA_VASE)");
    out.push("(ShowObjet=MMA_VASEBRISE)");
}

/// @ 0x004502b0.
fn paleo(f: &dyn Fn(usize) -> i32, out: &mut Vec<&'static str>) {
    let mut pv: &'static str;
    if f(604) == 1 {
        out.push("(HideObjet=PAL_HERO_MORTE)");
        out.push("(HideObjet=PAL_COEUR_CERF)");
        out.push("(HideObjet=PAL_COEUR_AVROCH)");
        out.push("(HideObjet=PAL_COEUR_CHEVAL)");
        pv = "(HideObjet=PAL_COEUR_BOUQUETIN)";
    } else {
        out.push("(ShowObjet=PAL_HERO_MORTE)");
        out.push("(ShowObjet=PAL_COEUR_CERF)");
        out.push("(ShowObjet=PAL_COEUR_AVROCH)");
        out.push("(ShowObjet=PAL_COEUR_CHEVAL)");
        pv = "(ShowObjet=PAL_COEUR_BOUQUETIN)";
    }
    out.push(pv);
    if f(101) == 1 {
        pv = "(HideObjet=PAL_SOUCHE)";
    } else {
        pv = "(ShowObjet=PAL_SOUCHE)";
    }
    out.push(pv);
    if f(107) == 1 {
        pv = "(ShowObjet=PAL_SOUCHE2)";
    } else {
        pv = "(HideObjet=PAL_SOUCHE2)";
    }
    out.push(pv);
    if f(109) == 1 {
        pv = "(HideObjet=PAL_CAILLOU)";
    } else {
        pv = "(ShowObjet=PAL_CAILLOU)";
    }
    out.push(pv);
    if f(111) == 1 {
        pv = "(HidePerso=PAL_TIGRE)";
    } else {
        pv = "(ShowPerso=PAL_TIGRE)";
    }
    out.push(pv);
    if f(112) == 1 {
        pv = "(HideObjet=PAL_LEVIER)";
    } else {
        pv = "(ShowObjet=PAL_LEVIER)";
    }
    out.push(pv);
    if f(613) == 1 {
        pv = "(HideObjet=PAL_PERCHE)";
    } else {
        pv = "(ShowObjet=PAL_PERCHE)";
    }
    out.push(pv);
    if (f(600) == 1) && (f(114) == 0) {
        pv = "(ShowPerso=PAL_LOUP_NOIR)";
    } else {
        pv = "(HidePerso=PAL_LOUP_NOIR)";
    }
    out.push(pv);
    if f(117) == 1 {
        pv = "(HideObjet=PAL_CRANE)";
    } else {
        pv = "(ShowObjet=PAL_CRANE)";
    }
    out.push(pv);
    if f(614) == 1 {
        pv = "(HideObjet=PAL_PROPULSEUR)";
    } else {
        pv = "(ShowObjet=PAL_PROPULSEUR)";
    }
    out.push(pv);
    if f(608) == 1 {
        pv = "(HideObjet=PAL_CERF)";
    } else {
        pv = "(ShowObjet=PAL_CERF)";
    }
    out.push(pv);
    if f(609) == 1 {
        pv = "(HideObjet=PAL_AVROCH)";
    } else {
        pv = "(ShowObjet=PAL_AVROCH)";
    }
    out.push(pv);
    if f(610) == 1 {
        pv = "(HideObjet=PAL_CHEVAL)";
    } else {
        pv = "(ShowObjet=PAL_CHEVAL)";
    }
    out.push(pv);
    if f(611) == 1 {
        pv = "(HideObjet=PAL_BOUQUETIN)";
    } else {
        pv = "(ShowObjet=PAL_BOUQUETIN)";
    }
    out.push(pv);
    if f(615) == 1 {
        out.push("(HideObjet=PAL_LANCE)");
        out.push("(SacADos=PAL_LANCE)");
        return;
    }
    out.push("(ShowObjet=PAL_LANCE)");
}

/// @ 0x00450510.
fn pmagi(f: &dyn Fn(usize) -> i32, out: &mut Vec<&'static str>) {
    let mut pv: &'static str;
    if f(812) == 0 {
        out.push("(HideObjet=PMA_CRANE)");
        out.push("(HideObjet=PMA_SUPPORTCRANE)");
        out.push("(HideObjet=PMA_CHIFFON)");
        out.push("(HideObjet=PMA_LAMPE)");
        out.push("(HideObjet=PMA_TIRETTE)");
    } else {
        if f(158) == 0 {
            out.push("(HideObjet=PMA_CRANE)");
            out.push("(HideObjet=PMA_SUPPORTCRANE)");
            out.push("(ShowObjet=PMA_LAMPE)");
        } else {
            if f(156) == 0 {
                out.push("(ShowObjet=PMA_CRANE)");
                pv = "(HideObjet=PMA_SUPPORTCRANE)";
            } else {
                out.push("(HideObjet=PMA_CRANE)");
                pv = "(ShowObjet=PMA_SUPPORTCRANE)";
            }
            out.push(pv);
            out.push("(HideObjet=PMA_LAMPE)");
        }
        if f(58) == 0 {
            pv = "(ShowObjet=PMA_CHIFFON)";
        } else {
            pv = "(HideObjet=PMA_CHIFFON)";
        }
        out.push(pv);
        out.push("(ShowObjet=PMA_TIRETTE)");
    }
    if f(154) == 0 {
        pv = "(ShowObjet=PMA_COBRAS)";
    } else {
        pv = "(HideObjet=PMA_COBRAS)";
    }
    out.push(pv);
    if (f(846) == 1) && (f(902) == 0) {
        pv = "(ShowObjet=PMA_INDICE1)";
    } else {
        pv = "(HideObjet=PMA_INDICE1)";
    }
    out.push(pv);
    if (f(847) == 1) && (f(903) == 0) {
        pv = "(ShowObjet=PMA_INDICE2)";
    } else {
        pv = "(HideObjet=PMA_INDICE2)";
    }
    out.push(pv);
    if (f(848) == 1) && (f(904) == 0) {
        pv = "(ShowObjet=PMA_INDICE3)";
    } else {
        pv = "(HideObjet=PMA_INDICE3)";
    }
    out.push(pv);
    if (f(849) == 1) && (f(905) == 0) {
        pv = "(ShowObjet=PMA_INDICE4)";
    } else {
        pv = "(HideObjet=PMA_INDICE4)";
    }
    out.push(pv);
    out.push("(ShowObjet=PMA_LIENSLIGHT)");
    out.push("(ShowObjet=PMA_ETOILESLIGHT)");
}

/// @ 0x0044f9d0.
fn jardin(f: &dyn Fn(usize) -> i32, out: &mut Vec<&'static str>) {
    let mut pv: &'static str;
    let pick = |c: bool, a: &'static str, b: &'static str| if c { a } else { b };
    if f(803) == 0 {
        pv = "(HideObjet=BAG_BANANE)";
    } else {
        pv = "(ShowObjet=BAG_BANANE)";
    }
    out.push(pv);
    if f(127) == 0 {
        pv = "(ShowObjet=BAG_PERCHE)";
    } else {
        pv = "(HideObjet=BAG_PERCHE)";
    }
    out.push(pv);
    if f(804) == 0 {
        pv = "(ShowPerso=BAG_SINGE)";
    } else {
        pv = "(HidePerso=BAG_SINGE)";
    }
    out.push(pv);
    if f(125) == 0 {
        pv = "(HideObjet=BAG_3BATONS)";
    } else {
        pv = "(ShowObjet=BAG_3BATONS)";
    }
    out.push(pv);
    if f(145) == 0 {
        pv = "(ShowObjet=BAG_BOULE_LAITEUSE)";
    } else {
        pv = "(HideObjet=BAG_BOULE_LAITEUSE)";
    }
    out.push(pv);
    if f(148) == 0 {
        pv = "(HideObjet=BAG_BOULE_LAITEUSEPOSEE)";
    } else {
        pv = "(ShowObjet=BAG_BOULE_LAITEUSEPOSEE)";
    }
    out.push(pv);
    if f(144) == 0 {
        out.push("(ShowObjet=BAG_CAGE)");
        out.push("(HideObjet=BAG_CAGEBAISSEE)");
        pv = "(HideObjet=BAG_CORNESERRURE)";
    } else {
        out.push("(HideObjet=BAG_CAGE)");
        out.push("(ShowObjet=BAG_CAGEBAISSEE)");
        pv = "(ShowObjet=BAG_CORNESERRURE)";
    }
    out.push(pv);
    if f(128) == 0 {
        out.push("(ShowPerso=BAG_HOURISVERTE_DEHORS)");
        out.push("(HidePerso=BAG_HOURISVERTE_DEDANS)");
        if f(129) == 0 {
            out.push("(HidePerso=BAG_SOLDAT_BOIS)");
        } else {
            out.push("(ShowPerso=BAG_SOLDAT_BOIS)");
            out.push("(PlayAnimp=BAG_SOLDAT_BOIS,2)");
        }
    } else {
        out.push("(HidePerso=BAG_SOLDAT_BOIS)");
        if f(805) == 0 {
            out.push("(ShowPerso=BAG_HOURISVERTE_DEHORS)");
            out.push("(HidePerso=BAG_HOURISVERTE_DEDANS)");
        } else {
            out.push("(HidePerso=BAG_HOURISVERTE_DEHORS)");
            out.push(pick(
                f(808) == 0,
                "(ShowPerso=BAG_HOURISVERTE_DEDANS)",
                "(HidePerso=BAG_HOURISVERTE_DEDANS)",
            ));
        }
    }
    if f(134) == 0 {
        out.push("(ShowPerso=BAG_HOURISROUGE_DEHORS)");
        out.push("(HidePerso=BAG_HOURISROUGE_DEDANS)");
        if f(135) != 0 {
            out.push("(ShowPerso=BAG_SOLDAT_TERRE)");
            out.push("(HideObjet=BAG_POT_TERRE)");
            out.push("(PlayAnimp=BAG_SOLDAT_TERRE,2)");
        } else {
            out.push("(ShowObjet=BAG_POT_TERRE)");
            out.push("(HidePerso=BAG_SOLDAT_TERRE)");
        }
    } else {
        out.push("(ShowObjet=BAG_POT_TERRE)");
        out.push("(HidePerso=BAG_SOLDAT_TERRE)");
        if f(806) != 0 {
            out.push("(HidePerso=BAG_HOURISROUGE_DEHORS)");
            out.push(pick(
                f(809) == 0,
                "(ShowPerso=BAG_HOURISROUGE_DEDANS)",
                "(HidePerso=BAG_HOURISROUGE_DEDANS)",
            ));
        } else {
            out.push("(ShowPerso=BAG_HOURISROUGE_DEHORS)");
            out.push("(HidePerso=BAG_HOURISROUGE_DEDANS)");
        }
    }
    if f(139) == 0 {
        out.push("(ShowPerso=BAG_HOURISBLEUE_DEHORS)");
        out.push("(HidePerso=BAG_HOURISBLEUE_DEDANS)");
        if f(140) != 0 {
            out.push("(ShowPerso=BAG_SOLDAT_PIERRE)");
            out.push("(ShowObjet=BAG_PIERRES_GARDE)");
            out.push("(PlayAnimp=BAG_SOLDAT_PIERRE,2)");
        } else {
            out.push("(HidePerso=BAG_SOLDAT_PIERRE)");
            out.push("(HideObjet=BAG_PIERRES_GARDE)");
        }
        return;
    }
    out.push("(HidePerso=BAG_SOLDAT_PIERRE)");
    out.push("(HideObjet=BAG_PIERRES_GARDE)");
    if f(807) == 0 {
        out.push("(ShowPerso=BAG_HOURISBLEUE_DEHORS)");
        out.push("(HidePerso=BAG_HOURISBLEUE_DEDANS)");
        return;
    }
    out.push("(HidePerso=BAG_HOURISBLEUE_DEHORS)");
    out.push(pick(
        f(810) != 0,
        "(HidePerso=BAG_HOURISBLEUE_DEDANS)",
        "(ShowPerso=BAG_HOURISBLEUE_DEDANS)",
    ));
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
