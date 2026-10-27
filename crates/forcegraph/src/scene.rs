//! Payload → bodies. The snapshot is the contract the producer streams: a
//! full node/link list, an optional focus directive, everything else the
//! view's own business.
//!
//! The one rule that matters here: **a surviving node keeps its position**.
//! The scene is rebuilt on every snapshot (the wire carries whole snapshots,
//! and appends simply arrive as a longer list), but positions are carried over
//! by id, so appending three nodes grows the graph instead of relaying it.
//! That is what makes the view navigable at all — spatial memory dies the
//! moment a layout re-shuffles.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::force::{self, Body, Link};
use crate::math::Vec3;

/// Radius of the golden-spiral seed sphere for nodes with no placed neighbour.
/// Same order as a link's rest length: d3 seeds inside the graph's own scale,
/// and a seed cloud much wider than `link_distance` spends its whole alpha
/// budget just pulling itself together.
const SEED_RADIUS: f32 = 30.0;
/// How far off the neighbour centroid a new node starts, as a fraction of the
/// seed radius: close enough to belong to the cluster, far enough that no two
/// nodes ever share a position.
const SEED_OFFSET: f32 = 0.25;
/// How many points on a new node's spiral to try before accepting a crowded
/// one. A golden-angle walk can revisit a spot it has already visited, and two
/// nodes seeded in the same place give the layout no direction to separate
/// along.
const SEED_ATTEMPTS: usize = 8;
/// Two seeded nodes closer than this are treated as coincident. A fifth of the
/// seed radius: well under a link's rest length, well over a rounding error.
const MIN_SEPARATION: f32 = SEED_RADIUS * 0.2;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Node {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub weight: Option<f32>,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GraphLink {
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub strength: Option<f32>,
    #[serde(default)]
    pub color: Option<String>,
}

/// "Bring these into view": the producer's way of saying a search landed.
/// `seq` lets an identical target be re-flown (bump it and the camera moves
/// again); without it, the same ids twice is a no-op.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Focus {
    #[serde(default)]
    pub ids: Vec<String>,
    #[serde(default)]
    pub seq: u64,
    #[serde(default)]
    pub padding_px: Option<f32>,
    #[serde(default)]
    pub duration_ms: Option<f32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    #[serde(default)]
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub links: Vec<GraphLink>,
    #[serde(default)]
    pub focus: Option<Focus>,
}

/// What one snapshot changed, for the caller's bookkeeping (fit-once, refocus,
/// "unknown option"-style warnings).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Diff {
    /// Indices of nodes that did not exist before this snapshot.
    pub added: Vec<usize>,
    /// How many nodes disappeared.
    pub removed: usize,
    /// The focus directive differs from the previous one.
    pub focus_changed: bool,
    /// Links dropped because an endpoint was not in the node list.
    pub dropped_links: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Scene {
    pub bodies: Vec<Body>,
    pub nodes: Vec<Node>,
    pub links: Vec<Link>,
    pub focus: Option<Focus>,
    /// Set once the caller has framed the graph; later snapshots must not
    /// re-frame it (that would move the camera under the user's hands).
    pub fitted: bool,
}

impl Scene {
    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.nodes.iter().position(|n| n.id == id)
    }

    /// Heaviest body in the scene — the reference node radii normalise against.
    pub fn max_weight(&self) -> f32 {
        self.bodies.iter().map(|b| b.weight).fold(1.0, f32::max)
    }

    pub fn weight_of(&self, index: usize) -> f32 {
        self.bodies
            .get(index)
            .map(|b| b.weight)
            .filter(|w| *w > 0.0)
            .unwrap_or(1.0)
    }

    pub fn is_focused(&self, index: usize) -> bool {
        let Some(focus) = self.focus.as_ref() else {
            return false;
        };
        if focus.ids.is_empty() {
            return false;
        }
        self.nodes
            .get(index)
            .is_some_and(|n| focus.ids.iter().any(|id| id == &n.id))
    }

    /// Positions of the focused nodes, for framing the camera on them.
    pub fn focus_points(&self) -> Vec<Vec3> {
        let Some(focus) = self.focus.as_ref() else {
            return Vec::new();
        };
        let wanted: HashSet<&str> = focus.ids.iter().map(String::as_str).collect();
        self.nodes
            .iter()
            .zip(self.bodies.iter())
            .filter(|(n, _)| wanted.contains(n.id.as_str()))
            .map(|(_, b)| b.pos)
            .collect()
    }

    pub fn apply(&mut self, snapshot: Snapshot) -> Diff {
        let carried: HashMap<&str, (Vec3, bool)> = self
            .nodes
            .iter()
            .zip(self.bodies.iter())
            .map(|(n, b)| (n.id.as_str(), (b.pos, b.pinned)))
            .collect();

        let index: HashMap<&str, usize> = snapshot
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.as_str(), i))
            .collect();

        let mut added = Vec::new();
        let mut positions: Vec<Option<Vec3>> = Vec::with_capacity(snapshot.nodes.len());
        for node in &snapshot.nodes {
            match carried.get(node.id.as_str()) {
                Some((pos, _)) => positions.push(Some(*pos)),
                None => {
                    added.push(positions.len());
                    positions.push(None);
                }
            }
        }

        // Neighbour centroids: resolve links first, then place the new nodes
        // next to whatever is already placed. An append thus lands inside the
        // cluster it belongs to instead of at the sphere's edge.
        let mut links = Vec::with_capacity(snapshot.links.len());
        let mut dropped_links = 0;
        let mut neighbours: HashMap<usize, Vec<usize>> = HashMap::new();
        for link in &snapshot.links {
            let (Some(&a), Some(&b)) = (
                index.get(link.source.as_str()),
                index.get(link.target.as_str()),
            ) else {
                dropped_links += 1;
                continue;
            };
            if a == b {
                dropped_links += 1;
                continue;
            }
            neighbours.entry(a).or_default().push(b);
            neighbours.entry(b).or_default().push(a);
            links.push(Link {
                a: a.min(b),
                b: a.max(b),
                strength: link.strength.unwrap_or(1.0).clamp(0.0, 8.0),
            });
        }

        let seeds = force::seed_positions(snapshot.nodes.len(), SEED_RADIUS);
        for i in 0..positions.len() {
            if positions.get(i).copied().flatten().is_some() {
                continue;
            }
            let placed: Vec<Vec3> = neighbours
                .get(&i)
                .map(|ns| {
                    ns.iter()
                        .filter_map(|n| positions.get(*n).copied().flatten())
                        .collect()
                })
                .unwrap_or_default();
            let pos = if placed.is_empty() {
                seeds.get(i).copied().unwrap_or(Vec3::ZERO)
            } else {
                let sum = placed.iter().fold(Vec3::ZERO, |acc, p| acc + *p);
                let centroid = sum * (1.0 / placed.len() as f32);
                // Off the centroid, not on it. A node whose only placed
                // neighbour is its parent has a centroid exactly on that
                // parent, and a whole chain of such nodes lands on one point;
                // the layout then only has collision pushes to separate them,
                // which is how a graph becomes a line.
                let step = SEED_RADIUS * SEED_OFFSET;
                let mut candidate = centroid + force::spiral_dir(i) * step;
                // ...and not on a node that is already there either: the spiral
                // is a deterministic walk, and it revisits. Rotate along it
                // until the point is clear.
                for attempt in 1..SEED_ATTEMPTS {
                    let crowded = positions
                        .iter()
                        .flatten()
                        .any(|p| (*p - candidate).len() < MIN_SEPARATION);
                    if !crowded {
                        break;
                    }
                    candidate = centroid + force::spiral_dir(i * SEED_ATTEMPTS + attempt) * step;
                }
                candidate
            };
            if let Some(slot) = positions.get_mut(i) {
                *slot = Some(pos);
            }
        }

        let nodes = snapshot.nodes;
        let bodies: Vec<Body> = nodes
            .iter()
            .zip(positions)
            .map(|(node, pos)| {
                let pinned = carried.get(node.id.as_str()).is_some_and(|(_, p)| *p);
                Body {
                    pos: pos.unwrap_or(Vec3::ZERO),
                    vel: Vec3::ZERO,
                    weight: node.weight.unwrap_or(1.0).max(0.01),
                    pinned,
                    // Filled by the view from the rendered radius; a scene on
                    // its own knows nothing about pixels.
                    collide: 0.0,
                }
            })
            .collect();

        let removed = self.nodes.len().saturating_sub(nodes.len() - added.len());
        let focus_changed = self.focus != snapshot.focus;
        self.bodies = bodies;
        self.nodes = nodes;
        self.links = links;
        self.focus = snapshot.focus;

        Diff {
            added,
            removed,
            focus_changed,
            dropped_links,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn node(id: &str) -> Node {
        Node {
            id: id.to_string(),
            ..Node::default()
        }
    }

    fn link(source: &str, target: &str) -> GraphLink {
        GraphLink {
            source: source.to_string(),
            target: target.to_string(),
            ..GraphLink::default()
        }
    }

    fn snapshot(ids: &[&str], links: &[(&str, &str)]) -> Snapshot {
        Snapshot {
            nodes: ids.iter().map(|id| node(id)).collect(),
            links: links.iter().map(|(a, b)| link(a, b)).collect(),
            focus: None,
        }
    }

    #[test]
    fn an_append_keeps_existing_positions() {
        let mut scene = Scene::default();
        scene.apply(snapshot(&["a", "b", "c"], &[("a", "b"), ("b", "c")]));
        for (i, body) in scene.bodies.iter_mut().enumerate() {
            body.pos = Vec3::new(i as f32 * 10.0, 0.0, 0.0);
        }
        let before: Vec<Vec3> = scene.bodies.iter().map(|b| b.pos).collect();

        let diff = scene.apply(snapshot(
            &["a", "b", "c", "d"],
            &[("a", "b"), ("b", "c"), ("c", "d")],
        ));

        assert_eq!(diff.added, vec![3]);
        assert_eq!(diff.removed, 0);
        for (i, pos) in before.iter().enumerate() {
            assert_eq!(scene.bodies[i].pos, *pos, "node {i} moved");
        }
        // The new node lands next to its neighbour, not on it and not at the
        // origin: an exact landing point would put every appended node inside
        // its parent.
        let landing = scene.bodies[3].pos;
        let gap = (landing - before[2]).len();
        assert!(
            gap > 1.0 && gap < SEED_RADIUS,
            "new node landed {gap} from its neighbour"
        );
    }

    #[test]
    fn a_removal_reindexes_links() {
        let mut scene = Scene::default();
        scene.apply(snapshot(&["a", "b", "c"], &[("a", "b"), ("b", "c")]));
        let diff = scene.apply(snapshot(&["a", "c"], &[("a", "c")]));
        assert_eq!(diff.removed, 1);
        assert_eq!(scene.bodies.len(), 2);
        assert_eq!(scene.links.len(), 1);
        assert_eq!(scene.links[0].a, 0);
        assert_eq!(scene.links[0].b, 1);
    }

    /// The browser's real path: nodes arrive one at a time, each linked to a
    /// node that is already placed. Before the seed offset, every new node
    /// landed exactly on its parent, so the whole graph collapsed onto one
    /// point and only collision pushes (all along one axis) separated it — a
    /// vertical line instead of a graph.
    #[test]
    fn nodes_added_one_by_one_never_share_a_position() {
        let mut scene = Scene::default();
        let mut nodes: Vec<Node> = Vec::new();
        let mut links: Vec<GraphLink> = Vec::new();
        for i in 0..8 {
            nodes.push(Node {
                id: format!("n{i}"),
                name: Some(format!("n{i}")),
                weight: Some(5.0),
                ..Node::default()
            });
            if i > 0 {
                links.push(GraphLink {
                    source: format!("n{}", i - 1),
                    target: format!("n{i}"),
                    strength: Some(1.0),
                    ..GraphLink::default()
                });
            }
            let diff = scene.apply(Snapshot {
                nodes: nodes.clone(),
                links: links.clone(),
                ..Snapshot::default()
            });
            assert_eq!(diff.added.len(), 1, "one node per push");
        }

        for i in 0..scene.bodies.len() {
            for j in (i + 1)..scene.bodies.len() {
                let gap = (scene.bodies[i].pos - scene.bodies[j].pos).len();
                assert!(gap > 1.0, "nodes {i} and {j} start on one point ({gap})");
            }
        }

        // And it relaxes into three dimensions, not into a line.
        let params = force::Params::default();
        let mut alpha = force::ALPHA_START;
        for _ in 0..400 {
            force::step(&mut scene.bodies, &scene.links, &params, alpha);
            alpha = (alpha * force::ALPHA_DECAY).max(force::ALPHA_FLOOR);
        }
        let span = |f: fn(Vec3) -> f32| {
            let values: Vec<f32> = scene.bodies.iter().map(|b| f(b.pos)).collect();
            values.iter().copied().fold(f32::NEG_INFINITY, f32::max)
                - values.iter().copied().fold(f32::INFINITY, f32::min)
        };
        let (dx, dy, dz) = (span(|p| p.x), span(|p| p.y), span(|p| p.z));
        let widest = dx.max(dy).max(dz);
        assert!(
            dx > widest * 0.15 && dy > widest * 0.15 && dz > widest * 0.15,
            "collapsed to a plane/line: {dx:.1} x {dy:.1} x {dz:.1}"
        );
    }

    #[test]
    fn dangling_links_are_counted_not_panicked_on() {
        let mut scene = Scene::default();
        let diff = scene.apply(snapshot(&["a"], &[("a", "ghost")]));
        assert_eq!(diff.dropped_links, 1);
        assert!(scene.links.is_empty());
    }

    #[test]
    fn the_same_ids_are_not_reported_as_added() {
        let mut scene = Scene::default();
        let first = scene.apply(snapshot(&["a", "b"], &[]));
        assert_eq!(first.added, vec![0, 1]);
        let second = scene.apply(snapshot(&["a", "b"], &[]));
        assert!(second.added.is_empty());
    }

    #[test]
    fn focus_tracks_ids_and_sequence() {
        let mut scene = Scene::default();
        scene.apply(snapshot(&["a", "b"], &[]));
        let mut snap = snapshot(&["a", "b"], &[]);
        snap.focus = Some(Focus {
            ids: vec!["b".to_string()],
            seq: 1,
            ..Focus::default()
        });
        let diff = scene.apply(snap.clone());
        assert!(diff.focus_changed);
        assert!(scene.is_focused(1));
        assert!(!scene.is_focused(0));
        assert_eq!(scene.focus_points(), vec![scene.bodies[1].pos]);

        let quiet = scene.apply(snap);
        assert!(!quiet.focus_changed, "an unchanged focus must not refly");
    }
}