//! Machine-neutral control-flow graph.
//!
//! A front-end builds one [`Cfg`] per method body: basic blocks with machine
//! offsets (`u32` — JVM bytecode pcs, DEX code units, ...), successor and
//! predecessor edges, handler edges, and the exception ranges that connect
//! them. Nothing here knows how instructions are encoded; the front-end keeps
//! its own instruction storage alongside (it needs it to build expressions).
//!
//! Conventions the structurer relies on (see `docs/CONTRACT.md`):
//! * `succ[0]` is the fall-through successor of a conditional terminator,
//!   `succ[1]` the taken one.
//! * `BlockResult`s are indexed by block id.
//! * `exc_ranges[i]` pairs with the exception edges carrying `range == i`.

#[derive(Debug, Clone)]
pub struct Block {
    pub id: usize,
    /// First machine offset covered by this block (inclusive).
    pub start: u32,
    /// End offset (exclusive).
    pub end: u32,
    /// Number of machine instructions (or code units) in this block. `0`
    /// marks a block with no code of its own — a structural stub the CFG
    /// split produced (handler entries, empty ranges).
    pub ins_len: u32,
    /// Successor block ids. For a conditional branch: `[fallthrough, taken]`
    /// (the structurer may reorder branches based on condition polarity).
    pub succ: Vec<usize>,
    pub pred: Vec<usize>,
    /// Exception-range indices this block is the HANDLER of (index into
    /// `Cfg::exc_ranges`) — the structurer's try reconstruction reads those,
    /// not source blocks.
    pub handlers: Vec<u32>,
}

impl Block {
    /// True if the block ends with a conditional branch (two successors).
    pub fn ends_cond(&self) -> bool {
        matches!(self.succ.len(), 2)
    }

    /// True if the block cannot fall through anywhere (a return/throw/
    /// goto-only ending as far as the graph is concerned).
    pub fn is_exit(&self) -> bool {
        self.succ.is_empty()
    }
}

/// One exception-table entry: `[start, end)` is protected, `handler` catches.
#[derive(Debug, Clone)]
pub struct ExcRange {
    pub start: u32,
    pub end: u32,
    pub handler: u32,
    /// `None` = catch-all (the source form is usually `finally`).
    pub catch_type: Option<String>,
}

/// An edge from a protected region to its handler.
#[derive(Debug, Clone)]
pub struct ExcEdge {
    /// Index into [`Cfg::exc_ranges`].
    pub range: usize,
    pub from: usize,
    pub to: usize,
}

pub struct Cfg {
    pub blocks: Vec<Block>,
    pub entry: usize,
    pub exc_edges: Vec<ExcEdge>,
    pub exc_ranges: Vec<ExcRange>,
    /// Sorted block start offsets, for `block_at`.
    starts: Vec<u32>,
}

impl Cfg {
    /// Assemble a CFG from front-end-built blocks. Predecessor lists, handler
    /// lists and exception edges are derived here so a front-end only has to
    /// state successors and exception ranges.
    pub fn from_blocks(blocks: Vec<Block>, entry: usize, exc_ranges: Vec<ExcRange>) -> Cfg {
        let mut blocks = blocks;
        // Predecessors.
        for b in blocks.iter_mut() {
            b.pred.clear();
            b.handlers.clear();
        }
        for i in 0..blocks.len() {
            let succ = blocks[i].succ.clone();
            for s in succ {
                if s < blocks.len() {
                    blocks[s].pred.push(i);
                }
            }
        }
        // Handler back-references, with jcdc's exact semantics:
        //  * `Block.handlers` lists the exception-RANGE indices this block is
        //    the handler of (the structurer's try reconstruction reads range
        //    indices, not source blocks);
        //  * a block is protected by a range when its FIRST offset lies inside
        //    `[start, end)` — using `start < end` (not `end <= end`) keeps
        //    blocks whose trailing return/throw sits exactly on the boundary
        //    (`try { return f(); } catch ..` — the call is protected, the
        //    areturn lands on `end` and can still reach the loop header
        //    through the handler, which loop membership must see).
        let mut exc_edges = Vec::new();
        for (ri, r) in exc_ranges.iter().enumerate() {
            let handler = blocks.iter().position(|b| b.start == r.handler);
            if let Some(h) = handler {
                blocks[h].handlers.push(ri as u32);
            }
            for b in blocks.iter() {
                if b.start >= r.start && b.start < r.end && b.ins_len != 0 {
                    if let Some(h) = handler {
                        exc_edges.push(ExcEdge { range: ri, from: b.id, to: h });
                    }
                }
            }
        }
        for b in blocks.iter_mut() {
            b.pred.sort_unstable();
            b.pred.dedup();
            b.handlers.sort_unstable();
            b.handlers.dedup();
        }
        let mut starts: Vec<u32> = blocks.iter().map(|b| b.start).collect();
        starts.sort_unstable();
        if crate::dbg_flag!("JCDC_DBG_CORE_CFG") {
            for b in &blocks {
                eprintln!(
                    "CORE_CFG b={} start={} end={} ins_len={} succ={:?} pred={:?} handlers={:?}",
                    b.id, b.start, b.end, b.ins_len, b.succ, b.pred, b.handlers
                );
            }
            for (i, r) in exc_ranges.iter().enumerate() {
                eprintln!("CORE_EXC r={} {}..{} -> {} type={:?}", i, r.start, r.end, r.handler, r.catch_type);
            }
            for e in &exc_edges {
                eprintln!("CORE_EDGE r={} from={} to={}", e.range, e.from, e.to);
            }
        }
        Cfg { blocks, entry, exc_edges, exc_ranges, starts }
    }

    /// Assemble a CFG from blocks the caller has **already filled in**:
    /// `pred` and `handlers` are taken as given and `exc_edges` is supplied,
    /// so no derivation runs.
    ///
    /// This is the constructor for a front-end that is mirroring a graph it
    /// already maintains (jcdc's `Cfg::to_core`): the derivation above would
    /// only be thrown away, and at four to five snapshots per method that
    /// waste — plus the allocate-then-replace churn of the derived lists —
    /// is measurable.
    pub fn from_parts(
        blocks: Vec<Block>,
        entry: usize,
        exc_ranges: Vec<ExcRange>,
        exc_edges: Vec<ExcEdge>,
    ) -> Cfg {
        let mut starts: Vec<u32> = blocks.iter().map(|b| b.start).collect();
        starts.sort_unstable();
        Cfg { blocks, entry, exc_edges, exc_ranges, starts }
    }

    /// Block containing machine offset `p`.
    pub fn block_at(&self, p: u32) -> Option<usize> {
        let i = self.starts.partition_point(|&l| l <= p);
        if i == 0 {
            None
        } else {
            Some(i - 1)
        }
    }

    /// Number of blocks.
    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// True if `p` starts a block.
    pub fn is_block_start(&self, p: u32) -> bool {
        self.starts.binary_search(&p).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blk(id: usize, start: u32, len: u32, succ: Vec<usize>) -> Block {
        Block {
            id,
            start,
            end: start + len,
            ins_len: len,
            succ,
            pred: Vec::new(),
            handlers: Vec::new(),
        }
    }

    #[test]
    fn preds_and_handlers_are_derived() {
        // 0 → 1 → 2, with 2 throwing into 3.
        let cfg = Cfg::from_blocks(
            vec![blk(0, 0, 2, vec![1]), blk(1, 2, 2, vec![2]), blk(2, 4, 2, vec![]), blk(3, 6, 1, vec![])],
            0,
            vec![ExcRange { start: 0, end: 6, handler: 6, catch_type: Some("java/lang/Throwable".into()) }],
        );
        assert_eq!(cfg.blocks[1].pred, vec![0]);
        assert_eq!(cfg.blocks[2].pred, vec![1]);
        assert_eq!(cfg.blocks[3].pred, Vec::<usize>::new());
        // `handlers` holds the exception-RANGE indices this block handles;
        // the protected side is `exc_edges[..].from`.
        assert_eq!(cfg.blocks[3].handlers, vec![0]);
        assert_eq!(cfg.exc_edges.len(), 3);
        assert!(cfg.exc_edges.iter().all(|e| e.range == 0 && e.to == 3));
        assert!(cfg.block_at(3) == Some(1));
        assert!(cfg.block_at(6) == Some(3));
        assert!(cfg.is_block_start(4) && !cfg.is_block_start(5));
    }
}