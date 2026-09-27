//! Walking a found path: the waypoints an actor heads for, one after the
//! other, and how far along it a step of movement takes it.
//!
//! Movement is measured in the ground plane (x, y); the height comes from the
//! ground under the actor, which the caller traces.

use crate::PathGraph;

/// How far from an endpoint the search looks for the node to start or end on.
pub const ROUTE_NODE_SEARCH_RADIUS: f32 = 512.0;

/// A waypoint closer than this counts as reached and the next one is taken.
const WAYPOINT_REACHED: f32 = 1.0;

/// A path to a goal position: the path nodes' origins, then the goal itself.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Route {
    pub waypoints: Vec<[f32; 3]>,
    /// The waypoint being walked to.
    pub next: usize,
}

/// What one [`Route::advance`] did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RouteStep {
    /// The new position; its height is the caller's to set.
    pub origin: [f32; 3],
    /// The ground-plane direction of the last bit of movement, if any.
    pub heading: Option<[f32; 2]>,
    /// Within the goal radius of the last waypoint.
    pub arrived: bool,
}

fn flat_distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// How far along the ray `at + dir * t`, `t` in `0..=len`, it first comes
/// within `radius` of `center` in the ground plane.
fn enters_circle(
    at: [f32; 3],
    dir: [f32; 2],
    len: f32,
    center: [f32; 3],
    radius: f32,
) -> Option<f32> {
    let off = [at[0] - center[0], at[1] - center[1]];
    let b = off[0] * dir[0] + off[1] * dir[1];
    let c = off[0] * off[0] + off[1] * off[1] - radius * radius;
    let disc = b * b - c;
    if disc < 0.0 {
        return None;
    }
    let t = (-b - disc.sqrt()).max(0.0);
    (t <= len).then_some(t)
}

impl PathGraph {
    /// `SetGoalPos`'s path: the nearest node to each end, the cheapest open
    /// chain between them, then `goal`. `None` is the engine's `bad_path`:
    /// no node near an end, or no open chain. Two ends on the same node walk
    /// straight to the goal.
    pub fn route(&self, from: [f32; 3], goal: [f32; 3]) -> Option<Route> {
        let start = self.nearest_node(from, ROUTE_NODE_SEARCH_RADIUS)?;
        let end = self.nearest_node(goal, ROUTE_NODE_SEARCH_RADIUS)?;
        let nodes = self.find_path(start, end)?;
        let mut waypoints: Vec<[f32; 3]> = nodes
            .iter()
            .map(|&node| self.nodes[node as usize].origin)
            .collect();
        // The start node may sit behind the actor: when the actor is nearer
        // the second waypoint than the first waypoint is, it is already on
        // its way and the first is skipped.
        if waypoints.len() >= 2
            && flat_distance(from, waypoints[1]) < flat_distance(waypoints[0], waypoints[1])
        {
            waypoints.remove(0);
        }
        waypoints.push(goal);
        Some(Route { waypoints, next: 0 })
    }
}

impl Route {
    /// A route straight to `goal`.
    pub fn direct(goal: [f32; 3]) -> Self {
        Self {
            waypoints: vec![goal],
            next: 0,
        }
    }

    pub fn goal(&self) -> Option<[f32; 3]> {
        self.waypoints.last().copied()
    }

    /// Within `goal_radius` of the goal, in the ground plane.
    pub fn at_goal(&self, origin: [f32; 3], goal_radius: f32) -> bool {
        self.goal()
            .is_some_and(|goal| flat_distance(origin, goal) <= goal_radius)
    }

    /// Moves `origin` up to `distance` units along the waypoints, stopping at
    /// the goal radius.
    pub fn advance(&mut self, origin: [f32; 3], distance: f32, goal_radius: f32) -> RouteStep {
        let mut at = origin;
        let mut left = distance.max(0.0);
        let mut heading = None;
        loop {
            if self.at_goal(at, goal_radius) {
                return RouteStep {
                    origin: at,
                    heading,
                    arrived: true,
                };
            }
            let Some(&target) = self.waypoints.get(self.next) else {
                return RouteStep {
                    origin: at,
                    heading,
                    arrived: false,
                };
            };
            let last = self.next + 1 == self.waypoints.len();
            let to_target = flat_distance(at, target);
            if !last && to_target <= WAYPOINT_REACHED {
                self.next += 1;
                continue;
            }
            if left <= 0.0 {
                return RouteStep {
                    origin: at,
                    heading,
                    arrived: false,
                };
            }
            let dir = [
                (target[0] - at[0]) / to_target,
                (target[1] - at[1]) / to_target,
            ];
            // The last leg stops at the goal radius, not on the goal.
            let room = if last {
                (to_target - goal_radius).max(0.0)
            } else {
                to_target
            };
            let step = left.min(room);
            heading = Some(dir);
            // A leg toward an earlier waypoint may pass within the goal
            // radius: the actor stops where it first does.
            if !last
                && let Some(goal) = self.goal()
                && let Some(enter) = enters_circle(at, dir, step, goal, goal_radius)
            {
                at = [at[0] + dir[0] * enter, at[1] + dir[1] * enter, at[2]];
                return RouteStep {
                    origin: at,
                    heading,
                    arrived: true,
                };
            }
            at = [at[0] + dir[0] * step, at[1] + dir[1] * step, at[2]];
            left -= step;
            if step >= room {
                if last {
                    // On the goal radius (a float may land just outside it).
                    return RouteStep {
                        origin: at,
                        heading,
                        arrived: true,
                    };
                }
                self.next += 1;
            }
        }
    }
}
