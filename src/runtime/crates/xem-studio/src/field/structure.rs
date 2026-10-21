//! A conservative, lossless view of one event's instruction flow.
//!
//! Storage blocks are deliberately not control-flow vertices: a selected entry
//! can be inside a compound block. Each instruction owner carries its original
//! outgoing edges, including unresolved edges and coincident execution paths.
//! Containers arrange those owners; they never substitute a second graph for
//! the instructions in the tree. A loop's body owns its header instruction.

use super::{Edge, FieldError, FieldGraph};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FoldCounts {
    pub ifs: usize,
    pub loops: usize,
    pub switches: usize,
    pub gotos: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OwnedInstruction {
    pub pc: usize,
    pub edges: Vec<Edge>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Sequence {
    pub nodes: Vec<StructureNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SwitchCase {
    /// The inline table slot instruction, rather than its jump destination.
    pub entry_pc: usize,
    pub body: Sequence,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StructureNode {
    Instruction {
        instruction: OwnedInstruction,
    },
    If {
        header: OwnedInstruction,
        /// The branch edge (the condition's taken path).
        then_branch: Sequence,
        /// The fallthrough edge.
        else_branch: Sequence,
        join: usize,
    },
    Loop {
        header_pc: usize,
        /// Owns the header exactly once; back edges finish an iteration.
        body: Sequence,
        exit_pc: Option<usize>,
    },
    Switch {
        header: OwnedInstruction,
        cases: Vec<SwitchCase>,
        join: Option<usize>,
    },
    Label {
        pc: usize,
    },
    /// All outgoing records are explicit; this node implies no fallthrough to
    /// the next displayed row. Terminal instructions can have an empty list.
    Goto {
        instruction: OwnedInstruction,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EventStructure {
    pub schema_version: u32,
    pub actor: usize,
    pub event: usize,
    pub entry_pc: Option<usize>,
    pub skipped: bool,
    pub body: Sequence,
    pub counts: FoldCounts,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FlattenedStructure {
    pub instruction_pcs: Vec<usize>,
    pub edges: Vec<Edge>,
    pub counts: FoldCounts,
}

/// Read only instruction owners in the tree. Sorting is stable, so the edge
/// order of each source and duplicate coincident paths remain unchanged.
pub fn flatten(event: &EventStructure) -> FlattenedStructure {
    let mut result = FlattenedStructure::default();
    let mut pending: Vec<&StructureNode> = event.body.nodes.iter().rev().collect();
    while let Some(node) = pending.pop() {
        let instruction = match node {
            StructureNode::Instruction { instruction } => Some(instruction),
            StructureNode::Goto { instruction } => {
                result.counts.gotos += 1;
                Some(instruction)
            }
            StructureNode::If {
                header,
                then_branch,
                else_branch,
                ..
            } => {
                result.counts.ifs += 1;
                pending.extend(else_branch.nodes.iter().rev());
                pending.extend(then_branch.nodes.iter().rev());
                Some(header)
            }
            StructureNode::Loop { body, .. } => {
                result.counts.loops += 1;
                pending.extend(body.nodes.iter().rev());
                None
            }
            StructureNode::Switch { header, cases, .. } => {
                result.counts.switches += 1;
                for case in cases.iter().rev() {
                    pending.extend(case.body.nodes.iter().rev());
                }
                Some(header)
            }
            StructureNode::Label { .. } => None,
        };
        if let Some(instruction) = instruction {
            result.instruction_pcs.push(instruction.pc);
            result.edges.extend(instruction.edges.iter().cloned());
        }
    }
    result.instruction_pcs.sort_unstable();
    result.edges.sort_by_key(|edge| edge.from_pc);
    result
}

type RegionArm = (usize, BTreeSet<usize>);
type BranchArms = (Vec<RegionArm>, Option<usize>);

/// Validate a canonical M2 graph once per map, then index individual PCs. The
/// last entry's tree is cached for the many actor/event aliases without keeping
/// a potentially quadratic collection of overlapping trees alive.
pub struct StructureIndex<'a> {
    graph: &'a FieldGraph,
    by_pc: BTreeMap<usize, usize>,
    edges: Vec<Vec<Edge>>,
    next: Vec<Vec<usize>>,
    previous: Vec<Vec<usize>>,
    entries: Vec<[Option<usize>; 32]>,
    cache: RefCell<Option<(usize, Sequence, FoldCounts)>>,
}

impl<'a> StructureIndex<'a> {
    pub fn new(graph: &'a FieldGraph) -> Result<Self, FieldError> {
        super::encode(graph)?;
        let by_pc: BTreeMap<_, _> = graph
            .instructions
            .iter()
            .enumerate()
            .map(|(index, instruction)| (instruction.pc, index))
            .collect();
        let mut edges = vec![Vec::new(); graph.instructions.len()];
        let mut next = vec![Vec::new(); graph.instructions.len()];
        let mut previous = vec![Vec::new(); graph.instructions.len()];
        let mut entries = vec![[None; 32]; graph.actors.len()];
        for entry in &graph.entries {
            entries[entry.actor][entry.event] = Some(*by_pc.get(&entry.pc).ok_or_else(|| {
                FieldError::new("invalid_graph", "event entry has no decoded instruction")
            })?);
        }
        for edge in &graph.edges {
            let source = by_pc[&edge.from_pc];
            edges[source].push(edge.clone());
            if edge.status == "followed" {
                let target = *by_pc.get(&edge.to_pc).ok_or_else(|| {
                    FieldError::new("invalid_graph", "followed edge has no instruction")
                })?;
                if !next[source].contains(&target) {
                    next[source].push(target);
                    previous[target].push(source);
                }
            }
        }
        Ok(Self {
            graph,
            by_pc,
            edges,
            next,
            previous,
            entries,
            cache: RefCell::new(None),
        })
    }

    fn entry(&self, actor: usize, event: usize) -> Result<Option<usize>, FieldError> {
        let actor_entries = self.graph.actors.get(actor).ok_or_else(|| {
            FieldError::new(
                "invalid_event",
                format!("actor {actor} is outside this map"),
            )
        })?;
        let &raw_pc = actor_entries.get(event).ok_or_else(|| {
            FieldError::new("invalid_event", format!("event {event} is outside 0..32"))
        })?;
        let Some(entry) = self.entries[actor][event] else {
            // encode validated that the sole omitted-entry rule is the ff
            // map-record zero entry. No instruction is invented for that row.
            if raw_pc != 0 {
                return Err(FieldError::new(
                    "invalid_graph",
                    "active event entry is missing",
                ));
            }
            return Ok(None);
        };
        Ok(Some(entry))
    }

    /// Reachable instruction PCs in ascending order, independently of folding.
    pub fn reachable_pcs(&self, actor: usize, event: usize) -> Result<Vec<usize>, FieldError> {
        Ok(match self.entry(actor, event)? {
            Some(root) => self
                .reachable(root, None, None)
                .into_iter()
                .map(|vertex| self.graph.instructions[vertex].pc)
                .collect(),
            None => Vec::new(),
        })
    }

    pub fn event(&self, actor: usize, event: usize) -> Result<EventStructure, FieldError> {
        let Some(root) = self.entry(actor, event)? else {
            return Ok(EventStructure {
                schema_version: 1,
                actor,
                event,
                entry_pc: None,
                skipped: true,
                body: Sequence::default(),
                counts: FoldCounts::default(),
            });
        };
        let entry_pc = self.graph.instructions[root].pc;
        let cached = self.cache.borrow();
        let (body, counts) = if let Some((pc, body, counts)) = cached.as_ref()
            && *pc == entry_pc
        {
            (body.clone(), counts.clone())
        } else {
            drop(cached);
            let members = self.reachable(root, None, None);
            let body = self.region(root, &members, None, 0);
            let shell = EventStructure {
                schema_version: 1,
                actor,
                event,
                entry_pc: Some(entry_pc),
                skipped: false,
                body,
                counts: FoldCounts::default(),
            };
            let counts = flatten(&shell).counts;
            *self.cache.borrow_mut() = Some((entry_pc, shell.body.clone(), counts.clone()));
            (shell.body, counts)
        };
        Ok(EventStructure {
            schema_version: 1,
            actor,
            event,
            entry_pc: Some(entry_pc),
            skipped: false,
            body,
            counts,
        })
    }

    fn owned(&self, vertex: usize) -> OwnedInstruction {
        OwnedInstruction {
            pc: self.graph.instructions[vertex].pc,
            edges: self.edges[vertex].clone(),
        }
    }

    fn successors<'b>(
        &'b self,
        vertex: usize,
        members: &'b BTreeSet<usize>,
        cut: Option<usize>,
    ) -> impl Iterator<Item = usize> + 'b {
        self.next[vertex]
            .iter()
            .copied()
            .filter(move |target| Some(*target) != cut && members.contains(target))
    }

    fn reachable(
        &self,
        root: usize,
        members: Option<&BTreeSet<usize>>,
        stop: Option<usize>,
    ) -> BTreeSet<usize> {
        let mut found = BTreeSet::new();
        let mut pending = vec![root];
        while let Some(vertex) = pending.pop() {
            if Some(vertex) == stop
                || members.is_some_and(|members| !members.contains(&vertex))
                || !found.insert(vertex)
            {
                continue;
            }
            pending.extend(self.next[vertex].iter().copied());
        }
        found
    }

    fn explicit(&self, root: usize, members: &BTreeSet<usize>) -> Sequence {
        let mut pending = vec![root];
        let mut seen = BTreeSet::new();
        let mut nodes = Vec::new();
        while let Some(vertex) = pending.pop() {
            if !members.contains(&vertex) || !seen.insert(vertex) {
                continue;
            }
            nodes.push(StructureNode::Label {
                pc: self.graph.instructions[vertex].pc,
            });
            nodes.push(StructureNode::Goto {
                instruction: self.owned(vertex),
            });
            pending.extend(self.next[vertex].iter().rev().copied());
        }
        // A region can contain a disconnected remainder after a previously
        // folded arm. Such rows remain explicitly labelled, with no implied
        // ordering or fallthrough between them.
        for &vertex in members.difference(&seen) {
            nodes.push(StructureNode::Label {
                pc: self.graph.instructions[vertex].pc,
            });
            nodes.push(StructureNode::Goto {
                instruction: self.owned(vertex),
            });
        }
        Sequence { nodes }
    }

    fn region(
        &self,
        entry: usize,
        members: &BTreeSet<usize>,
        cut: Option<usize>,
        depth: usize,
    ) -> Sequence {
        if members.is_empty() {
            return Sequence::default();
        }
        // Bound both construction and derived serde/Clone/Drop recursion.
        // Large straight-line scripts still use a flat sequence.
        if depth >= 24 {
            return self.explicit(entry, members);
        }
        let analysis = Analysis::new(self, entry, members, cut);
        let mut remaining = members.clone();
        let mut current = entry;
        let mut nodes = Vec::new();
        while remaining.contains(&current) {
            let component = analysis.component[&current];
            if analysis.cyclic[component] {
                if let Some((header, exit)) =
                    self.loop_region(&analysis, component, entry, members, cut)
                    && header == current
                {
                    let loop_members = &analysis.groups[component];
                    nodes.push(StructureNode::Loop {
                        header_pc: self.graph.instructions[header].pc,
                        body: self.region(header, loop_members, Some(header), depth + 1),
                        exit_pc: exit.map(|vertex| self.graph.instructions[vertex].pc),
                    });
                    for vertex in loop_members {
                        remaining.remove(vertex);
                    }
                    if let Some(exit) = exit
                        && remaining.contains(&exit)
                    {
                        current = exit;
                        continue;
                    }
                    break;
                }
                nodes.extend(self.explicit(current, &remaining).nodes);
                remaining.clear();
                break;
            }
            let instruction = &self.graph.instructions[current];
            let is_if = instruction.spec.flow == "branch";
            let is_switch = instruction.spec.flow == "table"
                && self
                    .graph
                    .diagnostics
                    .tables
                    .iter()
                    .any(|record| record[0] == instruction.pc);
            if (is_if || is_switch)
                && let Some((arms, join)) =
                    self.arms(current, &analysis, &remaining, cut, is_switch)
            {
                let header = self.owned(current);
                remaining.remove(&current);
                let mut bodies = Vec::new();
                for (start, arm) in arms {
                    let body = self.region(start, &arm, cut, depth + 1);
                    for vertex in &arm {
                        remaining.remove(vertex);
                    }
                    bodies.push((start, body));
                }
                if is_switch {
                    nodes.push(StructureNode::Switch {
                        header,
                        cases: bodies
                            .into_iter()
                            .map(|(start, body)| SwitchCase {
                                entry_pc: self.graph.instructions[start].pc,
                                body,
                            })
                            .collect(),
                        join: join.map(|vertex| self.graph.instructions[vertex].pc),
                    });
                } else {
                    // arms() returns branch then fallthrough, including an
                    // empty arm when one path goes straight to the join.
                    let mut bodies = bodies.into_iter();
                    let then_branch = bodies.next().expect("two proven branch arms").1;
                    let else_branch = bodies.next().expect("two proven branch arms").1;
                    nodes.push(StructureNode::If {
                        header,
                        then_branch,
                        else_branch,
                        join: self.graph.instructions[join.expect("if has a join")].pc,
                    });
                }
                if let Some(join) = join
                    && remaining.contains(&join)
                {
                    current = join;
                    continue;
                }
                break;
            }
            let successors: Vec<_> = self.successors(current, &remaining, cut).collect();
            let clean = self.edges[current]
                .iter()
                .all(|edge| edge.status == "followed");
            if successors.len() > 1 || !clean {
                nodes.extend(self.explicit(current, &remaining).nodes);
                remaining.clear();
                break;
            }
            let owner = self.owned(current);
            let natural = instruction.spec.flow == "next"
                || instruction.spec.flow == "end"
                || instruction.spec.flow == "return"
                || self.edges[current].is_empty();
            nodes.push(if natural {
                StructureNode::Instruction { instruction: owner }
            } else {
                StructureNode::Goto { instruction: owner }
            });
            remaining.remove(&current);
            let Some(&next) = successors.first() else {
                break;
            };
            if self.previous[next].iter().any(|predecessor| {
                remaining.contains(predecessor)
                    && analysis.component[predecessor] != analysis.component[&next]
            }) {
                nodes.extend(self.explicit(next, &remaining).nodes);
                remaining.clear();
                break;
            }
            current = next;
        }
        if let Some(&root) = remaining.first() {
            nodes.extend(self.explicit(root, &remaining).nodes);
        }
        Sequence { nodes }
    }

    fn loop_region(
        &self,
        analysis: &Analysis,
        component: usize,
        entry: usize,
        members: &BTreeSet<usize>,
        cut: Option<usize>,
    ) -> Option<(usize, Option<usize>)> {
        let group = &analysis.groups[component];
        let mut entries = BTreeSet::new();
        let mut exits = BTreeSet::new();
        if group.contains(&entry) {
            entries.insert(entry);
        }
        for &vertex in group {
            if self.edges[vertex]
                .iter()
                .any(|edge| edge.status != "followed")
            {
                return None;
            }
            if Some(vertex) != cut
                && self.previous[vertex]
                    .iter()
                    .any(|source| members.contains(source) && !group.contains(source))
            {
                entries.insert(vertex);
            }
            for &target in &self.next[vertex] {
                if !group.contains(&target) {
                    exits.insert(target);
                }
            }
        }
        if entries.len() != 1 || exits.len() > 1 {
            return None;
        }
        Some((*entries.first()?, exits.first().copied()))
    }

    fn arms(
        &self,
        header: usize,
        analysis: &Analysis,
        members: &BTreeSet<usize>,
        cut: Option<usize>,
        switch: bool,
    ) -> Option<BranchArms> {
        let edges = &self.edges[header];
        if edges.iter().any(|edge| edge.status != "followed") {
            return None;
        }
        let starts: Vec<_> = if switch {
            if edges.is_empty() || edges.iter().any(|edge| edge.kind != "table_entry") {
                return None;
            }
            edges.iter().map(|edge| self.by_pc[&edge.to_pc]).collect()
        } else {
            if edges.len() != 2 {
                return None;
            }
            let branch = edges.iter().find(|edge| edge.kind == "branch")?;
            let otherwise = edges.iter().find(|edge| edge.kind == "fallthrough")?;
            vec![self.by_pc[&branch.to_pc], self.by_pc[&otherwise.to_pc]]
        };
        let component = analysis.component[&header];
        let join = analysis.postdom[component].and_then(|component| analysis.headers[component]);
        if (!switch && join.is_none()) || join == Some(header) {
            return None;
        }
        let mut all = BTreeSet::new();
        let mut arms = Vec::new();
        for start in starts {
            if Some(start) == cut || (!members.contains(&start) && Some(start) != join) {
                return None;
            }
            let mut arm = BTreeSet::new();
            let mut pending = vec![start];
            while let Some(vertex) = pending.pop() {
                if Some(vertex) == join
                    || Some(vertex) == cut
                    || !members.contains(&vertex)
                    || !arm.insert(vertex)
                {
                    continue;
                }
                pending.extend(self.next[vertex].iter().copied());
            }
            if arm.contains(&header) || !arm.is_disjoint(&all) {
                return None;
            }
            for &vertex in &arm {
                if self.previous[vertex].iter().any(|source| {
                    members.contains(source)
                        && !arm.contains(source)
                        && !(*source == header && vertex == start)
                }) {
                    return None;
                }
                // No path can escape an arm except through its single join
                // (or the virtual exit when a switch has terminal cases).
                if self.next[vertex]
                    .iter()
                    .any(|target| !arm.contains(target) && Some(*target) != join)
                    || self.edges[vertex]
                        .iter()
                        .any(|edge| edge.status != "followed")
                {
                    return None;
                }
                if join.is_some() && self.next[vertex].is_empty() {
                    return None;
                }
            }
            all.extend(&arm);
            arms.push((start, arm));
        }
        Some((arms, join))
    }
}

pub fn structure(
    graph: &FieldGraph,
    actor: usize,
    event: usize,
) -> Result<EventStructure, FieldError> {
    StructureIndex::new(graph)?.event(actor, event)
}

/// SCC condensation and immediate postdominators of its DAG. SCCs identify
/// actual cycles; only a single-entry, single-exit SCC is eligible as a loop.
/// Nonterminating SCCs have a virtual exit for analysis, never an invented edge.
struct Analysis {
    component: BTreeMap<usize, usize>,
    groups: Vec<BTreeSet<usize>>,
    cyclic: Vec<bool>,
    headers: Vec<Option<usize>>,
    postdom: Vec<Option<usize>>,
}

impl Analysis {
    fn new(
        index: &StructureIndex<'_>,
        entry: usize,
        members: &BTreeSet<usize>,
        cut: Option<usize>,
    ) -> Self {
        let mut seen = BTreeSet::new();
        let mut order = Vec::new();
        // Iterative DFS: even a 65,536-instruction path uses bounded call stack.
        for &root in members {
            if !seen.insert(root) {
                continue;
            }
            let mut stack = vec![(root, 0)];
            while let Some((vertex, position)) = stack.last_mut() {
                if let Some(&target) = index.next[*vertex].get(*position) {
                    *position += 1;
                    if Some(target) != cut && members.contains(&target) && seen.insert(target) {
                        stack.push((target, 0));
                    }
                } else {
                    order.push(*vertex);
                    stack.pop();
                }
            }
        }
        let mut component = BTreeMap::new();
        let mut groups = Vec::new();
        for &root in order.iter().rev() {
            if component.contains_key(&root) {
                continue;
            }
            let id = groups.len();
            let mut group = BTreeSet::new();
            let mut pending = vec![root];
            while let Some(vertex) = pending.pop() {
                if component.contains_key(&vertex) {
                    continue;
                }
                component.insert(vertex, id);
                group.insert(vertex);
                if Some(vertex) != cut {
                    pending.extend(
                        index.previous[vertex]
                            .iter()
                            .copied()
                            .filter(|source| members.contains(source)),
                    );
                }
            }
            groups.push(group);
        }
        let mut headers = Vec::new();
        let mut cyclic = Vec::new();
        let mut successors = vec![BTreeSet::new(); groups.len()];
        for (id, group) in groups.iter().enumerate() {
            let mut incoming = BTreeSet::new();
            if group.contains(&entry) {
                incoming.insert(entry);
            }
            let mut cycle = group.len() > 1;
            for &vertex in group {
                if Some(vertex) != cut
                    && index.previous[vertex]
                        .iter()
                        .any(|source| members.contains(source) && !group.contains(source))
                {
                    incoming.insert(vertex);
                }
                for &target in &index.next[vertex] {
                    if Some(target) == cut || !members.contains(&target) {
                        successors[id].insert(groups.len());
                    } else if component[&target] != id {
                        successors[id].insert(component[&target]);
                    } else if target == vertex && Some(target) != cut {
                        cycle = true;
                    }
                }
                if index.next[vertex].is_empty() {
                    successors[id].insert(groups.len());
                }
            }
            if successors[id].is_empty() {
                successors[id].insert(groups.len());
            }
            cyclic.push(cycle);
            headers.push(if incoming.len() == 1 {
                incoming.first().copied()
            } else {
                None
            });
        }
        // Kosaraju discovers the condensation in topological order, so all
        // successors' postdominators are available when visited in reverse.
        let virtual_exit = groups.len();
        let mut parents = vec![virtual_exit; groups.len() + 1];
        for id in (0..groups.len()).rev() {
            let mut targets = successors[id].iter().copied();
            let mut parent = targets.next().expect("component has exit or successor");
            for mut target in targets {
                while parent != target {
                    if parent < target {
                        parent = parents[parent];
                    } else {
                        target = parents[target];
                    }
                }
            }
            parents[id] = parent;
        }
        let postdom = parents[..groups.len()]
            .iter()
            .map(|&parent| (parent != virtual_exit).then_some(parent))
            .collect();
        Self {
            component,
            groups,
            cyclic,
            headers,
            postdom,
        }
    }
}
