//! Path search over a map's path nodes (`pathnodes`): the route AI walks is
//! the cheapest open chain of the map's own links, and the same graph always
//! gives the same route.

use pathnodes::{PathGraph, PathLink, PathNode};

fn node(x: f32, y: f32) -> PathNode {
    PathNode {
        origin: [x, y, 0.0],
        ..PathNode::default()
    }
}

/// Links both ways, with the straight distance as the link length.
fn link(graph: &mut PathGraph, a: u32, b: u32) {
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

/// A square 0-1-2-3 with a long way round (0-1-2) and a short cut (0-3-2).
fn square() -> PathGraph {
    let mut graph = PathGraph {
        nodes: vec![
            node(0.0, 0.0),
            node(0.0, 100.0),
            node(100.0, 100.0),
            node(60.0, 40.0),
        ],
    };
    link(&mut graph, 0, 1);
    link(&mut graph, 1, 2);
    link(&mut graph, 0, 3);
    link(&mut graph, 3, 2);
    graph
}

#[test]
fn find_path_takes_the_cheapest_chain_of_links() {
    let graph = square();
    assert_eq!(graph.find_path(0, 2), Some(vec![0, 3, 2]));
    assert_eq!(graph.find_path(2, 2), Some(vec![2]));
}

#[test]
fn a_disconnected_link_is_never_walked() {
    let mut graph = square();
    for link in &mut graph.nodes[0].links {
        if link.to == 3 {
            link.disconnected = true;
        }
    }
    assert_eq!(graph.find_path(0, 2), Some(vec![0, 1, 2]));
    for link in &mut graph.nodes[0].links {
        link.disconnected = true;
    }
    assert_eq!(graph.find_path(0, 2), None);
}

#[test]
fn equal_routes_resolve_to_the_same_one_every_time() {
    // Two mirror-image routes of equal length around a diamond.
    let mut graph = PathGraph {
        nodes: vec![
            node(0.0, 0.0),
            node(50.0, 50.0),
            node(50.0, -50.0),
            node(100.0, 0.0),
        ],
    };
    link(&mut graph, 0, 1);
    link(&mut graph, 0, 2);
    link(&mut graph, 1, 3);
    link(&mut graph, 2, 3);
    let first = graph.find_path(0, 3);
    for _ in 0..10 {
        assert_eq!(graph.clone().find_path(0, 3), first);
    }
    assert_eq!(first, Some(vec![0, 1, 3]));
}

#[test]
fn nearest_node_respects_the_search_radius() {
    let graph = square();
    assert_eq!(graph.nearest_node([55.0, 45.0, 0.0], 50.0), Some(3));
    assert_eq!(graph.nearest_node([500.0, 500.0, 0.0], 50.0), None);
}

#[test]
fn islands_are_counted_over_open_links() {
    let mut graph = square();
    graph.nodes.push(node(900.0, 900.0));
    assert_eq!(graph.component_count(), 2);
}
