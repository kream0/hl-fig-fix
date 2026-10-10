//! hl-fig-fix: repairs Hogwarts Legacy saves stuck in "Tomes and Tribulations" (FIG_01), where
//! Professor Fig can't be talked to after learning Incendio (WB bug HL-20832).
//!
//! FIG_01 step 12.1 sends a one-time "Start" event to the story graph CCL_FIG_01_MasterPT1_SG1.
//! If that event is lost, the graph never starts and has no rows in StoryGraphPersistenceDynamic.
//! FIG_01_Sub01 then waits forever for "ConvoStart  CCL_FIG_01_MasterPT1_SG1", and FIG_01 sits at
//! "WaitForSubMission01". The fix rewinds FIG_01 to the Start step so the game sends it again on load.
//!
//!   hl-fig-fix                          find the game's saves, report, fix stuck ones (asks first)
//!   hl-fig-fix <file-or-folder>...      same, for specific saves (drag-and-drop works)
//!   hl-fig-fix --check [paths]          report only, change nothing
//!   hl-fig-fix --yes [paths]            don't ask, don't pause
//!   hl-fig-fix unpack <in.sav> <out.db>
//!   hl-fig-fix pack <template.sav> <in.db> <out.sav>

mod save;

use rusqlite::{Connection, OptionalExtension};
use save::Res;
use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{env, fs, process};

const GRAPH: &str = "CCL_FIG_01_MasterPT1_SG1";
const STUCK_STEP: &str = "WaitForSubMission01";
const SUB_STEP: &str = "ConvoStart  CCL_FIG_01_MasterPT1_SG1"; // two spaces, as in the game data
const RESUME_STEP: &str = "Start  CCL_FIG_01_MasterPT1_SG1";
const FIG_OFFICE: [f64; 3] = [346980.0, -458580.0, -82608.0]; // world units (cm)
const IN_OFFICE_M: f64 = 25.0;
const STEAM_APP_ID: &str = "990080";

#[derive(PartialEq)]
enum State { NotStarted, Stuck, Patched, Waiting, Other(String) }

struct Info { name: String, state: State, dist_m: Option<f64>, game_time: Option<String> }

struct Save {
    path: PathBuf,
    slot: u32,
    age_secs: u64,
    info: Res<Info>,
    /// Set on a stuck save when the same character has a newer save that isn't stuck: it gets left alone.
    newer_ok: Option<String>,
}

impl Save {
    fn group(&self) -> (Option<&Path>, Option<String>) { (self.path.parent(), char_of(&self.path)) }
    fn state(&self) -> Option<&State> { self.info.as_ref().ok().map(|i| &i.state) }
    fn to_fix(&self) -> bool { self.state() == Some(&State::Stuck) && self.newer_ok.is_none() }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("unpack") if args.len() == 3 => exit_with(unpack(&args[1], &args[2]), false),
        Some("pack") if args.len() == 4 => exit_with(pack(&args[1], &args[2], &args[3]), false),
        Some("-h" | "--help" | "/?") => println!("{}", USAGE),
        _ => {
            let check = args.iter().any(|a| a == "--check" || a == "-c");
            let yes = args.iter().any(|a| a == "--yes" || a == "-y");
            let paths: Vec<PathBuf> = args.iter().filter(|a| !a.starts_with('-')).map(PathBuf::from).collect();
            let pause = cfg!(windows) && !yes && std::io::stdin().is_terminal();
            exit_with(run(&paths, check, yes), pause);
        }
    }
}

const USAGE: &str = "\
hl-fig-fix: fixes Hogwarts Legacy saves where Professor Fig can't be talked to after Incendio
(\"Tomes and Tribulations\" stuck, WB bug HL-20832).

  hl-fig-fix                          find your saves, show their state, fix the stuck ones
  hl-fig-fix <file-or-folder>...      only look at these saves (drag-and-drop onto the .exe works)
  hl-fig-fix --check [paths]          report only, change nothing
  hl-fig-fix --yes [paths]            fix without asking
  hl-fig-fix unpack <in.sav> <out.db>              extract a save's SQLite database
  hl-fig-fix pack <template.sav> <in.db> <out.sav> put a database back into a save

Quit the game before running it. Originals are backed up before anything is changed.";

fn exit_with(r: Res<()>, pause: bool) {
    let code = match r {
        Ok(()) => 0,
        Err(e) => { eprintln!("\nError: {e}"); 1 }
    };
    if pause {
        print!("\nPress Enter to close...");
        let _ = std::io::stdout().flush();
        let _ = std::io::stdin().lock().read_line(&mut String::new());
    }
    process::exit(code);
}

fn run(paths: &[PathBuf], check: bool, yes: bool) -> Res<()> {
    println!("Hogwarts Legacy - Fig / Incendio quest fix (HL-20832)\n");

    let files = if paths.is_empty() {
        let roots: Vec<PathBuf> = save_roots().into_iter().filter(|p| p.is_dir()).collect();
        if roots.is_empty() {
            return Err("couldn't find the Hogwarts Legacy save folder. Pass it (or a .sav file) as an argument, e.g.\n  \
                        hl-fig-fix \"%LOCALAPPDATA%\\Hogwarts Legacy\\Saved\\SaveGames\"".into());
        }
        for r in &roots { println!("Save folder: {}", r.display()); }
        roots.iter().flat_map(|r| find_saves(r)).collect::<Vec<_>>()
    } else {
        paths.iter().flat_map(|p| find_saves(p)).collect()
    };
    if files.is_empty() { return Err("no HL-xx-yy.sav files found".into()); }

    let now = SystemTime::now();
    let mut saves: Vec<Save> = files.into_iter().map(|path| {
        let age_secs = fs::metadata(&path).and_then(|m| m.modified()).ok()
            .and_then(|t| now.duration_since(t).ok()).map_or(0, |d| d.as_secs());
        let slot = slot_of(&path).map_or(99, |(_, s)| s);
        let info = fs::read(&path).map_err(|e| e.to_string()).and_then(|b| inspect(&b));
        Save { path, slot, age_secs, info, newer_ok: None }
    }).collect();
    saves.sort_by(|a, b| (a.group(), a.age_secs).cmp(&(b.group(), b.age_secs)));
    // Only fix saves newer than the character's latest save that is already past (or not at) the bug.
    for i in 0..saves.len() {
        if saves[i].state() != Some(&State::Stuck) { continue; }
        saves[i].newer_ok = saves.iter()
            .filter(|o| o.group() == saves[i].group() && o.age_secs < saves[i].age_secs)
            .find(|o| matches!(o.state(), Some(State::Patched | State::Waiting | State::Other(_))))
            .map(|o| o.path.file_stem().unwrap().to_string_lossy().into_owned());
    }

    print_report(&saves);

    let stuck: Vec<&Save> = saves.iter().filter(|s| s.to_fix()).collect();
    println!();
    if stuck.is_empty() {
        println!("Nothing to fix.");
        if saves.iter().any(|s| s.newer_ok.is_some()) {
            println!("Some older saves are stuck, but each of those characters has a newer save that isn't.");
        }
        let newest_patched = saves.iter().any(|s| s.state() == Some(&State::Patched)
            && !saves.iter().any(|o| o.group() == s.group() && o.age_secs < s.age_secs));
        if newest_patched {
            println!("Saves marked PATCHED are already fixed: load one, go to Fig's office and talk to him.");
        }
        return Ok(());
    }
    println!("{} stuck save(s) to fix.", stuck.len());
    let in_office = |s: &Save| matches!(&s.info, Ok(i) if i.dist_m.is_some_and(|d| d <= IN_OFFICE_M));
    if !stuck.iter().any(|s| in_office(s)) {
        println!("\nTip: none of these saves is in Fig's office, and the fix was tested by loading a save made there.");
        println!("     For the best chance: load one, go to his office, make a manual save there, quit the game");
        println!("     and run this tool again. Fixing them now also works if you then go to his office.");
    }
    if check { println!("(--check: nothing was changed)"); return Ok(()); }

    wait_for_game_exit(yes)?;
    if !yes && !confirm(&format!("Fix {} save(s)? The originals are backed up first. [Y/n] ", stuck.len()))? {
        println!("Nothing changed.");
        return Ok(());
    }

    let stamp = timestamp();
    let mut fixed = Vec::new();
    for s in &stuck {
        let name = s.path.file_name().unwrap().to_string_lossy();
        match fix_one(&s.path, &stamp) {
            Ok(backup) => { println!("  fixed {name}   (backup: {})", backup.display()); fixed.push(*s); }
            Err(e) => println!("  FAILED {name}: {e}   (file left unchanged)"),
        }
    }
    if fixed.is_empty() { return Err("no save could be fixed".into()); }

    let best = fixed.iter().min_by_key(|s| (!in_office(s), s.age_secs)).unwrap();
    let best_name = best.path.file_stem().unwrap().to_string_lossy();
    let best_near = in_office(best);
    println!("\nDone. Now:");
    println!("  1. Start the game and load {best_name} ({}, saved {} ago{}).",
             kind(best.slot), age(best.age_secs), if best_near { ", in Fig's office" } else { "" });
    println!("     If Steam reports a cloud save conflict, keep the LOCAL files.");
    println!("  2. Wait for the loading screen to finish. Fig's quest restarts its \"go talk to Fig\" step.");
    if !best_near {
        println!("     This save isn't in Fig's office: go there (Floo Flame \"Professor Fig's Classroom\", then upstairs).");
        println!("     If Fig still can't be talked to, make a manual save in his office, quit, and run this tool again.");
    }
    println!("  3. Talk to Fig. He's only there between 10:00 and 14:00 in-game: if he's missing, use the");
    println!("     \"wait\" circle next to his desk or wait from the map until morning.");
    println!("  Once it works, make a new manual save. To undo, copy the backups back into the save folder.");
    Ok(())
}

fn print_report(saves: &[Save]) {
    let mut last_char = None;
    for s in saves {
        if last_char != Some(s.group()) {
            let name = saves.iter().filter(|o| o.group() == s.group())
                .find_map(|o| o.info.as_ref().ok().map(|i| i.name.clone())).unwrap_or_default();
            println!("\nCharacter HL-{}  {name}", s.group().1.as_deref().unwrap_or("??"));
            last_char = Some(s.group());
        }
        let file = s.path.file_stem().unwrap().to_string_lossy();
        let head = format!("  {file:<9} {:<8} {:>9} ago  ", kind(s.slot), age(s.age_secs));
        match &s.info {
            Err(e) => println!("{head}unreadable: {e}"),
            Ok(i) => {
                let state = match &i.state {
                    State::NotStarted => "Fig quest not started yet".to_string(),
                    State::Stuck => match &s.newer_ok {
                        None => "STUCK (Fig can't be talked to)".to_string(),
                        Some(n) => format!("stuck, but {n} is newer and fine: skipped"),
                    },
                    State::Patched => "PATCHED (load it and talk to Fig)".to_string(),
                    State::Waiting => "ok (just go talk to Fig)".to_string(),
                    State::Other(o) => format!("ok ({o})"),
                };
                let mut extra = String::new();
                if matches!(i.state, State::Stuck | State::Patched | State::Waiting) {
                    if let Some(d) = i.dist_m {
                        extra += &if d <= IN_OFFICE_M { "  in Fig's office".to_string() }
                                  else if d < 1000.0 { format!("  {d:.0} m from Fig's office") }
                                  else { format!("  {:.1} km from Fig's office", d / 1000.0) };
                    }
                    if let Some(t) = &i.game_time { extra += &format!(", in-game {t}"); }
                }
                println!("{head}{state}{extra}");
            }
        }
    }
}

fn kind(slot: u32) -> &'static str { if slot < 10 { "manual" } else { "autosave" } }

fn age(secs: u64) -> String {
    match secs {
        s if s < 3600 => format!("{} min", s / 60),
        s if s < 172800 => format!("{} h", s / 3600),
        s => format!("{} days", s / 86400),
    }
}

/// "HL-01-12.sav" -> ("01", 12)
fn slot_of(p: &Path) -> Option<(String, u32)> {
    let name = p.file_name()?.to_str()?;
    let rest = name.strip_prefix("HL-")?.strip_suffix(".sav")?;
    let (c, s) = rest.split_once('-')?;
    Some((c.to_string(), s.parse().ok()?))
}

fn char_of(p: &Path) -> Option<String> { slot_of(p).map(|(c, _)| c) }

fn find_saves(p: &Path) -> Vec<PathBuf> {
    if p.is_file() { return vec![p.to_path_buf()]; }
    let mut out = Vec::new();
    let mut dirs = vec![p.to_path_buf()];
    if let Ok(rd) = fs::read_dir(p) {
        dirs.extend(rd.flatten().map(|e| e.path()).filter(|q| q.is_dir() && q.file_name().is_some_and(|n| n != "FigFixBackups")));
    }
    for d in dirs {
        if let Ok(rd) = fs::read_dir(&d) {
            out.extend(rd.flatten().map(|e| e.path()).filter(|q| q.is_file() && slot_of(q).is_some()));
        }
    }
    out.sort();
    out
}

/// Where the game keeps its saves: Windows directly, Linux / Steam Deck inside the Proton prefix.
fn save_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(l) = env::var_os("LOCALAPPDATA") {
        roots.push(PathBuf::from(l).join("Hogwarts Legacy").join("Saved").join("SaveGames"));
    }
    if let Some(home) = env::var_os("HOME").map(PathBuf::from) {
        let mut libs = Vec::new();
        for steam in [".local/share/Steam", ".steam/steam", ".steam/root", ".var/app/com.valvesoftware.Steam/.local/share/Steam"] {
            let steam = home.join(steam);
            libs.push(steam.clone());
            // Games installed on other drives (e.g. a Steam Deck SD card) keep their Proton prefix there.
            if let Ok(vdf) = fs::read_to_string(steam.join("steamapps/libraryfolders.vdf")) {
                for line in vdf.lines() {
                    let parts: Vec<&str> = line.split('"').collect();
                    if parts.len() >= 4 && parts[1] == "path" { libs.push(PathBuf::from(parts[3].replace("\\\\", "\\"))); }
                }
            }
        }
        for lib in libs {
            roots.push(lib.join("steamapps/compatdata").join(STEAM_APP_ID)
                .join("pfx/drive_c/users/steamuser/AppData/Local/Hogwarts Legacy/Saved/SaveGames"));
        }
    }
    let mut seen = Vec::new();
    roots.into_iter().filter(|r| {
        let key = fs::canonicalize(r).unwrap_or_else(|_| r.clone());
        let new = !seen.contains(&key);
        seen.push(key);
        new
    }).collect()
}

/// The save DB is handled in a temp file, so the original save is never opened for writing.
struct TempDb(PathBuf);

impl TempDb {
    fn new(sqlite: &[u8]) -> Res<TempDb> {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let p = env::temp_dir().join(format!("hl-fig-fix-{}-{}.db", process::id(), N.fetch_add(1, Ordering::Relaxed)));
        fs::write(&p, sqlite).map_err(|e| format!("can't write temp file {}: {e}", p.display()))?;
        Ok(TempDb(p))
    }
    fn open(&self) -> Res<Connection> { Connection::open(&self.0).map_err(|e| e.to_string()) }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
        let _ = fs::remove_file(self.0.with_extension("db-journal"));
    }
}

fn inspect(sav: &[u8]) -> Res<Info> {
    let tmp = TempDb::new(&save::read_db(sav)?)?;
    inspect_db(&tmp.open()?).map_err(|e| e.to_string())
}

fn inspect_db(c: &Connection) -> rusqlite::Result<Info> {
    let misc = |owner: &str, name: &str| -> rusqlite::Result<Option<String>> {
        c.query_row("SELECT CAST(DataValue AS TEXT) FROM MiscDataDynamic WHERE DataOwner=?1 AND DataName=?2",
                    [owner, name], |r| r.get(0)).optional()
    };
    let step = |mission: &str| -> rusqlite::Result<Option<(String, String)>> {
        c.query_row("SELECT Text1, Text2 FROM MissionDynamic WHERE MissionID=?1 AND Type='Main'", [mission],
                    |r| Ok((r.get::<_, Option<String>>(0)?.unwrap_or_default(), r.get::<_, Option<String>>(1)?.unwrap_or_default())))
            .optional()
    };
    // Text values are compared exactly; the game's NOCASE collation doesn't matter for these strings.
    let fig = step("FIG_01")?;
    let sub = step("FIG_01_Sub01")?;
    let graph_rows: i64 = c.query_row("SELECT COUNT(*) FROM StoryGraphPersistenceDynamic WHERE Key=?1", [GRAPH], |r| r.get(0))?;

    let state = match &fig {
        None => State::NotStarted,
        Some((t1, t2)) if t1 == STUCK_STEP && t2 == "PreActive"
            && sub.as_ref().is_some_and(|(s1, _)| s1 == SUB_STEP) && graph_rows == 0 => State::Stuck,
        Some((t1, _)) if t1 == RESUME_STEP => State::Patched,
        Some((t1, _)) if t1 == STUCK_STEP => State::Waiting,
        Some((_, t2)) if t2 == "Invalid" => State::NotStarted,
        Some((t1, t2)) if t1 == "None" => State::Other(format!("Fig quest {}", t2.to_lowercase())),
        Some((t1, t2)) => State::Other(format!("Fig quest {}, step {t1}", t2.to_lowercase())),
    };

    let name = [misc("Player", "PlayerFirstName")?, misc("Player", "PlayerLastName")?]
        .into_iter().flatten().collect::<Vec<_>>().join(" ");

    let coord = |n: &str| misc("Player", n).map(|v| v.and_then(|s| s.trim().parse::<f64>().ok()));
    let dist_m = match (misc("Player", "World")?, coord("LocX")?, coord("LocY")?, coord("LocZ")?) {
        (Some(w), Some(x), Some(y), Some(z)) if w.eq_ignore_ascii_case("Overland") => {
            let d = [x - FIG_OFFICE[0], y - FIG_OFFICE[1], z - FIG_OFFICE[2]];
            Some((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() / 100.0)
        }
        _ => None,
    };

    // SimTime is an FDateTime: 100 ns ticks since 0001-01-01.
    let game_time = misc("Scheduler", "SimTime")?.and_then(|s| s.trim().parse::<i64>().ok()).map(|ticks| {
        let secs = ticks / 10_000_000;
        format!("{:02}:{:02}", secs % 86400 / 3600, secs % 3600 / 60)
    });

    Ok(Info { name, state, dist_m, game_time })
}

/// Backs up `path`, rewinds FIG_01 to its Start step, writes the save back and re-checks it.
fn fix_one(path: &Path, stamp: &str) -> Res<PathBuf> {
    let orig = fs::read(path).map_err(|e| format!("can't read: {e}"))?;
    let tmp = TempDb::new(&save::read_db(&orig)?)?;
    {
        let c = tmp.open()?;
        if inspect_db(&c).map_err(|e| e.to_string())?.state != State::Stuck { return Err("save changed since it was checked".into()); }
        let n = c.execute("UPDATE MissionDynamic SET Text1=?1 WHERE MissionID='FIG_01' AND Type='Main' AND Text1=?2",
                          [RESUME_STEP, STUCK_STEP]).map_err(|e| e.to_string())?;
        if n != 1 { return Err(format!("expected to update 1 row, updated {n}")); }
        c.close().map_err(|(_, e)| e.to_string())?;
    }
    let patched_db = fs::read(&tmp.0).map_err(|e| e.to_string())?;
    let patched = save::write_db(&orig, &patched_db)?;
    let check = TempDb::new(&save::read_db(&patched)?)?;
    {
        let c = check.open()?;
        let ok: String = c.query_row("PRAGMA integrity_check", [], |r| r.get(0)).map_err(|e| e.to_string())?;
        if ok != "ok" { return Err(format!("integrity check failed: {ok}")); }
        if inspect_db(&c).map_err(|e| e.to_string())?.state != State::Patched { return Err("patched save didn't verify".into()); }
    }

    let backup_dir = backup_dir_for(path, stamp);
    fs::create_dir_all(&backup_dir).map_err(|e| format!("can't create backup folder {}: {e}", backup_dir.display()))?;
    let backup = backup_dir.join(path.file_name().unwrap());
    fs::write(&backup, &orig).map_err(|e| format!("backup failed: {e}"))?;
    if fs::read(&backup).ok().as_deref() != Some(&orig[..]) { return Err("backup didn't verify".into()); }

    let tmp_out = path.with_extension("sav.figfix-tmp");
    fs::write(&tmp_out, &patched).map_err(|e| format!("write failed: {e}"))?;
    fs::rename(&tmp_out, path).map_err(|e| { let _ = fs::remove_file(&tmp_out); format!("replace failed: {e}") })?;
    Ok(backup)
}

/// <...>/Hogwarts Legacy/Saved/SaveGames/<id>/HL-01-12.sav -> <...>/Hogwarts Legacy/FigFixBackups/<stamp>/<id>/
/// (outside the save folder, so the game and Steam Cloud ignore it). Anything else: next to the file.
fn backup_dir_for(path: &Path, stamp: &str) -> PathBuf {
    let id_dir = path.parent().unwrap_or(Path::new("."));
    let save_games = id_dir.parent();
    let saved = save_games.and_then(Path::parent);
    if save_games.and_then(Path::file_name).is_some_and(|n| n == "SaveGames") && saved.and_then(Path::file_name).is_some_and(|n| n == "Saved") {
        if let (Some(root), Some(id)) = (saved.and_then(Path::parent), id_dir.file_name()) {
            return root.join("FigFixBackups").join(stamp).join(id);
        }
    }
    id_dir.join("FigFixBackups").join(stamp)
}

fn game_running() -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        process::Command::new("tasklist").args(["/FI", "IMAGENAME eq HogwartsLegacy.exe", "/NH"])
            .creation_flags(CREATE_NO_WINDOW).output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_ascii_lowercase().contains("hogwartslegacy.exe"))
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        let me = process::id().to_string();
        fs::read_dir("/proc").map(|rd| rd.flatten().any(|e| {
            e.file_name() != me.as_str()
                && fs::read(e.path().join("cmdline")).is_ok_and(|c| c.windows(18).any(|w| w == b"HogwartsLegacy.exe"))
        })).unwrap_or(false)
    }
}

fn wait_for_game_exit(yes: bool) -> Res<()> {
    while game_running() {
        if yes || !std::io::stdin().is_terminal() {
            return Err("Hogwarts Legacy is running. Quit the game completely, then run this again.".into());
        }
        print!("\nHogwarts Legacy is running. Quit the game completely (it would overwrite the fix), then press Enter...");
        let _ = std::io::stdout().flush();
        std::io::stdin().lock().read_line(&mut String::new()).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn confirm(q: &str) -> Res<bool> {
    if !std::io::stdin().is_terminal() { return Err("not running in a terminal; use --yes to fix without asking".into()); }
    print!("\n{q}");
    let _ = std::io::stdout().flush();
    let mut s = String::new();
    std::io::stdin().lock().read_line(&mut s).map_err(|e| e.to_string())?;
    Ok(!s.trim().to_ascii_lowercase().starts_with('n'))
}

/// Current UTC time as YYYY-MM-DD_HHMMSS, without a date library.
fn timestamp() -> String {
    let t = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    let (days, sod) = (t.div_euclid(86400), t.rem_euclid(86400));
    // Howard Hinnant's civil_from_days
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}_{:02}{:02}{:02}", sod / 3600, sod % 3600 / 60, sod % 60)
}

fn unpack(sav: &str, out: &str) -> Res<()> {
    let db = save::read_db(&fs::read(sav).map_err(|e| e.to_string())?)?;
    fs::write(out, &db).map_err(|e| e.to_string())?;
    eprintln!("wrote {out} ({} bytes of SQLite)", db.len());
    Ok(())
}

fn pack(template: &str, db: &str, out: &str) -> Res<()> {
    let sav = fs::read(template).map_err(|e| e.to_string())?;
    let db = fs::read(db).map_err(|e| e.to_string())?;
    let new = save::write_db(&sav, &db)?;
    fs::write(out, &new).map_err(|e| e.to_string())?;
    eprintln!("wrote {out} ({} bytes), verified", new.len());
    Ok(())
}
