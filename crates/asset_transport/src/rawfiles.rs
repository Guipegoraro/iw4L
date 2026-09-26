//! `iw4l export-rawfiles <zone>`: write every raw file in a zone (scripts,
//! configs, tables) to disk, inflating the zlib-packed ones. Black Ops keeps
//! its GSC/CSC scripts as rawfiles whose body is an 8-byte size prefix and a
//! zlib stream; they come out as plain source text.
//!
//! This is how the zombie scripts are read for porting, and how a custom map
//! or mod's scripts can be looked at. The output is the player's own game
//! data: it stays under `iw4l-artifacts/` and is never committed.

use std::path::{Path, PathBuf};

use fastfile_t5::{AssetLinkSink, AssetSink, AssetType, Ptr, ScriptStrings, ZoneStream};

#[derive(Debug, Default)]
pub struct RawFileExport {
    pub files: usize,
    pub inflated: usize,
    pub bytes: usize,
}

/// Export the raw files of a Black Ops (T5) zone into `out_dir`.
pub fn export_t5_rawfiles(zone: &Path, out_dir: &Path) -> Result<RawFileExport, String> {
    let image = crate::open_zone(zone).map_err(|e| format!("{}: {e:?}", zone.display()))?;
    let header = image
        .t5_header()
        .map_err(|e| format!("{}: not a Black Ops zone ({e:?})", zone.display()))?;
    let mut memory = crate::T5ZoneMemory::for_header(&header);
    let mut stream = memory
        .stream(&image.bytes)
        .map_err(|e| format!("{}: {e:?}", zone.display()))?;
    let mut sink = RawFileSink {
        out_dir: out_dir.to_path_buf(),
        export: RawFileExport::default(),
        error: None,
    };
    fastfile_t5::load_zone(&mut stream, &mut sink)
        .map_err(|e| format!("{}: walk stopped: {e:?}", zone.display()))?;
    // A map zone's entity string (spawners, zones, doors, structs), as
    // `mapents.txt`: the map's half of what its scripts look up.
    let entities = stream.map_ents().and_then(|ents| {
        let text = ents.entity_string?;
        stream.slice_at(text, 0, ents.entity_chars).ok()
    });
    if let Some(text) = entities {
        let text = text.strip_suffix(&[0]).unwrap_or(text);
        std::fs::create_dir_all(out_dir)
            .and_then(|()| std::fs::write(out_dir.join("mapents.txt"), text))
            .map_err(|e| format!("{}: {e}", out_dir.display()))?;
        sink.export.files += 1;
        sink.export.bytes += text.len();
    }
    match sink.error {
        Some(error) => Err(error),
        None => Ok(sink.export),
    }
}

/// A packed script: an 8-byte size prefix, then a zlib stream.
fn unpack(data: &[u8]) -> Option<Vec<u8>> {
    let body = data.get(8..)?;
    if body.len() < 2 || body[0] != 0x78 {
        return None;
    }
    crate::iwd::inflate_zlib(body).ok()
}

struct RawFileSink {
    out_dir: PathBuf,
    export: RawFileExport,
    error: Option<String>,
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

    fn capture_raw_file(
        &mut self,
        name: &str,
        data: &[u8],
        _zlib_compressed: bool,
    ) -> fastfile_t5::Result<()> {
        let data = data.strip_suffix(&[0]).unwrap_or(data);
        let unpacked = unpack(data);
        let body = unpacked.as_deref().unwrap_or(data);
        // Zone names use `/` or `\`; keep them inside the output folder.
        let relative: PathBuf = name
            .split(['/', '\\'])
            .filter(|part| !part.is_empty() && *part != "..")
            .map(|part| part.replace(':', "_"))
            .collect();
        let out = self.out_dir.join(relative);
        let written = out
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&out, body));
        match written {
            Ok(()) => {
                self.export.files += 1;
                self.export.inflated += usize::from(unpacked.is_some());
                self.export.bytes += body.len();
            }
            Err(error) if self.error.is_none() => {
                self.error = Some(format!("{}: {error}", out.display()));
            }
            Err(_) => {}
        }
        Ok(())
    }
}
