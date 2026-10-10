//! Hogwarts Legacy `.sav` <-> SQLite.
//!
//! A save is a UE GVAS file. Its `RawDatabaseImage` property (ArrayProperty<ByteProperty>) holds the
//! game-state SQLite database, written with UE's `FCompression::SerializeCompressedData`:
//! a run of units, each one
//!     u64 tag 0x9E2A83C1, u64 chunk size, u64 compressed total, u64 uncompressed total,
//!     (u64 compressed, u64 uncompressed) per chunk, then the Oodle (Kraken) chunks.
//! The decompressed bytes are i32 (len + 4), i32 len, then the SQLite file itself.
//!
//! Writing uses Oodle "stored" blocks (header 0xCC 0x06, then the raw bytes), which every Oodle
//! decoder, including the game's, accepts. That way no Oodle encoder is needed.

use oozextract::Extractor;

pub type Res<T> = Result<T, String>;

const TAG: u64 = 0x9E2A83C1;
const CHUNK: usize = 0x20000;
const PROP: &[u8] = b"RawDatabaseImage\0";
const SQLITE_MAGIC: &[u8] = b"SQLite format 3\0";

fn rd32(b: &[u8], o: usize) -> Res<u32> {
    b.get(o..o + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap())).ok_or_else(|| format!("file truncated at offset {o}"))
}

fn rd64(b: &[u8], o: usize) -> Res<u64> {
    b.get(o..o + 8).map(|s| u64::from_le_bytes(s.try_into().unwrap())).ok_or_else(|| format!("file truncated at offset {o}"))
}

fn fstring<'a>(b: &'a [u8], o: &mut usize) -> Res<&'a [u8]> {
    let n = rd32(b, *o)? as usize;
    *o += 4;
    let s = b.get(*o..*o + n).ok_or("file truncated in a string")?;
    *o += n;
    Ok(s)
}

struct Layout { size_off: usize, count_off: usize, data_off: usize, data_len: usize }

fn locate(sav: &[u8]) -> Res<Layout> {
    let p = sav.windows(PROP.len()).position(|w| w == PROP).ok_or("not a Hogwarts Legacy save (no RawDatabaseImage)")?;
    let mut o = p + PROP.len();
    if fstring(sav, &mut o)? != b"ArrayProperty\0" { return Err("RawDatabaseImage is not an ArrayProperty".into()); }
    let size_off = o;
    o += 8;
    if fstring(sav, &mut o)? != b"ByteProperty\0" { return Err("RawDatabaseImage is not a byte array".into()); }
    if sav.get(o) != Some(&0) { return Err("unexpected property GUID".into()); }
    o += 1;
    let count_off = o;
    let data_len = rd32(sav, o)? as usize;
    o += 4;
    if rd64(sav, size_off)? as usize != data_len + 4 || o + data_len > sav.len() {
        return Err("RawDatabaseImage size fields don't match".into());
    }
    Ok(Layout { size_off, count_off, data_off: o, data_len })
}

fn decompress(blob: &[u8]) -> Res<Vec<u8>> {
    let mut out = Vec::new();
    let mut o = 0;
    while o < blob.len() {
        if rd64(blob, o)? != TAG { return Err(format!("bad compression tag at {o}")); }
        let chunk = rd64(blob, o + 8)? as usize;
        let total_u = rd64(blob, o + 24)? as usize;
        if chunk == 0 { return Err("zero chunk size".into()); }
        o += 32;
        let n = total_u.div_ceil(chunk);
        let mut infos = Vec::with_capacity(n);
        for i in 0..n {
            infos.push((rd64(blob, o + i * 16)? as usize, rd64(blob, o + i * 16 + 8)? as usize));
        }
        o += n * 16;
        for (c, u) in infos {
            let src = blob.get(o..o + c).ok_or("compressed chunk runs past the end")?;
            let mut buf = vec![0u8; u];
            let got = Extractor::new().read_from_slice(src, &mut buf).map_err(|e| format!("Oodle decode failed: {e:?}"))?;
            if got != u { return Err(format!("chunk decoded to {got} bytes, expected {u}")); }
            out.extend_from_slice(&buf);
            o += c;
        }
    }
    Ok(out)
}

fn compress_stored(data: &[u8]) -> Vec<u8> {
    let mut blob = Vec::with_capacity(data.len() + data.len() / CHUNK * 50 + 50);
    for piece in data.chunks(CHUNK) {
        let c = piece.len() + 2;
        for v in [TAG, CHUNK as u64, c as u64, piece.len() as u64, c as u64, piece.len() as u64] {
            blob.extend_from_slice(&v.to_le_bytes());
        }
        blob.extend_from_slice(&[0xCC, 0x06]);
        blob.extend_from_slice(piece);
    }
    blob
}

/// Returns the SQLite database stored in a save.
pub fn read_db(sav: &[u8]) -> Res<Vec<u8>> {
    let l = locate(sav)?;
    let raw = decompress(&sav[l.data_off..l.data_off + l.data_len])?;
    let n = rd32(&raw, 4)? as usize;
    if rd32(&raw, 0)? as usize != n + 4 || raw.len() != n + 8 || !raw[8..].starts_with(SQLITE_MAGIC) {
        return Err("embedded database has an unexpected header".into());
    }
    Ok(raw[8..].to_vec())
}

/// Returns a copy of `sav` with its database replaced by `sqlite`, verified by reading it back.
pub fn write_db(sav: &[u8], sqlite: &[u8]) -> Res<Vec<u8>> {
    let l = locate(sav)?;
    let mut raw = Vec::with_capacity(sqlite.len() + 8);
    raw.extend_from_slice(&((sqlite.len() + 4) as u32).to_le_bytes());
    raw.extend_from_slice(&(sqlite.len() as u32).to_le_bytes());
    raw.extend_from_slice(sqlite);
    let blob = compress_stored(&raw);

    let mut out = Vec::with_capacity(sav.len() + blob.len());
    out.extend_from_slice(&sav[..l.size_off]);
    out.extend_from_slice(&((blob.len() + 4) as u64).to_le_bytes());
    out.extend_from_slice(&sav[l.size_off + 8..l.count_off]);
    out.extend_from_slice(&(blob.len() as u32).to_le_bytes());
    out.extend_from_slice(&blob);
    out.extend_from_slice(&sav[l.data_off + l.data_len..]);

    if read_db(&out)? != sqlite { return Err("re-read check failed".into()); }
    Ok(out)
}
