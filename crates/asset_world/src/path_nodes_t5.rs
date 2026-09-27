//! `PathData` nodes and links, read back after the walk.
//!
//! A node's constant part has the Call of Duty 4 `pathnode_constant_t` layout,
//! which the walk's own offsets confirm (`totalLinkCount` at 0x3e, `Links` at
//! 0x40, 12-byte links); names are script-string indices.

use fastfile_t5::{Ptr, ScriptStrings, ZonePtr, ZoneStream};

const TYPE_OFF: usize = 0x00;
const SPAWNFLAGS_OFF: usize = 0x04;
const TARGETNAME_OFF: usize = 0x06;
const SCRIPT_LINKNAME_OFF: usize = 0x08;
const SCRIPT_NOTEWORTHY_OFF: usize = 0x0a;
const TARGET_OFF: usize = 0x0c;
const ANIMSCRIPT_OFF: usize = 0x0e;
const ORIGIN_OFF: usize = 0x14;
const ANGLE_OFF: usize = 0x20;
const RADIUS_OFF: usize = 0x2c;

const LINK_DIST_OFF: usize = 0;
const LINK_NODE_OFF: usize = 4;
const LINK_DISCONNECT_OFF: usize = 6;
const LINK_NEGOTIATION_OFF: usize = 7;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PathNodeRecord {
    /// The engine's node type enum, as stored.
    pub node_type: u32,
    pub spawnflags: u16,
    pub targetname: String,
    pub script_linkname: String,
    pub script_noteworthy: String,
    pub target: String,
    /// A negotiation (traversal) node's animscript, e.g.
    /// `zombie_mantle_over_40`.
    pub animscript: String,
    pub origin: [f32; 3],
    pub angle: f32,
    pub radius: f32,
    pub links: Vec<PathLinkRecord>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PathLinkRecord {
    pub dist: f32,
    pub node: u16,
    pub disconnect_count: u8,
    /// The link is a traversal: the pair of negotiation nodes it joins is
    /// crossed by the begin node's animscript.
    pub negotiation: bool,
}

/// Every node of the zone's `PathData`, in node-number order; empty when the
/// zone has none.
pub fn read_path_nodes(s: &ZoneStream<'_>, strings: &ScriptStrings) -> Vec<PathNodeRecord> {
    let Some(geometry) = s.path_data() else {
        return Vec::new();
    };
    let Some(nodes) = geometry.nodes else {
        return Vec::new();
    };
    (0..geometry.node_count)
        .filter_map(|index| read_node(s, strings, nodes.at(index * fastfile_t5::size::PATH_NODE)))
        .collect()
}

fn read_node(s: &ZoneStream<'_>, strings: &ScriptStrings, node: Ptr) -> Option<PathNodeRecord> {
    let name = |off: usize| -> String {
        s.u16_at(node, off)
            .ok()
            .filter(|&index| index != 0)
            .and_then(|index| strings.get(s, index))
            .unwrap_or("")
            .to_owned()
    };
    let link_count = s
        .u16_at(node, fastfile_t5::size::PATH_NODE_TOTAL_LINK_COUNT_OFF)
        .ok()? as usize;
    let links = match s
        .ptr_at(node, fastfile_t5::size::PATH_NODE_LINKS_OFF)
        .ok()?
    {
        ZonePtr::Offset(links) => {
            let links = s.resolve_alias(links);
            (0..link_count)
                .filter_map(|i| {
                    let link = links.at(i * fastfile_t5::size::PATH_LINK);
                    Some(PathLinkRecord {
                        dist: s.f32_at(link, LINK_DIST_OFF).ok()?,
                        node: s.u16_at(link, LINK_NODE_OFF).ok()?,
                        disconnect_count: s.u8_at(link, LINK_DISCONNECT_OFF).ok()?,
                        negotiation: s.u8_at(link, LINK_NEGOTIATION_OFF).ok()? != 0,
                    })
                })
                .collect()
        }
        _ => Vec::new(),
    };
    Some(PathNodeRecord {
        node_type: s.u32_at(node, TYPE_OFF).ok()?,
        spawnflags: s.u16_at(node, SPAWNFLAGS_OFF).ok()?,
        targetname: name(TARGETNAME_OFF),
        script_linkname: name(SCRIPT_LINKNAME_OFF),
        script_noteworthy: name(SCRIPT_NOTEWORTHY_OFF),
        target: name(TARGET_OFF),
        animscript: name(ANIMSCRIPT_OFF),
        origin: [
            s.f32_at(node, ORIGIN_OFF).ok()?,
            s.f32_at(node, ORIGIN_OFF + 4).ok()?,
            s.f32_at(node, ORIGIN_OFF + 8).ok()?,
        ],
        angle: s.f32_at(node, ANGLE_OFF).ok()?,
        radius: s.f32_at(node, RADIUS_OFF).ok()?,
        links,
    })
}
