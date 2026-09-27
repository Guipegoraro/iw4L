//! A map's path nodes, as its compiled `PathData` holds them, and the search AI
//! walks them with.
//!
//! The links are the map's own, precomputed by the map compiler; a search here
//! never invents one. Traversal links (a pair of negotiation nodes crossed by
//! an animscript, like a window mantle) are ordinary links flagged
//! `negotiation`. A link a script closed (`disconnectpaths`) is skipped.
//!
//! The search is A* on link distance with the straight-line distance as the
//! heuristic; equal costs are broken by node number, so a path is a pure
//! function of the graph and its endpoints.

mod route;

pub use route::{ROUTE_NODE_SEARCH_RADIUS, Route, RouteStep};

use std::cmp::Ordering;
use std::collections::BinaryHeap;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PathNode {
    pub origin: [f32; 3],
    /// The engine's node type, as stored.
    pub node_type: u32,
    pub targetname: String,
    pub target: String,
    pub script_noteworthy: String,
    /// A negotiation node's traversal animscript.
    pub animscript: String,
    pub links: Vec<PathLink>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PathLink {
    pub to: u32,
    pub dist: f32,
    pub negotiation: bool,
    /// Closed by a script (`disconnectpaths`) or by the map.
    pub disconnected: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PathGraph {
    pub nodes: Vec<PathNode>,
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

#[derive(Clone, Copy, PartialEq)]
struct Open {
    estimate: f32,
    node: u32,
}

impl Eq for Open {}

impl Ord for Open {
    fn cmp(&self, other: &Self) -> Ordering {
        // A max-heap: the smallest estimate, then the lowest node, comes out first.
        other
            .estimate
            .total_cmp(&self.estimate)
            .then_with(|| other.node.cmp(&self.node))
    }
}

impl PartialOrd for Open {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PathGraph {
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn link_count(&self) -> usize {
        self.nodes.iter().map(|node| node.links.len()).sum()
    }

    /// How many islands the open links split the nodes into, links taken both
    /// ways; a map's walkable space is normally one, plus a few loose nodes.
    pub fn component_count(&self) -> usize {
        let count = self.nodes.len();
        let mut neighbours = vec![Vec::new(); count];
        for (from, node) in self.nodes.iter().enumerate() {
            for link in node.links.iter().filter(|link| !link.disconnected) {
                let to = link.to as usize;
                if to < count {
                    neighbours[from].push(to);
                    neighbours[to].push(from);
                }
            }
        }
        let mut seen = vec![false; count];
        let mut islands = 0;
        for start in 0..count {
            if seen[start] {
                continue;
            }
            islands += 1;
            let mut stack = vec![start];
            seen[start] = true;
            while let Some(at) = stack.pop() {
                for &next in &neighbours[at] {
                    if !seen[next] {
                        seen[next] = true;
                        stack.push(next);
                    }
                }
            }
        }
        islands
    }

    /// The node closest to `origin` within `max_dist`, lowest number on a tie.
    pub fn nearest_node(&self, origin: [f32; 3], max_dist: f32) -> Option<u32> {
        self.nodes
            .iter()
            .enumerate()
            .map(|(i, node)| (i as u32, distance(node.origin, origin)))
            .filter(|&(_, d)| d <= max_dist)
            .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
            .map(|(i, _)| i)
    }

    /// The cheapest open route from `from` to `to`, both ends included.
    pub fn find_path(&self, from: u32, to: u32) -> Option<Vec<u32>> {
        let count = self.nodes.len();
        let (from_i, to_i) = (from as usize, to as usize);
        if from_i >= count || to_i >= count {
            return None;
        }
        let goal = self.nodes[to_i].origin;
        let mut cost = vec![f32::INFINITY; count];
        let mut came_from = vec![u32::MAX; count];
        let mut closed = vec![false; count];
        let mut open = BinaryHeap::new();
        cost[from_i] = 0.0;
        open.push(Open {
            estimate: distance(self.nodes[from_i].origin, goal),
            node: from,
        });
        while let Some(Open { node, .. }) = open.pop() {
            let n = node as usize;
            if closed[n] {
                continue;
            }
            if node == to {
                let mut path = vec![to];
                let mut at = to;
                while at != from {
                    at = came_from[at as usize];
                    path.push(at);
                }
                path.reverse();
                return Some(path);
            }
            closed[n] = true;
            for link in &self.nodes[n].links {
                let next = link.to as usize;
                if link.disconnected || next >= count || closed[next] {
                    continue;
                }
                let through = cost[n] + link.dist;
                if through < cost[next] {
                    cost[next] = through;
                    came_from[next] = node;
                    open.push(Open {
                        estimate: through + distance(self.nodes[next].origin, goal),
                        node: link.to,
                    });
                }
            }
        }
        None
    }
}
