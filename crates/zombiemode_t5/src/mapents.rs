//! The map's entities as GSC sees them: `GetEntArray( value, key )`,
//! `GetStructArray( value, key )` and the key/value pairs a spawner, volume or
//! struct was placed with.

/// One entity from the map's entity string, in map order.
/// The key naming the entity another one points at (`ent.target`).
pub const TARGET: &str = "target";
/// The key other entities point at, and `getent( name, "targetname" )`.
pub const TARGETNAME: &str = "targetname";
/// `ent.script_string`.
pub const SCRIPT_STRING: &str = "script_string";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapEnt {
    pub fields: Vec<(String, String)>,
}

impl MapEnt {
    /// `ent.<key>`; `None` is undefined.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }

    pub fn classname(&self) -> &str {
        self.get("classname").unwrap_or("")
    }

    pub fn origin(&self) -> [f32; 3] {
        self.vec3("origin")
    }

    pub fn angles(&self) -> [f32; 3] {
        self.vec3("angles")
    }

    fn vec3(&self, key: &str) -> [f32; 3] {
        let mut out = [0.0; 3];
        if let Some(text) = self.get(key) {
            for (slot, part) in out.iter_mut().zip(text.split_whitespace()) {
                *slot = part.parse().unwrap_or(0.0);
            }
        }
        out
    }
}

/// The entity string, parsed once per match.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapEnts {
    pub ents: Vec<MapEnt>,
}

impl MapEnts {
    /// Parses `{ "key" "value" ... }` blocks; anything malformed ends the
    /// block it is in.
    pub fn parse(text: &str) -> Self {
        let mut ents = Vec::new();
        let mut current: Option<MapEnt> = None;
        let mut pending_key: Option<String> = None;
        let mut rest = text;
        while let Some(start) = rest.find(['{', '}', '"']) {
            let token = rest.as_bytes()[start];
            rest = &rest[start + 1..];
            match token {
                b'{' => {
                    current = Some(MapEnt::default());
                    pending_key = None;
                }
                b'}' => {
                    if let Some(ent) = current.take() {
                        ents.push(ent);
                    }
                    pending_key = None;
                }
                _ => {
                    let Some(end) = rest.find('"') else {
                        break;
                    };
                    let word = rest[..end].to_owned();
                    rest = &rest[end + 1..];
                    match (pending_key.take(), current.as_mut()) {
                        (Some(key), Some(ent)) => ent.fields.push((key, word)),
                        (None, Some(_)) => pending_key = Some(word),
                        _ => {}
                    }
                }
            }
        }
        Self { ents }
    }

    pub fn get(&self, index: u32) -> Option<&MapEnt> {
        self.ents.get(index as usize)
    }

    /// `GetEntArray( value, key )` / `GetStructArray( value, key )`: the
    /// indices of every entity whose `key` is `value`, in map order.
    pub fn array(&self, value: &str, key: &str) -> Vec<u32> {
        self.ents
            .iter()
            .enumerate()
            .filter(|(_, ent)| ent.get(key) == Some(value))
            .map(|(i, _)| i as u32)
            .collect()
    }
}
