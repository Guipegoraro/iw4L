//! Path search over a map's path nodes (`pathnodes`): the route AI walks is
//! the cheapest open chain of the map's own links, and the same graph always
//! gives the same route.

use pathnodes::PathGraph;

use crate::path_fixtures::{link, node};

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

/// A corridor of three nodes along x, 100 units apart.
fn corridor() -> PathGraph {
    let mut graph = PathGraph {
        nodes: vec![node(0.0, 0.0), node(100.0, 0.0), node(200.0, 0.0)],
    };
    link(&mut graph, 0, 1);
    link(&mut graph, 1, 2);
    graph
}

#[test]
fn a_route_walks_the_nodes_then_ends_on_the_goal() {
    let route = corridor()
        .route([-10.0, 0.0, 0.0], [230.0, 10.0, 0.0])
        .expect("both ends are near a node");
    assert_eq!(
        route.waypoints,
        vec![
            [0.0, 0.0, 0.0],
            [100.0, 0.0, 0.0],
            [200.0, 0.0, 0.0],
            [230.0, 10.0, 0.0]
        ]
    );
}

#[test]
fn a_route_skips_a_start_node_behind_the_actor() {
    // The actor stands between nodes 0 and 1, closer to 1.
    let route = corridor()
        .route([45.0, 0.0, 0.0], [200.0, 0.0, 0.0])
        .expect("both ends are near a node");
    assert_eq!(route.waypoints[0], [100.0, 0.0, 0.0]);
}

#[test]
fn no_route_is_a_bad_path() {
    let mut graph = corridor();
    graph.nodes.push(node(5000.0, 0.0));
    assert_eq!(graph.route([0.0, 0.0, 0.0], [5000.0, 0.0, 0.0]), None);
    assert_eq!(graph.route([0.0, 0.0, 0.0], [90_000.0, 0.0, 0.0]), None);
}

#[test]
fn advancing_turns_corners_and_keeps_the_distance() {
    let mut route = pathnodes::Route {
        waypoints: vec![[100.0, 0.0, 0.0], [100.0, 100.0, 0.0]],
        next: 0,
    };
    // 150 units: 100 to the corner, then 50 up the second leg.
    let step = route.advance([0.0, 0.0, 7.0], 150.0, 0.0);
    assert_eq!(step.origin, [100.0, 50.0, 7.0]);
    assert_eq!(step.heading, Some([0.0, 1.0]));
    assert!(!step.arrived);
}

#[test]
fn advancing_stops_on_the_goal_radius_and_reports_arrival() {
    let mut route = pathnodes::Route::direct([100.0, 0.0, 0.0]);
    let step = route.advance([0.0, 0.0, 0.0], 500.0, 32.0);
    assert_eq!(step.origin, [68.0, 0.0, 0.0]);
    assert!(step.arrived);
    let again = route.advance(step.origin, 10.0, 32.0);
    assert_eq!(again.origin, step.origin);
    assert!(again.arrived);
}
