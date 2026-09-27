//! `iw4l export-rawfiles <zone>`: write every raw file in a zone (scripts,
//! configs, tables) to disk, inflating the zlib-packed ones. Black Ops keeps
//! its GSC/CSC scripts as rawfiles whose body is an 8-byte size prefix and a
//! zlib stream; they come out as plain source text.
//!
//! This is how the zombie scripts are read for porting, and how a custom map
//! or mod's scripts can be looked at. The output is the player's own game
//! data: it stays under `iw4l-artifacts/` and is never committed.

use std::path::{Path, PathBuf};

use fastfile_t5::{AssetLinkSink, AssetSink, AssetType, Ptr, ScriptStrings, ZonePtr, ZoneStream};

#[derive(Debug, Default)]
pub struct RawFileExport {
    pub files: usize,
    pub inflated: usize,
    pub bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RawFileKind {
    /// A rawfile: script source (unpacked when it was zlib-packed), config.
    Raw { inflated: bool },
    /// A string table, as CSV.
    StringTable,
}

#[derive(Clone, Debug)]
pub struct RawFileEntry {
    pub name: String,
    pub kind: RawFileKind,
    pub body: Vec<u8>,
}

/// A Black Ops (T5) zone's raw files and string tables, in zone order, plus its
/// entity string when it is a map zone.
#[derive(Clone, Debug, Default)]
pub struct ZoneRawFiles {
    pub files: Vec<RawFileEntry>,
    pub map_ents: Option<Vec<u8>>,
}

/// Reads the raw files of a Black Ops (T5) zone into memory.
pub fn read_t5_rawfiles(zone: &Path) -> Result<ZoneRawFiles, String> {
    let image = crate::open_zone(zone).map_err(|e| format!("{}: {e:?}", zone.display()))?;
    let header = image
        .t5_header()
        .map_err(|e| format!("{}: not a Black Ops zone ({e:?})", zone.display()))?;
    let mut memory = crate::T5ZoneMemory::for_header(&header);
    let mut stream = memory
        .stream(&image.bytes)
        .map_err(|e| format!("{}: {e:?}", zone.display()))?;
    let mut sink = RawFileSink::default();
    fastfile_t5::load_zone(&mut stream, &mut sink)
        .map_err(|e| format!("{}: walk stopped: {e:?}", zone.display()))?;
    // A map zone's entity string (spawners, zones, doors, structs): the map's
    // half of what its scripts look up.
    let map_ents = stream.map_ents().and_then(|ents| {
        let text = ents.entity_string?;
        let text = stream.slice_at(text, 0, ents.entity_chars).ok()?;
        Some(text.strip_suffix(&[0]).unwrap_or(text).to_vec())
    });
    Ok(ZoneRawFiles {
        files: sink.files,
        map_ents,
    })
}

/// Export the raw files of a Black Ops (T5) zone into `out_dir`; a map zone
/// also gives `mapents.txt`.
pub fn export_t5_rawfiles(zone: &Path, out_dir: &Path) -> Result<RawFileExport, String> {
    let read = read_t5_rawfiles(zone)?;
    let mut export = RawFileExport::default();
    let mut write = |name: &str, body: &[u8]| -> Result<(), String> {
        // Zone names use `/` or `\`; keep them inside the output folder.
        let relative: PathBuf = name
            .split(['/', '\\'])
            .filter(|part| !part.is_empty() && *part != "..")
            .map(|part| part.replace(':', "_"))
            .collect();
        let out = out_dir.join(relative);
        out.parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&out, body))
            .map_err(|error| format!("{}: {error}", out.display()))?;
        export.files += 1;
        export.bytes += body.len();
        Ok(())
    };
    for file in &read.files {
        write(&file.name, &file.body)?;
    }
    if let Some(text) = &read.map_ents {
        write("mapents.txt", text)?;
    }
    export.inflated = read
        .files
        .iter()
        .filter(|file| file.kind == RawFileKind::Raw { inflated: true })
        .count();
    Ok(export)
}

/// A packed script: an 8-byte size prefix, then a zlib stream.
fn unpack(data: &[u8]) -> Option<Vec<u8>> {
    let body = data.get(8..)?;
    if body.len() < 2 || body[0] != 0x78 {
        return None;
    }
    crate::iwd::inflate_zlib(body).ok()
}

#[derive(Default)]
struct RawFileSink {
    files: Vec<RawFileEntry>,
}

impl AssetSink for RawFileSink {
    fn set_script_strings(&mut self, _strings: ScriptStrings) {}

    fn load_asset(
        &mut self,
        s: &mut ZoneStream<'_>,
        _index: usize,
        ty: AssetType,
        slot: Ptr,
    ) -> fastfile_t5::Result<()> {
        fastfile_t5::load_asset_at_observed(s, ty, slot, self).map(|_| ())
    }
}

impl AssetLinkSink for RawFileSink {
    fn loaded(
        &mut self,
        _s: &ZoneStream<'_>,
        _ty: AssetType,
        _slot: Ptr,
        _insert_slot: Option<Ptr>,
    ) -> fastfile_t5::Result<()> {
        Ok(())
    }

    fn alias(&mut self, _ty: AssetType, _slot: Ptr, _target: Ptr) -> fastfile_t5::Result<()> {
        Ok(())
    }

    /// String tables (`mp/zombiemode.csv`, weapon and perk tables) come back
    /// out as CSV.
    fn capture_string_table(&mut self, s: &ZoneStream<'_>, header: Ptr) -> fastfile_t5::Result<()> {
        if let Some((name, csv)) = string_table_csv(s, header) {
            self.files.push(RawFileEntry {
                name,
                kind: RawFileKind::StringTable,
                body: csv.into_bytes(),
            });
        }
        Ok(())
    }

    fn capture_raw_file(
        &mut self,
        name: &str,
        data: &[u8],
        _zlib_compressed: bool,
    ) -> fastfile_t5::Result<()> {
        let data = data.strip_suffix(&[0]).unwrap_or(data);
        let unpacked = unpack(data);
        self.files.push(RawFileEntry {
            name: name.to_owned(),
            kind: RawFileKind::Raw {
                inflated: unpacked.is_some(),
            },
            body: unpacked.unwrap_or_else(|| data.to_vec()),
        });
        Ok(())
    }
}

/// A T5 `StringTable` asset as `(name, CSV)`: one line per row, cells joined
/// with `,`.
pub fn string_table_csv(s: &ZoneStream<'_>, header: Ptr) -> Option<(String, String)> {
    let text_at = |slot: Ptr| match s.ptr_at(slot, 0) {
        Ok(ZonePtr::Offset(p)) => s.cstr(s.resolve_alias(p)).unwrap_or("").to_owned(),
        _ => String::new(),
    };
    let name = text_at(header);
    if name.is_empty() {
        return None;
    }
    let columns = s.i32_at(header, 4).unwrap_or(0).max(0) as usize;
    let rows = s.i32_at(header, 8).unwrap_or(0).max(0) as usize;
    let mut csv = String::new();
    if let Ok(ZonePtr::Offset(cells)) = s.ptr_at(header, 12) {
        let cells = s.resolve_alias(cells);
        for row in 0..rows {
            let line: Vec<_> = (0..columns)
                .map(|col| {
                    let cell =
                        cells.at((row * columns + col) * fastfile_t5::size::STRING_TABLE_CELL);
                    text_at(cell)
                })
                .collect();
            csv.push_str(&line.join(","));
            csv.push('\n');
        }
    }
    Some((name, csv))
}
