#!/usr/bin/env node
// CyberManju OS — Wiki knowledge-graph aggregator (scripts/wiki-graph.mjs).
// Role: fold graphify's symbol-level graph.json (tree-sitter AST extraction,
//   every edge tagged EXTRACTED/INFERRED) into the subject-level payload the
//   wiki generator emits — cross-subject edges with provenance, god nodes,
//   Leiden communities, and all-pairs module paths for the SPA path tracer.
// Exports: aggregateGraph(graph, subjects, opts) → { engine:'graphify', commit,
//   stats, subjectEdges, subjectGods, subjectCommunity, subjectSymbols,
//   godNodes, communities, paths }; subjectOfFile(file, subjects) (shared
//   matching rule).
// Integrates: scripts/generate-wiki.mjs calls it when graphify-out/graph.json
//   exists (npm run wiki:graph / CI's `graphify extract . --code-only`);
//   without that file the generator keeps the heuristic link miner.
// Contracts: pure function — never touches the filesystem or network; same
//   honest engine label as faces/code-intel: everything here reports
//   engine "graphify", the fallback reports "heuristic"; INFERRED counts
//   stay separate from EXTRACTED, never merged into one confidence.

// File-prefix match, identical to collectSubject in generate-wiki.mjs.
export function subjectOfFile(file, subjects) {
  for (const s of subjects) {
    if (s.paths.some((p) => file === p || file.startsWith(p))) return s.id;
  }
  return null;
}

function locLine(loc) {
  const m = typeof loc === 'string' ? /^L(\d+)/.exec(loc) : null;
  return m ? Number(m[1]) : 0;
}

/**
 * Fold a graphify graph.json into subject-level wiki data.
 * @param {{nodes?: Array, links?: Array, built_at_commit?: string}} graph
 * @param {Array<{id: string, paths: string[]}>} subjects  the SUBJECTS table
 * @param {{godCap?: number, linkCap?: number, godPerSubject?: number}} [opts]
 */
export function aggregateGraph(graph, subjects, opts = {}) {
  const godCap = opts.godCap ?? 40;
  const linkCap = opts.linkCap ?? 5;
  const godPerSubject = opts.godPerSubject ?? 3;
  const nodes = graph.nodes ?? [];
  const links = graph.links ?? [];

  // Node index: label/file/line + subject + community.
  const info = new Map();
  const nodeSub = new Map();
  const nodeComm = new Map();
  for (const n of nodes) {
    const sub = n.source_file ? subjectOfFile(n.source_file, subjects) : null;
    info.set(n.id, {
      label: n.label ?? n.id,
      file: n.source_file ?? '',
      line: locLine(n.source_location),
    });
    if (sub) {
      nodeSub.set(n.id, sub);
      nodeComm.set(n.id, typeof n.community === 'number' ? n.community : -1);
    }
  }

  // Weighted degree over ALL edges (hubs inside subjects count every
  // connection they sit on, not just the cross-subject ones).
  const degree = new Map();
  // Cross-subject buckets: one acc per unordered subject pair.
  const buckets = new Map();
  let extracted = 0;
  let inferred = 0;
  let mapped = 0;
  for (const e of links) {
    const w = e.weight ?? 1;
    degree.set(e.source, (degree.get(e.source) ?? 0) + w);
    degree.set(e.target, (degree.get(e.target) ?? 0) + w);
    const conf = e.confidence === 'INFERRED' ? 'inferred' : 'extracted';
    if (conf === 'inferred') inferred++;
    else extracted++;
    const a = nodeSub.get(e.source);
    const b = nodeSub.get(e.target);
    if (!a || !b || a === b) continue;
    mapped++;
    const key = a < b ? `${a}|${b}` : `${b}|${a}`;
    let acc = buckets.get(key);
    if (!acc) {
      acc = {
        a: a < b ? a : b,
        b: a < b ? b : a,
        weight: 0,
        extracted: 0,
        inferred: 0,
        relations: {},
        samples: [],
      };
      buckets.set(key, acc);
    }
    acc.weight += w;
    acc[conf]++;
    const rel = e.relation ?? 'references';
    acc.relations[rel] = (acc.relations[rel] ?? 0) + 1;
    if (acc.samples.length < 2) {
      const s = info.get(e.source);
      const t = info.get(e.target);
      acc.samples.push(`${rel} ${s.label} → ${t.label} (${e.source_file}:${locLine(e.source_location)})`);
    }
  }

  // Symmetric per-subject edge lists, strongest first, capped.
  const subjectEdges = new Map(subjects.map((s) => [s.id, []]));
  const adjacency = new Map(subjects.map((s) => [s.id, new Set()]));
  for (const acc of buckets.values()) {
    const rels = Object.fromEntries(
      Object.entries(acc.relations)
        .sort((x, y) => y[1] - x[1])
        .slice(0, 3),
    );
    const edge = {
      weight: acc.weight,
      extracted: acc.extracted,
      inferred: acc.inferred,
      relations: rels,
      sample: acc.samples[0] ?? '',
    };
    subjectEdges.get(acc.a).push({ to: acc.b, ...edge });
    subjectEdges.get(acc.b).push({ to: acc.a, ...edge });
    adjacency.get(acc.a).add(acc.b);
    adjacency.get(acc.b).add(acc.a);
  }
  for (const list of subjectEdges.values()) {
    list.sort((x, y) => y.weight - x.weight);
    if (list.length > linkCap) list.length = linkCap;
  }

  // Symbol-level connections (real repo edges): every graph edge touching a
  // node inside a subject becomes a neighbor entry — direction kept via
  // via: 'out'|'in', EXTRACTED/INFERRED provenance never merged.
  const rawNeighbors = new Map(); // node id → neighbor acc list
  const addNeighbor = (from, to, rel, conf, w, via) => {
    if (!rawNeighbors.has(from)) rawNeighbors.set(from, []);
    rawNeighbors.get(from).push({ to, relation: rel, confidence: conf, weight: w, via });
  };
  for (const e of links) {
    const conf = e.confidence === 'INFERRED' ? 'INFERRED' : 'EXTRACTED';
    const rel = e.relation ?? 'references';
    addNeighbor(e.source, e.target, rel, conf, e.weight ?? 1, 'out');
    addNeighbor(e.target, e.source, rel, conf, e.weight ?? 1, 'in');
  }

  // Per-subject symbol nodes (strongest hubs first), each carrying up to
  // `symbolNeighborCap` neighbors resolved to name/file/line.
  const symbolNeighborCap = opts.symbolNeighborCap ?? 6;
  const symbolPerSubject = opts.symbolPerSubject ?? 36;
  const subjectSymbols = new Map(subjects.map((s) => [s.id, []]));
  for (const [id, sub] of nodeSub) {
    const n = info.get(id);
    const deg = degree.get(id) ?? 0;
    const neighbors = (rawNeighbors.get(id) ?? [])
      .sort((x, y) => y.weight - x.weight)
      .slice(0, symbolNeighborCap)
      .map((nb) => {
        const t = info.get(nb.to);
        return {
          name: t?.label ?? nb.to,
          file: t?.file ?? '',
          line: t?.line ?? 0,
          relation: nb.relation,
          confidence: nb.confidence,
          weight: nb.weight,
          via: nb.via,
        };
      });
    subjectSymbols.get(sub).push({
      id,
      name: n.label,
      file: n.file,
      line: n.line,
      degree: deg,
      community: nodeComm.get(id) ?? -1,
      neighbors,
    });
  }
  for (const list of subjectSymbols.values()) {
    list.sort((x, y) => y.degree - x.degree);
    if (list.length > symbolPerSubject) list.length = symbolPerSubject;
  }

  // God nodes: highest weighted-degree nodes that live inside a subject.
  // Per-subject picks come from the FULL ranking (a global cap would starve
  // every subject outside the top N); the global list is capped for payload.
  const ranked = [...degree.entries()]
    .filter(([id]) => nodeSub.has(id))
    .sort((x, y) => y[1] - x[1])
    .map(([id, deg]) => {
      const n = info.get(id);
      return {
        name: n.label,
        file: n.file,
        line: n.line,
        subject: nodeSub.get(id),
        community: nodeComm.get(id) ?? -1,
        degree: deg,
      };
    });
  const subjectGods = new Map(subjects.map((s) => [s.id, []]));
  for (const g of ranked) subjectGods.get(g.subject)?.push(g);
  for (const list of subjectGods.values()) {
    if (list.length > godPerSubject) list.length = godPerSubject;
  }
  const godNodes = ranked.slice(0, godCap);

  // Dominant community per subject (mode over its nodes' Leiden ids).
  const subjectCommunity = new Map();
  const commNodes = new Map(); // community id → node count (whole graph)
  for (const n of nodes) {
    if (typeof n.community === 'number') commNodes.set(n.community, (commNodes.get(n.community) ?? 0) + 1);
  }
  for (const s of subjects) {
    const counts = new Map();
    for (const [id, sub] of nodeSub) {
      if (sub !== s.id) continue;
      const c = nodeComm.get(id) ?? -1;
      counts.set(c, (counts.get(c) ?? 0) + 1);
    }
    let best = -1;
    let bestN = 0;
    for (const [c, n] of counts) {
      if (n > bestN) {
        best = c;
        bestN = n;
      }
    }
    if (best !== -1) subjectCommunity.set(s.id, { id: best, nodes: bestN });
  }
  // Communities as seen from the subjects (grouping signal for the SPA).
  const byComm = new Map();
  for (const [sid, c] of subjectCommunity) {
    if (!byComm.has(c.id)) byComm.set(c.id, { id: c.id, size: commNodes.get(c.id) ?? 0, subjects: [] });
    byComm.get(c.id).subjects.push(sid);
  }
  const communities = [...byComm.values()].sort((x, y) => y.size - x.size);

  // All-pairs shortest module paths (undirected BFS; 15 subjects → instant).
  const paths = {};
  for (const s of subjects) {
    const prev = new Map();
    const queue = [s.id];
    prev.set(s.id, null);
    while (queue.length) {
      const cur = queue.shift();
      for (const nxt of adjacency.get(cur) ?? []) {
        if (!prev.has(nxt)) {
          prev.set(nxt, cur);
          queue.push(nxt);
        }
      }
    }
    for (const t of subjects) {
      if (t.id === s.id || !prev.has(t.id)) continue;
      const chain = [t.id];
      let cur = t.id;
      while (prev.get(cur) !== null) {
        cur = prev.get(cur);
        chain.unshift(cur);
      }
      paths[`${s.id}>${t.id}`] = chain;
    }
  }

  return {
    engine: 'graphify',
    commit: graph.built_at_commit ?? null,
    stats: {
      nodes: nodes.length,
      edges: links.length,
      mapped,
      extracted,
      inferred,
      communities: commNodes.size,
      subjects: subjects.length,
    },
    subjectEdges,
    subjectGods,
    subjectCommunity,
    subjectSymbols,
    godNodes,
    communities,
    paths,
  };
}
