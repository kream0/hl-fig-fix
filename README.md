# Hogwarts Legacy: "can't talk to Professor Fig after learning Incendio" save fix

A one-run tool that repairs saves hit by the bug where **Professor Fig can't be talked to after you learn
Incendio**. The quest marker stays over his head and the "wait here" circle does nothing. The quest is
"Tomes and Tribulations" (internal ID `FIG_01`, WB bug HL-20832).

It finds your saves, shows which ones are stuck, backs them up, and patches one value in each stuck
save so the quest restarts its "go talk to Fig" step the next time you load.

This fix was made and confirmed working on a real stuck Steam save on 2026-10-10. Read
[What was tested](#what-was-tested) before relying on it.

---

## Quick start

### Windows (Steam or Epic)

1. **Quit Hogwarts Legacy completely.**
2. Download **`hl-fig-fix.exe`** from the [latest release](https://github.com/kream0/hl-fig-fix/releases/latest)
   and double-click it.
   - If Windows SmartScreen appears ("Windows protected your PC"), click **More info → Run anyway**.
     The exe isn't code-signed. If you'd rather not run it, the full source is in this repository, see
     [Building from source](#building-from-source).
3. The tool lists your saves. Press **Enter** to fix the ones marked `STUCK`.
4. Start the game, load the save it recommends, and talk to Fig.

### Steam Deck / Linux (Proton)

1. Quit the game. On a Deck, switch to Desktop Mode and open Konsole.
2. Download **`hl-fig-fix-linux`** from the [latest release](https://github.com/kream0/hl-fig-fix/releases/latest)
   and run it from the folder you saved it in:
   ```sh
   chmod +x hl-fig-fix-linux
   ./hl-fig-fix-linux
   ```
   It searches every Steam library, SD cards included, for the Proton prefix of app 990080.

### After fixing

- Load the recommended save. The tool picks one made in Fig's office when there is one. After the loading
  screen, the quest re-sends the event that got lost.
- **Best results: a save made in Fig's office.** That's the case confirmed in-game. If none of your stuck
  saves are there (the tool will say so), load one, go to his office (Floo Flame **"Professor Fig's
  Classroom"**, then upstairs), make a manual save there, quit the game, and run the tool again.
- Fig is only in his office between **10:00 and 14:00** in-game. If he isn't there, use the "wait" circle
  next to his desk or wait from the map.
- If Steam reports a **cloud save conflict**, keep the **local** files.
- Once the conversation starts, make a new manual save.

### Options

```
hl-fig-fix                          find your saves, show their state, fix the stuck ones
hl-fig-fix <file-or-folder>...      only look at these saves (drag-and-drop a save or folder onto the .exe works)
hl-fig-fix --check [paths]          report only, change nothing
hl-fig-fix --yes [paths]            fix without asking
hl-fig-fix unpack <in.sav> <out.db>              extract a save's SQLite database
hl-fig-fix pack <template.sav> <in.db> <out.sav> put a (modified) database back into a save
```

Save locations it searches:

| Platform | Folder |
|---|---|
| Windows | `%LOCALAPPDATA%\Hogwarts Legacy\Saved\SaveGames\<id>\HL-xx-yy.sav` |
| Linux / Steam Deck | `<steam library>/steamapps/compatdata/990080/pfx/drive_c/users/steamuser/AppData/Local/Hogwarts Legacy/Saved/SaveGames/<id>/` |

Save names are `HL-<character>-<slot>.sav`. Slots `00`–`09` are manual saves; `10`–`14` are the rotating
autosaves.

### Example run

```
Hogwarts Legacy - Fig / Incendio quest fix (HL-20832)

Save folder: C:\Users\<you>\AppData\Local\Hogwarts Legacy\Saved\SaveGames

Character HL-01  <your character>
  HL-01-12  autosave    29 min ago  STUCK (Fig can't be talked to)  in Fig's office, in-game 11:45
  HL-01-11  autosave    32 min ago  STUCK (Fig can't be talked to)  in Fig's office, in-game 10:15
  HL-01-10  autosave    35 min ago  STUCK (Fig can't be talked to)  691 m from Fig's office, in-game 11:57
  HL-01-14  autosave    38 min ago  STUCK (Fig can't be talked to)  in Fig's office, in-game 07:15

4 stuck save(s) to fix.

Fix 4 save(s)? The originals are backed up first. [Y/n]
  fixed HL-01-12.sav   (backup: C:\Users\<you>\AppData\Local\Hogwarts Legacy\FigFixBackups\2026-10-10_165530\<id>\HL-01-12.sav)
  ...

Done. Now:
  1. Start the game and load HL-01-12 (autosave, saved 29 min ago, in Fig's office).
  ...
```

What the statuses mean:

| Status | Meaning |
|---|---|
| `STUCK` | Has this exact bug. It will be fixed. |
| `stuck, but HL-xx-yy is newer and fine: skipped` | An old save of a character that's already past the bug. Left alone. |
| `PATCHED` | Already fixed by this tool. Load it and talk to Fig. |
| `ok (just go talk to Fig)` | Same quest step, but the conversation is set up correctly. Not bugged. |
| `ok (Fig quest active/complete ...)`, `Fig quest not started yet` | Not at this point of the story. |

### Undo

Backups go to `...\Hogwarts Legacy\FigFixBackups\<UTC date_time>\<id>\`, next to the `Saved` folder and
outside it, so neither the game nor Steam Cloud touches them. To undo, copy a backed-up `.sav` back into
`Saved\SaveGames\<id>\`.

### No-tool workaround (if you have an older save)

People on Reddit got past the bug by reloading an autosave from **before turning in Professor Hecat's
assignment**, learning Incendio again, using the Floo Flame to reach Fig's office, and waiting in the
quest circle. It likely works for the same reason: it replays the step that got lost. You only need the tool
if you don't have such a save or don't want to redo that part.

---

## What the bug actually is

Hogwarts Legacy stores almost all game state in a **SQLite database embedded inside each `.sav`**. Quest
logic is data-driven: each quest is a list of steps defined in the game's `PhoenixShipData.sqlite`. Here
are the relevant steps of `FIG_01` and its sub-mission:

```
FIG_01
  10.1  LoadTransientSublevel   "Load Sublevel M_FIG_01"            M_FIG_01, within 50 m / 100 m
  11.1  LoadMissionSublevel     "Load M_FIG_01_CP1"
  12.1  TriggerStoryGraphEvent  "Start  CCL_FIG_01_MasterPT1_SG1"   actor=CCL_FIG_01_MasterPT1_SG1, event=Start,
                                                                   ResetStoryGraph=No, WaitForCurtain=CurtainRaised
  13.1  EvalProximity           "Get in range of Fig"
  ...
  16.1  EvalMissionComplete     "WaitForSubMission01"               waits for FIG_01_Sub01

FIG_01_Sub01  (the conversation with Fig)
   2.1  EvalStoryGraphEvent     "ConvoStart  CCL_FIG_01_MasterPT1_SG1"   waits for the story graph to start the convo
   4.1  EvalStoryGraphEvent     "Finish CCL_FIG_01_MasterPT1_SG1"
```

Step **12.1** sends a **one-time** `Start` event to the story graph `CCL_FIG_01_MasterPT1_SG1`, the
graph that runs Fig's conversation. In bugged saves:

- `MissionDynamic` shows `FIG_01` already at `WaitForSubMission01`, so step 12.1 has run.
- The sub-mission `FIG_01_Sub01` is waiting at `ConvoStart  CCL_FIG_01_MasterPT1_SG1`.
- **`StoryGraphPersistenceDynamic` has no rows for `CCL_FIG_01_MasterPT1_SG1`.** The graph never
  received its Start event. In a healthy save at the same step, the graph has rows.

The quest moved on, but the conversation graph it was supposed to start never started. Nothing re-sends
the event, so Fig shows the quest icon and the wait circle but has no conversation to offer. Reloading
doesn't help, because the save resumes past step 12.1.

The most likely cause, which I didn't prove, is a timing race with level streaming. Steps 10–11 stream in
the quest's sublevels (`M_FIG_01`, `M_FIG_01_CP1`) right before 12.1. If the graph's actor isn't loaded yet when the event fires, the
event is dropped.

**The fix:** set `FIG_01`'s current step back to `Start  CCL_FIG_01_MasterPT1_SG1`. When the save loads,
the quest runs step 12.1 again, now with the office already loaded. The graph starts, the conversation
appears, and the quest continues normally. It's a one-value change:

```sql
UPDATE MissionDynamic
   SET Text1 = 'Start  CCL_FIG_01_MasterPT1_SG1'      -- note: TWO spaces after "Start"
 WHERE MissionID = 'FIG_01' AND Type = 'Main' AND Text1 = 'WaitForSubMission01';
```

After loading the patched save and talking to Fig, the next save shows `FIG_01` `Active` at
`WaitForSubMission01a stat M_FIG_01_ExclusiveState` and `FIG_01_Sub01` `Complete`. The
`CCL_FIG_01_MasterPT1_SG1` rows now exist (e.g. `StoryNode_InputEvent_0`, activation source `FIG_01`).

### How the tool decides a save is stuck

A save counts as stuck only when **all** of these are true:

1. `FIG_01` Main row: `Text1 = 'WaitForSubMission01'` and `Text2 = 'PreActive'`
2. `FIG_01_Sub01` Main row: `Text1 = 'ConvoStart  CCL_FIG_01_MasterPT1_SG1'`
3. `SELECT COUNT(*) FROM StoryGraphPersistenceDynamic WHERE Key = 'CCL_FIG_01_MasterPT1_SG1'` is `0`

Condition 3 separates a bugged save from a healthy one where you simply haven't talked to Fig yet. On the
test machine, both kinds existed side by side and the rule told them apart correctly.

The tool also skips stuck saves that are **older** than a working save of the same character. That way
it never points you back to an old save when you've already moved on.

---

## How I found it

### 1. Research

The Reddit thread (r/hogwartslegacyJKR, "Anyone who's encountered the bug of being unable to talk to Fig
...") was blocked to plain scripted requests. Its `.rss` feed (`.../comments/1gwyrk9/.rss`) was readable
with a browser User-Agent. The thread confirmed the symptoms: weeks with no official fix, and the bug
coming back after restarting the storyline. It also gave the workaround above, which hinted that
replaying an earlier quest step fixes it.

### 2. Patching the save, not the game

The original idea was to patch game memory or the game binaries. That turned out unnecessary: the bug
lives entirely in **persisted quest state**, and the game rebuilds its runtime state from the save on
load. Editing the save is far simpler and survives game updates. Patching memory would mean chasing
Unreal object layouts and would break with every patch.

### 3. Cracking the save format

`xxd` / `strings` on a `.sav` showed a UE4 **GVAS** header and a property named `RawDatabaseImage`
(`ArrayProperty` of `ByteProperty`), but no plaintext `SQLite format 3`. So the database is compressed.
The bytes inside follow Unreal's `FCompression::SerializeCompressedData` layout, with **Oodle Kraken**
chunks:

```
GVAS header ... "RawDatabaseImage" ArrayProperty <u64 size> "ByteProperty" <u8 0> <u32 count> <bytes>

<bytes> = one or more units:
  u64 0x9E2A83C1            package tag
  u64 0x20000               chunk size
  u64 compressed, u64 uncompressed            (summary)
  (u64 compressed, u64 uncompressed) x N      (per chunk)
  N Oodle chunks

decompressed = i32 (len+4), i32 len, <SQLite database>
```

Decompression uses the pure-Rust Oodle decoder [`oozextract`](https://crates.io/crates/oozextract), so
no game DLL is needed.

**Writing saves back without an Oodle compressor:** an Oodle block whose header is `0xCC 0x06` is a
Kraken block flagged *uncompressed*, followed by the raw bytes. Every Oodle decoder accepts it, including
the game's. The tool writes the database back as stored blocks in the same 0x20000-byte units the game
uses, then fixes the two size fields. The save gets bigger (about 1.6 MB instead of 0.5 MB), and the game
loads it fine. It recompresses the file the next time it saves.

### 4. Reading the game's quest definitions

Quest step definitions aren't in the save, only "which step am I on". They live in the game's `.pak`
files, which **aren't encrypted**. I used [repak](https://github.com/trumank/repak) to extract them:

```sh
cargo install --locked --git https://github.com/trumank/repak repak_cli
PAKS=".../steamapps/common/Hogwarts Legacy/Phoenix/Content/Paks"
repak list "$PAKS/pakchunk0-WindowsNoEditor.pak" | grep -i -E 'sqlite|MissionList'
repak get  "$PAKS/pakchunk0-WindowsNoEditor.pak" Phoenix/Content/SQLiteDB/PhoenixShipData.sqlite > PhoenixShipData.sqlite
repak get  "$PAKS/pakchunk0-WindowsNoEditor.pak" Phoenix/Content/SQLiteDB/PhoenixGameData.sqlite > PhoenixGameData.sqlite
```

Useful tables:

| Database | Table | What's in it |
|---|---|---|
| PhoenixShipData | `MissionDefinition`, `MissionSteps` | Every quest and its steps (`Step`, `TaskTypeID`, `StepNickname`, `Keyword1..n`) |
| PhoenixShipData | `MissionTaskTypes` | What each keyword means per task type (e.g. `TriggerStoryGraphEvent` → actor, event, reset, wait-for-curtain) |
| PhoenixGameData | `FastTravelLocations` | Floo flame positions (used to sanity-check Fig's office coordinates) |

### 5. Diagnosis

I unpacked the stuck saves and compared them with each other and with the quest definitions:

```sql
-- in an unpacked save
SELECT MissionID, Text1, Text2 FROM MissionDynamic
 WHERE MissionID IN ('FIG_01', 'FIG_01_Sub01') AND Type = 'Main';
SELECT * FROM StoryGraphPersistenceDynamic WHERE Key LIKE '%FIG_01%';

-- in PhoenixShipData.sqlite
SELECT Step, TaskTypeID, StepNickname, Keyword1, Keyword2, Keyword3, Keyword4
  FROM MissionSteps WHERE MissionID IN ('FIG_01', 'FIG_01_Sub01') ORDER BY MissionID, Step;
```

In the save, `MissionDynamic`'s `Type='Main'` row holds the quest's current `StepNickname` in `Text1` and
its status in `Text2`. Mapping that onto `MissionSteps` showed `FIG_01` waiting on its sub-mission, and
the sub-mission waiting on a story-graph event. The graph itself had no persisted state at all.

Other save tables that helped:

| Table / row | Use |
|---|---|
| `MiscDataDynamic` `Player` / `LocX`,`LocY`,`LocZ`,`World` | Player position in cm (Fig's office ≈ 346980, −458580, −82608 in `Overland`) |
| `MiscDataDynamic` `Scheduler` / `SimTime` | In-game clock as FDateTime ticks (100 ns since 0001‑01‑01). Fig's quest entry is only active 10:00–14:00. |
| `MiscDataDynamic` `Player` / `PlayerFirstName`, `PlayerLastName` | Character name |
| `SaveGameList.sav` (separate GVAS file) | Slot list shown in the Load menu: slot, type, time, map. The game reads it at startup and keeps it in memory, so edit saves **with the game closed**. |

### 6. Applying and verifying the fix

1. I patched the most recent stuck save (one value, as above) and installed it into a manual save slot,
   after backing up the whole save folder.
2. I loaded it in Fig's office. The conversation started and the quest continued.
3. A save made afterwards showed the expected healthy state: graph rows present, sub-mission complete,
   quest active at its next step.

### 7. Turning it into a one-run tool

`hl-fig-fix` packages all of the above into one program:

- finds saves on Windows and in Proton prefixes
- applies the detection rule
- backs up, then patches in a temporary copy of the database (the original file is never opened for
  writing)
- repacks, then re-reads the result and runs `PRAGMA integrity_check`
- replaces the save atomically (write temp file + rename)
- refuses to run while `HogwartsLegacy.exe` is running

---

## Manual fix (for the curious, or if the tool's detection doesn't match your save)

You need `hl-fig-fix` (for unpack/pack) and any SQLite editor, such as `sqlite3` or
[DB Browser for SQLite](https://sqlitebrowser.org/).

```sh
# game closed; copy the save somewhere safe first
hl-fig-fix unpack HL-01-12.sav save.db
sqlite3 save.db "SELECT MissionID, Text1, Text2 FROM MissionDynamic WHERE MissionID IN ('FIG_01','FIG_01_Sub01') AND Type='Main';"
sqlite3 save.db "UPDATE MissionDynamic SET Text1='Start  CCL_FIG_01_MasterPT1_SG1' WHERE MissionID='FIG_01' AND Type='Main';"
hl-fig-fix pack HL-01-12.sav save.db HL-01-12.fixed.sav
# then replace HL-01-12.sav with HL-01-12.fixed.sav
```

If your `FIG_01` row shows something other than `WaitForSubMission01`, you probably have a different
problem. Don't apply this blindly.

---

## Tools used

| Tool | Used for |
|---|---|
| `curl` + Reddit's `.rss` endpoint | Reading the bug report thread |
| `xxd`, `strings` | First look at the `.sav` layout |
| Rust + [`oozextract`](https://crates.io/crates/oozextract) 0.5.5 | Oodle Kraken decompression of the embedded database |
| [`repak`](https://github.com/trumank/repak) 0.2.3 | Listing and extracting the game's `.pak` files (`PhoenixShipData.sqlite`, `PhoenixGameData.sqlite`) |
| `sqlite3`, `sqldiff` | Querying saves and game data; checking that the fix is a one-row change |
| [`rusqlite`](https://crates.io/crates/rusqlite) (bundled SQLite) | Database access inside the fixer |
| [`cargo-zigbuild`](https://github.com/rust-cross/cargo-zigbuild) + Zig | Cross-compiling the Windows and Linux binaries from one machine |

`hl-fig-fix unpack` / `pack` replace the small `hlsave` tool used during the investigation.

## Building from source

- `src/save.rs`: the save container format
- `src/main.rs`: discovery, detection, backup and patching

Build it with Rust 1.85 or newer:

```sh
git clone https://github.com/kream0/hl-fig-fix && cd hl-fig-fix
cargo build --release                 # binary for the machine you're on: target/release/hl-fig-fix
```

The shipped binaries were cross-compiled from Linux:

```sh
pip install ziglang && cargo install cargo-zigbuild && rustup target add x86_64-pc-windows-gnu
# keep your home directory out of the panic-message paths baked into the binary
export RUSTFLAGS="--remap-path-prefix=$HOME/.cargo/registry/src=/cargo --remap-path-prefix=$HOME/.rustup=/rustup --remap-path-prefix=$PWD=/hl-fig-fix"
cargo zigbuild --release --target x86_64-pc-windows-gnu          # -> hl-fig-fix.exe
cargo zigbuild --release --target x86_64-unknown-linux-gnu.2.17  # -> hl-fig-fix-linux (glibc >= 2.17)
```

SHA-256 of the v0.1.0 release binaries:

```
44773e487e15584737bc81ec1ec3ea177e190cd816dee7f2f52a16d56a2e9dd3  hl-fig-fix.exe
6057c1cb878f370faa8d58a43f9af03fafe9c43e5b5f4068a3a2824239b97f12  hl-fig-fix-linux
```

## What was tested

- **In-game:** the fix itself (the one-value change, repacked with stored Oodle blocks) on a real stuck
  Steam save on Windows. It was loaded while standing in Fig's office between 10:00 and 14:00 in-game.
  The conversation started and the quest progressed. That is one player and one save.
- **The tool, on copies of real saves:**
  - It flagged exactly the bugged saves and passed over a healthy save at the same quest step.
  - Each fixed save differs from its backup by that single row, and `PRAGMA integrity_check` passes.
  - Run on the save that was fixed by hand, its output is logically identical (`sqldiff` is empty) to the
    save confirmed working in-game.
- **Windows exe:** run on Windows 10 from a terminal. Tested: auto-detection, fixing, Windows paths, and
  the "game is running" guard.
- **Linux binary:** tested on Linux with a simulated Steam / Steam Deck layout, including a second
  library on another drive. It was **not** run on an actual Steam Deck.

**Not tested:**

- Loading a patched save made somewhere other than Fig's office. The replayed step fires right after the
  loading screen, wherever you are, so it may hit the same race again. In that case, go to his office,
  make a manual save there, quit, and run the tool again. That puts you in the tested situation.
- Double-clicking the exe in Explorer. The prompts and the "press Enter to close" pause were only
  exercised from a terminal.
- The Epic Games version. It's believed to use the same save folder.
- **Xbox app / Game Pass**: it stores saves in a different container format, so it is **not supported**.
- **Consoles**: not supported, because console saves can't be edited. Use the no-tool workaround above.

This project isn't affiliated with WB Games or Avalanche. You use it at your own risk, but backups are
always made first.
