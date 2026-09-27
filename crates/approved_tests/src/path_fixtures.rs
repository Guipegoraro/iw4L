//! Small path graphs for the tests that walk them: nodes on the ground and
//! links both ways.

use pathnodes::{PathGraph, PathLink, PathNode};

/// A node on the ground at `x, y`, with no links yet.
pub fn node(x: f32, y: f32) -> PathNode {
    PathNode {
        origin: [x, y, 0.0],
        ..PathNode::default()
    }
}

/// Links both ways, with the straight distance as the link length.
pub fn link(graph: &mut PathGraph, a: u32, b: u32) {
    let (pa, pb) = (
        graph.nodes[a as usize].origin,
        graph.nodes[b as usize].origin,
    );
    let dist = ((pa[0] - pb[0]).powi(2) + (pa[1] - pb[1]).powi(2)).sqrt();
    for (from, to) in [(a, b), (b, a)] {
        graph.nodes[from as usize].links.push(PathLink {
            to,
            dist,
            negotiation: false,
            disconnected: false,
        });
    }
}
