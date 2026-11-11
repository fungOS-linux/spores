//! The brain: the workflow graph laid out the way the dashboard's d3 view lays
//! it out — one station per domain around an ellipse, workflows pulled to their
//! own domain, capabilities clustered in the core, edges drawn.
//!
//! This is a port, not a second design. `src/ui.html` in the Elmers repo settles
//! the same forces with d3 every frame; here they are settled once, in a fixed
//! order, and painted with paths instead of divs — so the picture is the same
//! picture every time it is opened, and it needs no browser.
//!
//! The layout is computed in its own coordinate space and scaled to fit the
//! window, so resizing magnifies the picture instead of rearranging it.

use std::collections::BTreeMap;

use gpui::{
    App, Bounds, Hsla, PathBuilder, Pixels, Point, SharedString, TextAlign, TextRun, Window, font,
    hsla, point, px, quad, rgb, size, transparent_black,
};

use crate::view::Workflow;

/// Layout space. Everything below is in these units; `paint` scales to fit.
pub const SPACE: (f32, f32) = (1280.0, 900.0);

/// d3's defaults, which the dashboard leans on: 300 ticks down to alpha 0.001,
/// with each tick's velocity multiplied by 0.4 before it moves the node.
const TICKS: usize = 300;
const VELOCITY_DECAY: f32 = 0.4;
const ALPHA_MIN: f32 = 0.001;
const ALPHA_DECAY: f32 = 0.022_76;
/// Repulsion is local on purpose: unbounded, every node pushes every other and
/// the graph inflates past any window that still shows nodes apart.
const CHARGE_REACH: f32 = 200.0;
const LINK_DISTANCE: f32 = 24.0;
const COLLIDE_PAD: f32 = 3.5;
const STATION_PULL: f32 = 0.34;
const CORE_PULL: f32 = 0.13;

const EDGE: u32 = 0x6f8194;
const CAPABILITY: u32 = 0x55606d;
const PICKED: u32 = 0xf0a26b;

/// Station colours, in ring order — muted on purpose, because the picture's job
/// is to show grouping and twenty saturated blobs read as confetti. Hue is a
/// fraction here: `hsla` clamps, so degrees would all collapse to red.
const HUES: [f32; 12] = [
    140.0 / 360.0,
    208.0 / 360.0,
    45.0 / 360.0,
    12.0 / 360.0,
    250.0 / 360.0,
    175.0 / 360.0,
    322.0 / 360.0,
    78.0 / 360.0,
    222.0 / 360.0,
    28.0 / 360.0,
    122.0 / 360.0,
    272.0 / 360.0,
];

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Workflow,
    Capability,
}

struct Node {
    name: String,
    kind: Kind,
    /// Index into `stations`; capabilities have none.
    station: usize,
    radius: f32,
    home: (f32, f32),
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    gated: bool,
    owner: String,
    steps: usize,
    spend: String,
    calls: Vec<String>,
}

/// Where the picture is looking. The layout is settled in `SPACE` and fitted to
/// the canvas; zoom magnifies that fit and pan shifts it, so a camera change
/// never relayouts anything — it moves the window onto the same picture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub zoom: f32,
    pub pan: (f32, f32),
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan: (0.0, 0.0),
        }
    }
}

impl Camera {
    /// Below this the graph is smaller than a postcard; above it, a node fills
    /// the window and there is nothing left to look at.
    pub const MIN: f32 = 0.4;
    pub const MAX: f32 = 12.0;

    /// Window origin and scale for a canvas: fit, then zoom about the centre,
    /// then pan. `paint` and `hit` both come through here, which is the only
    /// reason they agree about where a node is.
    pub fn frame(&self, bounds: Bounds<Pixels>) -> (Point<Pixels>, f32) {
        let (width, height) = SPACE;
        let fit: f32 =
            f32::from(bounds.size.width / width).min(f32::from(bounds.size.height / height));
        let scale = fit * self.zoom;
        let centre = point(
            bounds.origin.x + bounds.size.width / 2.0,
            bounds.origin.y + bounds.size.height / 2.0,
        );
        let origin = point(
            centre.x - px(width / 2.0) * scale + px(self.pan.0),
            centre.y - px(height / 2.0) * scale + px(self.pan.1),
        );
        (origin, scale)
    }

    /// Zoom by `factor`, keeping whatever is under `at` under `at` — the
    /// gesture people expect from a map, and the only one that lets a cluster
    /// grow without sliding out of view.
    pub fn zoomed(self, factor: f32, at: Point<Pixels>, bounds: Bounds<Pixels>) -> Self {
        let zoom = (self.zoom * factor).clamp(Self::MIN, Self::MAX);
        if zoom == self.zoom {
            return self;
        }
        let (origin, scale) = self.frame(bounds);
        let world = point(
            px(f32::from(at.x - origin.x) / scale),
            px(f32::from(at.y - origin.y) / scale),
        );
        let (width, height) = SPACE;
        let fit: f32 =
            f32::from(bounds.size.width / width).min(f32::from(bounds.size.height / height));
        let next = fit * zoom;
        let centre = point(
            bounds.origin.x + bounds.size.width / 2.0,
            bounds.origin.y + bounds.size.height / 2.0,
        );
        Self {
            zoom,
            pan: (
                f32::from(at.x - world.x * next - centre.x + px(width / 2.0) * next),
                f32::from(at.y - world.y * next - centre.y + px(height / 2.0) * next),
            ),
        }
    }
}

/// What the panel next to the graph shows for one node. Assembled here because
/// the labels, the counts and the links all live in this module.
pub struct Detail {
    pub name: String,
    pub kind: &'static str,
    /// The station a workflow rides, as `name · count`.
    pub station: Option<String>,
    pub rows: Vec<(&'static str, String)>,
    /// Workflows that call this capability; empty for a workflow.
    pub callers: Vec<String>,
}

struct Link {
    from: usize,
    to: usize,
    distance: f32,
    strength: f32,
}

pub struct Station {
    pub name: String,
    pub count: usize,
    color: Hsla,
    x: f32,
    y: f32,
}

pub struct Brain {
    nodes: Vec<Node>,
    links: Vec<Link>,
    stations: Vec<Station>,
}

/// The folder a workflow lives in, with `lib/`, `tests/` and `examples/`
/// grouped by the segment beneath them: an outreach helper files under
/// `outreach`, beside the outreach workflows that call it. A two-segment
/// `lib/thing` has no segment underneath and keeps its folder.
fn domain_of(name: &str) -> String {
    let parts: Vec<&str> = name.split('/').collect();
    if parts.len() > 2 && matches!(parts[0], "lib" | "tests" | "examples") {
        parts[1].to_string()
    } else {
        parts
            .first()
            .copied()
            .filter(|part| !part.is_empty())
            .unwrap_or("·")
            .to_string()
    }
}

/// The owner a domain is named after: the one holding most of its workflows,
/// and on a tie the alphabetically first, so the answer never depends on
/// iteration luck.
fn chief(owners: &BTreeMap<String, usize>) -> (String, usize) {
    let mut best: Option<(&String, usize)> = None;
    for (name, count) in owners {
        let better = match best {
            None => true,
            Some((best_name, best_count)) => {
                *count > best_count || (*count == best_count && name < best_name)
            }
        };
        if better {
            best = Some((name, *count));
        }
    }
    best.map(|(name, count)| (name.clone(), count))
        .unwrap_or_default()
}

impl Brain {
    /// Build and settle the layout. Deterministic: input order seeds the nodes
    /// and the forces are integrated a fixed number of times, so one graph
    /// always produces one picture.
    pub fn build(workflows: &[Workflow]) -> Self {
        // Capabilities are what a workflow's steps call; a hub's fan-in decides
        // how hard its edges pull, so count them before placing anything.
        let mut calls: BTreeMap<String, usize> = BTreeMap::new();
        for workflow in workflows {
            for call in &workflow.calls {
                *calls.entry(call.clone()).or_default() += 1;
            }
        }

        let mut domains: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
        let mut owners: BTreeMap<String, usize> = BTreeMap::new();
        for workflow in workflows {
            *domains
                .entry(domain_of(&workflow.name))
                .or_default()
                .entry(workflow.owner.clone())
                .or_default() += 1;
            *owners.entry(workflow.owner.clone()).or_default() += 1;
        }

        // A one-workflow domain still needs room for a disc, so a slot is
        // sqrt(n) plus a flat share — pure sqrt gives the small domains slots
        // narrower than a single node and piles them into each other.
        let slot = |domain: &BTreeMap<String, usize>| {
            let count: usize = domain.values().sum();
            (count as f32).sqrt() + 0.7
        };

        let mut order: Vec<(String, BTreeMap<String, usize>)> = domains.into_iter().collect();
        // Departments come out as contiguous arcs, so the ring reads as regions
        // rather than confetti. A fully specified key, so it is the same ring
        // every build.
        order.sort_by(|(name_a, domain_a), (name_b, domain_b)| {
            let (chief_a, _) = chief(domain_a);
            let (chief_b, _) = chief(domain_b);
            let weight_a = owners.get(&chief_a).copied().unwrap_or(0);
            let weight_b = owners.get(&chief_b).copied().unwrap_or(0);
            weight_b
                .cmp(&weight_a)
                .then(chief_a.cmp(&chief_b))
                .then_with(|| slot(domain_b).total_cmp(&slot(domain_a)))
                .then(name_a.cmp(name_b))
        });

        let (width, height) = SPACE;
        let (centre_x, centre_y) = (width / 2.0, height / 2.0);
        // An ellipse matched to the space: a circle would be fitted by height
        // and leave a third of the width empty.
        let (radius_x, radius_y) = (width * 0.60, height * 0.62);
        let total: f32 = order.iter().map(|(_, domain)| slot(domain)).sum();

        let mut stations = Vec::with_capacity(order.len());
        let mut station_of: BTreeMap<String, usize> = BTreeMap::new();
        let mut travelled = 0.0;
        for (index, (name, domain)) in order.iter().enumerate() {
            let arc = slot(domain);
            let angle = ((travelled + arc / 2.0) / total) * std::f32::consts::TAU
                - std::f32::consts::FRAC_PI_2;
            travelled += arc;
            // Alternating bands stagger neighbouring discs inwards, so their
            // overlap lands in the gap between rings instead of merging into
            // one annulus. Angular order, and so the arcs, are untouched.
            let band = if index % 2 == 1 { 0.72 } else { 1.0 };
            station_of.insert(name.clone(), index);
            stations.push(Station {
                name: name.clone(),
                count: domain.values().sum(),
                color: hsla(HUES[index % HUES.len()], 0.42, 0.62, 1.0),
                x: centre_x + angle.cos() * radius_x * band,
                y: centre_y + angle.sin() * radius_y * band,
            });
        }

        let mut nodes = Vec::with_capacity(workflows.len() + calls.len());
        let mut workflow_node = Vec::with_capacity(workflows.len());
        for workflow in workflows {
            let station = station_of
                .get(&domain_of(&workflow.name))
                .copied()
                .unwrap_or(0);
            // A node's size is how much there is behind it: steps it runs, and
            // whether it spends model money at all.
            let spend = match workflow.model_spend.as_str() {
                "always" => 9.0,
                "conditional" => 4.0,
                _ => 0.0,
            };
            let radius = 6.0 + 2.1 * (workflow.steps as f32).sqrt() + spend;
            let (home_x, home_y) = stations
                .get(station)
                .map(|station| (station.x, station.y))
                .unwrap_or((centre_x, centre_y));
            workflow_node.push(nodes.len());
            nodes.push(Node {
                name: workflow.name.clone(),
                kind: Kind::Workflow,
                station,
                radius,
                home: (home_x, home_y),
                x: 0.0,
                y: 0.0,
                vx: 0.0,
                vy: 0.0,
                gated: workflow.gated,
                owner: workflow.owner.clone(),
                steps: workflow.steps,
                spend: workflow.model_spend.clone(),
                calls: workflow.calls.clone(),
            });
        }

        // Capabilities ride a small ring in the core rather than a single
        // anchor: at one point they are a pile that collision spends the whole
        // simulation untangling. Name order keeps the ring stable.
        let mut capability_node: BTreeMap<String, usize> = BTreeMap::new();
        let capability_count = calls.len().max(1);
        for (index, (name, fan_in)) in calls.iter().enumerate() {
            let angle = (index as f32 / capability_count as f32) * std::f32::consts::TAU;
            capability_node.insert(name.clone(), nodes.len());
            nodes.push(Node {
                name: name.clone(),
                kind: Kind::Capability,
                station: usize::MAX,
                radius: 5.0 + 2.6 * (*fan_in as f32).sqrt(),
                home: (
                    centre_x + angle.cos() * width * 0.09,
                    centre_y + angle.sin() * height * 0.09,
                ),
                x: 0.0,
                y: 0.0,
                vx: 0.0,
                vy: 0.0,
                gated: false,
                owner: String::new(),
                steps: 0,
                spend: String::new(),
                calls: Vec::new(),
            });
        }

        let mut links = Vec::new();
        for (index, workflow) in workflows.iter().enumerate() {
            let from = workflow_node[index];
            for call in &workflow.calls {
                if let Some(&to) = capability_node.get(call) {
                    // A hub everything touches says nothing about where anything
                    // belongs, so its pull falls off with its fan-in. That one
                    // line is most of the difference between clusters and a
                    // hairball.
                    let fan_in = calls.get(call).copied().unwrap_or(1).max(1);
                    links.push(Link {
                        from,
                        to,
                        distance: LINK_DISTANCE + nodes[from].radius + nodes[to].radius,
                        strength: 0.45 / (fan_in as f32).sqrt(),
                    });
                }
            }
            // A workflow that runs another is an edge too, but a looser one: a
            // hand-off says two things are adjacent, not that they are one.
            for target in &workflow.links {
                if let Some(to) = workflows
                    .iter()
                    .position(|other| &other.name == target)
                    .and_then(|other| workflow_node.get(other))
                {
                    links.push(Link {
                        from,
                        to: *to,
                        distance: LINK_DISTANCE + nodes[from].radius + nodes[*to].radius,
                        strength: 0.2,
                    });
                }
            }
        }

        // Deterministic seeds: a spiral by index, not a random scatter. d3 can
        // afford to start anywhere; a picture people learn cannot.
        let golden = std::f32::consts::PI * (3.0 - 5.0_f32.sqrt());
        for (index, node) in nodes.iter_mut().enumerate() {
            let angle = index as f32 * golden;
            let reach = (index as f32 + 1.0).sqrt() * 9.0;
            node.x = centre_x + angle.cos() * reach;
            node.y = centre_y + angle.sin() * reach;
        }

        let mut brain = Self {
            nodes,
            links,
            stations,
        };
        brain.settle();
        brain
    }

    /// d3 runs this on a timer; here it runs once, because the picture is a
    /// document, not an animation.
    fn settle(&mut self) {
        let mut alpha = 1.0;
        for _ in 0..TICKS {
            alpha += -alpha * ALPHA_DECAY;
            if alpha < ALPHA_MIN {
                break;
            }
            self.springs(alpha);
            self.charge(alpha);
            for _ in 0..2 {
                self.collide();
            }
            for node in &mut self.nodes {
                let pull = match node.kind {
                    Kind::Workflow => STATION_PULL,
                    Kind::Capability => CORE_PULL,
                };
                node.vx += (node.home.0 - node.x) * pull * alpha;
                node.vy += (node.home.1 - node.y) * pull * alpha;
                node.vx *= VELOCITY_DECAY;
                node.vy *= VELOCITY_DECAY;
                node.x += node.vx;
                node.y += node.vy;
            }
        }
    }

    fn springs(&mut self, alpha: f32) {
        for index in 0..self.links.len() {
            let Link {
                from,
                to,
                distance,
                strength,
            } = self.links[index];
            let (dx, dy) = (
                self.nodes[to].x + self.nodes[to].vx - self.nodes[from].x - self.nodes[from].vx,
                self.nodes[to].y + self.nodes[to].vy - self.nodes[from].y - self.nodes[from].vy,
            );
            let length = (dx * dx + dy * dy).sqrt().max(1.0);
            let factor = (length - distance) / length * alpha * strength;
            self.nodes[from].vx += dx * factor;
            self.nodes[from].vy += dy * factor;
            self.nodes[to].vx -= dx * factor;
            self.nodes[to].vy -= dy * factor;
        }
    }

    /// Many-body repulsion, local to `CHARGE_REACH` — d3's `forceManyBody` with
    /// a `distanceMax`, which is what keeps the picture inside a window.
    fn charge(&mut self, alpha: f32) {
        let reach = CHARGE_REACH * CHARGE_REACH;
        for a in 0..self.nodes.len() {
            for b in (a + 1)..self.nodes.len() {
                let strength = -10.0 - 1.9 * (self.nodes[a].radius + self.nodes[b].radius) * 0.5;
                let mut dx = self.nodes[b].x - self.nodes[a].x;
                let mut dy = self.nodes[b].y - self.nodes[a].y;
                let mut squared = dx * dx + dy * dy;
                if squared >= reach {
                    continue;
                }
                if squared < 1.0 {
                    // Coincident nodes must not divide by zero; nudge them.
                    dx = 0.5 + (b % 3) as f32 * 0.5;
                    dy = 0.5 + (a % 3) as f32 * 0.5;
                    squared = dx * dx + dy * dy;
                }
                let weight = strength * alpha / squared;
                self.nodes[a].vx += dx * weight;
                self.nodes[a].vy += dy * weight;
                self.nodes[b].vx -= dx * weight;
                self.nodes[b].vy -= dy * weight;
            }
        }
    }

    /// Solid discs: two nodes may touch, but the picture must not let one hide
    /// another.
    fn collide(&mut self) {
        for a in 0..self.nodes.len() {
            for b in (a + 1)..self.nodes.len() {
                let (dx, dy) = (
                    self.nodes[b].x - self.nodes[a].x,
                    self.nodes[b].y - self.nodes[a].y,
                );
                let wanted = self.nodes[a].radius + self.nodes[b].radius + COLLIDE_PAD;
                let distance = (dx * dx + dy * dy).sqrt().max(0.01);
                if distance >= wanted {
                    continue;
                }
                let shift = (wanted - distance) / distance * 0.5;
                self.nodes[a].x -= dx * shift;
                self.nodes[a].y -= dy * shift;
                self.nodes[b].x += dx * shift;
                self.nodes[b].y += dy * shift;
            }
        }
    }

    /// Workflows and capabilities, for the header's counts.
    pub fn summary(&self) -> (usize, usize) {
        let workflows = self.nodes.len()
            - (self
                .nodes
                .iter()
                .filter(|node| node.kind == Kind::Capability)
                .count());
        (workflows, self.nodes.len() - workflows)
    }

    pub fn station_count(&self) -> usize {
        self.stations.len()
    }

    /// The station a workflow belongs to, for the header.
    pub fn station_of(&self, node: usize) -> Option<&Station> {
        self.nodes
            .get(node)
            .and_then(|node| self.stations.get(node.station))
    }

    /// Nodes are created workflow-first, so the first `workflows` indices are
    /// the graph's own order. Tests use this to walk them; nothing at runtime
    /// needs the mapping back, because a click gives a node directly.
    #[cfg(test)]
    pub fn workflow_at(&self, workflow: usize) -> Option<usize> {
        self.nodes
            .get(workflow)
            .filter(|node| node.kind == Kind::Workflow)
            .map(|_| workflow)
    }

    /// A node's centre in window coordinates — the same transform the painter
    /// and the hit test use, so a test can prove both agree.
    #[cfg(test)]
    pub fn centre_of(&self, node: usize) -> Option<Point<Pixels>> {
        let frame = Camera::default().frame(Bounds {
            origin: point(px(0.0), px(0.0)),
            size: size(px(SPACE.0), px(SPACE.1)),
        });
        self.nodes.get(node).map(|node| {
            point(
                frame.0.x + px(node.x) * frame.1,
                frame.0.y + px(node.y) * frame.1,
            )
        })
    }

    pub fn name_of(&self, node: usize) -> Option<&str> {
        self.nodes.get(node).map(|node| node.name.as_str())
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// The node under a window position, or the nearest one within reach: a six
    /// pixel dot is hard to hit exactly and there is no ambiguity about which
    /// node a pointer just inside one meant.
    pub fn hit(&self, bounds: Bounds<Pixels>, camera: Camera, at: Point<Pixels>) -> Option<usize> {
        let (origin, scale) = camera.frame(bounds);
        let x = f32::from(at.x - origin.x) / scale;
        let y = f32::from(at.y - origin.y) / scale;
        let mut best: Option<(usize, f32)> = None;
        for (index, node) in self.nodes.iter().enumerate() {
            let dx = node.x - x;
            let dy = node.y - y;
            let distance = (dx * dx + dy * dy).sqrt();
            if distance > node.radius + 6.0 {
                continue;
            }
            if best.map(|(_, nearest)| distance < nearest).unwrap_or(true) {
                best = Some((index, distance));
            }
        }
        best.map(|(index, _)| index)
    }

    /// Everything the panel beside the graph says about one node.
    pub fn detail(&self, index: usize) -> Option<Detail> {
        let node = self.nodes.get(index)?;
        let station = self
            .stations
            .get(node.station)
            .map(|station| format!("{} · {}", station.name, station.count));
        let mut rows = Vec::new();
        let mut callers = Vec::new();
        match node.kind {
            Kind::Workflow => {
                rows.push(("owner", node.owner.clone()));
                rows.push(("steps", node.steps.to_string()));
                rows.push(("model spend", node.spend.clone()));
                rows.push((
                    "gate",
                    if node.gated {
                        "waits for a person"
                    } else {
                        "none"
                    }
                    .to_string(),
                ));
                rows.push((
                    "calls",
                    if node.calls.is_empty() {
                        "nothing".to_string()
                    } else {
                        node.calls.join(", ")
                    },
                ));
            }
            Kind::Capability => {
                for link in &self.links {
                    if link.to == index
                        && let Some(from) = self.nodes.get(link.from)
                    {
                        callers.push(from.name.clone());
                    }
                }
                callers.sort();
                rows.push(("called from", format!("{} workflows", callers.len())));
            }
        }
        Some(Detail {
            name: node.name.clone(),
            kind: match node.kind {
                Kind::Workflow => "workflow",
                Kind::Capability => "capability",
            },
            station,
            rows,
            callers,
        })
    }

    /// The canvas that draws the graph. `canvas` hands back the element's bounds
    /// in paint, which is where the fit-to-window scale comes from, so the view
    /// needs no knowledge of its own size.
    /// The canvas that draws the graph.
    pub(crate) fn paint(
        &self,
        bounds: Bounds<Pixels>,
        camera: Camera,
        picked: Option<usize>,
        hover: Option<usize>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let (origin, scale) = camera.frame(bounds);
        let at = |x: f32, y: f32| point(origin.x + px(x) * scale, origin.y + px(y) * scale);

        // Edges first: the graph in one stroke, then the picked node's own
        // links over it in a colour that can be followed.
        let mut all = PathBuilder::stroke(px(1.0));
        let mut mine = PathBuilder::stroke(px(1.6));
        for link in &self.links {
            let (from, to) = (&self.nodes[link.from], &self.nodes[link.to]);
            let start = at(from.x, from.y);
            let end = at(to.x, to.y);
            // A slight bow: straight lines from one hub to forty nodes overlay
            // into a single stroke and say nothing.
            let bow = point(
                (start.x + end.x) / 2.0 + (end.y - start.y) * 0.08,
                (start.y + end.y) / 2.0 - (end.x - start.x) * 0.08,
            );
            all.move_to(start);
            all.curve_to(end, bow);
            if picked == Some(link.from)
                || picked == Some(link.to)
                || hover == Some(link.from)
                || hover == Some(link.to)
            {
                mine.move_to(start);
                mine.curve_to(end, bow);
            }
        }
        if let Ok(path) = all.build() {
            window.paint_path(path, hsla(210.0 / 360.0, 0.25, 0.6, 0.22));
        }
        if let Ok(path) = mine.build() {
            window.paint_path(path, rgb(PICKED));
        }

        for (index, node) in self.nodes.iter().enumerate() {
            let centre = at(node.x, node.y);
            let radius = px(node.radius * scale);
            let selected = picked == Some(index);
            let hot = hover == Some(index);
            if selected || hot {
                // A halo rather than a border: the node keeps its own colour and
                // still reads as picked at a glance. The pointer's node is
                // brighter, so hovering and picking are not the same statement.
                window.paint_quad(quad(
                    Bounds {
                        origin: point(centre.x - radius * 2.0, centre.y - radius * 2.0),
                        size: size(radius * 4.0, radius * 4.0),
                    },
                    radius * 2.0,
                    hsla(28.0 / 360.0, 0.9, 0.6, if hot { 0.34 } else { 0.20 }),
                    px(0.0),
                    transparent_black(),
                    Default::default(),
                ));
            }
            let fill = match node.kind {
                Kind::Workflow => self
                    .stations
                    .get(node.station)
                    .map(|station| station.color)
                    .unwrap_or_else(|| rgb(EDGE).into()),
                Kind::Capability => rgb(CAPABILITY).into(),
            };
            window.paint_quad(quad(
                Bounds {
                    origin: point(centre.x - radius, centre.y - radius),
                    size: size(radius * 2.0, radius * 2.0),
                },
                radius,
                fill,
                px(0.0),
                transparent_black(),
                Default::default(),
            ));
            // A gate is the one piece of state worth seeing without selecting:
            // a workflow that stops and waits for a person.
            if node.gated {
                let mark = px(2.0 * scale);
                window.paint_quad(quad(
                    Bounds {
                        origin: point(centre.x - mark, centre.y - mark),
                        size: size(mark * 2.0, mark * 2.0),
                    },
                    mark,
                    rgb(0xd14b4b),
                    px(0.0),
                    transparent_black(),
                    Default::default(),
                ));
            }
        }

        // Names last, so they sit on top of the graph rather than under it.
        let station_size = 11.0 * scale;
        for station in &self.stations {
            let centre = at(station.x, station.y);
            let above = px(6.0 + 3.2 * (station.count as f32).sqrt()) * scale;
            self.text(
                &format!("{} · {}", station.name, station.count),
                point(centre.x - px(station_size) * 1.2, centre.y - above),
                station_size,
                station.color,
                window,
                cx,
            );
        }
        if let Some(picked) = hover
            .and_then(|node| self.nodes.get(node))
            .or_else(|| picked.and_then(|node| self.nodes.get(node)))
        {
            let centre = at(picked.x, picked.y);
            let size = 12.0 * scale;
            let label = picked.name.clone();
            self.text(
                &label,
                point(
                    centre.x + px(picked.radius * scale) + px(4.0),
                    centre.y - px(size * 0.6),
                ),
                size,
                rgb(0xdbe4ee).into(),
                window,
                cx,
            );
        }
    }

    fn text(
        &self,
        text: &str,
        origin: Point<Pixels>,
        font_size: f32,
        color: Hsla,
        window: &mut Window,
        cx: &mut App,
    ) {
        let run = TextRun {
            len: text.len(),
            font: font("Lilex"),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let line = window.text_system().shape_line(
            SharedString::from(text.to_string()),
            px(font_size),
            &[run],
            None,
        );
        let _ = line.paint(
            origin,
            px(font_size * 1.3),
            TextAlign::Left,
            None,
            window,
            cx,
        );
    }
}
